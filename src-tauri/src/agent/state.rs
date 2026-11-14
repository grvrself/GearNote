// Agent 配置、人格与状态读取。
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentConfig { base_url: String, model: String, #[serde(default = "default_explore_references")] explore_references: bool, #[serde(default)] context_tier: ContextTier }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextTier { #[default] Small, Medium }

impl ContextTier {
    fn char_limit(self) -> usize { match self { Self::Small => 24_000, Self::Medium => 48_000 } }
}

fn default_explore_references() -> bool { true }

impl Default for AgentConfig {
    fn default() -> Self { Self { base_url: "https://api.openai.com/v1".into(), model: String::new(), explore_references: true, context_tier: ContextTier::Small } }
}

pub struct AgentState {
    config: Mutex<AgentConfig>,
    key: Mutex<Option<String>>,
    key_warning: Mutex<Option<String>>,
    pub(crate) sessions: Mutex<ChatStore>,
    persona: Mutex<String>,
    pub(crate) busy: Mutex<bool>,
    auto_compact_failures: Mutex<HashMap<String, usize>>,
    config_path: PathBuf,
    key_path: PathBuf,
    pub(crate) sessions_path: PathBuf,
    persona_path: PathBuf,
}

const DEFAULT_PERSONA: &str = include_str!("../../default-persona.md");

impl AgentState {
    pub fn load(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = app.path().app_data_dir()?;
        fs::create_dir_all(&dir)?;
        let config_path = dir.join("agent-config.json");
        let key_path = dir.join("agent-key.bin");
        let legacy_messages_path = dir.join("chat.json");
        let sessions_path = dir.join(crate::db::FILE_NAME);
        let persona_path = dir.join("persona.md");
        let config = if config_path.exists() { serde_json::from_slice(&fs::read(&config_path)?)? } else { AgentConfig::default() };
        let (key, key_warning) = match crate::secret::load(&key_path) {
            Ok(key) => (key, None),
            Err(_) => (None, Some("保存的密钥无法读取，请重新填写并保存".into())),
        };
        let sessions = load_chat_store(&sessions_path, &legacy_messages_path)?;
        if !persona_path.exists() { fs::write(&persona_path, DEFAULT_PERSONA)?; }
        let persona = fs::read_to_string(&persona_path)?;
        let persona = if valid_persona(&persona) { persona } else { DEFAULT_PERSONA.into() };
        Ok(Self { config: Mutex::new(config), key: Mutex::new(key), key_warning: Mutex::new(key_warning), sessions: Mutex::new(sessions), persona: Mutex::new(persona), busy: Mutex::new(false), auto_compact_failures: Mutex::new(HashMap::new()), config_path, key_path, sessions_path, persona_path })
    }
}

fn valid_persona(content: &str) -> bool {
    !content.trim().is_empty() && content.chars().count() <= 12_000 && !content.contains('\0')
}

fn persist_persona(path: &PathBuf, content: &str) -> Result<(), String> {
    let temp = path.with_extension("md.tmp");
    let backup = path.with_extension("md.bak");
    let mut file = fs::File::create(&temp).map_err(|_| "人格文件保存失败")?;
    file.write_all(content.as_bytes()).map_err(|_| "人格文件保存失败")?;
    file.sync_all().map_err(|_| "人格文件保存失败")?;
    drop(file);
    if backup.exists() { fs::remove_file(&backup).map_err(|_| "人格文件备份清理失败")?; }
    let previous = path.exists();
    if previous { fs::rename(path, &backup).map_err(|_| "人格文件备份失败")?; }
    if fs::rename(&temp, path).is_err() {
        if previous { let _ = fs::rename(&backup, path); }
        return Err("人格文件保存失败".into());
    }
    if previous { let _ = fs::remove_file(backup); }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaInfo { content: String, path: String, is_default: bool }

fn persona_info(state: &AgentState) -> Result<PersonaInfo, String> {
    let content = state.persona.lock().map_err(|_| "人格文件暂不可用")?.clone();
    Ok(PersonaInfo { is_default: content == DEFAULT_PERSONA, content, path: state.persona_path.to_string_lossy().into_owned() })
}

#[tauri::command]
pub fn get_persona(state: State<'_, AgentState>) -> Result<PersonaInfo, String> { persona_info(&state) }

#[tauri::command]
pub fn save_persona(state: State<'_, AgentState>, content: String) -> Result<PersonaInfo, String> {
    if !valid_persona(&content) { return Err("人格内容不能为空、超过 12000 字或包含无效字符".into()); }
    persist_persona(&state.persona_path, &content)?;
    *state.persona.lock().map_err(|_| "人格文件暂不可用")? = content;
    persona_info(&state)
}

#[tauri::command]
pub fn get_default_persona() -> String { DEFAULT_PERSONA.into() }

#[tauri::command]
pub fn reload_persona(state: State<'_, AgentState>) -> Result<PersonaInfo, String> {
    let content = fs::read_to_string(&state.persona_path).map_err(|_| "人格文件读取失败")?;
    if !valid_persona(&content) { return Err("人格文件为空、超过 12000 字或包含无效字符".into()); }
    *state.persona.lock().map_err(|_| "人格文件暂不可用")? = content;
    persona_info(&state)
}

fn persist_chat_store(path: &PathBuf, store: &ChatStore) -> Result<(), String> {
    if path.extension().is_some_and(|value| value == "db") { return crate::db::write_chat(path, store); }
    let encoded = serde_json::to_vec_pretty(store).map_err(|_| "会话无法编码")?;
    let temp = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    let mut file = fs::File::create(&temp).map_err(|_| "对话记录保存失败")?;
    file.write_all(&encoded).map_err(|_| "会话保存失败")?;
    file.sync_all().map_err(|_| "会话保存失败")?;
    drop(file);
    if backup.exists() { fs::remove_file(&backup).map_err(|_| "会话备份清理失败")?; }
    let previous = path.exists();
    if previous { fs::rename(path, &backup).map_err(|_| "会话备份失败")?; }
    if fs::rename(&temp, path).is_err() {
        if previous { let _ = fs::rename(&backup, path); }
        return Err("会话保存失败".into());
    }
    if previous { let _ = fs::remove_file(backup); }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus { base_url: String, model: String, explore_references: bool, context_tier: ContextTier, has_key: bool, key_persistent: bool, key_warning: Option<String> }

#[tauri::command]
pub fn get_agent_status(state: State<'_, AgentState>) -> Result<AgentStatus, String> {
    let config = state.config.lock().map_err(|_| "模型配置暂不可用")?.clone();
    let has_key = state.key.lock().map_err(|_| "模型密钥暂不可用")?.is_some();
    let key_warning = state.key_warning.lock().map_err(|_| "密钥状态暂不可用")?.clone();
    Ok(AgentStatus { base_url: config.base_url, model: config.model, explore_references: config.explore_references, context_tier: config.context_tier, has_key, key_persistent: cfg!(windows), key_warning })
}

#[tauri::command]
pub fn save_agent_config(state: State<'_, AgentState>, base_url: String, model: String, api_key: Option<String>) -> Result<AgentStatus, String> {
    let url = Url::parse(base_url.trim()).map_err(|_| "API 地址无效")?;
    let localhost = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.scheme() != "https" && !(localhost && url.scheme() == "http") {
        return Err("云端 API 必须使用 HTTPS；本机地址可以使用 HTTP".into());
    }
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("API 地址不能包含账号、参数或片段".into());
    }
    let model = model.trim();
    if model.is_empty() || model.len() > 100 || model.chars().any(char::is_whitespace) { return Err("请输入有效的模型名称".into()); }
    if let Some(ref key) = api_key {
        if !key.is_empty() && (key.trim() != key || key.len() > 4096 || key.chars().any(char::is_control)) { return Err("API 密钥格式无效".into()); }
    }
    let mut saved_config = state.config.lock().map_err(|_| "模型配置暂不可用")?;
    let config = AgentConfig { base_url: url.as_str().trim_end_matches('/').into(), model: model.into(), explore_references: saved_config.explore_references, context_tier: saved_config.context_tier };
    let data = serde_json::to_vec_pretty(&config).map_err(|_| "模型配置无法编码")?;
    if let Some(ref key) = api_key {
        crate::secret::save(&state.key_path, key)?;
    }
    fs::write(&state.config_path, data).map_err(|_| "模型配置保存失败")?;
    *saved_config = config;
    drop(saved_config);
    if let Some(key) = api_key {
        *state.key.lock().map_err(|_| "模型密钥暂不可用")? = if key.is_empty() { None } else { Some(key) };
        *state.key_warning.lock().map_err(|_| "密钥状态暂不可用")? = None;
    }
    get_agent_status(state)
}

#[tauri::command]
pub fn save_explore_references(state: State<'_, AgentState>, enabled: bool) -> Result<AgentStatus, String> {
    let mut saved_config = state.config.lock().map_err(|_| "模型配置暂不可用")?;
    let mut next = saved_config.clone();
    next.explore_references = enabled;
    let data = serde_json::to_vec_pretty(&next).map_err(|_| "资料探索设置无法编码")?;
    fs::write(&state.config_path, data).map_err(|error| format!("资料探索设置保存失败：{error}"))?;
    *saved_config = next;
    drop(saved_config);
    get_agent_status(state)
}

#[tauri::command]
pub fn save_context_tier(state: State<'_, AgentState>, tier: ContextTier) -> Result<AgentStatus, String> {
    let mut saved = state.config.lock().map_err(|_| "模型配置暂不可用")?;
    let mut next = saved.clone();
    next.context_tier = tier;
    let data = serde_json::to_vec_pretty(&next).map_err(|_| "上下文档位无法编码")?;
    fs::write(&state.config_path, data).map_err(|error| format!("上下文档位保存失败：{error}"))?;
    *saved = next;
    drop(saved);
    get_agent_status(state)
}

#[tauri::command]
pub fn list_chat_messages(state: State<'_, AgentState>) -> Result<Vec<ChatMessage>, String> {
    let store = state.sessions.lock().map_err(|_| "会话暂不可用")?;
    Ok(state_view(&store)?.messages)
}

#[tauri::command]
pub fn get_chat_state(state: State<'_, AgentState>) -> Result<ChatState, String> {
    let store = state.sessions.lock().map_err(|_| "会话暂不可用")?;
    state_view(&store)
}

#[tauri::command]
pub fn get_chat_frame_image(app: AppHandle, state: State<'_, AgentState>, image_id: String) -> Result<String, String> {
    Uuid::parse_str(&image_id).map_err(|_| "截图编号无效")?;
    let store = state.sessions.lock().map_err(|_| "会话暂不可用")?;
    let session = store.sessions.iter().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
    if !session.messages.iter().any(|message| message.frame.as_ref().is_some_and(|frame| frame.image_id == image_id)) {
        return Err("当前会话没有这张截图".into());
    }
    drop(store);
    let path = app.path().app_data_dir().map_err(|error| error.to_string())?
        .join("chat-frames").join(format!("{image_id}.jpg"));
    let bytes = fs::read(path).map_err(|_| "截图文件已丢失")?;
    if bytes.is_empty() || bytes.len() > 8_000_000 { return Err("截图文件无效".into()); }
    Ok(format!("data:image/jpeg;base64,{}", crate::storyboard::base64(&bytes)))
}
