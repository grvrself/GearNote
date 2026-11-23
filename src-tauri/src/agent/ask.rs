// Agent 问答入口与上下文预览。
fn validate_deep_video<'a>(records: &'a [KnowledgeRecord], ids: &[String], has_image: bool, has_messages: bool, has_open_choice: bool) -> Result<&'a KnowledgeRecord, String> {
    if ids.len() != 1 || has_image || has_messages || has_open_choice {
        return Err("深度模式只支持本轮选中一条视频且不附加图片或对话消息".into());
    }
    let record = records.iter().find(|record| record.id == ids[0]).ok_or("深度模式的视频不在本轮参考中")?;
    if record.record_type != "video" || record.status != crate::ProcessStatus::Ready || record.transcript.as_ref().is_none_or(Vec::is_empty) {
        return Err("深度模式需要一条已完成字幕提取的视频".into());
    }
    Ok(record)
}

#[tauri::command]
pub fn preview_agent_context(app: AppHandle, agent: State<'_, AgentState>, storage: State<'_, AppState>, question: String, referenced_ids: Vec<String>, frame_id: Option<String>, message_ids: Vec<String>, deep_mode: bool) -> Result<ContextPreview, String> {
    let question = question.trim();
    if question.is_empty() || question.chars().count() > 2000 { return Err("请先输入不超过 2000 字的问题".into()); }
    if referenced_ids.len() > 20 { return Err("每次最多引用 20 条资料".into()); }
    let persona = agent.persona.lock().map_err(|_| "人格文件暂不可用")?.clone();
    let mut config = agent.config.lock().map_err(|_| "模型配置暂不可用")?.clone();
    config.reading_budget = config.reading_budget.effective();
    let explore = config.explore_references;
    let (messages, selected, image_frame, project_memory, working, summary, open_choice) = {
        let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let session = store.sessions.iter().find(|item| item.id == store.active_id).ok_or("当前会话不存在")?;
        let frame = if let Some(image_id) = frame_id {
            Uuid::parse_str(&image_id).map_err(|_| "截图编号无效")?;
            Some(session.messages.iter().find_map(|message| message.frame.as_ref().filter(|frame| frame.image_id == image_id).cloned())
                .ok_or("当前会话没有这张截图")?)
        } else { None };
        (session.messages[session.compacted_count.min(session.messages.len())..].to_vec(), selected_messages(session, &message_ids)?, frame, store.project_memory.iter().filter(|entry| memory_is_current(entry)).cloned().collect::<Vec<_>>(), session.working_context.clone(), session.compacted_summary.clone(), session.pending_choice.as_ref().filter(|choice| choice.status == "open").cloned())
    };
    let records = storage.records.lock().map_err(|_| "资料库暂不可用")?.iter()
        .filter(|record| referenced_ids.contains(&record.id) || image_frame.as_ref().is_some_and(|frame| frame.record_id == record.id))
        .cloned().collect::<Vec<_>>();
    let image = if let Some(frame) = image_frame {
        let path = app.path().app_data_dir().map_err(|error| error.to_string())?
            .join("chat-frames").join(format!("{}.jpg", frame.image_id));
        if !path.is_file() { return Err("截图文件已丢失".into()); }
        let title = records.iter().find(|record| record.id == frame.record_id)
            .map(|record| record.title.as_str()).unwrap_or("视频截图");
        Some(SourceChunk::image(frame.record_id, title.into(), frame.at_seconds))
    } else { None };
    let deep_mode = deep_mode || (config.explore_references && config.reading_budget.strategy == ReadingStrategy::Full && referenced_ids.len() == 1 && records.first().is_some_and(|record| record.record_type == "video") && image.is_none() && selected.is_empty() && open_choice.is_none());
    let deep_record = if deep_mode { Some(validate_deep_video(&records, &referenced_ids, image.is_some(), !selected.is_empty(), open_choice.is_some())?) } else { None };
    let general_chat = referenced_ids.is_empty() && selected.is_empty() && image.is_none();
    let semantic_pending = config.semantic_retrieval && !deep_mode && image.is_none()
        && selected.is_empty() && !referenced_ids.is_empty();
    let index = (explore && !deep_mode && image.is_none() && selected.is_empty() && !referenced_ids.is_empty()).then(|| if config.reading_budget.strategy == ReadingStrategy::Full { ReadIndex::for_full_read(&records, &referenced_ids) } else { ReadIndex::new(&records, &referenced_ids) });
    let mut prepared = if let Some(record) = deep_record {
        let batch_count = deep_video::batches(record).len();
        context::build_exploration_limit(&persona, question, &messages, 1,
            format!("深度模式：仅顺序读取《{}》的字幕；共约 {batch_count} 批，每批最多 3000 字；本次预算 {} 批或 {} 秒。正文将在发送后逐批读取；未读字幕不会作为事实依据。", record.title, config.reading_budget.deep_batches, config.reading_budget.deep_seconds),
            batch_count, &project_memory, &working, &summary, config.context_tier.char_limit())
    } else if let Some(index) = index.as_ref().filter(|index| !index.is_empty()) {
        context::build_exploration_limit(&persona, question, &messages, referenced_ids.len(),
            index.directory(&records, &referenced_ids, 6_000), index.len(), &project_memory, &working, &summary, config.context_tier.char_limit())
    } else {
        context::build_with_message_refs_limit(&persona, question, &messages, &selected, &records, &referenced_ids, image,
            &project_memory, &working, &summary, 0, config.context_tier.char_limit())
    };
    if general_chat { context::enable_general_chat(&mut prepared, &persona); }
    if let Some(choice) = &open_choice {
        let addition = format!("\n\n{}", choice_system_instruction(choice, question, None));
        prepared.preview.system_text.push_str(&addition);
        prepared.preview.text_chars += addition.chars().count();
    }
    prepared.preview.semantic_pending = semantic_pending;
    Ok(prepared.preview)
}

struct GeneratedAnswer { text: String, warning: Option<String> }

async fn generate_final_answer(app: &AppHandle, client: &reqwest::Client, config: &AgentConfig, key: &str,
    request_id: &str, messages: &[serde_json::Value], explored: bool, has_attachment: bool) -> Result<GeneratedAnswer, String> {
    let mut response = client.post(format!("{}/chat/completions", config.base_url)).bearer_auth(key)
        .json(&answer_payload(config, messages, explored))
        .send().await.map_err(|error| format!("模型请求失败：{error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let detail = provider_error(status, &body, &config.base_url, &config.model);
        if has_attachment && status == reqwest::StatusCode::BAD_REQUEST {
            return Err(format!("图片提问被服务拒绝；请确认当前模型支持视觉输入。{detail}"));
        }
        return Err(detail);
    }
    let mut parser = SseParser::default();
    let mut answer = String::new();
    let mut warning = None;
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) if !answer.trim().is_empty() => {
                warning = Some(format!("流式连接中断：{error}；已保留收到的正文"));
                break;
            }
            Err(error) => return Err(format!("模型流式连接中断：{error}")),
        };
        let deltas = match parser.push(&chunk) {
            Ok(deltas) => deltas,
            Err(error) if !answer.trim().is_empty() => { warning = Some(format!("流式格式错误：{error}；已保留收到的正文")); break; }
            Err(error) => return Err(error),
        };
        if let Err(error) = publish_deltas(app, request_id, &mut answer, deltas, true, config.reading_budget.answer_chars) {
            if answer.trim().is_empty() { return Err(error); }
            warning = Some(format!("回答超过显示上限：{error}；已保留收到的正文"));
            break;
        }
        if parser.done { break; }
    }
    if warning.is_none() {
        match parser.push(b"\n\n") {
            Ok(tail) => {
                if let Err(error) = publish_deltas(app, request_id, &mut answer, tail, true, config.reading_budget.answer_chars) {
                    if answer.trim().is_empty() { return Err(error); }
                    warning = Some(format!("回答超过显示上限：{error}；已保留收到的正文"));
                }
            }
            Err(error) if !answer.trim().is_empty() => warning = Some(format!("流式格式错误：{error}；已保留收到的正文")),
            Err(error) => return Err(error),
        }
    }
    if !parser.done && parser.finish_reason.is_none() && warning.is_none() {
        warning = Some("模型流式连接提前结束；已保留收到的正文".into());
    }
    if let Some(reason) = parser.finish_reason.as_deref() {
        if reason != "stop" { warning = Some(format!("模型结束原因是 {reason}；已保留收到的正文")); }
    }
    let answer = answer.trim();
    if answer.is_empty() { return Err("模型返回了空回答".into()); }
    let text = answer.chars().take(config.reading_budget.answer_chars).collect();
    Ok(GeneratedAnswer { text, warning })
}


fn save_final_failure_exchange(session: &mut ChatSession, question: &str, error: &str, now: &str, elapsed_ms: u64,
    frame: Option<ChatFrame>, mut steps: Vec<String>, read_sources: Vec<ReadSource>, read_coverage: Option<ReadCoverage>,
    choice_id: Option<&str>) -> ChatMessage {
    let detail = error.chars().filter(|ch| !ch.is_control()).take(350).collect::<String>();
    let user = ChatMessage { id: format!("msg-{}", Uuid::new_v4()), role: "user".into(), created_at: now.into(),
        paragraphs: vec![question.into()], tool_steps: Vec::new(), citations: Vec::new(), frame, run: None, suggestions: Vec::new() };
    steps.push(format!("回答未完成：{detail}"));
    let assistant = ChatMessage { id: format!("msg-{}", Uuid::new_v4()), role: "agent".into(), created_at: now.into(),
        paragraphs: vec![format!("本轮未收到可显示的模型正文。原因：{detail}")], tool_steps: steps,
        citations: Vec::new(), frame: None,
        run: Some(ChatRun { elapsed_ms, changed_files: Vec::new(), read_sources, read_coverage }), suggestions: Vec::new() };
    answer_pending_choice(session, question, now, choice_id);
    session.messages.push(user);
    session.messages.push(assistant.clone());
    session.updated_at = now.into();
    assistant
}

#[tauri::command]
pub async fn ask_agent(app: AppHandle, agent: State<'_, AgentState>, storage: State<'_, AppState>, notes: State<'_, NotesState>, question: String, referenced_ids: Vec<String>, request_id: String, frame_id: Option<String>, message_ids: Vec<String>, choice_id: Option<String>, deep_mode: bool, continue_message_id: Option<String>) -> Result<ChatMessage, String> {
    let continuation = if let Some(id) = continue_message_id.as_deref() {
        if frame_id.is_some() || !message_ids.is_empty() || choice_id.is_some() { return Err("续读时请移除图片和对话参考".into()); }
        let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let session = store.sessions.iter().find(|item| item.id == store.active_id).ok_or("会话不存在")?;
        Some(continuation_for(session, id, &referenced_ids)?)
    } else { None };
    let deep_mode = continuation.as_ref().map(|value| value.progress.deep).unwrap_or(deep_mode);
    let question = continuation.as_ref().map(|value| value.question.clone()).unwrap_or(question);
    let question = question.trim();
    if question.is_empty() || question.chars().count() > 2000 { return Err("问题不能为空或超过 2000 字".into()); }
    if referenced_ids.len() > 20 { return Err("每次最多引用 20 条资料".into()); }
    if request_id.is_empty() || request_id.len() > 80 || !request_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
        return Err("请求编号无效".into());
    }
    {
        let mut busy = agent.busy.lock().map_err(|_| "Agent 暂不可用")?;
        if *busy { return Err("上一条提问仍在处理，请稍候".into()); }
        *busy = true;
    }
    let _guard = BusyGuard(&agent.busy);
    let started_at = Instant::now();
    let request_session_id = agent.sessions.lock().map_err(|_| "会话暂不可用")?.active_id.clone();
    if choice_id.is_some() {
        let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let session = store.sessions.iter().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
        validate_choice_submission(session, question, choice_id.as_deref())?;
    }
    if let Some(parsed) = capture_time(question) {
        let seconds = parsed?;
        let videos = {
            let records = storage.records.lock().map_err(|_| "资料库暂不可用")?;
            records.iter().filter(|record| referenced_ids.contains(&record.id) && record.record_type == "video")
                .cloned().collect::<Vec<_>>()
        };
        if videos.len() != 1 { return Err("请在本轮参考中只保留一条视频，再指定截图时间。".into()); }
        let video = &videos[0];
        if video.duration_seconds.is_some_and(|duration| seconds >= duration) {
            return Err("时间点已超出视频时长。".into());
        }
        let image_id = Uuid::new_v4().to_string();
        let image_path = crate::storyboard::capture_frame(&app, &video.source_url, seconds, &image_id).await?;
        let now = match now_millis() {
            Ok(now) => now,
            Err(error) => { let _ = fs::remove_file(image_path); return Err(error); }
        };
        let label = if seconds >= 3600 {
            format!("{:02}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
        } else { format!("{:02}:{:02}", seconds / 60, seconds % 60) };
        let user = ChatMessage { id: format!("msg-{}", Uuid::new_v4()), role: "user".into(), created_at: now.clone(),
            paragraphs: vec![question.into()], tool_steps: Vec::new(), citations: Vec::new(), frame: None, run: None, suggestions: Vec::new() };
        let assistant = ChatMessage { id: format!("msg-{}", Uuid::new_v4()), role: "agent".into(), created_at: now.clone(),
            paragraphs: vec![format!("已截取《{}》在 {label} 的画面。", video.title)],
            tool_steps: Vec::new(),
            citations: vec![ChatCitation { record_id: video.id.clone(), at_seconds: Some(seconds), source_index: None, label: Some("[画面]".into()), kind: Some("image".into()) }],
            frame: Some(ChatFrame { image_id, record_id: video.id.clone(), at_seconds: seconds }),
            run: Some(ChatRun { elapsed_ms: started_at.elapsed().as_millis().min(u64::MAX as u128) as u64, changed_files: Vec::new(), read_sources: Vec::new(), read_coverage: None }), suggestions: Vec::new() };
        let saved = (|| {
            let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
            let mut next = store.clone();
            if next.active_id != request_session_id { return Err("会话已切换，本轮截图未保存".into()); }
            let active = next.sessions.iter_mut().find(|item| item.id == next.active_id).ok_or("当前会话不存在")?;
            if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
            active.messages.push(user);
            active.messages.push(assistant.clone());
            active.updated_at = now.clone();
            persist_chat_store(&agent.sessions_path, &next)?;
            *store = next;
            Ok::<(), String>(())
        })();
        if let Err(error) = saved { let _ = fs::remove_file(image_path); return Err(error); }
        let _ = app.emit("knowflow:chat-changed", ());
        return Ok(assistant);
    }
    let mut config = agent.config.lock().map_err(|_| "模型配置暂不可用")?.clone();
    config.reading_budget = config.reading_budget.effective();
    let key = agent.key.lock().map_err(|_| "请先在设置页填写 API 密钥")?.clone().ok_or("请先在设置页填写 API 密钥")?;
    let persona = agent.persona.lock().map_err(|_| "人格文件暂不可用")?.clone();
    if config.model.is_empty() { return Err("请先在设置页填写模型名称".into()); }
    let mut confirmed_library_followup = false;
    if library_search_requested(question) && frame_id.is_none() && !deep_mode && choice_id.is_none() {
        let (already_confirmed, open_proposal) = {
            let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
            let session = store.sessions.iter().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
            let confirmed = session.pending_library_search.as_ref().is_some_and(|proposal|
                proposal.status == "accepted" && proposal.question == question && proposal.records.iter().all(|record| referenced_ids.contains(&record.id)));
            let open = session.pending_library_search.as_ref().is_some_and(|proposal| proposal.status == "open");
            (confirmed, open)
        };
        if open_proposal { return Err("请先确认或跳过上一次资料库搜索".into()); }
        confirmed_library_followup = already_confirmed;
        if !already_confirmed {
            let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(60))
                .build().map_err(|_| "无法初始化搜索连接")?;
            let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: "正在搜索整个资料库的标题…".into() });
            let query = propose_library_query(&client, &config, &key, question).await?;
            let records = crate::db::search_library_titles(&agent.sessions_path, &query, 8)?;
            let now = now_millis()?;
            let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
            let mut next = store.clone();
            if next.active_id != request_session_id { return Err("会话已切换，搜索结果未保存".into()); }
            let active = next.sessions.iter_mut().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
            if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
            let assistant = save_library_exchange(active, question, records, &now, started_at.elapsed().as_millis().min(u64::MAX as u128) as u64);
            persist_chat_store(&agent.sessions_path, &next)?;
            *store = next;
            drop(store);
            let _ = app.emit("knowflow:chat-changed", ());
            return Ok(assistant);
        }
    }
    if chat_note_edit_requested(question) && !library_search_requested(question) && !command_requested(question) && frame_id.is_none() && !deep_mode && choice_id.is_none() {
        {
            let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
            let session = store.sessions.iter().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
            if session.pending_chat_note_edit.as_ref().is_some_and(|item| item.proposal.status == "pending") {
                return Err("请先确认或放弃上一次笔记修改建议".into());
            }
        }
        let note = {
            let saved = notes.notes.lock().map_err(|_| "笔记暂不可用")?;
            select_chat_note(question, &saved)?.clone()
        };
        if note.body.chars().count() > 12_000 { return Err("这篇笔记超过 12000 字，暂不能生成修改建议".into()); }
        let evidence_required = note_evidence_requested(question);
        let records = storage.records.lock().map_err(|_| "资料库暂不可用")?.iter()
            .filter(|record| referenced_ids.contains(&record.id)).cloned().collect::<Vec<_>>();
        let (mut sources, _) = context::select_sources(&records, &referenced_ids, question);
        sources.truncate(8);
        for source in &mut sources { source.text = source.text.chars().take(1_200).collect(); }
        if evidence_required && sources.is_empty() { return Err("本轮没有已读资料，无法提出按资料修改的建议".into()); }
        let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(90))
            .build().map_err(|_| "无法初始化笔记建议连接")?;
        let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: "正在准备待确认的笔记修改建议…".into() });
        let args = propose_chat_note_args(&client, &config, &key, question, &note, &sources, evidence_required).await?;
        let (proposal, after) = workflow::parse_model_edit_with_sources(&args, &note, &sources, evidence_required)?;
        {
            let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
            if store.active_id != request_session_id { return Err("会话已切换，笔记建议未保存".into()); }
        }
        workflow::store_note_proposal(&notes.path, &note, &after, &proposal)?;
        let now = now_millis()?;
        let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let mut next = store.clone();
        if next.active_id != request_session_id { return Err("会话已切换，笔记建议未显示".into()); }
        let active = next.sessions.iter_mut().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
        if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
        let assistant = save_chat_note_exchange(active, question, &note.title, proposal, &sources, &now,
            started_at.elapsed().as_millis().min(u64::MAX as u128) as u64);
        persist_chat_store(&agent.sessions_path, &next)?;
        *store = next;
        drop(store);
        let _ = app.emit("knowflow:chat-changed", ());
        return Ok(assistant);
    }
    if command_requested(question) && !library_search_requested(question) && frame_id.is_none() && !deep_mode && choice_id.is_none() {
        {
            let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
            let session = store.sessions.iter().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
            if session.pending_command.as_ref().is_some_and(|proposal| matches!(proposal.status.as_str(), "open" | "running")) {
                return Err("请先确认或拒绝上一次命令提案".into());
            }
        }
        let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(60))
            .build().map_err(|_| "无法初始化命令提案连接")?;
        let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: "正在准备待确认的命令…".into() });
        let (program, args) = propose_command_args(&client, &config, &key, question).await?;
        let now = now_millis()?;
        let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let mut next = store.clone();
        if next.active_id != request_session_id { return Err("会话已切换，命令提案未保存".into()); }
        let active = next.sessions.iter_mut().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
        if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
        let assistant = save_command_exchange(active, question, program, args, &now,
            started_at.elapsed().as_millis().min(u64::MAX as u128) as u64)?;
        persist_chat_store(&agent.sessions_path, &next)?;
        *store = next;
        drop(store);
        let _ = app.emit("knowflow:chat-changed", ());
        return Ok(assistant);
    }
    let (needs_compaction, message_count) = {
        let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let session = store.sessions.iter().find(|item| item.id == request_session_id).ok_or("发起提问的会话不存在")?;
        (should_auto_compact(session), session.messages.len())
    };
    let recently_failed = agent.auto_compact_failures.lock().ok()
        .and_then(|failures| failures.get(&request_session_id).copied())
        .is_some_and(|count| message_count < count.saturating_add(4));
    if needs_compaction && !recently_failed {
        let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: "正在自动压缩较早对话…".into() });
        match compact_session_context(&app, &agent, &request_session_id, false, Duration::from_secs(45)).await {
            Ok(_) => {
                if let Ok(mut failures) = agent.auto_compact_failures.lock() { failures.remove(&request_session_id); }
                let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: "较早对话已整理，继续回答…".into() });
            }
            Err(error) => {
                if let Ok(mut failures) = agent.auto_compact_failures.lock() { failures.insert(request_session_id.clone(), message_count); }
                eprintln!("GearNote automatic chat compaction failed: {error}");
                let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: "自动压缩未完成，继续使用近期对话…".into() });
            }
        }
    }
    let (recent_messages, selected, project_memory, working, summary, open_choice) = {
        let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let session = store.sessions.iter().find(|item| item.id == request_session_id).ok_or("发起提问的会话不存在")?;
        (session.messages[session.compacted_count.min(session.messages.len())..].to_vec(), selected_messages(session, &message_ids)?, store.project_memory.iter().filter(|entry| memory_is_current(entry)).cloned().collect::<Vec<_>>(), session.working_context.clone(), session.compacted_summary.clone(), session.pending_choice.as_ref().filter(|choice| choice.status == "open").cloned())
    };
    let mut image_source = None;
    let attachment = if let Some(image_id) = frame_id {
        if config.base_url.starts_with("https://api.deepseek.com") && config.model.eq_ignore_ascii_case("deepseek-v4-pro") {
            return Err("DeepSeek V4 Pro 不处理图片；请在设置中选择支持视觉输入的 deepseek-flash。".into());
        }
        Uuid::parse_str(&image_id).map_err(|_| "截图编号无效")?;
        let frame = {
            let store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
            let session = store.sessions.iter().find(|item| item.id == request_session_id).ok_or("发起提问的会话不存在")?;
            session.messages.iter().find_map(|message| message.frame.as_ref().filter(|frame| frame.image_id == image_id).cloned())
                .ok_or("当前会话没有这张截图")?
        };
        let path = app.path().app_data_dir().map_err(|error| error.to_string())?
            .join("chat-frames").join(format!("{image_id}.jpg"));
        let bytes = fs::read(path).map_err(|_| "截图文件已丢失")?;
        if bytes.len() < 3 || bytes.len() > 8_000_000 || !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            return Err("截图文件无效或超过 8 MB".into());
        }
        let title = storage.records.lock().map_err(|_| "资料库暂不可用")?.iter()
            .find(|record| record.id == frame.record_id).map(|record| record.title.clone())
            .unwrap_or_else(|| "视频截图".into());
        image_source = Some(SourceChunk::image(frame.record_id.clone(), title, frame.at_seconds));
        Some((frame, format!("data:image/jpeg;base64,{}", crate::storyboard::base64(&bytes))))
    } else { None };
    let records = storage.records.lock().map_err(|_| "资料库暂不可用")?.iter()
        .filter(|record| referenced_ids.contains(&record.id)).cloned().collect::<Vec<_>>();
    let full_reading = config.reading_budget.strategy == ReadingStrategy::Full || continuation.as_ref().is_some_and(|value| value.progress.full);
    let deep_mode = deep_mode || (full_reading && (config.explore_references || confirmed_library_followup || continuation.is_some()) && referenced_ids.len() == 1 && records.first().is_some_and(|record| record.record_type == "video") && image_source.is_none() && selected.is_empty() && open_choice.is_none());
    let deep_record = if deep_mode { Some(validate_deep_video(&records, &referenced_ids, image_source.is_some(), !selected.is_empty(), open_choice.is_some())?) } else { None };
    let general_chat = referenced_ids.is_empty() && selected.is_empty() && image_source.is_none();
    let use_exploration = (config.explore_references || confirmed_library_followup || continuation.is_some()) && !deep_mode && image_source.is_none() && selected.is_empty() && !referenced_ids.is_empty();
    let use_semantic = config.semantic_retrieval && !deep_mode && !full_reading && image_source.is_none() && selected.is_empty() && !referenced_ids.is_empty();
    if continuation.as_ref().is_some_and(|value| value.fingerprint != reading_fingerprint(&records)) {
        return Err("资料内容已改变，请重新提问后读取".into());
    }
    let index = (use_exploration || use_semantic).then(|| {
        let mut index = if full_reading { ReadIndex::for_full_read(&records, &referenced_ids) } else { ReadIndex::new(&records, &referenced_ids) };
        if let Some(resume) = &continuation { index.exclude_read(&resume.progress.read_chunk_ids); }
        index
    });
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(config.reading_budget.request_seconds)).build().map_err(|_| "无法初始化网络客户端")?;
    let semantic_scores = if use_semantic {
        if let Some(index) = index.as_ref().filter(|index| !index.is_empty()) {
            let custom = !config.embedding_base_url.is_empty();
            let base_url = if custom { &config.embedding_base_url } else { &config.base_url };
            let embedding_key = if custom { agent.embedding_key.lock().ok().and_then(|value| value.clone()) }
                else { Some(key.clone()) };
            if let Some(embedding_key) = embedding_key.or_else(|| {
                Url::parse(base_url).ok().and_then(|url| matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")).then(String::new))
            }) {
                crate::semantic::similarities(&client, base_url, &config.embedding_model, &embedding_key,
                    &agent.sessions_path, index.entries(), question).await
            } else { None }
        } else { None }
    } else { None };
    let mut prepared = context::build_with_message_refs_limit_semantic(&persona, question, &recent_messages, &selected, &records, &referenced_ids,
        image_source, &project_memory, &working, &summary, 0, config.context_tier.char_limit(), semantic_scores.as_deref());
    if continuation.is_some() && !deep_mode {
        if let Some(index) = &index {
            // 不支持工具或没有工具调用时，续读的普通上下文也只提供尚未读过的片段。
            let entries = index.entries();
            let scores = context::score_chunks(question, &entries.iter().map(|(_, source)| source).collect::<Vec<_>>());
            let mut ranked = entries.iter().zip(scores).collect::<Vec<_>>();
            ranked.sort_by(|a, b| b.1.cmp(&a.1));
            prepared.sources = ranked.into_iter().take(8).map(|((_, source), _)| {
                let mut source = source.clone(); source.text = source.text.chars().take(1_200).collect(); source
            }).collect();
            let text = prepared.sources.iter().enumerate().map(|(index, source)| format!("[{}] {}\n{}", index + 1, source.title, source.text))
                .collect::<Vec<_>>().join("\n\n");
            if let Some((prefix, _)) = prepared.user_text.rsplit_once("本次可用资料：\n") {
                prepared.user_text = format!("{prefix}本次可用资料：\n{text}\n本次是续读，请结合先前回答重点说明新读内容，不要声称已读未提供的正文。");
            }
        }
    }
    if general_chat { context::enable_general_chat(&mut prepared, &persona); }
    let mut exploration_prepared = index.as_ref().filter(|index| use_exploration && !index.is_empty()).map(|index| {
        context::build_exploration_limit(&persona, question, &recent_messages, referenced_ids.len(),
            index.directory(&records, &referenced_ids, 6_000), index.len(), &project_memory, &working, &summary, config.context_tier.char_limit())
    });
    let mut deep_prepared = deep_record.map(|record| {
        let batch_count = deep_video::batches(record).len();
        context::build_exploration_limit(&persona, question, &recent_messages, 1,
            format!("深度模式：仅顺序读取《{}》的字幕；共约 {batch_count} 批，每批最多 3000 字；本次预算 {} 批或 {} 秒。正文将逐批读取。", record.title, config.reading_budget.deep_batches, config.reading_budget.deep_seconds),
            batch_count, &project_memory, &working, &summary, config.context_tier.char_limit())
    });
    if let Some(choice) = &open_choice {
        let instruction = format!("\n\n{}", choice_system_instruction(choice, question, choice_id.as_deref()));
        prepared.system_text.push_str(&instruction);
        if let Some(exploration_prepared) = &mut exploration_prepared { exploration_prepared.system_text.push_str(&instruction); }
        if let Some(deep_prepared) = &mut deep_prepared { deep_prepared.system_text.push_str(&instruction); }
    }
    let reading_lease = if deep_record.is_some() || exploration_prepared.is_some() { Some(ReadingLease::start(&agent, &app, &request_id)?) } else { None };
    let reading_task = reading_lease.as_ref().map(|lease| ReadingTask::new(lease.control.clone(), config.reading_budget.clone()));
    let (exploration, read_coverage) = if let (Some(record), Some(deep_prepared)) = (deep_record, deep_prepared.as_ref()) {
        let outcome = explore_video_deep_task(|message| {
            let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: message.into() });
        }, &config, &key, deep_prepared, record, question, continuation.as_ref().map(|value| &value.progress), reading_task.as_ref()).await;
        let (result, coverage) = match outcome {
            Ok(value) => value,
            Err(error) => {
                let now = now_millis()?;
                let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
                let mut next = store.clone();
                if next.active_id != request_session_id { return Err("会话已切换，深度阅读失败记录未保存".into()); }
                let active = next.sessions.iter_mut().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
                if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
                let assistant = save_deep_failure_exchange(active, question, &error, &now,
                    started_at.elapsed().as_millis().min(u64::MAX as u128) as u64, choice_id.as_deref());
                persist_chat_store(&agent.sessions_path, &next)?;
                *store = next;
                drop(store);
                let _ = app.emit("knowflow:chat-changed", ());
                return Ok(assistant);
            }
        };
        (Some(result), Some(coverage))
    } else if let (Some(index), Some(exploration_prepared)) = (index.as_ref(), exploration_prepared.as_ref()) {
        let activity = |message: &str| { let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: message.into() }); };
        if full_reading {
            let full_index = ReadIndex::for_full_read(&records, &referenced_ids);
            let result = read_full_references(activity, &client, &config, &key, exploration_prepared, &full_index,
                continuation.as_ref().filter(|value| value.progress.full).map(|value| &value.progress), reading_task.as_ref().ok_or("阅读状态未初始化")?).await?;
            (Some(result), None)
        } else {
            (explore_references_task(activity, &client, &config, &key, exploration_prepared, index, open_choice.is_none(), semantic_scores.as_deref(), reading_task.as_ref()).await?, None)
        }
    } else { (None, None) };
    drop(reading_lease);
    if let Some((draft, steps)) = exploration.as_ref().and_then(|value| value.choice.clone().map(|draft| (draft, value.steps.clone()))) {
        let now = now_millis()?;
        let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let mut next = store.clone();
        if next.active_id != request_session_id { return Err("会话已切换，本轮选择题未保存".into()); }
        let active = next.sessions.iter_mut().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
        if active.pending_choice.as_ref().is_some_and(|choice| choice.status == "open") { return Err("会话已有未回答的选择题".into()); }
        if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
        let assistant = save_choice_exchange(active, question, draft, steps, &now, started_at.elapsed().as_millis().min(u64::MAX as u128) as u64);
        persist_chat_store(&agent.sessions_path, &next)?;
        *store = next;
        drop(store);
        let _ = app.emit("knowflow:chat-changed", ());
        return Ok(assistant);
    }
    let mut read_progress = exploration.as_ref().and_then(|value| value.progress.clone())
        .or_else(|| (exploration.is_none() && use_exploration && index.as_ref().is_some_and(|index| !index.is_empty())).then(ReadProgress::default));
    if let Some(progress) = &mut read_progress {
        if !progress.deep && !progress.full {
            if let Some(resume) = &continuation { progress.read_chunk_ids.extend(resume.progress.read_chunk_ids.clone()); }
            progress.read_chunk_ids.sort(); progress.read_chunk_ids.dedup();
            let mut remaining = ReadIndex::new(&records, &referenced_ids);
            remaining.exclude_read(&progress.read_chunk_ids);
            if remaining.is_empty() { read_progress = None; }
        }
    }
    let explored = exploration.is_some();
    let (request_messages, sources, mut tool_steps) = if let Some(exploration) = exploration {
        (exploration.messages, exploration.sources, exploration.steps)
    } else {
        let user_content = request_user_content(prepared.user_text, attachment.as_ref().map(|(_, image_url)| image_url.as_str()));
        (vec![serde_json::json!({"role":"system","content":prepared.system_text}),
            serde_json::json!({"role":"user","content":user_content})], prepared.sources, Vec::new())
    };
    if !explored && use_exploration {
        if let (Some(progress), Some(index)) = (&mut read_progress, &index) {
            for (id, source) in index.entries() {
                if sources.iter().any(|used| used.record_id == source.record_id && used.at_seconds == source.at_seconds
                    && source.text.starts_with(&used.text)) { progress.read_chunk_ids.push(id.clone()); }
            }
            let mut remaining = ReadIndex::new(&records, &referenced_ids);
            remaining.exclude_read(&progress.read_chunk_ids);
            if remaining.is_empty() { read_progress = None; }
        }
    }
    if let Some(task) = &reading_task {
        if let Some(reason) = task.reason() { tool_steps.push(format!("阅读结束：{reason}，正在根据已提供内容回答")); }
    }
    let read_sources = if explored { sources.iter().enumerate().map(|(index, source)| ReadSource {
        source_index: index + 1, record_id: source.record_id.clone(), title: source.title.clone(), at_seconds: source.at_seconds,
    }).collect() } else { Vec::new() };
    let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: "正在生成回答…".into() });
    let final_result = generate_final_answer(&app, &client, &config, &key, &request_id,
        &request_messages, explored, attachment.is_some()).await;
    let generated = match final_result {
        Ok(value) => value,
        Err(error) => {
            let now = now_millis()?;
            let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
            let mut next = store.clone();
            if next.active_id != request_session_id { return Err("会话已切换，本轮失败记录未保存".into()); }
            let active = next.sessions.iter_mut().find(|item| item.id == request_session_id).ok_or("当前会话不存在")?;
            if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
            if read_progress.is_some() { tool_steps.push("可继续读取：本轮最终回答未返回，已保留阅读进度".into()); }
            let assistant = save_final_failure_exchange(active, question, &error, &now,
                started_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
                attachment.as_ref().map(|(frame, _)| frame.clone()), tool_steps, read_sources, read_coverage, choice_id.as_deref());
            active.read_continuation = read_progress.map(|progress| ReadContinuation { message_id: assistant.id.clone(),
                question: question.into(), referenced_ids: referenced_ids.clone(), fingerprint: reading_fingerprint(&records), progress });
            persist_chat_store(&agent.sessions_path, &next)?;
            *store = next;
            drop(store);
            let _ = app.emit("knowflow:chat-changed", ());
            return Ok(assistant);
        }
    };
    let (citations, evidence_notice) = assess_answer_evidence(&generated.text, &sources);
    if let Some(notice) = evidence_notice { tool_steps.push(notice); }
    if let Some(warning) = generated.warning { tool_steps.push(format!("回答提醒：{warning}")); }
    let answer = generated.text;
    if read_progress.is_some() { tool_steps.push("可继续读取：点击按钮将使用新的阅读预算，可能增加 API 用量".into()); }
    let now = now_millis()?;
    let user = ChatMessage { id: format!("msg-{}", Uuid::new_v4()), role: "user".into(), created_at: now.to_string(), paragraphs: vec![if continuation.is_some() { format!("继续读取：{question}") } else { question.into() }], tool_steps: Vec::new(), citations: Vec::new(), frame: attachment.as_ref().map(|(frame, _)| frame.clone()), run: None, suggestions: Vec::new() };
    let assistant = ChatMessage { id: format!("msg-{}", Uuid::new_v4()), role: "agent".into(), created_at: now.to_string(), paragraphs: vec![answer], tool_steps, citations, frame: None,
        run: Some(ChatRun { elapsed_ms: started_at.elapsed().as_millis().min(u64::MAX as u128) as u64, changed_files: Vec::new(), read_sources, read_coverage }), suggestions: Vec::new() };
    {
        let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let mut next = store.clone();
        if next.active_id != request_session_id { return Err("会话已切换，本轮回答未保存".into()); }
        let active = next.sessions.iter_mut().find(|item| item.id == next.active_id).ok_or("当前会话不存在")?;
        if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
        answer_pending_choice(active, question, &now, choice_id.as_deref());
        active.read_continuation = read_progress.map(|progress| ReadContinuation { message_id: assistant.id.clone(),
            question: question.into(), referenced_ids: referenced_ids.clone(), fingerprint: reading_fingerprint(&records), progress });
        if active.pending_library_search.as_ref().is_some_and(|proposal| proposal.status == "accepted" || proposal.status == "open") {
            active.pending_library_search = None;
        }
        active.messages.push(user);
        active.messages.push(assistant.clone());
        active.updated_at = now;
        persist_chat_store(&agent.sessions_path, &next)?;
        *store = next;
    }
    let _ = app.emit("knowflow:chat-changed", ());
    Ok(assistant)
}
