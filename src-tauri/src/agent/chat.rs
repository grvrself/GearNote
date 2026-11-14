// 会话操作与工作记忆。
fn capture_time(question: &str) -> Option<Result<u32, String>> {
    let text = question.trim().trim_start_matches('请').trim_start_matches("帮我").trim_start_matches("给我").trim();
    if !text.starts_with("截取") && !text.starts_with("截图") && !text.starts_with("/截图") { return None; }
    let stamps = text.split(|ch: char| !ch.is_ascii_digit() && ch != ':')
        .filter(|part| part.contains(':')).collect::<Vec<_>>();
    if stamps.len() != 1 { return Some(Err("请只指定一个时间点，例如：截取 01:20 的画面。".into())); }
    let parts = stamps[0].split(':').collect::<Vec<_>>();
    if !(parts.len() == 2 || parts.len() == 3) || parts.iter().any(|part| part.is_empty() || part.len() > 2) {
        return Some(Err("时间格式应为 01:20 或 01:02:20。".into()));
    }
    let parsed = parts.iter().map(|part| part.parse::<u32>()).collect::<Result<Vec<_>, _>>();
    let Ok(parts) = parsed else { return Some(Err("时间格式无效。".into())); };
    let seconds = if parts.len() == 2 {
        if parts[1] >= 60 { return Some(Err("秒数应小于 60。".into())); }
        parts[0] * 60 + parts[1]
    } else {
        if parts[1] >= 60 || parts[2] >= 60 { return Some(Err("分和秒均应小于 60。".into())); }
        parts[0] * 3600 + parts[1] * 60 + parts[2]
    };
    if seconds > 864_000 { return Some(Err("时间点超出允许范围。".into())); }
    Some(Ok(seconds))
}

fn mutate_chat<F>(app: &AppHandle, state: &AgentState, require_idle: bool, notify: bool, edit: F) -> Result<ChatState, String>
where F: FnOnce(&mut ChatStore) -> Result<bool, String> {
    let busy_guard = if require_idle { Some(state.busy.lock().map_err(|_| "Agent 暂不可用")?) } else { None };
    if busy_guard.as_deref() == Some(&true) {
        return Err("正在生成回答，请完成后再切换会话".into());
    }
    let mut store = state.sessions.lock().map_err(|_| "会话暂不可用")?;
    let mut next = store.clone();
    let changed = edit(&mut next)?;
    if changed {
        persist_chat_store(&state.sessions_path, &next)?;
        *store = next;
    }
    let view = state_view(&store)?;
    drop(store);
    drop(busy_guard);
    if changed && notify { let _ = app.emit("knowflow:chat-changed", ()); }
    Ok(view)
}

pub(crate) fn record_completed_event(app: &AppHandle, description: &str, event_id: &str) -> Result<(), String> {
    let state = app.state::<AgentState>();
    let timestamp = now_millis()?;
    let marker = format!("[操作:{event_id}]");
    mutate_chat(app, &state, false, true, |store| {
        let session = store.sessions.iter_mut().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
        if session.working_context.goal.trim().is_empty() || session.working_context.completed.contains(&marker) { return Ok(false); }
        let line = format!("{marker} {description}（{timestamp}）");
        let combined = if session.working_context.completed.trim().is_empty() { line } else { format!("{}\n{line}", session.working_context.completed) };
        session.working_context.completed = combined.chars().rev().take(1200).collect::<String>().chars().rev().collect();
        session.working_context.updated_at = timestamp;
        Ok(true)
    })?;
    Ok(())
}

#[tauri::command]
pub fn create_chat_session(app: AppHandle, state: State<'_, AgentState>) -> Result<ChatState, String> {
    mutate_chat(&app, &state, true, true, |store| {
        let session = new_session()?;
        store.active_id = session.id.clone();
        store.sessions.push(session);
        Ok(true)
    })
}

#[tauri::command]
pub fn switch_chat_session(app: AppHandle, state: State<'_, AgentState>, id: String) -> Result<ChatState, String> {
    mutate_chat(&app, &state, true, true, |store| {
        if !store.sessions.iter().any(|item| item.id == id) { return Err("会话不存在".into()); }
        if store.active_id == id { return Ok(false); }
        store.active_id = id;
        Ok(true)
    })
}

#[tauri::command]
pub fn rename_chat_session(app: AppHandle, state: State<'_, AgentState>, id: String, title: String) -> Result<ChatState, String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 80 || title.chars().any(char::is_control) {
        return Err("会话名称不能为空或超过 80 字".into());
    }
    mutate_chat(&app, &state, false, true, |store| {
        let session = store.sessions.iter_mut().find(|item| item.id == id).ok_or("会话不存在")?;
        if session.title == title { return Ok(false); }
        session.title = title.into();
        Ok(true)
    })
}

#[tauri::command]
pub fn delete_chat_session(app: AppHandle, state: State<'_, AgentState>, id: String) -> Result<ChatState, String> {
    let images = {
        let store = state.sessions.lock().map_err(|_| "会话暂不可用")?;
        store.sessions.iter().find(|session| session.id == id).ok_or("会话不存在")?
            .messages.iter().filter_map(|message| message.frame.as_ref().map(|frame| frame.image_id.clone())).collect::<Vec<_>>()
    };
    let view = mutate_chat(&app, &state, true, true, |store| {
        let index = store.sessions.iter().position(|item| item.id == id).ok_or("会话不存在")?;
        store.sessions.remove(index);
        if store.sessions.is_empty() { store.sessions.push(new_session()?); }
        if store.active_id == id {
            store.active_id = store.sessions.iter().max_by(|a, b| a.updated_at.cmp(&b.updated_at)).unwrap().id.clone();
        }
        Ok(true)
    })?;
    if let Ok(dir) = app.path().app_data_dir() {
        for image_id in images {
            if Uuid::parse_str(&image_id).is_ok() { let _ = fs::remove_file(dir.join("chat-frames").join(format!("{image_id}.jpg"))); }
        }
    }
    Ok(view)
}

#[tauri::command]
pub fn set_chat_references(app: AppHandle, state: State<'_, AgentState>, id: String, referenced_ids: Vec<String>) -> Result<ChatState, String> {
    if referenced_ids.len() > 20 || referenced_ids.iter().any(|item| item.len() > 100) {
        return Err("每个会话最多引用 20 条资料".into());
    }
    let mut seen = HashSet::new();
    let unique = referenced_ids.into_iter().filter(|item| seen.insert(item.clone())).collect::<Vec<_>>();
    mutate_chat(&app, &state, false, false, |store| {
        let Some(session) = store.sessions.iter_mut().find(|item| item.id == id) else { return Ok(false); };
        if session.referenced_ids == unique { return Ok(false); }
        session.referenced_ids = unique;
        Ok(true)
    })
}

fn resolve_library_search_in_session(session: &mut ChatSession, proposal_id: &str, accept: bool, now: &str) -> Result<(), String> {
    let proposal = session.pending_library_search.as_mut()
        .filter(|proposal| proposal.id == proposal_id && proposal.status == "open")
        .ok_or("资料搜索提案已失效")?;
    if accept {
        let mut next = session.referenced_ids.clone();
        for record in &proposal.records {
            if !next.contains(&record.id) { next.push(record.id.clone()); }
        }
        if next.len() > 20 { return Err("本轮参考超过 20 条，请先移除其他资料".into()); }
        session.referenced_ids = next;
        proposal.status = "accepted".into();
    } else { proposal.status = "dismissed".into(); }
    session.updated_at = now.into();
    Ok(())
}

#[tauri::command]
pub fn resolve_library_search(app: AppHandle, state: State<'_, AgentState>, proposal_id: String, accept: bool) -> Result<ChatState, String> {
    let ids = {
        let store = state.sessions.lock().map_err(|_| "会话暂不可用")?;
        let session = store.sessions.iter().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
        let proposal = session.pending_library_search.as_ref()
            .filter(|item| item.id == proposal_id && item.status == "open").ok_or("资料搜索提案已失效")?;
        proposal.records.iter().map(|record| record.id.clone()).collect::<Vec<_>>()
    };
    if accept { crate::db::ready_library_titles(&state.sessions_path, &ids)?; }
    let now = now_millis()?;
    mutate_chat(&app, &state, true, true, |store| {
        let session = store.sessions.iter_mut().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
        resolve_library_search_in_session(session, &proposal_id, accept, &now)?;
        Ok(true)
    })
}

#[tauri::command]
pub fn save_working_context(app: AppHandle, state: State<'_, AgentState>, goal: String, completed: String, next_step: String) -> Result<ChatState, String> {
    for value in [&goal, &completed, &next_step] {
        if value.chars().count() > 1200 || value.contains('\0') { return Err("工作状态每项最多 1200 字".into()); }
    }
    mutate_chat(&app, &state, true, true, |store| {
        let session = store.sessions.iter_mut().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
        session.working_context = WorkingContext { goal, completed, next_step, updated_at: now_millis()? };
        Ok(true)
    })
}

#[tauri::command]
pub fn confirm_project_memory(app: AppHandle, state: State<'_, AgentState>, content: String, source: String, expires_at: Option<String>) -> Result<ChatState, String> {
    let content = content.trim();
    if content.is_empty() || content.chars().count() > 1000 || content.contains('\0') { return Err("项目记忆需为 1–1000 字".into()); }
    if source.chars().count() > 500 { return Err("来源说明最多 500 字".into()); }
    if expires_at.as_ref().is_some_and(|value| value.parse::<u128>().is_err()) { return Err("到期时间无效".into()); }
    mutate_chat(&app, &state, true, true, |store| {
        let now = now_millis()?;
        store.project_memory.push(ProjectMemory { id: format!("mem-{}", Uuid::new_v4()), content: content.into(), source, created_at: now.clone(), confirmed_at: now, expires_at, active: true });
        Ok(true)
    })
}

#[tauri::command]
pub fn revoke_project_memory(app: AppHandle, state: State<'_, AgentState>, id: String) -> Result<ChatState, String> {
    mutate_chat(&app, &state, true, true, |store| {
        let entry = store.project_memory.iter_mut().find(|entry| entry.id == id).ok_or("项目记忆不存在")?;
        entry.active = false;
        Ok(true)
    })
}

struct BusyGuard<'a>(&'a Mutex<bool>);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) { if let Ok(mut busy) = self.0.lock() { *busy = false; } }
}
