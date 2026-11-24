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
        fs::create_dir(&dir).map_err(|error| format!("无法创建临时凭据任务目录：{error}"))?;
        restrict_dir_permissions(&dir)?;
        let lock = FileLock::acquire(&dir.join("lock"))?;
        let file = dir.join("cookies.txt");
        fs::write(&file, content).map_err(|error| format!("无法写入临时 cookies：{error}"))?;
        restrict_file_permissions(&file)?;
        Ok(Self { dir, file, lock: Some(lock) })
    }

    pub fn file(&self) -> &Path { &self.file }
}

impl Drop for TaskCookies {
    fn drop(&mut self) {
        // 先释放锁并关闭句柄，再删除目录；删除失败只记录路径，绝不记录凭据内容，
        // 残留目录会由启动/新任务时的清理机制重试。
        self.lock.take();
        if let Err(error) = fs::remove_dir_all(&self.dir) {
            eprintln!("GearNote 临时凭据清理失败，将在下次启动重试：{} ({error})", self.dir.display());
        }
    }
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

    /// 核对 DACL 中没有 Everyone / Guests 的允许访问项；发现即拒绝创建凭据。
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
        if dacl.is_null() { return Ok(()); } // 无 DACL 的极端情况由继承策略兜底，这里不放宽
        let mut size = AclSizeInformation { ace_count: 0, acl_bytes_in_use: 0, acl_bytes_free: 0 };
        if unsafe { GetAclInformation(dacl, &mut size as *mut _ as *mut c_void, std::mem::size_of::<AclSizeInformation>() as u32, 2) } == 0 {
            return Err("无法读取临时凭据访问控制列表".into());
        }
        let broad = ["S-1-1-0", "S-1-5-32-546"]; // Everyone, BUILTIN\Guests
        for sid_text in broad {
            let wide: Vec<u16> = sid_text.encode_utf16().chain(std::iter::once(0)).collect();
            let mut sid: *mut c_void = std::ptr::null_mut();
            if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } == 0 { continue; }
            for index in 0..size.ace_count {
                let mut ace: *mut c_void = std::ptr::null_mut();
                if unsafe { GetAce(dacl, index, &mut ace) } == 0 || ace.is_null() { continue; }
                let ace_type = unsafe { *(ace as *const u8) };
                // ACCESS_ALLOWED_ACE_TYPE=0；SID 位于 4 字节头 + 4 字节掩码之后。
                if ace_type == 0 {
                    let ace_sid = unsafe { (ace as *const u8).add(8) as *const c_void };
                    if unsafe { EqualSid(ace_sid, sid) } != 0 {
                        unsafe { LocalFree(sid); }
                        return Err("临时凭据目录存在过宽的访问权限，已拒绝写入".into());
                    }
                }
            }
            unsafe { LocalFree(sid); }
        }
        Ok(())
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
        if !lock_path.is_file() { continue; } // 无法证明所有权结构的目录保守跳过
        let Ok(lock) = FileLock::acquire(&lock_path) else { continue };
        drop(lock);
        if fs::remove_dir_all(&canonical).is_ok() { removed += 1; }
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
        // 用户临时目录继承的 ACL 不应包含 Everyone/Guests 允许项。
        assert!(TaskCookies::prepare(FAKE_COOKIES, &credentials).is_ok());
        let _ = fs::remove_dir_all(root);
    }
}
