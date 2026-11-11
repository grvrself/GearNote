use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

pub const FILE_NAME: &str = "knowflow.db";

fn open(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|error| format!("数据库打开失败：{error}"))?;
    connection.busy_timeout(std::time::Duration::from_secs(5)).map_err(|error| error.to_string())?;
    connection.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS snapshots (name TEXT PRIMARY KEY, json TEXT NOT NULL);")
        .map_err(|error| format!("数据库初始化失败：{error}"))?;
    Ok(connection)
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
}
