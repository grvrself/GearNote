use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Manager};
use url::Url;
use uuid::Uuid;

const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_PROMPT_BYTES: usize = 128 * 1024;
const MAX_REPOSITORY_RESPONSE_BYTES: usize = 64 * 1024;
const NETWORK_TIMEOUT: Duration = Duration::from_secs(20);
const USER_AGENT: &str = concat!("GearNote/", env!("CARGO_PKG_VERSION"), " skill-installer");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillManifest {
    #[serde(rename = "type")]
    pub manifest_type: String,
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub homepage: String,
    pub permissions: Vec<String>,
    pub inputs: Vec<SkillInput>,
    pub steps: Vec<SkillStep>,
    pub output: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillInput {
    #[serde(rename = "type")]
    pub input_type: String,
    pub min: u32,
    pub max: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillStep {
    pub action: String,
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRepository {
    default_branch: String,
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 128 || id == "." || id == ".." {
        return Err("技能 id 不能为空、为路径特殊目录或超过 128 个字符".into());
    }
    if !id.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Err("技能 id 只能包含小写字母、数字、点、下划线和连字符".into());
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<(), String> {
    if version.is_empty() || version.len() > 64 {
        return Err("技能 version 必须是简短的 x.y.z 格式 semver".into());
    }
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
        })
    {
        return Err("技能 version 必须是简单的 x.y.z 数字 semver".into());
    }
    Ok(())
}

fn validate_prompt_path(prompt: &str) -> Result<(), String> {
    if prompt.is_empty() || prompt.len() > 512 || !prompt.to_ascii_lowercase().ends_with(".md") {
        return Err("agent.ask 的 prompt 必须是长度不超过 512 的 .md 相对路径".into());
    }
    if prompt.starts_with('/')
        || prompt.starts_with('\\')
        || prompt.contains('\\')
        || prompt.contains(':')
        || Path::new(prompt).is_absolute()
        || prompt.chars().any(|character| character.is_control())
    {
        return Err("prompt 不能是绝对路径、Windows 路径或包含控制字符".into());
    }
    if prompt
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("prompt 不能包含空路径段、当前目录或 .. 路径穿越".into());
    }
    Ok(())
}

fn prompt_relative_path(prompt: &str) -> Result<PathBuf, String> {
    validate_prompt_path(prompt)?;
    let mut path = PathBuf::new();
    for part in prompt.split('/') {
        path.push(part);
    }
    Ok(path)
}

/// Validate the complete declarative skill contract without touching the network or filesystem.
pub fn validate_manifest(manifest: &SkillManifest) -> Result<(), String> {
    if manifest.manifest_type != "gearnote-skill" {
        return Err("Manifest type 必须是 gearnote-skill".into());
    }
    if manifest.schema_version != 1 {
        return Err("Manifest schemaVersion 目前只支持 1".into());
    }
    validate_id(&manifest.id)?;
    validate_version(&manifest.version)?;
    for input in &manifest.inputs {
        if input.input_type != "records" {
            return Err(format!(
                "不允许的 skill input type：{}；只允许 records",
                input.input_type
            ));
        }
        if input.min > input.max {
            return Err("records input 的 min 不能大于 max".into());
        }
    }
    for permission in &manifest.permissions {
        if !matches!(
            permission.as_str(),
            "records.read.selected" | "agent.chat" | "chat.write"
        ) {
            return Err(format!("技能请求了不允许的权限：{permission}"));
        }
    }
    if manifest.steps.is_empty() {
        return Err("技能至少需要一个 agent.ask step".into());
    }
    let mut prompt = None::<&str>;
    for step in &manifest.steps {
        if step.action != "agent.ask" {
            return Err(format!(
                "不允许的 skill action：{}；只允许 agent.ask",
                step.action
            ));
        }
        validate_prompt_path(&step.prompt)?;
        if let Some(previous) = prompt {
            if previous != step.prompt {
                return Err("一个技能包最多只能包含一个 prompt 文件".into());
            }
        } else {
            prompt = Some(&step.prompt);
        }
    }
    if manifest.output != "chat.message" {
        return Err("技能 output 只允许 chat.message".into());
    }
    Ok(())
}

fn parse_manifest(bytes: &[u8]) -> Result<SkillManifest, String> {
    let manifest = serde_json::from_slice::<SkillManifest>(bytes)
        .map_err(|error| format!("技能 Manifest JSON 无效：{error}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn parse_github_url(value: &str) -> Result<(String, String), String> {
    let url = Url::parse(value)
        .map_err(|_| "只接受 https://github.com/{owner}/{repo} 链接".to_string())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("只接受不带端口、查询或片段的 https://github.com/{owner}/{repo} 链接".into());
    }
    let path = url.path();
    let path = path.strip_suffix('/').unwrap_or(path);
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() != 3
        || !parts[0].is_empty()
        || parts[1..].iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
    {
        return Err("GitHub 链接必须只有 owner 和 repo 两段路径，可带一个尾部斜杠".into());
    }
    Ok((parts[1].to_string(), parts[2].to_string()))
}

fn validate_remote_url(url: &Url, host: &str) -> Result<(), String> {
    if url.scheme() != "https"
        || url.host_str() != Some(host)
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(format!("网络请求被限制为 https://{host}"));
    }
    Ok(())
}

fn github_api_url(owner: &str, repo: &str) -> Result<Url, String> {
    let mut url =
        Url::parse("https://api.github.com").map_err(|_| "GitHub API 地址无效".to_string())?;
    url.path_segments_mut()
        .map_err(|_| "GitHub API 地址不能包含路径段".to_string())?
        .push("repos")
        .push(owner)
        .push(repo);
    validate_remote_url(&url, "api.github.com")?;
    Ok(url)
}

fn validate_branch(branch: &str) -> Result<(), String> {
    if branch.is_empty()
        || branch.len() > 255
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains('\\')
        || branch.contains("..")
        || branch.chars().any(|character| character.is_control())
        || branch
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err("GitHub 返回的 default_branch 无效".into());
    }
    Ok(())
}

fn github_raw_url(
    owner: &str,
    repo: &str,
    branch: &str,
    relative_path: &str,
) -> Result<Url, String> {
    validate_branch(branch)?;
    let mut url = Url::parse("https://raw.githubusercontent.com")
        .map_err(|_| "GitHub Raw 地址无效".to_string())?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "GitHub Raw 地址不能包含路径段".to_string())?;
        segments.push(owner).push(repo);
        for part in branch.split('/') {
            segments.push(part);
        }
        for part in relative_path.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                return Err("Raw 文件路径无效".into());
            }
            segments.push(part);
        }
    }
    validate_remote_url(&url, "raw.githubusercontent.com")?;
    Ok(url)
}

async fn fetch_bytes(
    client: &Client,
    url: &Url,
    host: &str,
    maximum: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    validate_remote_url(url, host)?;
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| format!("{label}请求失败：{error}"))?;
    validate_remote_url(response.url(), host)?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{label}返回非成功状态 HTTP {}", status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > maximum as u64)
    {
        return Err(format!("{label}超过大小上限 {} KB", maximum / 1024));
    }
    read_bounded_response(&mut response, maximum, label).await
}

async fn read_bounded_response(
    response: &mut Response,
    maximum: usize,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("{label}读取失败：{error}"))?
    {
        if bytes.len().saturating_add(chunk.len()) > maximum {
            return Err(format!("{label}超过大小上限 {} KB", maximum / 1024));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn skills_root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法定位应用数据目录：{error}"))?
        .join("skills"))
}

fn checked_skill_directory(app: &AppHandle, id: &str, version: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    validate_version(version)?;
    Ok(skills_root(app)?.join(id).join(version))
}

fn canonical_direct_child(child: &Path, parent: &Path, label: &str) -> Result<PathBuf, String> {
    let actual_parent = parent
        .canonicalize()
        .map_err(|error| format!("{label}的父目录不可用：{error}"))?;
    let actual_child = child
        .canonicalize()
        .map_err(|error| format!("{label}不可用：{error}"))?;
    if actual_child.parent() != Some(actual_parent.as_path()) {
        return Err(format!("{label}越过允许的应用目录，已拒绝"));
    }
    Ok(actual_child)
}

fn read_file_limited(path: &Path, maximum: usize, label: &str) -> Result<Vec<u8>, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("无法读取{label}：{error}"))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{label}不是普通文件"));
    }
    if metadata.len() > maximum as u64 {
        return Err(format!("{label}超过大小上限 {} KB", maximum / 1024));
    }
    let bytes = fs::read(path).map_err(|error| format!("读取{label}失败：{error}"))?;
    if bytes.len() > maximum {
        return Err(format!("{label}超过大小上限 {} KB", maximum / 1024));
    }
    Ok(bytes)
}

fn installed_manifest(id: &str, version: &str, directory: &Path) -> Result<SkillManifest, String> {
    let bytes = read_file_limited(
        &directory.join("manifest.json"),
        MAX_MANIFEST_BYTES,
        "技能 Manifest",
    )?;
    let manifest = parse_manifest(&bytes)?;
    if manifest.id != id || manifest.version != version {
        return Err("技能目录名与 Manifest 的 id/version 不一致".into());
    }
    Ok(manifest)
}

/// Install one declarative skill from the repository root, without executing repository code.
#[tauri::command]
pub async fn install_skill_from_github(
    app: AppHandle,
    url: String,
) -> Result<SkillManifest, String> {
    let (owner, repo) = parse_github_url(&url)?;
    let client = Client::builder()
        .https_only(true)
        .connect_timeout(NETWORK_TIMEOUT)
        .timeout(NETWORK_TIMEOUT)
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("无法初始化 GitHub 网络客户端：{error}"))?;

    let repository_url = github_api_url(&owner, &repo)?;
    let repository_bytes = fetch_bytes(
        &client,
        &repository_url,
        "api.github.com",
        MAX_REPOSITORY_RESPONSE_BYTES,
        "GitHub 仓库信息",
    )
    .await?;
    let repository = serde_json::from_slice::<GitHubRepository>(&repository_bytes)
        .map_err(|error| format!("GitHub 仓库信息无效：{error}"))?;
    validate_branch(&repository.default_branch)?;

    let manifest_url = github_raw_url(
        &owner,
        &repo,
        &repository.default_branch,
        "gearnote-skill.json",
    )?;
    let manifest_bytes = fetch_bytes(
        &client,
        &manifest_url,
        "raw.githubusercontent.com",
        MAX_MANIFEST_BYTES,
        "技能 Manifest",
    )
    .await?;
    let manifest = parse_manifest(&manifest_bytes)?;

    // Validation guarantees that there is exactly one distinct prompt path.
    let prompt = manifest
        .steps
        .first()
        .map(|step| step.prompt.clone())
        .ok_or("技能缺少 prompt")?;
    let prompt_url = github_raw_url(&owner, &repo, &repository.default_branch, &prompt)?;
    let prompt_bytes = fetch_bytes(
        &client,
        &prompt_url,
        "raw.githubusercontent.com",
        MAX_PROMPT_BYTES,
        "技能 prompt",
    )
    .await?;
    let prompt_relative = prompt_relative_path(&prompt)?;

    let root = skills_root(&app)?;
    let id_directory = root.join(&manifest.id);
    fs::create_dir_all(&id_directory).map_err(|error| format!("无法创建技能目录：{error}"))?;
    let data_directory = root.parent().ok_or("应用数据目录无效")?;
    canonical_direct_child(&root, data_directory, "技能根目录")?;
    canonical_direct_child(&id_directory, &root, "技能 id 目录")?;
    let target = id_directory.join(&manifest.version);
    if fs::symlink_metadata(&target).is_ok() {
        return Err("这个技能版本已经安装；请先卸载后再安装".into());
    }
    let temporary = id_directory.join(format!(".installing-{}", Uuid::new_v4()));
    fs::create_dir(&temporary).map_err(|error| format!("无法创建技能临时目录：{error}"))?;

    let write_result = (|| -> Result<(), String> {
        fs::write(temporary.join("manifest.json"), &manifest_bytes)
            .map_err(|error| format!("写入技能 Manifest 失败：{error}"))?;
        let prompt_path = temporary.join(&prompt_relative);
        if let Some(parent) = prompt_path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建 prompt 目录失败：{error}"))?;
        }
        fs::write(&prompt_path, &prompt_bytes)
            .map_err(|error| format!("写入技能 prompt 失败：{error}"))?;
        if fs::symlink_metadata(&target).is_ok() {
            return Err("这个技能版本已经安装；请先卸载后再安装".into());
        }
        fs::rename(&temporary, &target).map_err(|error| format!("完成技能安装失败：{error}"))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    write_result?;
    Ok(manifest)
}

/// List valid installed manifests by reading the skills directory only.
#[tauri::command]
pub fn list_installed_skills(app: AppHandle) -> Result<Vec<SkillManifest>, String> {
    let root = skills_root(&app)?;
    let ids = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("无法读取技能目录：{error}")),
    };
    let mut manifests = Vec::new();
    for id_entry in ids {
        let Ok(id_entry) = id_entry else { continue };
        let id = id_entry.file_name().to_string_lossy().into_owned();
        if validate_id(&id).is_err() || !id_entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let versions = match fs::read_dir(id_entry.path()) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for version_entry in versions {
            let Ok(version_entry) = version_entry else {
                continue;
            };
            let version = version_entry.file_name().to_string_lossy().into_owned();
            if validate_version(&version).is_err()
                || !version_entry.file_type().is_ok_and(|kind| kind.is_dir())
            {
                continue;
            }
            if let Ok(manifest) = installed_manifest(&id, &version, &version_entry.path()) {
                manifests.push(manifest);
            }
        }
    }
    manifests.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.version.cmp(&right.version))
    });
    Ok(manifests)
}

/// Read the installed prompt after validating both path components and the manifest again.
#[tauri::command]
pub fn get_skill_prompt(app: AppHandle, id: String, version: String) -> Result<String, String> {
    let directory = checked_skill_directory(&app, &id, &version)?;
    let root = skills_root(&app)?;
    let data_directory = root.parent().ok_or("应用数据目录无效")?;
    canonical_direct_child(&root, data_directory, "技能根目录")?;
    let id_directory = root.join(&id);
    canonical_direct_child(&id_directory, &root, "技能 id 目录")?;
    let actual_directory = canonical_direct_child(&directory, &id_directory, "技能目录")?;
    let manifest = installed_manifest(&id, &version, &directory)?;
    let prompt = manifest
        .steps
        .first()
        .map(|step| step.prompt.as_str())
        .ok_or("技能缺少 prompt")?;
    let relative = prompt_relative_path(prompt)?;
    let prompt_path = directory.join(relative);
    let actual_prompt = prompt_path
        .canonicalize()
        .map_err(|error| format!("prompt 文件不可用：{error}"))?;
    if !actual_prompt.starts_with(&actual_directory) || actual_prompt == actual_directory {
        return Err("prompt 路径越过技能目录，已拒绝".into());
    }
    let bytes = read_file_limited(&actual_prompt, MAX_PROMPT_BYTES, "技能 prompt")?;
    String::from_utf8(bytes).map_err(|_| "技能 prompt 不是有效 UTF-8 文本".into())
}

/// Remove one exact id/version directory after validating its identity and location.
#[tauri::command]
pub fn uninstall_skill(app: AppHandle, id: String, version: String) -> Result<(), String> {
    let directory = checked_skill_directory(&app, &id, &version)?;
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("无法检查技能目录：{error}")),
    };
    if !metadata.file_type().is_dir() {
        return Err("技能安装路径不是普通目录，已拒绝删除".into());
    }
    let root = skills_root(&app)?;
    let data_directory = root.parent().ok_or("应用数据目录无效")?;
    canonical_direct_child(&root, data_directory, "技能根目录")?;
    let id_directory = root.join(&id);
    canonical_direct_child(&id_directory, &root, "技能 id 目录")?;
    canonical_direct_child(&directory, &id_directory, "技能目录")?;
    fs::remove_dir_all(&directory).map_err(|error| format!("卸载技能失败：{error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MANIFEST: &str = r#"{
        "type":"gearnote-skill",
        "schemaVersion":1,
        "id":"community.reader_demo-1",
        "version":"1.2.3",
        "name":"阅读助手",
        "description":"总结当前选中的记录",
        "author":"GearNote Community",
        "license":"MIT",
        "homepage":"https://github.com/example/reader-skill",
        "permissions":["records.read.selected","agent.chat","chat.write"],
        "inputs":[{"type":"records","min":1,"max":5}],
        "steps":[{"action":"agent.ask","prompt":"prompts/main.md"}],
        "output":"chat.message"
    }"#;

    fn valid_manifest() -> SkillManifest {
        serde_json::from_str(VALID_MANIFEST).expect("valid test manifest")
    }

    #[test]
    fn accepts_legal_manifest_sample() {
        let manifest = valid_manifest();
        assert!(validate_manifest(&manifest).is_ok());
        assert_eq!(manifest.schema_version, 1);
        assert!(prompt_relative_path(&manifest.steps[0].prompt).is_ok());
    }

    #[test]
    fn official_compare_example_is_valid() {
        let manifest = parse_manifest(include_bytes!("../../gearnote-skill.json")).expect("official compare example");
        assert_eq!(manifest.id, "dev.gearnote.compare");
        assert_eq!(manifest.inputs[0].min, 2);
        assert_eq!(manifest.inputs[0].max, 4);
        assert_eq!(manifest.steps[0].prompt, "packages/skills/compare/prompts/compare.md");
    }

    #[test]
    fn rejects_malicious_permission() {
        let mut manifest = valid_manifest();
        manifest.permissions.push("records.delete.all".into());
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn rejects_prompt_path_traversal() {
        let mut manifest = valid_manifest();
        manifest.steps[0].prompt = "../outside.md".into();
        assert!(validate_manifest(&manifest).is_err());
        manifest.steps[0].prompt = "prompts/../../outside.md".into();
        assert!(validate_manifest(&manifest).is_err());
        manifest.steps[0].prompt = "C:\\outside.md".into();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn rejects_wrong_action() {
        let mut manifest = valid_manifest();
        manifest.steps[0].action = "process.execute".into();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn rejects_wrong_manifest_type() {
        let mut manifest = valid_manifest();
        manifest.manifest_type = "gearnote-plugin".into();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn accepts_only_exact_github_repository_urls() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo").expect("repository URL"),
            ("owner".into(), "repo".into())
        );
        assert!(parse_github_url("https://github.com/owner/repo/").is_ok());
        assert!(parse_github_url("http://github.com/owner/repo").is_err());
        assert!(parse_github_url("https://github.com/owner/repo/issues").is_err());
        assert!(parse_github_url("https://github.com/owner//repo").is_err());
        assert!(parse_github_url("https://github.com.evil.example/owner/repo").is_err());
    }

    #[test]
    fn rejects_invalid_identity_and_multiple_prompt_files() {
        let mut manifest = valid_manifest();
        manifest.id = "..".into();
        assert!(validate_manifest(&manifest).is_err());
        manifest.id = "reader".into();
        manifest.version = "1.2".into();
        assert!(validate_manifest(&manifest).is_err());
        manifest.version = "1.2.3".into();
        manifest.steps.push(SkillStep {
            action: "agent.ask".into(),
            prompt: "other.md".into(),
        });
        assert!(validate_manifest(&manifest).is_err());
    }
}
