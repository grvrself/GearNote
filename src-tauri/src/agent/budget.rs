// 用户可调整的单次阅读预算；续读每次仍使用一份有限预算。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadingBudget {
    pub tool_calls: usize, pub exploration_rounds: usize, pub tool_chars: usize,
    pub deep_seconds: u64, pub deep_batches: usize, pub request_seconds: u64, pub answer_chars: usize,
    pub strategy: ReadingStrategy, pub effort: ReadingEffort,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadingStrategy { #[default] Adaptive, Full }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadingEffort { #[default] Standard, Thorough, Custom }

impl Default for ReadingBudget {
    fn default() -> Self { Self { tool_calls: 6, exploration_rounds: 3, tool_chars: 4_000,
        deep_seconds: deep_video::MAX_SECONDS, deep_batches: deep_video::MAX_BATCHES, request_seconds: 180, answer_chars: 20_000,
        strategy: ReadingStrategy::Adaptive, effort: ReadingEffort::Standard } }
}

impl ReadingBudget {
    fn effective(&self) -> Self {
        let mut budget = self.clone();
        match self.effort {
            ReadingEffort::Standard => { budget.tool_calls = 24; budget.exploration_rounds = 12; budget.tool_chars = 16_000; budget.deep_seconds = 180; budget.deep_batches = 64; }
            ReadingEffort::Thorough => { budget.tool_calls = 48; budget.exploration_rounds = 24; budget.tool_chars = 32_000; budget.deep_seconds = 600; budget.deep_batches = 128; }
            ReadingEffort::Custom => {}
        }
        budget
    }
    fn validate(&self) -> Result<(), String> {
        if !(2..=48).contains(&self.tool_calls) || !(1..=32).contains(&self.exploration_rounds)
            || !(4_000..=32_000).contains(&self.tool_chars) || !(30..=900).contains(&self.deep_seconds)
            || !(4..=128).contains(&self.deep_batches) || !(30..=900).contains(&self.request_seconds)
            || !(2_000..=100_000).contains(&self.answer_chars) { return Err("阅读预算超出设置允许范围".into()); }
        Ok(())
    }
}

#[tauri::command]
pub fn save_reading_budget(state: State<'_, AgentState>, budget: ReadingBudget) -> Result<AgentStatus, String> {
    let budget = budget.effective();
    budget.validate()?;
    let mut saved = state.config.lock().map_err(|_| "模型配置暂不可用")?;
    let mut next = saved.clone();
    next.reading_budget = budget;
    fs::write(&state.config_path, serde_json::to_vec_pretty(&next).map_err(|error| error.to_string())?)
        .map_err(|error| format!("阅读预算保存失败：{error}"))?;
    *saved = next;
    drop(saved);
    get_agent_status(state)
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReadProgress {
    deep: bool, full: bool, batch_count: usize, findings: Vec<deep_video::Finding>, read_chunk_ids: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadContinuation {
    message_id: String, question: String, referenced_ids: Vec<String>, fingerprint: String, progress: ReadProgress,
}

fn reading_fingerprint(records: &[KnowledgeRecord]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(records).unwrap_or_default().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn continuation_for(session: &ChatSession, message_id: &str, ids: &[String]) -> Result<ReadContinuation, String> {
    let continuation = session.read_continuation.as_ref().filter(|value| value.message_id == message_id)
        .ok_or("这次阅读已结束或已被新的提问替代")?;
    if session.messages.last().is_none_or(|message| message.id != message_id) {
        return Err("会话已有新的消息，请重新提问后读取".into());
    }
    // 三方核对：续读原引用、当前会话持久化引用、客户端本次传入引用必须一致。
    // 仅比较客户端传入值会让“会话参考已改、旧调用者仍传原值”绕过校验。
    let mut expected = continuation.referenced_ids.clone(); expected.sort();
    let mut actual = ids.to_vec(); actual.sort();
    let mut current = session.referenced_ids.clone(); current.sort();
    if actual != expected || current != expected { return Err("本轮参考已改变，请重新提问后读取".into()); }
    Ok(continuation.clone())
}
