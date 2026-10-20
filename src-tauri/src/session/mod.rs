use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

pub(crate) mod catalog;
pub(crate) mod commands;
pub(crate) mod delete;
pub(crate) mod jsonl;
pub(crate) mod model;
pub(crate) mod text;
pub(crate) mod timeline;

pub(crate) mod claude_code;
pub(crate) mod codex;
pub(crate) mod dsh;
pub(crate) mod family_index;
pub(crate) mod family_timeline;
pub(crate) mod grokbuild;
pub(crate) mod opencode;
pub(crate) mod pi;
pub(crate) mod reader_engine;
pub(crate) mod rows_cache;
pub(crate) mod sources;
pub(crate) mod summary_cache;
pub(crate) mod tool_blocks;
pub(crate) mod transcript;
pub(crate) mod usage_day_cache;
pub(crate) mod usage_stats;
pub(crate) mod zcode;

use self::catalog::*;
use self::jsonl::*;
use self::model::*;
use self::text::*;
use self::usage_stats::{hour_key, merge_usage_event};

// Sync:来源注册表的 spec 会跨线程共享(按来源并行探测/统计),读取器自身
// 必须无内部可变性,并发安全靠各实现内部的 static Mutex 保证。
// 会话身份是(来源, source_session_id)二元组:transcript_path 只是展示字段,
// 读取器接口不认路径。
pub(crate) trait SessionReader: Sync {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>>;

    fn clear_cache(&self) -> Result<()> {
        Ok(())
    }

    fn parse_summary(&self, source_session_id: &str) -> Result<SessionSummary>;

    fn parse_overview(&self, source_session_id: &str) -> Result<SessionOverview>;

    // 删除预演的结构化输入(DeletePlanInput):family 的非渲染折叠。不支持
    // 删除的来源走默认实现——注册表里的 Unsupported 分支不会调到这里,报错
    // 只是把"来源不支持"与"会话不存在"保持可区分。
    fn delete_plan_input(&self, _source_session_id: &str) -> Result<delete::DeletePlanInput> {
        bail!("该来源不支持删除")
    }

    fn parse_messages_page(
        &self,
        source_session_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SessionMessagePage>;

    fn parse_events_page(
        &self,
        source_session_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SessionEventPage>;

    // family 聚合来源（Codex/Claude Code/OpenCode）按 agent session id 取该子代理
    // 的全部消息。子代理入口弹窗用它,不再依赖分页已加载的范围。
    // Pi 的 subagent 内嵌在 toolResult.details 里、不产生子会话，其入口行走的是
    // 块内 payload 而非本方法；这里报错而不返回空，是为了让“来源不支持”
    // 与“子会话确实没消息”保持可区分。
    fn parse_agent_messages(
        &self,
        _source_session_id: &str,
        _agent_session_id: &str,
    ) -> Result<Vec<SessionMessage>> {
        bail!("该来源的子代理不以独立会话存储，无法按 agent session id 取消息")
    }
}

pub(crate) fn reader(source_app: SourceApp) -> &'static dyn SessionReader {
    sources::spec(source_app).reader
}

// SQLite 来源的 db 文件与数据表都随该来源首次写入才落地,表缺失只说明它还没
// 有会话。读取器据此把"没有数据"与"读取失败"分开,不把 no such table 报给用户。
pub(crate) fn sqlite_table_exists(connection: &rusqlite::Connection, table: &str) -> Result<bool> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .with_context(|| format!("Failed to inspect sqlite table {table}"))?;
    Ok(exists.is_some())
}

// 同上,列级探测:zcode/opencode 的归档列是后加的,老库没有该列只说明
// 该来源还没有归档这一状态,读取器据此跳过,而不是让整条列表报错。
pub(crate) fn sqlite_column_exists(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> Result<bool> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2",
            rusqlite::params![table, column],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })
        .with_context(|| format!("Failed to inspect sqlite column {table}.{column}"))?;
    Ok(exists.is_some())
}

// 来源会话根的统一解析:走 agents 清单的 GlobalRoot(重定向优先、HOME
// 兜底),与 workspace/providers 同一条路径来源——设了 CODEX_HOME 之类的
// 重定向后,三个域看到同一份数据。pi 例外:它的根有官方 sessionDir 链等
// 特有契约,留在 pi.rs(行为本身已认重定向)。
pub(crate) fn spec_root(app: SourceApp) -> Result<PathBuf> {
    let env = crate::support::environment::Environment::from_env();
    crate::agents::spec_by_source_app(app)
        .expect("registered in AGENTS")
        .global
        .root
        .resolve(&env)
        .ok_or_else(|| anyhow!("Unable to determine home directory"))
}

/// 重新扫盘:只丢内存层(目录索引、时间线、内存摘要),持久层不动。
/// 刷新与删除走这条:文件集合与 mtime 的变化由索引失效与缓存键自己发现。
pub(crate) fn rescan_all_caches() -> Result<()> {
    for spec in sources::SOURCES {
        spec.reader.clear_cache()?;
    }
    Ok(())
}

/// 全量重建的逃生通道(显式清缓存命令):持久层一并清空。
///
/// 持久摘要与用量缓存按 (路径, mtime) 自失效,清空它们并不是刷新正确性所需,
/// 而是"怀疑缓存本身"时的手段;代价随会话总量增长——本机 codex 侧清一次要
/// 重新解析 1329 个转录(27.6s),只丢内存层是 1.4s。解析逻辑变更走缓存文件
/// 版本号 bump(summary_cache::DB_FILE_NAME),不靠这条路径。
pub(crate) fn clear_all_caches() -> Result<()> {
    rescan_all_caches()?;
    // 持久摘要与行缓存跨来源共用同一份库文件，所以清一次即覆盖所有用到它的来源。
    summary_cache::SummaryDb::at_store_dir()?.clear_all();
    rows_cache::RowsDb::at_store_dir()?.clear_all();
    usage_day_cache::clear_all();
    Ok(())
}

pub(crate) fn delete_session(source_app: SourceApp, source_session_id: &str) -> Result<()> {
    match sources::spec(source_app).delete {
        sources::DeletePolicy::Deleter { delete, .. } => delete(source_session_id),
        sources::DeletePolicy::Unsupported { reason, .. } => bail!("{reason}"),
    }
}

// 文件集合的新鲜度指纹:(路径, mtime, 大小) 排序串接。stat 全集合远便宜于
// 解析,自管 freshness 的目录型来源(pi/grokbuild/dsh)用它门控各自的索引
// 缓存;口径只有这一份,各家不得再拼。
pub(crate) fn sessions_fingerprint(files: &[PathBuf]) -> Result<String> {
    let mut parts = Vec::with_capacity(files.len());
    for path in files {
        let metadata = std::fs::metadata(path)
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        parts.push(format!(
            "{}:{}:{}",
            path.display(),
            crate::support::time::file_modified_timestamp_millis(path)?,
            metadata.len()
        ));
    }
    parts.sort();
    Ok(parts.join("\n"))
}

fn sort_entries(entries: &mut [SessionFileEntry]) {
    entries.sort_by(|left, right| {
        right
            .sort_timestamp
            .cmp(&left.sort_timestamp)
            .then_with(|| left.path.cmp(&right.path))
    });
}
