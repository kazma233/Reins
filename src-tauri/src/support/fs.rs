use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use walkdir::WalkDir;

pub(crate) fn grok_home_path(
    grok_home: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    grok_home
        .map(PathBuf::from)
        .or_else(|| home.map(|path| path.join(".grok")))
}

// pi 的 agent 目录与 GROK_HOME 同一解析模式：优先环境变量重定向。
pub(crate) fn pi_agent_dir_path(
    pi_agent_dir: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    pi_agent_dir
        .map(PathBuf::from)
        .or_else(|| home.map(|path| path.join(".pi").join("agent")))
}

// dirs::home_dir() resolves through the known-folder API on Windows and
// ignores HOME/USERPROFILE, so environment redirection cannot isolate tests.
// They swap this override instead; production keeps the dirs behavior.
fn home_override() -> &'static Mutex<Option<PathBuf>> {
    static LOCK: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
pub(crate) fn set_home_override(home: Option<PathBuf>) {
    *home_override()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = home;
}

pub(crate) fn user_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        // 子进程继承不了进程内的 home_override 静态状态，而 Windows 上
        // dirs::home_dir() 又忽略 HOME；测试子进程改经环境变量传递临时
        // home（由 TestEnvGuard 设置）。仅测试二进制生效，生产路径不变。
        if let Some(home) = std::env::var_os("REINS_TEST_HOME") {
            return Some(PathBuf::from(home));
        }
    }
    let guard = home_override()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.clone().or_else(dirs::home_dir)
}

// Windows 的 std::fs::canonicalize 返回 \\?\ verbatim 路径,直接进展示字段会把
// 前缀暴露给用户,还会让"规范化结果 vs join 构造路径"的相等比较恒不相等。
// 全部调用方统一经此函数取普通拼写,相互比较才建立在同一形式上。
pub(crate) fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    #[cfg(windows)]
    {
        dunce::canonicalize(path)
    }
    #[cfg(not(windows))]
    {
        fs::canonicalize(path)
    }
}

// Windows accepts both path separators, so the same file can spell its path
// differently depending on who built the string (WalkDir yields '\', callers
// may pass '/'). Canonicalize when the file exists so path-keyed maps match
// regardless of separator spelling.
pub(crate) fn path_key(path: &Path) -> String {
    canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

// Write through a same-directory temp file + rename, so a crash mid-write can
// never leave a half-written file behind. std::fs::rename replaces existing
// files on Windows as well.
//
// 权限语义：rename 会用临时文件的权限覆盖目标，而临时文件是默认 umask
// 建出来的——新文件必须是 0600 的敏感文件（如 dsh 的 .credentials.yaml）
// 会被静默放宽为 0644，依赖方会在下次启动时拒绝加载。因此覆盖既有文件时
// 先复制目标的权限，敏感文件用 write_atomic_private 强制 owner-only。
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    write_atomic_with(path, contents, false)
}

// 敏感文件（凭据等）的原子写：无论新建还是覆盖都强制 0600（unix），
// 顺带修复被写坏权限的既有文件。非 unix 平台与 write_atomic 等价。
pub(crate) fn write_atomic_private(path: &Path, contents: &str) -> Result<()> {
    write_atomic_with(path, contents, true)
}

fn write_atomic_with(path: &Path, contents: &str, private: bool) -> Result<()> {
    use std::ffi::OsStr;

    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(directory)?;

    let file_name = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("reins.tmp");
    // Unique sibling name: concurrent writers must not clobber each other's
    // temp file, and the dot prefix keeps it out of glob listings.
    let tmp_path = directory.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));

    let result = fs::write(&tmp_path, contents)
        .and_then(|()| apply_mode(&tmp_path, path, private))
        .and_then(|()| fs::rename(&tmp_path, path));
    if result.is_err() {
        // Best-effort cleanup; a leftover temp file is harmless but noisy.
        let _ = fs::remove_file(&tmp_path);
    }
    // No context added here on purpose: every caller attaches its own
    // user-facing message, and stacking contexts would only obscure it.
    result?;

    Ok(())
}

#[cfg(unix)]
fn apply_mode(tmp_path: &Path, path: &Path, private: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if private {
        0o600
    } else {
        match fs::metadata(path) {
            // mode() 是原始 st_mode，可能带文件类型位，chmod 只吃权限位。
            Ok(metadata) => metadata.permissions().mode() & 0o7777,
            Err(_) => return Ok(()),
        }
    };

    fs::set_permissions(tmp_path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn apply_mode(_tmp_path: &Path, _path: &Path, _private: bool) -> std::io::Result<()> {
    Ok(())
}

// Windows 的规范化路径会附加扩展长度前缀；该前缀只服务于文件系统 API，
// 返回给界面会干扰阅读。
pub(crate) fn display_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
            return format!("\\\\{unc}");
        }
        return text.strip_prefix("\\\\?\\").unwrap_or(&text).to_string();
    }

    #[cfg(not(windows))]
    {
        path.display().to_string()
    }
}

pub(crate) fn enumerate_jsonl_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some("jsonl")
        {
            files.push(entry.into_path());
        }
    }

    Ok(files)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).expect("metadata").permissions().mode() & 0o7777
    }

    fn test_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("reins-fs-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("test dir");
        dir
    }

    // 覆盖既有文件时权限必须随目标保留:临时文件是 umask 默认权限,
    // 不处理后 rename 会把目标权限静默放宽。
    #[test]
    fn write_atomic_preserves_existing_permissions() {
        let dir = test_dir("preserve");
        let path = dir.join("config.yaml");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        write_atomic(&path, "new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(mode_of(&path), 0o640);
        fs::remove_dir_all(&dir).ok();
    }

    // 敏感文件无论覆盖还是新建都强制 0600:覆盖场景顺带修复被旧版本
    // 写坏的权限,dsh 的启动守卫要求凭据文件必须 owner-only。
    #[test]
    fn write_atomic_private_forces_owner_only() {
        let dir = test_dir("private");
        let existing = dir.join(".credentials.yaml");
        fs::write(&existing, "old").unwrap();
        fs::set_permissions(&existing, fs::Permissions::from_mode(0o644)).unwrap();

        write_atomic_private(&existing, "new").unwrap();
        assert_eq!(mode_of(&existing), 0o600);

        let fresh = dir.join("fresh-credentials.yaml");
        write_atomic_private(&fresh, "new").unwrap();
        assert_eq!(mode_of(&fresh), 0o600);
        fs::remove_dir_all(&dir).ok();
    }
}
