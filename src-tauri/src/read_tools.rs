use crate::{context::{self, SourceChunk}, storage::KnowledgeRecord, ProcessStatus};
use serde_json::{json, Value};
use std::collections::HashSet;

const MAX_INDEX_CHUNKS: usize = 2_000;
const MAX_SEARCH_RESULTS: usize = 6;

pub struct ReadIndex { chunks: Vec<(String, SourceChunk)> }

impl ReadIndex {
    pub fn new(records: &[KnowledgeRecord], referenced_ids: &[String]) -> Self {
        let allowed: HashSet<&str> = referenced_ids.iter().map(String::as_str).collect();
        let mut chunks = Vec::new();
        for record in records.iter().filter(|record| allowed.contains(record.id.as_str())
            && record.status == ProcessStatus::Ready && !record.content_text.is_empty()) {
            for (index, chunk) in context::chunks(record).into_iter().enumerate() {
                if chunks.len() == MAX_INDEX_CHUNKS { return Self { chunks }; }
                chunks.push((format!("{}:{index}", record.id), chunk));
            }
        }
        Self { chunks }
    }

    pub fn is_empty(&self) -> bool { self.chunks.is_empty() }

    pub fn search(&self, query: &str) -> Value {
        let query = query.trim();
        if query.is_empty() || query.chars().count() > 200 {
            return json!({"error":"检索词需为 1–200 字"});
        }
        let terms = context::terms(query);
        let mut ranked = self.chunks.iter().filter_map(|(id, chunk)| {
            let score = context::terms(&chunk.text).intersection(&terms).count()
                + context::terms(&chunk.title).intersection(&terms).count() * 2;
            (score > 0).then_some((score, id, chunk))
        }).collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        let matches = ranked.into_iter().take(MAX_SEARCH_RESULTS).map(|(score, id, chunk)| json!({
            "chunk_id": id, "title": chunk.title, "at_seconds": chunk.at_seconds,
            "snippet": chunk.text.chars().take(180).collect::<String>(), "score": score
        })).collect::<Vec<_>>();
        json!({"matches": matches, "scope": "本轮已选且可用的资料"})
    }

    pub fn read(&self, chunk_id: &str) -> Result<SourceChunk, &'static str> {
        self.chunks.iter().find(|(id, _)| id == chunk_id)
            .map(|(_, chunk)| chunk.clone()).ok_or("片段不存在或不属于本轮已选资料")
    }
}

pub fn definitions() -> Value {
    json!([
        {"type":"function","function":{"name":"search_references","description":"在本轮已选且保存成功的文章和视频字幕中搜索。返回片段 ID 与短摘要；需要原文时再调用 read_reference。","parameters":{"type":"object","properties":{"query":{"type":"string","description":"不超过 200 字的检索词"}},"required":["query"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"read_reference","description":"按 search_references 返回的片段 ID 读取原文。只能读取本轮已选资料。返回可在回答中使用的来源编号。","parameters":{"type":"object","properties":{"chunk_id":{"type":"string"}},"required":["chunk_id"],"additionalProperties":false}}},
        {"type":"function","function":{"name":"request_choice","description":"仅当做法明显分叉且用户尚未表态时暂停问答，向用户提供 2–4 个单选选项；调用后不要同时给方案正文。不能修改笔记、资料或工作状态。","parameters":{"type":"object","properties":{"prompt":{"type":"string","description":"问题，最多 40 字"},"why":{"type":"string","description":"为何需要选择，最多 80 字"},"options":{"type":"array","minItems":2,"maxItems":4,"items":{"type":"object","properties":{"id":{"type":"string","description":"本轮唯一的短英文 ID"},"label":{"type":"string","description":"按钮名称，1–24 字"},"detail":{"type":"string","description":"选项说明，最多 80 字"}},"required":["id","label","detail"],"additionalProperties":false}},"recommended":{"type":"string","description":"可选，推荐选项的 id"}},"required":["prompt","why","options"],"additionalProperties":false}}}
    ])
}

pub fn definitions_without_choice() -> Value {
    let mut tools = definitions();
    tools.as_array_mut().expect("tool definitions are an array").retain(|tool| tool.pointer("/function/name").and_then(Value::as_str) != Some("request_choice"));
    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, status: ProcessStatus, text: &str) -> KnowledgeRecord {
        serde_json::from_value(json!({"id":id,"type":"article","title":"测试资料","site":"example.com",
            "savedAt":"2026-11-11T00:00:00Z","status":status,"sourceUrl":"https://example.com",
            "contentText":text,"excerpt":[]})).unwrap()
    }

    #[test]
    fn search_and_read_stay_inside_selected_ready_records() {
        let index = ReadIndex::new(&[
            record("selected", ProcessStatus::Ready, "Rust 所有权规则"),
            record("other", ProcessStatus::Ready, "Rust 所有权规则"),
            record("failed", ProcessStatus::Fault, "Rust 所有权规则"),
        ], &["selected".into(), "failed".into()]);
        let result = index.search("所有权");
        assert_eq!(result["matches"].as_array().unwrap().len(), 1);
        assert_eq!(result["matches"][0]["chunk_id"], "selected:0");
        assert_eq!(index.read("selected:0").unwrap().record_id, "selected");
        assert!(index.read("other:0").is_err());
        assert!(index.read("failed:0").is_err());
    }
}
