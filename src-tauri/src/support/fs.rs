use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Result, anyhow};
use walkdir::WalkDir;

pub(crate) fn grok_home_path(
    grok_home: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    grok_home
        .map(PathBuf::from)
        .or_else(|| home.map(|path| path.join(".grok")))
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

// Windows accepts both path separators, so the same file can spell its path
// differently depending on who built the string (WalkDir yields '\', callers
// may pass '/'). Canonicalize when the file exists so path-keyed maps match
// regardless of separator spelling.
pub(crate) fn path_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

// Write through a same-directory temp file + rename, so a crash mid-write can
// never leave a half-written file behind. std::fs::rename replaces existing
// files on Windows as well.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<()> {
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

    let result = fs::write(&tmp_path, contents).and_then(|()| fs::rename(&tmp_path, path));
    if result.is_err() {
        // Best-effort cleanup; a leftover temp file is harmless but noisy.
        let _ = fs::remove_file(&tmp_path);
    }
    // No context added here on purpose: every caller attaches its own
    // user-facing message, and stacking contexts would only obscure it.
    result?;

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

pub(crate) fn find_session_file(root: &Path, source_session_id: &str) -> Result<PathBuf> {
    enumerate_jsonl_files(root)?
        .into_iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(source_session_id))
        })
        .ok_or_else(|| anyhow!("Could not find session file for {source_session_id}"))
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
