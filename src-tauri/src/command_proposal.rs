use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::{Path, PathBuf}, process::{Command, Stdio}, sync::{Arc, Mutex}, thread, time::{Duration, Instant}};
use tauri::{AppHandle, Manager};

const MAX_OUTPUT_BYTES: usize = 8_192;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCommand {
    pub id: String, pub message_id: String, pub program: String, pub args: Vec<String>,
    #[serde(default)] pub effective_args: Vec<String>,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct CommandOutcome { pub exit_code: Option<i32>, pub timed_out: bool, pub output: String, pub truncated: bool }

fn media_name(value: &str) -> bool {
    !value.is_empty() && value.len() <= 180 && !value.starts_with('-') && !value.contains("..")
        && !value.contains(['/', '\\', ':']) && value.chars().all(|ch| ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-' | ' '))
        && ["mp4", "mkv", "webm", "mp3", "wav", "m4a", "mov", "png", "jpg"].iter()
            .any(|ext| value.to_ascii_lowercase().ends_with(&format!(".{ext}")))
}

fn safe_url(value: &str) -> bool {
    url::Url::parse(value).ok().is_some_and(|url| matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some() && url.username().is_empty() && url.password().is_none())
}

pub fn validate(program: &str, args: &[String]) -> Result<(), String> {
    if !matches!(program, "ffmpeg" | "ffprobe" | "yt-dlp") { return Err("程序不在允许名单内".into()); }
    if args.is_empty() || args.len() > 8 || args.iter().map(String::len).sum::<usize>() > 2_000 {
        return Err("参数数量或长度无效".into());
    }
    if args.iter().any(|arg| arg.is_empty() || arg.contains("..") || arg.chars().any(|ch| ch.is_control() || matches!(ch, '$' | '%' | '~' | '|' | '&' | ';' | '>' | '<' | '`' | '^' | '!' | '"' | '\''))) {
        return Err("参数包含路径穿越、环境变量或命令分隔符".into());
    }
    let a = args.iter().map(String::as_str).collect::<Vec<_>>();
    let allowed = match program {
        "ffmpeg" => matches!(a.as_slice(), ["-version"] | ["-formats"] | ["-codecs"] | ["-filters"])
            || matches!(a.as_slice(), ["-i", input, "-f", "null", "-"] if media_name(input))
            || matches!(a.as_slice(), ["-i", input, output] if media_name(input) && media_name(output) && input != output),
        "ffprobe" => matches!(a.as_slice(), ["-version"] | ["-formats"] | ["-codecs"])
            || matches!(a.as_slice(), ["-show_format", input] if media_name(input))
            || matches!(a.as_slice(), ["-show_streams", input] if media_name(input))
            || matches!(a.as_slice(), ["-v", "error", "-show_format", "-show_streams", "-of", "json", input] if media_name(input)),
        "yt-dlp" => matches!(a.as_slice(), ["--version"] | ["--list-extractors"])
            || matches!(a.as_slice(), [url] if safe_url(url))
            || matches!(a.as_slice(), ["--simulate", url] if safe_url(url)),
        _ => false,
    };
    if !allowed { return Err("这组参数不属于当前允许的媒体操作形式".into()); }
    Ok(())
}

pub fn effective_args(program: &str, args: &[String]) -> Result<Vec<String>, String> {
    validate(program, args)?;
    let mut result = match program {
        "ffmpeg" => vec!["-nostdin".into(), "-n".into()],
        "yt-dlp" => vec!["--ignore-config".into(), "--no-playlist".into(), "--restrict-filenames".into(), "--no-overwrites".into(),
            "--limit-rate".into(), "2M".into(), "--max-filesize".into(), "100M".into(),
            "--paths".into(), ".".into(), "--cache-dir".into(), ".".into()],
        _ => Vec::new(),
    };
    result.extend(args.iter().cloned());
    Ok(result)
}

fn bounded_output(bytes: &[u8]) -> (String, bool) {
    let truncated = bytes.len() > MAX_OUTPUT_BYTES;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_OUTPUT_BYTES)]).into_owned();
    (text, truncated)
}

fn read_pipe<R: Read + Send + 'static>(mut input: R, shared: Arc<Mutex<Vec<u8>>>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = [0_u8; 2048];
        while let Ok(count) = input.read(&mut buf) {
            if count == 0 { break; }
            if let Ok(mut saved) = shared.lock() {
                let remaining = (MAX_OUTPUT_BYTES + 1).saturating_sub(saved.len());
                saved.extend_from_slice(&buf[..count.min(remaining)]);
            }
        }
    })
}

fn verify_cached_input(cwd: &Path, program: &str, args: &[String]) -> Result<(), String> {
    let input = match (program, args.first().map(String::as_str)) {
        ("ffmpeg", Some("-i")) => args.get(1),
        ("ffprobe", Some("-show_format" | "-show_streams")) => args.get(1),
        ("ffprobe", Some("-v")) => args.last(),
        _ => None,
    };
    if let Some(input) = input {
        let path = cwd.join(input);
        let actual = path.canonicalize().map_err(|_| "输入媒体文件不在应用命令缓存中")?;
        if !actual.starts_with(cwd) || !actual.is_file() { return Err("输入媒体文件不在应用命令缓存中".into()); }
    }
    if program == "ffmpeg" && args.len() == 3 {
        let output = cwd.join(&args[2]);
        if output.exists() { return Err("输出文件已存在，不会覆盖".into()); }
    }
    Ok(())
}

fn run_process(program_path: PathBuf, program: &str, args: &[String], cwd: &Path) -> Result<CommandOutcome, String> {
    let cwd = cwd.canonicalize().map_err(|error| format!("缓存目录不可用：{error}"))?;
    verify_cached_input(&cwd, program, args)?;
    let effective = effective_args(program, args)?;
    let mut command = Command::new(program_path);
    command.args(&effective).current_dir(cwd)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|error| format!("程序启动失败：{error}"))?;
    let shared = Arc::new(Mutex::new(Vec::new()));
    let out = read_pipe(child.stdout.take().ok_or("标准输出不可用")?, shared.clone());
    let err = read_pipe(child.stderr.take().ok_or("错误输出不可用")?, shared.clone());
    let started = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().map_err(|error| format!("程序状态读取失败：{error}"))? { break (status, false); }
        if started.elapsed() >= TIMEOUT {
            #[cfg(windows)]
            if let Some(root) = std::env::var_os("SystemRoot") {
                let killer = PathBuf::from(root).join("System32").join("taskkill.exe");
                if killer.is_file() {
                    if let Ok(mut cleanup) = Command::new(killer).args(["/PID", &child.id().to_string(), "/T", "/F"])
                        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
                        let until = Instant::now() + Duration::from_secs(2);
                        while Instant::now() < until {
                            if cleanup.try_wait().ok().flatten().is_some() { break; }
                            thread::sleep(Duration::from_millis(30));
                        }
                        let _ = cleanup.kill();
                    }
                }
            }
            let _ = child.kill();
            let status = child.wait().map_err(|error| format!("超时后无法停止程序：{error}"))?;
            break (status, true);
        }
        thread::sleep(Duration::from_millis(30));
    };
    if !timed_out { let _ = out.join(); let _ = err.join(); }
    let bytes = shared.lock().map_err(|_| "命令输出不可用")?;
    let (output, truncated) = bounded_output(&bytes);
    Ok(CommandOutcome { exit_code: status.code(), timed_out, output, truncated })
}

fn resolve_binary(app: &AppHandle, program: &str) -> Result<PathBuf, String> {
    let bundled = crate::storyboard::tool_path(app, program);
    if bundled.is_file() { return bundled.canonicalize().map_err(|error| format!("程序路径无效：{error}")); }
    let name = format!("{program}.exe");
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let path = dir.join(&name);
        if path.is_file() { return path.canonicalize().map_err(|error| format!("程序路径无效：{error}")); }
    }
    Err(format!("未找到 {program}，请安装或使用随应用提供的工具"))
}

pub async fn execute(app: &AppHandle, program: &str, args: &[String]) -> Result<CommandOutcome, String> {
    validate(program, args)?;
    let root = app.path().app_cache_dir().map_err(|error| error.to_string())?;
    fs::create_dir_all(&root).map_err(|error| format!("无法创建应用缓存：{error}"))?;
    let root = root.canonicalize().map_err(|error| format!("应用缓存路径无效：{error}"))?;
    let cwd = root.join("agent-commands");
    fs::create_dir_all(&cwd).map_err(|error| format!("无法创建命令缓存：{error}"))?;
    if !cwd.canonicalize().map_err(|error| format!("命令缓存路径无效：{error}"))?.starts_with(&root) {
        return Err("命令缓存目录不在应用缓存内".into());
    }
    let binary = resolve_binary(app, program)?;
    let program = program.to_owned();
    let args = args.to_vec();
    tauri::async_runtime::spawn_blocking(move || run_process(binary, &program, &args, &cwd))
        .await.map_err(|error| format!("命令任务失败：{error}"))?
}

pub fn format_outcome(outcome: &CommandOutcome) -> String {
    let status = if outcome.timed_out { "超时：程序已停止".into() }
        else if outcome.exit_code == Some(0) { "执行成功（退出码 0）".into() }
        else { format!("执行失败（退出码 {}）", outcome.exit_code.map(|value| value.to_string()).unwrap_or_else(|| "未知".into())) };
    format!("{status}\n{}{}", outcome.output, if outcome.truncated { "\n[输出已达到 8192 字节上限，后续内容已截断]" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn rejects_shell_and_other_programs() {
        assert!(validate("powershell", &["-Command".into(), "echo hi".into()]).is_err());
        assert!(validate("ffmpeg", &["-i".into(), "a.mp4|whoami".into(), "out.mp4".into()]).is_err());
        assert!(validate("yt-dlp", &["--exec".into(), "calc".into()]).is_err());
        assert!(validate("ffmpeg", &["-i".into(), "../secret.mp4".into(), "out.mp4".into()]).is_err());
        assert!(validate("ffprobe", &["-show_format".into(), "C:/secret.mp4".into()]).is_err());
        assert!(validate("ffmpeg", &["-version".into()]).is_ok());
    }

    #[test]
    fn fake_executable_only_runs_after_confirmation_and_output_is_bounded() {
        let invoked = AtomicBool::new(false);
        let proposal = PendingCommand { id: "cmd-1".into(), message_id: "msg-1".into(), program: "ffprobe".into(),
            args: vec!["-version".into()], effective_args: vec!["-version".into()], status: "open".into() };
        assert!(!invoked.load(Ordering::SeqCst));
        let fake_executable = |_: &str, _: &[String]| {
            invoked.store(true, Ordering::SeqCst);
            let (output, truncated) = bounded_output(&vec![b'x'; 9_000]);
            CommandOutcome { exit_code: Some(7), timed_out: false, output, truncated }
        };
        assert_eq!(proposal.status, "open");
        let result = fake_executable(&proposal.program, &proposal.args);
        assert!(invoked.load(Ordering::SeqCst));
        assert!(result.truncated);
        assert_eq!(result.output.len(), MAX_OUTPUT_BYTES);
        assert!(format_outcome(&result).starts_with("执行失败（退出码 7）"));
        let timeout = CommandOutcome { exit_code: None, timed_out: true, output: String::new(), truncated: false };
        assert!(format_outcome(&timeout).starts_with("超时：程序已停止"));
    }
}
