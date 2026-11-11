use std::path::Path;

#[cfg(windows)]
mod windows {
    use std::{ffi::c_void, fs, io::Write, path::Path};

    #[repr(C)]
    struct DataBlob { len: u32, data: *mut u8 }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(input: *const DataBlob, description: *const u16, entropy: *const DataBlob,
            reserved: *mut c_void, prompt: *const c_void, flags: u32, output: *mut DataBlob) -> i32;
        fn CryptUnprotectData(input: *const DataBlob, description: *mut *mut u16, entropy: *const DataBlob,
            reserved: *mut c_void, prompt: *const c_void, flags: u32, output: *mut DataBlob) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" { fn LocalFree(memory: *mut c_void) -> *mut c_void; }

    fn transform(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
        let len = u32::try_from(bytes.len()).map_err(|_| "API 密钥过长")?;
        let input = DataBlob { len, data: bytes.as_ptr() as *mut u8 };
        let mut output = DataBlob { len: 0, data: std::ptr::null_mut() };
        // DPAPI defaults to the current Windows user, not every account on this computer.
        let success = unsafe {
            if encrypt {
                CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(),
                    std::ptr::null(), 0, &mut output)
            } else {
                CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(),
                    std::ptr::null(), 0, &mut output)
            }
        };
        if success == 0 { return Err(format!("Windows 密钥保护失败：{}", std::io::Error::last_os_error())); }
        let result = unsafe { std::slice::from_raw_parts(output.data, output.len as usize).to_vec() };
        unsafe { LocalFree(output.data.cast()); }
        Ok(result)
    }

    pub(super) fn load(path: &Path) -> Result<Option<String>, String> {
        let backup = path.with_extension("bin.bak");
        if !path.exists() && backup.exists() {
            fs::rename(&backup, path).map_err(|_| "密钥备份恢复失败")?;
        }
        if !path.exists() { return Ok(None); }
        let encrypted = fs::read(path).map_err(|_| "保存的密钥读取失败")?;
        let plain = transform(&encrypted, false)?;
        let key = String::from_utf8(plain).map_err(|_| "保存的密钥格式无效")?;
        if key.is_empty() { return Err("保存的密钥为空".into()); }
        Ok(Some(key))
    }

    pub(super) fn save(path: &Path, key: &str) -> Result<(), String> {
        let backup = path.with_extension("bin.bak");
        if key.is_empty() {
            if path.exists() { fs::remove_file(path).map_err(|_| "密钥清除失败")?; }
            if backup.exists() { fs::remove_file(backup).map_err(|_| "密钥备份清除失败")?; }
            return Ok(());
        }
        let encrypted = transform(key.as_bytes(), true)?;
        let temp = path.with_extension("bin.tmp");
        let mut file = fs::File::create(&temp).map_err(|_| "密钥保存失败")?;
        file.write_all(&encrypted).map_err(|_| "密钥保存失败")?;
        file.sync_all().map_err(|_| "密钥保存失败")?;
        drop(file);
        if backup.exists() { fs::remove_file(&backup).map_err(|_| "密钥备份清理失败")?; }
        let previous = path.exists();
        if previous { fs::rename(path, &backup).map_err(|_| "密钥备份失败")?; }
        if fs::rename(&temp, path).is_err() {
            if previous { let _ = fs::rename(&backup, path); }
            return Err("密钥保存失败".into());
        }
        if previous { let _ = fs::remove_file(backup); }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn persists_key_encrypted_and_restores_it() {
            let dir = std::env::temp_dir().join(format!("knowflow-key-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("agent-key.bin");
            save(&path, "test-secret-123").unwrap();
            assert!(!fs::read(&path).unwrap().windows(15).any(|part| part == b"test-secret-123"));
            assert_eq!(load(&path).unwrap().as_deref(), Some("test-secret-123"));
            save(&path, "second-secret").unwrap();
            assert_eq!(load(&path).unwrap().as_deref(), Some("second-secret"));
            save(&path, "").unwrap();
            assert_eq!(load(&path).unwrap(), None);
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[cfg(windows)]
pub fn load(path: &Path) -> Result<Option<String>, String> { windows::load(path) }
#[cfg(windows)]
pub fn save(path: &Path, key: &str) -> Result<(), String> { windows::save(path, key) }

#[cfg(not(windows))]
pub fn load(_path: &Path) -> Result<Option<String>, String> { Ok(None) }
#[cfg(not(windows))]
pub fn save(_path: &Path, _key: &str) -> Result<(), String> { Ok(()) }
