use crate::storage::{AppState, StoryboardCell, StoryboardResult};
use serde::Deserialize;
use std::{fs, path::{Path, PathBuf}, process::Command};
use tauri::{AppHandle, Manager, State};

#[derive(Deserialize)]
struct Evidence { cells: Vec<StoryboardCell> }

/// 启动时清理崩溃/强制退出遗留的临时凭据；只限专用目录与旧版文件模式，失败不阻塞启动。
pub(crate) fn cleanup_credentials_on_startup(video_cache_dir: &Path) {
    let _ = crate::credentials::cleanup_stale(&video_cache_dir.join("credentials"));
    let _ = crate::credentials::cleanup_legacy(video_cache_dir);
}

pub(crate) fn validate_bilibili_cookies(content: &str) -> Result<(), String> {
    if content.len() > 512_000 || content.contains('\0') {
        return Err("cookies.txt 超过 500 KB 或包含无效字符。".into());
    }
    let mut has_session = false;
    for line in content.lines() {
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() || (line.starts_with('#') && !line.starts_with("#HttpOnly_")) { continue; }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 7 { return Err("请选择 Netscape 格式的 B 站 cookies.txt。".into()); }
        let domain = fields[0].trim_start_matches("#HttpOnly_").trim_start_matches('.');
        if domain != "bilibili.com" && !domain.ends_with(".bilibili.com") {
            return Err("cookies.txt 包含其他网站的登录信息；请只导出 B 站 cookies。".into());
        }
        if fields[5] == "SESSDATA" && !fields[6].is_empty() { has_session = true; }
    }
    if !has_session { return Err("没有找到 B 站 SESSDATA；请先登录 B 站，再导出 Netscape cookies.txt。".into()); }
    Ok(())
}

fn prepare_bilibili_cookies(content: &str, credentials_dir: &Path) -> Result<crate::credentials::TaskCookies, String> {
    validate_bilibili_cookies(content)?;
    crate::credentials::TaskCookies::prepare(content, credentials_dir)
}

// Windows canonical paths can start with \\?\; Node's main-script loader treats
// that form as a directory and fails with EISDIR on the drive letter.
fn node_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let value = path.to_string_lossy();
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

fn engine_path(app: &AppHandle) -> Result<PathBuf, String> {
    let bundled = app.path().resource_dir().map_err(|error| error.to_string())?
        .join("storyboard").join("bin").join("storyboard.mjs");
    if bundled.is_file() { return Ok(bundled); }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let beside_exe = parent.join("storyboard").join("bin").join("storyboard.mjs");
            if beside_exe.is_file() { return Ok(beside_exe); }
        }
    }
    let development = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..").join("vendor").join("dsh-video-storyboard")
        .join("bin").join("storyboard.mjs");
    if development.is_file() { return Ok(development); }
    Err("分镜引擎文件缺失，请重新安装 GearNote。".into())
}

pub(crate) fn tool_path(app: &AppHandle, program: &str) -> PathBuf {
    if let Ok(resources) = app.path().resource_dir() {
        let bundled = resources.join("tools").join(format!("{program}.exe"));
        if bundled.is_file() { return bundled; }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let portable = parent.join("tools").join(format!("{program}.exe"));
            if portable.is_file() { return portable; }
        }
    }
    PathBuf::from(program)
}

fn available(app: &AppHandle, program: &str) -> bool {
    Command::new(tool_path(app, program)).arg(if program == "node" || program == "yt-dlp" { "--version" } else { "-version" })
        .output().is_ok_and(|output| output.status.success())
}

fn image_names(dir: &Path) -> Result<Vec<String>, String> {
    let mut names = fs::read_dir(dir).map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter_map(|item| item.file_name().into_string().ok())
        .filter(|name| name == "board.png" || (name.starts_with("board-") && name.ends_with(".png")))
        .collect::<Vec<_>>();
    names.sort();
    if names.is_empty() { return Err("分镜引擎没有生成图片。".into()); }
    Ok(names)
}

pub(crate) async fn capture_frame(app: &AppHandle, source_url: &str, seconds: u32, image_id: &str) -> Result<PathBuf, String> {
    let url = url::Url::parse(source_url).map_err(|_| "视频链接无效")?;
    if !matches!(url.scheme(), "http" | "https") { return Err("视频链接无效".into()); }
    let script = engine_path(app)?.with_file_name("knowflow-frame.mjs");
    if !script.is_file() { return Err("取帧脚本缺失，请重新安装 GearNote。".into()); }
    if !available(app, "node") || !available(app, "ffmpeg") || !available(app, "ffprobe") || !available(app, "yt-dlp") {
        return Err("取帧需要 Node.js、FFmpeg（含 ffprobe）和 yt-dlp。".into());
    }
    let dir = app.path().app_data_dir().map_err(|error| error.to_string())?.join("chat-frames");
    fs::create_dir_all(&dir).map_err(|error| format!("无法创建截图目录：{error}"))?;
    let output_path = dir.join(format!("{image_id}.jpg"));
    let run_script = node_path(script);
    let run_url = source_url.to_owned();
    let run_output = node_path(output_path.clone());
    let node_binary = tool_path(app, "node");
    let ffmpeg_binary = tool_path(app, "ffmpeg");
    let ffprobe_binary = tool_path(app, "ffprobe");
    let ytdlp_binary = tool_path(app, "yt-dlp");
    let tools_dir = node_binary.parent().filter(|path| path.join("yt-dlp.exe").is_file()).map(Path::to_path_buf);
    let cache_dir = app.path().app_cache_dir().map_err(|error| error.to_string())?.join("video-storyboard");
    let credentials_dir = cache_dir.join("credentials");
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut command = Command::new(node_binary);
        if let Some(tools_dir) = tools_dir.as_ref().filter(|path| path.is_dir()) {
            let mut paths = vec![node_path(tools_dir.clone())];
            paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
            if let Ok(path) = std::env::join_paths(paths) { command.env("PATH", path); }
        }
        command.env("KNOWFLOW_FFMPEG", &ffmpeg_binary)
            .env("DSH_FFMPEG_PATH", &ffmpeg_binary)
            .env("DSH_FFPROBE_PATH", &ffprobe_binary)
            .env("DSH_YTDLP_PATH", &ytdlp_binary)
            .env("DSH_VIDEO_CACHE_DIR", node_path(cache_dir.join("scan")))
            .env("DSH_VIDEO_MEDIA_DIR", node_path(cache_dir.join("media")))
            .arg(run_script).arg(run_url).arg(seconds.to_string()).arg(run_output).output()
    }).await.map_err(|error| error.to_string())?
        .map_err(|error| format!("取帧程序启动失败：{error}"))?;
    if !result.status.success() {
        let _ = fs::remove_file(&output_path);
        let stderr = String::from_utf8_lossy(&result.stderr);
        if stderr.contains("HTTP 567") {
            return Err("B 站拒绝匿名取流（HTTP 567）。请先在记录栏使用 cookies.txt 完成画面分析，再从缓存取帧。".into());
        }
        // 取帧虽无 cookies 入参，底层工具仍可能把缓存凭据写进 stderr；与画面分析一致脱敏。
        let secrets = crate::credentials::cached_sensitive_values(&credentials_dir);
        let safe = crate::credentials::redact(&stderr, &secrets);
        return Err(format!("取帧失败：{}", safe.chars().take(350).collect::<String>()));
    }
    let size = fs::metadata(&output_path).map_err(|_| "截图文件不存在")?.len();
    if size == 0 || size > 8_000_000 {
        let _ = fs::remove_file(&output_path);
        return Err("截图为空或超过 8 MB。".into());
    }
    Ok(output_path)
}

/// 持锁定位并写回分镜结果，避免异步下载期间并发捕获/删除导致位置索引漂移。
fn save_storyboard_result(state: &AppState, record_id: &str, result: &StoryboardResult) -> Result<(), String> {
    let records = state.records.lock().map_err(|_| "资料库暂不可用")?;
    let position = records.iter().position(|record| record.id == record_id).ok_or("视频记录已被删除")?;
    let mut record = crate::db::get_record(&state.records_path, record_id)?;
    record.storyboard = Some(result.clone());
    crate::db::upsert_record(&state.records_path, &record, position)
}

#[tauri::command]
pub async fn analyze_video_storyboard(app: AppHandle, state: State<'_, AppState>, record_id: String, cookies_text: Option<String>) -> Result<StoryboardResult, String> {
    let (source_url, is_bilibili) = {
        let record = crate::db::get_record(&state.records_path, &record_id)?;
        if record.record_type != "video" { return Err("只有视频记录可分析画面。".into()); }
        if record.storyboard.is_some() { return Err("该视频已完成画面分析。".into()); }
        let url = url::Url::parse(&record.source_url).map_err(|_| "视频链接无效")?;
        if !matches!(url.scheme(), "http" | "https") { return Err("视频链接无效".into()) }
        let host = url.host_str().unwrap_or_default();
        (record.source_url.clone(), host == "bilibili.com" || host.ends_with(".bilibili.com"))
    };
    if cookies_text.is_some() && !is_bilibili { return Err("当前仅支持 B 站 cookies.txt。".into()); }
    let engine = engine_path(&app)?;
    if !available(&app, "node") { return Err("缺少 Node.js，请安装后重启 GearNote。".into()); }
    if !available(&app, "ffmpeg") || !available(&app, "ffprobe") { return Err("缺少 FFmpeg（ffmpeg 和 ffprobe），请安装后重启 GearNote。".into()); }
    if !available(&app, "yt-dlp") {
        return Err("缺少 yt-dlp。请安装后重启 GearNote，网页视频画面分析需要它下载视频。".into());
    }
    let dir = app.path().app_data_dir().map_err(|error| error.to_string())?
        .join("storyboards").join(&record_id);
    let cache_dir = app.path().app_cache_dir().map_err(|error| error.to_string())?
        .join("video-storyboard");
    let credentials_dir = cache_dir.join("credentials");
    // 新任务开始前先回收上次崩溃/强制退出遗留的凭据；只限专用目录与旧版文件模式。
    let _ = crate::credentials::cleanup_stale(&credentials_dir);
    let _ = crate::credentials::cleanup_legacy(&cache_dir);
    let cookies = cookies_text.as_deref().map(|content| prepare_bilibili_cookies(content, &credentials_dir)).transpose()?;
    // 提取敏感值用于错误返回脱敏；独立于移入闭包的 guard，存活到错误处理之后。
    let credential_secrets = cookies_text.as_deref().map(crate::credentials::sensitive_values).unwrap_or_default();
    fs::create_dir_all(&dir).map_err(|error| format!("无法创建分镜目录：{error}"))?;
    for name in image_names(&dir).unwrap_or_default() {
        fs::remove_file(dir.join(name)).map_err(|error| format!("无法清理旧分镜图：{error}"))?;
    }
    let out = dir.join("board.png");
    let engine_run = engine.clone();
    let out_run = out.clone();
    let node_binary = tool_path(&app, "node");
    let ffmpeg_binary = tool_path(&app, "ffmpeg");
    let ffprobe_binary = tool_path(&app, "ffprobe");
    let ytdlp_binary = tool_path(&app, "yt-dlp");
    let tools_dir = node_binary.parent().filter(|path| path.join("yt-dlp.exe").is_file()).map(Path::to_path_buf);
    let output = tauri::async_runtime::spawn_blocking(move || {
        // 凭据 guard 移入阻塞闭包：生命周期跟随实际媒体进程。
        // 即使外层异步任务不再等待，正在运行的进程也不会提前失去所需文件。
        crate::credentials::run_with_cookies(cookies, |cookies_file| {
            let cookies_path = cookies_file.map(|path| node_path(path.to_path_buf()));
            let run = |low_quality: bool| {
                let mut command = Command::new(&node_binary);
                if let Some(tools_dir) = tools_dir.as_ref().filter(|path| path.is_dir()) {
                    let mut paths = vec![node_path(tools_dir.clone())];
                    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
                    if let Ok(path) = std::env::join_paths(paths) { command.env("PATH", path); }
                }
                command.env("DSH_FFMPEG_PATH", &ffmpeg_binary)
                    .env("DSH_FFPROBE_PATH", &ffprobe_binary)
                    .env("DSH_YTDLP_PATH", &ytdlp_binary)
                    .env("DSH_VIDEO_CACHE_DIR", node_path(cache_dir.join("scan")))
                    .env("DSH_VIDEO_MEDIA_DIR", node_path(cache_dir.join("media")))
                    .arg(node_path(engine_run.clone())).arg(&source_url)
                    .arg("--out").arg(node_path(out_run.clone()))
                    .arg("--strategy").arg("auto")
                    .arg("--max-frames").arg("64")
                    .arg("--max-sheets").arg("4")
                    .arg("--transcript").arg("off");
                if low_quality { command.arg("--quality").arg("low"); }
                if let Some(path) = &cookies_path { command.arg("--cookies").arg(path); }
                command.output()
            };
            let first = run(false)?;
            let denied = !first.status.success() && String::from_utf8_lossy(&first.stderr).contains("HTTP 567");
            if denied && cookies_path.is_none() { run(true) } else { Ok(first) }
        })
    }).await.map_err(|error| error.to_string())?
        .map_err(|error| format!("分镜引擎启动失败：{error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("HTTP 567") {
            return Err("B 站拒绝匿名取流（HTTP 567）。请先在浏览器登录该站，再选择只含 B 站登录信息的 Netscape cookies.txt 重试；若已选择文件，请重新导出登录态。".into());
        }
        // 媒体工具可能把凭据写入 stderr；在返回边界脱敏，避免进入界面与日志。
        let safe = crate::credentials::redact(&stderr, &credential_secrets);
        return Err(format!("画面分析失败：{}", safe.chars().take(500).collect::<String>()));
    }
    let evidence_path = dir.join("board.evidence.json");
    let evidence: Evidence = serde_json::from_slice(&fs::read(&evidence_path)
        .map_err(|error| format!("无法读取分镜时间点：{error}"))?)
        .map_err(|error| format!("分镜时间点无效：{error}"))?;
    let result = StoryboardResult { image_count: image_names(&dir)?.len(), cells: evidence.cells };
    save_storyboard_result(&state, &record_id, &result)?;
    let _ = crate::agent::record_completed_event(&app, "视频画面分析已完成", &format!("storyboard-{record_id}"));
    Ok(result)
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16) | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        encoded.push(TABLE[((n >> 18) & 63) as usize] as char);
        encoded.push(TABLE[((n >> 12) & 63) as usize] as char);
        encoded.push(if chunk.len() > 1 { TABLE[((n >> 6) & 63) as usize] as char } else { '=' });
        encoded.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
    }
    encoded
}

#[tauri::command]
pub fn get_storyboard_image(app: AppHandle, state: State<'_, AppState>, record_id: String, index: usize) -> Result<String, String> {
    let record = crate::db::get_record(&state.records_path, &record_id)?;
    let count = record.storyboard.as_ref().ok_or("尚未分析画面")?.image_count;
    if index >= count { return Err("分镜图片编号无效".into()); }
    let dir = app.path().app_data_dir().map_err(|error| error.to_string())?
        .join("storyboards").join(&record_id);
    let name = image_names(&dir)?.get(index).cloned().ok_or("分镜图片缺失")?;
    let bytes = fs::read(dir.join(name)).map_err(|_| "分镜图片读取失败")?;
    if bytes.len() > 8_000_000 { return Err("分镜图片过大".into()) }
    Ok(format!("data:image/png;base64,{}", base64(&bytes)))
}

#[tauri::command]
pub fn open_video_at(state: State<'_, AppState>, record_id: String, at_seconds: u32) -> Result<(), String> {
    let records = state.records.lock().map_err(|_| "资料库暂不可用")?;
    let record = records.iter().find(|record| record.id == record_id && record.record_type == "video")
        .ok_or("找不到该视频记录")?;
    let mut url = url::Url::parse(&record.source_url).map_err(|_| "视频链接无效")?;
    if !matches!(url.scheme(), "http" | "https") { return Err("视频链接无效".into()); }
    if at_seconds > 864_000 { return Err("时间点无效".into()); }
    url.query_pairs_mut().append_pair("t", &at_seconds.to_string());
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let program = "xdg-open";
    Command::new(program).arg(url.as_str()).spawn().map_err(|error| format!("无法打开原视频：{error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_evidence_frame_time_reaches_frontend() {
        let evidence: Evidence = serde_json::from_str(r#"{"cells":[{"cell":1,"sheet":1,"from":0,"to":2,"frame_at":1.25}]}"#).unwrap();
        assert_eq!(evidence.cells[0].frame_at, Some(1.25));
        let frontend = serde_json::to_value(&evidence.cells[0]).unwrap();
        assert_eq!(frontend["frameAt"], 1.25);
    }

    #[cfg(windows)]
    #[test]
    fn node_main_script_uses_regular_windows_path() {
        assert_eq!(node_path(PathBuf::from(r"\\?\C:\KnowFlow\storyboard.mjs")), PathBuf::from(r"C:\KnowFlow\storyboard.mjs"));
        assert_eq!(node_path(PathBuf::from(r"\\?\UNC\server\share\storyboard.mjs")), PathBuf::from(r"\\server\share\storyboard.mjs"));
    }

    #[test]
    fn cookies_file_must_contain_only_bilibili_login() {
        let valid = "# Netscape HTTP Cookie File\n#HttpOnly_.bilibili.com\tTRUE\t/\tTRUE\t1800000000\tSESSDATA\ttest-value\n";
        assert!(validate_bilibili_cookies(valid).is_ok());
        assert!(validate_bilibili_cookies(".example.com\tTRUE\t/\tTRUE\t1800000000\tSESSDATA\tsecret\n").is_err());
        assert!(validate_bilibili_cookies(".bilibili.com\tTRUE\t/\tTRUE\t1800000000\tbuvid3\ttest\n").is_err());
    }

    #[test]
    fn saving_storyboard_uses_current_position_after_concurrent_insert() {
        let dir = std::env::temp_dir().join(format!("gearnote-storyboard-race-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join(crate::db::FILE_NAME);
        let video: crate::storage::KnowledgeRecord = serde_json::from_value(serde_json::json!({
            "id": "vid-1", "type": "video", "title": "测试视频", "site": "example.com",
            "savedAt": "2026-11-25T00:00:00Z", "status": "ready",
            "sourceUrl": "https://example.com/video", "contentText": "字幕", "excerpt": []
        })).unwrap();
        crate::db::write_records(&db_path, &[video.clone()]).unwrap();
        let state = AppState::for_test("", db_path.clone());
        state.records.lock().unwrap().push(crate::storage::RecordSummary::of(&video));

        // 模拟异步分镜期间并发捕获新文章，旧视频 ordinal 从 0 后移为 1。
        state.capture(crate::storage::ArticleCapture {
            url: "https://example.com/article".into(),
            title: "新文章".into(),
            text: "测试文章正文内容。".repeat(20),
            captured_at: "2026-11-25T00:01:00Z".into(),
        }).unwrap();

        let result = StoryboardResult {
            image_count: 1,
            cells: vec![StoryboardCell { cell: 1, sheet: 1, from: 0.0, to: 1.0, frame_at: Some(0.5) }],
        };
        save_storyboard_result(&state, "vid-1", &result).unwrap();
        let saved = crate::db::get_record(&db_path, "vid-1").unwrap();
        assert_eq!(saved.storyboard.unwrap().image_count, 1);
        drop(state);
        fs::remove_dir_all(dir).unwrap();
    }
}
