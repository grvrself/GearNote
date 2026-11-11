use crate::storage::AppState;
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSource {
    pub record_id: String,
    pub title: String,
    pub url: String,
    pub at_seconds: Option<u32>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub sources: Vec<NoteSource>,
    pub created_at: u128,
    pub updated_at: u128,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSourceInput { pub record_id: String, pub at_seconds: Option<u32> }

pub struct NotesState { path: PathBuf, notes: Mutex<Vec<Note>> }

impl NotesState {
    pub fn load(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = app.path().app_data_dir()?;
        fs::create_dir_all(&dir)?;
        let legacy_path = dir.join("notes.json");
        let backup = legacy_path.with_extension("json.bak");
        if !legacy_path.exists() && backup.exists() { fs::rename(&backup, &legacy_path)?; }
        let path = dir.join(crate::db::FILE_NAME);
        let notes = if let Some(saved) = crate::db::read_notes(&path).map_err(std::io::Error::other)? {
            saved
        } else {
            let legacy: Vec<Note> = if legacy_path.exists() { serde_json::from_slice(&fs::read(&legacy_path)?)? } else { Vec::new() };
            crate::db::write_notes(&path, &legacy).map_err(std::io::Error::other)?;
            legacy
        };
        Ok(Self { path, notes: Mutex::new(notes) })
    }
}

fn now() -> Result<u128, String> {
    Ok(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "系统时间无效")?.as_millis())
}

fn persist(path: &PathBuf, notes: &[Note]) -> Result<(), String> {
    if path.extension().is_some_and(|value| value == "db") { return crate::db::write_notes(path, notes); }
    let temp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(notes).map_err(|_| "笔记无法编码")?;
    let mut file = fs::File::create(&temp).map_err(|error| format!("笔记保存失败：{error}"))?;
    file.write_all(&data).map_err(|error| format!("笔记保存失败：{error}"))?;
    file.sync_all().map_err(|error| format!("笔记保存失败：{error}"))?;
    drop(file);
    if path.exists() {
        let backup = path.with_extension("json.bak");
        if backup.exists() { fs::remove_file(&backup).map_err(|error| format!("笔记保存失败：{error}"))?; }
        fs::rename(path, &backup).map_err(|error| format!("笔记保存失败：{error}"))?;
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::rename(&backup, path);
            return Err(format!("笔记保存失败：{error}"));
        }
        let _ = fs::remove_file(backup);
    } else {
        fs::rename(&temp, path).map_err(|error| format!("笔记保存失败：{error}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn list_notes(state: State<'_, NotesState>) -> Result<Vec<Note>, String> {
    let mut notes = state.notes.lock().map_err(|_| "笔记暂不可用")?.clone();
    notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(notes)
}

#[tauri::command]
pub fn save_note(app: AppHandle, state: State<'_, NotesState>, records: State<'_, AppState>, id: Option<String>, title: String, body: String, sources: Vec<NoteSourceInput>) -> Result<Note, String> {
    let title = title.trim();
    let body = body.trim();
    if title.is_empty() || title.chars().count() > 120 || title.chars().any(char::is_control) { return Err("笔记标题应为 1–120 字".into()); }
    if body.is_empty() || body.chars().count() > 40_000 { return Err("笔记正文应为 1–40000 字".into()); }
    if sources.len() > 20 { return Err("每篇笔记最多关联 20 个来源".into()); }
    let resolved = {
        let records = records.records.lock().map_err(|_| "资料库暂不可用")?;
        let mut result = Vec::new();
        for source in sources {
            let record = records.iter().find(|item| item.id == source.record_id).ok_or("来源资料不存在")?;
            if source.at_seconds.is_some() && record.record_type != "video" { return Err("文章来源不能包含视频时间点".into()); }
            if result.iter().any(|item: &NoteSource| item.record_id == source.record_id && item.at_seconds == source.at_seconds) { continue; }
            result.push(NoteSource { record_id: record.id.clone(), title: record.title.clone(), url: record.source_url.clone(), at_seconds: source.at_seconds });
        }
        result
    };
    let timestamp = now()?;
    let mut guard = state.notes.lock().map_err(|_| "笔记暂不可用")?;
    let mut next = guard.clone();
    let note = if let Some(id) = id {
        let item = next.iter_mut().find(|item| item.id == id).ok_or("笔记不存在")?;
        item.title = title.into();
        item.body = body.into();
        item.sources = resolved;
        item.updated_at = timestamp;
        item.clone()
    } else {
        let item = Note { id: format!("note-{}", Uuid::new_v4()), title: title.into(), body: body.into(), sources: resolved, created_at: timestamp, updated_at: timestamp };
        next.push(item.clone());
        item
    };
    persist(&state.path, &next)?;
    *guard = next;
    drop(guard);
    let _ = crate::agent::record_completed_event(&app, &format!("笔记《{}》已保存", note.title), &format!("note-{}-{}", note.id, timestamp));
    Ok(note)
}

#[tauri::command]
pub fn export_note(app: AppHandle, state: State<'_, NotesState>, id: String) -> Result<String, String> {
    let note = state.notes.lock().map_err(|_| "笔记暂不可用")?.iter().find(|note| note.id == id).cloned().ok_or("笔记不存在")?;
    let dir = app.path().app_data_dir().map_err(|error| error.to_string())?.join("exports");
    fs::create_dir_all(&dir).map_err(|error| format!("导出目录创建失败：{error}"))?;
    let path = dir.join(format!("{}.md", note.id));
    let mut markdown = format!("# {}\n\n{}\n", note.title, note.body);
    if !note.sources.is_empty() {
        markdown.push_str("\n## 来源\n\n");
        for source in &note.sources {
            let label = source.title.replace([']', '\n', '\r'], " ");
            let url = source.url.replace([')', '\n', '\r'], "");
            let at = source.at_seconds.map(|value| format!(" · {:02}:{:02}", value / 60, value % 60)).unwrap_or_default();
            markdown.push_str(&format!("- [{label}]({url}){at}\n"));
        }
    }
    fs::write(&path, markdown).map_err(|error| format!("导出失败：{error}"))?;
    Ok(path.to_string_lossy().into_owned())
}
