fn budget_record() -> KnowledgeRecord {
    serde_json::from_value(serde_json::json!({"id":"video-budget","type":"video","title":"预算测试",
        "site":"example.com","sourceUrl":"https://example.com/video","savedAt":"1","status":"ready",
        "contentText":"","excerpt":[],"durationSeconds":800,
        "transcript":(0..8).map(|number| serde_json::json!({"atSeconds":number * 100,
            "text":format!("第{number}批字幕。{}", "测试".repeat(1450))})).collect::<Vec<_>>() })).unwrap()
}

// 有限等待的本机模型替身，避免测试失败时永久等待下一条请求。
fn budget_mock(replies: Vec<serde_json::Value>) -> (String, std::thread::JoinHandle<Vec<serde_json::Value>>) {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for reply in replies {
            let start = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && start.elapsed().as_secs() < 10 => std::thread::sleep(Duration::from_millis(5)),
                    Err(error) => panic!("mock connection: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new(); reader.read_line(&mut line).unwrap();
            let mut length = 0;
            loop {
                line.clear(); reader.read_line(&mut line).unwrap();
                if line == "\r\n" { break; }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") { length = value.trim().parse().unwrap(); }
            }
            let mut bytes = vec![0; length]; reader.read_exact(&mut bytes).unwrap();
            requests.push(serde_json::from_slice(&bytes).unwrap());
            let body = reply.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        }
        requests
    });
    (url, server)
}

#[test]
fn old_config_and_session_load_and_continuation_survives_persistence() {
    let config: AgentConfig = serde_json::from_str(r#"{"baseUrl":"https://example.com","model":"mock"}"#).unwrap();
    assert_eq!(config.reading_budget.tool_calls, 6);
    let mut budget = config.reading_budget.clone(); budget.tool_calls = 0;
    assert!(budget.validate().is_err());
    let mut session = new_session().unwrap();
    let mut legacy = serde_json::to_value(&session).unwrap();
    legacy.as_object_mut().unwrap().remove("readContinuation");
    assert!(serde_json::from_value::<ChatSession>(legacy).unwrap().read_continuation.is_none());
    let message = save_final_failure_exchange(&mut session, "总结", "空回答", AnswerEndReason::EmptyAnswer, "1", 0, None, vec![], vec![], None, None);
    session.read_continuation = Some(ReadContinuation { message_id: message.id.clone(), question: "总结".into(),
        referenced_ids: vec!["video-budget".into()], fingerprint: reading_fingerprint(&[budget_record()]),
        progress: ReadProgress { deep: true, batch_count: 4, ..ReadProgress::default() } });
    let dir = std::env::temp_dir().join(format!("gearnote-budget-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("knowflow.db");
    let store = ChatStore { active_id: session.id.clone(), sessions: vec![session], project_memory: vec![] };
    persist_chat_store(&path, &store).unwrap();
    let loaded = load_chat_store(&path, &dir.join("legacy.json")).unwrap();
    let resumed = continuation_for(&loaded.sessions[0], &message.id, &["video-budget".into()]).unwrap();
    assert_eq!(resumed.progress.batch_count, 4);
    assert!(continuation_for(&loaded.sessions[0], &message.id, &["different".into()]).is_err());
    assert!(continuation_for(&loaded.sessions[0], "stale-message", &["video-budget".into()]).is_err());
    let mut changed = budget_record(); changed.transcript.as_mut().unwrap()[0].text = "修改后的正文".into();
    assert_ne!(resumed.fingerprint, reading_fingerprint(&[changed]));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deep_continuation_reads_next_batches_with_new_budget() {
    let checkpoint = serde_json::json!({"choices":[{"message":{"content":"{\"enough\":false,\"notes\":[]}"}}]});
    let (url, server) = budget_mock(vec![checkpoint.clone(), checkpoint]);
    let mut config = AgentConfig { base_url: url, model: "mock".into(), ..AgentConfig::default() };
    config.reading_budget.deep_batches = 4;
    let record = budget_record();
    let prepared = context::build_exploration("人格", "总结一下", &[], 1, "深度模式".into(), 8, &[], &WorkingContext::default(), "");
    let (first, coverage) = tauri::async_runtime::block_on(explore_video_deep_resume(|_| {}, &config, "test", &prepared, &record, "总结一下", None)).unwrap();
    assert_eq!(coverage.batch_count, 4);
    assert!(!coverage.complete);
    let (second, coverage) = tauri::async_runtime::block_on(explore_video_deep_resume(|_| {}, &config, "test", &prepared, &record, "总结一下", first.progress.as_ref())).unwrap();
    assert_eq!(coverage.batch_count, 8);
    assert!(coverage.complete);
    assert!(second.progress.is_none());
    let requests = server.join().unwrap();
    let prompt = requests[1]["messages"][1]["content"].as_str().unwrap();
    assert!(prompt.contains("[5]"));
    assert!(!prompt.contains("[1]"));
}

#[test]
fn ordinary_budget_can_exceed_six_and_read_index_skips_consumed_chunks() {
    let record = budget_record();
    let ids = vec![record.id.clone()];
    let mut index = ReadIndex::new(&[record.clone()], &ids);
    let old_id = index.entries()[0].0.clone();
    index.exclude_read(&[old_id.clone()]);
    assert!(index.read(&old_id).is_err());
    let next_id = index.entries()[0].0.clone();
    let calls = (0..7).map(|number| serde_json::json!({"id":format!("read-{number}"),"type":"function",
        "function":{"name":"read_reference","arguments":serde_json::json!({"chunk_id":next_id}).to_string()}})).collect::<Vec<_>>();
    let (url, server) = budget_mock(vec![serde_json::json!({"choices":[{"message":{"role":"assistant","tool_calls":calls}}]})]);
    let mut config = AgentConfig { base_url: url, model: "mock".into(), ..AgentConfig::default() };
    config.reading_budget.tool_calls = 8;
    config.reading_budget.exploration_rounds = 1;
    config.reading_budget.tool_chars = 16_000;
    let prepared = context::build_exploration("人格", "总结", &[], 1, index.directory(&[record], &ids, 6000), index.len(), &[], &WorkingContext::default(), "");
    let result = tauri::async_runtime::block_on(explore_references(|_| {}, &reqwest::Client::new(), &config, "test", &prepared, &index, true)).unwrap().unwrap();
    server.join().unwrap();
    assert_eq!(result.messages[2]["tool_calls"].as_array().unwrap().len(), 7);
    assert_eq!(result.progress.unwrap().read_chunk_ids.len(), 7);
    assert!(!result.sources.is_empty());
}
