//! 持久化用量小时桶缓存(SQLite)。
//!
//! 用量曲线按消耗事件的时间戳归桶,每个转录文件都要整文件解析一次才能拿到
//! 逐条 usage;这层缓存按 (来源, 路径, mtime) 存归桶结果,进程重启后 mtime
//! 未变的文件直接命中,只有新增或变更过的文件才重新解析。失效语义与
//! summary_cache 一致(信任文件 mtime)。
//!
//! 错误契约:缓存是加速层,转录文件才是权威数据源;读写失败记日志并按 miss
//! 处理(load 返回 None、store 放弃写入),不得阻断统计。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::model::{SourceApp, UsageHourBuckets};

// 归桶逻辑变更时(如 day→hour 粒度调整)必须 bump 文件名里的版本号:
// 旧版本库文件整体失效,新版本从空库全量重建一次。
const DB_FILE_NAME: &str = "usage-day-cache_v2.db";

static CONNECTION: LazyLock<Mutex<Option<(PathBuf, Connection)>>> =
    LazyLock::new(|| Mutex::new(None));

pub(crate) fn load(source_app: SourceApp, path: &Path, mtime: i64) -> Option<UsageHourBuckets> {
    with_connection(|connection| {
        let days_json = connection
            .query_row(
                "SELECT days FROM usage_day_cache WHERE source_app = ?1 AND path = ?2 AND mtime = ?3",
                rusqlite::params![source_app.as_str(), path_key(path), mtime],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| anyhow::anyhow!("usage day cache lookup failed: {error}"))?;

        serde_json::from_str(&days_json)
            .map_err(|error| anyhow::anyhow!("usage day cache row failed to parse: {error}"))
    })
    .ok()
}

pub(crate) fn store(source_app: SourceApp, path: &Path, mtime: i64, hours: &UsageHourBuckets) {
    let result = with_connection(|connection| {
        let hours_json = serde_json::to_string(hours)?;
        connection.execute(
            "INSERT INTO usage_day_cache (source_app, path, mtime, days) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source_app, path) DO UPDATE SET mtime = excluded.mtime, days = excluded.days",
            rusqlite::params![source_app.as_str(), path_key(path), mtime, hours_json],
        )?;
        Ok(())
    });

    if let Err(error) = result {
        crate::logger::log_error(format!("usage day cache store failed: {error}"));
    }
}

pub(crate) fn clear_all() {
    let result = with_connection(|connection| {
        connection.execute("DELETE FROM usage_day_cache", [])?;
        Ok(())
    });

    if let Err(error) = result {
        crate::logger::log_error(format!("usage day cache clear failed: {error}"));
    }
}

fn path_key(path: &Path) -> String {
    crate::support::fs::path_key(path)
}

fn db_path() -> Result<PathBuf> {
    Ok(crate::support::fs::user_home_dir()
        .context("Unable to determine home directory")?
        .join(".reins")
        .join(DB_FILE_NAME))
}

// 清掉旧版本号残留的库文件,避免 bump 后旧库永远占着磁盘。失败不影响
// 主流程——残留只是多占空间,不影响正确性。
fn remove_stale_db_files(dir: &Path) {
    const DB_FILE_STEM: &str = "usage-day-cache";

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
        .map_err(|_| anyhow::anyhow!("usage day cache connection lock was poisoned"))?;

    // 测试会切换 HOME 覆盖;路径变化时必须重开连接,避免读写到上一个环境的库。
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
            "CREATE TABLE IF NOT EXISTS usage_day_cache (
                source_app TEXT NOT NULL,
                path TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                days TEXT NOT NULL,
                PRIMARY KEY (source_app, path)
            )",
            [],
        )?;

        *guard = Some((path, connection));
    }

    operation(&guard.as_ref().expect("connection was just ensured").1)
}
