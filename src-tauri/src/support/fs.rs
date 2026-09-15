use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use walkdir::WalkDir;

pub(crate) fn grok_home_path(
    grok_home: Option<std::ffi::OsString>,
    home: Option<PathBuf>,
) -> Option<PathBuf> {
    grok_home
        .map(PathBuf::from)
        .or_else(|| home.map(|path| path.join(".grok")))
}

pub(crate) fn effective_cwd(cwd: Option<&str>) -> Result<String> {
    if let Some(cwd) = cwd {
        return Ok(cwd.to_string());
    }

    Ok(user_home_dir()
        .context("Unable to determine home directory")?
        .display()
        .to_string())
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
    let guard = home_override()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.clone().or_else(dirs::home_dir)
}

pub(crate) fn write_jsonl_file(path: &Path, lines: &[Value]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let mut file =
        File::create(path).with_context(|| format!("Failed to create {}", path.display()))?;

    for line in lines {
        serde_json::to_writer(&mut file, line)?;
        writeln!(&mut file)?;
    }

    Ok(())
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
