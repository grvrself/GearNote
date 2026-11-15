use crate::{context::SourceChunk, storage::KnowledgeRecord};
use serde::{Deserialize, Serialize};

pub const BATCH_CHARS: usize = 3_000;
pub const BATCHES_PER_ROUND: usize = 4;
pub const MAX_BATCHES: usize = 32;
pub const MAX_SECONDS: u64 = 120;

pub struct VideoBatch { pub source: SourceChunk, pub end_seconds: u32 }

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Finding { pub source_index: usize, pub quote: String, pub finding: String }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint { pub enough: bool, pub notes: Vec<Finding> }

pub fn batches(record: &KnowledgeRecord) -> Vec<VideoBatch> {
    let Some(cues) = record.transcript.as_ref() else { return Vec::new(); };
    let mut result = Vec::new();
    let mut text = String::new();
    let mut start = 0;
    let mut end = 0;
    let flush = |result: &mut Vec<VideoBatch>, text: &mut String, start: u32, end: u32| {
        if text.is_empty() { return; }
        let body = std::mem::take(text);
        result.push(VideoBatch { source: SourceChunk {
            record_id: record.id.clone(), title: record.title.clone(), source_url: record.source_url.clone(),
            captured_at: record.saved_at.clone(), at_seconds: Some(start), search_text: body.clone(), text: body,
            score: 0, visual: false,
        }, end_seconds: end });
    };
    for cue in cues {
        let prefix = format!("{:02}:{:02} ", cue.at_seconds / 60, cue.at_seconds % 60);
        let mut remaining = cue.text.chars().peekable();
        while remaining.peek().is_some() {
            if text.is_empty() { start = cue.at_seconds; }
            let available = BATCH_CHARS.saturating_sub(text.chars().count() + prefix.chars().count() + 1);
            if available == 0 { flush(&mut result, &mut text, start, end); continue; }
            let part = remaining.by_ref().take(available).collect::<String>();
            text.push_str(&prefix);
            text.push_str(&part);
            text.push('\n');
            end = cue.at_seconds;
            if remaining.peek().is_some() { flush(&mut result, &mut text, start, end); }
        }
        if text.chars().count() + prefix.chars().count() + 2 > BATCH_CHARS {
            flush(&mut result, &mut text, start, end);
        }
    }
    flush(&mut result, &mut text, start, end);
    result
}

fn note_is_verifiable(note: &Finding, read: &[VideoBatch], first_index: usize) -> bool {
    let Some(batch) = note.source_index.checked_sub(first_index).and_then(|index| read.get(index)) else { return false; };
    let quote = note.quote.trim();
    !quote.is_empty() && quote.chars().count() <= 300 && !note.finding.trim().is_empty()
        && note.finding.chars().count() <= 200
        && batch.source.text.chars().filter(|ch| !ch.is_whitespace()).collect::<String>()
            .contains(&quote.chars().filter(|ch| !ch.is_whitespace()).collect::<String>())
}

pub fn parse_checkpoint(answer: &str, read: &[VideoBatch], first_index: usize) -> Result<Checkpoint, String> {
    let checkpoint: Checkpoint = serde_json::from_str(answer.trim()).map_err(|_| "深度模式检查点必须是完整 JSON 对象")?;
    if checkpoint.notes.len() > 8 { return Err("深度模式单轮摘要超过 8 条".into()); }
    for note in &checkpoint.notes {
        if note.source_index.checked_sub(first_index).and_then(|index| read.get(index)).is_none() {
            return Err("深度模式摘要引用了本轮未读的字幕批次".into());
        }
        if !note_is_verifiable(note, read, first_index) {
            return Err("深度模式摘要的原句不在对应字幕批次中".into());
        }
    }
    Ok(checkpoint)
}

pub fn keep_verified_notes(answer: &str, read: &[VideoBatch], first_index: usize) -> Result<(Checkpoint, usize), String> {
    let mut checkpoint: Checkpoint = serde_json::from_str(answer.trim()).map_err(|_| "深度模式检查点必须是完整 JSON 对象")?;
    let original_count = checkpoint.notes.len();
    checkpoint.notes = checkpoint.notes.into_iter().take(8)
        .filter(|note| note_is_verifiable(note, read, first_index)).collect();
    let rejected = original_count - checkpoint.notes.len();
    if rejected > 0 || checkpoint.notes.is_empty() { checkpoint.enough = false; }
    Ok((checkpoint, rejected))
}

pub fn needs_full_coverage(question: &str) -> bool {
    ["总结", "概括", "整段", "整个", "全片", "全程", "完整", "讲了什么", "讲什么", "全部内容"].iter().any(|term| question.contains(term))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(cues: Vec<(u32, String)>) -> KnowledgeRecord {
        serde_json::from_value(serde_json::json!({"id":"video","type":"video","title":"测试视频","site":"example.com",
            "savedAt":"1","status":"ready","sourceUrl":"https://example.com","contentText":"", "excerpt":[],
            "transcript":cues.into_iter().map(|(at, text)| serde_json::json!({"atSeconds":at,"text":text})).collect::<Vec<_>>()})).unwrap()
    }

    #[test]
    fn batches_cover_every_cue_without_exceeding_limit() {
        let record = video(vec![(0, "开头".into()), (30, "中段".repeat(2_000)), (90, "结尾".into())]);
        let pieces = batches(&record);
        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|item| item.source.text.chars().count() <= BATCH_CHARS));
        let combined = pieces.iter().map(|item| item.source.text.as_str()).collect::<String>();
        assert!(combined.contains("00:00 开头"));
        assert!(combined.contains("01:30 结尾"));
        assert_eq!(combined.matches('中').count(), 2_000);
        assert_eq!(combined.matches('段').count(), 2_000);
        assert_eq!(pieces.last().unwrap().end_seconds, 90);
    }

    #[test]
    fn checkpoint_only_accepts_quotes_from_current_batches() {
        let record = video(vec![(12, "这句来自字幕。".into())]);
        let pieces = batches(&record);
        let valid = r#"{"enough":true,"notes":[{"sourceIndex":1,"quote":"这句来自字幕。","finding":"提到字幕"}]}"#;
        assert!(parse_checkpoint(valid, &pieces, 1).is_ok());
        assert!(parse_checkpoint(&valid.replace("这句来自字幕。", "不存在的原句"), &pieces, 1).is_err());
        assert!(parse_checkpoint(&valid.replace("\"sourceIndex\":1", "\"sourceIndex\":2"), &pieces, 1).is_err());
        assert!(parse_checkpoint(&format!("```json\n{valid}\n```"), &pieces, 1).is_err());
        assert!(needs_full_coverage("总结整条视频"));
        assert!(!needs_full_coverage("开头说了什么"));
    }

    #[test]
    fn invalid_checkpoint_notes_are_discarded_without_accepting_fake_quotes() {
        let record = video(vec![(12, "这句来自字幕。".into())]);
        let pieces = batches(&record);
        let mixed = r#"{"enough":true,"notes":[{"sourceIndex":1,"quote":"这句来自字幕。","finding":"真实事实"},{"sourceIndex":1,"quote":"编造的原句","finding":"虚假事实"}]}"#;
        let (checkpoint, rejected) = keep_verified_notes(mixed, &pieces, 1).unwrap();
        assert_eq!(rejected, 1);
        assert_eq!(checkpoint.notes.len(), 1);
        assert_eq!(checkpoint.notes[0].quote, "这句来自字幕。");
        assert!(!checkpoint.enough);
        let (empty, rejected) = keep_verified_notes(&mixed.replace("这句来自字幕。", "另一个假引句"), &pieces, 1).unwrap();
        assert_eq!(rejected, 2);
        assert!(empty.notes.is_empty());
        assert!(!empty.enough);
    }
}
