use crate::storage::{AppState, ArticleCapture, VideoCapture};
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
                Err(error) => eprintln!("KnowFlow bridge connection failed: {error}"),
            }
        }
    });
}

fn handle_connection(mut stream: TcpStream, app: &AppHandle) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let response = read_request(&stream).and_then(|(method, path, headers, body)| {
        if method == "OPTIONS" {
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
            ("POST", "/v1/articles") => {
                let input: ArticleCapture = serde_json::from_slice(&body)
                    .map_err(|_| (400, "文章数据格式无效".to_owned()))?;
                let result = state.capture(input).map_err(|message| (400, message))?;
                if !result.already_saved {
                    let _ = app.emit("knowflow:records-changed", result.record.id.clone());
                    let _ = crate::agent::record_completed_event(&app, &format!("网页《{}》正文已保存", result.record.title), &format!("article-{}", result.record.id));
                }
                Ok((200, json!(result)))
            }
            ("POST", "/v1/videos") => {
                let input: VideoCapture = serde_json::from_slice(&body)
                    .map_err(|_| (400, "视频字幕数据格式无效".to_owned()))?;
                let result = state.capture_video(input).map_err(|message| (400, message))?;
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
    write_response(&mut stream, status, &body);
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

fn write_response(stream: &mut TcpStream, status: u16, body: &Value) {
    let phrase = match status {
        200 => "OK", 204 => "No Content", 400 => "Bad Request", 401 => "Unauthorized",
        404 => "Not Found", 413 => "Content Too Large", 431 => "Request Header Fields Too Large",
        _ => "Internal Server Error",
    };
    let bytes = if status == 204 { Vec::new() } else { body.to_string().into_bytes() };
    let header = format!(
        "HTTP/1.1 {status} {phrase}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type, X-KnowFlow-Token\r\nAccess-Control-Allow-Private-Network: true\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&bytes);
}
