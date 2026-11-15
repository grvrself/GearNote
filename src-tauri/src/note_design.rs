use crate::{agent::AgentState, db, note_images::{self, ImageSelection, ResolvedImage, SavedPlacement}, notes::{Note, NotesState}};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::{fs, path::{Path, PathBuf}, process::{Command, Stdio}, thread, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const CSS: &str = include_str!("note-design.css");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDesign {
    id: String,
    note_id: String,
    theme: String,
    created_at: u128,
    stale: bool,
    html: String,
    images: Vec<ImageSelection>,
}

fn valid_theme(value: &str) -> Result<&str, String> {
    match value { "mono" | "paper" => Ok(value), _ => Err("未知的笔记主题".into()) }
}

fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&#39;")
}

fn inline(value: &str) -> String {
    let safe = escape(value);
    let parts = safe.split("**").collect::<Vec<_>>();
    if parts.len() % 2 == 0 { return safe; }
    parts.into_iter().enumerate().map(|(index, part)| {
        if index % 2 == 1 { format!("<strong>{}</strong>", citation_badges(part)) } else { citation_badges(part) }
    }).collect::<String>()
}

fn display_char_count(value: &str) -> usize {
    value.chars().filter(|ch| !ch.is_whitespace() && !"，。！？、；：,.!?;:".contains(*ch)).count()
}

fn citation_badges(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        if let Some(end) = tail.find(']') {
            let number = &tail[..end];
            if !number.is_empty() && number.len() <= 3 && number.bytes().all(|byte| byte.is_ascii_digit()) {
                out.push_str(&format!("<sup class=\"ref-badge\">[{number}]</sup>"));
                rest = &tail[end + 1..];
                continue;
            }
        }
        out.push('[');
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn flush_paragraph(out: &mut String, lines: &mut Vec<String>) {
    if !lines.is_empty() {
        out.push_str("<p>");
        out.push_str(&lines.iter().map(|line| inline(line)).collect::<Vec<_>>().join("<br>"));
        out.push_str("</p>");
        lines.clear();
    }
}

fn is_fence(line: &str) -> bool {
    line.starts_with("~~~") || line.as_bytes().starts_with(&[96, 96, 96])
}

fn render_body(body: &str) -> String {
    let mut out = String::new();
    let mut paragraph = Vec::new();
    let mut list_open = None::<&str>;
    let mut code = None::<String>;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(current) = code.as_mut() {
            if is_fence(trimmed) {
                out.push_str("<pre><code>"); out.push_str(&escape(current.trim_end_matches('\n')));
                out.push_str("</code></pre>");
                code = None;
            } else { current.push_str(line); current.push('\n'); }
            continue;
        }
        if is_fence(trimmed) {
            flush_paragraph(&mut out, &mut paragraph);
            if let Some(kind) = list_open.take() { out.push_str(&format!("</{kind}>")); }
            code = Some(String::new());
            continue;
        }
        if trimmed.is_empty() {
            flush_paragraph(&mut out, &mut paragraph);
            if let Some(kind) = list_open.take() { out.push_str(&format!("</{kind}>")); }
            continue;
        }
        let heading = trimmed.strip_prefix("### ").map(|text| (3, text))
            .or_else(|| trimmed.strip_prefix("## ").map(|text| (2, text)))
            .or_else(|| trimmed.strip_prefix("# ").map(|text| (1, text)));
        let bullet = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* "));
        let numbered = trimmed.split_once(". ").filter(|(number, _)| !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())).map(|(_, text)| text);
        if heading.is_some() || bullet.is_some() || numbered.is_some() || trimmed.starts_with("> ") || trimmed == "---" {
            flush_paragraph(&mut out, &mut paragraph);
            let next_kind = if bullet.is_some() { Some("ul") } else if numbered.is_some() { Some("ol") } else { None };
            if list_open != next_kind {
                if let Some(kind) = list_open.take() { out.push_str(&format!("</{kind}>")); }
                if let Some(kind) = next_kind { out.push_str(&format!("<{kind}>")); list_open = Some(kind); }
            }
            if let Some((level, text)) = heading {
                out.push_str(&format!("<h{level}>{}</h{level}>", inline(text)));
            } else if let Some(text) = bullet {
                out.push_str("<li>"); out.push_str(&inline(text)); out.push_str("</li>");
            } else if let Some(text) = numbered {
                out.push_str("<li>"); out.push_str(&inline(text)); out.push_str("</li>");
            } else if let Some(text) = trimmed.strip_prefix("> ") {
                out.push_str("<blockquote><p>"); out.push_str(&inline(text)); out.push_str("</p></blockquote>");
            } else { out.push_str("<hr>"); }
        } else {
            if let Some(kind) = list_open.take() { out.push_str(&format!("</{kind}>")); }
            paragraph.push(trimmed.to_string());
        }
    }
    flush_paragraph(&mut out, &mut paragraph);
    if let Some(kind) = list_open { out.push_str(&format!("</{kind}>")); }
    if let Some(current) = code {
        out.push_str("<pre><code>"); out.push_str(&escape(current.trim_end_matches('\n')));
        out.push_str("</code></pre>");
    }
    out
}

fn source_url(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| escape(value))
}

fn render_figures(images: &[ResolvedImage], position: &str) -> String {
    let mut html = String::new();
    for image in images.iter().filter(|image| image.selection.position == position) {
        let caption = if image.selection.caption.trim().is_empty() { "视频截图" } else { image.selection.caption.trim() };
        let safe_caption = escape(caption);
        let uri = note_images::image_uri(&image.bytes);
        let time = format!("{:02}:{:02}", image.at_seconds / 60, image.at_seconds % 60);
        html.push_str(&format!("<figure class=\"note-figure\"><img src=\"{uri}\" alt=\"{safe_caption}\"><figcaption><span>{safe_caption}</span><span>VIDEO · {time}</span></figcaption></figure>"));
    }
    html
}

fn render_with_images(note: &Note, selected_theme: &str, images: &[ResolvedImage]) -> Result<String, String> {
    let selected_theme = valid_theme(selected_theme)?;
    let mut sources = String::new();
    if !note.sources.is_empty() {
        sources.push_str("<section class=\"sources\" id=\"note-sources\"><h2>引用来源</h2><ol>");
        for (index, source) in note.sources.iter().enumerate() {
            let label = escape(&source.title);
            let time = source.at_seconds.map(|seconds| format!("{:02}:{:02}", seconds / 60, seconds % 60)).unwrap_or_else(|| "—".into());
            sources.push_str(&format!("<li><span class=\"source-time\">{time}</span><span class=\"source-title\">"));
            if let Some(href) = source_url(&source.url) {
                sources.push_str(&format!("<a href=\"{href}\" rel=\"noopener noreferrer\">{label}</a>"));
            } else {
                sources.push_str(&label);
            }
            sources.push_str(&format!("</span><span class=\"source-number\">{:02}</span></li>", index + 1));
        }
        sources.push_str("</ol></section>");
    }
    let title = escape(&note.title);
    let body = render_body(&note.body);
    let before_images = render_figures(images, "before");
    let after_images = render_figures(images, "after");
    Ok(format!(r#"<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:"><title>{title} · GearNote</title><style>{CSS}</style></head><body data-theme="{selected_theme}"><main class="sheet"><div class="mast"><span>GEARNOTE / READING EDITION</span><span>知识笔记</span></div><div class="kicker">LIBRARY / NOTE</div><h1 id="note-title">{title}</h1><div class="meta"><span>本地笔记</span><span>来源 {count} 条</span><span>{word_count} 字</span><span>预计 {reading_minutes} 分钟</span><span>画面 {image_count} 张</span></div><div class="notch-rule" aria-hidden="true"></div><article id="note-body">{before_images}{body}{after_images}</article>{sources}<div class="foot"><span>GEARNOTE · LOCAL KNOWLEDGE WORKSPACE</span><span>来源可回到原资料核对</span></div></main></body></html>"#,
        image_count = images.len(),
        count = note.sources.len(),
        word_count = display_char_count(&note.body),
        reading_minutes = display_char_count(&note.body).div_ceil(220).max(1)))
}

#[cfg(test)]
fn render(note: &Note, selected_theme: &str) -> Result<String, String> {
    render_with_images(note, selected_theme, &[])
}

fn current_note(state: &NotesState, note_id: &str) -> Result<Note, String> {
    state.notes.lock().map_err(|_| "笔记暂不可用")?.iter()
        .find(|note| note.id == note_id).cloned().ok_or("笔记不存在".into())
}

fn active_session(agent: &AgentState) -> Result<crate::agent::ChatSession, String> {
    let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
    store.sessions.iter().find(|session| session.id == store.active_id)
        .cloned().ok_or("当前会话不存在".into())
}

#[tauri::command]
pub fn preview_note_design(app: AppHandle, state: State<'_, NotesState>, agent: State<'_, AgentState>,
    note_id: String, theme: String, images: Option<Vec<ImageSelection>>) -> Result<String, String> {
    let note = current_note(&state, &note_id)?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let session = active_session(&agent)?;
    let resolved = note_images::resolve_images(&data_dir, &state.path, Some(&session), &note, &images.unwrap_or_default())?;
    render_with_images(&note, &theme, &resolved)
}

#[tauri::command]
pub fn save_note_design(app: AppHandle, state: State<'_, NotesState>, agent: State<'_, AgentState>,
    note_id: String, theme: String, expected_updated_at: u64, images: Option<Vec<ImageSelection>>) -> Result<NoteDesign, String> {
    let note = current_note(&state, &note_id)?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let session = active_session(&agent)?;
    let resolved = note_images::resolve_images(&data_dir, &state.path, Some(&session), &note, &images.unwrap_or_default())?;
    save_design(&state.path, &data_dir, &note, &theme, u128::from(expected_updated_at), &resolved)
}

fn save_design(path: &Path, data_dir: &Path, note: &Note, selected_theme: &str,
    expected_updated_at: u128, images: &[ResolvedImage]) -> Result<NoteDesign, String> {
    let selected_theme = valid_theme(selected_theme)?;
    if note.updated_at != expected_updated_at { return Err("笔记已修改，请重新预览后保存高级版".into()); }
    let snapshot = serde_json::to_string(&note).map_err(|error| format!("笔记编码失败：{error}"))?;
    let id = Uuid::new_v4().to_string();
    let created_at = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "系统时间无效")?.as_millis();
    let mut connection = db::open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("高级版事务启动失败：{error}"))?;
    let stored: String = transaction.query_row("SELECT json FROM note_rows WHERE id=?1", [&note.id], |row| row.get(0))
        .map_err(|_| "笔记已改变，请重新打开后重试")?;
    if stored != snapshot { return Err("笔记已改变，请重新预览后保存高级版".into()); }
    let mut created_files = Vec::<PathBuf>::new();
    let result = (|| {
        let html = render_with_images(note, selected_theme, images)?;
        let placements = note_images::persist_images(&transaction, data_dir, note, images, &mut created_files)?;
        let images_json = serde_json::to_string(&placements).map_err(|error| format!("笔记图片编码失败：{error}"))?;
        transaction.execute("INSERT INTO note_renditions(id,note_id,theme,note_snapshot_json,images_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![&id, &note.id, selected_theme, &snapshot, &images_json, i64::try_from(created_at).map_err(|_| "时间超出范围")?])
            .map_err(|error| format!("高级版保存失败：{error}"))?;
        transaction.commit().map_err(|error| format!("高级版提交失败：{error}"))?;
        Ok(NoteDesign { id, note_id: note.id.clone(), theme: selected_theme.into(), created_at, stale: false,
            html,
            images: note_images::placements_to_selections(&placements) })
    })();
    if result.is_err() {
        for file in created_files { let _ = fs::remove_file(file); }
    } else {
        note_images::cleanup_saved_capture_drafts(data_dir, path, note, images);
    }
    result
}

#[tauri::command]
pub fn get_note_design(app: AppHandle, state: State<'_, NotesState>, note_id: String) -> Result<Option<NoteDesign>, String> {
    let note = current_note(&state, &note_id)?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    load_design(&state.path, &data_dir, &note)
}

#[tauri::command]
pub fn delete_note_design(state: State<'_, NotesState>, note_id: String) -> Result<usize, String> {
    let note = current_note(&state, &note_id)?;
    delete_design(&state.path, &note)
}

fn delete_design(path: &Path, note: &Note) -> Result<usize, String> {
    let connection = db::open(path)?;
    connection.execute("DELETE FROM note_renditions WHERE note_id=?1", [&note.id])
        .map_err(|error| format!("高级版删除失败：{error}"))
}

fn load_design(path: &Path, data_dir: &Path, note: &Note) -> Result<Option<NoteDesign>, String> {
    let connection = db::open(path)?;
    let row: Option<(String, String, String, String, i64)> = connection.query_row(
        "SELECT id,theme,note_snapshot_json,images_json,created_at FROM note_renditions WHERE note_id=?1 ORDER BY created_at DESC,rowid DESC LIMIT 1",
        [&note.id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)))
        .optional().map_err(|error| format!("高级版读取失败：{error}"))?;
    row.map(|(id, theme, snapshot, images_json, created_at)| {
        let saved: Note = serde_json::from_str(&snapshot).map_err(|error| format!("高级版内容损坏：{error}"))?;
        let placements: Vec<SavedPlacement> = serde_json::from_str(&images_json).map_err(|error| format!("高级版图片记录损坏：{error}"))?;
        let selections = note_images::placements_to_selections(&placements);
        let images = note_images::resolve_images(data_dir, path, None, &saved, &selections)?;
        let current = serde_json::to_string(&note).map_err(|error| format!("笔记编码失败：{error}"))?;
        Ok(NoteDesign { id, note_id: note.id.clone(), theme: theme.clone(), created_at: u128::try_from(created_at).map_err(|_| "高级版时间无效")?,
            stale: current != snapshot, html: render_with_images(&saved, &theme, &images)?, images: selections })
    }).transpose()
}

#[tauri::command]
pub fn export_note_design(app: AppHandle, state: State<'_, NotesState>, note_id: String, format: String) -> Result<String, String> {
    let note = current_note(&state, &note_id)?;
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    export_design(&state.path, &data_dir, &note, &format)
}

fn export_design(db_path: &Path, data_dir: &Path, note: &Note, format: &str) -> Result<String, String> {
    if !matches!(format, "html" | "pdf") { return Err("高级版只支持 HTML 或 PDF 导出".into()); }
    if note.id.is_empty() || note.id.len() > 100 || !note.id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_') {
        return Err("笔记编号无效，不能导出".into());
    }
    let design = load_design(db_path, data_dir, note)?.ok_or("这篇笔记尚未保存高级版")?;
    if design.stale { return Err("高级版基于旧正文，请先刷新并保存再导出".into()); }
    Uuid::parse_str(&design.id).map_err(|_| "高级版编号无效，不能导出")?;
    let folder = data_dir.join("exports");
    fs::create_dir_all(&folder).map_err(|error| format!("导出目录创建失败：{error}"))?;
    let stem = format!("{}-advanced-{}", note.id, design.id);
    let html_path = folder.join(format!("{stem}.html"));
    fs::write(&html_path, design.html.as_bytes()).map_err(|error| format!("HTML 导出失败：{error}"))?;
    if format == "html" { return Ok(html_path.to_string_lossy().into_owned()); }
    let pdf_path = folder.join(format!("{stem}.pdf"));
    if pdf_path.exists() { fs::remove_file(&pdf_path).map_err(|error| format!("旧 PDF 清理失败：{error}"))?; }
    print_pdf(&html_path, &pdf_path, &folder)?;
    Ok(pdf_path.to_string_lossy().into_owned())
}

#[cfg(windows)]
fn edge_executable() -> Option<PathBuf> {
    ["PROGRAMFILES(X86)", "PROGRAMFILES", "LOCALAPPDATA"].iter().filter_map(|name| std::env::var_os(name))
        .map(PathBuf::from).map(|root| root.join("Microsoft").join("Edge").join("Application").join("msedge.exe"))
        .find(|path| path.is_file())
}

#[cfg(windows)]
fn print_pdf(html: &Path, pdf: &Path, export_dir: &Path) -> Result<(), String> {
    let edge = edge_executable().ok_or("未找到 Microsoft Edge，无法生成 PDF；可以先导出 HTML")?;
    let profile = export_dir.join(format!(".pdf-profile-{}", Uuid::new_v4()));
    fs::create_dir_all(&profile).map_err(|error| format!("PDF 临时目录创建失败：{error}"))?;
    let url = url::Url::from_file_path(html).map_err(|_| "HTML 导出路径无效")?;
    let mut child = Command::new(edge)
        .arg("--headless").arg("--disable-gpu").arg("--no-first-run").arg("--disable-background-networking")
        .arg("--no-pdf-header-footer")
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--print-to-pdf={}", pdf.display()))
        .arg(url.as_str())
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().map_err(|error| format!("PDF 生成程序启动失败：{error}"))?;
    let start = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break if status.success() { Ok(()) } else { Err(format!("PDF 生成程序退出码：{status}")) },
            Ok(None) if start.elapsed() < Duration::from_secs(45) => thread::sleep(Duration::from_millis(150)),
            Ok(None) => { let _ = child.kill(); let _ = child.wait(); break Err("PDF 生成超时，请尝试导出 HTML".into()); }
            Err(error) => break Err(format!("PDF 生成状态检查失败：{error}")),
        }
    };
    let _ = fs::remove_dir_all(&profile);
    result?;
    let bytes = fs::read(pdf).map_err(|_| "PDF 未生成，请尝试导出 HTML")?;
    if bytes.len() < 100 || !bytes.starts_with(b"%PDF-") { return Err("PDF 文件无效，请尝试导出 HTML".into()); }
    Ok(())
}

#[cfg(not(windows))]
fn print_pdf(_html: &Path, _pdf: &Path, _export_dir: &Path) -> Result<(), String> {
    Err("当前系统暂不支持 PDF 导出，请使用 HTML".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::NoteSource;
    use serde_json::json;

    fn note() -> Note {
        Note { id: "note-1".into(), title: "标题<script>".into(), body: "## 第一节\n\n这是 **重点**。\n\n- 一\n- 二".into(),
            sources: vec![NoteSource { record_id: "r".into(), title: "来源".into(), url: "https://example.com".into(), at_seconds: None }],
            folder_id: None, created_at: 1, updated_at: 2 }
    }

    #[test]
    fn render_escapes_untrusted_content() {
        let mut value = note();
        value.body = "<script>alert(1)</script>\n\n[点击](javascript:alert(1))\n\n<img src=x onerror=alert(1)>".into();
        value.sources[0].url = "javascript:alert(1)".into();
        let html = render(&value, "mono").unwrap();
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img"));
        assert!(!html.contains("href=\"javascript:"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn render_has_headings_lists_and_bold() {
        let html = render(&note(), "paper").unwrap();
        assert!(html.contains("<h2>第一节</h2>"));
        assert!(html.contains("<strong>重点</strong>"));
        assert!(html.contains("<li>一</li><li>二</li>"));
        assert!(render(&note(), "unknown").is_err());
    }

    #[test]
    fn reader_has_real_anchors_and_computed_metadata_without_app_controls() {
        let mut value = note();
        value.sources[0].at_seconds = Some(25);
        let html = render(&value, "mono").unwrap();
        assert!(html.contains("id=\"note-title\""));
        assert!(html.contains("id=\"note-body\""));
        assert!(html.contains("id=\"note-sources\""));
        assert!(html.contains("00:25"));
        assert!(html.contains("预计 1 分钟"));
        assert!(!html.contains("NOTE 02"));
        assert!(!html.contains("ESC 退出"));
        assert!(!html.contains("自动保存"));
    }

    #[test]
    fn mono_reader_uses_saved_source_time_and_keeps_ordered_steps() {
        let mut value = note();
        value.body = "开头摘要。[2]\n\n1. 检查入口文件\n2. 记录配置\n\n> 原句".into();
        value.sources[0].at_seconds = Some(85);
        let html = render(&value, "mono").unwrap();
        assert!(html.contains("<div class=\"notch-rule\""));
        assert!(html.contains("<ol><li>检查入口文件</li><li>记录配置</li></ol>"));
        assert!(html.contains("<span class=\"source-time\">01:25</span>"));
        assert!(html.contains("<blockquote><p>原句</p></blockquote>"));
        assert!(html.contains("<sup class=\"ref-badge\">[2]</sup>"));
        assert!(!html.contains("次引用"));
    }

    #[test]
    fn saved_design_preserves_note_and_marks_old_snapshot_stale() {
        let dir = std::env::temp_dir().join(format!("gearnote-design-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let original = note();
        db::write_notes(&path, &[original.clone()]).unwrap();
        let saved = save_design(&path, &dir, &original, "mono", original.updated_at, &[]).unwrap();
        assert!(!saved.stale);
        assert_eq!(db::read_notes(&path).unwrap().unwrap()[0].body, original.body);
        assert!(!load_design(&path, &dir, &original).unwrap().unwrap().stale);

        let mut edited = original.clone();
        edited.body.push_str("\n\n人工补充。");
        edited.updated_at += 1;
        db::upsert_note(&path, &edited, 0).unwrap();
        let previous = load_design(&path, &dir, &edited).unwrap().unwrap();
        assert!(previous.stale);
        assert!(!previous.html.contains("人工补充"));
        assert!(save_design(&path, &dir, &edited, "mono", original.updated_at, &[]).is_err());
        drop(previous);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn selected_chat_frame_becomes_durable_note_asset_only_after_save() {
        let dir = std::env::temp_dir().join(format!("gearnote-design-image-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("chat-frames")).unwrap();
        let path = dir.join("test.db");
        let original = note();
        db::write_notes(&path, &[original.clone()]).unwrap();
        let image_id = Uuid::new_v4().to_string();
        let chat_file = dir.join("chat-frames").join(format!("{image_id}.jpg"));
        std::fs::write(&chat_file, [0xff, 0xd8, 0xff, 0xd9]).unwrap();
        let session: crate::agent::ChatSession = serde_json::from_value(json!({
            "id":"chat-1","title":"测试","createdAt":"1","updatedAt":"1",
            "messages":[{"id":"m1","role":"user","createdAt":"1","paragraphs":["截图"],
                "frame":{"imageId":image_id,"recordId":"r","atSeconds":85}}]
        })).unwrap();
        let choices = vec![ImageSelection { origin: "chat".into(), id: image_id, caption: "关键画面".into(), position: "after".into() }];
        let resolved = note_images::resolve_images(&dir, &path, Some(&session), &original, &choices).unwrap();
        assert!(note_images::resolve_images(&dir, &path, None, &original, &choices).is_err());
        assert!(!dir.join("note-assets").exists());
        let saved = save_design(&path, &dir, &original, "mono", original.updated_at, &resolved).unwrap();
        assert!(saved.html.contains("关键画面"));
        assert!(saved.html.contains("VIDEO · 01:25"));
        std::fs::remove_file(chat_file).unwrap();
        let loaded = load_design(&path, &dir, &original).unwrap().unwrap();
        assert!(loaded.html.contains("data:image/jpeg;base64,"));
        assert_eq!(loaded.images.len(), 1);
        assert_eq!(loaded.images[0].origin, "asset");
        assert_eq!(db::read_notes(&path).unwrap().unwrap()[0].body, original.body);
        let markdown = note_images::export_markdown_images(&dir, &path, &original).unwrap();
        assert!(markdown.contains("关键画面"));
        assert!(markdown.contains("-assets/"));
        let mut changed = original.clone(); changed.body.push_str(" 更新");
        assert!(note_images::export_markdown_images(&dir, &path, &changed).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saved_direct_capture_removes_its_temporary_copy() {
        let dir = std::env::temp_dir().join(format!("gearnote-design-capture-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("note-frame-drafts")).unwrap();
        let path = dir.join("test.db");
        let original = note();
        db::write_notes(&path, &[original.clone()]).unwrap();
        let image_id = Uuid::new_v4().to_string();
        let temporary = dir.join("note-frame-drafts").join(format!("{image_id}.jpg"));
        std::fs::write(&temporary, [0xff, 0xd8, 0xff, 0xd9]).unwrap();
        let connection = db::open(&path).unwrap();
        connection.execute("INSERT INTO note_frame_drafts(id,note_id,session_id,record_id,at_seconds,created_at) VALUES (?1,'note-1','session-1','r',85,1)", [&image_id]).unwrap();
        drop(connection);
        let session: crate::agent::ChatSession = serde_json::from_value(json!({
            "id":"session-1","title":"测试","createdAt":"1","updatedAt":"1","messages":[]
        })).unwrap();
        let choice = ImageSelection { origin: "capture".into(), id: image_id, caption: "指定画面".into(), position: "after".into() };
        let resolved = note_images::resolve_images(&dir, &path, Some(&session), &original, &[choice]).unwrap();
        assert!(temporary.exists());
        let saved = save_design(&path, &dir, &original, "mono", original.updated_at, &resolved).unwrap();
        assert!(!temporary.exists());
        assert_eq!(saved.images[0].origin, "asset");
        assert!(load_design(&path, &dir, &original).unwrap().unwrap().html.contains("指定画面"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn exported_html_is_portable_and_stale_design_is_rejected() {
        let dir = std::env::temp_dir().join(format!("gearnote-design-export-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let original = note();
        db::write_notes(&path, &[original.clone()]).unwrap();
        save_design(&path, &dir, &original, "paper", original.updated_at, &[]).unwrap();
        let exported = export_design(&path, &dir, &original, "html").unwrap();
        let content = std::fs::read_to_string(exported).unwrap();
        assert!(content.contains("<h2>第一节</h2>"));
        assert!(content.contains("data-theme=\"paper\""));
        assert!(export_design(&path, &dir, &original, "exe").is_err());
        let mut changed = original.clone();
        changed.body.push_str("\n已修改");
        changed.updated_at += 1;
        db::upsert_note(&path, &changed, 0).unwrap();
        assert!(export_design(&path, &dir, &changed, "html").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deleting_rendition_preserves_note_and_saved_image_asset() {
        let dir = std::env::temp_dir().join(format!("gearnote-design-delete-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("chat-frames")).unwrap();
        let path = dir.join("test.db");
        let original = note();
        db::write_notes(&path, &[original.clone()]).unwrap();
        let image_id = Uuid::new_v4().to_string();
        std::fs::write(dir.join("chat-frames").join(format!("{image_id}.jpg")), [0xff, 0xd8, 0xff, 0xd9]).unwrap();
        let session: crate::agent::ChatSession = serde_json::from_value(json!({
            "id":"session-1","title":"测试","createdAt":"1","updatedAt":"1",
            "messages":[{"id":"m1","role":"user","createdAt":"1","paragraphs":["截图"],
                "frame":{"imageId":image_id,"recordId":"r","atSeconds":85}}]
        })).unwrap();
        let selection = ImageSelection { origin: "chat".into(), id: image_id, caption: "保留".into(), position: "after".into() };
        let images = note_images::resolve_images(&dir, &path, Some(&session), &original, &[selection]).unwrap();
        let design = save_design(&path, &dir, &original, "mono", original.updated_at, &images).unwrap();
        assert_eq!(delete_design(&path, &original).unwrap(), 1);
        assert!(load_design(&path, &dir, &original).unwrap().is_none());
        assert_eq!(db::read_notes(&path).unwrap().unwrap()[0].body, original.body);
        assert_eq!(note_images::resolve_images(&dir, &path, None, &original, &design.images).unwrap().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires Microsoft Edge and a desktop process environment"]
    fn edge_pdf_smoke_test_when_available() {
        if edge_executable().is_none() { return; }
        let dir = std::env::temp_dir().join(format!("gearnote-pdf-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let html = dir.join("reading.html");
        let pdf = dir.join("reading.pdf");
        std::fs::write(&html, render(&note(), "mono").unwrap()).unwrap();
        print_pdf(&html, &pdf, &dir).unwrap();
        assert!(std::fs::metadata(&pdf).unwrap().len() > 100);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
