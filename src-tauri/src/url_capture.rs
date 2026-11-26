// 跨平台 URL 直采引擎：纯 Rust（reqwest + rustls / serde_json / url / std），
// 桌面端与未来 Android APK 共用。所有外网请求都经过严格 SSRF 校验与手工跳转。
use crate::storage::{AppState, ArticleCapture, CaptureJobStart, CaptureResult, TranscriptCue, VideoCapture};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};
use url::Url;

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REDIRECT_HOPS: usize = 5;
const MAX_CUES: usize = 5000;
const MAX_CUE_CHARS: usize = 500;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// B 站官方 WBI 混淆置换表（64 位）。
const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35,
    27, 43, 5, 49, 33, 9, 42, 19, 29, 28, 14, 39, 12, 38, 41, 13,
    37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4,
    22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

// ---------------------------------------------------------------------------
// 抓取配置：生产路径永久强制 allow_loopback = false；测试隔离仅在本模块内可见。
// ---------------------------------------------------------------------------

struct FetchConfig {
    /// 测试专用：把实际连接重写到本地 mock server（逻辑 URL 仍按生产规则校验）。
    transport_origin: Option<Url>,
    /// 测试专用：仅放行 transport_origin 自身的回环地址；生产恒为 false。
    allow_loopback: bool,
    bilibili_api: Url,
    youtube_api: Url,
}

impl FetchConfig {
    fn production() -> Self {
        Self {
            transport_origin: None,
            allow_loopback: false,
            bilibili_api: Url::parse("https://api.bilibili.com").expect("constant bilibili api url"),
            youtube_api: Url::parse("https://www.youtube.com").expect("constant youtube api url"),
        }
    }

    #[cfg(test)]
    fn for_test(mock_origin: Url) -> Self {
        Self {
            transport_origin: Some(mock_origin),
            allow_loopback: true,
            bilibili_api: Url::parse("https://api.bilibili.com").expect("constant bilibili api url"),
            youtube_api: Url::parse("https://www.youtube.com").expect("constant youtube api url"),
        }
    }
}

/// 把逻辑 URL 的 scheme/host/port 替换为测试 transport origin，保留 path/query。
/// 生产环境 transport_origin 为 None，原样返回。
fn transport_url(logical: &Url, config: &FetchConfig) -> Url {
    let Some(origin) = &config.transport_origin else { return logical.clone(); };
    let mut actual = origin.clone();
    actual.set_path(logical.path());
    actual.set_query(logical.query());
    actual.set_fragment(None);
    actual
}

// ---------------------------------------------------------------------------
// 链接提取与 URL 形状校验
// ---------------------------------------------------------------------------

fn validate_url_shape(url: &Url) -> Result<(), String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("只支持 http:// 或 https:// 链接".into());
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err("链接缺少主机名".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("链接不能包含账号或密码".into());
    }
    Ok(())
}

/// 从纯链接或 App 分享文本中提取首个 http/https 链接，并清理末尾中英文标点。
pub(crate) fn extract_http_url(raw: &str) -> Result<Url, String> {
    let lower = raw.to_ascii_lowercase();
    let start = ["https://", "http://"].iter()
        .filter_map(|scheme| lower.find(scheme)).min()
        .ok_or_else(|| "没有找到 http:// 或 https:// 链接".to_string())?;
    let rest = &raw[start..];
    let mut end = rest.len();
    for (index, ch) in rest.char_indices() {
        if ch.is_whitespace() || matches!(ch,
            '"' | '\'' | '>' | '）' | '】' | '、' | '。' | '，' | '；' | '！' | '？'
            | '」' | '』' | '》' | '〉' | '「' | '『' | '《' | '〈' | '【' | '〔'
            | '］' | '｝' | '…' | '·' | '｜')
        {
            end = index;
            break;
        }
    }
    let token = rest[..end].trim_end_matches(&[')', ']', '}', '.', ',', ';', ':', '!', '?', '"', '\''][..]);
    let parsed = Url::parse(token).map_err(|_| "链接格式无效".to_string())?;
    validate_url_shape(&parsed)?;
    Ok(parsed)
}

// ---------------------------------------------------------------------------
// 严格 SSRF 地址校验
// ---------------------------------------------------------------------------

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xc0) == 0x40)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        || (o[0] == 192 && o[1] == 88 && o[2] == 99)
        || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
        || o[0] >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let seg = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || (seg[0] & 0xfe00) == 0xfc00
        || (seg[0] & 0xffc0) == 0xfe80
        || (seg[0] & 0xffc0) == 0xfec0
        || (seg[0] & 0xff00) == 0xff00
        || (seg[0] == 0x2001 && seg[1] == 0x0db8)
        || seg[0] == 0x0100)
}

/// 判断 IP 是否为公网地址；IPv4-mapped IPv6 拆出 IPv4 递归判断。
pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() { return is_public_v4(mapped); }
            if let Some(compatible) = v6.to_ipv4() { return is_public_v4(compatible); }
            is_public_v6(v6)
        }
    }
}

/// 主机名层面的 SSRF 校验：拒绝 localhost / *.localhost / *.local 与非公网 IP 字面量。
pub(crate) fn ensure_public_host(host: &str) -> Result<(), String> {
    let lower = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let lower = lower.trim_start_matches('[').trim_end_matches(']');
    if lower.is_empty() { return Err("链接缺少主机名".into()); }
    if lower == "localhost" || lower.ends_with(".localhost") {
        return Err("拒绝访问本机地址（localhost）".into());
    }
    if lower.ends_with(".local") {
        return Err("拒绝访问本地网络地址（.local）".into());
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err(format!("拒绝访问非公网地址 {ip}"));
        }
    }
    Ok(())
}

fn is_bilibili_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "bilibili.com" || h.ends_with(".bilibili.com")
}

fn host_in_list(host: &str, list: &[&str]) -> bool {
    let h = host.to_ascii_lowercase();
    list.iter().any(|entry| h == *entry || h.ends_with(&format!(".{entry}")))
}

/// 在阻塞线程解析 (host, port)，校验每一个 IP 都为公网（测试 transport 除外），
/// 返回已验证地址供 reqwest resolve_to_addrs 钉住，防 DNS rebinding。
async fn resolve_verified(host: &str, port: u16, exempt_loopback: bool) -> Result<Vec<SocketAddr>, String> {
    let host_owned = host.to_owned();
    let resolved = tauri::async_runtime::spawn_blocking(move || {
        use std::net::ToSocketAddrs;
        (host_owned.as_str(), port).to_socket_addrs().map(|iter| iter.collect::<Vec<SocketAddr>>())
    }).await.map_err(|_| "DNS 解析任务失败".to_string())?
        .map_err(|error| format!("无法解析主机 {host}：{error}"))?;
    if resolved.is_empty() { return Err(format!("主机 {host} 没有解析结果")); }
    for addr in &resolved {
        if !is_public_ip(addr.ip()) && !exempt_loopback {
            return Err(format!("拒绝访问非公网地址 {}（{host}）", addr.ip()));
        }
    }
    Ok(resolved)
}

// ---------------------------------------------------------------------------
// 受限 HTTP 抓取：关闭自动重定向、手工跟踪、字节上限、总超时
// ---------------------------------------------------------------------------

struct HopResponse {
    status: u16,
    location: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

struct Fetched {
    url: Url,
    content_type: Option<String>,
    body: Vec<u8>,
}

/// 单次请求（不跟随重定向）。逻辑 URL 先做严格 SSRF 校验，再经 transport 发起。
async fn fetch_once(
    logical: &Url,
    config: &FetchConfig,
    whitelist: Option<&[&str]>,
    method: reqwest::Method,
    json_body: Option<&Value>,
    cookies: Option<&MemoryCookies>,
) -> Result<HopResponse, String> {
    validate_url_shape(logical)?;
    let host = logical.host_str().unwrap_or_default().to_owned();
    ensure_public_host(&host)?;
    if let Some(list) = whitelist {
        if !host_in_list(&host, list) {
            return Err(format!("跳转目标 {host} 不在允许的域名范围内"));
        }
    }
    let actual = transport_url(logical, config);
    let actual_host = actual.host_str().unwrap_or_default().to_owned();
    let port = actual.port_or_known_default().ok_or("链接缺少端口")?;
    let exempt = config.allow_loopback && config.transport_origin.is_some();
    let addrs = resolve_verified(&actual_host, port, exempt).await?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&actual_host, &addrs)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| format!("无法初始化网络客户端：{error}"))?;
    let mut request = client.request(method, actual);
    if let Some(cookies) = cookies {
        if is_bilibili_host(&host) {
            request = request.header(reqwest::header::COOKIE, cookies.header.as_str());
        }
    }
    if let Some(body) = json_body { request = request.json(body); }
    let mut response = request.send().await.map_err(|error| format!("网络请求失败：{error}"))?;
    let status = response.status().as_u16();
    let location = response.headers().get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok()).map(str::to_owned);
    let content_type = response.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok()).map(str::to_owned);
    if response.content_length().is_some_and(|len| len > MAX_RESPONSE_BYTES as u64) {
        return Err("响应超过 4 MB 上限".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| format!("读取响应失败：{error}"))? {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err("响应超过 4 MB 上限".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(HopResponse { status, location, content_type, body })
}

/// 手工跟踪最多 5 跳重定向，每一跳重新做 SSRF 与域名白名单校验。
async fn fetch_bytes(
    logical: &Url,
    config: &FetchConfig,
    whitelist: Option<&[&str]>,
    method: reqwest::Method,
    json_body: Option<&Value>,
    cookies: Option<&MemoryCookies>,
) -> Result<Fetched, String> {
    let mut current = logical.clone();
    for hop in 0..=MAX_REDIRECT_HOPS {
        let response = fetch_once(&current, config, whitelist, method.clone(), json_body, cookies).await?;
        if (300..400).contains(&response.status) {
            if hop == MAX_REDIRECT_HOPS { return Err("重定向次数过多，已停止跟踪".into()); }
            let location = response.location.ok_or("重定向响应缺少 Location")?;
            current = current.join(&location).map_err(|_| "重定向地址无效")?;
            continue;
        }
        if !(200..300).contains(&response.status) {
            return Err(format!("服务器返回 HTTP {}", response.status));
        }
        return Ok(Fetched { url: current, content_type: response.content_type, body: response.body });
    }
    Err("重定向次数过多，已停止跟踪".into())
}

async fn fetch_json(
    url: &Url,
    config: &FetchConfig,
    whitelist: Option<&[&str]>,
    method: reqwest::Method,
    json_body: Option<&Value>,
    cookies: Option<&MemoryCookies>,
) -> Result<Value, String> {
    let fetched = fetch_bytes(url, config, whitelist, method, json_body, cookies).await?;
    serde_json::from_slice(&fetched.body).map_err(|error| format!("接口返回的 JSON 无效：{error}"))
}

// ---------------------------------------------------------------------------
// 纯 Rust RFC-1321 MD5
// ---------------------------------------------------------------------------

fn md5_hex(input: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
        5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let mut message = input.to_vec();
    let bit_len = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 { message.push(0); }
    message.extend_from_slice(&bit_len.to_le_bytes());

    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;
    for chunk in message.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            m[index] = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let temp = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(temp.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = String::with_capacity(32);
    for value in [a0, b0, c0, d0] {
        for byte in value.to_le_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// B 站 WBI 签名
// ---------------------------------------------------------------------------

fn url_file_stem(raw: &str) -> Result<String, String> {
    let parsed = Url::parse(raw).map_err(|_| "WBI 密钥地址无效".to_string())?;
    let name = parsed.path_segments().and_then(|mut parts| parts.next_back())
        .ok_or_else(|| "WBI 密钥地址缺少文件名".to_string())?;
    let stem = name.split('.').next().unwrap_or_default();
    if stem.len() != 32 || !stem.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("WBI 密钥文件名格式无效".into());
    }
    Ok(stem.to_owned())
}

fn wbi_mixin_key(img_url: &str, sub_url: &str) -> Result<String, String> {
    let raw = format!("{}{}", url_file_stem(img_url)?, url_file_stem(sub_url)?);
    let bytes = raw.as_bytes();
    Ok(MIXIN_KEY_ENC_TAB.iter().take(32).map(|&index| bytes[index] as char).collect())
}

fn encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 按键升序 URL 编码拼接参数，尾部拼 mixin_key 做 MD5 得到 w_rid。
fn wbi_sign(params: &[(String, String)], mixin_key: &str) -> (String, String) {
    let mut sorted = params.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let query = sorted.iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>().join("&");
    let w_rid = md5_hex(format!("{query}{mixin_key}").as_bytes());
    (query, w_rid)
}

// ---------------------------------------------------------------------------
// UTC 时间格式化（配合 storage.rs 的 savedAt 校验）
// ---------------------------------------------------------------------------

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn format_utc_iso(secs: u64) -> String {
    let (year, month, day) = civil_from_days((secs / 86400) as i64);
    let rem = secs % 86400;
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

pub(crate) fn format_utc_iso_now() -> String {
    format_utc_iso(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs())
}

// ---------------------------------------------------------------------------
// 内存凭据（绝不落盘）
// ---------------------------------------------------------------------------

struct MemoryCookies {
    header: String,
}

fn parse_bilibili_cookies(content: &str) -> Result<MemoryCookies, String> {
    crate::storyboard::validate_bilibili_cookies(content)?;
    let mut pairs = Vec::new();
    for line in content.lines() {
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() || (line.starts_with('#') && !line.starts_with("#HttpOnly_")) { continue; }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() == 7 && !fields[5].is_empty() && !fields[6].is_empty() {
            if fields[5].bytes().chain(fields[6].bytes()).any(|byte| byte < 0x20 || byte == 0x7f) {
                return Err("cookies.txt 包含非法控制字符。".into());
            }
            pairs.push(format!("{}={}", fields[5], fields[6]));
        }
    }
    Ok(MemoryCookies { header: pairs.join("; ") })
}

// ---------------------------------------------------------------------------
// 字幕解析与轨道优先级
// ---------------------------------------------------------------------------

fn language_rank(lang: &str) -> u32 {
    let lower = lang.to_ascii_lowercase();
    if lower.starts_with("zh") { 0 } else if lower.starts_with("en") { 1 } else { 2 }
}

fn normalize_cues(items: impl IntoIterator<Item = (f64, String)>) -> Result<Vec<TranscriptCue>, String> {
    let mut cues = Vec::new();
    for (at, text) in items {
        let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_CUE_CHARS).collect();
        if text.is_empty() { continue; }
        let at_seconds = if at.is_finite() && at > 0.0 { (at.round() as u64).min(864_000) as u32 } else { 0 };
        cues.push(TranscriptCue { at_seconds, text });
    }
    if cues.len() > MAX_CUES { return Err("字幕超过 5000 条，暂不能完整保存".into()); }
    Ok(cues)
}

fn parse_bilibili_cues(value: &Value) -> Result<Vec<TranscriptCue>, String> {
    let body = value.get("body").and_then(Value::as_array).ok_or("字幕数据格式无效")?;
    normalize_cues(body.iter().map(|cue| (
        cue.get("from").and_then(Value::as_f64).unwrap_or(0.0),
        cue.get("content").and_then(Value::as_str).unwrap_or("").to_owned(),
    )))
}

fn parse_youtube_json3(value: &Value) -> Result<Vec<TranscriptCue>, String> {
    let empty = Vec::new();
    let events = value.get("events").and_then(Value::as_array).unwrap_or(&empty);
    normalize_cues(events.iter().map(|event| (
        event.get("tStartMs").and_then(Value::as_f64).unwrap_or(0.0) / 1000.0,
        event.get("segs").and_then(Value::as_array).map(|segs| {
            segs.iter().map(|seg| seg.get("utf8").and_then(Value::as_str).unwrap_or("")).collect::<String>()
        }).unwrap_or_default(),
    )))
}

fn parse_vtt_timestamp(stamp: &str) -> Option<f64> {
    let parts: Vec<&str> = stamp.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 { return None; }
    let mut total = 0.0;
    for part in parts {
        let value: f64 = part.trim().parse().ok()?;
        if !value.is_finite() || value < 0.0 { return None; }
        total = total * 60.0 + value;
    }
    Some(total)
}

fn parse_vtt(text: &str) -> Result<Vec<TranscriptCue>, String> {
    let text = text.replace('\r', "");
    let mut items = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        let Some(index) = lines.iter().position(|line| line.contains("-->")) else { continue; };
        let start = lines[index].split("-->").next().unwrap_or("").trim();
        let Some(seconds) = parse_vtt_timestamp(start) else { continue; };
        let cue_text = strip_tags(&lines[index + 1..].join(" "));
        items.push((seconds, cue_text));
    }
    normalize_cues(items)
}

fn bilibili_track_rank(track: &Value) -> u32 {
    let lan = track.get("lan").and_then(Value::as_str).unwrap_or("");
    let ai = track.get("ai_type").and_then(Value::as_u64).unwrap_or(0) > 0;
    language_rank(lan) * 10 + u32::from(ai)
}

fn youtube_track_rank(track: &Value) -> u32 {
    let lang = track.get("languageCode").and_then(Value::as_str).unwrap_or("");
    let asr = track.get("kind").and_then(Value::as_str) == Some("asr");
    language_rank(lang) * 10 + u32::from(asr)
}

fn verified_bilibili_subtitle_url(raw: &str, cid: u64) -> Option<Url> {
    let absolute = if raw.starts_with("//") { format!("https:{raw}") } else { raw.to_owned() };
    let url = Url::parse(&absolute).ok()?;
    if url.scheme() != "https" { return None; }
    let host = url.host_str()?;
    if !(host == "hdslb.com" || host.ends_with(".hdslb.com")) { return None; }
    if !url.path().contains(&cid.to_string()) { return None; }
    Some(url)
}

// ---------------------------------------------------------------------------
// 路由判定
// ---------------------------------------------------------------------------

enum Route {
    BilibiliVideo { bvid: String, page: u32 },
    BilibiliShortLink,
    YouTube { video_id: String },
    Article,
}

fn bilibili_bvid_from_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/video/")?;
    let segment = rest.split('/').next()?.trim();
    if segment.len() < 3 || !segment[..2].eq_ignore_ascii_case("BV") { return None; }
    if !segment[2..].bytes().all(|byte| byte.is_ascii_alphanumeric()) { return None; }
    Some(segment.to_owned())
}

fn bilibili_page_param(url: &Url) -> u32 {
    url.query_pairs().find(|(key, _)| key == "p")
        .and_then(|(_, value)| value.parse::<u32>().ok())
        .filter(|page| *page >= 1).unwrap_or(1)
}

fn youtube_video_id(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    let path = url.path();
    let candidate = if host == "youtu.be" || host.ends_with(".youtu.be") {
        path.trim_start_matches('/').split('/').next().map(str::to_owned)
    } else if path == "/watch" {
        url.query_pairs().find(|(key, _)| key == "v").map(|(_, value)| value.into_owned())
    } else {
        path.strip_prefix("/shorts/").or_else(|| path.strip_prefix("/live/")).or_else(|| path.strip_prefix("/embed/"))
            .and_then(|rest| rest.split('/').next()).map(str::to_owned)
    };
    let id = candidate?;
    if id.len() == 11 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') {
        Some(id)
    } else {
        None
    }
}

fn route_url(url: &Url) -> Route {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host == "b23.tv" || host.ends_with(".b23.tv") { return Route::BilibiliShortLink; }
    if is_bilibili_host(&host) {
        if let Some(bvid) = bilibili_bvid_from_path(url.path()) {
            return Route::BilibiliVideo { bvid, page: bilibili_page_param(url) };
        }
        return Route::Article;
    }
    if host == "youtube.com" || host.ends_with(".youtube.com") || host == "youtu.be" || host.ends_with(".youtu.be") {
        if let Some(video_id) = youtube_video_id(url) { return Route::YouTube { video_id }; }
        return Route::Article;
    }
    Route::Article
}

fn provisional_title(url: &Url, route: &Route) -> String {
    match route {
        Route::BilibiliVideo { bvid, .. } => format!("B 站视频 {bvid}"),
        Route::BilibiliShortLink => "B 站视频短链接".to_owned(),
        Route::YouTube { video_id } => format!("YouTube 视频 {video_id}"),
        Route::Article => format!("{} 网页", url.host_str().unwrap_or("未知")),
    }
}

// ---------------------------------------------------------------------------
// 路径 A：Bilibili 视频
// ---------------------------------------------------------------------------

async fn resolve_short_link(url: &Url, config: &FetchConfig) -> Result<(Url, String, u32), String> {
    const WHITELIST: &[&str] = &["b23.tv", "bilibili.com"];
    let mut current = url.clone();
    for hop in 0..=MAX_REDIRECT_HOPS {
        let host = current.host_str().unwrap_or_default();
        if is_bilibili_host(host) {
            if let Some(bvid) = bilibili_bvid_from_path(current.path()) {
                let page = bilibili_page_param(&current);
                return Ok((current, bvid, page));
            }
        }
        let response = fetch_once(&current, config, Some(WHITELIST), reqwest::Method::GET, None, None).await?;
        if !(300..400).contains(&response.status) {
            return Err("短链接没有指向 B 站视频页面".into());
        }
        if hop == MAX_REDIRECT_HOPS { return Err("重定向次数过多，已停止跟踪".into()); }
        let location = response.location.ok_or("短链接跳转缺少 Location")?;
        current = current.join(&location).map_err(|_| "短链接跳转地址无效")?;
    }
    Err("重定向次数过多，已停止跟踪".into())
}

async fn capture_bilibili(
    bvid: &str,
    page: u32,
    cookies: Option<&MemoryCookies>,
    config: &FetchConfig,
) -> Result<VideoCapture, String> {
    const WHITELIST: &[&str] = &["bilibili.com"];
    let api = &config.bilibili_api;
    let view_url = api.join(&format!("x/web-interface/view?bvid={}", encode_component(bvid)))
        .map_err(|_| "B 站视频接口地址无效")?;
    let view = fetch_json(&view_url, config, Some(WHITELIST), reqwest::Method::GET, None, cookies).await?;
    if view.get("code").and_then(Value::as_i64) != Some(0) {
        return Err(format!("B 站视频信息接口返回错误：{}", view.get("message").and_then(Value::as_str).unwrap_or("未知错误")));
    }
    let data = view.get("data").ok_or("B 站视频信息缺失")?;
    if !data.get("bvid").and_then(Value::as_str).unwrap_or_default().eq_ignore_ascii_case(bvid) {
        return Err("B 站返回的视频 BV 号不匹配，已拒绝保存".into());
    }
    let title = data.get("title").and_then(Value::as_str).unwrap_or("").trim();
    if title.is_empty() { return Err("B 站视频标题为空".into()); }
    let pages = data.get("pages").and_then(Value::as_array).ok_or("B 站视频分 P 信息缺失")?;
    let page_info = pages.get((page as usize).saturating_sub(1)).ok_or("指定的分 P 不存在")?;
    let cid = page_info.get("cid").and_then(Value::as_u64).filter(|value| *value > 0)
        .ok_or("未找到当前分 P 的 CID")?;
    let duration = page_info.get("duration").and_then(Value::as_u64)
        .or_else(|| data.get("duration").and_then(Value::as_u64))
        .filter(|value| *value <= 864_000).map(|value| value as u32);

    let nav_url = api.join("x/web-interface/nav").map_err(|_| "B 站导航接口地址无效")?;
    let nav = fetch_json(&nav_url, config, Some(WHITELIST), reqwest::Method::GET, None, cookies).await?;
    let wbi = nav.get("data").and_then(|data| data.get("wbi_img")).ok_or("B 站 WBI 密钥缺失")?;
    let img_url = wbi.get("img_url").and_then(Value::as_str).ok_or("B 站 WBI img_url 缺失")?;
    let sub_url = wbi.get("sub_url").and_then(Value::as_str).ok_or("B 站 WBI sub_url 缺失")?;
    let mixin_key = wbi_mixin_key(img_url, sub_url)?;
    let wts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let params = [
        ("bvid".to_string(), bvid.to_string()),
        ("cid".to_string(), cid.to_string()),
        ("wts".to_string(), wts.to_string()),
    ];
    let (query, w_rid) = wbi_sign(&params, &mixin_key);
    let player_url = api.join(&format!("x/player/wbi/v2?{query}&w_rid={w_rid}"))
        .map_err(|_| "B 站播放器接口地址无效")?;
    let player = fetch_json(&player_url, config, Some(WHITELIST), reqwest::Method::GET, None, cookies).await?;
    if player.get("code").and_then(Value::as_i64) != Some(0) {
        return Err("B 站字幕接口暂不可用，请确认已登录后重试".into());
    }
    let player_data = player.get("data").ok_or("B 站播放器数据缺失")?;
    if !player_data.get("bvid").and_then(Value::as_str).unwrap_or_default().eq_ignore_ascii_case(bvid)
        || player_data.get("cid").and_then(Value::as_u64) != Some(cid)
    {
        return Err("播放器返回的视频身份不匹配，已拒绝保存字幕".into());
    }
    let subtitles = player_data.get("subtitle").and_then(|subtitle| subtitle.get("subtitles"))
        .and_then(Value::as_array).cloned().unwrap_or_default();
    if subtitles.is_empty() {
        return Err(if player_data.get("need_login_subtitle").and_then(Value::as_bool).unwrap_or(false) {
            "该视频需要登录后才能读取字幕，请粘贴仅含 B 站登录信息的 Netscape cookies.txt 后重试。".into()
        } else {
            "该视频暂无可用字幕轨道。".into()
        });
    }
    let verified: Vec<(Url, &Value)> = subtitles.iter().filter_map(|track| {
        let raw = track.get("subtitle_url").and_then(Value::as_str)?;
        verified_bilibili_subtitle_url(raw, cid).map(|url| (url, track))
    }).collect();
    if verified.is_empty() {
        return Err("字幕轨道未包含当前分 P 的 CID，已拒绝保存不匹配的字幕".into());
    }
    let (track_url, track) = verified.iter().min_by_key(|(_, track)| bilibili_track_rank(track))
        .expect("verified non-empty");
    let subtitle = fetch_json(track_url, config, Some(&["hdslb.com"]), reqwest::Method::GET, None, None).await?;
    let cues = parse_bilibili_cues(&subtitle)?;
    if cues.is_empty() { return Err("该视频暂无可用字幕轨道。".into()); }
    let language = track.get("lan_doc").and_then(Value::as_str).filter(|value| !value.is_empty())
        .or_else(|| track.get("lan").and_then(Value::as_str)).unwrap_or("").to_owned();
    let mut source = Url::parse(&format!("https://www.bilibili.com/video/{bvid}")).expect("constant bilibili video url");
    if page > 1 { source.query_pairs_mut().append_pair("p", &page.to_string()); }
    Ok(VideoCapture {
        url: source.to_string(),
        title: title.chars().take(300).collect(),
        captured_at: format_utc_iso_now(),
        duration_seconds: duration,
        language,
        cues,
        fault_reason: String::new(),
        bvid: Some(bvid.to_string()),
        cid: Some(cid),
        track_url: Some(track_url.to_string()),
    })
}

// ---------------------------------------------------------------------------
// 路径 B：YouTube 视频
// ---------------------------------------------------------------------------

fn youtube_client(name: &str) -> Value {
    if name == "ANDROID" {
        json!({"clientName": "ANDROID", "clientVersion": "19.09.37", "androidSdkVersion": 30, "hl": "en"})
    } else {
        json!({"clientName": "WEB", "clientVersion": "2.20240101.00.00", "hl": "en"})
    }
}

fn set_query_param(url: &mut Url, key: &str, value: &str) {
    let mut pairs: Vec<(String, String)> = url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    pairs.retain(|(k, _)| k != key);
    pairs.push((key.to_owned(), value.to_owned()));
    url.query_pairs_mut().clear().extend_pairs(pairs);
}

fn extract_json_object_after(html: &str, marker: &str) -> Option<Value> {
    let start = html.find(marker)? + marker.len();
    let open = html[start..].find('{')? + start;
    let bytes = html.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut index = open;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped { escaped = false; }
            else if byte == b'\\' { escaped = true; }
            else if byte == b'"' { in_string = false; }
        } else if byte == b'"' { in_string = true; }
        else if byte == b'{' { depth += 1; }
        else if byte == b'}' {
            depth -= 1;
            if depth == 0 { return serde_json::from_str(&html[open..=index]).ok(); }
        }
        index += 1;
    }
    None
}

async fn fetch_watch_player(video_id: &str, config: &FetchConfig) -> Result<Value, String> {
    let watch = config.youtube_api.join(&format!("watch?v={video_id}"))
        .map_err(|_| "YouTube 页面地址无效")?;
    let fetched = fetch_bytes(&watch, config, Some(&["youtube.com"]), reqwest::Method::GET, None, None).await?;
    let html = String::from_utf8_lossy(&fetched.body);
    extract_json_object_after(&html, "ytInitialPlayerResponse")
        .ok_or_else(|| "无法从 YouTube 页面提取播放器信息".to_string())
}

async fn capture_youtube(video_id: &str, config: &FetchConfig) -> Result<VideoCapture, String> {
    const WHITELIST: &[&str] = &["youtube.com"];
    let player_url = config.youtube_api.join("youtubei/v1/player").map_err(|_| "YouTube 接口地址无效")?;
    let mut player = None;
    for client in ["WEB", "ANDROID"] {
        let body = json!({"context": {"client": youtube_client(client)}, "videoId": video_id});
        if let Ok(value) = fetch_json(&player_url, config, Some(WHITELIST), reqwest::Method::POST, Some(&body), None).await {
            if value.get("videoDetails").is_some() { player = Some(value); break; }
        }
    }
    let player = match player {
        Some(value) => value,
        None => fetch_watch_player(video_id, config).await?,
    };
    let details = player.get("videoDetails").ok_or("YouTube 视频信息缺失")?;
    if details.get("videoId").and_then(Value::as_str) != Some(video_id) {
        return Err("YouTube 返回的视频 ID 不匹配，已拒绝保存".into());
    }
    let title = details.get("title").and_then(Value::as_str).unwrap_or("").trim();
    if title.is_empty() { return Err("YouTube 视频标题为空".into()); }
    let duration = details.get("lengthSeconds").and_then(Value::as_str)
        .and_then(|value| value.parse::<u32>().ok()).filter(|value| *value <= 864_000);
    let tracks = player.get("captions")
        .and_then(|captions| captions.get("playerCaptionsTracklistRenderer"))
        .and_then(|renderer| renderer.get("captionTracks"))
        .and_then(Value::as_array).cloned().unwrap_or_default();
    if tracks.is_empty() { return Err("该视频没有可读取的字幕轨道。".into()); }
    let track = tracks.iter().min_by_key(|track| youtube_track_rank(track)).expect("tracks non-empty");
    let base = track.get("baseUrl").and_then(Value::as_str).ok_or("YouTube 字幕地址缺失")?;
    let base_url = Url::parse(base).map_err(|_| "YouTube 字幕地址无效")?;
    validate_url_shape(&base_url)?;
    let host = base_url.host_str().unwrap_or_default();
    if !(host == "youtube.com" || host.ends_with(".youtube.com")) {
        return Err("YouTube 字幕来源域名不受信任，已拒绝保存".into());
    }
    let mut json3_url = base_url.clone();
    set_query_param(&mut json3_url, "fmt", "json3");
    let mut cues = Vec::new();
    if let Ok(value) = fetch_json(&json3_url, config, Some(WHITELIST), reqwest::Method::GET, None, None).await {
        cues = parse_youtube_json3(&value)?;
    }
    if cues.is_empty() {
        let mut vtt_url = base_url.clone();
        set_query_param(&mut vtt_url, "fmt", "vtt");
        let fetched = fetch_bytes(&vtt_url, config, Some(WHITELIST), reqwest::Method::GET, None, None).await?;
        cues = parse_vtt(&String::from_utf8_lossy(&fetched.body))?;
    }
    if cues.is_empty() { return Err("该视频没有可读取的字幕轨道。".into()); }
    let mut language = if let Some(simple) = track.get("name").and_then(|name| name.get("simpleText")).and_then(Value::as_str) {
        simple.to_owned()
    } else if let Some(runs) = track.get("name").and_then(|name| name.get("runs")).and_then(Value::as_array) {
        runs.iter().filter_map(|run| run.get("text").and_then(Value::as_str)).collect::<String>()
    } else {
        String::new()
    };
    if language.is_empty() {
        language = track.get("languageCode").and_then(Value::as_str).unwrap_or("").to_owned();
    }
    Ok(VideoCapture {
        url: format!("https://www.youtube.com/watch?v={video_id}"),
        title: title.chars().take(300).collect(),
        captured_at: format_utc_iso_now(),
        duration_seconds: duration,
        language,
        cues,
        fault_reason: String::new(),
        bvid: None,
        cid: None,
        track_url: Some(base_url.to_string()),
    })
}

// ---------------------------------------------------------------------------
// 路径 C：网页文章 Fast Path（纯 Rust 安全 HTML 抽取）
// ---------------------------------------------------------------------------

struct Tag {
    name: String,
    end: usize,
    closing: bool,
    self_closing: bool,
    attrs: String,
}

fn parse_tag(html: &str, pos: usize) -> Option<Tag> {
    let bytes = html.as_bytes();
    if bytes.get(pos) != Some(&b'<') { return None; }
    let mut index = pos + 1;
    let closing = bytes.get(index) == Some(&b'/');
    if closing { index += 1; }
    let name_start = index;
    while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'-' | b'_' | b':')) {
        index += 1;
    }
    if index == name_start { return None; }
    let name = html[name_start..index].to_ascii_lowercase();
    let attrs_start = index;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(q) = quote {
            if byte == q { quote = None; }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' {
            break;
        }
        index += 1;
    }
    if index >= bytes.len() { return None; }
    let attrs = html[attrs_start..index].to_owned();
    let self_closing = attrs.trim_end().ends_with('/');
    Some(Tag { name, end: index + 1, closing, self_closing, attrs })
}

/// 返回匹配闭合标签之后的位置；未闭合时返回 from（仅移除开标签本身）。
fn element_end(html: &str, from: usize, name: &str) -> usize {
    let mut depth = 1usize;
    let mut index = from;
    while index < html.len() {
        let Some(rel) = html[index..].find('<') else { break; };
        let pos = index + rel;
        match parse_tag(html, pos) {
            Some(tag) => {
                if tag.name == name {
                    if tag.closing {
                        depth -= 1;
                        if depth == 0 { return tag.end; }
                    } else if !tag.self_closing {
                        depth += 1;
                    }
                }
                index = tag.end;
            }
            None => index = pos + 1,
        }
    }
    from
}

fn remove_noise(html: &str) -> String {
    const NOISE: [&str; 8] = ["script", "style", "noscript", "svg", "nav", "footer", "aside", "form"];
    let mut out = String::with_capacity(html.len());
    let mut index = 0;
    while index < html.len() {
        let Some(rel) = html[index..].find('<') else { out.push_str(&html[index..]); break; };
        let pos = index + rel;
        out.push_str(&html[index..pos]);
        if html[pos..].starts_with("<!--") {
            index = match html[pos + 4..].find("-->") {
                Some(r) => pos + 4 + r + 3,
                None => html.len(),
            };
            continue;
        }
        match parse_tag(html, pos) {
            Some(tag) if !tag.closing && NOISE.contains(&tag.name.as_str()) => {
                index = element_end(html, tag.end, &tag.name);
            }
            Some(tag) => { out.push_str(&html[pos..tag.end]); index = tag.end; }
            None => { out.push('<'); index = pos + 1; }
        }
    }
    out
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let bytes = attrs.as_bytes();
    let first = name.as_bytes()[0];
    for (index, _) in attrs.char_indices() {
        if !bytes[index].eq_ignore_ascii_case(&first) { continue; }
        let boundary = index == 0 || bytes[index - 1].is_ascii_whitespace();
        if !boundary || attrs.len() - index <= name.len() { continue; }
        if !bytes[index..index + name.len()].eq_ignore_ascii_case(name.as_bytes()) { continue; }
        if bytes[index + name.len()] != b'=' { continue; }
        let after = attrs[index + name.len() + 1..].trim_start();
        let value = if let Some(rest) = after.strip_prefix('"') {
            rest.split('"').next().unwrap_or("")
        } else if let Some(rest) = after.strip_prefix('\'') {
            rest.split('\'').next().unwrap_or("")
        } else {
            after.split(|ch: char| ch.is_whitespace() || ch == '>').next().unwrap_or("")
        };
        return Some(value.to_owned());
    }
    None
}

fn attr_token(attrs: &str, name: &str, token: &str) -> bool {
    attr_value(attrs, name).is_some_and(|value| value.split_whitespace().any(|part| part.eq_ignore_ascii_case(token)))
}

fn attr_eq(attrs: &str, name: &str, expected: &str) -> bool {
    attr_value(attrs, name).is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn container_inner(html: &str) -> Option<&str> {
    let mut best: Option<(u8, usize, usize)> = None;
    let mut index = 0;
    while index < html.len() {
        let Some(rel) = html[index..].find('<') else { break; };
        let pos = index + rel;
        let Some(tag) = parse_tag(html, pos) else { index = pos + 1; continue; };
        if !tag.closing && !tag.self_closing {
            let priority = if attr_token(&tag.attrs, "class", "opus-module-content") { Some(0u8) }
                else if attr_eq(&tag.attrs, "id", "article-content") || attr_token(&tag.attrs, "class", "article-content") { Some(1) }
                else if tag.name == "article" { Some(2) }
                else if tag.name == "main" { Some(3) }
                else if attr_eq(&tag.attrs, "role", "main") { Some(4) }
                else { None };
            if let Some(priority) = priority {
                if best.as_ref().is_none_or(|(best_priority, _, _)| priority < *best_priority) {
                    let end = element_end(html, tag.end, &tag.name);
                    best = Some((priority, tag.end, end));
                }
            }
        }
        index = tag.end;
    }
    best.map(|(_, start, end)| &html[start..end])
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos..];
        if let Some(semi) = after.find(';') {
            if semi <= 12 {
                let entity = &after[1..semi];
                let decoded = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => {
                        if let Some(hex) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
                            u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                        } else {
                            entity.strip_prefix('#').and_then(|num| num.parse::<u32>().ok()).and_then(char::from_u32)
                        }
                    }
                };
                if let Some(ch) = decoded {
                    out.push(ch);
                    rest = &after[semi + 1..];
                    continue;
                }
            }
        }
        out.push('&');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut index = 0;
    while index < html.len() {
        let Some(rel) = html[index..].find('<') else { out.push_str(&html[index..]); break; };
        let pos = index + rel;
        out.push_str(&html[index..pos]);
        match parse_tag(html, pos) {
            Some(tag) => index = tag.end,
            None => { out.push('<'); index = pos + 1; }
        }
    }
    decode_entities(&out)
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_blocks(html: &str) -> Vec<String> {
    const BLOCKS: [&str; 8] = ["h1", "h2", "h3", "h4", "p", "li", "blockquote", "pre"];
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < html.len() {
        let Some(rel) = html[index..].find('<') else { break; };
        let pos = index + rel;
        let Some(tag) = parse_tag(html, pos) else { index = pos + 1; continue; };
        if !tag.closing && !tag.self_closing && BLOCKS.contains(&tag.name.as_str()) {
            let end = element_end(html, tag.end, &tag.name);
            let text = normalize_text(&strip_tags(&html[tag.end..end]));
            if !text.is_empty() { blocks.push(text); }
            index = end;
        } else {
            index = tag.end;
        }
    }
    blocks
}

fn extract_title(html: &str) -> Option<String> {
    let mut og = None;
    let mut title = None;
    let mut index = 0;
    while index < html.len() {
        let Some(rel) = html[index..].find('<') else { break; };
        let pos = index + rel;
        let Some(tag) = parse_tag(html, pos) else { index = pos + 1; continue; };
        if !tag.closing {
            if tag.name == "meta" && og.is_none() {
                let prop = attr_value(&tag.attrs, "property").or_else(|| attr_value(&tag.attrs, "name"));
                if prop.is_some_and(|value| value.eq_ignore_ascii_case("og:title")) {
                    if let Some(content) = attr_value(&tag.attrs, "content") {
                        let text = normalize_text(&decode_entities(&content));
                        if !text.is_empty() { og = Some(text); }
                    }
                }
            } else if tag.name == "title" && title.is_none() && !tag.self_closing {
                let end = element_end(html, tag.end, "title");
                let text = normalize_text(&strip_tags(&html[tag.end..end]));
                if !text.is_empty() { title = Some(text); }
                index = end;
                continue;
            }
        }
        index = tag.end;
    }
    og.or(title)
}

fn extract_article_text(html: &str) -> String {
    let cleaned = remove_noise(html);
    let container = container_inner(&cleaned).filter(|inner| !inner.trim().is_empty()).unwrap_or(cleaned.as_str());
    let blocks = extract_blocks(container);
    let text = if blocks.is_empty() { normalize_text(&strip_tags(container)) } else { blocks.join("\n\n") };
    text.trim().chars().take(300_000).collect()
}

async fn capture_article(url: &Url, config: &FetchConfig) -> Result<ArticleCapture, String> {
    let fetched = fetch_bytes(url, config, None, reqwest::Method::GET, None, None).await?;
    if let Some(content_type) = &fetched.content_type {
        let lower = content_type.to_ascii_lowercase();
        if !(lower.starts_with("text/") || lower.contains("html") || lower.contains("xml")) {
            return Err("该链接不是可读取的网页文本（可能为文件下载）。".into());
        }
    }
    let html = String::from_utf8_lossy(&fetched.body);
    let title = extract_title(&html).unwrap_or_else(|| fetched.url.host_str().unwrap_or("网页").to_owned());
    let text = extract_article_text(&html);
    if text.chars().count() < 80 {
        return Err("网页正文不足 80 字，可能需登录或依赖浏览器动态渲染，建议桌面端使用浏览器扩展捕获。".into());
    }
    Ok(ArticleCapture {
        url: fetched.url.to_string(),
        title: title.chars().take(300).collect(),
        text,
        captured_at: format_utc_iso_now(),
    })
}

// ---------------------------------------------------------------------------
// 三路流水线入口与 Tauri 命令
// ---------------------------------------------------------------------------

enum Prepared {
    Article(ArticleCapture),
    Video(VideoCapture),
}

async fn prepare_capture(url: &Url, cookies_text: Option<&str>, config: &FetchConfig) -> Result<Prepared, String> {
    let cookies = match cookies_text.map(str::trim).filter(|content| !content.is_empty()) {
        Some(content) => Some(parse_bilibili_cookies(content)?),
        None => None,
    };
    let route = route_url(url);
    if cookies.is_some() && !matches!(route, Route::BilibiliVideo { .. } | Route::BilibiliShortLink) {
        return Err("cookies.txt 仅适用于 B 站视频链接。".into());
    }
    match route {
        Route::BilibiliVideo { bvid, page } => capture_bilibili(&bvid, page, cookies.as_ref(), config).await.map(Prepared::Video),
        Route::BilibiliShortLink => {
            let (_, bvid, page) = resolve_short_link(url, config).await?;
            capture_bilibili(&bvid, page, cookies.as_ref(), config).await.map(Prepared::Video)
        }
        Route::YouTube { video_id } => capture_youtube(&video_id, config).await.map(Prepared::Video),
        Route::Article => capture_article(url, config).await.map(Prepared::Article),
    }
}

#[tauri::command]
pub async fn capture_from_url(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    cookies_text: Option<String>,
) -> Result<CaptureResult, String> {
    let secrets = cookies_text.as_deref().map(crate::credentials::sensitive_values).unwrap_or_default();
    let parsed = extract_http_url(&url)?;
    let config = FetchConfig::production();
    let route = route_url(&parsed);
    let record_type = if matches!(route, Route::Article) { "article" } else { "video" };
    let job = state.start_capture_job(CaptureJobStart {
        url: parsed.to_string(),
        title: provisional_title(&parsed, &route),
        record_type: record_type.to_owned(),
    })?;
    let _ = app.emit("gearnote:capture-jobs-changed", ());

    let prepared = match prepare_capture(&parsed, cookies_text.as_deref(), &config).await {
        Ok(prepared) => prepared,
        Err(error) => {
            let safe = crate::credentials::redact(&error, &secrets);
            let _ = state.finish_capture_job(&job.id, Some(&safe));
            let _ = app.emit("gearnote:capture-jobs-changed", ());
            return Err(safe);
        }
    };
    let _ = state.set_capture_job_stage(&job.id);
    let _ = app.emit("gearnote:capture-jobs-changed", ());

    let saved = match prepared {
        Prepared::Article(input) => state.capture(input),
        Prepared::Video(input) => state.capture_video(input),
    };
    match saved {
        Ok(result) => {
            let _ = state.finish_capture_job(&job.id, None);
            let _ = app.emit("gearnote:capture-jobs-changed", ());
            if !result.already_saved {
                let _ = app.emit("knowflow:records-changed", &result.record.id);
            }
            Ok(result)
        }
        Err(error) => {
            let safe = crate::credentials::redact(&error, &secrets);
            let _ = state.finish_capture_job(&job.id, Some(&safe));
            let _ = app.emit("gearnote:capture-jobs-changed", ());
            Err(safe)
        }
    }
}

#[cfg(test)]
mod tests;