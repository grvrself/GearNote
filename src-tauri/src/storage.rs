use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeRecord {
    pub id: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub title: String,
    pub site: String,
    pub saved_at: String,
    pub status: crate::ProcessStatus,
    pub source_url: String,
    pub content_text: String,
    pub excerpt: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<Vec<TranscriptCue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moments: Option<Vec<KeyMoment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_language: Option<String>,
    #[serde(default)]
    pub caption_source_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storyboard: Option<StoryboardResult>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryboardResult {
    pub image_count: usize,
    pub cells: Vec<StoryboardCell>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryboardCell {
    pub cell: u32,
    pub sheet: u32,
    pub from: f64,
    pub to: f64,
    #[serde(rename(serialize = "frameAt", deserialize = "frame_at"))]
    pub frame_at: Option<f64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCue {
    pub at_seconds: u32,
    pub text: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyMoment {
    pub at_seconds: u32,
    pub title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleCapture {
    pub url: String,
    pub title: String,
    pub text: String,
    pub captured_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoCapture {
    pub url: String,
    pub title: String,
    pub captured_at: String,
    pub duration_seconds: Option<u32>,
    pub language: String,
    pub cues: Vec<TranscriptCue>,
    pub fault_reason: String,
    pub bvid: Option<String>,
    pub cid: Option<u64>,
    pub track_url: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureResult {
    pub record: KnowledgeRecord,
    pub already_saved: bool,
    pub updated: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureJob {
    pub id: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub title: String,
    pub site: String,
    pub status: crate::ProcessStatus,
    pub fault_reason: Option<String>,
    pub started_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureJobStart {
    pub url: String,
    pub title: String,
    #[serde(rename = "type")]
    pub record_type: String,
}

struct CaptureJobEntry {
    job: CaptureJob,
    source_url: String,
    started: Instant,
}

pub struct AppState {
    pub records: Mutex<Vec<KnowledgeRecord>>,
    capture_jobs: Mutex<Vec<CaptureJobEntry>>,
    pub records_path: PathBuf,
    pub pair_code: String,
    pub last_extension_seen: Mutex<Option<Instant>>,
    pub bridge_error: Mutex<Option<String>>,
}

impl AppState {
    pub fn load(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = app.path().app_data_dir()?;
        fs::create_dir_all(&data_dir)?;
        let records_path = data_dir.join("records.json");
        let backup_path = data_dir.join("records.json.bak");
        if !records_path.exists() {
            let legacy = data_dir.join("articles.json");
            let legacy_backup = data_dir.join("articles.json.bak");
            if legacy.exists() {
                fs::rename(legacy, &records_path)?;
            } else if legacy_backup.exists() {
                fs::rename(legacy_backup, &records_path)?;
            }
        }
        if !records_path.exists() && backup_path.exists() {
            fs::rename(&backup_path, &records_path)?;
        }
        let db_path = data_dir.join(crate::db::FILE_NAME);
        crate::db::backup_before_phase_four(&db_path).map_err(std::io::Error::other)?;
        let mut records: Vec<KnowledgeRecord> = if let Some(saved) = crate::db::read_records(&db_path).map_err(std::io::Error::other)? {
            saved
        } else {
            let legacy = if records_path.exists() { serde_json::from_slice(&fs::read(&records_path)?)? } else { Vec::new() };
            crate::db::write_records(&db_path, &legacy).map_err(std::io::Error::other)?;
            legacy
        };
        if invalidate_unverified_bilibili(&mut records) {
            persist_records(&db_path, &records).map_err(std::io::Error::other)?;
        }
        let pair_path = data_dir.join("extension-pair-code.txt");
        let pair_code = if pair_path.exists() {
            fs::read_to_string(&pair_path)?.trim().to_owned()
        } else {
            let code = Uuid::new_v4().simple().to_string();
            fs::write(&pair_path, &code)?;
            code
        };
        if pair_code.len() != 32 || !pair_code.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("扩展配对码文件无效".into());
        }
        Ok(Self {
            records: Mutex::new(records),
            capture_jobs: Mutex::new(Vec::new()),
            records_path: db_path,
            pair_code,
            last_extension_seen: Mutex::new(None),
            bridge_error: Mutex::new(None),
        })
    }

    pub fn start_capture_job(&self, input: CaptureJobStart) -> Result<CaptureJob, String> {
        if !matches!(input.record_type.as_str(), "video" | "article") { return Err("资料类型无效".into()); }
        let parsed = Url::parse(&input.url).map_err(|_| "网页地址无效".to_owned())?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none()
            || !parsed.username().is_empty() || parsed.password().is_some() { return Err("网页地址无效".into()); }
        if input.title.trim().is_empty() { return Err("资料标题为空".into()); }
        let title: String = input.title.trim().chars().take(300).collect();
        let source_url = parsed.to_string();
        let mut jobs = self.capture_jobs.lock().map_err(|_| "采集任务暂不可用".to_owned())?;
        jobs.retain(|entry| !(entry.source_url == source_url && entry.job.status == crate::ProcessStatus::Fault));
        if jobs.iter().filter(|entry| matches!(entry.job.status, crate::ProcessStatus::Extracting | crate::ProcessStatus::Summarizing)).count() >= 20 {
            return Err("同时采集的资料过多，请稍后重试".into());
        }
        let started_at_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
        let job = CaptureJob { id: format!("job-{}", Uuid::new_v4()), record_type: input.record_type,
            title, site: parsed.host_str().unwrap_or_default().to_owned(),
            status: crate::ProcessStatus::Extracting, fault_reason: None, started_at_ms };
        jobs.insert(0, CaptureJobEntry { job: job.clone(), source_url, started: Instant::now() });
        let mut fault_count = 0;
        jobs.retain(|entry| {
            if entry.job.status != crate::ProcessStatus::Fault { return true; }
            fault_count += 1;
            fault_count <= 20
        });
        Ok(job)
    }

    pub fn finish_capture_job(&self, id: &str, reason: Option<&str>) -> Result<bool, String> {
        let mut jobs = self.capture_jobs.lock().map_err(|_| "采集任务暂不可用".to_owned())?;
        let Some(index) = jobs.iter().position(|entry| entry.job.id == id) else { return Ok(false); };
        if let Some(reason) = reason {
            jobs[index].job.status = crate::ProcessStatus::Fault;
            jobs[index].job.fault_reason = Some(reason.chars().take(240).collect());
        } else {
            jobs.remove(index);
        }
        Ok(true)
    }

    pub fn set_capture_job_stage(&self, id: &str) -> Result<bool, String> {
        let mut jobs = self.capture_jobs.lock().map_err(|_| "采集任务暂不可用".to_owned())?;
        let Some(entry) = jobs.iter_mut().find(|entry| entry.job.id == id) else { return Ok(false); };
        if entry.job.status != crate::ProcessStatus::Extracting { return Ok(false); }
        entry.job.status = crate::ProcessStatus::Summarizing;
        entry.started = Instant::now();
        Ok(true)
    }

    pub fn expire_capture_jobs(&self) -> bool {
        let Ok(mut jobs) = self.capture_jobs.lock() else { return false; };
        let mut changed = false;
        for entry in jobs.iter_mut() {
            if matches!(entry.job.status, crate::ProcessStatus::Extracting | crate::ProcessStatus::Summarizing)
                && entry.started.elapsed() >= Duration::from_secs(600) {
                entry.job.status = crate::ProcessStatus::Fault;
                entry.job.fault_reason = Some("采集等待超过 10 分钟，请在原页面重试。".into());
                changed = true;
            }
        }
        changed
    }

    pub fn capture_job_snapshot(&self) -> Result<Vec<CaptureJob>, String> {
        let jobs = self.capture_jobs.lock().map_err(|_| "采集任务暂不可用".to_owned())?;
        Ok(jobs.iter().map(|entry| entry.job.clone()).collect())
    }

    pub fn dismiss_capture_job(&self, id: &str) -> Result<bool, String> {
        let mut jobs = self.capture_jobs.lock().map_err(|_| "采集任务暂不可用".to_owned())?;
        let Some(index) = jobs.iter().position(|entry| entry.job.id == id && entry.job.status == crate::ProcessStatus::Fault) else { return Ok(false); };
        jobs.remove(index);
        Ok(true)
    }

    pub fn capture(&self, input: ArticleCapture) -> Result<CaptureResult, String> {
        let mut parsed = Url::parse(&input.url).map_err(|_| "网页地址无效".to_owned())?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err("只支持 HTTP 或 HTTPS 网页".into());
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("网页地址不能包含账号或密码".into());
        }
        parsed.set_fragment(None);
        let bilibili_article = normalize_bilibili_article_url(&mut parsed);
        let title = input.title.trim();
        let text = input.text.trim();
        if title.is_empty() || title.chars().count() > 300 {
            return Err("网页标题为空或过长".into());
        }
        if text.chars().count() < 80 || text.chars().count() > 300_000 {
            return Err("正文太短或超过 30 万字".into());
        }
        if input.captured_at.len() > 40 || !input.captured_at.contains('T') {
            return Err("保存时间无效".into());
        }
        let mut records = self.records.lock().map_err(|_| "资料库暂不可用".to_owned())?;
        let existing_index = records.iter().position(|item| {
            if item.record_type != "article" { return false; }
            if item.source_url == parsed.as_str() { return true; }
            bilibili_article && Url::parse(&item.source_url).ok()
                .is_some_and(|mut saved_url| normalize_bilibili_article_url(&mut saved_url) && saved_url == parsed)
        });
        if let Some(index) = existing_index {
            if !bilibili_article { return Ok(CaptureResult { record: records[index].clone(), already_saved: true, updated: false }); }
        }
        let excerpt = text
            .split("\n\n")
            .map(str::trim)
            .filter(|paragraph| !paragraph.is_empty())
            .take(3)
            .map(|paragraph| paragraph.chars().take(240).collect())
            .collect();
        let record = KnowledgeRecord {
            id: format!("rec-{}", Uuid::new_v4()),
            record_type: "article".into(),
            title: title.into(),
            site: parsed.host_str().unwrap_or_default().into(),
            saved_at: input.captured_at,
            status: crate::ProcessStatus::Ready,
            source_url: parsed.to_string(),
            content_text: text.into(),
            excerpt,
            transcript: None,
            moments: None,
            duration_seconds: None,
            storyboard: None,
            fault_reason: None,
            caption_language: None,
            caption_source_verified: false,
        };
        if let Some(index) = existing_index {
            let existing_id = records[index].id.clone();
            let previous = std::mem::replace(&mut records[index], KnowledgeRecord { id: existing_id, ..record });
            if let Err(error) = persist_record_at(&self.records_path, &records[index], index, &records) {
                records[index] = previous;
                return Err(error);
            }
            return Ok(CaptureResult { record: records[index].clone(), already_saved: false, updated: true });
        }
        records.insert(0, record.clone());
        if let Err(error) = persist_record_at(&self.records_path, &records[0], 0, &records) {
            records.remove(0);
            return Err(error);
        }
        Ok(CaptureResult { record, already_saved: false, updated: false })
    }

    pub fn capture_video(&self, input: VideoCapture) -> Result<CaptureResult, String> {
        let mut parsed = Url::parse(&input.url).map_err(|_| "视频地址无效".to_owned())?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err("只支持 HTTP 或 HTTPS 视频地址".into());
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("视频地址不能包含账号或密码".into());
        }
        parsed.set_fragment(None);
        let host = parsed.host_str().unwrap_or_default().to_owned();
        let is_bilibili = host == "bilibili.com" || host.ends_with(".bilibili.com");
        if (host == "youtube.com" || host.ends_with(".youtube.com")) && parsed.path() == "/watch" {
            let video_id = parsed.query_pairs().find(|(key, _)| key == "v").map(|(_, value)| value.into_owned());
            if let Some(video_id) = video_id {
                parsed.query_pairs_mut().clear().append_pair("v", &video_id);
            }
        } else if (host == "bilibili.com" || host.ends_with(".bilibili.com")) && parsed.path().starts_with("/video/") {
            let page = parsed.query_pairs().find(|(key, _)| key == "p").map(|(_, value)| value.into_owned());
            parsed.set_query(None);
            if let Some(page) = page {
                parsed.query_pairs_mut().append_pair("p", &page);
            }
        }
        let title = input.title.trim();
        if title.is_empty() || title.chars().count() > 300 {
            return Err("视频标题为空或过长".into());
        }
        if input.captured_at.len() > 40 || !input.captured_at.contains('T') {
            return Err("保存时间无效".into());
        }
        if input.cues.len() > 5000 {
            return Err("字幕超过 5000 条，暂不能完整保存".into());
        }
        let mut cues = input.cues;
        for cue in &mut cues {
            cue.text = cue.text.trim().to_owned();
            if cue.text.chars().count() > 500 || cue.at_seconds > 864_000 {
                return Err("字幕条目过长或时间无效".into());
            }
        }
        cues.retain(|cue| !cue.text.is_empty());
        cues.sort_by_key(|cue| cue.at_seconds);
        let source_verified = !cues.is_empty();
        if source_verified && is_bilibili {
            let page_bvid = parsed.path_segments().and_then(|mut parts| {
                if parts.next() == Some("video") { parts.next().map(str::to_owned) } else { None }
            }).unwrap_or_default();
            let claimed_bvid = input.bvid.as_deref().unwrap_or_default();
            let claimed_cid = input.cid.unwrap_or_default();
            let track_url = input.track_url.as_deref().and_then(|value| Url::parse(value).ok());
            let track_host = track_url.as_ref().and_then(Url::host_str).unwrap_or_default();
            if page_bvid.is_empty() || !page_bvid.eq_ignore_ascii_case(claimed_bvid)
                || claimed_cid == 0 || track_url.as_ref().is_none_or(|url| url.scheme() != "https")
                || !(track_host == "hdslb.com" || track_host.ends_with(".hdslb.com"))
                || !track_url.as_ref().is_some_and(|url| url.path().contains(&claimed_cid.to_string())) {
                return Err("字幕来源与当前视频 CID 不匹配，已拒绝保存".into());
            }
        }
        let content_text = cues.iter().map(|cue| {
            format!("{:02}:{:02}:{:02} {}", cue.at_seconds / 3600,
                cue.at_seconds / 60 % 60, cue.at_seconds % 60, cue.text)
        }).collect::<Vec<_>>().join("\n");
        if content_text.chars().count() > 300_000 {
            return Err("字幕正文超过 30 万字，暂不能完整保存".into());
        }
        let status = if cues.is_empty() { crate::ProcessStatus::Fault } else { crate::ProcessStatus::Ready };
        let fault_reason = if cues.is_empty() {
            let reason = input.fault_reason.trim();
            Some(if reason.is_empty() { "该视频没有可读取的字幕轨道".into() }
                else { reason.chars().take(240).collect() })
        } else { None };
        let mut next_moment = 0;
        let moments: Vec<KeyMoment> = cues.iter().filter_map(|cue| {
            if cue.at_seconds < next_moment || next_moment > 864_000 { return None; }
            next_moment = cue.at_seconds.saturating_add(300);
            Some(KeyMoment {
                at_seconds: cue.at_seconds,
                title: cue.text.chars().take(24).collect(),
            })
        }).take(20).collect();
        let record = KnowledgeRecord {
            id: format!("rec-{}", Uuid::new_v4()),
            record_type: "video".into(),
            title: title.into(),
            site: host,
            saved_at: input.captured_at,
            status,
            source_url: parsed.to_string(),
            content_text,
            excerpt: Vec::new(),
            transcript: if cues.is_empty() { None } else { Some(cues) },
            moments: if moments.is_empty() { None } else { Some(moments) },
            duration_seconds: input.duration_seconds.filter(|duration| *duration <= 864_000),
            storyboard: None,
            fault_reason,
            caption_language: if input.language.trim().is_empty() { None }
                else { Some(input.language.chars().take(80).collect()) },
            caption_source_verified: source_verified,
        };
        let mut records = self.records.lock().map_err(|_| "资料库暂不可用".to_owned())?;
        if let Some(index) = records.iter().position(|item| item.record_type == "video" && item.source_url == record.source_url) {
            if records[index].status == crate::ProcessStatus::Ready {
                return Ok(CaptureResult { record: records[index].clone(), already_saved: true, updated: false });
            }
            let existing_id = records[index].id.clone();
            let old = std::mem::replace(&mut records[index], KnowledgeRecord {
                id: existing_id, ..record.clone()
            });
            if let Err(error) = persist_record_at(&self.records_path, &records[index], index, &records) {
                records[index] = old;
                return Err(error);
            }
            return Ok(CaptureResult { record: records[index].clone(), already_saved: false, updated: true });
        }
        records.insert(0, record.clone());
        if let Err(error) = persist_record_at(&self.records_path, &records[0], 0, &records) {
            records.remove(0);
            return Err(error);
        }
        Ok(CaptureResult { record, already_saved: false, updated: false })
    }

    pub fn mark_extension_seen(&self) {
        if let Ok(mut last_seen) = self.last_extension_seen.lock() {
            *last_seen = Some(Instant::now());
        }
    }
}

fn normalize_bilibili_article_url(url: &mut Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    if host != "bilibili.com" && !host.ends_with(".bilibili.com") { return false; }
    let path = url.path().trim_end_matches('/');
    let opus = path.strip_prefix("/opus/").is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()));
    let read = path.strip_prefix("/read/cv").is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()));
    if !opus && !read { return false; }
    let clean_path = path.to_owned();
    url.set_path(&clean_path);
    url.set_query(None);
    true
}

fn invalidate_unverified_bilibili(records: &mut [KnowledgeRecord]) -> bool {
    let mut changed = false;
    for record in records {
        let host = Url::parse(&record.source_url).ok()
            .and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_default();
        if record.record_type == "video"
            && (host == "bilibili.com" || host.ends_with(".bilibili.com"))
            && record.status == crate::ProcessStatus::Ready
            && !record.caption_source_verified {
            record.status = crate::ProcessStatus::Fault;
            record.transcript = None;
            record.moments = None;
            record.content_text.clear();
            record.caption_language = None;
            record.fault_reason = Some("旧版字幕来源未核对，已移除错误字幕。请在视频页重新捕获。".into());
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_bilibili_caption_is_removed_before_display() {
        let mut records: Vec<KnowledgeRecord> = serde_json::from_value(serde_json::json!([{
            "id": "rec-old", "type": "video", "title": "Tauri 教程", "site": "www.bilibili.com",
            "savedAt": "2026-11-02T00:00:00Z", "status": "ready",
            "sourceUrl": "https://www.bilibili.com/video/BV1fLpg6HE8E/",
            "contentText": "错误的字幕", "excerpt": [],
            "transcript": [{ "atSeconds": 1, "text": "错误的字幕" }],
            "moments": [{ "atSeconds": 1, "title": "错误的字幕" }]
        }])).unwrap();
        assert!(invalidate_unverified_bilibili(&mut records));
        assert_eq!(records[0].status, crate::ProcessStatus::Fault);
        assert!(records[0].transcript.is_none());
        assert!(records[0].moments.is_none());
        assert!(records[0].content_text.is_empty());
        assert_eq!(records[0].id, "rec-old");
        assert!(!invalidate_unverified_bilibili(&mut records));
    }

    #[test]
    fn recapturing_bilibili_opus_replaces_contaminated_text() {
        let old: KnowledgeRecord = serde_json::from_value(serde_json::json!({
            "id": "rec-existing", "type": "article", "title": "原专栏", "site": "www.bilibili.com",
            "savedAt": "2026-11-02T00:00:00Z", "status": "ready",
            "sourceUrl": "https://www.bilibili.com/opus/396287239310805041?from=search",
            "contentText": "会员提示和旧正文", "excerpt": []
        })).unwrap();
        let path = std::env::temp_dir().join(format!("knowflow-article-test-{}.json", Uuid::new_v4()));
        let state = AppState {
            records: Mutex::new(vec![old]), capture_jobs: Mutex::new(Vec::new()), records_path: path.clone(), pair_code: String::new(),
            last_extension_seen: Mutex::new(None), bridge_error: Mutex::new(None),
        };
        let result = state.capture(ArticleCapture {
            url: "https://www.bilibili.com/opus/396287239310805041".into(),
            title: "原专栏".into(), text: "这里是经过正文区域核对的专栏文字。".repeat(8),
            captured_at: "2026-11-03T00:00:00Z".into(),
        }).unwrap();
        assert!(!result.already_saved);
        assert!(result.updated);
        assert_eq!(result.record.id, "rec-existing");
        assert!(!result.record.content_text.contains("会员提示"));
        assert_eq!(result.record.source_url, "https://www.bilibili.com/opus/396287239310805041");
        let saved: Vec<KnowledgeRecord> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id, "rec-existing");
        fs::remove_file(path).unwrap();
    }
}

pub(crate) fn persist_records(path: &PathBuf, records: &[KnowledgeRecord]) -> Result<(), String> {
    if path.extension().is_some_and(|value| value == "db") { return crate::db::write_records(path, records); }
    let serialized = serde_json::to_vec_pretty(records).map_err(|_| "资料无法编码".to_owned())?;
    let temp_path = path.with_extension("json.tmp");
    let backup_path = path.with_extension("json.bak");
    let mut temp = fs::File::create(&temp_path).map_err(|error| format!("保存资料失败：{error}"))?;
    temp.write_all(&serialized).map_err(|error| format!("保存资料失败：{error}"))?;
    temp.sync_all().map_err(|error| format!("保存资料失败：{error}"))?;
    drop(temp);
    if backup_path.exists() {
        fs::remove_file(&backup_path).map_err(|error| format!("清理旧备份失败：{error}"))?;
    }
    let had_previous = path.exists();
    if had_previous {
        fs::rename(path, &backup_path).map_err(|error| format!("备份资料失败：{error}"))?;
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        if had_previous {
            let _ = fs::rename(&backup_path, path);
        }
        return Err(format!("保存资料失败：{error}"));
    }
    if had_previous {
        let _ = fs::remove_file(&backup_path);
    }
    Ok(())
}

pub(crate) fn persist_record_at(path: &PathBuf, record: &KnowledgeRecord, position: usize, all_records: &[KnowledgeRecord]) -> Result<(), String> {
    if path.extension().is_some_and(|value| value == "db") { crate::db::upsert_record(path, record, position) }
    else { persist_records(path, all_records) }
}

#[tauri::command]
pub fn list_records(state: State<'_, AppState>) -> Result<Vec<RecordSummary>, String> {
    let records = state.records.lock().map_err(|_| "资料库暂不可用")?;
    Ok(records.iter().map(|record| RecordSummary {
        id: record.id.clone(), record_type: record.record_type.clone(), title: record.title.clone(),
        site: record.site.clone(), saved_at: record.saved_at.clone(), status: record.status,
        source_url: record.source_url.clone(),
        duration_seconds: record.duration_seconds, caption_language: record.caption_language.clone(), fault_reason: record.fault_reason.clone(),
    }).collect())
}

#[tauri::command]
pub fn list_capture_jobs(state: State<'_, AppState>) -> Result<Vec<CaptureJob>, String> {
    state.capture_job_snapshot()
}

#[tauri::command]
pub fn dismiss_capture_job(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state.dismiss_capture_job(&id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSummary {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) record_type: String,
    pub(crate) title: String,
    pub(crate) site: String,
    pub(crate) saved_at: String,
    pub(crate) status: crate::ProcessStatus,
    pub(crate) source_url: String,
    pub(crate) duration_seconds: Option<u32>,
    pub(crate) caption_language: Option<String>,
    pub(crate) fault_reason: Option<String>,
}

#[tauri::command]
pub fn query_record_page(state: State<'_, AppState>, record_type: String, search: String, offset: u64, limit: u64) -> Result<crate::db::RecordPage, String> {
    crate::db::query_record_page(&state.records_path, &record_type, &search, offset, limit)
}

#[tauri::command]
pub fn get_record(state: State<'_, AppState>, id: String) -> Result<KnowledgeRecord, String> {
    crate::db::get_record(&state.records_path, &id)
}

#[tauri::command]
pub fn query_record_segments(state: State<'_, AppState>, record_id: String, start_at: Option<u32>, end_at: Option<u32>, search: String, limit: u64) -> Result<Vec<crate::db::RecordSegment>, String> {
    crate::db::query_record_segments(&state.records_path, &record_id, start_at, end_at, &search, limit)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    pair_code: String,
    connected: bool,
    error: Option<String>,
}

#[tauri::command]
pub fn get_bridge_status(state: State<'_, AppState>) -> BridgeStatus {
    let connected = state.last_extension_seen.lock().ok()
        .and_then(|seen| *seen)
        .is_some_and(|seen| seen.elapsed() < Duration::from_secs(95));
    let error = state.bridge_error.lock().ok().and_then(|error| error.clone());
    BridgeStatus { pair_code: state.pair_code.clone(), connected, error }
}

#[cfg(test)]
mod capture_job_tests {
    use super::*;

    fn state() -> AppState {
        AppState { records: Mutex::new(Vec::new()), capture_jobs: Mutex::new(Vec::new()),
            records_path: PathBuf::new(), pair_code: String::new(),
            last_extension_seen: Mutex::new(None), bridge_error: Mutex::new(None) }
    }

    fn start(state: &AppState) -> CaptureJob {
        state.start_capture_job(CaptureJobStart { url: "https://example.com/video".into(),
            title: "测试视频".into(), record_type: "video".into() }).unwrap()
    }

    #[test]
    fn capture_job_rotates_only_during_real_stages() {
        let state = state();
        let job = start(&state);
        assert_eq!(job.status, crate::ProcessStatus::Extracting);
        assert!(state.set_capture_job_stage(&job.id).unwrap());
        assert_eq!(state.capture_job_snapshot().unwrap()[0].status, crate::ProcessStatus::Summarizing);
        assert!(state.finish_capture_job(&job.id, None).unwrap());
        assert!(state.capture_job_snapshot().unwrap().is_empty());
    }

    #[test]
    fn failed_or_stale_jobs_stop_and_can_be_dismissed() {
        let state = state();
        let job = start(&state);
        state.capture_jobs.lock().unwrap()[0].started = Instant::now() - Duration::from_secs(601);
        assert!(state.expire_capture_jobs());
        assert_eq!(state.capture_job_snapshot().unwrap()[0].status, crate::ProcessStatus::Fault);
        assert!(!state.set_capture_job_stage(&job.id).unwrap());
        assert!(state.dismiss_capture_job(&job.id).unwrap());
        assert!(state.capture_job_snapshot().unwrap().is_empty());
    }
}
