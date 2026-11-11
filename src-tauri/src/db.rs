use crate::storage::KnowledgeRecord;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{de::DeserializeOwned, Serialize};
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
        CREATE INDEX IF NOT EXISTS idx_record_rows_ordinal ON record_rows(ordinal);")
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

pub fn read<T: DeserializeOwned>(path: &Path, name: &str) -> Result<Option<T>, String> {
    let connection = open(path)?;
    let json: Option<String> = connection.query_row("SELECT json FROM snapshots WHERE name = ?1", [name], |row| row.get(0))
        .optional().map_err(|error| format!("数据库读取失败：{error}"))?;
    json.map(|value| serde_json::from_str(&value).map_err(|error| format!("数据库数据无效：{error}"))).transpose()
}

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
}
