// 临时登录凭据的生命周期：每任务独立目录 + 跨进程文件锁 + 受限残留清理。
// 媒体工具需要明文 cookies 文件，这里如实承认其短暂存在；不通过改扩展名或编码假装加密。
use std::{fs, path::{Path, PathBuf}, time::Duration};

/// 跨进程排他锁。进程崩溃或被强制结束时由操作系统自动释放，
/// 因此“能否抢到锁”可以作为目录是否仍有活任务所有的可验证依据。
pub(crate) struct FileLock { file: fs::File }

impl FileLock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        let file = fs::OpenOptions::new().create(true).read(true).write(true).open(path)
            .map_err(|error| format!("无法打开凭据锁文件：{error}"))?;
        platform::lock(&file)?;
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) { platform::unlock(&self.file); }
}

#[cfg(windows)]
mod platform {
    use std::{ffi::c_void, fs::File, os::windows::io::AsRawHandle};

    #[repr(C)]
    struct Overlapped { internal: usize, internal_high: usize, offset: u32, offset_high: u32, event: *mut c_void }

    #[link(name = "kernel32")]
    extern "system" {
        fn LockFileEx(handle: *mut c_void, flags: u32, reserved: u32, bytes_low: u32, bytes_high: u32, overlapped: *mut Overlapped) -> i32;
        fn UnlockFileEx(handle: *mut c_void, reserved: u32, bytes_low: u32, bytes_high: u32, overlapped: *mut Overlapped) -> i32;
    }

    const LOCKFILE_EXCLUSIVE_LOCK: u32 = 0x2;
    const LOCKFILE_FAIL_IMMEDIATELY: u32 = 0x1;

    pub fn lock(file: &File) -> Result<(), String> {
        let mut overlapped = Overlapped { internal: 0, internal_high: 0, offset: 0, offset_high: 0, event: std::ptr::null_mut() };
        let ok = unsafe { LockFileEx(file.as_raw_handle(), LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY, 0, 1, 0, &mut overlapped) };
        if ok == 0 { Err("凭据目录正被其他任务使用".into()) } else { Ok(()) }
    }

    pub fn unlock(file: &File) {
        let mut overlapped = Overlapped { internal: 0, internal_high: 0, offset: 0, offset_high: 0, event: std::ptr::null_mut() };
        unsafe { UnlockFileEx(file.as_raw_handle(), 0, 1, 0, &mut overlapped); }
    }
}

#[cfg(unix)]
mod platform {
    use std::{fs::File, os::unix::io::AsRawFd};

    extern "C" { fn flock(fd: i32, operation: i32) -> i32; }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    const LOCK_UN: i32 = 8;

    pub fn lock(file: &File) -> Result<(), String> {
        if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0 {
            Err("凭据目录正被其他任务使用".into())
        } else { Ok(()) }
    }

    pub fn unlock(file: &File) { unsafe { flock(file.as_raw_fd(), LOCK_UN); } }
}

/// 一次分析任务的临时凭据。锁与文件同生命周期；Drop 时先释放锁再删除整个任务目录。
pub(crate) struct TaskCookies { dir: PathBuf, file: PathBuf, lock: Option<FileLock> }

impl TaskCookies {
    /// 在专用凭据目录下创建独立任务目录。内容校验由调用方（B 站 cookies 校验）先行完成。
    pub fn prepare(content: &str, credentials_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(credentials_dir).map_err(|error| format!("无法创建临时凭据目录：{error}"))?;
        restrict_dir_permissions(credentials_dir)?;
        let dir = credentials_dir.join(format!("task-{}", uuid::Uuid::new_v4()));
        create_private_dir(&dir)?;
        Self::prepare_in(&dir, content)
    }

    /// 目录已存在时建立清理所有权：此后任一步骤失败都由 Drop 清理本次任务目录，
    /// 不会在 guard 建立前提前返回而留下明文文件。
    fn prepare_in(dir: &Path, content: &str) -> Result<Self, String> {
        let mut guard = Self { dir: dir.to_path_buf(), file: dir.join("cookies.txt"), lock: None };
        restrict_dir_permissions(dir)?;
        guard.lock = Some(FileLock::acquire(&dir.join("lock"))?);
        write_private_file(&guard.file, content)?;
        restrict_file_permissions(&guard.file)?;
        Ok(guard)
    }

    pub fn file(&self) -> &Path { &self.file }
}

/// 创建目录时即收紧权限（Unix 0700），避免先以较宽权限创建再 chmod 的窗口。
fn create_private_dir(dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(dir)
            .map_err(|error| format!("无法创建临时凭据任务目录：{error}"))?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(dir).map_err(|error| format!("无法创建临时凭据任务目录：{error}"))?;
    }
    Ok(())
}

/// 写入文件时即收紧权限（Unix 0600）。错误只含 OS 诊断，不含凭据内容。
fn write_private_file(path: &Path, content: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut file = fs::OpenOptions::new().create(true).write(true).truncate(true).mode(0o600)
            .open(path).map_err(|error| format!("无法写入临时 cookies：{error}"))?;
        file.write_all(content.as_bytes()).map_err(|error| format!("无法写入临时 cookies：{error}"))?;
    }
    #[cfg(not(unix))]
    {
        fs::write(path, content).map_err(|error| format!("无法写入临时 cookies：{error}"))?;
    }
    Ok(())
}

/// 从 Netscape cookies 内容提取敏感值（第 7 字段），用于错误返回脱敏。
pub(crate) fn sensitive_values(content: &str) -> Vec<String> {
    let mut values = Vec::new();
    for line in content.lines() {
        let line = line.trim_start_matches('\u{feff}');
        if line.trim().is_empty() || (line.starts_with('#') && !line.starts_with("#HttpOnly_")) { continue; }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() == 7 && !fields[6].is_empty() { values.push(fields[6].to_string()); }
    }
    values
}

/// 收集专用凭据目录中现存任务 cookies 的敏感值（只读），供取帧等无 cookies 入参的错误脱敏使用。
pub(crate) fn cached_sensitive_values(credentials_dir: &Path) -> Vec<String> {
    let mut values = Vec::new();
    let Ok(entries) = fs::read_dir(credentials_dir) else { return values };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("task-") { continue; }
        if let Ok(content) = fs::read_to_string(entry.path().join("cookies.txt")) {
            values.extend(sensitive_values(&content));
        }
    }
    values
}

/// 保守脱敏：替换已知敏感值，并掩码 SESSDATA=... 形式；保留其余必要诊断信息。
pub(crate) fn redact(text: &str, secrets: &[String]) -> String {
    let mut result = text.to_string();
    for secret in secrets {
        if !secret.is_empty() { result = result.replace(secret.as_str(), "[已脱敏]"); }
    }
    redact_key_value(&result, "SESSDATA")
}

fn redact_key_value(text: &str, key: &str) -> String {
    let needle = format!("{key}=");
    let mut out = String::new();
    let mut rest = text;
    while let Some(pos) = rest.find(&needle) {
        out.push_str(&rest[..pos + needle.len()]);
        let after = &rest[pos + needle.len()..];
        let end = after.find(|c: char| c.is_whitespace() || c == ';' || c == '&').unwrap_or(after.len());
        out.push_str("[已脱敏]");
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

impl Drop for TaskCookies {
    fn drop(&mut self) {
        // 先释放锁并关闭句柄，再删除目录；删除失败只记录路径，绝不记录凭据内容，
        // 残留目录会由启动/新任务时的清理机制重试。
        self.lock.take();
        if let Err(error) = remove_task_dir(&self.dir) {
            eprintln!("GearNote 临时凭据清理失败，将在下次启动重试：{} ({error})", self.dir.display());
        }
    }
}

/// 删除任务目录时先单独移除明文凭据文件，再删整个目录：
/// 即使目录删除中途失败，敏感内容也已优先移除，不会留下无锁保护的明文。
fn remove_task_dir(dir: &Path) -> std::io::Result<()> {
    let _ = fs::remove_file(dir.join("cookies.txt"));
    fs::remove_dir_all(dir)
}

/// 无 lock 文件的 task-* 目录可能是删除中途失败的残留。保守判定：
/// 仅含已知凭据文件、且 cookies.txt 超过 60 秒未修改时才视为可清理残留，
/// 避免误删刚创建尚未加锁的活动任务。
fn is_orphaned_lockless_task(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else { return false };
    let mut has_cookies = false;
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { return false };
        match name {
            "cookies.txt" => has_cookies = true,
            "lock" => {}
            _ => return false,
        }
    }
    if !has_cookies { return false; }
    let Ok(meta) = fs::metadata(dir.join("cookies.txt")) else { return false };
    let Ok(modified) = meta.modified() else { return false };
    modified.elapsed().map(|age| age > Duration::from_secs(60)).unwrap_or(false)
}

/// 凭据生命周期跟随实际媒体执行：guard 被移入执行闭包，
/// 即使外层异步任务不再等待，正在运行的媒体进程也不会提前失去所需文件。
pub(crate) fn run_with_cookies<T>(cookies: Option<TaskCookies>, run: impl FnOnce(Option<&Path>) -> T) -> T {
    let path = cookies.as_ref().map(TaskCookies::file);
    let result = run(path);
    drop(cookies);
    result
}

fn restrict_dir_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("无法收紧临时凭据目录权限：{error}"))?;
    }
    #[cfg(windows)]
    {
        // 缓存目录位于当前用户的 LocalAppData，ACL 默认仅当前用户、SYSTEM 与 Administrators。
        // 这里核对没有放宽到 Everyone/Guests；绝不主动授予广泛访问权限。
        windows::verify_no_broad_access(path)?;
    }
    Ok(())
}

fn restrict_file_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("无法收紧临时凭据文件权限：{error}"))?;
    }
    #[cfg(windows)]
    {
        windows::verify_no_broad_access(path)?;
    }
    Ok(())
}

#[cfg(windows)]
mod windows {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path};

    #[link(name = "advapi32")]
    extern "system" {
        fn GetNamedSecurityInfoW(object: *const u16, object_type: u32, security_info: u32,
            owner: *mut *mut c_void, group: *mut *mut c_void, dacl: *mut *mut c_void,
            sacl: *mut *mut c_void, descriptor: *mut *mut c_void) -> u32;
        fn GetAclInformation(acl: *const c_void, info: *mut c_void, len: u32, class: u32) -> i32;
        fn GetAce(acl: *const c_void, index: u32, ace: *mut *mut c_void) -> i32;
        fn EqualSid(sid1: *const c_void, sid2: *const c_void) -> i32;
        fn ConvertStringSidToSidW(text: *const u16, sid: *mut *mut c_void) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" { fn LocalFree(memory: *mut c_void) -> *mut c_void; }

    #[repr(C)]
    struct AclSizeInformation { ace_count: u32, acl_bytes_in_use: u32, acl_bytes_free: u32 }

    /// 过宽主体：Everyone、BUILTIN\Guests、BUILTIN\Users、Authenticated Users。
    /// 当前用户、SYSTEM、Administrators 不在其中，不会误伤合法权限。
    const BROAD_SIDS: [&str; 4] = ["S-1-1-0", "S-1-5-32-546", "S-1-5-32-545", "S-1-5-11"];

    /// 核对 DACL 不对过宽主体授予访问；任何无法确证安全的情况都失败关闭，拒绝写入明文凭据。
    pub fn verify_no_broad_access(path: &Path) -> Result<(), String> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut dacl: *mut c_void = std::ptr::null_mut();
        let mut descriptor: *mut c_void = std::ptr::null_mut();
        // SE_FILE_OBJECT=1, DACL_SECURITY_INFORMATION=4
        let status = unsafe { GetNamedSecurityInfoW(wide.as_ptr(), 1, 4,
            std::ptr::null_mut(), std::ptr::null_mut(), &mut dacl, std::ptr::null_mut(), &mut descriptor) };
        if status != 0 { return Err(format!("无法核对临时凭据访问权限（错误 {status}）")); }
        let result = inspect(dacl);
        if !descriptor.is_null() { unsafe { LocalFree(descriptor); } }
        result
    }

    fn inspect(dacl: *mut c_void) -> Result<(), String> {
        // NULL DACL 表示完全不限制访问，绝不能解释为“继承兜底”；失败关闭。
        if dacl.is_null() { return Err("临时凭据对象没有 DACL（不受限访问），已拒绝写入".into()); }
        let mut size = AclSizeInformation { ace_count: 0, acl_bytes_in_use: 0, acl_bytes_free: 0 };
        if unsafe { GetAclInformation(dacl, &mut size as *mut _ as *mut c_void, std::mem::size_of::<AclSizeInformation>() as u32, 2) } == 0 {
            return Err("无法读取临时凭据访问控制列表".into());
        }
        let broad = broad_sids()?;
        for index in 0..size.ace_count {
            let mut ace: *mut c_void = std::ptr::null_mut();
            if unsafe { GetAce(dacl, index, &mut ace) } == 0 || ace.is_null() {
                return Err("无法读取临时凭据访问控制项".into());
            }
            let ace_ptr = ace as *const u8;
            let ace_type = unsafe { *ace_ptr };
            // 拒绝型 ACE（1=ACCESS_DENIED, 6=ACCESS_DENIED_OBJECT）不授予访问，跳过。
            if ace_type == 1 || ace_type == 6 { continue; }
            let Some(sid) = (match ace_type {
                // ACCESS_ALLOWED_ACE：4 字节头 + 4 字节掩码后是 SID。
                0 => Some(unsafe { ace_ptr.add(8) as *const c_void }),
                // ACCESS_ALLOWED_OBJECT_ACE：头4 + 掩码4 + 标志4，再按标志跳过可选 GUID。
                5 => Some(unsafe {
                    let flags = *(ace_ptr.add(8) as *const u32);
                    let offset = 12 + if flags & 0x1 != 0 { 16 } else { 0 } + if flags & 0x2 != 0 { 16 } else { 0 };
                    ace_ptr.add(offset) as *const c_void
                }),
                // 其它（含条件/回调允许型）无法确证安全，失败关闭。
                _ => None,
            }) else {
                return Err("临时凭据存在无法核验的允许访问项，已拒绝写入".into());
            };
            for broad_sid in &broad {
                if unsafe { EqualSid(sid, *broad_sid) } != 0 {
                    free_sids(&broad);
                    return Err("临时凭据目录存在过宽的访问权限，已拒绝写入".into());
                }
            }
        }
        free_sids(&broad);
        Ok(())
    }

    fn broad_sids() -> Result<Vec<*mut c_void>, String> {
        let mut sids = Vec::new();
        for text in BROAD_SIDS {
            let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let mut sid: *mut c_void = std::ptr::null_mut();
            if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } == 0 || sid.is_null() {
                free_sids(&sids);
                return Err("无法构造用于核对的安全主体".into());
            }
            sids.push(sid);
        }
        Ok(sids)
    }

    fn free_sids(sids: &[*mut c_void]) {
        for sid in sids { if !sid.is_null() { unsafe { LocalFree(*sid); } } }
    }

    /// 仅供隔离测试：用 SDDL 设置目录 DACL，或设置 NULL DACL，以构造安全/危险场景。
    #[cfg(test)]
    pub mod test_support {
        use super::*;

        #[link(name = "advapi32")]
        extern "system" {
            fn ConvertStringSecurityDescriptorToSecurityDescriptorW(text: *const u16, revision: u32,
                descriptor: *mut *mut c_void, size: *mut u32) -> i32;
            fn SetNamedSecurityInfoW(object: *const u16, object_type: u32, security_info: u32,
                owner: *mut c_void, group: *mut c_void, dacl: *mut c_void, sacl: *mut c_void) -> u32;
            fn GetSecurityDescriptorDacl(descriptor: *const c_void, present: *mut i32, dacl: *mut *mut c_void, defaulted: *mut i32) -> i32;
        }

        fn wide_path(path: &Path) -> Vec<u16> { path.as_os_str().encode_wide().chain(std::iter::once(0)).collect() }

        /// 用 SDDL 的 DACL 部分覆盖目录 DACL。
        pub fn set_dacl_sddl(path: &Path, sddl: &str) -> Result<(), String> {
            let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
            let mut descriptor: *mut c_void = std::ptr::null_mut();
            if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(wide.as_ptr(), 1, &mut descriptor, std::ptr::null_mut()) } == 0 {
                return Err("SDDL 转换失败".into());
            }
            let mut present = 0; let mut dacl: *mut c_void = std::ptr::null_mut(); let mut defaulted = 0;
            let ok = unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) };
            let object = wide_path(path);
            let status = if ok != 0 && present != 0 {
                unsafe { SetNamedSecurityInfoW(object.as_ptr(), 1, 4, std::ptr::null_mut(), std::ptr::null_mut(), dacl, std::ptr::null_mut()) }
            } else { 1 };
            unsafe { LocalFree(descriptor); }
            if status != 0 { return Err(format!("设置 DACL 失败（{status}）")); }
            Ok(())
        }

        /// 设置 NULL DACL（完全不限制访问）。
        pub fn set_null_dacl(path: &Path) -> Result<(), String> {
            let object = wide_path(path);
            // DACL_SECURITY_INFORMATION=4，传入 NULL dacl 且不带 PROTECTED 标志即设置 NULL DACL。
            let status = unsafe { SetNamedSecurityInfoW(object.as_ptr(), 1, 4, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
            if status != 0 { return Err(format!("设置 NULL DACL 失败（{status}）")); }
            Ok(())
        }
    }
}

/// 清理崩溃/强制退出遗留的任务目录：只扫描专用凭据目录下的 task-* 子目录，
/// 以“能否抢到锁”验证所有权；抢不到锁说明仍有活任务，跳过。
/// 不跟随符号链接/重解析点，不触碰媒体缓存、分镜图或其他应用文件。
pub(crate) fn cleanup_stale(credentials_dir: &Path) -> usize {
    let mut removed = 0;
    let Ok(entries) = fs::read_dir(credentials_dir) else { return 0 };
    let Ok(root) = credentials_dir.canonicalize() else { return 0 };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("task-") { continue; }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else { continue };
        if !meta.is_dir() || meta.file_type().is_symlink() { continue; }
        let Ok(canonical) = entry.path().canonicalize() else { continue };
        if !canonical.starts_with(&root) || canonical == root { continue; }
        let lock_path = canonical.join("lock");
        if lock_path.is_file() {
            // 有锁文件：抢到锁说明无活任务所有，可清理；抢不到则跳过。
            let Ok(lock) = FileLock::acquire(&lock_path) else { continue };
            drop(lock);
        } else if !is_orphaned_lockless_task(&canonical) {
            // 无锁文件：仅保守清理可证明的残留，其余跳过。
            continue;
        }
        if remove_task_dir(&canonical).is_ok() { removed += 1; }
    }
    removed
}

/// 旧版 auth-*.txt 没有所有权锁，无法证明是否仍被其他实例使用；
/// 保守策略：仅当能独占锁定该文件且超过 24 小时未被修改时才删除。
pub(crate) fn cleanup_legacy(cache_dir: &Path) -> usize {
    let mut removed = 0;
    let Ok(entries) = fs::read_dir(cache_dir) else { return 0 };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("auth-") || !name.ends_with(".txt") { continue; }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else { continue };
        if !meta.is_file() || meta.file_type().is_symlink() { continue; }
        let Ok(lock) = FileLock::acquire(&entry.path()) else { continue };
        let stale = fs::metadata(entry.path()).and_then(|meta| meta.modified()).ok()
            .is_some_and(|modified| modified.elapsed().is_ok_and(|age| age > Duration::from_secs(24 * 3600)));
        if stale && fs::remove_file(entry.path()).is_ok() { removed += 1; }
        drop(lock);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_COOKIES: &str = "# Netscape HTTP Cookie File\n#HttpOnly_.bilibili.com\tTRUE\t/\tTRUE\t1800000000\tSESSDATA\tfake-test-value\n";

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gearnote-credentials-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn task_cookies_exist_during_use_and_are_removed_on_drop() {
        let root = temp_dir("drop");
        let credentials = root.join("credentials");
        let task = TaskCookies::prepare(FAKE_COOKIES, &credentials).unwrap();
        assert!(task.file().is_file());
        assert_eq!(fs::read_to_string(task.file()).unwrap(), FAKE_COOKIES);
        let dir = task.dir.clone();
        drop(task);
        assert!(!dir.exists(), "正常结束后任务目录必须被删除");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn run_with_cookies_keeps_file_until_execution_finishes_even_on_error() {
        let root = temp_dir("engine");
        let credentials = root.join("credentials");
        let task = TaskCookies::prepare(FAKE_COOKIES, &credentials).unwrap();
        let dir = task.dir.clone();
        let file = task.file().to_path_buf();
        // 模拟媒体执行：执行期间文件必须存在；返回错误后仍须清理。
        let result: Result<(), String> = run_with_cookies(Some(task), |path| {
            assert!(path.unwrap().is_file());
            assert!(file.is_file());
            Err("模拟引擎失败".into())
        });
        assert!(result.is_err());
        assert!(!dir.exists(), "执行失败后也要清理凭据");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_removes_crash_leftovers_but_skips_live_tasks() {
        let root = temp_dir("cleanup");
        let credentials = root.join("credentials");
        // 模拟崩溃残留：有任务目录和锁文件，但没有活进程持锁。
        let orphan = credentials.join(format!("task-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("lock"), "").unwrap();
        fs::write(orphan.join("cookies.txt"), FAKE_COOKIES).unwrap();
        // 活任务：本进程持锁，清理必须跳过。
        let live = TaskCookies::prepare(FAKE_COOKIES, &credentials).unwrap();
        let live_dir = live.dir.clone();
        assert_eq!(cleanup_stale(&credentials), 1);
        assert!(!orphan.exists());
        assert!(live_dir.exists(), "仍在使用的凭据不能被误删");
        drop(live);
        assert!(!live_dir.exists());
        let _ = fs::remove_dir_all(root);
    }

    fn make_file_old(path: &Path) {
        let past = std::time::SystemTime::now() - Duration::from_secs(120);
        fs::File::options().write(true).open(path).unwrap().set_modified(past).unwrap();
    }

    #[test]
    fn cleanup_removes_lockless_orphan_with_old_cookies() {
        let root = temp_dir("lockless-orphan");
        let credentials = root.join("credentials");
        // 模拟“删除中途删掉了 lock 文件、cookies.txt 残留”的异常路径。
        let orphan = credentials.join(format!("task-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("cookies.txt"), FAKE_COOKIES).unwrap();
        make_file_old(&orphan.join("cookies.txt"));
        assert_eq!(cleanup_stale(&credentials), 1, "无锁但含陈旧明文的残留必须被清理");
        assert!(!orphan.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_keeps_fresh_lockless_task_to_protect_active_creation() {
        let root = temp_dir("lockless-fresh");
        let credentials = root.join("credentials");
        let fresh = credentials.join(format!("task-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&fresh).unwrap();
        fs::write(fresh.join("cookies.txt"), FAKE_COOKIES).unwrap();
        // 刚创建、尚未加锁的活动任务不能被误删。
        assert_eq!(cleanup_stale(&credentials), 0);
        assert!(fresh.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_keeps_lockless_dir_with_unrelated_file() {
        let root = temp_dir("lockless-unrelated");
        let credentials = root.join("credentials");
        let dir = credentials.join(format!("task-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("cookies.txt"), FAKE_COOKIES).unwrap();
        fs::write(dir.join("other.txt"), b"unrelated").unwrap();
        make_file_old(&dir.join("cookies.txt"));
        assert_eq!(cleanup_stale(&credentials), 0, "含非凭据文件的目录保守保留");
        assert!(dir.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn drop_removes_plaintext_cookies_file() {
        let root = temp_dir("drop-file");
        let credentials = root.join("credentials");
        let task = TaskCookies::prepare(FAKE_COOKIES, &credentials).unwrap();
        let file = task.file().to_path_buf();
        assert!(file.is_file());
        drop(task);
        assert!(!file.exists(), "Drop 后明文凭据文件必须被移除");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cached_sensitive_values_reads_existing_task_cookies() {
        let root = temp_dir("cached-values");
        let credentials = root.join("credentials");
        let task = TaskCookies::prepare(FAKE_COOKIES, &credentials).unwrap();
        let values = cached_sensitive_values(&credentials);
        assert!(values.contains(&"fake-test-value".to_string()));
        drop(task);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_never_touches_unrelated_files_or_escapes_directory() {
        let root = temp_dir("boundary");
        let credentials = root.join("credentials");
        fs::create_dir_all(&credentials).unwrap();
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        fs::write(media.join("video.mp4"), b"not a credential").unwrap();
        fs::write(credentials.join("notes.txt"), b"unrelated").unwrap();
        // 不带 lock 文件的 task-* 目录：无法证明所有权，保守保留。
        let unknown = credentials.join(format!("task-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&unknown).unwrap();
        fs::write(unknown.join("cookies.txt"), FAKE_COOKIES).unwrap();
        assert_eq!(cleanup_stale(&credentials), 0);
        assert!(media.join("video.mp4").is_file());
        assert!(credentials.join("notes.txt").is_file());
        assert!(unknown.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_skips_symlinked_task_directories() {
        let root = temp_dir("symlink");
        let credentials = root.join("credentials");
        fs::create_dir_all(&credentials).unwrap();
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, credentials.join(format!("task-{}", uuid::Uuid::new_v4()))).unwrap();
        assert_eq!(cleanup_stale(&credentials), 0);
        assert!(outside.join("keep.txt").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_cleanup_requires_lock_and_age() {
        let root = temp_dir("legacy");
        let old = root.join(format!("auth-{}.txt", uuid::Uuid::new_v4()));
        let fresh = root.join(format!("auth-{}.txt", uuid::Uuid::new_v4()));
        fs::write(&old, FAKE_COOKIES).unwrap();
        fs::write(&fresh, FAKE_COOKIES).unwrap();
        let past = std::time::SystemTime::now() - Duration::from_secs(25 * 3600);
        fs::File::options().write(true).open(&old).unwrap().set_modified(past).unwrap();
        assert_eq!(cleanup_legacy(&root), 1);
        assert!(!old.exists(), "超过 24 小时且可独占锁定的旧残留应被清理");
        assert!(fresh.exists(), "新文件不能仅凭模式被删除");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn error_messages_do_not_echo_credential_values() {
        let root = temp_dir("error");
        // 把“凭据目录”指向一个已存在的普通文件，制造创建失败。
        let blocker = root.join("blocker");
        fs::write(&blocker, b"x").unwrap();
        let error = match TaskCookies::prepare(FAKE_COOKIES, &blocker) {
            Err(error) => error,
            Ok(_) => panic!("把普通文件当作凭据目录时应当失败"),
        };
        assert!(!error.contains("fake-test-value"), "错误信息不得包含凭据内容");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepare_failure_after_dir_creation_cleans_up_task_dir() {
        let root = temp_dir("prepare-fail");
        let dir = root.join(format!("task-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        // 让 cookies.txt 成为一个目录，使写入步骤失败；此时 guard 已建立，应当次清理。
        fs::create_dir(dir.join("cookies.txt")).unwrap();
        let result = TaskCookies::prepare_in(&dir, FAKE_COOKIES);
        assert!(result.is_err());
        assert!(!dir.exists(), "准备阶段失败后必须清理本次任务目录，不能留下明文文件");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn redaction_removes_known_secrets_and_sessdata_pattern() {
        let secrets = vec!["fake-test-value".to_string()];
        let text = "引擎失败：SESSDATA=fake-test-value 出现在 stderr；另有 SESSDATA=other-secret; 路径正常";
        let redacted = redact(text, &secrets);
        assert!(!redacted.contains("fake-test-value"), "已知敏感值必须被替换");
        assert!(!redacted.contains("other-secret"), "SESSDATA= 形式必须被掩码");
        assert!(redacted.contains("引擎失败"), "正常诊断信息应保留");
        assert!(redacted.contains("路径正常"));
        // 即使没有已知敏感值列表，SESSDATA= 形式仍被掩码。
        assert!(!redact("SESSDATA=abc123", &[]).contains("abc123"));
    }

    #[test]
    fn sensitive_values_extracts_cookie_seventh_field() {
        let values = sensitive_values(FAKE_COOKIES);
        assert_eq!(values, vec!["fake-test-value".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn unix_permissions_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let root = temp_dir("perms");
        let credentials = root.join("credentials");
        let task = TaskCookies::prepare(FAKE_COOKIES, &credentials).unwrap();
        assert_eq!(fs::metadata(task.file()).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(&task.dir).unwrap().permissions().mode() & 0o777, 0o700);
        drop(task);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn windows_acl_check_accepts_user_scoped_cache() {
        let root = temp_dir("acl");
        let credentials = root.join("credentials");
        // 用户临时目录继承的 ACL 不应包含过宽允许项。
        assert!(TaskCookies::prepare(FAKE_COOKIES, &credentials).is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    fn remove_with_reset(path: &Path) {
        // 仅针对隔离测试目录：恢复可删除权限后清理，不触碰真实应用缓存。
        let _ = windows::test_support::set_dacl_sddl(path, "D:(A;;GA;;;WD)(A;;GA;;;SY)");
        let _ = fs::remove_dir_all(path);
    }

    #[cfg(windows)]
    #[test]
    fn windows_acl_rejects_broad_principals() {
        // WD=Everyone, BG=Guests, BU=BUILTIN\Users, AU=Authenticated Users
        for sddl in ["D:(A;;GA;;;WD)", "D:(A;;GA;;;BG)", "D:(A;;GA;;;BU)", "D:(A;;GA;;;AU)"] {
            let root = temp_dir("broad");
            windows::test_support::set_dacl_sddl(&root, sddl).unwrap();
            assert!(windows::verify_no_broad_access(&root).is_err(), "应拒绝过宽 ACL：{sddl}");
            remove_with_reset(&root);
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_acl_rejects_object_allow_ace_for_broad_principal() {
        let root = temp_dir("object-ace");
        // OA = ACCESS_ALLOWED_OBJECT_ACE，授予 Everyone。
        windows::test_support::set_dacl_sddl(&root, "D:(OA;;GA;;;WD)").unwrap();
        assert!(windows::verify_no_broad_access(&root).is_err());
        remove_with_reset(&root);
    }

    #[cfg(windows)]
    #[test]
    fn windows_acl_rejects_null_dacl() {
        let root = temp_dir("null-dacl");
        windows::test_support::set_null_dacl(&root).unwrap();
        assert!(windows::verify_no_broad_access(&root).is_err(), "NULL DACL 必须失败关闭");
        remove_with_reset(&root);
    }

    #[cfg(windows)]
    #[test]
    fn windows_acl_accepts_system_and_administrators_only() {
        let root = temp_dir("safe-dacl");
        windows::test_support::set_dacl_sddl(&root, "D:(A;;GA;;;SY)(A;;GA;;;BA)").unwrap();
        assert!(windows::verify_no_broad_access(&root).is_ok());
        remove_with_reset(&root);
    }
}
