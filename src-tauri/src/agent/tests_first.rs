// Agent 既有测试的前半部分。
    use super::*;
    use crate::{storage::KnowledgeRecord, ProcessStatus};

    #[test]
    fn library_search_requires_explicit_request_and_confirmation() {
        assert!(library_search_requested("请在整个资料库里找 Rust"));
        assert!(!library_search_requested("请总结已选的 Rust 资料"));
        let mut session = new_session().unwrap();
        let title = crate::db::LibraryTitle { id: "unselected".into(), title: "Rust 指南".into() };
        let answer = save_library_exchange(&mut session, "在整个资料库里找 Rust", vec![title], "100", 20);
        assert!(session.referenced_ids.is_empty());
        assert!(answer.citations.is_empty());
        assert_eq!(session.pending_library_search.as_ref().unwrap().status, "open");
        let proposal_id = session.pending_library_search.as_ref().unwrap().id.clone();
        let mut dismissed = session.clone();
        resolve_library_search_in_session(&mut dismissed, &proposal_id, false, "101").unwrap();
        assert!(dismissed.referenced_ids.is_empty());
        assert_eq!(dismissed.pending_library_search.as_ref().unwrap().status, "dismissed");
        assert!(resolve_library_search_in_session(&mut dismissed, &proposal_id, true, "102").is_err());
        resolve_library_search_in_session(&mut session, &proposal_id, true, "101").unwrap();
        assert_eq!(session.referenced_ids, vec!["unselected"]);
        assert_eq!(session.pending_library_search.as_ref().unwrap().status, "accepted");
    }

    #[test]
    fn old_session_without_library_proposal_still_loads() {
        let session = new_session().unwrap();
        let mut value = serde_json::to_value(&session).unwrap();
        value.as_object_mut().unwrap().remove("pendingLibrarySearch");
        let parsed: ChatSession = serde_json::from_value(value).unwrap();
        assert!(parsed.pending_library_search.is_none());
    }

    #[test]
    fn chat_note_proposal_waits_for_confirmation_without_writing() {
        assert!(chat_note_edit_requested("请修改笔记《测试笔记》"));
        assert!(!chat_note_edit_requested("总结这篇笔记"));
        assert!(note_evidence_requested("根据资料修改笔记"));
        assert_eq!(note_edit_tool()["function"]["name"], "propose_note_edit");
        let dir = std::env::temp_dir().join(format!("gearnote-chat-note-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(crate::db::FILE_NAME);
        let note: crate::notes::Note = serde_json::from_value(serde_json::json!({"id":"note-1","title":"测试笔记",
            "body":"原文。","sources":[],"createdAt":100,"updatedAt":200})).unwrap();
        crate::db::write_notes(&path, &[note.clone()]).unwrap();
        let args = r#"{"edits":[{"mode":"replace","targetText":"原文。","replacementText":"新文字。","reason":"修订"}]}"#;
        let (proposal, after) = workflow::parse_model_edit_with_sources(args, &note, &[], false).unwrap();
        workflow::store_note_proposal(&path, &note, &after, &proposal).unwrap();
        let mut session = new_session().unwrap();
        let message = save_chat_note_exchange(&mut session, "修改笔记《测试笔记》", &note.title, proposal, &[], "101", 10);
        assert_eq!(session.pending_chat_note_edit.as_ref().unwrap().message_id, message.id);
        assert_eq!(session.pending_chat_note_edit.as_ref().unwrap().proposal.status, "pending");
        assert_eq!(crate::db::read_notes(&path).unwrap().unwrap()[0].body, "原文。");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn command_proposal_does_not_execute_before_confirmation() {
        assert!(command_requested("请运行 ffprobe 查看版本"));
        assert!(!command_requested("ffprobe 是什么？"));
        assert!(!command_requested("不要运行 ffprobe"));
        assert_eq!(command_tool()["function"]["name"], "propose_command");
        let mut session = new_session().unwrap();
        let message = save_command_exchange(&mut session, "请运行 ffprobe 查看版本", "ffprobe".into(), vec!["-version".into()], "100", 10).unwrap();
        assert_eq!(session.pending_command.as_ref().unwrap().message_id, message.id);
        assert_eq!(session.pending_command.as_ref().unwrap().status, "open");
        let id = session.pending_command.as_ref().unwrap().id.clone();
        let (program, args) = transition_command(&mut session, &id, false).unwrap();
        assert_eq!(program, "ffprobe");
        assert_eq!(args, ["-version"]);
        assert_eq!(session.pending_command.as_ref().unwrap().status, "rejected");
        assert!(transition_command(&mut session, &id, true).is_err());
        let mut approved = new_session().unwrap();
        save_command_exchange(&mut approved, "请运行 ffprobe 查看版本", "ffprobe".into(), vec!["-version".into()], "101", 10).unwrap();
        let approved_id = approved.pending_command.as_ref().unwrap().id.clone();
        let (program, args) = transition_command(&mut approved, &approved_id, true).unwrap();
        assert_eq!(approved.pending_command.as_ref().unwrap().status, "running");
        assert_eq!((program, args), ("ffprobe".into(), vec!["-version".into()]));
    }

    #[test]
    fn ordinary_chat_without_references_accepts_plain_answer_but_not_fake_citation() {
        assert!(validate_answer_evidence("可以先明确目标，再列出可选方案。", &[], true).unwrap().is_empty());
        assert!(validate_answer_evidence("根据资料 [1]，方案已经完成。", &[], true).is_err());
        assert!(validate_answer_evidence("根据资料，方案已经完成。", &[], false).is_err());
    }

    #[test]
    fn deep_video_failure_keeps_question_and_error_in_session() {
        let mut session = new_session().unwrap();
        let reply = save_deep_failure_exchange(&mut session, "你看看这个视频", "摘要原句不在字幕中", "100", 1200, None);
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].paragraphs, vec!["你看看这个视频"]);
        assert_eq!(session.messages[1].id, reply.id);
        assert!(reply.paragraphs[0].contains("摘要原句不在字幕中"));
        assert!(reply.citations.is_empty());
        assert!(reply.tool_steps[0].starts_with("深度阅读失败："));
        let deep_outcome = reply.outcome.clone().unwrap();
        assert_eq!(deep_outcome.kind, MessageKind::Answer);
        assert_eq!(deep_outcome.answer, AnswerStatus::Failed);
        assert_eq!(deep_outcome.answer_reason, Some(AnswerEndReason::ReadingFailed));
        assert_eq!(deep_outcome.reading_reason, Some(ReadingEndReason::ReadFailed));
        assert_eq!(reply.run.unwrap().elapsed_ms, 1200);
    }

    #[test]
    fn final_request_without_any_model_text_keeps_question_and_error() {
        let mut session = new_session().unwrap();
        let reply = save_final_failure_exchange(&mut session, "你觉得讲得怎么样", "模型返回了空回答", AnswerEndReason::EmptyAnswer, Some(ReadingEndReason::UserStopped), "100", 2400,
            None, vec!["深度阅读结束：已读完".into()],
            vec![ReadSource { source_index: 1, record_id: "video".into(), title: "视频字幕".into(), at_seconds: Some(15) }],
            Some(ReadCoverage { read_until_seconds: 15, total_seconds: 15, batch_count: 1, complete: true, reason: "字幕已读完".into() }), None);
        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].paragraphs, vec!["你觉得讲得怎么样"]);
        assert_eq!(session.messages[1].id, reply.id);
        assert!(reply.paragraphs[0].contains("未收到可显示的模型正文"));
        assert!(reply.citations.is_empty());
        assert_eq!(reply.run.unwrap().read_coverage.unwrap().batch_count, 1);
        assert!(reply.tool_steps.last().unwrap().starts_with("回答未完成："));
        let outcome = reply.outcome.unwrap();
        assert_eq!(outcome.kind, MessageKind::Answer);
        assert_eq!(outcome.answer, AnswerStatus::Failed);
        assert_eq!(outcome.answer_reason, Some(AnswerEndReason::EmptyAnswer));
        // 回答失败仍须保留阅读结束原因，二者不混用一个状态。
        assert_eq!(outcome.reading_reason, Some(ReadingEndReason::UserStopped));
    }


    #[test]
    fn deep_video_retries_one_invalid_checkpoint_without_accepting_fake_quote() {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut prompts = Vec::new();
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0;
                loop {
                    line.clear(); reader.read_line(&mut line).unwrap();
                    if line == "\r\n" { break; }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") { length = value.trim().parse::<usize>().unwrap(); }
                }
                let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                prompts.push(request["messages"][1]["content"].as_str().unwrap().to_string());
                let quote = if attempt == 0 { "不存在的原句" } else { "真实字幕原句" };
                let content = format!("{{\"enough\":true,\"notes\":[{{\"sourceIndex\":1,\"quote\":\"{quote}\",\"finding\":\"事实\"}}]}}");
                let response = serde_json::json!({"choices":[{"message":{"content":content}}]}).to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            }
            prompts
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({"id":"video","type":"video","title":"测试","site":"example.com",
            "savedAt":"1","status":"ready","sourceUrl":"https://example.com","contentText":"","excerpt":[],
            "transcript":[{"atSeconds":0,"text":"真实字幕原句"}]})).unwrap();
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let prepared = context::build_exploration_limit("人格", "开头讲了什么", &[], 1, "深度模式".into(), 1,
            &[], &WorkingContext::default(), "", config.context_tier.char_limit());
        let (exploration, _) = tauri::async_runtime::block_on(explore_video_deep(|_| {}, &config, "test", &prepared, &record, "开头讲了什么")).unwrap();
        let prompts = server.join().unwrap();
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains("上次检查点被拒绝"));
        assert!(exploration.messages[1]["content"].as_str().unwrap().contains("真实字幕原句"));
        assert!(!exploration.messages[1]["content"].as_str().unwrap().contains("不存在的原句"));
        assert_eq!(exploration.sources.len(), 1);
        assert!(exploration.sources[0].text.contains("真实字幕原句"));
    }

    #[test]
    fn deep_video_continues_after_two_invalid_checkpoints() {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0;
                loop {
                    line.clear(); reader.read_line(&mut line).unwrap();
                    if line == "\r\n" { break; }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") { length = value.trim().parse::<usize>().unwrap(); }
                }
                let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
                let content = r#"{"enough":true,"notes":[{"sourceIndex":1,"quote":"不存在的原句","finding":"虚假事实"}]}"#;
                let response = serde_json::json!({"choices":[{"message":{"content":content}}]}).to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            }
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({"id":"video","type":"video","title":"测试","site":"example.com",
            "savedAt":"1","status":"ready","sourceUrl":"https://example.com","contentText":"","excerpt":[],
            "transcript":[{"atSeconds":0,"text":"真实字幕原句"}]})).unwrap();
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let prepared = context::build_exploration_limit("人格", "总结一下", &[], 1, "深度模式".into(), 1,
            &[], &WorkingContext::default(), "", config.context_tier.char_limit());
        let (exploration, coverage) = tauri::async_runtime::block_on(explore_video_deep(|_| {}, &config, "test", &prepared, &record, "总结一下")).unwrap();
        server.join().unwrap();
        assert!(coverage.complete);
        assert!(exploration.steps.iter().any(|step| step.contains("忽略 1 条无法核对")));
        assert!(!exploration.messages[1]["content"].as_str().unwrap().contains("不存在的原句"));
        assert!(exploration.messages[1]["content"].as_str().unwrap().contains("真实字幕原句"));
        assert_eq!(exploration.sources.len(), 1);
    }

    #[test]
    fn deep_video_stops_after_verified_enough_and_reports_partial_coverage() {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut length = 0;
            loop {
                line.clear(); reader.read_line(&mut line).unwrap();
                if line == "\r\n" { break; }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") { length = value.trim().parse::<usize>().unwrap(); }
            }
            let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            let content = r#"{"enough":true,"notes":[{"sourceIndex":1,"quote":"开头事实。","finding":"提到开头事实"}]}"#;
            let response = serde_json::json!({"choices":[{"message":{"role":"assistant","content":content}}]}).to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            request
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({"id":"video","type":"video","title":"长视频","site":"example.com",
            "savedAt":"1","status":"ready","sourceUrl":"https://example.com","contentText":"","excerpt":[],
            "transcript":[{"atSeconds":0,"text":format!("开头事实。{}", "普通字幕".repeat(4_000))}],"durationSeconds":600})).unwrap();
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let prepared = context::build_exploration_limit("人格", "开头说了什么", &[], 1, "深度模式".into(), 10,
            &[], &WorkingContext::default(), "", config.context_tier.char_limit());
        let (exploration, coverage) = tauri::async_runtime::block_on(explore_video_deep(|_| {}, &config, "test", &prepared, &record, "开头说了什么")).unwrap();
        let request = server.join().unwrap();
        assert!(request["messages"][1]["content"].as_str().unwrap().contains("开头事实。"));
        assert_eq!(coverage.batch_count, 4);
        assert!(!coverage.complete);
        assert!(coverage.read_until_seconds < coverage.total_seconds);
        assert_eq!(exploration.sources.len(), 4);
        assert!(exploration.messages[1]["content"].as_str().unwrap().contains("[1] 提到开头事实"));
        assert!(validate_answer_evidence("开头提到事实。「开头事实。」[1]", &exploration.sources, false).is_ok());
    }

    #[test]
    fn deep_video_scope_rejects_other_records_and_attachments() {
        let video: KnowledgeRecord = serde_json::from_value(serde_json::json!({"id":"video","type":"video","title":"视频","site":"example.com",
            "savedAt":"1","status":"ready","sourceUrl":"https://example.com","contentText":"","excerpt":[],
            "transcript":[{"atSeconds":0,"text":"字幕"}]})).unwrap();
        assert!(validate_deep_video(&[video.clone()], &["video".into()], false, false, false).is_ok());
        assert!(validate_deep_video(&[video.clone()], &["video".into(), "other".into()], false, false, false).is_err());
        assert!(validate_deep_video(&[video.clone()], &["video".into()], true, false, false).is_err());
        assert!(validate_deep_video(&[video.clone()], &["video".into()], false, true, false).is_err());
        assert!(validate_deep_video(&[video.clone()], &["video".into()], false, false, true).is_err());
        let mut article = video;
        article.record_type = "article".into();
        assert!(validate_deep_video(&[article], &["video".into()], false, false, false).is_err());
    }

    fn choice_args() -> serde_json::Value {
        serde_json::json!({"prompt":"选哪种整理方式？","why":"两种做法会影响后续结构。","options":[
            {"id":"by_topic","label":"按主题","detail":"按主题分组"},
            {"id":"by_time","label":"按时间","detail":"按时间排列"}],"recommended":"by_topic"})
    }

    fn mock_choice_exploration(replies: Vec<serde_json::Value>) -> (Exploration, Vec<serde_json::Value>) {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0;
                loop {
                    line.clear(); reader.read_line(&mut line).unwrap();
                    if line == "\r\n" { break; }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") { length = value.trim().parse::<usize>().unwrap(); }
                }
                let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
                requests.push(serde_json::from_slice::<serde_json::Value>(&body).unwrap());
                let body = reply.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
            requests
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({
            "id":"rec-selected","type":"article","title":"测试资料","site":"example.com",
            "savedAt":"2026-11-11T00:00:00Z","status":"ready","sourceUrl":"https://example.com",
            "contentText":"按主题或时间整理","excerpt":[]})).unwrap();
        let prepared = context::build("人格", "怎样整理？", &[], &[record.clone()], &["rec-selected".into()], None, &[], &WorkingContext::default(), "");
        let index = ReadIndex::new(&[record], &["rec-selected".into()]);
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let result = tauri::async_runtime::block_on(explore_references(|_| {}, &reqwest::Client::new(), &config, "test", &prepared, &index, true)).unwrap().unwrap();
        (result, server.join().unwrap())
    }

    #[test]
    fn valid_choice_pauses_without_final_answer_and_persists_open() {
        let args = choice_args().to_string();
        let reply = serde_json::json!({"choices":[{"message":{"role":"assistant","content":"不应展示这段方案正文","tool_calls":[
            {"id":"choice-1","type":"function","function":{"name":"request_choice","arguments":args}}]}}]});
        let (result, requests) = mock_choice_exploration(vec![reply]);
        assert_eq!(requests.len(), 1);
        assert_eq!(result.messages.len(), 3);
        let draft = result.choice.unwrap();
        let mut session = new_session().unwrap();
        let assistant = save_choice_exchange(&mut session, "怎样整理？", draft, result.steps, "100", 123);
        assert_eq!(assistant.paragraphs, vec!["选哪种整理方式？", "两种做法会影响后续结构。"]);
        assert!(!assistant.paragraphs.join(" ").contains("不应展示"));
        assert!(assistant.citations.is_empty());
        assert!(assistant.tool_steps.iter().any(|step| step.contains("等待用户选择")));
        assert_eq!(session.pending_choice.as_ref().unwrap().status, "open");
        assert_eq!(session.pending_choice.as_ref().unwrap().message_id, assistant.id);
        let dir = std::env::temp_dir().join(format!("gearnote-choice-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(crate::db::FILE_NAME);
        let store = ChatStore { active_id: session.id.clone(), sessions: vec![session], project_memory: Vec::new() };
        crate::db::write_chat(&path, &store).unwrap();
        let restored = crate::db::read_chat(&path).unwrap().unwrap();
        assert_eq!(restored.sessions[0].pending_choice.as_ref().unwrap().status, "open");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_choice_and_second_request_are_returned_as_errors() {
        let mut invalid = choice_args();
        invalid["options"] = serde_json::json!([{"id":"only","label":"唯一","detail":""}]);
        let call = |id: &str, args: serde_json::Value| serde_json::json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[
            {"id":id,"type":"function","function":{"name":"request_choice","arguments":args.to_string()}}]}}]});
        let (result, requests) = mock_choice_exploration(vec![call("first", invalid), call("second", choice_args()),
            serde_json::json!({"choices":[{"message":{"role":"assistant","content":"直接回答"}}]})]);
        assert_eq!(requests.len(), 3);
        assert!(result.choice.is_none());
        assert!(result.messages.iter().filter(|item| item["role"] == "tool").all(|item| item["content"].as_str().unwrap().contains("error")));
        assert!(result.messages.last().unwrap()["content"].as_str().unwrap().contains("回答原问题"));
        let mut duplicate = choice_args(); duplicate["options"][1]["id"] = serde_json::json!("by_topic");
        assert!(validate_choice(&duplicate, false, false).is_err());
        let mut bad_recommendation = choice_args(); bad_recommendation["recommended"] = serde_json::json!("missing");
        assert!(validate_choice(&bad_recommendation, false, false).is_err());
        let mut forbidden = choice_args(); forbidden["options"][0]["detail"] = serde_json::json!("立即修改笔记");
        assert!(validate_choice(&forbidden, false, false).is_err());
        let mut too_long = choice_args(); too_long["prompt"] = serde_json::json!("过".repeat(41));
        assert!(validate_choice(&too_long, false, false).is_err());
        assert!(validate_choice(&choice_args(), false, true).is_err());
    }

    #[test]
    fn two_choice_calls_in_one_model_reply_are_rejected() {
        let args = choice_args().to_string();
        let reply = serde_json::json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[
            {"id":"first","type":"function","function":{"name":"request_choice","arguments":args}},
            {"id":"second","type":"function","function":{"name":"request_choice","arguments":choice_args().to_string()}}]}}]});
        let (result, requests) = mock_choice_exploration(vec![reply, serde_json::json!({"choices":[{"message":{"role":"assistant","content":"直接回答"}}]})]);
        assert_eq!(requests.len(), 2);
        assert!(result.choice.is_none());
        assert_eq!(result.messages.iter().filter(|item| item["role"] == "tool" && item["content"].as_str().unwrap().contains("error")).count(), 2);
    }

    #[test]
    fn selected_and_custom_choice_are_recorded_and_disable_choice_tool() {
        let mut session = new_session().unwrap();
        save_choice_exchange(&mut session, "怎样整理？", validate_choice(&choice_args(), false, false).unwrap(), vec!["等待用户选择".into()], "100", 10);
        let open = session.pending_choice.as_ref().unwrap();
        let choice_id = open.id.clone();
        assert!(validate_choice_submission(&session, "选择：按主题", Some(&choice_id)).is_ok());
        assert!(validate_choice_submission(&session, "选择：不存在", Some(&choice_id)).is_err());
        let instruction = choice_system_instruction(open, "选择：按主题", Some(&choice_id));
        assert!(instruction.contains("ID=by_topic"));
        assert!(instruction.contains("禁止调用 request_choice"));
        let config = AgentConfig::default();
        let payload = exploration_payload(&config, &[], "required", false);
        assert!(payload["tools"].as_array().unwrap().iter().all(|tool| tool["function"]["name"] != "request_choice"));
        answer_pending_choice(&mut session, "选择：按主题", "200", Some(&choice_id));
        let selected = session.pending_choice.as_ref().unwrap();
        assert_eq!(selected.status, "answered");
        assert_eq!(selected.selected_id.as_deref(), Some("by_topic"));
        assert!(selected.custom_text.is_none());
        assert!(validate_choice_submission(&session, "选择：按主题", Some(&choice_id)).is_err());
        session.pending_choice.as_mut().unwrap().status = "open".into();
        answer_pending_choice(&mut session, "选择：按主题", "300", None);
        let custom = session.pending_choice.as_ref().unwrap();
        assert_eq!(custom.custom_text.as_deref(), Some("选择：按主题"));
        assert!(custom.selected_id.is_none());
        assert!(choice_system_instruction(custom, "选择：按主题", None).contains("自定义回答原文"));
        let mut legacy = serde_json::to_value(&session).unwrap();
        legacy.as_object_mut().unwrap().remove("pendingChoice");
        let loaded: ChatSession = serde_json::from_value(legacy).unwrap();
        assert!(loaded.pending_choice.is_none());
    }
    #[test]
    fn followup_suggestions_are_bounded_and_old_messages_still_load() {
        let parsed = parse_chat_suggestions(r#"{"suggestions":[{"label":"比较资料","prompt":"比较这两条资料的差异"},{"label":"换个说法","prompt":"比较这两条资料的差异"}]}"#).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parse_chat_suggestions(r#"{"suggestions":[{"label":"运行命令","prompt":"第一行\n第二行"}]}"#).is_err());
        let old: ChatMessage = serde_json::from_value(serde_json::json!({
            "id":"old","role":"agent","createdAt":"1","paragraphs":["旧回答"]
        })).unwrap();
        assert!(old.suggestions.is_empty());
    }
    #[test]
    fn automatic_compaction_waits_for_a_meaningful_batch() {
        let mut session = new_session().unwrap();
        for index in 0..15 {
            session.messages.push(ChatMessage { id: format!("msg-{index}"), role: "user".into(), created_at: "1".into(),
                paragraphs: vec!["短消息".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: Vec::new(), outcome: None });
        }
        assert!(!should_auto_compact(&session));
        session.messages.push(ChatMessage { id: "msg-15".into(), role: "user".into(), created_at: "1".into(),
            paragraphs: vec!["短消息".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: Vec::new(), outcome: None });
        assert!(should_auto_compact(&session));
        session.compacted_count = 8;
        assert!(!should_auto_compact(&session));

        session.compacted_count = 0;
        session.messages.truncate(10);
        for message in &mut session.messages { message.paragraphs = vec!["长内容".repeat(120)]; }
        assert!(should_auto_compact(&session));
    }
    #[test]
    fn selected_messages_are_scoped_to_current_session() {
        let mut session = new_session().unwrap();
        session.messages.push(ChatMessage { id: "msg-1".into(), role: "user".into(), created_at: "1".into(), paragraphs: vec!["原话".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: Vec::new(), outcome: None });
        assert_eq!(selected_messages(&session, &["msg-1".into()]).unwrap().len(), 1);
        assert!(selected_messages(&session, &["msg-other-session".into()]).is_err());
        assert!(selected_messages(&session, &["msg-1".into(), "msg-1".into()]).is_err());
    }
    #[test]
    fn chat_source_citation_keeps_message_kind() {
        let source = SourceChunk { record_id: "chat:msg-1".into(), title: "对话摘录 / 用户消息".into(), source_url: String::new(), captured_at: String::new(), at_seconds: None, text: "原话".into(), search_text: String::new(), score: 0, visual: false };
        let found = citations_from_answer("用户先前说过原话。「原话」[对话1]", &[source]);
        assert_eq!(found[0].kind.as_deref(), Some("chat"));
        assert_eq!(found[0].label.as_deref(), Some("[对话1]"));
    }
    #[test]
    fn request_uses_image_part_only_when_selected() {
        assert_eq!(request_user_content("问题".into(), None), serde_json::json!("问题"));
        let content = request_user_content("看图".into(), Some("data:image/jpeg;base64,abc"));
        assert_eq!(content[0], serde_json::json!({"type":"text","text":"看图"}));
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[1]["image_url"]["url"], "data:image/jpeg;base64,abc");
    }
    #[test]
    fn capture_command_accepts_one_explicit_time() {
        assert_eq!(capture_time("截取 01:20 的画面").unwrap().unwrap(), 80);
        assert_eq!(capture_time("请截图 01:02:03").unwrap().unwrap(), 3723);
        assert!(capture_time("请解释 01:20 的画面").is_none());
        assert!(capture_time("截取 01:80 的画面").unwrap().is_err());
        assert!(capture_time("截取 00:20 和 00:30").unwrap().is_err());
    }
    #[test]
    fn legacy_chat_migrates_without_losing_messages() {
        let dir = std::env::temp_dir().join(format!("knowflow-sessions-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("chat.json");
        let sessions = dir.join("chat-sessions.json");
        let old = ChatMessage { id: "msg-old".into(), role: "user".into(), created_at: "1".into(), paragraphs: vec!["旧对话问题".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: Vec::new(), outcome: None };
        fs::write(&legacy, serde_json::to_vec(&vec![old]).unwrap()).unwrap();
        let store = load_chat_store(&sessions, &legacy).unwrap();
        assert_eq!(store.sessions.len(), 1);
        assert_eq!(store.sessions[0].messages[0].id, "msg-old");
        assert_eq!(store.sessions[0].title, "旧对话问题");
        assert!(legacy.exists());
        let mut next = store.clone();
        let fresh = new_session().unwrap();
        next.active_id = fresh.id.clone();
        next.sessions.push(fresh);
        persist_chat_store(&sessions, &next).unwrap();
        let reloaded = load_chat_store(&sessions, &legacy).unwrap();
        assert!(state_view(&reloaded).unwrap().messages.is_empty());
        assert_eq!(reloaded.sessions[0].messages[0].id, "msg-old");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn sqlite_migration_keeps_legacy_file_and_subsequent_changes() {
        let dir = std::env::temp_dir().join(format!("knowflow-db-migration-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let sessions = dir.join("chat-sessions.json");
        let legacy = dir.join("chat.json");
        let database = dir.join(crate::db::FILE_NAME);
        let mut old = new_session().unwrap();
        old.messages.push(ChatMessage { id: "kept".into(), role: "user".into(), created_at: "1".into(), paragraphs: vec!["旧问题".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: Vec::new(), outcome: None });
        let store = ChatStore { active_id: old.id.clone(), sessions: vec![old], project_memory: Vec::new() };
        fs::write(&sessions, serde_json::to_vec(&store).unwrap()).unwrap();
        let loaded = load_chat_store(&database, &legacy).unwrap();
        assert!(sessions.exists());
        assert_eq!(loaded.sessions[0].messages[0].id, "kept");
        let mut next = loaded.clone();
        next.sessions[0].title = "新标题".into();
        persist_chat_store(&database, &next).unwrap();
        assert_eq!(load_chat_store(&database, &legacy).unwrap().sessions[0].title, "新标题");
        assert_eq!(load_chat_store(&sessions, &legacy).unwrap().sessions[0].title, "新会话");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn recent_context_uses_only_active_session() {
        let mut first = new_session().unwrap();
        first.messages.push(ChatMessage { id: "first".into(), role: "user".into(), created_at: "1".into(), paragraphs: vec!["只属于会话甲".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: Vec::new(), outcome: None });
        let mut second = new_session().unwrap();
        second.messages.push(ChatMessage { id: "second".into(), role: "user".into(), created_at: "2".into(), paragraphs: vec!["只属于会话乙".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: Vec::new(), outcome: None });
        let store = ChatStore { active_id: second.id.clone(), sessions: vec![first, second], project_memory: Vec::new() };
        let active = store.sessions.iter().find(|item| item.id == store.active_id).unwrap();
        let context = context::build("人格", "问题", &active.messages, &[], &[], None, &[], &WorkingContext::default(), "").user_text;
        assert!(context.contains("只属于会话乙"));
        assert!(!context.contains("只属于会话甲"));
    }
    #[test]
    fn older_sessions_load_with_empty_memory_and_work_state() {
        let old = serde_json::json!({"activeId":"chat-old","sessions":[{
            "id":"chat-old","title":"旧会话","createdAt":"1","updatedAt":"1",
            "referencedIds":[],"messages":[]
        }]});
        let store: ChatStore = serde_json::from_value(old).unwrap();
        assert_eq!(store.sessions[0].project_id, "project-default");
        assert!(store.sessions[0].working_context.goal.is_empty());
        assert!(store.project_memory.is_empty());
    }
    #[test]
    fn duplicate_followup_requests_share_one_inflight_slot() {
        let requests = SuggestionRequests::default();
        let first = requests.begin("chat-a", "message-a").unwrap().unwrap();
        assert!(requests.begin("chat-a", "message-a").unwrap().is_none());
        let _second = requests.begin("chat-a", "message-b").unwrap().unwrap();
        assert_eq!(requests.len(), 2);
        drop(first);
        assert_eq!(requests.len(), 1);
        assert!(requests.begin("chat-a", "message-a").unwrap().is_some());
    }

    #[test]
    fn outcome_gate_blocks_failed_and_non_answer_messages_from_suggestions() {
        let base = ChatMessage { id: "m".into(), role: "agent".into(), created_at: "1".into(),
            paragraphs: vec!["正文".into()], tool_steps: vec![], citations: vec![], frame: None, run: None,
            suggestions: Vec::new(), outcome: None };
        // 旧消息保持原行为。
        assert!(outcome_allows_suggestions(&base));
        let with = |kind: MessageKind, answer: AnswerStatus| ChatMessage {
            outcome: Some(MessageOutcome { kind, answer, answer_reason: None, reading_reason: None, notices: Vec::new() }), ..base.clone() };
        assert!(outcome_allows_suggestions(&with(MessageKind::Answer, AnswerStatus::Completed)));
        assert!(outcome_allows_suggestions(&with(MessageKind::Answer, AnswerStatus::Partial)));
        assert!(!outcome_allows_suggestions(&with(MessageKind::Answer, AnswerStatus::Failed)));
        assert!(!outcome_allows_suggestions(&with(MessageKind::Choice, AnswerStatus::NotApplicable)));
        assert!(!outcome_allows_suggestions(&with(MessageKind::LibraryProposal, AnswerStatus::NotApplicable)));
        assert!(!outcome_allows_suggestions(&with(MessageKind::NoteProposal, AnswerStatus::NotApplicable)));
        assert!(!outcome_allows_suggestions(&with(MessageKind::CommandProposal, AnswerStatus::NotApplicable)));
        assert!(!outcome_allows_suggestions(&with(MessageKind::Frame, AnswerStatus::NotApplicable)));
    }

    #[test]
    fn message_outcome_roundtrips_and_legacy_messages_still_load() {
        let outcome = MessageOutcome { kind: MessageKind::Answer, answer: AnswerStatus::Partial,
            answer_reason: Some(AnswerEndReason::StreamInterrupted), reading_reason: Some(ReadingEndReason::UserStopped),
            notices: vec![MessageNotice { code: "answer_warning".into(), message: "任意展示文案，可修改".into() }] };
        let message = ChatMessage { id: "msg-outcome".into(), role: "agent".into(), created_at: "1".into(),
            paragraphs: vec!["部分回答已保留".into()], tool_steps: vec!["回答提醒：旧文案".into()], citations: vec![],
            frame: None, run: None, suggestions: Vec::new(), outcome: Some(outcome) };
        let mut encoded = serde_json::to_value(&message).unwrap();
        let restored: ChatMessage = serde_json::from_value(encoded.clone()).unwrap();
        let restored_outcome = restored.outcome.unwrap();
        assert_eq!(restored_outcome.answer, AnswerStatus::Partial);
        assert_eq!(restored_outcome.answer_reason, Some(AnswerEndReason::StreamInterrupted));
        assert_eq!(restored_outcome.reading_reason, Some(ReadingEndReason::UserStopped));
        assert_eq!(restored_outcome.notices[0].code, "answer_warning");
        // 修改中文展示文案不改变结构化状态。
        encoded["toolSteps"] = serde_json::json!(["完全不同的新文案"]);
        encoded["outcome"]["notices"][0]["message"] = serde_json::json!("translated notice");
        let edited: ChatMessage = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(edited.outcome.unwrap().answer, AnswerStatus::Partial);
        // 旧消息没有 outcome 字段仍可加载，且不得被推断为成功或失败。
        encoded.as_object_mut().unwrap().remove("outcome");
        let legacy: ChatMessage = serde_json::from_value(encoded).unwrap();
        assert!(legacy.outcome.is_none());
        assert_eq!(legacy.paragraphs, vec!["部分回答已保留"]);
    }

    #[test]
    fn outcome_persists_through_chat_store_and_state_view_exposes_continuation() {
        let mut session = new_session().unwrap();
        let outcome = MessageOutcome { kind: MessageKind::Answer, answer: AnswerStatus::Completed,
            answer_reason: Some(AnswerEndReason::Normal), reading_reason: Some(ReadingEndReason::UserStopped), notices: Vec::new() };
        let message = ChatMessage { id: "msg-persisted".into(), role: "agent".into(), created_at: "1".into(),
            paragraphs: vec!["停止阅读后仍完成的回答".into()], tool_steps: vec![], citations: vec![],
            frame: None, run: None, suggestions: Vec::new(), outcome: Some(outcome) };
        session.messages.push(message.clone());
        session.referenced_ids = vec!["record-a".into()];
        session.read_continuation = Some(ReadContinuation { message_id: message.id.clone(), question: "问题".into(),
            referenced_ids: vec!["record-a".into()], fingerprint: "fp".into(),
            progress: ReadProgress { read_chunk_ids: vec!["chunk-1".into()], ..ReadProgress::default() } });
        let dir = std::env::temp_dir().join(format!("gearnote-outcome-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("knowflow.db");
        let store = ChatStore { active_id: session.id.clone(), sessions: vec![session], project_memory: vec![] };
        persist_chat_store(&path, &store).unwrap();
        let loaded = load_chat_store(&path, &dir.join("legacy.json")).unwrap();
        let saved_outcome = loaded.sessions[0].messages[0].outcome.clone().unwrap();
        // 用户停止读取后回答仍正常完成：不得分类为失败。
        assert_eq!(saved_outcome.answer, AnswerStatus::Completed);
        assert_eq!(saved_outcome.reading_reason, Some(ReadingEndReason::UserStopped));
        let view = state_view(&loaded).unwrap();
        let summary = view.read_continuation.unwrap();
        assert_eq!(summary.message_id, "msg-persisted");
        assert_eq!(summary.mode, "adaptive");
        assert!(summary.has_progress);
        // 空进度不应声称可续读。
        let mut empty = loaded.clone();
        empty.sessions[0].read_continuation.as_mut().unwrap().progress = ReadProgress::default();
        assert!(!state_view(&empty).unwrap().read_continuation.unwrap().has_progress);
        // 已有更新消息时不应声称可续读。
        let mut newer = loaded.clone();
        newer.sessions[0].messages.push(ChatMessage { id: "msg-newer".into(), role: "user".into(), created_at: "2".into(),
            paragraphs: vec!["新问题".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: vec![], outcome: None });
        assert!(!state_view(&newer).unwrap().read_continuation.unwrap().has_progress);
        // 参考失效时不应声称可续读。
        let mut switched = loaded.clone();
        switched.sessions[0].referenced_ids = vec!["record-b".into()];
        assert!(!state_view(&switched).unwrap().read_continuation.unwrap().has_progress);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn has_progress_hides_when_record_content_fingerprint_changes() {
        let record = budget_record();
        let mut session = new_session().unwrap();
        let message = ChatMessage { id: "msg-fp".into(), role: "agent".into(), created_at: "1".into(),
            paragraphs: vec!["回答".into()], tool_steps: vec![], citations: vec![], frame: None, run: None, suggestions: vec![], outcome: None };
        session.messages.push(message.clone());
        session.referenced_ids = vec![record.id.clone()];
        session.read_continuation = Some(ReadContinuation { message_id: message.id.clone(), question: "问".into(),
            referenced_ids: vec![record.id.clone()], fingerprint: reading_fingerprint(&[record.clone()]),
            progress: ReadProgress { read_chunk_ids: vec!["c1".into()], ..ReadProgress::default() } });
        let store = ChatStore { active_id: session.id.clone(), sessions: vec![session], project_memory: vec![] };
        // 正文未改动：按钮可显示。
        assert!(state_view_with_records(&store, Some(&[record.clone()])).unwrap().read_continuation.unwrap().has_progress);
        // 正文被改动（如重新捕获）：指纹失效，按钮应隐藏，与点击续读入口的拒绝一致。
        let mut changed = record.clone();
        changed.content_text = "重新捕获后的新正文".into();
        assert!(!state_view_with_records(&store, Some(&[changed])).unwrap().read_continuation.unwrap().has_progress);
        // 引用资料被删除：重算指纹不同，同样隐藏。
        assert!(!state_view_with_records(&store, Some(&[])).unwrap().read_continuation.unwrap().has_progress);
        // 无当前资料信息时仅做结构检查（其余返回 ChatState 的路径），不误判。
        assert!(state_view(&store).unwrap().read_continuation.unwrap().has_progress);
    }
