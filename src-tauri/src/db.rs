use crate::{agent::{ChatMessage, ChatSession, ChatStore}, notes::Note, storage::KnowledgeRecord};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde::de::DeserializeOwned;
use std::{collections::HashSet, path::Path};

pub const FILE_NAME: &str = "knowflow.db";

fn open(path: &Path) -> Result<Connection, String> {
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
        CREATE TABLE IF NOT EXISTS note_rows (
            id TEXT PRIMARY KEY, ordinal INTEGER NOT NULL, title TEXT NOT NULL, updated_at INTEGER NOT NULL, json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_note_rows_updated ON note_rows(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_note_rows_ordinal ON note_rows(ordinal);
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
        CREATE INDEX IF NOT EXISTS idx_message_rows_order ON message_rows(session_id,ordinal);")
        .map_err(|error| format!("数据库初始化失败：{error}"))?;
    Ok(connection)
}

fn sync_record_rows(transaction: &Transaction<'_>, records: &[KnowledgeRecord], snapshot: &str) -> Result<(), String> {
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='records'", [], |row| row.get(0))
        .optional().map_err(|error| format!("资料迁移状态读取失败：{error}"))?;
    if version.is_some_and(|value| value != 1) { return Err("资料数据库版本高于当前程序，无法写入".into()); }
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
        stale.remove(&record.id);
    }
    for id in stale {
        transaction.execute("DELETE FROM record_rows WHERE id=?1", [id]).map_err(|error| format!("旧资料清理失败：{error}"))?;
    }
    transaction.execute("INSERT INTO snapshots(name,json) VALUES ('records',?1) ON CONFLICT(name) DO UPDATE SET json=excluded.json", [snapshot])
        .map_err(|error| format!("资料快照写入失败：{error}"))?;
    transaction.execute("INSERT INTO entity_meta(name,version) VALUES ('records',1) ON CONFLICT(name) DO UPDATE SET version=excluded.version", [])
        .map_err(|error| format!("资料迁移状态写入失败：{error}"))?;
    Ok(())
}

pub fn read_records(path: &Path) -> Result<Option<Vec<KnowledgeRecord>>, String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("资料读取事务启动失败：{error}"))?;
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='records'", [], |row| row.get(0))
        .optional().map_err(|error| format!("资料迁移状态读取失败：{error}"))?;
    match version {
        Some(1) => {
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
            let snapshot: Option<String> = transaction.query_row("SELECT json FROM snapshots WHERE name='records'", [], |row| row.get(0))
                .optional().map_err(|error| format!("资料快照读取失败：{error}"))?;
            if let Some(snapshot) = snapshot {
                let current = rows.as_ref().ok().and_then(|records| serde_json::to_string(records).ok());
                if current.as_deref() != Some(snapshot.as_str()) {
                    let recovered: Vec<KnowledgeRecord> = serde_json::from_str(&snapshot).map_err(|error| format!("资料快照与逐条记录不一致，且快照无效：{error}"))?;
                    let canonical = serde_json::to_string(&recovered).map_err(|error| format!("资料快照编码失败：{error}"))?;
                    sync_record_rows(&transaction, &recovered, &canonical)?;
                    transaction.commit().map_err(|error| format!("资料同步提交失败：{error}"))?;
                    return Ok(Some(recovered));
                }
            }
            transaction.commit().map_err(|error| format!("资料读取事务结束失败：{error}"))?;
            rows.map(Some)
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
            let canonical = serde_json::to_string(&records).map_err(|error| format!("旧资料快照编码失败：{error}"))?;
            sync_record_rows(&transaction, &records, &canonical)?;
            transaction.commit().map_err(|error| format!("资料迁移提交失败：{error}"))?;
            Ok(Some(records))
        }
    }
}

pub fn write_records(path: &Path, records: &[KnowledgeRecord]) -> Result<(), String> {
    let snapshot = serde_json::to_string(records).map_err(|error| format!("资料快照编码失败：{error}"))?;
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("资料事务启动失败：{error}"))?;
    sync_record_rows(&transaction, records, &snapshot)?;
    transaction.commit().map_err(|error| format!("资料事务提交失败：{error}"))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordPage {
    pub items: Vec<KnowledgeRecord>,
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
    let predicate = "(?1 = 'all' OR record_type = ?1) AND (instr(lower(title), lower(?2)) > 0 OR instr(lower(site), lower(?2)) > 0)";
    let total: i64 = transaction.query_row(&format!("SELECT count(*) FROM record_rows WHERE {predicate}"), params![record_type, search], |row| row.get(0))
        .map_err(|error| format!("资料计数失败：{error}"))?;
    let mut items = Vec::new();
    {
        let mut query = transaction.prepare(&format!("SELECT json FROM record_rows WHERE {predicate} ORDER BY ordinal ASC LIMIT ?3 OFFSET ?4"))
            .map_err(|error| format!("资料分页查询失败：{error}"))?;
        let rows = query.query_map(params![record_type, search, limit, offset], |row| row.get::<_, String>(0))
            .map_err(|error| format!("资料分页查询失败：{error}"))?;
        for row in rows {
            let json = row.map_err(|error| format!("资料分页查询失败：{error}"))?;
            items.push(serde_json::from_str(&json).map_err(|error| format!("资料记录无效：{error}"))?);
        }
    }
    transaction.commit().map_err(|error| format!("资料查询事务结束失败：{error}"))?;
    Ok(RecordPage { items, total: total as u64, offset: offset as u64, limit: limit as u64 })
}

fn sync_note_rows(transaction: &Transaction<'_>, notes: &[Note], snapshot: &str) -> Result<(), String> {
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='notes'", [], |row| row.get(0))
        .optional().map_err(|error| format!("笔记迁移状态读取失败：{error}"))?;
    if version.is_some_and(|value| value != 1) { return Err("笔记数据库版本高于当前程序，无法写入".into()); }
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
    transaction.execute("INSERT INTO snapshots(name,json) VALUES ('notes',?1) ON CONFLICT(name) DO UPDATE SET json=excluded.json", [snapshot])
        .map_err(|error| format!("笔记快照写入失败：{error}"))?;
    transaction.execute("INSERT INTO entity_meta(name,version) VALUES ('notes',1) ON CONFLICT(name) DO UPDATE SET version=excluded.version", [])
        .map_err(|error| format!("笔记迁移状态写入失败：{error}"))?;
    Ok(())
}

pub fn read_notes(path: &Path) -> Result<Option<Vec<Note>>, String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("笔记读取事务启动失败：{error}"))?;
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='notes'", [], |row| row.get(0))
        .optional().map_err(|error| format!("笔记迁移状态读取失败：{error}"))?;
    let snapshot: Option<String> = transaction.query_row("SELECT json FROM snapshots WHERE name='notes'", [], |row| row.get(0))
        .optional().map_err(|error| format!("笔记快照读取失败：{error}"))?;
    match version {
        Some(1) => {
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
            if let Some(snapshot) = snapshot {
                let current = rows.as_ref().ok().and_then(|notes| serde_json::to_string(notes).ok());
                if current.as_deref() != Some(snapshot.as_str()) {
                    let recovered: Vec<Note> = serde_json::from_str(&snapshot).map_err(|error| format!("笔记快照与逐条记录不一致，且快照无效：{error}"))?;
                    let canonical = serde_json::to_string(&recovered).map_err(|error| format!("笔记快照编码失败：{error}"))?;
                    sync_note_rows(&transaction, &recovered, &canonical)?;
                    transaction.commit().map_err(|error| format!("笔记同步提交失败：{error}"))?;
                    return Ok(Some(recovered));
                }
            }
            transaction.commit().map_err(|error| format!("笔记读取事务结束失败：{error}"))?;
            rows.map(Some)
        }
        Some(_) => Err("笔记数据库版本高于当前程序，无法读取".into()),
        None => {
            let Some(snapshot) = snapshot else {
                transaction.commit().map_err(|error| format!("笔记读取事务结束失败：{error}"))?;
                return Ok(None);
            };
            let notes: Vec<Note> = serde_json::from_str(&snapshot).map_err(|error| format!("旧笔记快照无效：{error}"))?;
            let canonical = serde_json::to_string(&notes).map_err(|error| format!("笔记快照编码失败：{error}"))?;
            sync_note_rows(&transaction, &notes, &canonical)?;
            transaction.commit().map_err(|error| format!("笔记迁移提交失败：{error}"))?;
            Ok(Some(notes))
        }
    }
}

pub fn write_notes(path: &Path, notes: &[Note]) -> Result<(), String> {
    let snapshot = serde_json::to_string(notes).map_err(|error| format!("笔记快照编码失败：{error}"))?;
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("笔记事务启动失败：{error}"))?;
    sync_note_rows(&transaction, notes, &snapshot)?;
    transaction.commit().map_err(|error| format!("笔记事务提交失败：{error}"))
}

fn sync_chat_rows(transaction: &Transaction<'_>, store: &ChatStore, snapshot: &str) -> Result<(), String> {
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='chat'", [], |row| row.get(0))
        .optional().map_err(|error| format!("会话迁移状态读取失败：{error}"))?;
    if version.is_some_and(|value| value != 1) { return Err("会话数据库版本高于当前程序，无法写入".into()); }
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
    transaction.execute("INSERT INTO snapshots(name,json) VALUES ('chat',?1) ON CONFLICT(name) DO UPDATE SET json=excluded.json", [snapshot])
        .map_err(|error| format!("会话快照写入失败：{error}"))?;
    transaction.execute("INSERT INTO entity_meta(name,version) VALUES ('chat',1) ON CONFLICT(name) DO UPDATE SET version=excluded.version", [])
        .map_err(|error| format!("会话迁移状态写入失败：{error}"))?;
    Ok(())
}

pub fn read_chat(path: &Path) -> Result<Option<ChatStore>, String> {
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("会话读取事务启动失败：{error}"))?;
    let version: Option<i64> = transaction.query_row("SELECT version FROM entity_meta WHERE name='chat'", [], |row| row.get(0))
        .optional().map_err(|error| format!("会话迁移状态读取失败：{error}"))?;
    let snapshot: Option<String> = transaction.query_row("SELECT json FROM snapshots WHERE name='chat'", [], |row| row.get(0))
        .optional().map_err(|error| format!("会话快照读取失败：{error}"))?;
    match version {
        Some(1) => {
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
            if let Some(snapshot) = snapshot {
                let current = rows.as_ref().ok().and_then(|store| serde_json::to_string(store).ok());
                if current.as_deref() != Some(snapshot.as_str()) {
                    let recovered: ChatStore = serde_json::from_str(&snapshot).map_err(|error| format!("会话快照与逐条记录不一致，且快照无效：{error}"))?;
                    let canonical = serde_json::to_string(&recovered).map_err(|error| format!("会话快照编码失败：{error}"))?;
                    sync_chat_rows(&transaction, &recovered, &canonical)?;
                    transaction.commit().map_err(|error| format!("会话同步提交失败：{error}"))?;
                    return Ok(Some(recovered));
                }
            }
            transaction.commit().map_err(|error| format!("会话读取事务结束失败：{error}"))?;
            rows.map(Some)
        }
        Some(_) => Err("会话数据库版本高于当前程序，无法读取".into()),
        None => {
            let Some(snapshot) = snapshot else {
                transaction.commit().map_err(|error| format!("会话读取事务结束失败：{error}"))?;
                return Ok(None);
            };
            let store: ChatStore = serde_json::from_str(&snapshot).map_err(|error| format!("旧会话快照无效：{error}"))?;
            let canonical = serde_json::to_string(&store).map_err(|error| format!("会话快照编码失败：{error}"))?;
            sync_chat_rows(&transaction, &store, &canonical)?;
            transaction.commit().map_err(|error| format!("会话迁移提交失败：{error}"))?;
            Ok(Some(store))
        }
    }
}

pub fn write_chat(path: &Path, store: &ChatStore) -> Result<(), String> {
    let snapshot = serde_json::to_string(store).map_err(|error| format!("会话快照编码失败：{error}"))?;
    let mut connection = open(path)?;
    let transaction = connection.transaction().map_err(|error| format!("会话事务启动失败：{error}"))?;
    sync_chat_rows(&transaction, store, &snapshot)?;
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
    fn row_updates_and_snapshot_are_atomic() {
        let dir = std::env::temp_dir().join(format!("knowflow-record-rows-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        write_records(&path, &[record("first", "旧标题"), record("second", "第二条")]).unwrap();
        write_records(&path, &[record("first", "新标题")]).unwrap();
        let rows = read_records(&path).unwrap().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "新标题");
        assert_eq!(read::<Vec<KnowledgeRecord>>(&path, "records").unwrap().unwrap().len(), 1);
        assert!(write_records(&path, &[record("duplicate", "甲"), record("duplicate", "乙")]).is_err());
        assert_eq!(read_records(&path).unwrap().unwrap()[0].title, "新标题");
        assert_eq!(read::<Vec<KnowledgeRecord>>(&path, "records").unwrap().unwrap()[0].title, "新标题");
        write(&path, "records", &vec![record("legacy", "旧版后续更新")]).unwrap();
        assert_eq!(read_records(&path).unwrap().unwrap()[0].id, "legacy");
        assert_eq!(read_records(&path).unwrap().unwrap()[0].title, "旧版后续更新");
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
        let second = query_record_page(&path, "all", "rust", 1, 1).unwrap();
        assert_eq!(second.items[0].id, "article");
        let only_article = query_record_page(&path, "article", "", 0, 20).unwrap();
        assert_eq!(only_article.total, 2);
        assert_eq!(only_article.items[0].id, "article");
        assert!(query_record_page(&path, "invalid", "", 0, 20).is_err());
        assert!(query_record_page(&path, "all", "", 0, 0).is_err());
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
        assert_eq!(read::<Vec<Note>>(&path, "notes").unwrap().unwrap()[0].id, "new");
        assert!(write_notes(&path, &[note("dup", "甲"), note("dup", "乙")]).is_err());
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].id, "new");
        write(&path, "notes", &vec![note("legacy", "旧版修改")]).unwrap();
        assert_eq!(read_notes(&path).unwrap().unwrap()[0].id, "legacy");
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
        assert_eq!(read_chat(&path).unwrap().unwrap().sessions.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
