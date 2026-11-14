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
    let config = agent.config.lock().map_err(|_| "模型配置暂不可用")?.clone();
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
    let deep_record = if deep_mode { Some(validate_deep_video(&records, &referenced_ids, image.is_some(), !selected.is_empty(), open_choice.is_some())?) } else { None };
    let general_chat = referenced_ids.is_empty() && selected.is_empty() && image.is_none();
    let semantic_pending = config.semantic_retrieval && !deep_mode && image.is_none()
        && selected.is_empty() && !referenced_ids.is_empty();
    let index = (explore && !deep_mode && image.is_none() && selected.is_empty() && !referenced_ids.is_empty()).then(|| ReadIndex::new(&records, &referenced_ids));
    let mut prepared = if let Some(record) = deep_record {
        let batch_count = deep_video::batches(record).len();
        context::build_exploration_limit(&persona, question, &messages, 1,
            format!("深度模式：仅顺序读取《{}》的字幕；共约 {batch_count} 批，每批最多 3000 字，每轮最多 4 批。正文将在发送后逐批读取；未读字幕不会作为事实依据。", record.title),
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

#[tauri::command]
pub async fn ask_agent(app: AppHandle, agent: State<'_, AgentState>, storage: State<'_, AppState>, notes: State<'_, NotesState>, question: String, referenced_ids: Vec<String>, request_id: String, frame_id: Option<String>, message_ids: Vec<String>, choice_id: Option<String>, deep_mode: bool) -> Result<ChatMessage, String> {
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
    let config = agent.config.lock().map_err(|_| "模型配置暂不可用")?.clone();
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
    let deep_record = if deep_mode { Some(validate_deep_video(&records, &referenced_ids, image_source.is_some(), !selected.is_empty(), open_choice.is_some())?) } else { None };
    let general_chat = referenced_ids.is_empty() && selected.is_empty() && image_source.is_none();
    let use_exploration = (config.explore_references || confirmed_library_followup) && !deep_mode && image_source.is_none() && selected.is_empty() && !referenced_ids.is_empty();
    let use_semantic = config.semantic_retrieval && !deep_mode && image_source.is_none() && selected.is_empty() && !referenced_ids.is_empty();
    let index = (use_exploration || use_semantic).then(|| ReadIndex::new(&records, &referenced_ids));
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(180)).build().map_err(|_| "无法初始化网络客户端")?;
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
    if general_chat { context::enable_general_chat(&mut prepared, &persona); }
    let mut exploration_prepared = index.as_ref().filter(|index| use_exploration && !index.is_empty()).map(|index| {
        context::build_exploration_limit(&persona, question, &recent_messages, referenced_ids.len(),
            index.directory(&records, &referenced_ids, 6_000), index.len(), &project_memory, &working, &summary, config.context_tier.char_limit())
    });
    let mut deep_prepared = deep_record.map(|record| {
        let batch_count = deep_video::batches(record).len();
        context::build_exploration_limit(&persona, question, &recent_messages, 1,
            format!("深度模式：仅顺序读取《{}》的字幕；共约 {batch_count} 批，每批最多 3000 字，每轮最多 4 批。正文将逐批读取。", record.title),
            batch_count, &project_memory, &working, &summary, config.context_tier.char_limit())
    });
    if let Some(choice) = &open_choice {
        let instruction = format!("\n\n{}", choice_system_instruction(choice, question, choice_id.as_deref()));
        prepared.system_text.push_str(&instruction);
        if let Some(exploration_prepared) = &mut exploration_prepared { exploration_prepared.system_text.push_str(&instruction); }
        if let Some(deep_prepared) = &mut deep_prepared { deep_prepared.system_text.push_str(&instruction); }
    }
    let (exploration, read_coverage) = if let (Some(record), Some(deep_prepared)) = (deep_record, deep_prepared.as_ref()) {
        let (result, coverage) = explore_video_deep(|message| {
            let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: message.into() });
        }, &config, &key, deep_prepared, record, question).await?;
        (Some(result), Some(coverage))
    } else if let (Some(index), Some(exploration_prepared)) = (index.as_ref(), exploration_prepared.as_ref()) {
        (explore_references_with_semantic(|message| {
            let _ = app.emit("knowflow:agent-tool", ToolActivity { request_id: request_id.clone(), message: message.into() });
        }, &client, &config, &key, exploration_prepared, index, open_choice.is_none(), semantic_scores.as_deref()).await?, None)
    } else { (None, None) };
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
    let explored = exploration.is_some();
    let (request_messages, sources, tool_steps) = if let Some(exploration) = exploration {
        (exploration.messages, exploration.sources, exploration.steps)
    } else {
        let user_content = request_user_content(prepared.user_text, attachment.as_ref().map(|(_, image_url)| image_url.as_str()));
        (vec![serde_json::json!({"role":"system","content":prepared.system_text}),
            serde_json::json!({"role":"user","content":user_content})], prepared.sources, Vec::new())
    };
    let read_sources = if explored { sources.iter().enumerate().map(|(index, source)| ReadSource {
        source_index: index + 1, record_id: source.record_id.clone(), title: source.title.clone(), at_seconds: source.at_seconds,
    }).collect() } else { Vec::new() };
    let mut response = client.post(format!("{}/chat/completions", config.base_url)).bearer_auth(key)
        .json(&answer_payload(&config, &request_messages, explored))
        .send().await.map_err(|error| format!("模型请求失败：{error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let detail = provider_error(status, &body, &config.base_url, &config.model);
        if attachment.is_some() && status == reqwest::StatusCode::BAD_REQUEST {
            return Err(format!("图片提问被服务拒绝；请确认当前模型支持视觉输入。{detail}"));
        }
        return Err(detail);
    }
    let mut parser = SseParser::default();
    let mut answer = String::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| format!("模型流式连接中断：{error}"))? {
        let deltas = parser.push(&chunk)?;
        publish_deltas(&app, &request_id, &mut answer, deltas)?;
        if parser.done { break; }
    }
    let tail = parser.push(b"\n\n")?;
    publish_deltas(&app, &request_id, &mut answer, tail)?;
    if !parser.done && parser.finish_reason.is_none() { return Err("模型流式连接提前结束，回答未保存".into()); }
    if let Some(reason) = parser.finish_reason.as_deref() {
        if reason != "stop" { return Err(format!("模型结束原因是 {reason}，回答未保存")); }
    }
    let answer = answer.trim();
    if answer.is_empty() { return Err("模型返回了空回答".into()); }
    if answer.chars().count() > 20_000 { return Err("模型回答过长，未保存".into()); }
    let citations = validate_answer_evidence(answer, &sources, general_chat)?;
    let now = now_millis()?;
    let user = ChatMessage { id: format!("msg-{}", Uuid::new_v4()), role: "user".into(), created_at: now.to_string(), paragraphs: vec![question.into()], tool_steps: Vec::new(), citations: Vec::new(), frame: attachment.as_ref().map(|(frame, _)| frame.clone()), run: None, suggestions: Vec::new() };
    let assistant = ChatMessage { id: format!("msg-{}", Uuid::new_v4()), role: "agent".into(), created_at: now.to_string(), paragraphs: vec![answer.into()], tool_steps, citations, frame: None,
        run: Some(ChatRun { elapsed_ms: started_at.elapsed().as_millis().min(u64::MAX as u128) as u64, changed_files: Vec::new(), read_sources, read_coverage }), suggestions: Vec::new() };
    {
        let mut store = agent.sessions.lock().map_err(|_| "会话暂不可用")?;
        let mut next = store.clone();
        if next.active_id != request_session_id { return Err("会话已切换，本轮回答未保存".into()); }
        let active = next.sessions.iter_mut().find(|item| item.id == next.active_id).ok_or("当前会话不存在")?;
        if active.messages.is_empty() && active.title == "新会话" { active.title = session_title(question); }
        answer_pending_choice(active, question, &now, choice_id.as_deref());
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
