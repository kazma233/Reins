//! 持久化会话摘要缓存（SQLite）。
//!
//! 首次加载时 codex/claude 的每个会话根转录文件要整文件解析一次才能拿到
//! 标题（如用户机器上 1233 个文件共 2.1GB）。这份缓存按 (来源, 路径, mtime)
//! 存解析结果：进程重启后 mtime 未变的文件直接命中缓存，只有新增或变更过的
//! 文件才重新解析。缓存键的失效语义与各后端的内存摘要缓存完全一致（都信任
//! 文件 mtime），缓存只是把同一份结果延伸到了进程生命周期之外。
//!
//! 错误契约：缓存是加速层，转录文件才是权威数据源；任何缓存读写失败都记日志
//! 并按 miss 处理（load 返回 None、store 放弃写入），不得阻断列表加载。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::model::{SessionSummary, SourceApp};

// 解析逻辑变更时（如标题提取规则调整）必须 bump 文件名里的版本号：
// 旧版本库文件整体失效，新版本从空库全量重建一次。
const DB_FILE_NAME: &str = "session-summary-cache_v2.db";

static CONNECTION: LazyLock<Mutex<Option<(PathBuf, Connection)>>> =
    LazyLock::new(|| Mutex::new(None));

pub(crate) fn load(source_app: SourceApp, path: &Path, mtime: i64) -> Option<SessionSummary> {
    with_connection(|connection| {
        let summary_json = connection
            .query_row(
                "SELECT summary FROM session_summary_cache WHERE source_app = ?1 AND path = ?2 AND mtime = ?3",
                rusqlite::params![source_app.as_str(), path_key(path), mtime],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| anyhow::anyhow!("summary cache lookup failed: {error}"))?;

        serde_json::from_str(&summary_json)
            .map_err(|error| anyhow::anyhow!("summary cache row failed to parse: {error}"))
    })
    .ok()
}

pub(crate) fn store(source_app: SourceApp, path: &Path, mtime: i64, summary: &SessionSummary) {
    let result = with_connection(|connection| {
        let summary_json = serde_json::to_string(summary)?;
        connection.execute(
            "INSERT INTO session_summary_cache (source_app, path, mtime, summary) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source_app, path) DO UPDATE SET mtime = excluded.mtime, summary = excluded.summary",
            rusqlite::params![source_app.as_str(), path_key(path), mtime, summary_json],
        )?;
        Ok(())
    });

    if let Err(error) = result {
        crate::logger::log_error(format!("summary cache store failed: {error}"));
    }
}

pub(crate) fn clear_all() {
    let result = with_connection(|connection| {
        connection.execute("DELETE FROM session_summary_cache", [])?;
        Ok(())
    });

    if let Err(error) = result {
        crate::logger::log_error(format!("summary cache clear failed: {error}"));
    }
}

// 与 family 索引的 path map 一致，用 canonicalize 后的路径做键，
// 避免同一文件因分隔符拼写不同而重复缓存。
fn path_key(path: &Path) -> String {
    crate::support::fs::path_key(path)
}

fn db_path() -> Result<PathBuf> {
    Ok(crate::support::fs::user_home_dir()
        .context("Unable to determine home directory")?
        .join(".reins")
        .join(DB_FILE_NAME))
}

// 清掉旧版本（含版本化之前的无后缀命名）残留的库文件，避免 bump 后旧库
// 永远占着磁盘。失败不影响主流程——残留只是多占空间，不影响正确性。
fn remove_stale_db_files(dir: &Path) {
    const DB_FILE_STEM: &str = "session-summary-cache";

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };

        let is_cache_db = name
            .strip_suffix("-wal")
            .or_else(|| name.strip_suffix("-shm"))
            .unwrap_or(name)
            .starts_with(DB_FILE_STEM);

        if is_cache_db && !name.starts_with(DB_FILE_NAME) {
            fs::remove_file(entry.path()).ok();
        }
    }
}

fn with_connection<T>(operation: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let path = db_path()?;
    let mut guard = CONNECTION
        .lock()
        .map_err(|_| anyhow::anyhow!("summary cache connection lock was poisoned"))?;

    // 测试会切换 HOME 覆盖；路径变化时必须重开连接，避免读写到上一个环境的库。
    if guard
        .as_ref()
        .is_some_and(|(cached_path, _)| *cached_path != path)
    {
        *guard = None;
    }

    if guard.is_none() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
            remove_stale_db_files(parent);
        }

        let connection = Connection::open(&path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;

        connection.execute(
            "CREATE TABLE IF NOT EXISTS session_summary_cache (
                source_app TEXT NOT NULL,
                path TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                summary TEXT NOT NULL,
                PRIMARY KEY (source_app, path)
            )",
            [],
        )?;

        *guard = Some((path, connection));
    }

    operation(&guard.as_ref().expect("connection was just ensured").1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(title: &str) -> SessionSummary {
        SessionSummary {
            source_app: SourceApp::Codex,
            source_session_id: "test-session".to_string(),
            title: title.to_string(),
            cwd: None,
            git_branch: None,
            transcript_path: "/tmp/does-not-exist.jsonl".to_string(),
            created_at: Some(1),
            updated_at: Some(2),
        }
    }

    #[test]
    fn store_then_load_round_trips_and_mtime_gates_hits() {
        let temp_home = std::env::temp_dir().join(format!("reins-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_home).unwrap();
        let _guard = crate::test_support::TestEnvGuard::set_home(&temp_home);

        let path = std::path::Path::new("/tmp/some-transcript.jsonl");

        // mtime 未变时命中，变化后失效。
        store(SourceApp::Codex, path, 100, &summary("cached title"));
        let loaded = load(SourceApp::Codex, path, 100).expect("cache hit");
        assert_eq!(loaded.title, "cached title");
        assert!(load(SourceApp::Codex, path, 101).is_none());

        // 覆盖写入后按新 mtime 命中。
        store(SourceApp::Codex, path, 200, &summary("updated title"));
        let reloaded = load(SourceApp::Codex, path, 200).expect("cache hit after update");
        assert_eq!(reloaded.title, "updated title");
        assert!(load(SourceApp::Codex, path, 100).is_none());

        std::fs::remove_dir_all(&temp_home).ok();
    }

    #[test]
    fn rows_are_isolated_per_source_app() {
        let temp_home = std::env::temp_dir().join(format!("reins-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_home).unwrap();
        let _guard = crate::test_support::TestEnvGuard::set_home(&temp_home);

        let path = std::path::Path::new("/tmp/some-transcript.jsonl");
        store(SourceApp::Codex, path, 100, &summary("codex title"));

        assert!(load(SourceApp::ClaudeCode, path, 100).is_none());

        std::fs::remove_dir_all(&temp_home).ok();
    }

    #[test]
    fn clear_all_invalidates_every_row() {
        let temp_home = std::env::temp_dir().join(format!("reins-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_home).unwrap();
        let _guard = crate::test_support::TestEnvGuard::set_home(&temp_home);

        let path = std::path::Path::new("/tmp/some-transcript.jsonl");
        store(SourceApp::Codex, path, 100, &summary("codex title"));
        clear_all();

        assert!(load(SourceApp::Codex, path, 100).is_none());

        std::fs::remove_dir_all(&temp_home).ok();
    }

    #[test]
    fn opening_current_db_removes_stale_version_files() {
        let temp_home = std::env::temp_dir().join(format!("reins-test-{}", uuid::Uuid::new_v4()));
        let cache_dir = temp_home.join(".reins");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let _guard = crate::test_support::TestEnvGuard::set_home(&temp_home);

        // 版本化之前的无后缀命名和旧版本号文件都应在打开当前库时被清掉。
        for stale in [
            "session-summary-cache.db",
            "session-summary-cache.db-wal",
            "session-summary-cache_v1.db",
            "session-summary-cache_v1.db-shm",
        ] {
            std::fs::write(cache_dir.join(stale), b"stale").unwrap();
        }

        let path = std::path::Path::new("/tmp/some-transcript.jsonl");
        store(SourceApp::Codex, path, 100, &summary("codex title"));

        let remaining = std::fs::read_dir(&cache_dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            remaining.iter().all(|name| name.starts_with(DB_FILE_NAME)),
            "stale cache db files survived: {remaining:?}"
        );

        std::fs::remove_dir_all(&temp_home).ok();
    }
}
