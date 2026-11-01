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
            connection.execute("DELETE FROM row_contrib", [])?;
            Ok(())
        });
        if let Err(error) = result {
            crate::logger::log_error(format!("rows cache clear failed: {error}"));
        }
    }

    /// 行级贡献表:来源聚合类扫描按 (app, root, 行id) 存单行贡献,行级
    /// 失效键 stamp 由来源声明(如 opencode 的消息 time_updated)。返回
    /// (id, stamp, payload);读失败按空集处理,调用方退回全量重提取,
    /// 权威数据始终是来源自身。
    pub(crate) fn load_contribs(&self, app: SourceApp, root: &str) -> Vec<(String, i64, String)> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, stamp, payload FROM row_contrib WHERE app = ?1 AND root = ?2",
            )?;
            let rows = statement.query_map(rusqlite::params![app.as_str(), root], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            let collected =
                rows.collect::<std::result::Result<Vec<(String, i64, String)>, rusqlite::Error>>()?;
            Ok(collected)
        })
        .unwrap_or_else(|error| {
            crate::logger::log_error(format!("row contrib load failed: {error}"));
            Vec::new()
        })
    }

    /// 贡献表事务写回:upsert 变化行、删除消失行;失败记日志放弃,下轮
    /// 按 stamp diff 自然补偿。
    pub(crate) fn sync_contribs(
        &self,
        app: SourceApp,
        root: &str,
        upserts: &[(String, i64, String)],
        removed_ids: &[String],
    ) {
        let result = self.with_connection(|connection| {
            let transaction = connection.transaction()?;
            {
                let mut statement = transaction.prepare(
                    "INSERT INTO row_contrib (app, root, id, stamp, payload)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(app, root, id) DO UPDATE SET stamp = excluded.stamp, payload = excluded.payload",
                )?;
                for (id, stamp, payload) in upserts {
                    statement.execute(rusqlite::params![app.as_str(), root, id, stamp, payload])?;
                }
            }
            if !removed_ids.is_empty() {
                // 单条 DELETE IN 同样受绑定变量上限约束:来源库整体清空/替换
                // 时 removed 是全量,超限会让整个事务(含 upsert)回滚,缓存
                // 从此永远写不进去,按批删除。
                for batch in removed_ids.chunks(super::SQL_PARAMETER_BATCH) {
                    let placeholders = vec!["?"; batch.len()].join(",");
                    transaction.execute(
                        &format!("DELETE FROM row_contrib WHERE app = ?1 AND root = ?2 AND id IN ({placeholders})"),
                        rusqlite::params_from_iter(
                            [app.as_str().to_string(), root.to_string()]
                                .into_iter()
                                .chain(batch.iter().cloned()),
                        ),
                    )?;
                }
            }
            transaction.commit()?;
            Ok(())
        });

        if let Err(error) = result {
            crate::logger::log_error(format!("row contrib sync failed: {error}"));
        }
    }

    fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T>,
    ) -> Result<T> {
        let mut guard = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("rows cache connection lock was poisoned"))?;

        if guard.is_none() {
            *guard = Some(open_connection(&self.dir)?);
        }

        operation(guard.as_mut().expect("connection was just ensured"))
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
    // 行级贡献表:rows_cache 存整来源行集合(单键),row_contrib 存来源自己
    // 声明的行级贡献(多键),同一库文件、同一失效信任模型。
    connection.execute(
        "CREATE TABLE IF NOT EXISTS row_contrib (
            app TEXT NOT NULL,
            root TEXT NOT NULL,
            id TEXT NOT NULL,
            stamp INTEGER NOT NULL,
            payload TEXT NOT NULL,
            PRIMARY KEY (app, root, id)
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
        let loaded: Option<Vec<(String, i64)>> = db.load(SourceApp::Codex, "/scan/root", "stamp:1");
        assert!(loaded.is_none(), "empty cache misses");

        db.store(SourceApp::Codex, "/scan/root", "stamp:1", &rows());
        let hit: Vec<(String, i64)> = db
            .load(SourceApp::Codex, "/scan/root", "stamp:1")
            .expect("cache hit");
        assert_eq!(hit, rows());

        // freshness 变化或换扫描根都视为 miss。
        assert!(
            db.load::<(String, i64)>(SourceApp::Codex, "/scan/root", "stamp:2")
                .is_none()
        );
        assert!(
            db.load::<(String, i64)>(SourceApp::Codex, "/other/root", "stamp:1")
                .is_none()
        );
        // 来源不同也不共享行。
        assert!(
            db.load::<(String, i64)>(SourceApp::Dsh, "/scan/root", "stamp:1")
                .is_none()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_all_invalidates_every_row() {
        let dir = std::env::temp_dir().join(format!("reins-rows-cache-{}", uuid::Uuid::new_v4()));
        let db = RowsDb::at(dir.clone());
        db.store(SourceApp::Codex, "/scan/root", "stamp:1", &rows());
        db.clear_all();

        assert!(
            db.load::<(String, i64)>(SourceApp::Codex, "/scan/root", "stamp:1")
                .is_none()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deserialization_failure_is_a_miss() {
        let dir = std::env::temp_dir().join(format!("reins-rows-cache-{}", uuid::Uuid::new_v4()));
        let db = RowsDb::at(dir.clone());
        // 存入的形状与请求的类型不符,读出按 miss 而不是报错。
        db.store(SourceApp::Codex, "/scan/root", "stamp:1", &rows());
        assert!(
            db.load::<String>(SourceApp::Codex, "/scan/root", "stamp:1")
                .is_none()
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn contrib_round_trip_stamp_gates_and_removal() {
        let dir = std::env::temp_dir().join(format!("reins-rows-cache-{}", uuid::Uuid::new_v4()));
        let db = RowsDb::at(dir.clone());

        // 空表按空集处理,不报错。
        assert!(db.load_contribs(SourceApp::OpenCode, "/db").is_empty());

        db.sync_contribs(
            SourceApp::OpenCode,
            "/db",
            &[
                ("m1".to_string(), 10, r#"{"usage":1}"#.to_string()),
                ("m2".to_string(), 20, r#"{"usage":2}"#.to_string()),
            ],
            &[],
        );
        let loaded = db.load_contribs(SourceApp::OpenCode, "/db");
        assert_eq!(loaded.len(), 2);

        // stamp 变化的行 upsert 覆盖,消失的行删除,未变的行原样保留。
        db.sync_contribs(
            SourceApp::OpenCode,
            "/db",
            &[("m1".to_string(), 11, r#"{"usage":3}"#.to_string())],
            &["m2".to_string()],
        );
        let loaded = db.load_contribs(SourceApp::OpenCode, "/db");
        assert_eq!(
            loaded,
            vec![("m1".to_string(), 11, r#"{"usage":3}"#.to_string())]
        );

        // 来源或扫描根不同不共享贡献。
        db.sync_contribs(
            SourceApp::OpenCode,
            "/db",
            &[("m3".to_string(), 1, "{}".to_string())],
            &[],
        );
        assert_eq!(db.load_contribs(SourceApp::Zcode, "/db").len(), 0);
        assert_eq!(db.load_contribs(SourceApp::OpenCode, "/other").len(), 0);

        // 显式清缓存连贡献表一起清。
        db.clear_all();
        assert!(db.load_contribs(SourceApp::OpenCode, "/db").is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    // 来源库整体清空/替换时 removed 是全量缓存行;超过 SQLite 绑定变量上限
    // 的单条 DELETE IN 会让整个事务回滚,缓存从此永远写不进去,必须分批删净。
    #[test]
    fn sync_removes_beyond_sql_parameter_limit_in_batches() {
        let dir = std::env::temp_dir().join(format!("reins-rows-cache-{}", uuid::Uuid::new_v4()));
        let db = RowsDb::at(dir.clone());

        let count = 32767;
        let upserts: Vec<(String, i64, String)> = (0..count)
            .map(|index| (format!("m{index}"), index as i64, "{}".to_string()))
            .collect();
        db.sync_contribs(SourceApp::OpenCode, "/db", &upserts, &[]);
        assert_eq!(db.load_contribs(SourceApp::OpenCode, "/db").len(), count);

        let removed: Vec<String> = (0..count).map(|index| format!("m{index}")).collect();
        db.sync_contribs(SourceApp::OpenCode, "/db", &[], &removed);
        assert!(
            db.load_contribs(SourceApp::OpenCode, "/db").is_empty(),
            "batched DELETE must clear every row"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
