use crate::storage::{AppState, ArticleCapture, CaptureJobStart, VideoCapture};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

pub const BRIDGE_PORT: u16 = 45147;
const MAX_BODY_BYTES: usize = 1_200_000;
const MAX_HEADER_BYTES: usize = 16_384;

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
        for connection in listener.incoming() {
            match connection {
                Ok(stream) => handle_connection(stream, &app),
                Err(error) => eprintln!("GearNote bridge connection failed: {error}"),
            }
        }
    });
}

fn handle_connection(mut stream: TcpStream, app: &AppHandle) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut allowed_origin = None;
    let response = read_request(&stream).and_then(|(method, path, headers, body)| {
        let origin = check_browser_origin(&headers)?;
        allowed_origin = origin.map(str::to_owned);
        if method == "OPTIONS" {
            check_preflight(&headers)?;
            return Ok((204, json!({})));
        }
        let supplied = headers.get("x-knowflow-token").map(String::as_str).unwrap_or_default();
        let state = app.state::<AppState>();
        if !same_token(supplied, &state.pair_code) {
            return Ok((401, json!({ "error": "配对码无效，请在扩展设置中重新输入" })));
        }
        state.mark_extension_seen();
        match (method.as_str(), path.as_str()) {
            ("GET", "/v1/status") => Ok((200, json!({ "connected": true }))),
            ("POST", "/v1/capture-jobs") => {
                let input: CaptureJobStart = serde_json::from_slice(&body)
                    .map_err(|_| (400, "采集任务格式无效".to_owned()))?;
                let job = state.start_capture_job(input).map_err(|message| (400, message))?;
                let _ = app.emit("gearnote:capture-jobs-changed", ());
                Ok((200, json!(job)))
            }
            ("POST", "/v1/capture-jobs/stage") => {
                let value: Value = serde_json::from_slice(&body).map_err(|_| (400, "采集阶段格式无效".to_owned()))?;
                let id = value.get("id").and_then(Value::as_str).ok_or((400, "采集任务编号缺失".to_owned()))?;
                if value.get("stage").and_then(Value::as_str) != Some("summarizing") {
                    return Ok((400, json!({ "error": "采集阶段无效" })));
                }
                let changed = state.set_capture_job_stage(id).map_err(|message| (400, message))?;
                if changed { let _ = app.emit("gearnote:capture-jobs-changed", ()); }
                Ok((200, json!({ "changed": changed })))
            }
            ("POST", "/v1/capture-jobs/fail") => {
                let value: Value = serde_json::from_slice(&body).map_err(|_| (400, "失败状态格式无效".to_owned()))?;
                let id = value.get("id").and_then(Value::as_str).ok_or((400, "采集任务编号缺失".to_owned()))?;
                let reason = value.get("reason").and_then(Value::as_str).filter(|reason| !reason.trim().is_empty()).unwrap_or("采集失败，请在原页面重试。");
                let changed = state.finish_capture_job(id, Some(reason)).map_err(|message| (400, message))?;
                if changed { let _ = app.emit("gearnote:capture-jobs-changed", ()); }
                Ok((200, json!({ "changed": changed })))
            }
            ("POST", "/v1/articles") => {
                let job_id = serde_json::from_slice::<Value>(&body).ok().and_then(|value| value.get("jobId").and_then(Value::as_str).map(str::to_owned));
                let input: ArticleCapture = serde_json::from_slice(&body)
                    .map_err(|_| (400, "文章数据格式无效".to_owned()))?;
                let result = state.capture(input).map_err(|message| (400, message))?;
                if job_id.as_deref().is_some_and(|id| state.finish_capture_job(id, None).unwrap_or(false)) {
                    let _ = app.emit("gearnote:capture-jobs-changed", ());
                }
                if !result.already_saved {
                    let _ = app.emit("knowflow:records-changed", result.record.id.clone());
                    let _ = crate::agent::record_completed_event(&app, &format!("网页《{}》正文已保存", result.record.title), &format!("article-{}", result.record.id));
                }
                Ok((200, json!(result)))
            }
            ("POST", "/v1/videos") => {
                let job_id = serde_json::from_slice::<Value>(&body).ok().and_then(|value| value.get("jobId").and_then(Value::as_str).map(str::to_owned));
                let input: VideoCapture = serde_json::from_slice(&body)
                    .map_err(|_| (400, "视频字幕数据格式无效".to_owned()))?;
                let result = state.capture_video(input).map_err(|message| (400, message))?;
                if job_id.as_deref().is_some_and(|id| state.finish_capture_job(id, None).unwrap_or(false)) {
                    let _ = app.emit("gearnote:capture-jobs-changed", ());
                }
                if !result.already_saved {
                    let _ = app.emit("knowflow:records-changed", result.record.id.clone());
                    if result.record.status == crate::ProcessStatus::Ready {
                        let _ = crate::agent::record_completed_event(&app, &format!("视频《{}》字幕已保存", result.record.title), &format!("subtitle-{}", result.record.id));
                    }
                }
                Ok((200, json!(result)))
            }
            _ => Ok((404, json!({ "error": "入口不存在" }))),
        }
    });
    let (status, body) = match response {
        Ok(result) => result,
        Err((status, message)) => (status, json!({ "error": message })),
    };
    write_response(&mut stream, status, &body, allowed_origin.as_deref());
}

// Browser requests must come from a Chrome/Edge extension. Native clients without
// an Origin still need the pairing token on every non-preflight request.
fn check_browser_origin(headers: &HashMap<String, String>) -> Result<Option<&str>, (u16, String)> {
    if headers.get("host").map(String::as_str) != Some("127.0.0.1:45147") {
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

fn read_request(stream: &TcpStream) -> Result<(String, String, HashMap<String, String>, Vec<u8>), (u16, String)> {
    let cloned = stream.try_clone().map_err(|_| (500, "连接无法读取".to_owned()))?;
    let mut reader = BufReader::new(cloned);
    let mut header_bytes = 0;
    let request_line = read_header_line(&mut reader, MAX_HEADER_BYTES)?;
    header_bytes += request_line.len();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    if !matches!(method.as_str(), "GET" | "POST" | "OPTIONS") || !path.starts_with("/v1/") {
        return Err((404, "入口不存在".into()));
    }
    let mut headers = HashMap::new();
    loop {
        let line = read_header_line(&mut reader, MAX_HEADER_BYTES - header_bytes)?;
        header_bytes += line.len();
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
    reader.read_exact(&mut body).map_err(|_| (400, "正文未完整传送".to_owned()))?;
    Ok((method, path, headers, body))
}

fn read_header_line(reader: &mut BufReader<TcpStream>, limit: usize) -> Result<String, (u16, String)> {
    let mut bytes = Vec::new();
    let read = reader.take((limit + 1) as u64).read_until(b'\n', &mut bytes)
        .map_err(|_| (400, "请求头无法读取".to_owned()))?;
    if read > limit {
        return Err((431, "请求头过长".into()));
    }
    String::from_utf8(bytes).map_err(|_| (400, "请求头编码无效".into()))
}

fn write_response(stream: &mut TcpStream, status: u16, body: &Value, origin: Option<&str>) {
    let phrase = match status {
        200 => "OK", 204 => "No Content", 400 => "Bad Request", 401 => "Unauthorized",
        403 => "Forbidden", 404 => "Not Found", 413 => "Content Too Large", 431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    };
    let bytes = if status == 204 { Vec::new() } else { body.to_string().into_bytes() };
    let cors = cors_headers(origin);
    let header = format!(
        "HTTP/1.1 {status} {phrase}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nVary: Origin\r\n{cors}Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&bytes);
}

fn cors_headers(origin: Option<&str>) -> String {
    origin.map(|origin| format!("Access-Control-Allow-Origin: {origin}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, X-KnowFlow-Token\r\nAccess-Control-Allow-Private-Network: true\r\n")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(origin: Option<&str>) -> HashMap<String, String> {
        let mut headers = HashMap::from([("host".into(), "127.0.0.1:45147".into())]);
        if let Some(origin) = origin { headers.insert("origin".into(), origin.into()); }
        headers
    }

    #[test]
    fn accepts_extension_origin_and_native_client() {
        let extension = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";
        assert_eq!(check_browser_origin(&headers(Some(extension))).unwrap(), Some(extension));
        assert_eq!(check_browser_origin(&headers(None)).unwrap(), None);
    }

    #[test]
    fn rejects_web_origin_and_rebound_host() {
        assert!(check_browser_origin(&headers(Some("https://example.com"))).is_err());
        assert!(check_browser_origin(&headers(Some("null"))).is_err());
        assert!(check_browser_origin(&headers(Some("chrome-extension://bad/id"))).is_err());
        let mut rebound = headers(None);
        rebound.insert("host".into(), "example.com:45147".into());
        assert!(check_browser_origin(&rebound).is_err());
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
        let origin = check_browser_origin(&request).unwrap();
        assert!(cors_headers(origin).contains(&format!("Access-Control-Allow-Origin: {extension}\r\n")));
        assert!(cors_headers(None).is_empty());
        assert!(check_browser_origin(&headers(Some("https://example.com"))).is_err());
    }
}
