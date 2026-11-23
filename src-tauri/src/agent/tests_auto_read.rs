fn auto_task(budget: ReadingBudget) -> ReadingTask {
    ReadingTask::new(Arc::new(ReadingControl::new("test-reading".into())), budget)
}

#[test]
fn reading_policy_defaults_migrate_and_roundtrip() {
    let old: ReadingBudget = serde_json::from_str(r#"{"toolCalls":6,"deepSeconds":120}"#).unwrap();
    let standard = old.effective();
    assert_eq!(standard.strategy, ReadingStrategy::Adaptive);
    assert_eq!(standard.tool_calls, 24);
    assert_eq!(standard.exploration_rounds, 12);
    let custom = ReadingBudget { effort: ReadingEffort::Custom, tool_calls: 9, ..old.clone() };
    assert_eq!(custom.effective().tool_calls, 9);
    let thorough = ReadingBudget { strategy: ReadingStrategy::Full, effort: ReadingEffort::Thorough, ..old }.effective();
    let loaded: ReadingBudget = serde_json::from_str(&serde_json::to_string(&thorough).unwrap()).unwrap();
    assert_eq!(loaded.strategy, ReadingStrategy::Full);
    assert_eq!(loaded.tool_calls, 48);
    assert_eq!(loaded.deep_seconds, 600);
}

#[test]
fn stopping_reading_wakes_an_inflight_future_immediately() {
    let task = auto_task(ReadingBudget::default().effective());
    let control = task.control.clone();
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30)); control.stop();
    });
    let start = Instant::now();
    let result = tauri::async_runtime::block_on(task.wait(std::future::pending::<Result<(), String>>()));
    stopper.join().unwrap();
    assert!(result.unwrap_err().contains("用户停止读取"));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(task.requests.load(Ordering::SeqCst), 0);
}

#[test]
fn adaptive_reader_continues_past_six_calls_and_stops_when_enough() {
    let record = budget_record();
    let ids = vec![record.id.clone()];
    let index = ReadIndex::new(&[record.clone()], &ids);
    let mut replies = index.entries().iter().take(7).enumerate().map(|(number, (id, _))| serde_json::json!({"choices":[{"message":{
        "role":"assistant","tool_calls":[{"id":format!("call-{number}"),"type":"function",
        "function":{"name":"read_reference","arguments":serde_json::json!({"chunk_id":id}).to_string()}}]}}]})).collect::<Vec<_>>();
    replies.push(serde_json::json!({"choices":[{"message":{"role":"assistant","content":"信息已经足够，准备回答"}}]}));
    let (url, server) = budget_mock(replies);
    let mut config = AgentConfig { base_url: url, model: "mock".into(), ..AgentConfig::default() };
    config.reading_budget = config.reading_budget.effective();
    let task = auto_task(config.reading_budget.clone());
    let prepared = context::build_exploration("人格", "解释这个案例", &[], 1, index.directory(&[record], &ids, 6000), index.len(), &[], &WorkingContext::default(), "");
    let result = tauri::async_runtime::block_on(explore_references_task(|_| {}, &reqwest::Client::new(), &config, "test", &prepared, &index, true, None, Some(&task))).unwrap().unwrap();
    assert_eq!(server.join().unwrap().len(), 8);
    assert_eq!(result.sources.len(), 7);
    assert_eq!(task.requests.load(Ordering::SeqCst), 8);
    assert_eq!(task.calls.load(Ordering::SeqCst), 7);
}

#[test]
fn stopping_between_tools_keeps_protocol_balanced_for_final_answer() {
    let record = budget_record();
    let ids = vec![record.id.clone()];
    let index = ReadIndex::new(&[record], &ids);
    let calls = index.entries().iter().take(2).enumerate().map(|(number, (id, _))| serde_json::json!({"id":format!("call-{number}"),"type":"function",
        "function":{"name":"read_reference","arguments":serde_json::json!({"chunk_id":id}).to_string()}})).collect::<Vec<_>>();
    let (url, server) = budget_mock(vec![serde_json::json!({"choices":[{"message":{"role":"assistant","tool_calls":calls}}]})]);
    let mut config = AgentConfig { base_url: url, model: "mock".into(), ..AgentConfig::default() };
    config.reading_budget = config.reading_budget.effective();
    let task = auto_task(config.reading_budget.clone());
    let prepared = context::build_exploration("人格", "解释案例", &[], 1, "选中资料目录".into(), index.len(), &[], &WorkingContext::default(), "");
    let result = tauri::async_runtime::block_on(explore_references_task(|message| {
        if message.contains("来源 [1]") { task.control.stop(); }
    }, &reqwest::Client::new(), &config, "test", &prepared, &index, true, None, Some(&task))).unwrap().unwrap();
    assert_eq!(server.join().unwrap().len(), 1);
    assert_eq!(result.sources.len(), 1);
    let replies = result.messages.iter().filter(|message| message["role"] == "tool").collect::<Vec<_>>();
    assert_eq!(replies.len(), 2);
    assert!(replies[1]["content"].as_str().unwrap().contains("未执行"));
    assert!(result.messages.last().unwrap()["content"].as_str().unwrap().contains("回答原问题"));
}

#[test]
fn stopping_before_first_batch_produces_an_answer_context_without_panic() {
    let mut config = AgentConfig::default(); config.reading_budget = config.reading_budget.effective();
    let task = auto_task(config.reading_budget.clone()); task.control.stop();
    let record = budget_record();
    let prepared = context::build_exploration("人格", "问题", &[], 1, "选中视频".into(), 8, &[], &WorkingContext::default(), "");
    let (answer, coverage) = tauri::async_runtime::block_on(explore_video_deep_task(|_| {}, &config, "test", &prepared, &record, "问题", None, Some(&task))).unwrap();
    assert_eq!(coverage.batch_count, 0);
    assert!(!coverage.complete);
    assert!(coverage.reason.contains("用户停止"));
    assert!(answer.sources.is_empty());
    assert!(answer.progress.is_some());
    assert_eq!(task.requests.load(Ordering::SeqCst), 0);
}

#[test]
fn full_reader_ignores_enough_and_never_reads_unselected_or_failed_material() {
    let record = budget_record();
    let mut unselected = record.clone(); unselected.id = "secret-unselected".into();
    let mut failed = record.clone(); failed.id = "failed".into(); failed.status = ProcessStatus::Fault;
    let ids = vec![record.id.clone(), failed.id.clone()];
    let index = ReadIndex::for_full_read(&[record, unselected, failed], &ids);
    assert!(index.len() > 4);
    let checkpoint = serde_json::json!({"choices":[{"message":{"content":"{\"enough\":true,\"notes\":[]}"}}]});
    let (url, server) = budget_mock(vec![checkpoint; index.len().div_ceil(4)]);
    let mut config = AgentConfig { base_url: url, model: "mock".into(), ..AgentConfig::default() };
    config.reading_budget = config.reading_budget.effective(); config.reading_budget.strategy = ReadingStrategy::Full;
    let task = auto_task(config.reading_budget.clone());
    let prepared = context::build_exploration("人格", "解释这个案例", &[], 1, "已选资料目录".into(), index.len(), &[], &WorkingContext::default(), "");
    let result = tauri::async_runtime::block_on(read_full_references(|_| {}, &reqwest::Client::new(), &config, "test", &prepared, &index, None, &task)).unwrap();
    let requests = server.join().unwrap();
    assert!(requests.len() >= 2);
    assert_eq!(result.sources.len(), index.len());
    assert!(result.progress.is_none());
    assert!(result.sources.iter().all(|source| source.record_id == "video-budget"));
    assert!(!serde_json::to_string(&requests).unwrap().contains("secret-unselected"));
    assert!(result.steps.last().unwrap().contains("已读完"));
}

#[test]
fn full_reader_preserves_progress_on_request_budget_and_user_stop() {
    for stop_manually in [false, true] {
        let record = budget_record();
        let index = ReadIndex::for_full_read(&[record.clone()], &[record.id]);
        let (url, server) = budget_mock(vec![serde_json::json!({"choices":[{"message":{"content":"{\"enough\":false,\"notes\":[]}"}}]})]);
        let mut config = AgentConfig { base_url: url, model: "mock".into(), ..AgentConfig::default() };
        config.reading_budget = config.reading_budget.effective();
        if !stop_manually { config.reading_budget.exploration_rounds = 1; }
        let task = auto_task(config.reading_budget.clone());
        let prepared = context::build_exploration("人格", "完整阅读", &[], 1, "已选资料目录".into(), index.len(), &[], &WorkingContext::default(), "");
        let result = tauri::async_runtime::block_on(read_full_references(|message| {
            if stop_manually && message.contains("第 5") { task.control.stop(); }
        }, &reqwest::Client::new(), &config, "test", &prepared, &index, None, &task)).unwrap();
        assert_eq!(server.join().unwrap().len(), 1);
        assert_eq!(result.sources.len(), 4);
        assert_eq!(result.progress.unwrap().read_chunk_ids.len(), 4);
        let text = serde_json::to_string(&result.messages).unwrap();
        assert!(text.contains("已处理 4 / 8"));
        assert!(text.contains(if stop_manually { "用户停止读取" } else { "达到整项模型请求预算" }));
    }
}

#[test]
fn deep_full_reading_does_not_stop_at_model_enough() {
    let checkpoint = serde_json::json!({"choices":[{"message":{"content":"{\"enough\":true,\"notes\":[]}"}}]});
    let (url, server) = budget_mock(vec![checkpoint.clone(), checkpoint]);
    let mut config = AgentConfig { base_url: url, model: "mock".into(), ..AgentConfig::default() };
    config.reading_budget = config.reading_budget.effective(); config.reading_budget.strategy = ReadingStrategy::Full;
    let task = auto_task(config.reading_budget.clone());
    let record = budget_record();
    let prepared = context::build_exploration("人格", "开头讲什么", &[], 1, "选中的视频".into(), 8, &[], &WorkingContext::default(), "");
    let (_, coverage) = tauri::async_runtime::block_on(explore_video_deep_task(|_| {}, &config, "test", &prepared, &record, "开头讲什么", None, Some(&task))).unwrap();
    assert_eq!(server.join().unwrap().len(), 2);
    assert!(coverage.complete);
    assert_eq!(coverage.batch_count, 8);
}
