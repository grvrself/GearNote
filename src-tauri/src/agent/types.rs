// 会话、消息和提案的数据类型及持久化。
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatCitation { pub record_id: String, #[serde(skip_serializing_if = "Option::is_none")] pub at_seconds: Option<u32>, #[serde(skip_serializing_if = "Option::is_none")] pub source_index: Option<usize>, #[serde(default, skip_serializing_if = "Option::is_none")] pub label: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] pub kind: Option<String> }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFrame { pub image_id: String, pub record_id: String, pub at_seconds: u32 }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRun {
    pub elapsed_ms: u64,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_sources: Vec<ReadSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_coverage: Option<ReadCoverage>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadCoverage { pub read_until_seconds: u32, pub total_seconds: u32, pub batch_count: usize, pub complete: bool, pub reason: String }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadSource { pub source_index: usize, pub record_id: String, pub title: String, pub at_seconds: Option<u32> }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSuggestion { pub label: String, pub prompt: String }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceOption { pub id: String, pub label: String, pub detail: String }

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceDraft { prompt: String, why: String, options: Vec<ChoiceOption>, recommended: Option<String> }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingChoice {
    pub id: String, pub message_id: String, pub prompt: String, pub why: String,
    pub options: Vec<ChoiceOption>, pub recommended: Option<String>, pub status: String,
    pub selected_id: Option<String>, pub custom_text: Option<String>, pub answered_at: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingLibrarySearch {
    pub id: String, pub message_id: String, pub question: String,
    pub records: Vec<crate::db::LibraryTitle>, pub status: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingChatNoteEdit {
    pub message_id: String, pub note_title: String, pub proposal: workflow::NoteEditProposal,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelSuggestions { suggestions: Vec<ChatSuggestion> }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub created_at: String,
    pub paragraphs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<ChatCitation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<ChatFrame>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<ChatRun>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<ChatSuggestion>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default = "default_project_id")]
    pub project_id: String,
    #[serde(default)]
    pub working_context: WorkingContext,
    #[serde(default)]
    pub compacted_summary: String,
    #[serde(default)]
    pub compacted_count: usize,
    #[serde(default)]
    pub compacted_at: String,
    #[serde(default)]
    pub referenced_ids: Vec<String>,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_choice: Option<PendingChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_library_search: Option<PendingLibrarySearch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_chat_note_edit: Option<PendingChatNoteEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_command: Option<PendingCommand>,
}

fn default_project_id() -> String { "project-default".into() }

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingContext { pub goal: String, pub completed: String, pub next_step: String, pub updated_at: String }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMemory { pub id: String, pub content: String, pub source: String, pub created_at: String, pub confirmed_at: String, #[serde(default)] pub expires_at: Option<String>, #[serde(default)] pub active: bool }

pub fn memory_is_current(entry: &ProjectMemory) -> bool {
    if !entry.active { return false; }
    let Some(expires) = &entry.expires_at else { return true; };
    let Ok(limit) = expires.parse::<u128>() else { return false; };
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|now| now.as_millis() < limit).unwrap_or(false)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatStore { pub(crate) active_id: String, pub(crate) sessions: Vec<ChatSession>, #[serde(default)] pub(crate) project_memory: Vec<ProjectMemory> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionSummary { id: String, title: String, updated_at: String, message_count: usize }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatState { active_id: String, sessions: Vec<ChatSessionSummary>, messages: Vec<ChatMessage>, referenced_ids: Vec<String>, working_context: WorkingContext, project_memory: Vec<ProjectMemory>, compacted_summary: String, compacted_count: usize, pending_choice: Option<PendingChoice>, pending_library_search: Option<PendingLibrarySearch>, pending_chat_note_edit: Option<PendingChatNoteEdit>, pending_command: Option<PendingCommand> }

fn now_millis() -> Result<String, String> {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis().to_string()).map_err(|_| "系统时间无效".into())
}

fn new_session() -> Result<ChatSession, String> {
    let now = now_millis()?;
    Ok(ChatSession { id: format!("chat-{}", Uuid::new_v4()), title: "新会话".into(), created_at: now.clone(), updated_at: now.clone(), project_id: default_project_id(), working_context: WorkingContext::default(), compacted_summary: String::new(), compacted_count: 0, compacted_at: String::new(), referenced_ids: Vec::new(), messages: Vec::new(), pending_choice: None, pending_library_search: None, pending_chat_note_edit: None, pending_command: None })
}

fn session_title(question: &str) -> String { question.chars().take(28).collect() }

pub(crate) fn state_view(store: &ChatStore) -> Result<ChatState, String> {
    let active = store.sessions.iter().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
    let mut sessions = store.sessions.iter().map(|item| ChatSessionSummary {
        id: item.id.clone(), title: item.title.clone(), updated_at: item.updated_at.clone(), message_count: item.messages.len(),
    }).collect::<Vec<_>>();
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(ChatState { active_id: store.active_id.clone(), sessions, messages: active.messages.clone(), referenced_ids: active.referenced_ids.clone(), working_context: active.working_context.clone(), project_memory: store.project_memory.iter().filter(|entry| entry.active).cloned().collect(), compacted_summary: active.compacted_summary.clone(), compacted_count: active.compacted_count, pending_choice: active.pending_choice.clone(), pending_library_search: active.pending_library_search.clone(), pending_chat_note_edit: active.pending_chat_note_edit.clone(), pending_command: active.pending_command.clone() })
}

fn load_chat_store(path: &PathBuf, legacy_path: &PathBuf) -> Result<ChatStore, Box<dyn std::error::Error>> {
    if path.extension().is_some_and(|value| value == "db") {
        if let Some(store) = crate::db::read_chat(path).map_err(std::io::Error::other)? { return Ok(store); }
        let old_path = path.parent().ok_or("数据库路径无效")?.join("chat-sessions.json");
        let store = load_chat_store(&old_path, legacy_path)?;
        crate::db::write_chat(path, &store).map_err(std::io::Error::other)?;
        return Ok(store);
    }
    let backup = path.with_extension("json.bak");
    if !path.exists() && backup.exists() { fs::rename(&backup, path)?; }
    let store = if path.exists() {
        serde_json::from_slice::<ChatStore>(&fs::read(path)?)?
    } else {
        let mut first = new_session()?;
        let legacy_backup = legacy_path.with_extension("json.bak");
        if !legacy_path.exists() && legacy_backup.exists() { fs::rename(&legacy_backup, legacy_path)?; }
        if legacy_path.exists() {
            first.messages = serde_json::from_slice(&fs::read(legacy_path)?)?;
            if let Some(message) = first.messages.iter().find(|item| item.role == "user") {
                if let Some(question) = message.paragraphs.first() { first.title = session_title(question); }
            }
        }
        let store = ChatStore { active_id: first.id.clone(), sessions: vec![first], project_memory: Vec::new() };
        persist_chat_store(path, &store)?;
        store
    };
    if store.sessions.is_empty() || !store.sessions.iter().any(|item| item.id == store.active_id) {
        return Err("会话文件缺少当前会话".into());
    }
    Ok(store)
}
