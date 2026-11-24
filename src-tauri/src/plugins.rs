use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

const TOOL_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_TOOL_OUTPUT_BYTES: usize = 32 * 1024;
const MAX_CACHE_DEPTH: usize = 4;
const MAX_CACHE_ITEMS: usize = 5_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub name: String,
    pub found: bool,
    pub path: String,
    pub version: String,
    pub error: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub data_dir: String,
    pub cache_dir: String,
    pub database_bytes: u64,
    pub cache_bytes: u64,
}

struct ToolRun {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
}

fn read_output<R: Read + Send + 'static>(mut reader: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut saved = Vec::new();
        let mut buffer = [0_u8; 2048];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let remaining = MAX_TOOL_OUTPUT_BYTES.saturating_sub(saved.len());
                    saved.extend_from_slice(&buffer[..count.min(remaining)]);
                }
                Err(_) => break,
            }
        }
        saved
    })
}

fn run_tool_version(path: &Path, argument: &str) -> Result<ToolRun, String> {
    let mut command = Command::new(path);
    command
        .arg(argument)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW (0x08000000) | BELOW_NORMAL_PRIORITY_CLASS (0x00004000)：
        // 版本检测是后台诊断，降低优先级避免其启动 CPU 冲击抢占 UI 渲染造成卡顿。
        command.creation_flags(0x0800_4000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动程序：{error}"))?;
    let stdout = match child.stdout.take() {
        Some(pipe) => read_output(pipe),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("无法读取程序标准输出".into());
        }
    };
    let stderr = match child.stderr.take() {
        Some(pipe) => read_output(pipe),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.join();
            return Err("无法读取程序错误输出".into());
        }
    };

    let started = Instant::now();
    let (status, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (status, false),
            Ok(None) if started.elapsed() >= TOOL_TIMEOUT => {
                let _ = child.kill();
                let status = child
                    .wait()
                    .map_err(|error| format!("停止超时程序失败：{error}"))?;
                break (status, true);
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("读取程序状态失败：{error}"));
            }
        }
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(ToolRun {
        status,
        stdout,
        stderr,
        timed_out,
    })
}

fn first_output_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(500).collect())
}

fn unsuccessful_tool_error(run: &ToolRun) -> String {
    let detail = first_output_line(&run.stderr)
        .or_else(|| first_output_line(&run.stdout))
        .unwrap_or_else(|| "没有错误输出".into());
    format!("版本检查退出码 {:?}：{detail}", run.status.code())
}

/// Diagnose the exact tool paths used by storyboard without changing tool resolution.
/// 命令为 async 并把重型工作放入 spawn_blocking，避免占用 tokio 工作线程；
/// 四个工具并行检测，墙钟时间约等于最慢的一个（yt-dlp），而非顺序累加。
#[tauri::command]
pub async fn get_toolchain_status(app: AppHandle) -> Result<Vec<ToolStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let tools = [
            ("node", "--version"),
            ("ffmpeg", "-version"),
            ("ffprobe", "-version"),
            ("yt-dlp", "--version"),
        ];
        std::thread::scope(|scope| {
            let handles: Vec<_> = tools
                .iter()
                .map(|(name, argument)| {
                    let app = &app;
                    scope.spawn(move || check_tool(app, name, argument))
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap_or_else(|_| ToolStatus {
                    name: "unknown".into(), found: false, path: String::new(),
                    version: String::new(), error: "工具检测线程异常".into(),
                }))
                .collect()
        })
    })
    .await
    .map_err(|error| format!("工具检测任务失败：{error}"))
}

fn check_tool(app: &AppHandle, name: &str, argument: &str) -> ToolStatus {
    let path = crate::storyboard::tool_path(app, name);
    let display_path = path.to_string_lossy().into_owned();
    let mut status = ToolStatus {
        name: name.into(),
        found: false,
        path: display_path,
        version: String::new(),
        error: String::new(),
    };
    match run_tool_version(&path, argument) {
        Ok(run) => {
            status.found = true;
            if run.timed_out {
                status.error = "版本检查超过 8 秒，程序已停止".into();
            } else if !run.status.success() {
                status.error = unsuccessful_tool_error(&run);
            } else {
                status.version = first_output_line(&run.stdout)
                    .or_else(|| first_output_line(&run.stderr))
                    .unwrap_or_default();
                if status.version.is_empty() {
                    status.error = "程序成功退出，但没有返回版本信息".into();
                }
            }
        }
        Err(error) => {
            // A bundled path can exist even when the OS refuses to launch it.
            status.found = path.is_file();
            status.error = error;
        }
    }
    status
}

fn bounded_directory_size(root: &Path) -> u64 {
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut visited = 0_usize;
    let mut total = 0_u64;
    while let Some((directory, depth)) = pending.pop() {
        // Root entries have depth 1; no entries deeper than MAX_CACHE_DEPTH are visited.
        if depth >= MAX_CACHE_DEPTH {
            continue;
        }
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries {
            if visited >= MAX_CACHE_ITEMS {
                return total;
            }
            let Ok(entry) = entry else { continue };
            visited += 1;
            let entry_depth = depth + 1;
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(_) => continue,
            };
            if kind.is_file() {
                if let Ok(metadata) = entry.metadata() {
                    total = total.saturating_add(metadata.len());
                }
            } else if kind.is_dir() && entry_depth < MAX_CACHE_DEPTH {
                pending.push((entry.path(), entry_depth));
            }
            // Symlinks and reparse points are deliberately not followed.
        }
    }
    total
}

/// Return workspace paths and bounded, best-effort disk usage diagnostics.
/// 磁盘遍历放入 spawn_blocking，避免占用 tokio 工作线程。
#[tauri::command]
pub async fn get_workspace_info(app: AppHandle) -> Result<WorkspaceInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("无法定位应用数据目录：{error}"))?;
        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|error| format!("无法定位应用缓存目录：{error}"))?;
        let database_bytes = fs::metadata(data_dir.join("knowflow.db"))
            .ok()
            .filter(|metadata| metadata.is_file())
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let cache_bytes = bounded_directory_size(&cache_dir);
        Ok(WorkspaceInfo {
            data_dir: data_dir.to_string_lossy().into_owned(),
            cache_dir: cache_dir.to_string_lossy().into_owned(),
            database_bytes,
            cache_bytes,
        })
    })
    .await
    .map_err(|error| format!("工作区信息任务失败：{error}"))?
}

/// Create and open a fixed workspace directory; arbitrary paths are never accepted.
#[tauri::command]
pub fn open_workspace_path(app: AppHandle, kind: String) -> Result<(), String> {
    let path = match kind.as_str() {
        "data" => app
            .path()
            .app_data_dir()
            .map_err(|error| format!("无法定位应用数据目录：{error}"))?,
        "cache" => app
            .path()
            .app_cache_dir()
            .map_err(|error| format!("无法定位应用缓存目录：{error}"))?,
        "exports" => app
            .path()
            .app_data_dir()
            .map_err(|error| format!("无法定位应用数据目录：{error}"))?
            .join("exports"),
        _ => return Err("工作区目录 kind 只允许 data、cache 或 exports".into()),
    };
    fs::create_dir_all(&path).map_err(|error| format!("无法创建工作区目录：{error}"))?;

    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let program = "xdg-open";
    Command::new(program)
        .arg(&path)
        .spawn()
        .map_err(|error| format!("无法打开工作区目录：{error}"))?;
    Ok(())
}
