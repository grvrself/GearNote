// Agent 既有测试的后半部分。
    #[test]
    fn citations_only_map_to_supplied_chunks() {
        let sources = vec![SourceChunk { record_id: "rec-real".into(), title: "真实资料".into(), source_url: "https://example.com".into(), captured_at: "2026-11-10".into(), at_seconds: Some(42), text: "内容".into(), search_text: "内容".into(), score: 1, visual: false }];
        let found = citations_from_answer("结论 [1]，伪造 [2]", &sources);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].record_id, "rec-real");
        assert_eq!(found[0].at_seconds, Some(42));
        assert!(validate_citation_numbers("结论 [1]", sources.len()).is_ok());
        assert!(validate_citation_numbers("结论 [1]，伪造 [2]", sources.len()).unwrap_err().contains("[2]"));
        assert_eq!(invalid_citation_numbers("数组 item[2] 与引用 [0]、[9]、[9]", 1), vec![0, 9]);
    }

    #[test]
    fn citation_problems_do_not_discard_generated_answer() {
        let sources = [evidence_source("rec", "视频", "原句确实存在。")];
        let (citations, notice) = assess_answer_evidence("我认为这段视频主要在介绍攻击方式。", &sources);
        assert!(citations.is_empty());
        assert!(notice.is_none());
        for answer in ["结论。『不在字幕中的引句』[1]", "结论。『原句确实存在。』[9]"] {
            let (citations, notice) = assess_answer_evidence(answer, &sources);
            assert!(notice.as_deref().is_some_and(|text| text.contains("模型原文已保留")));
            assert!(citations.iter().all(|citation| citation.record_id == "rec"));
        }
        let (citations, notice) = assess_answer_evidence("结论。『原句确实存在。』[1]", &sources);
        assert_eq!(citations.len(), 1);
        assert!(notice.is_none());
    }

    fn evidence_source(id: &str, title: &str, text: &str) -> SourceChunk {
        SourceChunk { record_id: id.into(), title: title.into(), source_url: String::new(), captured_at: String::new(),
            at_seconds: None, text: text.into(), search_text: text.into(), score: 1, visual: false }
    }

    #[test]
    fn exact_evidence_is_required_for_each_sourced_line() {
        let sources = [evidence_source("record-1", "资料", "系统在确认后保存配置。下一步重新启动服务。")];
        let valid = "配置会在确认后保存。「系统在确认后保存配置。」[1]\n下一步是重启服务。「下一步重新启动服务。」[1]";
        let citations = validate_answer_evidence(valid, &sources, false).unwrap();
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].source_index, Some(1));
        assert!(validate_answer_evidence("配置已保存。「不存在的原句」[1]", &sources, false).is_err());
        let spaced = [evidence_source("record-spaced", "资料", "系统 在确认后保存 配置。")];
        assert!(validate_answer_evidence("配置在确认后保存。「系统在确认后保存配置。」[1]", &spaced, false).is_ok());
        let wrong_source = [sources[0].clone(), evidence_source("record-2", "另一资料", "另一资料写了校验窗口。")];
        assert!(validate_answer_evidence("另一资料写了校验窗口。「另一资料写了校验窗口。」[1]", &wrong_source, false).is_err());
        let source_with_brackets = [evidence_source("record-2", "资料", "原文写了条目 [9] 的示例。")];
        assert!(validate_answer_evidence("资料给出了一个示例。「原文写了条目 [9] 的示例。」[1]", &source_with_brackets, false).is_ok());
        assert!(validate_answer_evidence("配置已保存。[1]", &sources, false).is_err());
        assert!(validate_answer_evidence("配置已保存。「系统在确认后保存配置。」[2]", &sources, false).is_err());
        assert!(validate_answer_evidence("配置已保存。", &sources, false).is_err());
        assert!(validate_answer_evidence("先保存。再重启。「系统在确认后保存配置。」[1]", &sources, false).is_err());
        assert!(validate_answer_evidence("配置已保存。「系统在确认后保存配置。」[1]\n还有一项无出处结论。", &sources, false).is_err());
        assert!(validate_answer_evidence("配置已保存。「系统在确认后保存配置。」[1]", &sources, true).is_ok());
    }

    #[test]
    fn insufficient_answer_cannot_hide_unsupported_claims() {
        let sources = [evidence_source("record-1", "资料", "原文只记录设备型号。")];
        assert!(validate_answer_evidence("资料不足：缺少电池容量。", &sources, false).unwrap().is_empty());
        assert!(validate_answer_evidence("资料不足，但电池容量是三千毫安时。", &sources, false).is_err());
        assert!(validate_answer_evidence("资料不足：缺少容量。\n电池容量是三千毫安时。", &sources, false).is_err());
        assert!(validate_answer_evidence("资料不足：缺少容量。电池容量是三千毫安时。", &sources, false).is_err());
        assert!(validate_answer_evidence("资料不足：缺少容量。[1]", &sources, false).is_err());
        assert!(validate_answer_evidence("资料不足：缺少容量。「原文只记录设备型号。」[1]", &sources, false).is_err());
    }

    #[test]
    fn chat_and_image_do_not_consume_document_numbers() {
        let chat = evidence_source("chat:msg-1", "对话摘录 / 用户消息", "用户说先做目录。 ");
        let document = evidence_source("record-1", "文章", "文章要求核对配置。 ");
        let image = SourceChunk::image("video-1".into(), "视频".into(), 80);
        let sources = [chat, image, document];
        let answer = "用户先前要求做目录。「用户说先做目录。」[对话1]\n文章要求核对配置。「文章要求核对配置。」[1]";
        let citations = validate_answer_evidence(answer, &sources, false).unwrap();
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].label.as_deref(), Some("[对话1]"));
        assert_eq!(citations[1].source_index, Some(1));
        assert!(validate_answer_evidence("文章要求核对配置。「文章要求核对配置。」[2]", &sources, false).is_err());
        assert!(validate_answer_evidence("Agent 说要做目录。「用户说先做目录。」[对话1]", &sources, false).is_err());
        assert!(validate_answer_evidence("画面显示蓝色按钮。", &sources, false).is_err());
        assert!(validate_answer_evidence("画面显示蓝色按钮。[画面]", &sources, false).is_ok());
        let visual = validate_answer_evidence("画面显示一个蓝色按钮。", &[sources[1].clone()], false).unwrap();
        assert_eq!(visual[0].label.as_deref(), Some("[画面]"));
        assert!(validate_answer_evidence("画面显示按钮。[1]", &[sources[1].clone()], false).is_err());
        assert!(validate_answer_evidence("工作计划仍在进行。", &[], true).unwrap().is_empty());
        assert!(validate_answer_evidence("未读取资料却作出结论。", &[], false).is_err());
        assert!(validate_answer_evidence("不存在的引用。[1]", &[], true).is_err());
    }

    #[test]
    fn retrieval_skips_failed_and_unselected_records() {
        let mut ready: KnowledgeRecord = serde_json::from_value(serde_json::json!({
            "id":"rec-ready", "type":"article", "title":"Rust", "site":"example.com",
            "savedAt":"2026-11-02T00:00:00Z", "status":"ready", "sourceUrl":"https://example.com/1",
            "contentText":"Rust 的所有权规则", "excerpt":[]
        })).unwrap();
        let mut fault = ready.clone();
        fault.id = "rec-fault".into();
        fault.status = ProcessStatus::Fault;
        ready.content_text = "Rust 的所有权规则".into();
        let (found, _) = context::select_sources(&[ready, fault], &["rec-ready".into(), "rec-fault".into()], "所有权是什么");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].record_id, "rec-ready");
    }

    #[test]
    fn read_tool_assigns_only_valid_citation_numbers() {
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({
            "id":"rec-selected", "type":"article", "title":"Rust", "site":"example.com",
            "savedAt":"2026-11-11T00:00:00Z", "status":"ready", "sourceUrl":"https://example.com/1",
            "contentText":"Rust 的所有权规则", "excerpt":[]
        })).unwrap();
        let index = ReadIndex::new(&[record], &["rec-selected".into()]);
        let mut sources = Vec::new();
        let read = || serde_json::json!({"chunk_id":"rec-selected:0"});
        assert_eq!(execute_reference_tool(&index, &mut sources, "read_reference", &read())["source_index"], 1);
        assert_eq!(execute_reference_tool(&index, &mut sources, "read_reference", &read())["source_index"], 1);
        assert!(execute_reference_tool(&index, &mut sources, "read_reference", &serde_json::json!({"chunk_id":"rec-other:0"})).get("error").is_some());
        assert_eq!(sources.len(), 1);
        assert_eq!(citations_from_answer("规则见 [1]，伪造的 [2]", &sources).len(), 1);
        let read_step = tool_step_label("read_reference", &read(), &execute_reference_tool(&index, &mut sources, "read_reference", &read()));
        assert!(read_step.contains("Rust"));
        assert!(read_step.contains("[1]"));
        assert!(!read_step.contains("rec-selected:0"));
        let search_step = tool_step_label("search_references", &serde_json::json!({"query":"所有权"}), &index.search("所有权"));
        assert!(search_step.contains("命中 1 个片段"));
    }

    #[test]
    fn tool_loop_returns_search_and_read_results_to_model() {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let replies = [
                serde_json::json!({"choices":[{"message":{"role":"assistant","content":"无关包装".repeat(3_000),"tool_calls":[{"id":"call-search","type":"function","function":{"name":"search_references","arguments":"{\"query\":\"所有权\"}"}}]}}]}),
                serde_json::json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-read","type":"function","function":{"name":"read_reference","arguments":"{\"chunk_id\":\"rec-selected:0\"}"}}]}}]}),
                serde_json::json!({"choices":[{"message":{"role":"assistant","content":"检索完成"}}]}),
            ];
            let mut requests = Vec::new();
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0;
                loop {
                    line.clear();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" { break; }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                requests.push(serde_json::from_slice::<serde_json::Value>(&body).unwrap());
                let body = reply.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
            requests
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({
            "id":"rec-selected", "type":"article", "title":"Rust", "site":"example.com",
            "savedAt":"2026-11-11T00:00:00Z", "status":"ready", "sourceUrl":"https://example.com/1",
            "contentText":format!("{}末尾只有读取工具才能得到的事实", "Rust 的所有权规则。".repeat(20)), "excerpt":[]
        })).unwrap();
        let ids = vec!["rec-selected".into()];
        let index = ReadIndex::new(&[record.clone()], &ids);
        let prepared = context::build_exploration("人格", "所有权是什么？", &[], ids.len(),
            index.directory(&[record], &ids, 6_000), index.len(), &[], &WorkingContext::default(), "");
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let client = reqwest::Client::new();
        let exploration = tauri::async_runtime::block_on(explore_references(|_| {}, &client, &config, "test", &prepared, &index, true)).unwrap().unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0]["tool_choice"], "required");
        assert!(requests[0]["messages"][1]["content"].as_str().unwrap().contains("chunk_id=rec-selected:0"));
        assert!(!requests[0]["messages"][1]["content"].as_str().unwrap().contains("末尾只有读取工具才能得到的事实"));
        assert_eq!(requests[1]["messages"][3]["tool_call_id"], "call-search");
        assert!(requests[1]["messages"][3]["content"].as_str().unwrap().contains("rec-selected:0"));
        assert_eq!(requests[2]["messages"][5]["tool_call_id"], "call-read");
        assert!(requests[2]["messages"][5]["content"].as_str().unwrap().contains("source_index"));
        assert_eq!(exploration.steps.len(), 2);
        assert!(exploration.messages.last().unwrap()["content"].as_str().unwrap().contains("回答原问题"));
        assert_eq!(exploration.sources.len(), 1);
        assert_eq!(exploration.sources[0].record_id, "rec-selected");
    }

    #[test]
    fn seventh_tool_call_stops_exploration_without_losing_the_answer_path() {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        for read_first in [true, false] {
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
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
                let calls = (0..7).map(|number| {
                    let (name, arguments) = if read_first && number == 0 {
                        ("read_reference", r#"{"chunk_id":"rec-selected:0"}"#)
                    } else { ("search_references", r#"{"query":"不存在的检索词"}"#) };
                    serde_json::json!({"id":format!("call-{number}"),"type":"function",
                        "function":{"name":name,"arguments":arguments}})
                }).collect::<Vec<_>>();
                let response = serde_json::json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":calls}}]}).to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            });
            let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({"id":"rec-selected","type":"article",
                "title":"测试资料","site":"example.com","savedAt":"1","status":"ready",
                "sourceUrl":"https://example.com","contentText":"一段可以读取的正文。","excerpt":[]})).unwrap();
            let ids = vec![record.id.clone()];
            let index = ReadIndex::new(&[record.clone()], &ids);
            let prepared = context::build_exploration("人格", "资料说了什么", &[], 1,
                index.directory(&[record], &ids, 6_000), index.len(), &[], &WorkingContext::default(), "");
            let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
            let result = tauri::async_runtime::block_on(explore_references(|_| {}, &reqwest::Client::new(),
                &config, "test", &prepared, &index, true)).unwrap();
            server.join().unwrap();
            if read_first {
                let exploration = result.expect("已读到正文时继续生成回答");
                assert_eq!(exploration.sources.len(), 1);
                assert_eq!(exploration.messages[2]["tool_calls"].as_array().unwrap().len(), 6);
                assert!(exploration.steps.iter().any(|step| step.contains("6 次工具调用上限")));
                assert!(exploration.messages.last().unwrap()["content"].as_str().unwrap().contains("回答原问题"));
            } else {
                assert!(result.is_none(), "只有搜索、尚未读取时应回退到普通上下文");
            }
        }
    }

    #[test]
    fn output_budget_stops_reading_and_keeps_only_returned_sources() {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new(); reader.read_line(&mut line).unwrap();
            let mut length = 0;
            loop {
                line.clear(); reader.read_line(&mut line).unwrap();
                if line == "\r\n" { break; }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") { length = value.trim().parse::<usize>().unwrap(); }
            }
            let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
            let calls = (0..6).map(|index| serde_json::json!({"id":format!("read-{index}"),"type":"function",
                "function":{"name":"read_reference","arguments":format!("{{\"chunk_id\":\"rec-selected:{index}\"}}")}})).collect::<Vec<_>>();
            let reply = serde_json::json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":calls}}]}).to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({"id":"rec-selected","type":"article","title":"长文",
            "site":"example.com","savedAt":"1","status":"ready","sourceUrl":"https://example.com",
            "contentText":(0..850).map(|index| format!("第{index}条资料说明。")).collect::<String>(),"excerpt":[]})).unwrap();
        let ids = vec![record.id.clone()];
        let index = ReadIndex::new(&[record.clone()], &ids);
        assert!(index.len() >= 6);
        let prepared = context::build_exploration("人格", "长文讲什么", &[], 1,
            index.directory(&[record], &ids, 6_000), index.len(), &[], &WorkingContext::default(), "");
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let result = tauri::async_runtime::block_on(explore_references(|_| {}, &reqwest::Client::new(), &config, "test", &prepared, &index, true)).unwrap().unwrap();
        server.join().unwrap();
        assert!(result.steps.iter().any(|step| step.contains("上限")), "steps={:?}, sources={}", result.steps, result.sources.len());
        assert!(result.sources.len() < 6);
        assert!(result.messages.last().unwrap()["content"].as_str().unwrap().contains("用已经读到的原文"));
        let output = result.messages.iter().filter(|message| message["role"] == "tool")
            .map(|message| message["content"].as_str().unwrap().chars().count()).sum::<usize>();
        assert!(output <= MAX_TOOL_OUTPUT_CHARS);
    }

    #[test]
    fn rejected_required_tool_choice_retries_with_auto() {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let replies = [
                ("400 Bad Request", serde_json::json!({"error":{"message":"tool_choice required is not supported"}})),
                ("200 OK", serde_json::json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-search","type":"function","function":{"name":"search_references","arguments":"{\"query\":\"所有权\"}"}}]}}]})),
                ("200 OK", serde_json::json!({"choices":[{"message":{"role":"assistant","content":"已检索"}}]})),
            ];
            let mut choices = Vec::new();
            for (status, reply) in replies {
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
                choices.push(request["tool_choice"].as_str().unwrap().to_string());
                let body = reply.to_string();
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
            choices
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({"id":"rec-selected","type":"article","title":"Rust","site":"example.com",
            "savedAt":"2026-11-11T00:00:00Z","status":"ready","sourceUrl":"https://example.com","contentText":"Rust 的所有权规则","excerpt":[]})).unwrap();
        let prepared = context::build("人格", "所有权是什么？", &[], &[record.clone()], &[record.id.clone()], None, &[], &WorkingContext::default(), "");
        let index = ReadIndex::new(&[record], &["rec-selected".into()]);
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let exploration = tauri::async_runtime::block_on(explore_references(|_| {}, &reqwest::Client::new(), &config, "test", &prepared, &index, true)).unwrap().unwrap();
        assert_eq!(server.join().unwrap(), ["required", "auto", "auto"]);
        assert_eq!(exploration.steps.len(), 1);
    }

    #[test]
    fn unsupported_tool_api_falls_back_to_regular_answer() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).unwrap();
            let body = r#"{"error":{"message":"tools are not supported"}}"#;
            write!(stream, "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({
            "id":"rec-selected", "type":"article", "title":"Rust", "site":"example.com",
            "savedAt":"2026-11-11T00:00:00Z", "status":"ready", "sourceUrl":"https://example.com/1",
            "contentText":"Rust 的所有权规则", "excerpt":[]
        })).unwrap();
        let prepared = context::build("人格", "问题", &[], &[record.clone()], &["rec-selected".into()], None, &[], &WorkingContext::default(), "");
        let index = ReadIndex::new(&[record], &["rec-selected".into()]);
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let result = tauri::async_runtime::block_on(explore_references(|_| {}, &reqwest::Client::new(), &config, "test", &prepared, &index, true)).unwrap();
        server.join().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn no_tool_call_falls_back_to_full_stage_one_context() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 8_192]; let _ = stream.read(&mut buffer).unwrap();
            let reply = r#"{"choices":[{"message":{"role":"assistant","content":"直接回答"}}]}"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
        });
        let record: KnowledgeRecord = serde_json::from_value(serde_json::json!({"id":"rec-selected","type":"article","title":"Rust",
            "site":"example.com","savedAt":"1","status":"ready","sourceUrl":"https://example.com",
            "contentText":"完整原文只在普通问答里。","excerpt":[]})).unwrap();
        let ids = vec![record.id.clone()];
        let index = ReadIndex::new(&[record.clone()], &ids);
        let exploration = context::build_exploration("人格", "问题", &[], 1,
            index.directory(&[record.clone()], &ids, 6_000), index.len(), &[], &WorkingContext::default(), "");
        let regular = context::build("人格", "问题", &[], &[record], &ids, None, &[], &WorkingContext::default(), "");
        assert!(exploration.sources.is_empty());
        assert_eq!(regular.sources.len(), 1);
        let config = AgentConfig { base_url: format!("http://127.0.0.1:{port}/v1"), model: "mock".into(), ..AgentConfig::default() };
        let result = tauri::async_runtime::block_on(explore_references(|_| {}, &reqwest::Client::new(), &config, "test", &exploration, &index, true)).unwrap();
        server.join().unwrap();
        assert!(result.is_none());
        assert!(regular.user_text.contains("完整原文只在普通问答里"));
    }

    #[test]
    fn older_chat_run_without_read_sources_still_loads() {
        let run: ChatRun = serde_json::from_str(r#"{"elapsedMs":17,"changedFiles":[]}"#).unwrap();
        assert!(run.read_sources.is_empty());
        assert!(run.read_coverage.is_none());
    }

    #[test]
    fn provider_error_shows_actionable_model_hint() {
        let message = provider_error(reqwest::StatusCode::BAD_REQUEST, "{}", "https://api.deepseek.com", "DeepSeek-V4.1-Flash-1017");
        assert!(message.contains("deepseek-flash"));
        let generic = provider_error(reqwest::StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"Unknown model: example"}}"#, "https://api.example.com/v1", "example");
        assert!(generic.contains("Unknown model: example"));
    }

    #[test]
    fn exploration_defaults_on_for_existing_config_and_saves_off() {
        let old = r#"{"baseUrl":"https://api.example.com/v1","model":"mock"}"#;
        let config: AgentConfig = serde_json::from_str(old).unwrap();
        assert!(config.explore_references);
        assert_eq!(config.context_tier, ContextTier::Small);
        assert!(!config.semantic_retrieval);
        assert!(config.embedding_model.is_empty());

        let mut disabled = config.clone();
        disabled.explore_references = false;
        let saved = serde_json::to_string(&disabled).unwrap();
        let restored: AgentConfig = serde_json::from_str(&saved).unwrap();
        assert!(!restored.explore_references);
        assert_eq!(restored.context_tier, ContextTier::Small);
        disabled.context_tier = ContextTier::Medium;
        let restored: AgentConfig = serde_json::from_str(&serde_json::to_string(&disabled).unwrap()).unwrap();
        assert_eq!(restored.context_tier.char_limit(), 48_000);
        assert!(!restored.explore_references);
        disabled.semantic_retrieval = true;
        disabled.embedding_model = "test-embedding".into();
        let restored: AgentConfig = serde_json::from_str(&serde_json::to_string(&disabled).unwrap()).unwrap();
        assert!(restored.semantic_retrieval);
        assert_eq!(restored.embedding_model, "test-embedding");
    }

    #[test]
    fn compact_summary_is_structured_and_legacy_text_remains_readable() {
        let evidence = "用户确认：先做阶段四。资料 ID rec-17。";
        let valid = r#"{"confirmedDecisions":["先做阶段四"],"openQuestions":[],"sourceIds":["rec-17"]}"#;
        assert_eq!(parse_compact_summary(valid, evidence).unwrap(), valid);
        assert!(parse_compact_summary(r#"{"confirmedDecisions":[],"openQuestions":[],"sourceIds":["rec-99"]}"#, evidence).is_err());
        assert!(parse_compact_summary(&format!("```json\n{valid}\n```"), evidence).is_err());
        assert!(parse_compact_summary(r#"{"confirmedDecisions":["x"],"openQuestions":[],"sourceIds":[],"command":"run"}"#, evidence).is_err());
        let old: ChatSession = serde_json::from_value(serde_json::json!({
            "id":"chat-old", "title":"旧会话", "createdAt":"1", "updatedAt":"1", "referencedIds":[], "messages":[], "compactedSummary":"旧散文摘要"
        })).unwrap();
        assert_eq!(old.compacted_summary, "旧散文摘要");
    }

    #[test]
    fn deepseek_tool_requests_explicitly_disable_thinking() {
        let deepseek = AgentConfig { base_url: "https://api.deepseek.com".into(), model: "deepseek-flash".into(), ..AgentConfig::default() };
        let first = exploration_payload(&deepseek, &[], "required", true);
        assert_eq!(first["tool_choice"], "required");
        assert_eq!(first["thinking"]["type"], "disabled");
        let final_answer = answer_payload(&deepseek, &[], true);
        assert_eq!(final_answer["tool_choice"], "none");
        assert_eq!(final_answer["thinking"]["type"], "disabled");
        assert!(final_answer["tools"].is_array());
        let regular_answer = answer_payload(&deepseek, &[], false);
        assert!(regular_answer.get("thinking").is_none());
        assert!(!is_deepseek_api("https://api.deepseek.com.example.invalid"));
    }

    #[test]
    fn sse_parser_handles_split_utf8_and_event_boundaries() {
        let mut parser = SseParser::default();
        let bytes = "data: {\"choices\":[{\"delta\":{\"content\":\"先生，您好\"},\"finish_reason\":null}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"。[1]\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n".as_bytes();
        let mut output = Vec::new();
        for piece in bytes.chunks(3) { output.extend(parser.push(piece).unwrap()); }
        assert_eq!(output.concat(), "先生，您好。[1]");
        assert!(parser.done);
        assert_eq!(parser.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn persona_content_is_bounded() {
        assert!(valid_persona(DEFAULT_PERSONA));
        assert!(!valid_persona(" \n "));
        assert!(!valid_persona(&"字".repeat(12_001)));
    }
