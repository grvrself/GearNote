// 对话压缩和摘要。
const COMPACT_KEEP_RECENT: usize = 8;
const AUTO_COMPACT_MESSAGE_THRESHOLD: usize = 16;
const AUTO_COMPACT_CHAR_THRESHOLD: usize = 3_200;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompactSummary {
    confirmed_decisions: Vec<String>,
    open_questions: Vec<String>,
    source_ids: Vec<String>,
}

fn parse_compact_summary(answer: &str, evidence: &str) -> Result<String, String> {
    let summary: CompactSummary = serde_json::from_str(answer.trim()).map_err(|_| "压缩摘要必须是完整的结构化 JSON")?;
    for item in summary.confirmed_decisions.iter().chain(summary.open_questions.iter()).chain(summary.source_ids.iter()) {
        if item.trim().is_empty() || item.chars().any(char::is_control) || item.chars().count() > 300 {
            return Err("压缩摘要字段为空、过长或包含无效字符".into());
        }
    }
    if summary.confirmed_decisions.len() > 8 || summary.open_questions.len() > 8 || summary.source_ids.len() > 20 {
        return Err("压缩摘要字段过多".into());
    }
    if summary.source_ids.iter().any(|id| !evidence.contains(id)) { return Err("摘要包含对话中未出现的资料 ID".into()); }
    let encoded = serde_json::to_string(&summary).map_err(|_| "压缩摘要无法编码")?;
    if encoded.chars().count() > 1_500 { return Err("摘要超过 1500 字，请重试".into()); }
    Ok(encoded)
}

fn should_auto_compact(session: &ChatSession) -> bool {
    let pending = &session.messages[session.compacted_count.min(session.messages.len())..];
    pending.len() >= AUTO_COMPACT_MESSAGE_THRESHOLD ||
        (pending.len() >= COMPACT_KEEP_RECENT + 2 && pending.iter()
            .flat_map(|message| message.paragraphs.iter())
            .map(|paragraph| paragraph.chars().count()).sum::<usize>() >= AUTO_COMPACT_CHAR_THRESHOLD)
}

async fn compact_session_context(app: &AppHandle, state: &AgentState, session_id: &str, require_idle: bool, timeout: Duration) -> Result<ChatState, String> {
    let (prior_summary, start, end, input, last_id) = {
        let store = state.sessions.lock().map_err(|_| "会话暂不可用")?;
        if store.active_id != session_id { return Err("会话已切换，请重新压缩".into()); }
        let session = store.sessions.iter().find(|item| item.id == session_id).ok_or("当前会话不存在")?;
        let limit = session.messages.len().saturating_sub(COMPACT_KEEP_RECENT);
        if limit <= session.compacted_count { return Err("目前没有需要压缩的较早对话".into()); }
        let mut input = String::new();
        let mut end = session.compacted_count;
        for message in &session.messages[session.compacted_count..limit] {
            let part = format!("{}（{}）：{}\n", message.role, message.created_at, message.paragraphs.join(" ").chars().take(1200).collect::<String>());
            if !input.is_empty() && input.chars().count() + part.chars().count() > 10_000 { break; }
            input.push_str(&part);
            end += 1;
        }
        (session.compacted_summary.clone(), session.compacted_count, end, input, session.messages[end - 1].id.clone())
    };
    let prompt = format!("已有摘要：\n{prior_summary}\n\n本次较早对话：\n{input}");
    let answer = auxiliary_completion_with_timeout(state, "把较早对话压缩成后续追问的背景。只提取用户明确确认的决定、尚未解决的问题、对话明确提到的资料 ID；不要把模型推测写成决定，不添加新事实。只输出完整 JSON 对象：{\"confirmedDecisions\":[\"...\"],\"openQuestions\":[\"...\"],\"sourceIds\":[\"...\"]}。每项最多 300 字，整体最多 1500 字。旧摘要可能是普通散文，请按原文理解。", prompt.clone(), timeout).await?;
    let summary = parse_compact_summary(&answer, &prompt)?;
    mutate_chat(app, state, require_idle, true, |store| {
        if store.active_id != session_id { return Err("会话已切换，请重新压缩".into()); }
        let session = store.sessions.iter_mut().find(|item| item.id == session_id).ok_or("会话不存在")?;
        if session.compacted_count != start || session.messages.get(end - 1).map(|item| &item.id) != Some(&last_id) { return Err("对话已更新，请重新压缩".into()); }
        session.compacted_summary = summary;
        session.compacted_count = end;
        session.compacted_at = now_millis()?;
        Ok(ChatEdit::Session(session_id.into()))
    })
}

#[tauri::command]
pub async fn compact_chat_context(app: AppHandle, state: State<'_, AgentState>) -> Result<ChatState, String> {
    let session_id = state.sessions.lock().map_err(|_| "会话暂不可用")?.active_id.clone();
    let result = compact_session_context(&app, &state, &session_id, true, Duration::from_secs(120)).await?;
    if let Ok(mut failures) = state.auto_compact_failures.lock() { failures.remove(&session_id); }
    Ok(result)
}
