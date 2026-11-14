// 回答引用与证据校验。
use super::*;
#[cfg(test)]
pub(super) fn citations_from_answer(answer: &str, sources: &[SourceChunk]) -> Vec<ChatCitation> {
    let mut result = Vec::new();
    let mut document_index = 0;
    let mut chat_index = 0;
    for source in sources {
        let (label, number, kind) = if source.visual { ("[画面]".to_string(), None, Some("image".into())) }
            else if source.record_id.starts_with("chat:") {
                chat_index += 1; (format!("[对话{chat_index}]"), None, Some("chat".into()))
            } else { document_index += 1; (format!("[{document_index}]"), Some(document_index), None) };
        if answer.contains(&label) {
            result.push(ChatCitation { record_id: source.record_id.clone(), at_seconds: source.at_seconds, source_index: number,
                label: (kind.is_some()).then_some(label), kind });
        }
    }
    result
}

pub(super) fn invalid_citation_numbers(answer: &str, source_count: usize) -> Vec<usize> {
    let mut invalid = Vec::new();
    let mut quoted_until = None;
    for (index, ch) in answer.char_indices() {
        if let Some(close) = quoted_until {
            if ch == close { quoted_until = None; }
            continue;
        }
        quoted_until = match ch { '「' => Some('」'), '“' => Some('”'), '『' => Some('』'), '‘' => Some('’'), '"' => Some('"'), _ => None };
        if quoted_until.is_some() || ch != '[' || (index > 0 && (answer.as_bytes()[index - 1].is_ascii_alphanumeric() || answer.as_bytes()[index - 1] == b'`')) { continue; }
        let tail = &answer[index + 1..];
        let digits = tail.bytes().take_while(u8::is_ascii_digit).take(6).count();
        if digits == 0 || tail.as_bytes().get(digits) != Some(&b']') { continue; }
        if let Ok(number) = tail[..digits].parse::<usize>() {
            if (number == 0 || number > source_count) && !invalid.contains(&number) { invalid.push(number); }
        }
    }
    invalid
}

pub(crate) fn validate_citation_numbers(answer: &str, source_count: usize) -> Result<(), String> {
    let invalid = invalid_citation_numbers(answer, source_count);
    if invalid.is_empty() { Ok(()) } else {
        Err(format!("模型使用了未提供的资料编号 {}，本轮回答未保存；请重试或缩小参考范围", invalid.iter().map(|number| format!("[{number}]")).collect::<Vec<_>>().join("、")))
    }
}

fn evidence_error(reason: &str) -> String {
    format!("模型回答的资料依据未通过本地核对（{reason}），本轮回答未保存；请重试或缩小参考范围")
}

fn compact_whitespace(text: &str) -> String { text.chars().filter(|ch| !ch.is_whitespace()).collect() }

fn one_claim(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().filter(|ch| matches!(ch, '。' | '！' | '？' | '!' | '?')).count() <= 1
}

fn validate_insufficient_answer(answer: &str) -> Result<(), String> {
    let body = answer.strip_prefix("资料不足：").ok_or_else(|| evidence_error("缺少可核对的原文；请只说明资料不足"))?;
    if !one_claim(body) || body.lines().count() != 1 || body.contains(['[', ']', '「', '」', '“', '”']) {
        return Err(evidence_error("资料不足说明不能混入结论、引文或编号"));
    }
    Ok(())
}

pub(super) fn validate_answer_evidence(answer: &str, sources: &[SourceChunk], allow_background_answer: bool) -> Result<Vec<ChatCitation>, String> {
    let documents = sources.iter().filter(|source| !source.visual && !source.record_id.starts_with("chat:")).collect::<Vec<_>>();
    let chats = sources.iter().filter(|source| source.record_id.starts_with("chat:")).collect::<Vec<_>>();
    let visual = sources.iter().find(|source| source.visual);
    validate_citation_numbers(answer, documents.len())?;
    if answer.starts_with("资料不足：") {
        validate_insufficient_answer(answer)?;
        return Ok(Vec::new());
    }
    if sources.is_empty() {
        if !allow_background_answer || answer.contains("[对话") || answer.contains("[画面]") { return Err(evidence_error("没有可引用的资料")); }
        return Ok(Vec::new());
    }
    if documents.is_empty() && chats.is_empty() && visual.is_some() && !answer.contains('[') {
        let source = visual.unwrap();
        return Ok(vec![ChatCitation { record_id: source.record_id.clone(), at_seconds: source.at_seconds,
            source_index: None, label: Some("[画面]".into()), kind: Some("image".into()) }]);
    }
    let mut citations = Vec::new();
    for raw_line in answer.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let line = raw_line.strip_prefix("- ").or_else(|| raw_line.strip_prefix("* ")).unwrap_or(raw_line).trim();
        if let Some(claim) = line.strip_suffix("[画面]") {
            let source = visual.ok_or_else(|| evidence_error("本轮未提供画面"))?;
            if !one_claim(claim) || claim.contains(['[', ']']) { return Err(evidence_error("画面结论格式无效")); }
            if !citations.iter().any(|item: &ChatCitation| item.kind.as_deref() == Some("image")) {
                citations.push(ChatCitation { record_id: source.record_id.clone(), at_seconds: source.at_seconds,
                    source_index: None, label: Some("[画面]".into()), kind: Some("image".into()) });
            }
            continue;
        }
        let bracket = line.rfind('[').ok_or_else(|| evidence_error("每条资料结论都需要原文引句和编号"))?;
        let reference = line[bracket + 1..].strip_suffix(']').ok_or_else(|| evidence_error("引用编号格式无效"))?;
        let before_reference = line[..bracket].trim_end();
        let (open, close) = match before_reference.chars().last() {
            Some('」') => ('「', '」'), Some('”') => ('“', '”'), Some('』') => ('『', '』'),
            Some('’') => ('‘', '’'), Some('"') => ('"', '"'), _ => return Err(evidence_error("引用编号前缺少原文引句")),
        };
        let quote_end = before_reference.len() - close.len_utf8();
        let quote_start = before_reference[..quote_end].rfind(open).ok_or_else(|| evidence_error("原文引句缺少起始引号"))?;
        let quote = &before_reference[quote_start + open.len_utf8()..quote_end];
        let claim = before_reference[..quote_start].trim();
        if !one_claim(claim) || claim.contains(['[', ']']) || compact_whitespace(quote).is_empty() {
            return Err(evidence_error("每行只能写一条结论，并附一段非空原文"));
        }
        let (source, source_index, label, kind) = if let Some(chat_number) = reference.strip_prefix("对话") {
            let number = chat_number.parse::<usize>().map_err(|_| evidence_error("对话编号无效"))?;
            let source = number.checked_sub(1).and_then(|index| chats.get(index)).ok_or_else(|| evidence_error("对话编号不属于本轮"))?;
            let role = if source.title.contains("用户消息") { "用户" } else { "Agent" };
            if !claim.contains(role) { return Err(evidence_error("对话摘录须说明是谁说的")); }
            (*source, None, Some(format!("[对话{number}]")), Some("chat".into()))
        } else {
            let number = reference.parse::<usize>().map_err(|_| evidence_error("资料编号无效"))?;
            let source = number.checked_sub(1).and_then(|index| documents.get(index)).ok_or_else(|| evidence_error("资料编号不属于本轮"))?;
            (*source, Some(number), None, None)
        };
        if !compact_whitespace(&source.text).contains(&compact_whitespace(quote)) {
            return Err(evidence_error("引句不是对应片段的连续原文"));
        }
        if !citations.iter().any(|item: &ChatCitation| item.record_id == source.record_id && item.source_index == source_index && item.label == label) {
            citations.push(ChatCitation { record_id: source.record_id.clone(), at_seconds: source.at_seconds, source_index, label, kind });
        }
    }
    if citations.is_empty() { return Err(evidence_error("回答缺少可核对的资料引句")); }
    Ok(citations)
}

pub(super) fn selected_messages(session: &ChatSession, ids: &[String]) -> Result<Vec<ChatMessage>, String> {
    if ids.len() > 4 { return Err("每次最多选择 4 条对话消息".into()); }
    let mut seen = HashSet::new();
    ids.iter().map(|id| {
        if !seen.insert(id) { return Err("请勿重复选择对话消息".into()); }
        session.messages.iter().find(|message| &message.id == id && !message.paragraphs.join(" ").trim().is_empty())
            .cloned().ok_or_else(|| "所选消息不在当前会话中，请重新选择".into())
    }).collect()
}
