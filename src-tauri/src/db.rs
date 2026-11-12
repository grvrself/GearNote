use crate::{agent::{ChatMessage, ChatSession, ChatStore}, notes::{Note, NoteFolder}, storage::{KnowledgeRecord, RecordSummary}};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde::de::DeserializeOwned;
use std::{collections::HashSet, path::Path};

pub const FILE_NAME: &str = "knowflow.db";

pub fn backup_before_phase_four(path: &Path) -> Result<(), String> {
    if !path.exists() { return Ok(()); }
    let backup = path.with_file_name("knowflow.pre-phase4.db");
    if backup.exists() { return Ok(()); }
    let connection = open(path)?;
    let has_legacy: i64 = connection.query_row("SELECT
        (SELECT count(*) FROM snapshots WHERE name IN ('records','notes','chat')) +
        (SELECT count(*) FROM entity_meta WHERE name IN ('records','notes','chat') AND version=1)", [], |row| row.get(0))
        .map_err(|error| format!("旧数据检查失败：{error}"))?;
    if has_legacy == 0 { return Ok(()); }
    let temporary = path.with_file_name(format!("knowflow.pre-phase4-{}.tmp", uuid::Uuid::new_v4()));
    let target = temporary.to_string_lossy().into_owned();
    connection.execute("VACUUM INTO ?1", [&target]).map_err(|error| format!("升级前数据库备份失败：{error}"))?;
    std::fs::rename(&temporary, &backup).map_err(|error| format!("升级前数据库备份保存失败：{error}"))?;
    Ok(())
}

pub(crate) fn open(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|error| format!("数据库打开失败：{error}"))?;
    connection.busy_timeout(std::time::Duration::from_secs(5)).map_err(|error| error.to_string())?;
    connection.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS snapshots (name TEXT PRIMARY KEY, json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS entity_meta (name TEXT PRIMARY KEY, version INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS record_rows (
            id TEXT PRIMARY KEY, ordinal INTEGER NOT NULL, record_type TEXT NOT NULL,
            title TEXT NOT NULL, site TEXT NOT NULL, status TEXT NOT NULL,
            source_url TEXT NOT NULL, saved_at TEXT NOT NULL, json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_record_rows_type_saved ON record_rows(record_type, saved_at DESC);
        CREATE INDEX IF NOT EXISTS idx_record_rows_status_saved ON record_rows(status, saved_at DESC);
        CREATE INDEX IF NOT EXISTS idx_record_rows_ordinal ON record_rows(ordinal);
        CREATE TABLE IF NOT EXISTS record_segments (
            record_id TEXT NOT NULL, ordinal INTEGER NOT NULL, at_seconds INTEGER,
            body TEXT NOT NULL, PRIMARY KEY(record_id,ordinal)
        );
        CREATE INDEX IF NOT EXISTS idx_record_segments_time ON record_segments(record_id,at_seconds);
        CREATE VIRTUAL TABLE IF NOT EXISTS segment_fts USING fts5(body, content='record_segments', content_rowid='rowid', tokenize='trigram');
        CREATE TRIGGER IF NOT EXISTS record_segments_ai AFTER INSERT ON record_segments BEGIN
            INSERT INTO segment_fts(rowid,body) VALUES (new.rowid,new.body);
        END;
        CREATE TRIGGER IF NOT EXISTS record_segments_ad AFTER DELETE ON record_segments BEGIN
            INSERT INTO segment_fts(segment_fts,rowid,body) VALUES ('delete',old.rowid,old.body);
        END;
        CREATE TRIGGER IF NOT EXISTS record_segments_au AFTER UPDATE ON record_segments BEGIN
            INSERT INTO segment_fts(segment_fts,rowid,body) VALUES ('delete',old.rowid,old.body);
            INSERT INTO segment_fts(rowid,body) VALUES (new.rowid,new.body);
        END;
        CREATE TABLE IF NOT EXISTS note_rows (
            id TEXT PRIMARY KEY, ordinal INTEGER NOT NULL, title TEXT NOT NULL, updated_at INTEGER NOT NULL, json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_note_rows_updated ON note_rows(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_note_rows_ordinal ON note_rows(ordinal);
        CREATE TABLE IF NOT EXISTS note_folders (
            id TEXT PRIMARY KEY, name TEXT NOT NULL COLLATE NOCASE UNIQUE, created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS chat_meta (
            id INTEGER PRIMARY KEY CHECK(id=1), active_id TEXT NOT NULL, project_memory_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS session_rows (
            id TEXT PRIMARY KEY, ordinal INTEGER NOT NULL, title TEXT NOT NULL,
            project_id TEXT NOT NULL, updated_at TEXT NOT NULL, json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_session_rows_updated ON session_rows(updated_at DESC);
        CREATE TABLE IF NOT EXISTS message_rows (
            session_id TEXT NOT NULL, id TEXT NOT NULL, ordinal INTEGER NOT NULL,
            role TEXT NOT NULL, created_at TEXT NOT NULL, json TEXT NOT NULL,
            PRIMARY KEY(session_id,id)
        );
        CREATE INDEX IF NOT EXISTS idx_message_rows_order ON message_rows(session_id,ordinal);
        CREATE TABLE IF NOT EXISTS workflow_proposals (
            id TEXT PRIMARY KEY, kind TEXT NOT NULL, target_id TEXT NOT NULL,
            before_json TEXT NOT NULL, after_json TEXT NOT NULL,
            mode TEXT NOT NULL, before_text TEXT NOT NULL, after_text TEXT NOT NULL,
            reason TEXT NOT NULL, status TEXT NOT NULL, created_at TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT '', edits_json TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_proposals_target ON workflow_proposals(target_id,created_at DESC);
        CREATE TABLE IF NOT EXISTS workflow_events (
            id TEXT PRIMARY KEY, proposal_id TEXT NOT NULL, action TEXT NOT NULL,
            target_id TEXT NOT NULL, at TEXT NOT NULL, detail TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_events_target ON workflow_events(target_id,at DESC);")
        .map_err(|error| format!("数据库初始化失败：{error}"))?;
    let has_edits = connection.prepare("PRAGMA table_info(workflow_proposals)")
        .and_then(|mut query| query.query_map([], |row| row.get::<_, String>(1))
            .and_then(|rows| rows.collect::<Result<Vec<_>, _>>()))
        .map_err(|error| format!("操作建议结构检查失败：{error}"))?
        .iter().any(|name| name == "edits_json");
    if !has_edits {
        connection.execute("ALTER TABLE workflow_proposals ADD COLUMN edits_json TEXT NOT NULL DEFAULT '[]'", [])
            .map_err(|error| format!("操作建议结构升级失败：{error}"))?;
    }
    Ok(connection)
}

fn replace_record_segments(transaction: &Transaction<'_>, record: &KnowledgeRecord) -> Result<(), String> {
    transaction.execute("DELETE FROM record_segments WHERE record_id=?1", [&record.id])
        .map_err(|error| format!("旧资料片段清理失败：{error}"))?;
    if record.record_type == "video" && record.transcript.as_ref().is_some_and(|cues| !cues.is_empty()) {
        for (position, cue) in record.transcript.as_ref().into_iter().flatten().enumerate() {
            let ordinal = i64::try_from(position).map_err(|_| "字幕数量超过数据库上限")?;
            transaction.execute("INSERT INTO record_segments(record_id,ordinal,at_seconds,body) VALUES (?1,?2,?3,?4)",
                params![&record.id, ordinal, cue.at_seconds, &cue.text])
                .map_err(|error| format!("字幕片段索引失败：{error}"))?;
        }
    } else {
        let chars: Vec<char> = record.content_text.chars().collect();
        for (position, start) in (0..chars.len()).step_by(550).enumerate() {
            let part = &chars[start..(start + 750).min(chars.len())];
            let ordinal = i64::try_from(position).map_err(|_| "正文片段数量超过数据库上限")?;
            let body: String = part.iter().collect();
            transaction.execute("INSERT INTO record_segments(record_id,ordinal,at_seconds,body) VALUES (?1,?2,NULL,?3)",
                params![&record.id, ordinal, body])
                .map_err(|error| format!("正文片段索引失败：{error}"))?;
        }
    }
    Ok(())
}

fn ensure_segment_index(transaction: &Transaction<'_>, records: &[KnowledgeRecord]) -> Result<(), String> {
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='segments'", [], |row| row.get(0))
        .optional().map_err(|error| format!("资料片段版本读取失败：{error}"))?;
    match version {
        Some(1) => Ok(()),
        Some(_) => Err("资料片段索引版本高于当前程序，无法写入".into()),
        None => {
            transaction.execute("DELETE FROM record_segments", []).map_err(|error| format!("旧片段索引清理失败：{error}"))?;
            for record in records { replace_record_segments(transaction, record)?; }
            transaction.execute("INSERT INTO entity_meta(name,version) VALUES ('segments',1)", [])
                .map_err(|error| format!("资料片段版本写入失败：{error}"))?;
            Ok(())
        }
    }
}

fn sync_record_rows(transaction: &Transaction<'_>, records: &[KnowledgeRecord]) -> Result<(), String> {
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='records'", [], |row| row.get(0))
        .optional().map_err(|error| format!("资料迁移状态读取失败：{error}"))?;
    if version.is_some_and(|value| value != 1 && value != 2) { return Err("资料数据库版本高于当前程序，无法写入".into()); }
    let segments_ready: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='segments'", [], |row| row.get(0))
        .optional().map_err(|error| format!("资料片段版本读取失败：{error}"))?;
    if segments_ready.is_some_and(|value| value != 1) { return Err("资料片段索引版本高于当前程序，无法写入".into()); }
    let mut seen = HashSet::new();
    for record in records {
        if record.id.is_empty() || !seen.insert(record.id.as_str()) { return Err("资料编号为空或重复，未写入数据库".into()); }
    }
    let mut stale = HashSet::new();
    {
        let mut query = transaction.prepare("SELECT id FROM record_rows").map_err(|error| format!("资料索引读取失败：{error}"))?;
        let ids = query.query_map([], |row| row.get::<_, String>(0)).map_err(|error| format!("资料索引读取失败：{error}"))?;
        for id in ids { stale.insert(id.map_err(|error| format!("资料索引读取失败：{error}"))?); }
    }
    for (position, record) in records.iter().enumerate() {
        let ordinal = i64::try_from(position).map_err(|_| "资料数量超过数据库上限")?;
        let json = serde_json::to_string(record).map_err(|error| format!("资料编码失败：{error}"))?;
        let content_changed = if segments_ready == Some(1) {
            transaction.query_row("SELECT json FROM record_rows WHERE id=?1", [&record.id], |row| row.get::<_, String>(0))
                .optional().map_err(|error| format!("资料变更检查失败：{error}"))?.as_deref() != Some(json.as_str())
        } else { false };
        let status = match record.status {
            crate::ProcessStatus::Extracting => "extracting",
            crate::ProcessStatus::Summarizing => "summarizing",
            crate::ProcessStatus::Ready => "ready",
            crate::ProcessStatus::Fault => "fault",
        };
        transaction.execute("INSERT INTO record_rows(id,ordinal,record_type,title,site,status,source_url,saved_at,json)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
            ON CONFLICT(id) DO UPDATE SET ordinal=excluded.ordinal, record_type=excluded.record_type,
                title=excluded.title, site=excluded.site, status=excluded.status,
                source_url=excluded.source_url, saved_at=excluded.saved_at, json=excluded.json
            WHERE record_rows.ordinal != excluded.ordinal OR record_rows.json != excluded.json",
            params![&record.id, ordinal, &record.record_type, &record.title, &record.site, status, &record.source_url, &record.saved_at, json])
            .map_err(|error| format!("资料逐条写入失败：{error}"))?;
        if content_changed { replace_record_segments(transaction, record)?; }
        stale.remove(&record.id);
    }
    for id in stale {
        if segments_ready == Some(1) {
            transaction.execute("DELETE FROM record_segments WHERE record_id=?1", [&id])
                .map_err(|error| format!("旧资料片段清理失败：{error}"))?;
        }
        transaction.execute("DELETE FROM record_rows WHERE id=?1", [id]).map_err(|error| format!("旧资料清理失败：{error}"))?;
    }
    transaction.execute("INSERT INTO entity_meta(name,version) VALUES ('records',2) ON CONFLICT(name) DO UPDATE SET version=excluded.version", [])
        .map_err(|error| format!("资料迁移状态写入失败：{error}"))?;
    Ok(())
}

pub fn read_records(path: &Path) -> Result<Option<Vec<KnowledgeRecord>>, String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("资料读取事务启动失败：{error}"))?;
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='records'", [], |row| row.get(0))
        .optional().map_err(|error| format!("资料迁移状态读取失败：{error}"))?;
    match version {
        Some(1 | 2) => {
            let rows = (|| {
                let mut query = transaction.prepare("SELECT json FROM record_rows ORDER BY ordinal ASC")
                    .map_err(|error| format!("资料逐条读取失败：{error}"))?;
                let values = query.query_map([], |row| row.get::<_, String>(0)).map_err(|error| format!("资料逐条读取失败：{error}"))?;
                let mut records = Vec::new();
                for row in values {
                    let json = row.map_err(|error| format!("资料逐条读取失败：{error}"))?;
                    records.push(serde_json::from_str(&json).map_err(|error| format!("资料记录无效：{error}"))?);
                }
                Ok::<Vec<KnowledgeRecord>, String>(records)
            })();
            let snapshot: Option<String> = if version == Some(1) {
                transaction.query_row("SELECT json FROM snapshots WHERE name='records'", [], |row| row.get(0))
                    .optional().map_err(|error| format!("资料快照读取失败：{error}"))?
            } else { None };
            if let Some(snapshot) = snapshot {
                let current = rows.as_ref().ok().and_then(|records| serde_json::to_string(records).ok());
                if current.as_deref() != Some(snapshot.as_str()) {
                    let recovered: Vec<KnowledgeRecord> = serde_json::from_str(&snapshot).map_err(|error| format!("资料快照与逐条记录不一致，且快照无效：{error}"))?;
                    sync_record_rows(&transaction, &recovered)?;
                    ensure_segment_index(&transaction, &recovered)?;
                    transaction.commit().map_err(|error| format!("资料同步提交失败：{error}"))?;
                    return Ok(Some(recovered));
                }
            }
            let records = rows?;
            if version == Some(1) {
                transaction.execute("UPDATE entity_meta SET version=2 WHERE name='records'", [])
                    .map_err(|error| format!("资料版本切换失败：{error}"))?;
            }
            ensure_segment_index(&transaction, &records)?;
            transaction.commit().map_err(|error| format!("资料读取事务结束失败：{error}"))?;
            Ok(Some(records))
        }
        Some(_) => Err("资料数据库版本高于当前程序，无法读取".into()),
        None => {
            let snapshot: Option<String> = transaction.query_row("SELECT json FROM snapshots WHERE name='records'", [], |row| row.get(0))
                .optional().map_err(|error| format!("旧资料快照读取失败：{error}"))?;
            let Some(snapshot) = snapshot else {
                transaction.commit().map_err(|error| format!("资料读取事务结束失败：{error}"))?;
                return Ok(None);
            };
            let records: Vec<KnowledgeRecord> = serde_json::from_str(&snapshot).map_err(|error| format!("旧资料快照无效：{error}"))?;
            sync_record_rows(&transaction, &records)?;
            ensure_segment_index(&transaction, &records)?;
            transaction.commit().map_err(|error| format!("资料迁移提交失败：{error}"))?;
            Ok(Some(records))
        }
    }
}

pub fn write_records(path: &Path, records: &[KnowledgeRecord]) -> Result<(), String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("资料事务启动失败：{error}"))?;
    sync_record_rows(&transaction, records)?;
    ensure_segment_index(&transaction, records)?;
    transaction.commit().map_err(|error| format!("资料事务提交失败：{error}"))
}

pub fn upsert_record(path: &Path, record: &KnowledgeRecord, position: usize) -> Result<(), String> {
    if record.id.is_empty() { return Err("资料编号为空，未写入数据库".into()); }
    let ordinal = i64::try_from(position).map_err(|_| "资料数量超过数据库上限")?;
    let json = serde_json::to_string(record).map_err(|error| format!("资料编码失败：{error}"))?;
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("资料事务启动失败：{error}"))?;
    for (name, label) in [("records", "资料"), ("segments", "资料片段")] {
        let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name=?1", [name], |row| row.get(0))
            .optional().map_err(|error| format!("{label}版本读取失败：{error}"))?;
        if version != Some(if name == "records" { 2 } else { 1 }) { return Err(format!("{label}尚未完成迁移，请重启应用")); }
    }
    let previous: Option<(i64, String)> = transaction.query_row("SELECT ordinal,json FROM record_rows WHERE id=?1", [&record.id],
        |row| Ok((row.get(0)?, row.get(1)?))).optional().map_err(|error| format!("资料变更检查失败：{error}"))?;
    match &previous {
        Some((old_ordinal, _)) if *old_ordinal != ordinal => return Err("资料位置与数据库不一致，未写入".into()),
        None => {
            let count: i64 = transaction.query_row("SELECT count(*) FROM record_rows", [], |row| row.get(0))
                .map_err(|error| format!("资料计数失败：{error}"))?;
            if ordinal > count { return Err("资料插入位置无效".into()); }
            transaction.execute("UPDATE record_rows SET ordinal=ordinal+1 WHERE ordinal>=?1", [ordinal])
                .map_err(|error| format!("资料顺序更新失败：{error}"))?;
        }
        _ => {}
    }
    let status = match record.status {
        crate::ProcessStatus::Extracting => "extracting",
        crate::ProcessStatus::Summarizing => "summarizing",
        crate::ProcessStatus::Ready => "ready",
        crate::ProcessStatus::Fault => "fault",
    };
    transaction.execute("INSERT INTO record_rows(id,ordinal,record_type,title,site,status,source_url,saved_at,json)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
        ON CONFLICT(id) DO UPDATE SET record_type=excluded.record_type,title=excluded.title,site=excluded.site,
            status=excluded.status,source_url=excluded.source_url,saved_at=excluded.saved_at,json=excluded.json
        WHERE record_rows.json != excluded.json",
        params![&record.id, ordinal, &record.record_type, &record.title, &record.site, status, &record.source_url, &record.saved_at, json])
        .map_err(|error| format!("资料逐条写入失败：{error}"))?;
    if previous.as_ref().is_none_or(|(_, old)| old != &json) { replace_record_segments(&transaction, record)?; }
    transaction.commit().map_err(|error| format!("资料事务提交失败：{error}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPage {
    pub items: Vec<RecordSummary>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

pub fn query_record_page(path: &Path, record_type: &str, search: &str, offset: u64, limit: u64) -> Result<RecordPage, String> {
    if !matches!(record_type, "all" | "video" | "article") { return Err("资料类型筛选无效".into()); }
    if !(1..=100).contains(&limit) { return Err("每页资料数量应为 1–100 条".into()); }
    if search.chars().count() > 200 { return Err("搜索词过长".into()); }
    let offset = i64::try_from(offset).map_err(|_| "资料页码过大")?;
    let limit = i64::try_from(limit).map_err(|_| "资料页码过大")?;
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("资料查询事务启动失败：{error}"))?;
    let search = search.trim();
    let scan_short = search.chars().count() < 3 || search.contains('"');
    let fts_query = if scan_short { "\"knowflowfallback\"".to_owned() } else { format!("\"{}\"", search) };
    let predicate = "(?1 = 'all' OR record_type = ?1) AND (?2 = '' OR
        instr(lower(title), lower(?2)) > 0 OR instr(lower(site), lower(?2)) > 0 OR
        id IN (SELECT segment.record_id FROM segment_fts JOIN record_segments AS segment ON segment.rowid=segment_fts.rowid WHERE segment_fts MATCH ?3) OR
        (?4 = 1 AND id IN (SELECT record_id FROM record_segments WHERE instr(lower(body), lower(?2)) > 0)))";
    let total: i64 = transaction.query_row(&format!("SELECT count(*) FROM record_rows WHERE {predicate}"), params![record_type, search, fts_query, scan_short], |row| row.get(0))
        .map_err(|error| format!("资料计数失败：{error}"))?;
    let mut items = Vec::new();
    {
        let mut query = transaction.prepare(&format!("SELECT id,record_type,title,site,status,source_url,saved_at,
            json_extract(json,'$.durationSeconds'),json_extract(json,'$.captionLanguage'),json_extract(json,'$.faultReason')
            FROM record_rows WHERE {predicate} ORDER BY ordinal ASC LIMIT ?5 OFFSET ?6"))
            .map_err(|error| format!("资料分页查询失败：{error}"))?;
        let rows = query.query_map(params![record_type, search, fts_query, scan_short, limit, offset], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, String>(6)?,
                row.get::<_, Option<u32>>(7)?, row.get::<_, Option<String>>(8)?, row.get::<_, Option<String>>(9)?))
        })
            .map_err(|error| format!("资料分页查询失败：{error}"))?;
        for row in rows {
            let (id, record_type, title, site, status, source_url, saved_at, duration_seconds, caption_language, fault_reason) = row.map_err(|error| format!("资料分页查询失败：{error}"))?;
            let status = match status.as_str() {
                "extracting" => crate::ProcessStatus::Extracting,
                "summarizing" => crate::ProcessStatus::Summarizing,
                "ready" => crate::ProcessStatus::Ready,
                "fault" => crate::ProcessStatus::Fault,
                _ => return Err("资料状态索引无效".into()),
            };
            items.push(RecordSummary { id, record_type, title, site, status, source_url, saved_at, duration_seconds, caption_language, fault_reason });
        }
    }
    transaction.commit().map_err(|error| format!("资料查询事务结束失败：{error}"))?;
    Ok(RecordPage { items, total: total as u64, offset: offset as u64, limit: limit as u64 })
}

pub fn get_record(path: &Path, id: &str) -> Result<KnowledgeRecord, String> {
    let connection = open(path)?;
    let json: String = connection.query_row("SELECT json FROM record_rows WHERE id=?1", [id], |row| row.get(0))
        .optional().map_err(|error| format!("资料读取失败：{error}"))?.ok_or("资料不存在")?;
    serde_json::from_str(&json).map_err(|error| format!("资料记录无效：{error}"))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSegment {
    pub ordinal: i64,
    pub at_seconds: Option<u32>,
    pub text: String,
}

pub fn query_record_segments(path: &Path, record_id: &str, start_at: Option<u32>, end_at: Option<u32>, search: &str, limit: u64) -> Result<Vec<RecordSegment>, String> {
    if record_id.is_empty() || record_id.len() > 120 { return Err("资料编号无效".into()); }
    if !(1..=100).contains(&limit) { return Err("片段数量应为 1–100 条".into()); }
    let limit = i64::try_from(limit).map_err(|_| "片段数量超限")?;
    if search.chars().count() > 200 { return Err("搜索词过长".into()); }
    if start_at.zip(end_at).is_some_and(|(start, end)| start > end) { return Err("结束时间不能早于开始时间".into()); }
    let start_at = start_at.unwrap_or(0);
    let end_at = end_at.unwrap_or(u32::MAX);
    let connection = open(path)?;
    let mut query = connection.prepare("SELECT ordinal,at_seconds,body FROM record_segments
        WHERE record_id=?1 AND at_seconds BETWEEN ?2 AND ?3
        AND (?4 = '' OR instr(lower(body),lower(?4)) > 0)
        ORDER BY ordinal ASC LIMIT ?5")
        .map_err(|error| format!("资料片段查询失败：{error}"))?;
    let rows = query.query_map(params![record_id, start_at, end_at, search.trim(), limit], |row| {
        Ok(RecordSegment { ordinal: row.get(0)?, at_seconds: row.get(1)?, text: row.get(2)? })
    }).map_err(|error| format!("资料片段查询失败：{error}"))?;
    rows.map(|row| row.map_err(|error| format!("资料片段查询失败：{error}"))).collect()
}

fn sync_note_rows(transaction: &Transaction<'_>, notes: &[Note]) -> Result<(), String> {
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='notes'", [], |row| row.get(0))
        .optional().map_err(|error| format!("笔记迁移状态读取失败：{error}"))?;
    if version.is_some_and(|value| value != 1 && value != 2) { return Err("笔记数据库版本高于当前程序，无法写入".into()); }
    let mut stale = HashSet::new();
    {
        let mut query = transaction.prepare("SELECT id FROM note_rows").map_err(|error| format!("笔记索引读取失败：{error}"))?;
        let ids = query.query_map([], |row| row.get::<_, String>(0)).map_err(|error| format!("笔记索引读取失败：{error}"))?;
        for id in ids { stale.insert(id.map_err(|error| format!("笔记索引读取失败：{error}"))?); }
    }
    let mut seen = HashSet::new();
    for (position, note) in notes.iter().enumerate() {
        if note.id.is_empty() || !seen.insert(note.id.as_str()) { return Err("笔记编号为空或重复，未写入数据库".into()); }
        let ordinal = i64::try_from(position).map_err(|_| "笔记数量超过数据库上限")?;
        let updated_at = i64::try_from(note.updated_at).map_err(|_| "笔记时间超过数据库上限")?;
        let json = serde_json::to_string(note).map_err(|error| format!("笔记编码失败：{error}"))?;
        transaction.execute("INSERT INTO note_rows(id,ordinal,title,updated_at,json) VALUES (?1,?2,?3,?4,?5)
            ON CONFLICT(id) DO UPDATE SET ordinal=excluded.ordinal, title=excluded.title, updated_at=excluded.updated_at, json=excluded.json
            WHERE note_rows.ordinal != excluded.ordinal OR note_rows.json != excluded.json",
            params![&note.id, ordinal, &note.title, updated_at, json])
            .map_err(|error| format!("笔记逐条写入失败：{error}"))?;
        stale.remove(&note.id);
    }
    for id in stale {
        transaction.execute("DELETE FROM note_rows WHERE id=?1", [id]).map_err(|error| format!("旧笔记清理失败：{error}"))?;
    }
    transaction.execute("INSERT INTO entity_meta(name,version) VALUES ('notes',2) ON CONFLICT(name) DO UPDATE SET version=excluded.version", [])
        .map_err(|error| format!("笔记迁移状态写入失败：{error}"))?;
    Ok(())
}

pub fn read_notes(path: &Path) -> Result<Option<Vec<Note>>, String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("笔记读取事务启动失败：{error}"))?;
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='notes'", [], |row| row.get(0))
        .optional().map_err(|error| format!("笔记迁移状态读取失败：{error}"))?;
    match version {
        Some(1 | 2) => {
            let rows = (|| {
                let mut query = transaction.prepare("SELECT json FROM note_rows ORDER BY ordinal ASC")
                    .map_err(|error| format!("笔记逐条读取失败：{error}"))?;
                let values = query.query_map([], |row| row.get::<_, String>(0)).map_err(|error| format!("笔记逐条读取失败：{error}"))?;
                let mut notes = Vec::new();
                for value in values {
                    notes.push(serde_json::from_str(&value.map_err(|error| format!("笔记逐条读取失败：{error}"))?)
                        .map_err(|error| format!("笔记记录无效：{error}"))?);
                }
                Ok::<Vec<Note>, String>(notes)
            })();
            let snapshot: Option<String> = if version == Some(1) {
                transaction.query_row("SELECT json FROM snapshots WHERE name='notes'", [], |row| row.get(0))
                    .optional().map_err(|error| format!("笔记快照读取失败：{error}"))?
            } else { None };
            if let Some(snapshot) = snapshot {
                let current = rows.as_ref().ok().and_then(|notes| serde_json::to_string(notes).ok());
                if current.as_deref() != Some(snapshot.as_str()) {
                    let recovered: Vec<Note> = serde_json::from_str(&snapshot).map_err(|error| format!("笔记快照与逐条记录不一致，且快照无效：{error}"))?;
                    sync_note_rows(&transaction, &recovered)?;
                    transaction.commit().map_err(|error| format!("笔记同步提交失败：{error}"))?;
                    return Ok(Some(recovered));
                }
            }
            let notes = rows?;
            if version == Some(1) {
                transaction.execute("UPDATE entity_meta SET version=2 WHERE name='notes'", [])
                    .map_err(|error| format!("笔记版本切换失败：{error}"))?;
            }
            transaction.commit().map_err(|error| format!("笔记读取事务结束失败：{error}"))?;
            Ok(Some(notes))
        }
        Some(_) => Err("笔记数据库版本高于当前程序，无法读取".into()),
        None => {
            let snapshot: Option<String> = transaction.query_row("SELECT json FROM snapshots WHERE name='notes'", [], |row| row.get(0))
                .optional().map_err(|error| format!("旧笔记快照读取失败：{error}"))?;
            let Some(snapshot) = snapshot else {
                transaction.commit().map_err(|error| format!("笔记读取事务结束失败：{error}"))?;
                return Ok(None);
            };
            let notes: Vec<Note> = serde_json::from_str(&snapshot).map_err(|error| format!("旧笔记快照无效：{error}"))?;
            sync_note_rows(&transaction, &notes)?;
            transaction.commit().map_err(|error| format!("笔记迁移提交失败：{error}"))?;
            Ok(Some(notes))
        }
    }
}

pub fn write_notes(path: &Path, notes: &[Note]) -> Result<(), String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("笔记事务启动失败：{error}"))?;
    sync_note_rows(&transaction, notes)?;
    transaction.commit().map_err(|error| format!("笔记事务提交失败：{error}"))
}

pub fn upsert_note(path: &Path, note: &Note, position: usize) -> Result<(), String> {
    if note.id.is_empty() { return Err("笔记编号为空，未写入数据库".into()); }
    let ordinal = i64::try_from(position).map_err(|_| "笔记数量超过数据库上限")?;
    let updated_at = i64::try_from(note.updated_at).map_err(|_| "笔记时间超过数据库上限")?;
    let json = serde_json::to_string(note).map_err(|error| format!("笔记编码失败：{error}"))?;
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("笔记事务启动失败：{error}"))?;
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='notes'", [], |row| row.get(0))
        .optional().map_err(|error| format!("笔记版本读取失败：{error}"))?;
    if version != Some(2) { return Err("笔记尚未完成迁移，请重启应用".into()); }
    let previous: Option<i64> = transaction.query_row("SELECT ordinal FROM note_rows WHERE id=?1", [&note.id], |row| row.get(0))
        .optional().map_err(|error| format!("笔记位置读取失败：{error}"))?;
    match previous {
        Some(old) if old != ordinal => return Err("笔记位置与数据库不一致，未写入".into()),
        None => {
            let count: i64 = transaction.query_row("SELECT count(*) FROM note_rows", [], |row| row.get(0))
                .map_err(|error| format!("笔记计数失败：{error}"))?;
            if ordinal != count { return Err("笔记插入位置无效".into()); }
        }
        _ => {}
    }
    transaction.execute("INSERT INTO note_rows(id,ordinal,title,updated_at,json) VALUES (?1,?2,?3,?4,?5)
        ON CONFLICT(id) DO UPDATE SET title=excluded.title,updated_at=excluded.updated_at,json=excluded.json
        WHERE note_rows.json != excluded.json",
        params![&note.id, ordinal, &note.title, updated_at, json]).map_err(|error| format!("笔记逐条写入失败：{error}"))?;
    transaction.commit().map_err(|error| format!("笔记事务提交失败：{error}"))
}

pub fn list_note_folders(path: &Path) -> Result<Vec<NoteFolder>, String> {
    let connection = open(path)?;
    let mut query = connection.prepare("SELECT id,name,created_at FROM note_folders ORDER BY name COLLATE NOCASE,id")
        .map_err(|error| format!("文件夹读取失败：{error}"))?;
    let rows = query.query_map([], |row| Ok(NoteFolder { id: row.get(0)?, name: row.get(1)?, created_at: row.get(2)? }))
        .map_err(|error| format!("文件夹读取失败：{error}"))?;
    rows.map(|row| row.map_err(|error| format!("文件夹读取失败：{error}"))).collect()
}

pub fn note_folder_exists(path: &Path, id: &str) -> Result<bool, String> {
    let connection = open(path)?;
    let exists: i64 = connection.query_row("SELECT EXISTS(SELECT 1 FROM note_folders WHERE id=?1)", [id], |row| row.get(0))
        .map_err(|error| format!("文件夹检查失败：{error}"))?;
    Ok(exists == 1)
}

pub fn create_note_folder(path: &Path, folder: &NoteFolder) -> Result<(), String> {
    let connection = open(path)?;
    connection.execute("INSERT INTO note_folders(id,name,created_at) VALUES (?1,?2,?3)",
        params![&folder.id, &folder.name, &folder.created_at]).map_err(|error| format!("文件夹创建失败，名称可能已存在：{error}"))?;
    Ok(())
}

pub fn rename_note_folder(path: &Path, id: &str, name: &str) -> Result<(), String> {
    let connection = open(path)?;
    let changed = connection.execute("UPDATE note_folders SET name=?1 WHERE id=?2", params![name, id])
        .map_err(|error| format!("文件夹重命名失败，名称可能已存在：{error}"))?;
    if changed != 1 { return Err("文件夹不存在".into()); }
    Ok(())
}

pub fn delete_note_folder(path: &Path, id: &str, notes: &[Note], timestamp: u128) -> Result<Vec<Note>, String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("文件夹事务启动失败：{error}"))?;
    let exists: i64 = transaction.query_row("SELECT EXISTS(SELECT 1 FROM note_folders WHERE id=?1)", [id], |row| row.get(0))
        .map_err(|error| format!("文件夹检查失败：{error}"))?;
    if exists != 1 { return Err("文件夹不存在".into()); }
    let stored_count: i64 = transaction.query_row("SELECT count(*) FROM note_rows WHERE json_extract(json,'$.folderId')=?1", [id], |row| row.get(0))
        .map_err(|error| format!("文件夹笔记检查失败：{error}"))?;
    let mut next = notes.to_vec();
    let mut moved = 0_i64;
    for note in next.iter_mut().filter(|note| note.folder_id.as_deref() == Some(id)) {
        let before_json = serde_json::to_string(note).map_err(|error| format!("笔记编码失败：{error}"))?;
        note.folder_id = None;
        note.updated_at = timestamp.max(note.updated_at.saturating_add(1));
        let after_json = serde_json::to_string(note).map_err(|error| format!("笔记编码失败：{error}"))?;
        let changed = transaction.execute("UPDATE note_rows SET updated_at=?1,json=?2 WHERE id=?3 AND json=?4",
            params![i64::try_from(note.updated_at).map_err(|_| "笔记时间超限")?, after_json, &note.id, before_json])
            .map_err(|error| format!("笔记移出文件夹失败：{error}"))?;
        if changed != 1 { return Err("笔记已变化，请刷新后重试".into()); }
        moved += 1;
    }
    if moved != stored_count { return Err("文件夹笔记数量已变化，请刷新后重试".into()); }
    transaction.execute("DELETE FROM note_folders WHERE id=?1", [id]).map_err(|error| format!("文件夹删除失败：{error}"))?;
    transaction.commit().map_err(|error| format!("文件夹删除提交失败：{error}"))?;
    Ok(next)
}

fn sync_chat_rows(transaction: &Transaction<'_>, store: &ChatStore) -> Result<(), String> {
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='chat'", [], |row| row.get(0))
        .optional().map_err(|error| format!("会话迁移状态读取失败：{error}"))?;
    if version.is_some_and(|value| value != 1 && value != 2) { return Err("会话数据库版本高于当前程序，无法写入".into()); }
    if store.sessions.is_empty() || !store.sessions.iter().any(|session| session.id == store.active_id) {
        return Err("会话缺少当前会话，未写入数据库".into());
    }
    let mut old_sessions = HashSet::new();
    {
        let mut query = transaction.prepare("SELECT id FROM session_rows").map_err(|error| format!("会话索引读取失败：{error}"))?;
        for row in query.query_map([], |row| row.get::<_, String>(0)).map_err(|error| format!("会话索引读取失败：{error}"))? {
            old_sessions.insert(row.map_err(|error| format!("会话索引读取失败：{error}"))?);
        }
    }
    let mut old_messages = HashSet::new();
    {
        let mut query = transaction.prepare("SELECT session_id,id FROM message_rows").map_err(|error| format!("消息索引读取失败：{error}"))?;
        for row in query.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map_err(|error| format!("消息索引读取失败：{error}"))? {
            old_messages.insert(row.map_err(|error| format!("消息索引读取失败：{error}"))?);
        }
    }
    let mut seen_sessions = HashSet::new();
    for (position, session) in store.sessions.iter().enumerate() {
        if session.id.is_empty() || !seen_sessions.insert(session.id.as_str()) { return Err("会话编号为空或重复，未写入数据库".into()); }
        let ordinal = i64::try_from(position).map_err(|_| "会话数量超过数据库上限")?;
        let mut metadata = session.clone();
        metadata.messages.clear();
        let json = serde_json::to_string(&metadata).map_err(|error| format!("会话编码失败：{error}"))?;
        transaction.execute("INSERT INTO session_rows(id,ordinal,title,project_id,updated_at,json) VALUES (?1,?2,?3,?4,?5,?6)
            ON CONFLICT(id) DO UPDATE SET ordinal=excluded.ordinal,title=excluded.title,project_id=excluded.project_id,
                updated_at=excluded.updated_at,json=excluded.json
            WHERE session_rows.ordinal != excluded.ordinal OR session_rows.json != excluded.json",
            params![&session.id, ordinal, &session.title, &session.project_id, &session.updated_at, json])
            .map_err(|error| format!("会话逐条写入失败：{error}"))?;
        old_sessions.remove(&session.id);
        let mut seen_messages = HashSet::new();
        for (message_position, message) in session.messages.iter().enumerate() {
            if message.id.is_empty() || !seen_messages.insert(message.id.as_str()) { return Err("消息编号为空或重复，未写入数据库".into()); }
            let message_ordinal = i64::try_from(message_position).map_err(|_| "消息数量超过数据库上限")?;
            let message_json = serde_json::to_string(message).map_err(|error| format!("消息编码失败：{error}"))?;
            transaction.execute("INSERT INTO message_rows(session_id,id,ordinal,role,created_at,json) VALUES (?1,?2,?3,?4,?5,?6)
                ON CONFLICT(session_id,id) DO UPDATE SET ordinal=excluded.ordinal,role=excluded.role,
                    created_at=excluded.created_at,json=excluded.json
                WHERE message_rows.ordinal != excluded.ordinal OR message_rows.json != excluded.json",
                params![&session.id, &message.id, message_ordinal, &message.role, &message.created_at, message_json])
                .map_err(|error| format!("消息逐条写入失败：{error}"))?;
            old_messages.remove(&(session.id.clone(), message.id.clone()));
        }
    }
    for (session_id, id) in old_messages {
        transaction.execute("DELETE FROM message_rows WHERE session_id=?1 AND id=?2", params![session_id, id])
            .map_err(|error| format!("旧消息清理失败：{error}"))?;
    }
    for id in old_sessions {
        transaction.execute("DELETE FROM session_rows WHERE id=?1", [id]).map_err(|error| format!("旧会话清理失败：{error}"))?;
    }
    let memory = serde_json::to_string(&store.project_memory).map_err(|error| format!("项目记忆编码失败：{error}"))?;
    transaction.execute("INSERT INTO chat_meta(id,active_id,project_memory_json) VALUES (1,?1,?2)
        ON CONFLICT(id) DO UPDATE SET active_id=excluded.active_id,project_memory_json=excluded.project_memory_json",
        params![&store.active_id, memory]).map_err(|error| format!("会话元数据写入失败：{error}"))?;
    transaction.execute("INSERT INTO entity_meta(name,version) VALUES ('chat',2) ON CONFLICT(name) DO UPDATE SET version=excluded.version", [])
        .map_err(|error| format!("会话迁移状态写入失败：{error}"))?;
    Ok(())
}

pub fn read_chat(path: &Path) -> Result<Option<ChatStore>, String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("会话读取事务启动失败：{error}"))?;
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='chat'", [], |row| row.get(0))
        .optional().map_err(|error| format!("会话迁移状态读取失败：{error}"))?;
    match version {
        Some(1 | 2) => {
            let rows = (|| {
                let (active_id, memory): (String, String) = transaction.query_row("SELECT active_id,project_memory_json FROM chat_meta WHERE id=1", [],
                    |row| Ok((row.get(0)?, row.get(1)?))).map_err(|error| format!("会话元数据读取失败：{error}"))?;
                let project_memory = serde_json::from_str(&memory).map_err(|error| format!("项目记忆无效：{error}"))?;
                let mut sessions = Vec::new();
                let mut query = transaction.prepare("SELECT json FROM session_rows ORDER BY ordinal ASC")
                    .map_err(|error| format!("会话逐条读取失败：{error}"))?;
                for row in query.query_map([], |row| row.get::<_, String>(0)).map_err(|error| format!("会话逐条读取失败：{error}"))? {
                    let mut session: ChatSession = serde_json::from_str(&row.map_err(|error| format!("会话逐条读取失败：{error}"))?)
                        .map_err(|error| format!("会话记录无效：{error}"))?;
                    let mut messages = transaction.prepare("SELECT json FROM message_rows WHERE session_id=?1 ORDER BY ordinal ASC")
                        .map_err(|error| format!("消息逐条读取失败：{error}"))?;
                    for message in messages.query_map([&session.id], |row| row.get::<_, String>(0))
                        .map_err(|error| format!("消息逐条读取失败：{error}"))? {
                        session.messages.push(serde_json::from_str::<ChatMessage>(&message.map_err(|error| format!("消息逐条读取失败：{error}"))?)
                            .map_err(|error| format!("消息记录无效：{error}"))?);
                    }
                    sessions.push(session);
                }
                Ok::<ChatStore, String>(ChatStore { active_id, sessions, project_memory })
            })();
            let snapshot: Option<String> = if version == Some(1) {
                transaction.query_row("SELECT json FROM snapshots WHERE name='chat'", [], |row| row.get(0))
                    .optional().map_err(|error| format!("会话快照读取失败：{error}"))?
            } else { None };
            if let Some(snapshot) = snapshot {
                let current = rows.as_ref().ok().and_then(|store| serde_json::to_string(store).ok());
                if current.as_deref() != Some(snapshot.as_str()) {
                    let recovered: ChatStore = serde_json::from_str(&snapshot).map_err(|error| format!("会话快照与逐条记录不一致，且快照无效：{error}"))?;
                    sync_chat_rows(&transaction, &recovered)?;
                    transaction.commit().map_err(|error| format!("会话同步提交失败：{error}"))?;
                    return Ok(Some(recovered));
                }
            }
            let store = rows?;
            if version == Some(1) {
                transaction.execute("UPDATE entity_meta SET version=2 WHERE name='chat'", [])
                    .map_err(|error| format!("会话版本切换失败：{error}"))?;
            }
            transaction.commit().map_err(|error| format!("会话读取事务结束失败：{error}"))?;
            Ok(Some(store))
        }
        Some(_) => Err("会话数据库版本高于当前程序，无法读取".into()),
        None => {
            let snapshot: Option<String> = transaction.query_row("SELECT json FROM snapshots WHERE name='chat'", [], |row| row.get(0))
                .optional().map_err(|error| format!("旧会话快照读取失败：{error}"))?;
            let Some(snapshot) = snapshot else {
                transaction.commit().map_err(|error| format!("会话读取事务结束失败：{error}"))?;
                return Ok(None);
            };
            let store: ChatStore = serde_json::from_str(&snapshot).map_err(|error| format!("旧会话快照无效：{error}"))?;
            sync_chat_rows(&transaction, &store)?;
            transaction.commit().map_err(|error| format!("会话迁移提交失败：{error}"))?;
            Ok(Some(store))
        }
    }
}

pub fn write_chat(path: &Path, store: &ChatStore) -> Result<(), String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("会话事务启动失败：{error}"))?;
    sync_chat_rows(&transaction, store)?;
    transaction.commit().map_err(|error| format!("会话事务提交失败：{error}"))
}

#[cfg(test)]
pub fn read<T: DeserializeOwned>(path: &Path, name: &str) -> Result<Option<T>, String> {
    let connection = open(path)?;
    let json: Option<String> = connection.query_row("SELECT json FROM snapshots WHERE name = ?1", [name], |row| row.get(0))
        .optional().map_err(|error| format!("数据库读取失败：{error}"))?;
    json.map(|value| serde_json::from_str(&value).map_err(|error| format!("数据库数据无效：{error}"))).transpose()
}

#[cfg(test)]
pub fn write<T: Serialize>(path: &Path, name: &str, value: &T) -> Result<(), String> {
    let json = serde_json::to_string(value).map_err(|error| format!("数据库数据编码失败：{error}"))?;
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("数据库事务启动失败：{error}"))?;
    transaction.execute("INSERT INTO snapshots(name,json) VALUES (?1,?2) ON CONFLICT(name) DO UPDATE SET json=excluded.json", params![name, json])
        .map_err(|error| format!("数据库写入失败：{error}"))?;
    transaction.commit().map_err(|error| format!("数据库提交失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct BadEncoding;
    impl Serialize for BadEncoding {
        fn serialize<S>(&self, _: S) -> Result<S::Ok, S::Error>
        where S: serde::Serializer {
            Err(serde::ser::Error::custom("deliberate encoding failure"))
        }
    }

    #[test]
    fn snapshots_are_separate_and_failed_encoding_preserves_old_value() {
        let dir = std::env::temp_dir().join(format!("knowflow-db-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        write(&path, "chat", &vec!["旧消息"]).unwrap();
        write(&path, "records", &vec!["资料"]).unwrap();
        assert_eq!(read::<Vec<String>>(&path, "chat").unwrap().unwrap(), vec!["旧消息"]);
        assert_eq!(read::<Vec<String>>(&path, "records").unwrap().unwrap(), vec!["资料"]);
        assert!(write(&path, "chat", &BadEncoding).is_err());
        assert_eq!(read::<Vec<String>>(&path, "chat").unwrap().unwrap(), vec!["旧消息"]);
        drop(read::<Vec<String>>(&path, "chat"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn record(id: &str, title: &str) -> KnowledgeRecord {
        serde_json::from_value(json!({"id":id,"type":"article","title":title,"site":"example.com",
            "savedAt":"2026-11-11T00:00:00Z","status":"ready","sourceUrl":"https://example.com",
            "contentText":"测试正文","excerpt":[]})).unwrap()
    }

    #[test]
    fn old_record_snapshot_migrates_to_rows_without_losing_backup() {
        let dir = std::env::temp_dir().join(format!("knowflow-record-migration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let original = vec![record("first", "第一条"), record("second", "第二条")];
        write(&path, "records", &original).unwrap();
        let loaded = read_records(&path).unwrap().unwrap();
        assert_eq!(loaded.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["first", "second"]);
        assert_eq!(read::<Vec<KnowledgeRecord>>(&path, "records").unwrap().unwrap().len(), 2);
        let connection = open(&path).unwrap();
        let count: i64 = connection.query_row("SELECT count(*) FROM record_rows", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);
        drop(connection);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn first_phase_four_upgrade_makes_independent_sqlite_backup() {
        let dir = std::env::temp_dir().join(format!("knowflow-pre-phase4-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        write(&path, "records", &vec![record("old", "旧资料")]).unwrap();
        backup_before_phase_four(&path).unwrap();
        let backup = dir.join("knowflow.pre-phase4.db");
        assert!(backup.exists());
        read_records(&path).unwrap();
        write_records(&path, &[record("new", "新资料")]).unwrap();
        assert_eq!(read::<Vec<KnowledgeRecord>>(&backup, "records").unwrap().unwrap()[0].id, "old");
        assert_eq!(read_records(&path).unwrap().unwrap()[0].id, "new");
        backup_before_phase_four(&path).unwrap();
        assert_eq!(read::<Vec<KnowledgeRecord>>(&backup, "records").unwrap().unwrap()[0].id, "old");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn row_updates_are_atomic_and_legacy_snapshot_stays_as_backup() {
        let dir = std::env::temp_dir().join(format!("knowflow-record-rows-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        write(&path, "records", &vec![record("first", "旧标题"), record("second", "第二条")]).unwrap();
        read_records(&path).unwrap();
        write_records(&path, &[record("first", "新标题")]).unwrap();
        let rows = read_records(&path).unwrap().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "新标题");
        assert_eq!(read::<Vec<KnowledgeRecord>>(&path, "records").unwrap().unwrap().len(), 2);
        assert!(write_records(&path, &[record("duplicate", "甲"), record("duplicate", "乙")]).is_err());
        assert_eq!(read_records(&path).unwrap().unwrap()[0].title, "新标题");
        assert_eq!(read::<Vec<KnowledgeRecord>>(&path, "records").unwrap().unwrap()[0].title, "旧标题");
        write(&path, "records", &vec![record("legacy", "旧版后续更新")]).unwrap();
        assert_eq!(read_records(&path).unwrap().unwrap()[0].id, "first");
        let connection = open(&path).unwrap();
        connection.execute("UPDATE entity_meta SET version=1 WHERE name='records'", []).unwrap();
        drop(connection);
        assert_eq!(read_records(&path).unwrap().unwrap()[0].id, "legacy");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn record_page_filters_searches_and_preserves_display_order() {
        let dir = std::env::temp_dir().join(format!("knowflow-record-page-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let mut video = record("video", "Rust 学习");
        video.record_type = "video".into();
        write_records(&path, &[video, record("article", "Rust 文章"), record("other", "无关内容")]).unwrap();
        let first = query_record_page(&path, "all", "RUST", 0, 1).unwrap();
        assert_eq!(first.total, 2);
        assert_eq!(first.items[0].id, "video");
        assert!(serde_json::to_value(&first.items[0]).unwrap().get("contentText").is_none());
        assert_eq!(get_record(&path, "video").unwrap().content_text, "测试正文");
        let second = query_record_page(&path, "all", "rust", 1, 1).unwrap();
        assert_eq!(second.items[0].id, "article");
        let only_article = query_record_page(&path, "article", "", 0, 20).unwrap();
        assert_eq!(only_article.total, 2);
        assert_eq!(only_article.items[0].id, "article");
        assert!(query_record_page(&path, "invalid", "", 0, 20).is_err());
        assert!(query_record_page(&path, "all", "", 0, 0).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn full_text_and_time_index_follow_record_updates_and_backfill() {
        let dir = std::env::temp_dir().join(format!("knowflow-segments-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let mut video = record("video", "视频资料");
        video.record_type = "video".into();
        video.content_text = "开场白\n隐藏命中词\n结尾".into();
        video.transcript = Some(vec![
            crate::storage::TranscriptCue { at_seconds: 10, text: "开场白".into() },
            crate::storage::TranscriptCue { at_seconds: 65, text: "隐藏命中词".into() },
            crate::storage::TranscriptCue { at_seconds: 130, text: "结尾".into() },
        ]);
        let mut article = record("article", "普通文章");
        article.content_text = format!("{}跨界命中词{}", "a".repeat(748), "b".repeat(80));
        let mut legacy_video = record("legacy-video", "旧视频");
        legacy_video.record_type = "video".into();
        legacy_video.content_text = "旧版字幕文本仍可找到".into();
        write_records(&path, &[video.clone(), article, legacy_video]).unwrap();
        assert_eq!(query_record_page(&path, "all", "隐藏命中词", 0, 20).unwrap().total, 1);
        assert_eq!(query_record_page(&path, "article", "跨界命中词", 0, 20).unwrap().total, 1);
        assert_eq!(query_record_page(&path, "video", "旧版字幕文本", 0, 20).unwrap().total, 1);
        let timed = query_record_segments(&path, "video", Some(60), Some(120), "", 20).unwrap();
        assert_eq!(timed.len(), 1);
        assert_eq!(timed[0].at_seconds, Some(65));
        assert_eq!(timed[0].text, "隐藏命中词");
        assert!(query_record_segments(&path, "video", Some(120), Some(60), "", 20).is_err());
        video.transcript.as_mut().unwrap()[1].text = "已经改变".into();
        video.content_text = "开场白\n已经改变\n结尾".into();
        let mut article = get_record(&path, "article").unwrap();
        article.content_text = "修改后正文".into();
        let legacy_video = get_record(&path, "legacy-video").unwrap();
        write_records(&path, &[record("new", "新资料"), video, article, legacy_video]).unwrap();
        assert_eq!(query_record_page(&path, "all", "隐藏命中词", 0, 20).unwrap().total, 0);
        assert_eq!(query_record_page(&path, "all", "已经改变", 0, 20).unwrap().total, 1);
        let connection = open(&path).unwrap();
        connection.execute("DELETE FROM entity_meta WHERE name='segments'", []).unwrap();
        connection.execute("DELETE FROM record_segments", []).unwrap();
        drop(connection);
        assert_eq!(read_records(&path).unwrap().unwrap().len(), 4);
        assert_eq!(query_record_page(&path, "all", "已经改变", 0, 20).unwrap().total, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn single_record_writes_shift_order_and_preserve_other_rows() {
        let dir = std::env::temp_dir().join(format!("knowflow-record-delta-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        write_records(&path, &[record("first", "第一条")]).unwrap();
        upsert_record(&path, &record("new", "新条目"), 0).unwrap();
        assert_eq!(read_records(&path).unwrap().unwrap().iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), vec!["new", "first"]);
        let updated = record("first", "已修改");
        upsert_record(&path, &updated, 1).unwrap();
        assert_eq!(query_record_page(&path, "all", "已修改", 0, 20).unwrap().total, 1);
        assert!(upsert_record(&path, &updated, 0).is_err());
        assert_eq!(read_records(&path).unwrap().unwrap().len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn note(id: &str, title: &str) -> Note {
        serde_json::from_value(json!({"id":id,"title":title,"body":"正文","sources":[],"createdAt":100,"updatedAt":200})).unwrap()
    }

    #[test]
    fn note_rows_migrate_update_and_reconcile_old_snapshot() {
        let dir = std::env::temp_dir().join(format!("knowflow-note-rows-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        write(&path, "notes", &vec![note("old", "旧笔记")]).unwrap();
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].title, "旧笔记");
        let connection = open(&path).unwrap();
        let count: i64 = connection.query_row("SELECT count(*) FROM note_rows", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        drop(connection);
        write_notes(&path, &[note("new", "新笔记")]).unwrap();
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].id, "new");
        assert_eq!(read::<Vec<Note>>(&path, "notes").unwrap().unwrap()[0].id, "old");
        assert!(write_notes(&path, &[note("dup", "甲"), note("dup", "乙")]).is_err());
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].id, "new");
        write(&path, "notes", &vec![note("legacy", "旧版修改")]).unwrap();
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].id, "new");
        let connection = open(&path).unwrap();
        connection.execute("UPDATE entity_meta SET version=1 WHERE name='notes'", []).unwrap();
        drop(connection);
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].id, "legacy");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn single_note_writes_do_not_rewrite_other_notes() {
        let dir = std::env::temp_dir().join(format!("knowflow-note-delta-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        write_notes(&path, &[note("first", "第一篇")]).unwrap();
        upsert_note(&path, &note("second", "第二篇"), 1).unwrap();
        upsert_note(&path, &note("first", "已修改"), 0).unwrap();
        assert_eq!(read_notes(&path).unwrap().unwrap().iter().map(|item| item.title.as_str()).collect::<Vec<_>>(), vec!["已修改", "第二篇"]);
        assert!(upsert_note(&path, &note("first", "错误位置"), 1).is_err());
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].title, "已修改");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn note_folders_classify_legacy_notes_and_keep_notes_on_delete() {
        let dir = std::env::temp_dir().join(format!("knowflow-note-folders-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let old = note("legacy", "旧笔记");
        write_notes(&path, &[old.clone()]).unwrap();
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].folder_id, None);
        let folder = NoteFolder { id: "folder-1".into(), name: "项目资料".into(), created_at: "300".into() };
        create_note_folder(&path, &folder).unwrap();
        assert!(create_note_folder(&path, &NoteFolder { id: "folder-2".into(), name: "项目资料".into(), created_at: "301".into() }).is_err());
        let mut filed = old.clone();
        filed.folder_id = Some(folder.id.clone());
        filed.updated_at = 301;
        upsert_note(&path, &filed, 0).unwrap();
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].folder_id.as_deref(), Some("folder-1"));
        rename_note_folder(&path, &folder.id, "项目笔记").unwrap();
        assert_eq!(list_note_folders(&path).unwrap()[0].name, "项目笔记");
        assert!(delete_note_folder(&path, &folder.id, &[old], 302).is_err());
        assert!(note_folder_exists(&path, &folder.id).unwrap());
        let unfiled = delete_note_folder(&path, &folder.id, &[filed], 302).unwrap();
        assert_eq!(unfiled[0].folder_id, None);
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].title, "旧笔记");
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].folder_id, None);
        assert!(list_note_folders(&path).unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn chat_store() -> ChatStore {
        serde_json::from_value(json!({"activeId":"chat-a","projectMemory":[],"sessions":[
            {"id":"chat-a","title":"甲","createdAt":"1","updatedAt":"2","referencedIds":[],"messages":[
                {"id":"msg-1","role":"user","createdAt":"1","paragraphs":["问题甲"]}]},
            {"id":"chat-b","title":"乙","createdAt":"1","updatedAt":"3","referencedIds":[],"messages":[
                {"id":"msg-1","role":"user","createdAt":"2","paragraphs":["问题乙"]}]}]})).unwrap()
    }

    #[test]
    fn chat_sessions_and_messages_migrate_without_cross_session_leakage() {
        let dir = std::env::temp_dir().join(format!("knowflow-chat-rows-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let original = chat_store();
        write(&path, "chat", &original).unwrap();
        let loaded = read_chat(&path).unwrap().unwrap();
        assert_eq!(loaded.sessions.len(), 2);
        assert_eq!(loaded.sessions[0].messages[0].paragraphs[0], "问题甲");
        assert_eq!(loaded.sessions[1].messages[0].paragraphs[0], "问题乙");
        let connection = open(&path).unwrap();
        let count: i64 = connection.query_row("SELECT count(*) FROM message_rows", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);
        drop(connection);
        let mut next = loaded;
        next.sessions.remove(0);
        next.active_id = "chat-b".into();
        write_chat(&path, &next).unwrap();
        assert_eq!(read_chat(&path).unwrap().unwrap().sessions.len(), 1);
        let connection = open(&path).unwrap();
        let count: i64 = connection.query_row("SELECT count(*) FROM message_rows", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        drop(connection);
        write(&path, "chat", &original).unwrap();
        assert_eq!(read_chat(&path).unwrap().unwrap().sessions.len(), 1);
        let connection = open(&path).unwrap();
        connection.execute("UPDATE entity_meta SET version=1 WHERE name='chat'", []).unwrap();
        drop(connection);
        assert_eq!(read_chat(&path).unwrap().unwrap().sessions.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn combined_migration_keeps_record_note_and_chat_links() {
        let dir = std::env::temp_dir().join(format!("knowflow-linked-migration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let source = record("source", "来源资料");
        let mut linked_note = note("linked", "关联笔记");
        linked_note.sources.push(crate::notes::NoteSource {
            record_id: source.id.clone(), title: source.title.clone(), url: source.source_url.clone(), at_seconds: None,
        });
        let mut linked_chat = chat_store();
        linked_chat.sessions[0].referenced_ids.push(source.id.clone());
        linked_chat.sessions[0].messages[0].citations.push(crate::agent::ChatCitation {
            record_id: source.id.clone(), at_seconds: None, source_index: Some(1), label: None, kind: None,
        });
        write(&path, "records", &vec![source]).unwrap();
        write(&path, "notes", &vec![linked_note]).unwrap();
        write(&path, "chat", &linked_chat).unwrap();
        backup_before_phase_four(&path).unwrap();
        let records = read_records(&path).unwrap().unwrap();
        let notes = read_notes(&path).unwrap().unwrap();
        let chat = read_chat(&path).unwrap().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(notes.len(), 1);
        assert_eq!(chat.sessions.len(), 2);
        assert_eq!(notes[0].sources[0].record_id, records[0].id);
        assert_eq!(chat.sessions[0].referenced_ids[0], records[0].id);
        assert_eq!(chat.sessions[0].messages[0].citations[0].record_id, records[0].id);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
