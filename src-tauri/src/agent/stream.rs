// 流式响应与服务商错误。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamDelta { request_id: String, delta: String }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolActivity { request_id: String, message: String }

#[derive(Default)]
struct SseParser { buffer: Vec<u8>, done: bool, finish_reason: Option<String> }

impl SseParser {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.buffer.extend(bytes.iter().copied().filter(|byte| *byte != b'\r'));
        if self.buffer.len() > 1_000_000 { return Err("模型流式响应的单条事件过大".into()); }
        let mut deltas = Vec::new();
        while let Some(end) = self.buffer.windows(2).position(|pair| pair == b"\n\n") {
            let frame: Vec<u8> = self.buffer.drain(..end + 2).collect();
            let frame = std::str::from_utf8(&frame[..end]).map_err(|_| "模型流式响应不是 UTF-8 文本")?;
            let data = frame.lines().filter_map(|line| line.strip_prefix("data:")).map(str::trim_start).collect::<Vec<_>>().join("\n");
            if data.is_empty() { continue; }
            if data == "[DONE]" { self.done = true; continue; }
            let value: serde_json::Value = serde_json::from_str(&data).map_err(|_| "模型流式响应不是兼容的 Chat Completions 格式")?;
            if let Some(error) = value.pointer("/error/message").and_then(serde_json::Value::as_str) {
                return Err(format!("模型流式响应出错：{}", error.chars().take(300).collect::<String>()));
            }
            if let Some(choices) = value.get("choices").and_then(serde_json::Value::as_array) {
                if let Some(choice) = choices.first() {
                    if let Some(reason) = choice.get("finish_reason").and_then(serde_json::Value::as_str) {
                        self.finish_reason = Some(reason.into());
                    }
                    if let Some(delta) = choice.pointer("/delta/content").and_then(serde_json::Value::as_str) {
                        if !delta.is_empty() { deltas.push(delta.into()); }
                    }
                }
            }
        }
        Ok(deltas)
    }
}

fn publish_deltas(app: &AppHandle, request_id: &str, answer: &mut String, deltas: Vec<String>, emit: bool) -> Result<(), String> {
    for delta in deltas {
        answer.push_str(&delta);
        if answer.chars().count() > 20_000 { return Err("模型回答超过 20000 字，已停止接收".into()); }
        if emit { let _ = app.emit("knowflow:agent-stream", StreamDelta { request_id: request_id.into(), delta }); }
    }
    Ok(())
}

fn provider_error(status: reqwest::StatusCode, body: &str, base_url: &str, model: &str) -> String {
    if status == reqwest::StatusCode::BAD_REQUEST
        && base_url.starts_with("https://api.deepseek.com")
        && model.eq_ignore_ascii_case("DeepSeek-V4.1-Flash-1017") {
        return "DeepSeek 的 API 模型 ID 应填 deepseek-flash；当前填写的是版本展示名。请在设置中修改模型名称。".into();
    }
    let detail = serde_json::from_str::<serde_json::Value>(body).ok()
        .and_then(|value| value.pointer("/error/message").or_else(|| value.get("message")).and_then(|message| message.as_str()).map(str::to_owned))
        .unwrap_or_default();
    let detail: String = detail.chars().filter(|ch| !ch.is_control()).take(300).collect();
    if detail.is_empty() {
        format!("模型服务返回 HTTP {}，请检查 API 地址和模型 ID", status.as_u16())
    } else {
        format!("模型服务返回 HTTP {}：{detail}", status.as_u16())
    }
}

fn request_user_content(text: String, image_url: Option<&str>) -> serde_json::Value {
    if let Some(image_url) = image_url {
        serde_json::json!([{"type": "text", "text": text}, {"type": "image_url", "image_url": {"url": image_url}}])
    } else { serde_json::Value::String(text) }
}
