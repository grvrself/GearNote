use crate::storage::{AppState, ArticleCapture, CaptureJobStart, VideoCapture};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, TrySendError},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

pub const BRIDGE_PORT: u16 = 45147;
const MAX_BODY_BYTES: usize = 1_200_000;
const MAX_HEADER_BYTES: usize = 16_384;
/// 固定工作者数量：慢连接只占用自己的工作者，不再独占整个入口。
const WORKERS: usize = 4;
/// 有界等待队列：满载时立即返回 503，不无限累积连接。
const QUEUE_CAPACITY: usize = 16;
/// 请求读取的绝对截止时间：从接受连接开始计时（队列等待也计入）。
/// 每次底层读取都按“剩余时间”设置超时，缓慢分批发送数据不能延长总时限。
/// 业务执行（数据库事务等）不在此时限内被强制中断。
const READ_DEADLINE: Duration = Duration::from_secs(5);
/// 响应写入的总时限：不读取响应的客户端不能长期占住工作者。
const WRITE_DEADLINE: Duration = Duration::from_secs(5);

type Dispatch = Arc<dyn Fn(&str, &str, &HashMap<String, String>, &[u8]) -> Result<(u16, Value), (u16, String)> + Send + Sync>;

/// 路由层向宿主发出的通知；生产环境映射为 Tauri 事件，测试中可为空实现。
pub(crate) enum BridgeEvent {
    CaptureJobsChanged,
    RecordsChanged(String),
    ArticleSaved { id: String, title: String },
    VideoSaved { id: String, title: String },
}

pub fn start(app: AppHandle) {
    let watchdog_app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        if watchdog_app.state::<AppState>().expire_capture_jobs() {
            let _ = watchdog_app.emit("gearnote:capture-jobs-changed", ());
        }
    });
    std::thread::spawn(move || {
        let listener = match TcpListener::bind(("127.0.0.1", BRIDGE_PORT)) {
            Ok(listener) => listener,
            Err(error) => {
                if let Ok(mut message) = app.state::<AppState>().bridge_error.lock() {
                    *message = Some(format!("本机连接端口 {BRIDGE_PORT} 无法启动：{error}"));
                }
                return;
            }
        };
        let dispatch_app = app.clone();
        let dispatch: Dispatch = Arc::new(move |method, path, headers, body| {
            let state = dispatch_app.state::<AppState>();
            let notify_app = dispatch_app.clone();
            let notify = |event: &BridgeEvent| match event {
                BridgeEvent::CaptureJobsChanged => { let _ = notify_app.emit("gearnote:capture-jobs-changed", ()); }
                BridgeEvent::RecordsChanged(id) => { let _ = notify_app.emit("knowflow:records-changed", id.clone()); }
                BridgeEvent::ArticleSaved { id, title } => {
                    let _ = crate::agent::record_completed_event(&notify_app, &format!("网页《{title}》正文已保存"), &format!("article-{id}"));
                }
                BridgeEvent::VideoSaved { id, title } => {
                    let _ = crate::agent::record_completed_event(&notify_app, &format!("视频《{title}》字幕已保存"), &format!("subtitle-{id}"));
                }
            };
            route_request(&state, method, path, headers, body, &notify)
        });
        let core = BridgeCore::spawn(listener, format!("127.0.0.1:{BRIDGE_PORT}"), dispatch, READ_DEADLINE, WORKERS, QUEUE_CAPACITY);
        // 服务随应用生命周期运行，无需主动关闭。
        std::mem::forget(core);
    });
}

/// 可测试的服务核心：固定工作者 + 有界队列 + 绝对读取截止时间。
/// 可绑定临时端口；shutdown 会停止接受并 join 全部线程。
#[allow(dead_code)] // 生产路径随应用生命周期运行；shutdown 供隔离测试使用。
pub(crate) struct BridgeCore {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

struct QueuedConnection { stream: TcpStream, accepted_at: Instant }

impl BridgeCore {
    pub(crate) fn spawn(listener: TcpListener, expected_host: String, dispatch: Dispatch,
        read_deadline: Duration, workers: usize, queue_capacity: usize) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = sync_channel::<QueuedConnection>(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut threads = Vec::new();
        for _ in 0..workers {
            let receiver = receiver.clone();
            let dispatch = dispatch.clone();
            let expected_host = expected_host.clone();
            threads.push(std::thread::spawn(move || loop {
                let job = {
                    let queue = match receiver.lock() { Ok(queue) => queue, Err(_) => break };
                    queue.recv_timeout(Duration::from_millis(20))
                };
                match job {
                    Ok(job) => handle_connection(job.stream, job.accepted_at, &expected_host, &dispatch, read_deadline),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }));
        }
        let accept_stop = stop.clone();
        threads.push(std::thread::spawn(move || {
            let _ = listener.set_nonblocking(true);
            loop {
                if accept_stop.load(Ordering::SeqCst) { break; }
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let job = QueuedConnection { stream, accepted_at: Instant::now() };
                        match sender.try_send(job) {
                            Ok(()) => {}
                            Err(TrySendError::Full(job)) => reject_busy(job.stream),
                            Err(TrySendError::Disconnected(_)) => break,
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
                    Err(error) => {
                        eprintln!("GearNote bridge connection failed: {error}");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
            drop(sender);
        }));
        Self { stop, threads }
    }

    #[cfg(test)]
    pub(crate) fn shutdown(self) {
        self.stop.store(true, Ordering::SeqCst);
        for thread in self.threads { let _ = thread.join(); }
    }
}

fn handle_connection(mut stream: TcpStream, accepted_at: Instant, expected_host: &str, dispatch: &Dispatch, read_deadline: Duration) {
    let deadline = accepted_at + read_deadline;
    let mut allowed_origin = None;
    let response = read_request(&stream, deadline).and_then(|(method, path, headers, body)| {
        let origin = check_browser_origin(&headers, expected_host)?;
        allowed_origin = origin.map(str::to_owned);
        if method == "OPTIONS" {
            check_preflight(&headers)?;
            return Ok((204, json!({})));
        }
        dispatch(&method, &path, &headers, &body)
    });
    let failed = response.is_err();
    let (status, body) = match response {
        Ok(result) => result,
        Err((status, message)) => (status, json!({ "error": message })),
    };
    write_response(&mut stream, status, &body, allowed_origin.as_deref());
    // 请求未读完时，接收缓冲里可能还有客户端数据；Windows 下直接关闭会发 RST，
    // 导致对端丢失已发出的响应。关闭前先短暂排空输入。
    if failed { drain_before_close(&mut stream); }
}

/// 关闭前短暂排空未读输入，避免 RST 吞掉已写入的响应；不无限等待慢客户端。
fn drain_before_close(stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let mut scratch = [0_u8; 4096];
    for _ in 0..5 {
        match stream.read(&mut scratch) {
            Ok(0) | Err(_) => break,
            Ok(_) => continue,
        }
    }
}

/// 配对码校验与全部既有路由；与传输层分离，便于隔离测试。
fn route_request(state: &AppState, method: &str, path: &str, headers: &HashMap<String, String>,
    body: &[u8], notify: &dyn Fn(&BridgeEvent)) -> Result<(u16, Value), (u16, String)> {
    let supplied = headers.get("x-knowflow-token").map(String::as_str).unwrap_or_default();
    if !same_token(supplied, &state.pair_code) {
        return Ok((401, json!({ "error": "配对码无效，请在扩展设置中重新输入" })));
    }
    state.mark_extension_seen();
    match (method, path) {
        ("GET", "/v1/status") => Ok((200, json!({ "connected": true }))),
        ("POST", "/v1/capture-jobs") => {
            let input: CaptureJobStart = serde_json::from_slice(body)
                .map_err(|_| (400, "采集任务格式无效".to_owned()))?;
            let job = state.start_capture_job(input).map_err(|message| (400, message))?;
            notify(&BridgeEvent::CaptureJobsChanged);
            Ok((200, json!(job)))
        }
        ("POST", "/v1/capture-jobs/stage") => {
            let value: Value = serde_json::from_slice(body).map_err(|_| (400, "采集阶段格式无效".to_owned()))?;
            let id = value.get("id").and_then(Value::as_str).ok_or((400, "采集任务编号缺失".to_owned()))?;
            if value.get("stage").and_then(Value::as_str) != Some("summarizing") {
                return Ok((400, json!({ "error": "采集阶段无效" })));
            }
            let changed = state.set_capture_job_stage(id).map_err(|message| (400, message))?;
            if changed { notify(&BridgeEvent::CaptureJobsChanged); }
            Ok((200, json!({ "changed": changed })))
        }
        ("POST", "/v1/capture-jobs/fail") => {
            let value: Value = serde_json::from_slice(body).map_err(|_| (400, "失败状态格式无效".to_owned()))?;
            let id = value.get("id").and_then(Value::as_str).ok_or((400, "采集任务编号缺失".to_owned()))?;
            let reason = value.get("reason").and_then(Value::as_str).filter(|reason| !reason.trim().is_empty()).unwrap_or("采集失败，请在原页面重试。");
            let changed = state.finish_capture_job(id, Some(reason)).map_err(|message| (400, message))?;
            if changed { notify(&BridgeEvent::CaptureJobsChanged); }
            Ok((200, json!({ "changed": changed })))
        }
        ("POST", "/v1/articles") => {
            let job_id = serde_json::from_slice::<Value>(body).ok().and_then(|value| value.get("jobId").and_then(Value::as_str).map(str::to_owned));
            let input: ArticleCapture = serde_json::from_slice(body)
                .map_err(|_| (400, "文章数据格式无效".to_owned()))?;
            let result = state.capture(input).map_err(|message| (400, message))?;
            if job_id.as_deref().is_some_and(|id| state.finish_capture_job(id, None).unwrap_or(false)) {
                notify(&BridgeEvent::CaptureJobsChanged);
            }
            if !result.already_saved {
                notify(&BridgeEvent::RecordsChanged(result.record.id.clone()));
                notify(&BridgeEvent::ArticleSaved { id: result.record.id.clone(), title: result.record.title.clone() });
            }
            Ok((200, json!(result)))
        }
        ("POST", "/v1/videos") => {
            let job_id = serde_json::from_slice::<Value>(body).ok().and_then(|value| value.get("jobId").and_then(Value::as_str).map(str::to_owned));
            let input: VideoCapture = serde_json::from_slice(body)
                .map_err(|_| (400, "视频字幕数据格式无效".to_owned()))?;
            let result = state.capture_video(input).map_err(|message| (400, message))?;
            if job_id.as_deref().is_some_and(|id| state.finish_capture_job(id, None).unwrap_or(false)) {
                notify(&BridgeEvent::CaptureJobsChanged);
            }
            if !result.already_saved {
                notify(&BridgeEvent::RecordsChanged(result.record.id.clone()));
                if result.record.status == crate::ProcessStatus::Ready {
                    notify(&BridgeEvent::VideoSaved { id: result.record.id.clone(), title: result.record.title.clone() });
                }
            }
            Ok((200, json!(result)))
        }
        _ => Ok((404, json!({ "error": "入口不存在" }))),
    }
}

// Browser requests must come from a Chrome/Edge extension. Native clients without
// an Origin still need the pairing token on every non-preflight request.
fn check_browser_origin<'a>(headers: &'a HashMap<String, String>, expected_host: &str) -> Result<Option<&'a str>, (u16, String)> {
    if headers.get("host").map(String::as_str) != Some(expected_host) {
        return Err((403, "本机接口地址无效".into()));
    }
    let Some(origin) = headers.get("origin").map(String::as_str) else { return Ok(None) };
    let Some(id) = origin.strip_prefix("chrome-extension://") else {
        return Err((403, "只允许浏览器扩展连接本机接口".into()));
    };
    if id.len() != 32 || !id.bytes().all(|byte| (b'a'..=b'p').contains(&byte)) {
        return Err((403, "浏览器扩展来源无效".into()));
    }
    Ok(Some(origin))
}

fn check_preflight(headers: &HashMap<String, String>) -> Result<(), (u16, String)> {
    if !headers.contains_key("origin") {
        return Err((403, "预检请求缺少扩展来源".into()));
    }
    if !matches!(headers.get("access-control-request-method").map(String::as_str), Some("GET" | "POST")) {
        return Err((403, "预检请求方法无效".into()));
    }
    if let Some(requested) = headers.get("access-control-request-headers") {
        if requested.split(',').any(|header| !matches!(header.trim().to_ascii_lowercase().as_str(), "content-type" | "x-knowflow-token")) {
            return Err((403, "预检请求头无效".into()));
        }
    }
    Ok(())
}

fn same_token(received: &str, expected: &str) -> bool {
    received.len() == expected.len()
        && received.bytes().zip(expected.bytes()).fold(0_u8, |diff, (a, b)| diff | (a ^ b)) == 0
}

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
}

/// 每次底层读取前按绝对截止时间重设剩余超时；到期返回 408。
fn set_remaining_timeout(stream: &TcpStream, deadline: Instant) -> Result<(), (u16, String)> {
    let remaining = deadline.checked_duration_since(Instant::now()).filter(|duration| !duration.is_zero())
        .ok_or((408, "请求读取超时".to_owned()))?;
    stream.set_read_timeout(Some(remaining)).map_err(|_| (500, "连接无法读取".to_owned()))
}

fn read_request(stream: &TcpStream, deadline: Instant) -> Result<(String, String, HashMap<String, String>, Vec<u8>), (u16, String)> {
    let cloned = stream.try_clone().map_err(|_| (500, "连接无法读取".to_owned()))?;
    let mut reader = BufReader::new(cloned);
    let mut header_bytes: usize = 0;
    let request_line = read_header_line(&mut reader, MAX_HEADER_BYTES, deadline)?;
    header_bytes = header_bytes.checked_add(request_line.len()).ok_or((431, "请求头过长".to_owned()))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    if !matches!(method.as_str(), "GET" | "POST" | "OPTIONS") || !path.starts_with("/v1/") {
        return Err((404, "入口不存在".into()));
    }
    let mut headers = HashMap::new();
    loop {
        let remaining = MAX_HEADER_BYTES.checked_sub(header_bytes).ok_or((431, "请求头过长".to_owned()))?;
        let line = read_header_line(&mut reader, remaining, deadline)?;
        header_bytes = header_bytes.checked_add(line.len()).ok_or((431, "请求头过长".to_owned()))?;
        if header_bytes > MAX_HEADER_BYTES { return Err((431, "请求头过长".into())); }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if line.is_empty() {
            return Err((400, "请求头不完整".into()));
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let length = headers.get("content-length")
        .map(|value| value.parse::<usize>().map_err(|_| (400, "正文长度无效".to_owned())))
        .transpose()?
        .unwrap_or(0);
    if length > MAX_BODY_BYTES {
        return Err((413, "文章内容超过大小限制".into()));
    }
    let mut body = vec![0; length];
    let mut filled = 0;
    while filled < length {
        // 超时必须设置在实际执行读取的套接字句柄上（Windows 的复制句柄不共享该选项）。
        set_remaining_timeout(reader.get_ref(), deadline)?;
        match reader.read(&mut body[filled..]) {
            Ok(0) => return Err((400, "正文未完整传送".into())),
            Ok(read) => filled += read,
            Err(error) if is_timeout(&error) => return Err((408, "请求读取超时".into())),
            Err(_) => return Err((400, "正文未完整传送".into())),
        }
    }
    Ok((method, path, headers, body))
}

/// 逐字节读取请求头行，保证每次底层读取都受剩余时间约束，
/// 缓慢分批发送请求头不能把读取期限无限延长。
fn read_header_line(reader: &mut BufReader<TcpStream>, limit: usize, deadline: Instant) -> Result<String, (u16, String)> {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        set_remaining_timeout(reader.get_ref(), deadline)?;
        match reader.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                bytes.push(byte[0]);
                // 读取后立即检查，确保任何返回路径（含读到换行）都不超过上限。
                if bytes.len() > limit { return Err((431, "请求头过长".into())); }
                if byte[0] == b'\n' { break; }
            }
            Err(error) if is_timeout(&error) => return Err((408, "请求读取超时".into())),
            Err(_) => return Err((400, "请求头无法读取".into())),
        }
    }
    String::from_utf8(bytes).map_err(|_| (400, "请求头编码无效".into()))
}

fn write_response(stream: &mut TcpStream, status: u16, body: &Value, origin: Option<&str>) {
    write_response_with_deadline(stream, status, body, origin, Instant::now() + WRITE_DEADLINE);
}

fn write_response_with_deadline(stream: &mut TcpStream, status: u16, body: &Value, origin: Option<&str>, deadline: Instant) {
    let phrase = match status {
        200 => "OK", 204 => "No Content", 400 => "Bad Request", 401 => "Unauthorized",
        403 => "Forbidden", 404 => "Not Found", 408 => "Request Timeout",
        413 => "Content Too Large", 431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let bytes = if status == 204 { Vec::new() } else { body.to_string().into_bytes() };
    let cors = cors_headers(origin);
    let header = format!(
        "HTTP/1.1 {status} {phrase}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nVary: Origin\r\n{cors}Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    let mut payload = header.into_bytes();
    payload.extend_from_slice(&bytes);
    let _ = write_all_deadline(stream, &payload, deadline);
}

/// 响应写入同样按剩余时间逐段限时，避免不读取响应的客户端占住工作者。
fn write_all_deadline(stream: &mut TcpStream, mut bytes: &[u8], deadline: Instant) -> std::io::Result<()> {
    while !bytes.is_empty() {
        let remaining = deadline.checked_duration_since(Instant::now()).filter(|duration| !duration.is_zero())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "响应写入超时"))?;
        stream.set_write_timeout(Some(remaining))?;
        let written = stream.write(bytes)?;
        if written == 0 { return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "响应写入中断")); }
        bytes = &bytes[written..];
    }
    Ok(())
}

/// 队列满载时快速拒绝；使用更短的写入时限，避免接受入口被占用。
fn reject_busy(mut stream: TcpStream) {
    let body = json!({ "error": "本机桥接正忙，请稍后重试" });
    write_response_with_deadline(&mut stream, 503, &body, None, Instant::now() + Duration::from_secs(1));
    drain_before_close(&mut stream);
}

fn cors_headers(origin: Option<&str>) -> String {
    origin.map(|origin| format!("Access-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, X-KnowFlow-Token\r\nAccess-Control-Allow-Private-Network: true\r\n")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Condvar, Barrier};

    fn headers(origin: Option<&str>) -> HashMap<String, String> {
        let mut headers = HashMap::from([("host".into(), "127.0.0.1:45147".into())]);
        if let Some(origin) = origin { headers.insert("origin".into(), origin.into()); }
        headers
    }

    #[test]
    fn accepts_extension_origin_and_native_client() {
        let extension = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";
        assert_eq!(check_browser_origin(&headers(Some(extension)), "127.0.0.1:45147").unwrap(), Some(extension));
        assert_eq!(check_browser_origin(&headers(None), "127.0.0.1:45147").unwrap(), None);
    }

    #[test]
    fn rejects_web_origin_and_rebound_host() {
        assert!(check_browser_origin(&headers(Some("https://example.com")), "127.0.0.1:45147").is_err());
        assert!(check_browser_origin(&headers(Some("null")), "127.0.0.1:45147").is_err());
        assert!(check_browser_origin(&headers(Some("chrome-extension://bad/id")), "127.0.0.1:45147").is_err());
        let mut rebound = headers(None);
        rebound.insert("host".into(), "example.com:45147".into());
        assert!(check_browser_origin(&rebound, "127.0.0.1:45147").is_err());
    }

    #[test]
    fn preflight_allows_only_bridge_methods_and_headers() {
        let mut request = headers(Some("chrome-extension://abcdefghijklmnopabcdefghijklmnop"));
        request.insert("access-control-request-method".into(), "POST".into());
        request.insert("access-control-request-headers".into(), "content-type, x-knowflow-token".into());
        assert!(check_preflight(&request).is_ok());
        request.insert("access-control-request-headers".into(), "authorization".into());
        assert!(check_preflight(&request).is_err());
        request.insert("access-control-request-headers".into(), "content-type".into());
        request.insert("access-control-request-method".into(), "DELETE".into());
        assert!(check_preflight(&request).is_err());
        assert!(check_preflight(&headers(None)).is_err());
    }

    #[test]
    fn cors_headers_only_echo_approved_origin() {
        let extension = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";
        let request = headers(Some(extension));
        let origin = check_browser_origin(&request, "127.0.0.1:45147").unwrap();
        assert!(cors_headers(origin).contains(&format!("Access-Control-Allow-Origin: {extension}\r\n")));
        assert!(cors_headers(None).is_empty());
        assert!(check_browser_origin(&headers(Some("https://example.com")), "127.0.0.1:45147").is_err());
    }

    // ---- 服务核心集成测试：临时端口、隔离状态、可关闭 ----

    fn test_core(dispatch: Dispatch, workers: usize, queue: usize, deadline: Duration) -> (BridgeCore, u16) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let core = BridgeCore::spawn(listener, format!("127.0.0.1:{port}"), dispatch, deadline, workers, queue);
        (core, port)
    }

    fn ok_dispatch() -> Dispatch {
        Arc::new(|method, path, _headers, _body| {
            if method == "GET" && path == "/v1/status" { Ok((200, json!({ "connected": true }))) }
            else { Ok((404, json!({ "error": "入口不存在" }))) }
        })
    }

    fn connect(port: u16) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream
    }

    fn get_request(port: u16) -> String {
        format!("GET /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n")
    }

    fn read_head(stream: &mut TcpStream) -> (u16, usize) {
        let mut buf = Vec::new();
        let mut byte = [0_u8; 1];
        loop {
            match stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => { buf.push(byte[0]); if buf.ends_with(b"\r\n\r\n") { break; } }
                Err(_) => break,
            }
        }
        let text = String::from_utf8_lossy(&buf).into_owned();
        let status = text.split_whitespace().nth(1).and_then(|value| value.parse().ok()).unwrap_or(0);
        let length = text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.trim().eq_ignore_ascii_case("content-length")).then(|| value.trim().parse().ok())?
        }).unwrap_or(0);
        (status, length)
    }

    fn read_response(stream: &mut TcpStream) -> (u16, Value) {
        let (status, length) = read_head(stream);
        let mut body = vec![0; length];
        if length > 0 { stream.read_exact(&mut body).unwrap(); }
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    #[test]
    fn slow_connection_does_not_block_other_workers() {
        let (core, port) = test_core(ok_dispatch(), 2, 4, Duration::from_secs(5));
        let slow = connect(port); // 连上后不发送任何数据，占住一个工作者
        std::thread::sleep(Duration::from_millis(100));
        let mut fast = connect(port);
        fast.write_all(get_request(port).as_bytes()).unwrap();
        let started = Instant::now();
        assert_eq!(read_response(&mut fast).0, 200);
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(slow);
        core.shutdown();
    }

    /// 缓慢分批发送的同时探测响应；收到响应头即停止发送，避免关闭竞态。
    fn trickle_and_read_status(stream: &mut TcpStream, payload: &[u8], interval: Duration) -> u16 {
        let mut head: Vec<u8> = Vec::new();
        let mut byte = [0_u8; 1];
        for value in payload {
            if stream.write_all(&[*value]).is_err() { break; }
            let _ = stream.set_read_timeout(Some(Duration::from_millis(2)));
            match stream.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {
                    head.push(byte[0]);
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    while !head.ends_with(b"\r\n\r\n") {
                        match stream.read(&mut byte) {
                            Ok(0) => break,
                            Ok(_) => head.push(byte[0]),
                            Err(_) => break,
                        }
                    }
                    break;
                }
                Err(_) => {}
            }
            std::thread::sleep(interval);
        }
        let text = String::from_utf8_lossy(&head).into_owned();
        text.split_whitespace().nth(1).and_then(|value| value.parse().ok()).unwrap_or(0)
    }

    #[test]
    fn trickle_headers_cannot_extend_absolute_deadline() {
        let (core, port) = test_core(ok_dispatch(), 1, 4, Duration::from_millis(300));
        let mut stream = connect(port);
        stream.write_all(b"GET /v1/status HTTP/1.1\r\n").unwrap();
        let started = Instant::now();
        let payload = format!("Host: 127.0.0.1:{port}\r\n\r\n");
        assert_eq!(trickle_and_read_status(&mut stream, payload.as_bytes(), Duration::from_millis(60)), 408);
        assert!(started.elapsed() < Duration::from_secs(2));
        core.shutdown();
    }

    #[test]
    fn trickle_body_cannot_extend_absolute_deadline() {
        let (core, port) = test_core(ok_dispatch(), 1, 4, Duration::from_millis(300));
        let mut stream = connect(port);
        let head = format!("POST /v1/articles HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 64\r\n\r\n");
        stream.write_all(head.as_bytes()).unwrap();
        let started = Instant::now();
        assert_eq!(trickle_and_read_status(&mut stream, &[b'x'; 64], Duration::from_millis(60)), 408);
        assert!(started.elapsed() < Duration::from_secs(2));
        core.shutdown();
    }

    #[test]
    fn queue_full_returns_busy_quickly() {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let dispatch_gate = gate.clone();
        let dispatch: Dispatch = Arc::new(move |_method, _path, _headers, _body| {
            let (lock, cvar) = &*dispatch_gate;
            let mut released = lock.lock().unwrap();
            while !*released {
                let (guard, timeout) = cvar.wait_timeout(released, Duration::from_secs(5)).unwrap();
                released = guard;
                if timeout.timed_out() { break; }
            }
            Ok((200, json!({ "connected": true })))
        });
        let (core, port) = test_core(dispatch, 1, 1, Duration::from_secs(5));
        let mut first = connect(port);
        first.write_all(get_request(port).as_bytes()).unwrap();
        std::thread::sleep(Duration::from_millis(100)); // 工作者进入阻塞业务
        let mut queued = connect(port);
        queued.write_all(get_request(port).as_bytes()).unwrap();
        std::thread::sleep(Duration::from_millis(100)); // 队列容量 1 已被占用
        let mut overflow = connect(port);
        overflow.write_all(get_request(port).as_bytes()).unwrap();
        let started = Instant::now();
        assert_eq!(read_response(&mut overflow).0, 503);
        assert!(started.elapsed() < Duration::from_secs(1));
        let (lock, cvar) = &*gate;
        *lock.lock().unwrap() = true;
        cvar.notify_all();
        assert_eq!(read_response(&mut first).0, 200);
        assert_eq!(read_response(&mut queued).0, 200);
        core.shutdown();
    }

    #[test]
    fn oversize_headers_and_body_still_rejected() {
        let (core, port) = test_core(ok_dispatch(), 1, 4, Duration::from_secs(5));
        let mut stream = connect(port);
        let huge = format!("GET /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Big: {}\r\n\r\n", "a".repeat(MAX_HEADER_BYTES + 100));
        stream.write_all(huge.as_bytes()).unwrap();
        assert_eq!(read_head(&mut stream).0, 431);
        let mut stream = connect(port);
        let head = format!("POST /v1/articles HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n", MAX_BODY_BYTES + 1);
        stream.write_all(head.as_bytes()).unwrap();
        assert_eq!(read_head(&mut stream).0, 413);
        core.shutdown();
    }

    #[test]
    fn header_limit_boundaries_do_not_panic_and_recover() {
        let (core, port) = test_core(ok_dispatch(), 1, 4, Duration::from_secs(5));
        let request_line = "GET /v1/status HTTP/1.1\r\n";
        // 围绕总上限构造边界（CRLF）：超限必须稳定 431，任何边界都不得 panic。
        for extra in [MAX_HEADER_BYTES - 1, MAX_HEADER_BYTES, MAX_HEADER_BYTES + 1, MAX_HEADER_BYTES + 100] {
            let pad_len = extra.saturating_sub(request_line.len() + 5);
            let request = format!("{request_line}X: {}\r\n\r\n", "a".repeat(pad_len));
            let mut stream = connect(port);
            stream.write_all(request.as_bytes()).unwrap();
            let status = read_head(&mut stream).0;
            assert!(status != 0, "边界 {extra} 不应 panic 或无响应");
            if extra > MAX_HEADER_BYTES { assert_eq!(status, 431, "超限必须返回 431"); }
        }
        // LF 行尾同样受上限约束，不 panic。
        let lf_line = "GET /v1/status HTTP/1.1\n";
        let request = format!("{lf_line}X: {}\n\n", "a".repeat(MAX_HEADER_BYTES + 50));
        let mut stream = connect(port);
        stream.write_all(request.as_bytes()).unwrap();
        assert_eq!(read_head(&mut stream).0, 431);
        // 超限请求之后，正常请求仍须成功。
        let mut good = connect(port);
        good.write_all(get_request(port).as_bytes()).unwrap();
        assert_eq!(read_response(&mut good).0, 200);
        core.shutdown();
    }

    #[test]
    fn security_checks_still_enforced_by_core() {
        let (core, port) = test_core(ok_dispatch(), 1, 4, Duration::from_secs(5));
        let cases = [
            format!("GET /v1/status HTTP/1.1\r\nHost: example.com:{port}\r\n\r\n"),
            format!("GET /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: https://example.com\r\n\r\n"),
            format!("OPTIONS /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccess-Control-Request-Method: POST\r\n\r\n"),
            format!("OPTIONS /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: chrome-extension://abcdefghijklmnopabcdefghijklmnop\r\nAccess-Control-Request-Method: DELETE\r\n\r\n"),
        ];
        for request in cases {
            let mut stream = connect(port);
            stream.write_all(request.as_bytes()).unwrap();
            assert_eq!(read_head(&mut stream).0, 403);
        }
        core.shutdown();
    }

    fn state_dispatch(state: Arc<AppState>) -> Dispatch {
        Arc::new(move |method, path, headers, body| route_request(&state, method, path, headers, body, &|_event| {}))
    }

    #[test]
    fn wrong_pair_code_rejected_and_correct_accepted() {
        let dir = std::env::temp_dir().join(format!("gearnote-bridge-token-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = Arc::new(AppState::for_test("0123456789abcdef0123456789abcdef", dir.join(crate::db::FILE_NAME)));
        let (core, port) = test_core(state_dispatch(state), 1, 4, Duration::from_secs(5));
        let mut stream = connect(port);
        let wrong = format!("GET /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-KnowFlow-Token: ffffffffffffffffffffffffffffffff\r\n\r\n");
        stream.write_all(wrong.as_bytes()).unwrap();
        assert_eq!(read_response(&mut stream).0, 401);
        let mut stream = connect(port);
        let right = format!("GET /v1/status HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-KnowFlow-Token: 0123456789abcdef0123456789abcdef\r\n\r\n");
        stream.write_all(right.as_bytes()).unwrap();
        assert_eq!(read_response(&mut stream).0, 200);
        core.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn concurrent_capture_of_same_article_stays_deduplicated() {
        let dir = std::env::temp_dir().join(format!("gearnote-bridge-dedup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join(crate::db::FILE_NAME);
        // 初始化隔离数据库并完成迁移，等价于应用启动时的状态。
        crate::db::write_records(&db_path, &[]).unwrap();
        let state = Arc::new(AppState::for_test("0123456789abcdef0123456789abcdef", db_path));
        let (core, port) = test_core(state_dispatch(state.clone()), 4, 8, Duration::from_secs(5));
        let body = json!({ "url": "https://example.com/article", "title": "并发测试文章", "text": "并发捕获时去重仍须成立，同一网址只保存一条记录。".repeat(6), "capturedAt": "2026-01-01T00:00:00Z" }).to_string();
        let request = format!("POST /v1/articles HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-KnowFlow-Token: 0123456789abcdef0123456789abcdef\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let request = request.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                let mut stream = connect(port);
                barrier.wait();
                stream.write_all(request.as_bytes()).unwrap();
                read_response(&mut stream)
            }));
        }
        let results: Vec<(u16, Value)> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();
        assert!(results.iter().all(|(status, _)| *status == 200), "并发捕获响应异常：{results:?}");
        let saved: Vec<bool> = results.iter().map(|(_, value)| value["alreadySaved"].as_bool().unwrap()).collect();
        assert_eq!(saved.iter().filter(|value| !**value).count(), 1, "同一资料并发捕获只应新建一次");
        assert_eq!(state.records.lock().unwrap().len(), 1);
        core.shutdown();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bad_connection_does_not_kill_service_and_shutdown_joins() {
        let (core, port) = test_core(ok_dispatch(), 1, 4, Duration::from_secs(5));
        let mut bad = connect(port);
        bad.write_all(b"GARBAGE-NOT-HTTP\r\n\r\n").unwrap();
        let _ = read_head(&mut bad);
        let mut good = connect(port);
        good.write_all(get_request(port).as_bytes()).unwrap();
        assert_eq!(read_response(&mut good).0, 200);
        let started = Instant::now();
        core.shutdown();
        assert!(started.elapsed() < Duration::from_secs(2), "shutdown 必须能 join 全部线程");
    }
}
