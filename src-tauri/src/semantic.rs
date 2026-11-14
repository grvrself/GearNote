//! Optional embedding retrieval for the current, explicitly selected ReadIndex.
//! Any provider, cache, or response error returns None so lexical search remains usable.
use crate::{context::SourceChunk, db};
use reqwest::Client;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{path::Path, time::{Duration, Instant}};

const MAX_DIM: usize = 4_096;
const BATCH_ITEMS: usize = 32;
const BATCH_CHARS: usize = 20_000;

/// Reciprocal rank fusion (k=60). Returns None for malformed vector results.
pub fn fuse_scores(lexical: &[usize], semantic: &[f32]) -> Option<Vec<usize>> {
    if lexical.len() != semantic.len() || semantic.iter().any(|value| !value.is_finite()) { return None; }
    let mut words = (0..lexical.len()).filter(|&index| lexical[index] > 0).collect::<Vec<_>>();
    words.sort_by(|&a, &b| lexical[b].cmp(&lexical[a]).then_with(|| a.cmp(&b)));
    let mut vectors = (0..semantic.len()).filter(|&index| semantic[index] > 0.0).collect::<Vec<_>>();
    vectors.sort_by(|&a, &b| semantic[b].total_cmp(&semantic[a]).then_with(|| a.cmp(&b)));
    let mut fused = vec![0.0_f64; lexical.len()];
    for (rank, index) in words.into_iter().enumerate() { fused[index] += 1.0 / (61 + rank) as f64; }
    for (rank, index) in vectors.into_iter().take(24).enumerate() { fused[index] += 1.0 / (61 + rank) as f64; }
    Some(fused.into_iter().map(|score| (score * 1_000_000.0).round() as usize).collect())
}

fn digest(text: &str) -> String {
    let hash = text.as_bytes().iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    format!("{hash:016x}:{}", text.len())
}

fn normalize_vector(value: &Value) -> Option<Vec<f32>> {
    let values = value.as_array()?;
    if values.is_empty() || values.len() > MAX_DIM { return None; }
    let vector = values.iter().map(|item| item.as_f64().map(|value| value as f32))
        .collect::<Option<Vec<_>>>()?;
    let norm = vector.iter().map(|value| f64::from(*value).powi(2)).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= f64::EPSILON { return None; }
    let vector = vector.into_iter().map(|value| (f64::from(value) / norm) as f32).collect::<Vec<_>>();
    vector.iter().all(|value| value.is_finite()).then_some(vector)
}

fn parse_embeddings(body: &Value, count: usize) -> Option<Vec<Vec<f32>>> {
    let data = body.get("data")?.as_array()?;
    if data.len() != count { return None; }
    let mut result = vec![None; count];
    for item in data {
        let index = usize::try_from(item.get("index")?.as_u64()?).ok()?;
        if index >= count || result[index].is_some() { return None; }
        result[index] = Some(normalize_vector(item.get("embedding")?)?);
    }
    let result = result.into_iter().collect::<Option<Vec<_>>>()?;
    (result.iter().all(|vector| vector.len() == result[0].len())).then_some(result)
}

async fn embed(client: &Client, base_url: &str, model: &str, key: &str, input: &[String]) -> Option<Vec<Vec<f32>>> {
    let request = client.post(format!("{base_url}/embeddings")).timeout(Duration::from_secs(20))
        .json(&json!({"model":model,"input":input}));
    let request = if key.is_empty() { request } else { request.bearer_auth(key) };
    let mut response = request.send().await.ok()?;
    if !response.status().is_success() { return None; }
    if response.content_length().is_some_and(|size| size > 8_000_000) { return None; }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 8_000_000 { return None; }
        bytes.extend_from_slice(&chunk);
    }
    parse_embeddings(&serde_json::from_slice(&bytes).ok()?, input.len())
}

fn decode_vector(bytes: &[u8], dim: usize) -> Option<Vec<f32>> {
    if dim == 0 || dim > MAX_DIM || bytes.len() != dim * 4 { return None; }
    let vector = bytes.chunks_exact(4).map(|chunk| Some(f32::from_le_bytes(chunk.try_into().ok()?)))
        .collect::<Option<Vec<_>>>()?;
    vector.iter().all(|value| value.is_finite()).then_some(vector)
}

fn load_cached(path: &Path, provider: &str, model: &str, chunks: &[(String, SourceChunk)]) -> Option<Vec<Option<Vec<f32>>>> {
    let connection = db::open(path).ok()?;
    let mut query = connection.prepare("SELECT digest,dim,vec FROM chunk_vectors WHERE chunk_id=?1 AND provider=?2 AND model=?3").ok()?;
    chunks.iter().map(|(id, chunk)| {
        let cached: Option<(String, i64, Vec<u8>)> = query.query_row(params![id, provider, model], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .optional().ok()?;
        Some(cached.and_then(|(saved_digest, dim, bytes)| {
            (saved_digest == digest(&chunk.search_text)).then(|| decode_vector(&bytes, usize::try_from(dim).ok()?)).flatten()
        }))
    }).collect()
}

fn store_cached(path: &Path, provider: &str, model: &str, chunks: &[(String, SourceChunk)], vectors: &[Vec<f32>], indexes: &[usize]) -> Option<()> {
    let mut connection = db::open(path).ok()?;
    let transaction = connection.transaction().ok()?;
    for (&index, vector) in indexes.iter().zip(vectors) {
        let (id, chunk) = &chunks[index];
        let bytes = vector.iter().flat_map(|value| value.to_le_bytes()).collect::<Vec<_>>();
        transaction.execute("INSERT INTO chunk_vectors(chunk_id,record_id,provider,model,digest,dim,vec)
            VALUES (?1,?2,?3,?4,?5,?6,?7)
            ON CONFLICT(chunk_id,provider,model) DO UPDATE SET record_id=excluded.record_id,
                digest=excluded.digest,dim=excluded.dim,vec=excluded.vec",
            params![id, chunk.record_id, provider, model, digest(&chunk.search_text), i64::try_from(vector.len()).ok()?, bytes]).ok()?;
    }
    transaction.commit().ok()
}

/// Returns cosine similarities in ReadIndex order. No unselected record is opened or embedded.
pub async fn similarities(client: &Client, base_url: &str, model: &str, key: &str,
    path: &Path, chunks: &[(String, SourceChunk)], question: &str) -> Option<Vec<f32>> {
    if chunks.is_empty() || chunks.len() > 2_000 || model.is_empty() { return None; }
    let started = Instant::now();
    let query = embed(client, base_url, model, key, &[question.to_string()]).await?.pop()?;
    let mut vectors = load_cached(path, base_url, model, chunks)?;
    for vector in &mut vectors {
        if vector.as_ref().is_some_and(|value| value.len() != query.len()) { *vector = None; }
    }
    let missing = vectors.iter().enumerate().filter_map(|(index, value)| value.is_none().then_some(index)).collect::<Vec<_>>();
    let mut start = 0;
    while start < missing.len() {
        if started.elapsed() > Duration::from_secs(90) { return None; }
        let mut end = start;
        let mut chars = 0;
        while end < missing.len() && end - start < BATCH_ITEMS {
            let next = chunks[missing[end]].1.search_text.chars().count().min(1_200);
            if end > start && chars + next > BATCH_CHARS { break; }
            chars += next;
            end += 1;
        }
        let indexes = &missing[start..end];
        let input = indexes.iter().map(|&index| chunks[index].1.search_text.chars().take(1_200).collect::<String>()).collect::<Vec<_>>();
        let batch = embed(client, base_url, model, key, &input).await?;
        if batch.iter().any(|vector| vector.len() != query.len()) { return None; }
        store_cached(path, base_url, model, chunks, &batch, indexes)?;
        for (&index, vector) in indexes.iter().zip(batch) { vectors[index] = Some(vector); }
        start = end;
    }
    vectors.into_iter().map(|vector| {
        let vector = vector?;
        if vector.len() != query.len() { return None; }
        Some(vector.iter().zip(&query).map(|(a, b)| a * b).sum::<f32>())
    }).collect()
}

pub struct ProbeMetrics { pub dimensions: usize, pub cases: usize, pub top_one: usize, pub recall_three: usize }

/// Checks protocol and ranking on fictional paraphrases; never touches the user's library.
pub async fn probe(client: &Client, base_url: &str, model: &str, key: &str) -> Option<ProbeMetrics> {
    let questions = [
        "网络断开后还能继续写草稿吗？",
        "How can I recover a note I accidentally removed?",
        "导出太慢通常是什么原因？",
        "Can teammates edit the same document together?",
    ];
    let passages = [
        "应用在离线状态下仍允许修改草稿。", "连接异常时可查看网络诊断日志。",
        "已移除的笔记会在回收站保留七天。", "新建笔记可以指定文件夹。",
        "服务端限流会降低文件传输速度。", "导出的文件默认采用 Markdown 格式。",
        "团队成员可以共同修改同一份文档。", "个人文档可以导出到本机目录。",
    ];
    let input = questions.iter().chain(passages.iter()).map(|text| (*text).to_string()).collect::<Vec<_>>();
    let vectors = embed(client, base_url, model, key, &input).await?;
    let dim = vectors.first()?.len();
    let mut top_one = 0;
    let mut recall_three = 0;
    for index in 0..questions.len() {
        let mut ranked = (0..passages.len()).map(|candidate| {
            (candidate, vectors[index].iter().zip(&vectors[questions.len() + candidate]).map(|(a, b)| a * b).sum::<f32>())
        }).collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let position = ranked.iter().position(|(candidate, _)| *candidate == index * 2)?;
        top_one += usize::from(position == 0);
        recall_three += usize::from(position < 3);
    }
    Some(ProbeMetrics { dimensions: dim, cases: questions.len(), top_one, recall_three })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{read_tools::ReadIndex, storage::KnowledgeRecord};

    #[test]
    fn validates_provider_vectors_and_cache_bytes() {
        let parsed = parse_embeddings(&json!({"data":[
            {"index":1,"embedding":[0.0,2.0]}, {"index":0,"embedding":[3.0,0.0]}
        ]}), 2).unwrap();
        assert_eq!(parsed[0], vec![1.0, 0.0]);
        assert_eq!(parsed[1], vec![0.0, 1.0]);
        assert!(parse_embeddings(&json!({"data":[{"index":0,"embedding":[0,0]}]}), 1).is_none());
        assert!(parse_embeddings(&json!({"data":[{"index":0,"embedding":[1]},{"index":0,"embedding":[1]}]}), 2).is_none());
        assert!(decode_vector(&[0, 0, 0], 1).is_none());
    }

    #[test]
    fn fusion_retains_exact_hits_and_adds_semantic_candidates() {
        let result = fuse_scores(&[100, 0, 20], &[0.1, 0.9, 0.3]).unwrap();
        assert!(result[0] > 0 && result[1] > 0 && result[2] > 0);
        assert!(result[0] > result[1]);
        assert!(fuse_scores(&[1], &[]).is_none());
    }

    #[test]
    fn embedding_requests_cache_only_selected_chunks_and_invalidate_changed_text() {
        use std::io::{BufRead, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && std::time::Instant::now() < deadline =>
                            std::thread::sleep(std::time::Duration::from_millis(10)),
                        Err(error) => panic!("mock embedding service did not receive request: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut length = 0;
                loop {
                    line.clear(); reader.read_line(&mut line).unwrap();
                    if line == "\r\n" { break; }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length]; reader.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                let input = request["input"].as_array().unwrap();
                assert!(input.iter().all(|value| !value.as_str().unwrap().contains("未选资料")));
                let data = input.iter().enumerate().map(|(index, value)| {
                    let text = value.as_str().unwrap();
                    json!({"index":index,"embedding":if text.contains("断线") || text.contains("离线") { vec![1.0,0.0] } else { vec![0.0,1.0] }})
                }).collect::<Vec<_>>();
                let response = json!({"data":data}).to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            }
        });
        let record = |id: &str, text: &str| -> KnowledgeRecord { serde_json::from_value(json!({
            "id":id,"type":"article","title":"资料","site":"example.invalid","savedAt":"1",
            "status":"ready","sourceUrl":"https://example.invalid","contentText":text,"excerpt":[]
        })).unwrap() };
        let index = ReadIndex::new(&[
            record("selected", "应用在离线状态下允许修改草稿。"),
            record("unselected", "未选资料中的秘密内容。"),
        ], &["selected".into()]);
        let dir = std::env::temp_dir().join(format!("gearnote-embedding-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(db::FILE_NAME);
        let provider = format!("http://127.0.0.1:{port}/v1");
        let scores = tauri::async_runtime::block_on(similarities(&Client::new(), &provider, "mock", "test",
            &path, index.entries(), "断线后能继续编辑吗")).unwrap();
        server.join().unwrap();
        assert_eq!(scores.len(), 1);
        assert!(scores[0] > 0.99);
        let connection = db::open(&path).unwrap();
        let count: i64 = connection.query_row("SELECT count(*) FROM chunk_vectors", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        drop(connection);
        assert!(load_cached(&path, &provider, "mock", index.entries()).unwrap()[0].is_some());
        assert!(load_cached(&path, &provider, "other-model", index.entries()).unwrap()[0].is_none());
        assert!(load_cached(&path, "https://other.invalid/v1", "mock", index.entries()).unwrap()[0].is_none());
        let mut changed = index.entries().to_vec();
        changed[0].1.search_text.push_str("修订");
        assert!(load_cached(&path, &provider, "mock", &changed).unwrap()[0].is_none());
        let connection = db::open(&path).unwrap();
        connection.execute("INSERT INTO record_rows(id,ordinal,record_type,title,site,status,source_url,saved_at,json)
            VALUES ('selected',0,'article','资料','example.invalid','ready','https://example.invalid','1','{}')", []).unwrap();
        connection.execute("UPDATE record_rows SET json='{\"version\":1}' WHERE id='selected'", []).unwrap();
        let count: i64 = connection.query_row("SELECT count(*) FROM chunk_vectors", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        let vector = vec![vec![1.0, 0.0]];
        store_cached(&path, &provider, "mock", index.entries(), &vector, &[0]).unwrap();
        connection.execute("DELETE FROM record_rows WHERE id='selected'", []).unwrap();
        let count: i64 = connection.query_row("SELECT count(*) FROM chunk_vectors", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        drop(connection);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unsupported_embedding_endpoint_returns_lexical_fallback() {
        use std::io::Write;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            write!(stream, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let record: KnowledgeRecord = serde_json::from_value(json!({
            "id":"selected","type":"article","title":"资料","site":"example.invalid","savedAt":"1",
            "status":"ready","sourceUrl":"https://example.invalid","contentText":"离线备份步骤。","excerpt":[]
        })).unwrap();
        let index = ReadIndex::new(&[record], &["selected".into()]);
        let path = std::env::temp_dir().join(format!("gearnote-fallback-{}.db", uuid::Uuid::new_v4()));
        let result = tauri::async_runtime::block_on(similarities(&Client::new(), &format!("http://127.0.0.1:{port}/v1"),
            "unsupported", "test", &path, index.entries(), "备份步骤"));
        server.join().unwrap();
        assert!(result.is_none());
        assert_eq!(index.search_with_semantic("备份步骤", result.as_deref())["matches"][0]["chunk_id"], "selected:0");
        assert!(!path.exists());
    }
}
