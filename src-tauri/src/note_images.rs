use crate::{agent::{AgentState, ChatSession}, db, notes::{Note, NotesState}, storyboard, storage::AppState, ProcessStatus};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const MAX_IMAGES: usize = 4;
const MAX_IMAGE_BYTES: usize = 8_000_000;
const MAX_TOTAL_BYTES: usize = 16_000_000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageSelection {
    pub origin: String,
    pub id: String,
    pub caption: String,
    pub position: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameOption { pub image_id: String, pub record_id: String, pub at_seconds: u32, pub origin: String }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedPlacement { pub asset_id: String, pub caption: String, pub position: String }

pub struct ResolvedImage {
    pub selection: ImageSelection,
    pub record_id: String,
    pub at_seconds: u32,
    pub bytes: Vec<u8>,
    pub existing_asset_id: Option<String>,
    pub source_image_id: String,
}

fn image_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|_| "截图文件已丢失")?;
    if bytes.len() < 4 || bytes.len() > MAX_IMAGE_BYTES || !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err("截图格式或大小无效".into());
    }
    Ok(bytes)
}

fn asset_path(data_dir: &Path, note_id: &str, asset_id: &str) -> Result<PathBuf, String> {
    Uuid::parse_str(asset_id).map_err(|_| "笔记图片编号无效")?;
    if note_id.is_empty() || note_id.len() > 100 || !note_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_') {
        return Err("笔记编号无效".into());
    }
    Ok(data_dir.join("note-assets").join(note_id).join(format!("{asset_id}.jpg")))
}

pub fn resolve_images(data_dir: &Path, db_path: &Path, session: Option<&ChatSession>, note: &Note,
    choices: &[ImageSelection]) -> Result<Vec<ResolvedImage>, String> {
    if choices.len() > MAX_IMAGES { return Err("每份高级版最多选择 4 张截图".into()); }
    let connection = db::open(db_path)?;
    let allowed = note.sources.iter().map(|source| source.record_id.as_str()).collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut seen_sources = HashSet::new();
    let mut total = 0;
    let mut result = Vec::new();
    for choice in choices {
        if choice.caption.chars().count() > 160 || choice.caption.chars().any(char::is_control)
            || !matches!(choice.position.as_str(), "before" | "after")
            || !seen.insert((choice.origin.as_str(), choice.id.as_str())) {
            return Err("截图说明、位置或重复选择无效".into());
        }
        let (record_id, at_seconds, source_image_id, existing_asset_id, path) = match choice.origin.as_str() {
            "chat" => {
                Uuid::parse_str(&choice.id).map_err(|_| "聊天截图编号无效")?;
                let frame = session.ok_or("当前会话没有这张截图")?.messages.iter().find_map(|message| message.frame.as_ref()
                    .filter(|frame| frame.image_id == choice.id)).ok_or("当前会话没有这张截图")?;
                let existing: Option<String> = connection.query_row(
                    "SELECT id FROM note_assets WHERE note_id=?1 AND source_image_id=?2",
                    params![&note.id, &choice.id], |row| row.get(0))
                    .optional().map_err(|error| format!("笔记图片读取失败：{error}"))?;
                (frame.record_id.clone(), frame.at_seconds, choice.id.clone(), existing,
                    data_dir.join("chat-frames").join(format!("{}.jpg", choice.id)))
            }
            "capture" => {
                Uuid::parse_str(&choice.id).map_err(|_| "笔记截图编号无效")?;
                let session_id = &session.ok_or("当前会话没有这张待保存截图")?.id;
                let row: Option<(String, u32)> = connection.query_row(
                    "SELECT record_id,at_seconds FROM note_frame_drafts WHERE id=?1 AND note_id=?2 AND session_id=?3",
                    params![&choice.id, &note.id, session_id], |row| Ok((row.get(0)?, row.get(1)?)))
                    .optional().map_err(|error| format!("笔记截图读取失败：{error}"))?;
                let (record_id, at_seconds) = row.ok_or("这张待保存截图不属于当前笔记或会话")?;
                (record_id, at_seconds, choice.id.clone(), None,
                    data_dir.join("note-frame-drafts").join(format!("{}.jpg", choice.id)))
            }
            "asset" => {
                let path = asset_path(data_dir, &note.id, &choice.id)?;
                let row: Option<(String, u32, String, String)> = connection.query_row(
                    "SELECT record_id,at_seconds,source_image_id,relative_path FROM note_assets WHERE id=?1 AND note_id=?2",
                    params![&choice.id, &note.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
                    .optional().map_err(|error| format!("笔记图片读取失败：{error}"))?;
                let (record_id, at_seconds, source_image_id, relative_path) = row.ok_or("这张图片不属于当前笔记")?;
                if relative_path != format!("note-assets/{}/{}.jpg", note.id, choice.id) {
                    return Err("笔记图片路径无效".into());
                }
                (record_id, at_seconds, source_image_id, Some(choice.id.clone()), path)
            }
            _ => return Err("图片来源无效".into()),
        };
        if !allowed.contains(record_id.as_str()) { return Err("截图所属视频不在笔记来源中".into()); }
        if !seen_sources.insert(source_image_id.clone()) { return Err("同一张截图不能重复加入".into()); }
        let bytes = image_bytes(&path)?;
        total += bytes.len();
        if total > MAX_TOTAL_BYTES { return Err("所选截图总大小超过 16 MB".into()); }
        result.push(ResolvedImage { selection: choice.clone(), record_id, at_seconds, bytes, existing_asset_id, source_image_id });
    }
    Ok(result)
}

pub fn persist_images(transaction: &Transaction<'_>, data_dir: &Path, note: &Note,
    images: &[ResolvedImage], created_files: &mut Vec<PathBuf>) -> Result<Vec<SavedPlacement>, String> {
    let mut placements = Vec::new();
    for image in images {
        let asset_id = if let Some(id) = &image.existing_asset_id { id.clone() } else {
            let id = Uuid::new_v4().to_string();
            let path = asset_path(data_dir, &note.id, &id)?;
            fs::create_dir_all(path.parent().ok_or("图片目录无效")?)
                .map_err(|error| format!("笔记图片目录创建失败：{error}"))?;
            fs::write(&path, &image.bytes).map_err(|error| format!("笔记图片保存失败：{error}"))?;
            created_files.push(path);
            let created_at = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "系统时间无效")?.as_millis();
            transaction.execute("INSERT INTO note_assets(id,note_id,source_image_id,record_id,at_seconds,relative_path,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![&id, &note.id, &image.source_image_id, &image.record_id, image.at_seconds,
                    format!("note-assets/{}/{}.jpg", note.id, id), i64::try_from(created_at).map_err(|_| "时间超出范围")?])
                .map_err(|error| format!("笔记图片登记失败：{error}"))?;
            id
        };
        placements.push(SavedPlacement { asset_id, caption: image.selection.caption.clone(), position: image.selection.position.clone() });
    }
    Ok(placements)
}

pub fn placements_to_selections(placements: &[SavedPlacement]) -> Vec<ImageSelection> {
    placements.iter().map(|item| ImageSelection {
        origin: "asset".into(), id: item.asset_id.clone(), caption: item.caption.clone(), position: item.position.clone()
    }).collect()
}

pub fn image_uri(bytes: &[u8]) -> String {
    format!("data:image/jpeg;base64,{}", storyboard::base64(bytes))
}

pub fn cleanup_saved_capture_drafts(data_dir: &Path, db_path: &Path, note: &Note, images: &[ResolvedImage]) {
    if let Ok(connection) = db::open(db_path) {
        for image in images.iter().filter(|image| image.selection.origin == "capture") {
            let id = &image.selection.id;
            if Uuid::parse_str(id).is_err() { continue; }
            if connection.execute("DELETE FROM note_frame_drafts WHERE id=?1 AND note_id=?2", params![id, &note.id]).is_ok() {
                let _ = fs::remove_file(data_dir.join("note-frame-drafts").join(format!("{id}.jpg")));
            }
        }
    }
}

pub fn prune_old_capture_drafts(data_dir: &Path, db_path: &Path) -> Result<(), String> {
    let connection = db::open(db_path)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "系统时间无效")?.as_millis();
    let cutoff = i64::try_from(now.saturating_sub(7 * 24 * 60 * 60 * 1000)).map_err(|_| "时间超出范围")?;
    let mut query = connection.prepare("SELECT id FROM note_frame_drafts WHERE created_at<?1")
        .map_err(|error| format!("旧截图清理准备失败：{error}"))?;
    let ids = query.query_map([cutoff], |row| row.get::<_, String>(0))
        .map_err(|error| format!("旧截图查询失败：{error}"))?
        .collect::<Result<Vec<_>, _>>().map_err(|error| format!("旧截图查询失败：{error}"))?;
    for id in ids {
        if Uuid::parse_str(&id).is_ok() {
            let _ = fs::remove_file(data_dir.join("note-frame-drafts").join(format!("{id}.jpg")));
        }
        connection.execute("DELETE FROM note_frame_drafts WHERE id=?1", [&id])
            .map_err(|error| format!("旧截图记录清理失败：{error}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn get_note_frame_image(app: AppHandle, agent: State<'_, AgentState>, notes: State<'_, NotesState>,
    note_id: String, image_id: String, origin: String) -> Result<String, String> {
    let note = notes.notes.lock().map_err(|_| "笔记暂不可用")?.iter()
        .find(|item| item.id == note_id).cloned().ok_or("笔记不存在")?;
    let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
    let session = store.sessions.iter().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let choice = ImageSelection { origin, id: image_id, caption: String::new(), position: "after".into() };
    let images = resolve_images(&data_dir, &notes.path, Some(session), &note, &[choice])?;
    Ok(image_uri(&images[0].bytes))
}

#[tauri::command]
pub async fn capture_note_frame(app: AppHandle, agent: State<'_, AgentState>, notes: State<'_, NotesState>,
    storage: State<'_, AppState>, note_id: String, record_id: String, at_seconds: u32) -> Result<FrameOption, String> {
    if at_seconds > 86_400 { return Err("截图时间超出允许范围".into()); }
    let note = notes.notes.lock().map_err(|_| "笔记暂不可用")?.iter()
        .find(|item| item.id == note_id).cloned().ok_or("笔记不存在")?;
    if !note.sources.iter().any(|source| source.record_id == record_id) {
        return Err("这条视频不在笔记来源中".into());
    }
    db::ready_library_titles(&notes.path, &[record_id.clone()])?;
    let (url, duration) = {
        let records = storage.records.lock().map_err(|_| "资料库暂不可用")?;
        let record = records.iter().find(|item| item.id == record_id && item.status == ProcessStatus::Ready
            && item.record_type == "video").ok_or("来源不是已处理成功的视频")?;
        (record.source_url.clone(), record.duration_seconds)
    };
    if duration.is_some_and(|duration| at_seconds > duration) { return Err("截图时间超过视频时长".into()); }
    let session_id = {
        let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        store.active_id.clone()
    };
    let connection = db::open(&notes.path)?;
    let count: i64 = connection.query_row("SELECT count(*) FROM note_frame_drafts WHERE note_id=?1 AND session_id=?2",
        params![&note.id, &session_id], |row| row.get(0)).map_err(|error| format!("截图数量检查失败：{error}"))?;
    if count >= 12 { return Err("这篇笔记已有 12 张待保存截图，请先保存高级版".into()); }
    let id = Uuid::new_v4().to_string();
    let original = storyboard::capture_frame(&app, &url, at_seconds, &id).await?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let folder = data_dir.join("note-frame-drafts");
    fs::create_dir_all(&folder).map_err(|error| format!("笔记截图目录创建失败：{error}"))?;
    let target = folder.join(format!("{id}.jpg"));
    fs::rename(&original, &target).map_err(|error| format!("笔记截图暂存失败：{error}"))?;
    if let Err(error) = image_bytes(&target) { let _ = fs::remove_file(&target); return Err(error); }
    let created_at = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "系统时间无效")?.as_millis();
    if let Err(error) = connection.execute("INSERT INTO note_frame_drafts(id,note_id,session_id,record_id,at_seconds,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![&id, &note.id, &session_id, &record_id, at_seconds, i64::try_from(created_at).map_err(|_| "时间超出范围")?]) {
        let _ = fs::remove_file(&target);
        return Err(format!("笔记截图登记失败：{error}"));
    }
    Ok(FrameOption { image_id: id, record_id, at_seconds, origin: "capture".into() })
}

pub fn export_markdown_images(data_dir: &Path, db_path: &Path, note: &Note) -> Result<String, String> {
    let connection = db::open(db_path)?;
    let row: Option<(String, String)> = connection.query_row(
        "SELECT note_snapshot_json,images_json FROM note_renditions WHERE note_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",
        [&note.id], |row| Ok((row.get(0)?, row.get(1)?)))
        .optional().map_err(|error| format!("笔记图片读取失败：{error}"))?;
    let Some((snapshot, json)) = row else { return Ok(String::new()); };
    if snapshot != serde_json::to_string(note).map_err(|_| "笔记编码失败")? { return Ok(String::new()); }
    let placements: Vec<SavedPlacement> = serde_json::from_str(&json).map_err(|_| "笔记图片记录损坏")?;
    if placements.is_empty() { return Ok(String::new()); }
    let images = resolve_images(data_dir, db_path, None, note, &placements_to_selections(&placements))?;
    let folder = format!("{}-assets", note.id);
    let export_dir = data_dir.join("exports").join(&folder);
    fs::create_dir_all(&export_dir).map_err(|error| format!("笔记图片导出目录创建失败：{error}"))?;
    let mut result = String::from("\n## 画面\n\n");
    for image in images {
        let id = image.existing_asset_id.as_deref().ok_or("笔记图片编号无效")?;
        let filename = format!("{id}.jpg");
        fs::write(export_dir.join(&filename), &image.bytes).map_err(|error| format!("笔记图片导出失败：{error}"))?;
        let caption = image.selection.caption.trim().replace(['[', ']', '\n', '\r'], " ");
        let caption = if caption.is_empty() { "视频截图" } else { &caption };
        result.push_str(&format!("![{caption}]({folder}/{filename})\n\n"));
    }
    Ok(result)
}

#[tauri::command]
pub fn list_note_frames(app: AppHandle, agent: State<'_, AgentState>, notes: State<'_, NotesState>, note_id: String) -> Result<Vec<FrameOption>, String> {
    let note = notes.notes.lock().map_err(|_| "笔记暂不可用")?.iter()
        .find(|item| item.id == note_id).cloned().ok_or("笔记不存在")?;
    let allowed = note.sources.iter().map(|source| source.record_id.as_str()).collect::<HashSet<_>>();
    let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
    let session = store.sessions.iter().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let mut seen = HashSet::new();
    let mut frames = session.messages.iter().rev().filter_map(|message| message.frame.as_ref())
        .filter(|frame| allowed.contains(frame.record_id.as_str()) && seen.insert(frame.image_id.as_str())
            && data_dir.join("chat-frames").join(format!("{}.jpg", frame.image_id)).is_file())
        .take(12).map(|frame| FrameOption { image_id: frame.image_id.clone(), record_id: frame.record_id.clone(), at_seconds: frame.at_seconds, origin: "chat".into() }).collect::<Vec<_>>();
    let connection = db::open(&notes.path)?;
    let mut query = connection.prepare("SELECT id,record_id,at_seconds FROM note_frame_drafts WHERE note_id=?1 AND session_id=?2 ORDER BY created_at DESC LIMIT 12")
        .map_err(|error| format!("笔记截图查询失败：{error}"))?;
    let captures = query.query_map(params![&note.id, &session.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, u32>(2)?)))
        .map_err(|error| format!("笔记截图查询失败：{error}"))?;
    for capture in captures {
        let (id, record_id, at_seconds) = capture.map_err(|error| format!("笔记截图读取失败：{error}"))?;
        if allowed.contains(record_id.as_str()) && data_dir.join("note-frame-drafts").join(format!("{id}.jpg")).is_file() {
            frames.push(FrameOption { image_id: id, record_id, at_seconds, origin: "capture".into() });
        }
    }
    let mut assets = connection.prepare("SELECT id,record_id,at_seconds FROM note_assets WHERE note_id=?1 ORDER BY created_at DESC LIMIT 12")
        .map_err(|error| format!("笔记附件查询失败：{error}"))?;
    let saved = assets.query_map([&note.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, u32>(2)?)))
        .map_err(|error| format!("笔记附件查询失败：{error}"))?;
    for asset in saved {
        let (id, record_id, at_seconds) = asset.map_err(|error| format!("笔记附件读取失败：{error}"))?;
        if allowed.contains(record_id.as_str()) && asset_path(&data_dir, &note.id, &id).is_ok_and(|path| path.is_file()) {
            frames.push(FrameOption { image_id: id, record_id, at_seconds, origin: "asset".into() });
        }
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::NoteSource;
    use serde_json::json;

    #[test]
    fn temporary_capture_is_bound_to_note_and_session() {
        let dir = std::env::temp_dir().join(format!("gearnote-note-capture-{}", Uuid::new_v4()));
        fs::create_dir_all(dir.join("note-frame-drafts")).unwrap();
        let path = dir.join("test.db");
        let note = Note { id: "note-1".into(), title: "测试".into(), body: "正文".into(),
            sources: vec![NoteSource { record_id: "video-1".into(), title: "视频".into(), url: "https://example.com".into(), at_seconds: None }],
            folder_id: None, created_at: 1, updated_at: 2 };
        db::write_notes(&path, &[note.clone()]).unwrap();
        let image_id = Uuid::new_v4().to_string();
        fs::write(dir.join("note-frame-drafts").join(format!("{image_id}.jpg")), [0xff, 0xd8, 0xff, 0xd9]).unwrap();
        let connection = db::open(&path).unwrap();
        connection.execute("INSERT INTO note_frame_drafts(id,note_id,session_id,record_id,at_seconds,created_at) VALUES (?1,'note-1','session-1','video-1',85,1)", [&image_id]).unwrap();
        let session: ChatSession = serde_json::from_value(json!({"id":"session-1","title":"测试","createdAt":"1","updatedAt":"1","messages":[]})).unwrap();
        let other_session: ChatSession = serde_json::from_value(json!({"id":"session-2","title":"测试","createdAt":"1","updatedAt":"1","messages":[]})).unwrap();
        let selection = ImageSelection { origin: "capture".into(), id: image_id, caption: "画面".into(), position: "after".into() };
        assert_eq!(resolve_images(&dir, &path, Some(&session), &note, &[selection.clone()]).unwrap()[0].at_seconds, 85);
        assert!(resolve_images(&dir, &path, Some(&other_session), &note, &[selection.clone()]).is_err());
        let mut other_note = note.clone(); other_note.id = "note-2".into();
        assert!(resolve_images(&dir, &path, Some(&session), &other_note, &[selection]).is_err());
        drop(connection);
        fs::remove_dir_all(dir).unwrap();
    }
}
