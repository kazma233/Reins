use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

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
pub(crate) mod sources;
pub(crate) mod summary_cache;
pub(crate) mod usage_day_cache;
pub(crate) mod usage_stats;
pub(crate) mod zcode;

use self::catalog::*;
use self::jsonl::*;
use self::model::*;
use self::text::*;
use self::usage_stats::{hour_key, merge_usage_bucket};

// Sync:来源注册表的 spec 会跨线程共享(按来源并行探测/统计),读取器自身
// 必须无内部可变性,并发安全靠各实现内部的 static Mutex 保证。
pub(crate) trait SessionReader: Sync {
    fn list_entries(&self) -> Result<Vec<SessionFileEntry>>;

    fn clear_cache(&self) -> Result<()> {
        Ok(())
    }

    fn resolve_path(&self, source_session_id: &str) -> Result<PathBuf>;

    fn parse_summary(&self, path: &Path) -> Result<SessionSummary>;

    fn parse_overview(&self, path: &Path) -> Result<SessionOverview>;

    fn parse_messages_page(
        &self,
        path: &Path,
        offset: usize,
        limit: usize,
    ) -> Result<SessionMessagePage>;

    fn parse_events_page(
        &self,
        path: &Path,
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
        _path: &Path,
        _agent_session_id: &str,
    ) -> Result<Vec<SessionMessage>> {
        bail!("该来源的子代理不以独立会话存储，无法按 agent session id 取消息")
    }
}

pub(crate) fn reader(source_app: SourceApp) -> &'static dyn SessionReader {
    sources::spec(source_app).reader
}

pub(crate) fn clear_all_caches() -> Result<()> {
    for spec in sources::SOURCES {
        spec.reader.clear_cache()?;
    }
    // 持久缓存一并清空：用户触发的刷新是"全量重建"的逃生通道。
    summary_cache::clear_all();
    usage_day_cache::clear_all();
    Ok(())
}

pub(crate) fn delete_session(source_app: SourceApp, path: &Path) -> Result<()> {
    match sources::spec(source_app).delete {
        sources::DeletePolicy::Deleter(delete) => delete(path),
        sources::DeletePolicy::Unsupported(reason) => bail!("{reason}"),
    }
}

fn sort_entries(entries: &mut [SessionFileEntry]) {
    entries.sort_by(|left, right| {
        right
            .sort_timestamp
            .cmp(&left.sort_timestamp)
            .then_with(|| left.path.cmp(&right.path))
    });
}
