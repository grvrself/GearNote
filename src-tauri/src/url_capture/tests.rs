use super::*;
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// 本地 mock HTTP server（零外网依赖）
// ---------------------------------------------------------------------------

struct MockRequest {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

struct MockResponse {
    status: u16,
    content_type: String,
    body: Vec<u8>,
    location: Option<String>,
}

impl MockResponse {
    fn json(value: Value) -> Self {
        Self { status: 200, content_type: "application/json".into(), body: value.to_string().into_bytes(), location: None }
    }
    fn html(body: &str) -> Self {
        Self { status: 200, content_type: "text/html; charset=utf-8".into(), body: body.as_bytes().to_vec(), location: None }
    }
    fn redirect(location: &str) -> Self {
        Self { status: 302, content_type: "text/plain".into(), body: Vec::new(), location: Some(location.to_owned()) }
    }
}

fn read_mock_request(stream: &mut std::net::TcpStream) -> Option<MockRequest> {
    let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?.to_owned();
    let mut headers = HashMap::new();
    let mut length = 0usize;
    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;
        if line == "\r\n" || line == "\n" { break; }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            if key == "content-length" { length = value.parse().unwrap_or(0); }
            headers.insert(key, value);
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 { reader.read_exact(&mut body).ok()?; }
    Some(MockRequest { method, target, headers, body })
}

fn write_mock_response(stream: &mut std::net::TcpStream, response: &MockResponse) {
    let reason = match response.status { 200 => "OK", 302 => "Found", 404 => "Not Found", _ => "Status" };
    let mut head = format!("HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status, response.content_type, response.body.len());
    if let Some(location) = &response.location { head.push_str(&format!("Location: {location}\r\n")); }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&response.body);
    let _ = stream.flush();
}

struct MockServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: std::thread::JoinHandle<()>,
}

impl MockServer {
    fn start(handler: impl Fn(&MockRequest) -> MockResponse + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        Self::start_on(listener, handler)
    }

    fn start_on(listener: TcpListener, handler: impl Fn(&MockRequest) -> MockResponse + Send + Sync + 'static) -> Self {
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let handler = Arc::new(handler);
        let handle = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                        if let Some(request) = read_mock_request(&mut stream) {
                            let response = handler(&request);
                            write_mock_response(&mut stream, &response);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock =>
                        std::thread::sleep(Duration::from_millis(5)),
                    Err(_) => break,
                }
            }
        });
        Self { port, stop, handle }
    }

    fn origin(&self) -> Url { Url::parse(&format!("http://127.0.0.1:{}", self.port)).unwrap() }
    fn config(&self) -> FetchConfig { FetchConfig::for_test(self.origin()) }
    fn stop(self) { self.stop.store(true, Ordering::SeqCst); let _ = self.handle.join(); }
}

fn test_state() -> (AppState, PathBuf) {
    let dir = std::env::temp_dir().join(format!("gearnote-urlcap-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(crate::db::FILE_NAME);
    crate::db::write_records(&path, &[]).unwrap();
    (AppState::for_test("", path), dir)
}

fn capture_to_db(state: &AppState, url: &str, cookies: Option<&str>, config: &FetchConfig) -> Result<CaptureResult, String> {
    tauri::async_runtime::block_on(async {
        let parsed = extract_http_url(url)?;
        match prepare_capture(&parsed, cookies, config).await? {
            Prepared::Article(input) => state.capture(input),
            Prepared::Video(input) => state.capture_video(input),
        }
    })
}

const FAKE_COOKIES: &str = "# Netscape HTTP Cookie File\n#HttpOnly_.bilibili.com\tTRUE\t/\tTRUE\t1800000000\tSESSDATA\tfake-test-value\n.bilibili.com\tTRUE\t/\tFALSE\t1800000000\tbuvid3\tfake-buvid\n";

fn bilibili_response(req: &MockRequest, require_cookie: bool) -> MockResponse {
    let path = req.target.split('?').next().unwrap_or("");
    let cookie = req.headers.get("cookie").cloned().unwrap_or_default();
    match path {
        "/x/web-interface/view" => {
            if require_cookie { assert!(cookie.contains("SESSDATA=fake-test-value"), "B 站请求必须附带 Cookie"); }
            MockResponse::json(json!({"code":0,"data":{"bvid":"BV1xx411c7mD","title":"测试视频标题","duration":120,
                "pages":[{"cid":111,"page":1,"duration":60},{"cid":123456,"page":2,"duration":120}]}}))
        }
        "/x/web-interface/nav" => {
            if require_cookie { assert!(cookie.contains("SESSDATA=fake-test-value"), "B 站请求必须附带 Cookie"); }
            MockResponse::json(json!({"code":0,"data":{"wbi_img":{
                "img_url":"https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png",
                "sub_url":"https://i0.hdslb.com/bfs/wbi/4932caff0ff746eab6f01bf08b70ac45.png"}}}))
        }
        "/x/player/wbi/v2" => {
            if require_cookie { assert!(cookie.contains("SESSDATA=fake-test-value"), "B 站请求必须附带 Cookie"); }
            let query = req.target.split_once('?').map(|(_, q)| q).unwrap_or("");
            let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
            let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default();
            assert_eq!(get("cid"), "123456", "应选中 p=2 的 CID");
            let expected_query = format!("bvid={}&cid={}&wts={}", get("bvid"), get("cid"), get("wts"));
            let expected = md5_hex(format!("{expected_query}ea1db124af3c7062474693fa704f4ff8").as_bytes());
            assert_eq!(get("w_rid"), expected, "WBI 签名必须正确");
            MockResponse::json(json!({"code":0,"data":{"bvid":"BV1xx411c7mD","cid":123456,"need_login_subtitle":false,
                "subtitle":{"subtitles":[{"lan":"zh-CN","lan_doc":"中文（中国）","ai_type":0,
                    "subtitle_url":"https://i0.hdslb.com/bfs/subtitle/123456-zh.json"}]}}}))
        }
        "/bfs/subtitle/123456-zh.json" => {
            assert!(!req.headers.contains_key("cookie"), "hdslb 字幕请求不得附带 B 站 Cookie");
            MockResponse::json(json!({"body":[{"from":0.0,"to":1.0,"content":"你好"},{"from":1.0,"to":2.0,"content":"世界"}]}))
        }
        other => panic!("unexpected bilibili path: {other}"),
    }
}

// ---------------------------------------------------------------------------
// 链接提取
// ---------------------------------------------------------------------------

#[test]
fn extract_http_url_handles_share_text_and_trailing_punctuation() {
    assert_eq!(extract_http_url("【标题】 https://b23.tv/abc123?p=2 复制此链接打开 bilibili").unwrap().as_str(),
        "https://b23.tv/abc123?p=2");
    assert_eq!(extract_http_url("https://b23.tv/abc】").unwrap().as_str(), "https://b23.tv/abc");
    assert_eq!(extract_http_url("https://b23.tv/abc）").unwrap().as_str(), "https://b23.tv/abc");
    assert_eq!(extract_http_url("https://b23.tv/abc。").unwrap().as_str(), "https://b23.tv/abc");
    assert_eq!(extract_http_url("https://example.com/a).").unwrap().as_str(), "https://example.com/a");
    assert_eq!(extract_http_url("看看 https://example.com/x 和 https://other.com/y").unwrap().host_str(), Some("example.com"));
    assert_eq!(extract_http_url("https://example.com/plain").unwrap().as_str(), "https://example.com/plain");
}

#[test]
fn extract_http_url_rejects_non_http_and_userinfo() {
    assert!(extract_http_url("file:///etc/passwd").is_err());
    assert!(extract_http_url("ftp://example.com/x").is_err());
    assert!(extract_http_url("https://user:pass@example.com/").is_err());
    assert!(extract_http_url("https://user@example.com/").is_err());
    assert!(extract_http_url("https://").is_err());
    assert!(extract_http_url("没有链接的文本").is_err());
}

// ---------------------------------------------------------------------------
// SSRF 防护
// ---------------------------------------------------------------------------

#[test]
fn non_public_ips_are_rejected() {
    for ip in ["127.0.0.1", "10.0.0.1", "172.16.5.1", "192.168.1.1", "169.254.169.254",
        "100.64.0.1", "0.0.0.0", "224.0.0.1", "240.0.0.1", "192.0.2.1", "198.18.0.1",
        "::1", "::", "fc00::1", "fe80::1", "ff02::1", "::ffff:127.0.0.1", "2001:db8::1"]
    {
        assert!(!is_public_ip(ip.parse().unwrap()), "{ip} 应被判定为非公网");
    }
    for ip in ["8.8.8.8", "1.1.1.1", "93.184.216.34", "2606:4700::1111", "::ffff:8.8.8.8"] {
        assert!(is_public_ip(ip.parse().unwrap()), "{ip} 应被判定为公网");
    }
}

#[test]
fn ensure_public_host_rejects_local_names_and_literals() {
    for host in ["localhost", "foo.localhost", "foo.local", "127.0.0.1", "10.0.0.1",
        "169.254.169.254", "[::1]", "::ffff:127.0.0.1", "localhost."]
    {
        assert!(ensure_public_host(host).is_err(), "{host} 应被拒绝");
    }
    assert!(ensure_public_host("example.com").is_ok());
    assert!(ensure_public_host("api.bilibili.com").is_ok());
    assert!(ensure_public_host("8.8.8.8").is_ok());
}

#[test]
fn production_config_rejects_loopback_before_any_connection() {
    let config = FetchConfig::production();
    assert!(!config.allow_loopback);
    assert!(config.transport_origin.is_none());
    let url = Url::parse("http://127.0.0.1:1/").unwrap();
    let error = tauri::async_runtime::block_on(fetch_bytes(&url, &config, None, reqwest::Method::GET, None, None)).err().unwrap();
    assert!(error.contains("非公网"), "{error}");
}

// ---------------------------------------------------------------------------
// MD5 与 WBI
// ---------------------------------------------------------------------------

#[test]
fn md5_matches_rfc1321_vectors() {
    assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(md5_hex(b"message digest"), "f96b697d7cb7938d525a2f31aaf161d0");
    assert_eq!(md5_hex(b"abcdefghijklmnopqrstuvwxyz"), "c3fcd3d76192e4007dfb496cca67e13b");
    assert_eq!(md5_hex(b"The quick brown fox jumps over the lazy dog"), "9e107d9d372bb6826bd81d3542a419d6");
}

#[test]
fn wbi_mixin_key_and_signature_match_official_vectors() {
    let mixin = wbi_mixin_key(
        "https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png",
        "https://i0.hdslb.com/bfs/wbi/4932caff0ff746eab6f01bf08b70ac45.png",
    ).unwrap();
    assert_eq!(mixin, "ea1db124af3c7062474693fa704f4ff8");
    let params = [("bvid".to_string(), "BV1xx411c7mD".to_string()),
        ("cid".to_string(), "123456".to_string()), ("wts".to_string(), "1700000000".to_string())];
    let (query, w_rid) = wbi_sign(&params, &mixin);
    assert_eq!(query, "bvid=BV1xx411c7mD&cid=123456&wts=1700000000");
    assert_eq!(w_rid, "7efd24a7599a55b8a36489705227eae5");
    let docs = [("foo", "114"), ("bar", "514"), ("baz", "1919810"), ("wts", "1702204169")]
        .map(|(k, v)| (k.to_string(), v.to_string()));
    let (query2, rid2) = wbi_sign(&docs, &mixin);
    assert_eq!(query2, "bar=514&baz=1919810&foo=114&wts=1702204169");
    assert_eq!(rid2, "6149fdadf571698ca7e6a567265cd0ee");
}

#[test]
fn utc_iso_format_matches_known_timestamps() {
    assert_eq!(format_utc_iso(0), "1970-01-01T00:00:00Z");
    assert_eq!(format_utc_iso(1700000000), "2023-11-14T22:13:20Z");
    assert!(format_utc_iso_now().contains('T'));
}

// ---------------------------------------------------------------------------
// 字幕解析与轨道优先级
// ---------------------------------------------------------------------------

#[test]
fn youtube_json3_and_vtt_parse_correctly() {
    let value = json!({"events":[{"tStartMs":0,"segs":[{"utf8":"你好"}]},
        {"tStartMs":1500,"segs":[{"utf8":"世"},{"utf8":"界"}]},{"tStartMs":2000,"segs":[]}]});
    let cues = parse_youtube_json3(&value).unwrap();
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].at_seconds, 0);
    assert_eq!(cues[1].at_seconds, 2);
    assert_eq!(cues[1].text, "世界");

    let vtt = "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\n你好\n\n00:01:02.500 --> 00:01:03.000\n<c>世界</c>\n\n";
    let cues = parse_vtt(vtt).unwrap();
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[1].at_seconds, 63);
    assert_eq!(cues[1].text, "世界");
}

#[test]
fn track_priority_prefers_chinese_manual_over_english_asr() {
    let youtube = vec![json!({"languageCode":"en","kind":"asr"}), json!({"languageCode":"zh-Hans"}), json!({"languageCode":"fr"})];
    let best = youtube.iter().min_by_key(|track| youtube_track_rank(track)).unwrap();
    assert_eq!(best["languageCode"], "zh-Hans");
    let bilibili = vec![json!({"lan":"ai-zh","ai_type":1}), json!({"lan":"zh-CN","ai_type":0}), json!({"lan":"en","ai_type":0})];
    let best = bilibili.iter().min_by_key(|track| bilibili_track_rank(track)).unwrap();
    assert_eq!(best["lan"], "zh-CN");
}

#[test]
fn bilibili_subtitle_url_must_be_https_hdslb_with_cid() {
    assert!(verified_bilibili_subtitle_url("https://i0.hdslb.com/bfs/subtitle/123456-zh.json", 123456).is_some());
    assert!(verified_bilibili_subtitle_url("//i0.hdslb.com/bfs/subtitle/123456.json", 123456).is_some());
    assert!(verified_bilibili_subtitle_url("https://evil.com/123456.json", 123456).is_none());
    assert!(verified_bilibili_subtitle_url("https://i0.hdslb.com/bfs/subtitle/999.json", 123456).is_none());
    assert!(verified_bilibili_subtitle_url("http://i0.hdslb.com/123456.json", 123456).is_none());
}

#[test]
fn cue_limit_and_truncation_are_enforced() {
    let long = "字".repeat(600);
    let cues = normalize_cues([(0.0, long)]).unwrap();
    assert_eq!(cues[0].text.chars().count(), 500);
    let many: Vec<(f64, String)> = (0..5001).map(|i| (i as f64, "字幕".to_string())).collect();
    assert!(normalize_cues(many).is_err());
}

// ---------------------------------------------------------------------------
// HTML 抽取
// ---------------------------------------------------------------------------

#[test]
fn html_extraction_strips_noise_and_decodes_entities() {
    let html = r#"<html><head><title>页标题</title><script>evil()</script><style>x{color:red}</style></head>
        <body><nav>导航</nav><article><p>第一段 &amp; 内容</p><p>第二段 &lt;标签&gt;</p></article>
        <footer>页脚</footer><aside>侧栏</aside><form>表单</form></body></html>"#;
    let text = extract_article_text(html);
    assert!(text.contains("第一段 & 内容"), "{text}");
    assert!(text.contains("第二段 <标签>"), "{text}");
    for noise in ["导航", "页脚", "侧栏", "表单", "evil", "color:red"] {
        assert!(!text.contains(noise), "正文不应包含 {noise}");
    }
    assert_eq!(extract_title(html).unwrap(), "页标题");
    assert_eq!(decode_entities("&amp;&lt;&gt;&quot;&#39;&nbsp;&#65;&#x42;"), "&<>\"' AB");
}

#[test]
fn opus_container_takes_priority_and_og_title_wins() {
    let html = r#"<head><meta property="og:title" content="专栏 &amp; 标题"><title>普通标题</title></head>
        <div class="opus-module-content"><p>专栏正文第一段。</p><p>专栏正文第二段。</p></div><p>外部段落</p>"#;
    let text = extract_article_text(html);
    assert!(text.contains("专栏正文第一段"));
    assert!(!text.contains("外部段落"));
    assert_eq!(extract_title(html).unwrap(), "专栏 & 标题");
}

#[test]
fn article_body_container_and_role_main_are_used() {
    let html = r#"<div role="main"><p>角色主区域正文内容足够长的一段文字。</p></div><p>外面的段落</p>"#;
    assert!(extract_article_text(html).contains("角色主区域"));
    let html2 = r#"<MAIN><P>主区域正文。</P></MAIN>"#;
    assert!(extract_article_text(html2).contains("主区域正文"));
    let html_upper = r#"<HEAD><META PROPERTY="og:title" CONTENT="大写属性标题"></HEAD><DIV ROLE="main"><P>正文内容足够展示。</P></DIV>"#;
    assert_eq!(extract_title(html_upper).unwrap(), "大写属性标题");
    assert!(extract_article_text(html_upper).contains("正文内容足够展示"));
}

// ---------------------------------------------------------------------------
// 内存 cookies 校验
// ---------------------------------------------------------------------------

#[test]
fn cookies_must_be_bilibili_netscape_with_sessdata() {
    let cookies = parse_bilibili_cookies(FAKE_COOKIES).unwrap();
    assert!(cookies.header.contains("SESSDATA=fake-test-value"));
    assert!(cookies.header.contains("buvid3=fake-buvid"));
    assert!(parse_bilibili_cookies(".bilibili.com\tTRUE\t/\tTRUE\t1800000000\tbuvid3\tx\n").is_err());
    assert!(parse_bilibili_cookies(".example.com\tTRUE\t/\tTRUE\t1800000000\tSESSDATA\tx\n").is_err());
    assert!(parse_bilibili_cookies(".bilibili.com\tTRUE\t/\tTRUE\t1800000000\tSESSDATA\tval\rbad\n").is_err());
    assert!(parse_bilibili_cookies("not-a-cookie\n").is_err());
}

// ---------------------------------------------------------------------------
// 端到端集成测试（本地 mock + SQLite）
// ---------------------------------------------------------------------------

#[test]
fn bilibili_wbi_video_capture_writes_sqlite() {
    let server = MockServer::start(|req| bilibili_response(req, false));
    let (state, dir) = test_state();
    let result = capture_to_db(&state, "https://www.bilibili.com/video/BV1xx411c7mD?p=2", None, &server.config()).unwrap();
    server.stop();
    assert_eq!(result.record.record_type, "video");
    assert_eq!(result.record.title, "测试视频标题");
    assert!(result.record.caption_source_verified);
    assert_eq!(result.record.source_url, "https://www.bilibili.com/video/BV1xx411c7mD?p=2");
    assert_eq!(result.record.duration_seconds, Some(120));
    let cues = result.record.transcript.as_ref().unwrap();
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].text, "你好");
    let saved = crate::db::get_record(&state.records_path, &result.record.id).unwrap();
    assert_eq!(saved.transcript.unwrap()[1].text, "世界");
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bilibili_cookies_attach_only_to_bilibili_domains() {
    let server = MockServer::start(|req| bilibili_response(req, true));
    let (state, dir) = test_state();
    let result = capture_to_db(&state, "https://www.bilibili.com/video/BV1xx411c7mD?p=2", Some(FAKE_COOKIES), &server.config()).unwrap();
    server.stop();
    assert!(result.record.caption_source_verified);
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bilibili_short_link_resolves_and_captures() {
    let server = MockServer::start(|req| {
        let path = req.target.split('?').next().unwrap_or("");
        if path == "/abc123" { return MockResponse::redirect("https://www.bilibili.com/video/BV1xx411c7mD?p=2"); }
        bilibili_response(req, false)
    });
    let (state, dir) = test_state();
    let result = capture_to_db(&state, "https://b23.tv/abc123", None, &server.config()).unwrap();
    server.stop();
    assert_eq!(result.record.source_url, "https://www.bilibili.com/video/BV1xx411c7mD?p=2");
    assert!(result.record.caption_source_verified);
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bilibili_need_login_subtitle_gives_actionable_error() {
    let server = MockServer::start(|req| {
        let path = req.target.split('?').next().unwrap_or("");
        match path {
            "/x/web-interface/view" => MockResponse::json(json!({"code":0,"data":{"bvid":"BV1xx411c7mD","title":"标题","pages":[{"cid":123456,"page":1,"duration":10}]}})),
            "/x/web-interface/nav" => MockResponse::json(json!({"code":0,"data":{"wbi_img":{
                "img_url":"https://i0.hdslb.com/bfs/wbi/7cd084941338484aae1ad9425b84077c.png",
                "sub_url":"https://i0.hdslb.com/bfs/wbi/4932caff0ff746eab6f01bf08b70ac45.png"}}})),
            "/x/player/wbi/v2" => MockResponse::json(json!({"code":0,"data":{"bvid":"BV1xx411c7mD","cid":123456,"need_login_subtitle":true,"subtitle":{"subtitles":[]}}})),
            other => panic!("unexpected path {other}"),
        }
    });
    let (state, dir) = test_state();
    let error = capture_to_db(&state, "https://www.bilibili.com/video/BV1xx411c7mD", None, &server.config()).err().unwrap();
    server.stop();
    assert!(error.contains("cookies.txt"), "{error}");
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn youtube_innertube_capture_writes_sqlite() {
    let server = MockServer::start(|req| {
        let path = req.target.split('?').next().unwrap_or("");
        match (req.method.as_str(), path) {
            ("POST", "/youtubei/v1/player") => {
                let body: Value = serde_json::from_slice(&req.body).unwrap();
                assert_eq!(body["videoId"], "dQw4w9WgXcQ");
                assert!(body["context"]["client"]["clientName"].is_string());
                MockResponse::json(json!({"videoDetails":{"videoId":"dQw4w9WgXcQ","title":"YouTube 测试视频","lengthSeconds":"60"},
                    "captions":{"playerCaptionsTracklistRenderer":{"captionTracks":[
                        {"baseUrl":"https://www.youtube.com/api/timedtext?v=dQw4w9WgXcQ&lang=en","languageCode":"en","kind":"asr","name":{"simpleText":"English (auto-generated)"}},
                        {"baseUrl":"https://www.youtube.com/api/timedtext?v=dQw4w9WgXcQ&lang=zh-Hans","languageCode":"zh-Hans","name":{"simpleText":"中文（简体）"}}]}}}))
            }
            ("GET", "/api/timedtext") => {
                let query = req.target.split_once('?').map(|(_, q)| q).unwrap_or("");
                assert!(query.contains("lang=zh-Hans"), "应优先中文人工轨道: {query}");
                assert!(query.contains("fmt=json3"));
                MockResponse::json(json!({"events":[{"tStartMs":0,"segs":[{"utf8":"你好"}]},{"tStartMs":1500,"segs":[{"utf8":"世界"}]}]}))
            }
            other => panic!("unexpected youtube request {other:?}"),
        }
    });
    let (state, dir) = test_state();
    let result = capture_to_db(&state, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, &server.config()).unwrap();
    server.stop();
    assert_eq!(result.record.record_type, "video");
    assert_eq!(result.record.title, "YouTube 测试视频");
    assert_eq!(result.record.source_url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    assert_eq!(result.record.duration_seconds, Some(60));
    assert_eq!(result.record.caption_language.as_deref(), Some("中文（简体）"));
    let cues = result.record.transcript.unwrap();
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[1].at_seconds, 2);
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn youtube_falls_back_to_vtt_when_json3_empty() {
    let server = MockServer::start(|req| {
        let path = req.target.split('?').next().unwrap_or("");
        match (req.method.as_str(), path) {
            ("POST", "/youtubei/v1/player") => MockResponse::json(json!({"videoDetails":{"videoId":"dQw4w9WgXcQ","title":"VTT 视频","lengthSeconds":"30"},
                "captions":{"playerCaptionsTracklistRenderer":{"captionTracks":[
                    {"baseUrl":"https://www.youtube.com/api/timedtext?v=dQw4w9WgXcQ&lang=en","languageCode":"en","name":{"simpleText":"English"}}]}}})),
            ("GET", "/api/timedtext") => {
                let query = req.target.split_once('?').map(|(_, q)| q).unwrap_or("");
                if query.contains("fmt=json3") {
                    MockResponse::json(json!({"events":[]}))
                } else {
                    assert!(query.contains("fmt=vtt"));
                    MockResponse { status: 200, content_type: "text/vtt".into(),
                        body: "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\n你好\n\n00:00:01.000 --> 00:00:02.000\n世界\n\n".as_bytes().to_vec(),
                        location: None }
                }
            }
            other => panic!("unexpected request {other:?}"),
        }
    });
    let (state, dir) = test_state();
    let result = capture_to_db(&state, "https://youtu.be/dQw4w9WgXcQ", None, &server.config()).unwrap();
    server.stop();
    let cues = result.record.transcript.unwrap();
    assert_eq!(cues.len(), 2);
    assert_eq!(cues[0].text, "你好");
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn youtube_watch_html_fallback_extracts_player_response() {
    let server = MockServer::start(|req| {
        let path = req.target.split('?').next().unwrap_or("");
        match (req.method.as_str(), path) {
            ("POST", "/youtubei/v1/player") => MockResponse::json(json!({"playabilityStatus":{"status":"ERROR"}})),
            ("GET", "/watch") => MockResponse::html(
                "<html><script>var ytInitialPlayerResponse = {\"videoDetails\":{\"videoId\":\"dQw4w9WgXcQ\",\"title\":\"HTML 回退\",\"lengthSeconds\":\"15\"},\"captions\":{\"playerCaptionsTracklistRenderer\":{\"captionTracks\":[{\"baseUrl\":\"https://www.youtube.com/api/timedtext?v=dQw4w9WgXcQ&lang=en\",\"languageCode\":\"en\",\"name\":{\"simpleText\":\"English\"}}]}}};</script></html>"),
            ("GET", "/api/timedtext") => MockResponse::json(json!({"events":[{"tStartMs":0,"segs":[{"utf8":"回退字幕"}]}]})),
            other => panic!("unexpected request {other:?}"),
        }
    });
    let (state, dir) = test_state();
    let result = capture_to_db(&state, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", None, &server.config()).unwrap();
    server.stop();
    assert_eq!(result.record.title, "HTML 回退");
    assert_eq!(result.record.transcript.unwrap()[0].text, "回退字幕");
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn article_capture_writes_sqlite() {
    let server = MockServer::start(|req| {
        assert_eq!(req.target.split('?').next().unwrap_or(""), "/post");
        MockResponse::html(r#"<html><head><meta property="og:title" content="测试文章 &amp; 标题"><title>忽略</title>
            <script>var secret="不应出现";</script><style>body{color:red}</style></head>
            <body><nav>导航不应出现</nav><article><h1>正文标题</h1>
            <p>第一段内容，包含足够多的文字以达到八十字门槛。</p>
            <p>第二段内容 &lt;实体&gt; 解码 &amp; 验证。</p>
            <p>第三段补充文字，确保整体正文长度超过八十个字符的限制要求。</p>
            </article><footer>页脚不应出现</footer></body></html>"#)
    });
    let (state, dir) = test_state();
    let result = capture_to_db(&state, "https://example.com/post", None, &server.config()).unwrap();
    server.stop();
    assert_eq!(result.record.record_type, "article");
    assert_eq!(result.record.title, "测试文章 & 标题");
    assert_eq!(result.record.source_url, "https://example.com/post");
    let text = &result.record.content_text;
    assert!(text.contains("第一段内容"));
    assert!(text.contains("第二段内容 <实体> 解码 & 验证"));
    for noise in ["不应出现", "secret", "color:red"] {
        assert!(!text.contains(noise), "正文不应包含 {noise}");
    }
    assert!(text.chars().count() >= 80);
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn short_article_gives_explainable_error() {
    let server = MockServer::start(|_| MockResponse::html(
        "<html><head><title>短</title></head><body><article><p>太短了</p></article></body></html>"));
    let (state, dir) = test_state();
    let error = capture_to_db(&state, "https://example.com/short", None, &server.config()).err().unwrap();
    server.stop();
    assert!(error.contains("网页正文不足 80 字"), "{error}");
    assert!(error.contains("浏览器扩展"));
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn binary_content_type_is_rejected() {
    let server = MockServer::start(|_| MockResponse { status: 200, content_type: "application/octet-stream".into(),
        body: vec![0u8; 200], location: None });
    let (state, dir) = test_state();
    let error = capture_to_db(&state, "https://example.com/file.bin", None, &server.config()).err().unwrap();
    server.stop();
    assert!(error.contains("不是可读取的网页文本"), "{error}");
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn redirects_to_loopback_or_localhost_are_blocked() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MockServer::start_on(listener, move |req| {
        let path = req.target.split('?').next().unwrap_or("");
        match path {
            "/redirect-ip" => MockResponse::redirect(&format!("http://127.0.0.1:{port}/secret")),
            "/redirect-localhost" => MockResponse::redirect(&format!("http://localhost:{port}/secret")),
            "/secret" => panic!("SSRF 防护失效：请求到达了内部地址"),
            other => panic!("unexpected path {other}"),
        }
    });
    let config = server.config();
    let (state, dir) = test_state();
    let error = capture_to_db(&state, "https://example.com/redirect-ip", None, &config).err().unwrap();
    assert!(error.contains("非公网"), "{error}");
    let error = capture_to_db(&state, "https://example.com/redirect-localhost", None, &config).err().unwrap();
    assert!(error.contains("localhost"), "{error}");
    server.stop();
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn short_link_redirect_outside_whitelist_is_blocked() {
    let server = MockServer::start(|req| {
        let path = req.target.split('?').next().unwrap_or("");
        match path {
            "/evil" => MockResponse::redirect("https://evil.com/x"),
            other => panic!("unexpected path {other}"),
        }
    });
    let (state, dir) = test_state();
    let error = capture_to_db(&state, "https://b23.tv/evil", None, &server.config()).err().unwrap();
    server.stop();
    assert!(error.contains("不在允许的域名范围内"), "{error}");
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn oversized_response_is_rejected() {
    let server = MockServer::start(|_| MockResponse { status: 200, content_type: "text/html".into(),
        body: vec![b'a'; 5 * 1024 * 1024], location: None });
    let (state, dir) = test_state();
    let error = capture_to_db(&state, "https://example.com/big", None, &server.config()).err().unwrap();
    server.stop();
    assert!(error.contains("4 MB"), "{error}");
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cookies_for_non_bilibili_route_are_rejected() {
    let (state, dir) = test_state();
    let config = FetchConfig::production();
    let error = capture_to_db(&state, "https://example.com/post", Some(FAKE_COOKIES), &config).err().unwrap();
    assert!(error.contains("仅适用于 B 站视频链接"), "{error}");
    drop(state);
    std::fs::remove_dir_all(dir).unwrap();
}