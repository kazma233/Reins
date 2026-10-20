//! 持久化目录行缓存(SQLite)。
//!
//! `list_rows` 是各来源冷启动最贵的一段:OpenCode 要对整个 session_message
//! 全表跑 json_extract 聚合,dsh 要逐转录整份解压,codex 要逐文件读首行。
//! 这份缓存按 (来源, 扫描根, freshness) 存行集合:进程重启后 freshness 未变
//! 时直接复用,跳过物理扫描。失效口径与引擎的内存目录索引完全一致(都信任
//! 各来源声明的 freshness),缓存只是把同一份结果延伸到进程生命周期之外。
//!
//! 行结构(字段增删或语义变化)变更时必须 bump 文件名里的版本号:旧行
//! 反序列化失败按 miss 处理会全量重建,新版本从空库开始。
//!
//! 错误契约:缓存是加速层,来源数据才是权威;任何读写失败都按 miss 处理
//! (load 返回 None、store 记日志放弃写入),不得阻断列表加载。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::model::SourceApp;

pub(crate) const DB_FILE_NAME: &str = "session-rows-cache_v1.db";

/// 实例级句柄:目录随读取器引擎实例注入(测试传临时目录),连接惰性打开。
pub(crate) struct RowsDb {
    dir: PathBuf,
    connection: Mutex<Option<Connection>>,
}

impl RowsDb {
    /// 生产路径的库目录(`~/.reins`)。
    pub(crate) fn at_store_dir() -> Result<Self> {
        let home =
            crate::support::fs::user_home_dir().context("Unable to determine home directory")?;
        Ok(Self::at(home.join(".reins")))
    }

    pub(crate) fn at(dir: PathBuf) -> Self {
        Self {
            dir,
            connection: Mutex::new(None),
        }
    }

    /// freshness 未变时返回持久化的行集合;查询失败或反序列化失败都按 miss。
    pub(crate) fn load<Row>(&self, app: SourceApp, root: &str, stamp: &str) -> Option<Vec<Row>>
    where
        Row: serde::de::DeserializeOwned,
    {
        let rows_json = self
            .with_connection(|connection| lookup_rows(connection, app, root, stamp))
            .ok()
            .flatten();
        rows_json.and_then(|json| serde_json::from_str(&json).ok())
    }

    pub(crate) fn store<Row>(&self, app: SourceApp, root: &str, stamp: &str, rows: &[Row])
    where
        Row: serde::Serialize,
    {
        let result = serde_json::to_string(rows)
            .map_err(anyhow::Error::new)
            .and_then(|rows_json| {
                self.with_connection(|connection| {
                    insert_rows(connection, app, root, stamp, &rows_json)
                })
            });

        if let Err(error) = result {
            crate::logger::log_error(format!("rows cache store failed: {error}"));
        }
    }

    /// 清空整表(显式清缓存命令的「全量重建」逃生通道)。
    pub(crate) fn clear_all(&self) {
        let result = self.with_connection(|connection| {
            connection.execute("DELETE FROM rows_cache", [])?;
            Ok(())
        });
        if let Err(error) = result {
            crate::logger::log_error(format!("rows cache clear failed: {error}"));
        }
    }

    fn with_connection<T>(&self, operation: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let mut guard = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("rows cache connection lock was poisoned"))?;

        if guard.is_none() {
            *guard = Some(open_connection(&self.dir)?);
        }

        operation(guard.as_ref().expect("connection was just ensured"))
    }
}

fn lookup_rows(
    connection: &Connection,
    app: SourceApp,
    root: &str,
    stamp: &str,
) -> Result<Option<String>> {
    let rows_json = connection
        .query_row(
            "SELECT rows FROM rows_cache WHERE app = ?1 AND root = ?2 AND stamp = ?3",
            rusqlite::params![app.as_str(), root, stamp],
            |row| row.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .with_context(|| "rows cache lookup failed".to_string())?;
    Ok(rows_json)
}

fn insert_rows(
    connection: &Connection,
    app: SourceApp,
    root: &str,
    stamp: &str,
    rows_json: &str,
) -> Result<()> {
    connection.execute(
        "INSERT INTO rows_cache (app, root, stamp, rows) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(app, root) DO UPDATE SET stamp = excluded.stamp, rows = excluded.rows",
        rusqlite::params![app.as_str(), root, stamp, rows_json],
    )?;
    Ok(())
}

fn open_connection(dir: &Path) -> Result<Connection> {
    fs::create_dir_all(dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    remove_stale_db_files(dir);

    let path = dir.join(DB_FILE_NAME);
    let connection =
        Connection::open(&path).with_context(|| format!("Failed to open {}", path.display()))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;

    connection.execute(
        "CREATE TABLE IF NOT EXISTS rows_cache (
            app TEXT NOT NULL,
            root TEXT NOT NULL,
            stamp TEXT NOT NULL,
            rows TEXT NOT NULL,
            PRIMARY KEY (app, root)
        )",
        [],
    )?;

    Ok(connection)
}

// 清掉旧版本残留的库文件,避免 bump 后旧库永远占着磁盘;失败不影响主流程。
fn remove_stale_db_files(dir: &Path) {
    const DB_FILE_STEM: &str = "session-rows-cache";

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

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<(String, i64)> {
        vec![("a".to_string(), 1), ("b".to_string(), 2)]
    }

    #[test]
    fn store_then_load_round_trips_and_stamp_gates_hits() {
        let dir = std::env::temp_dir().join(format!("reins-rows-cache-{}", uuid::Uuid::new_v4()));
        let db = RowsDb::at(dir.clone());
        let loaded: Option<Vec<(String, i64)>> =
            db.load(SourceApp::Codex, "/scan/root", "stamp:1");
        assert!(loaded.is_none(), "empty cache misses");

        db.store(SourceApp::Codex, "/scan/root", "stamp:1", &rows());
        let hit: Vec<(String, i64)> = db
            .load(SourceApp::Codex, "/scan/root", "stamp:1")
            .expect("cache hit");
        assert_eq!(hit, rows());

        // freshness 变化或换扫描根都视为 miss。
        assert!(db
            .load::<(String, i64)>(SourceApp::Codex, "/scan/root", "stamp:2")
            .is_none());
        assert!(db
            .load::<(String, i64)>(SourceApp::Codex, "/other/root", "stamp:1")
            .is_none());
        // 来源不同也不共享行。
        assert!(db
            .load::<(String, i64)>(SourceApp::Dsh, "/scan/root", "stamp:1")
            .is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_all_invalidates_every_row() {
        let dir = std::env::temp_dir().join(format!("reins-rows-cache-{}", uuid::Uuid::new_v4()));
        let db = RowsDb::at(dir.clone());
        db.store(SourceApp::Codex, "/scan/root", "stamp:1", &rows());
        db.clear_all();

        assert!(db
            .load::<(String, i64)>(SourceApp::Codex, "/scan/root", "stamp:1")
            .is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deserialization_failure_is_a_miss() {
        let dir = std::env::temp_dir().join(format!("reins-rows-cache-{}", uuid::Uuid::new_v4()));
        let db = RowsDb::at(dir.clone());
        // 存入的形状与请求的类型不符,读出按 miss 而不是报错。
        db.store(SourceApp::Codex, "/scan/root", "stamp:1", &rows());
        assert!(db
            .load::<String>(SourceApp::Codex, "/scan/root", "stamp:1")
            .is_none());

        std::fs::remove_dir_all(&dir).ok();
    }
}
