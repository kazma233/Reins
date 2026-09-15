use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

pub(crate) mod catalog;
pub(crate) mod commands;
pub(crate) mod delete;
pub(crate) mod import;
pub(crate) mod jsonl;
pub(crate) mod model;
pub(crate) mod text;
pub(crate) mod timeline;

pub(crate) mod claude_code;
pub(crate) mod codex;
pub(crate) mod family_index;
pub(crate) mod family_timeline;
pub(crate) mod opencode;
pub(crate) mod grokbuild;
pub(crate) mod pi;
pub(crate) mod summary_cache;

use self::catalog::*;
use self::family_timeline::*;
use self::jsonl::*;
use self::model::*;
use self::text::*;
use self::timeline::*;

pub(crate) trait SessionReader {
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

    fn parse_detail(&self, path: &Path) -> Result<SessionDetail>;

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

pub(crate) trait SessionExporter {
    fn planned_import_paths(
        &self,
        summary: &SessionSummary,
        session_id: &str,
    ) -> Result<Vec<String>>;

    fn export_session(
        &self,
        detail: &SessionDetail,
        new_session_id: &str,
    ) -> Result<(String, Vec<String>)>;
}

pub(crate) fn reader(source_app: SourceApp) -> &'static dyn SessionReader {
    match source_app {
        SourceApp::Codex => &codex::BACKEND,
        SourceApp::ClaudeCode => &claude_code::BACKEND,
        SourceApp::OpenCode => &opencode::BACKEND,
        SourceApp::Pi => &pi::BACKEND,
        SourceApp::GrokBuild => &grokbuild::BACKEND,
    }
}

pub(crate) fn clear_all_caches() -> Result<()> {
    reader(SourceApp::Codex).clear_cache()?;
    reader(SourceApp::ClaudeCode).clear_cache()?;
    reader(SourceApp::OpenCode).clear_cache()?;
    reader(SourceApp::Pi).clear_cache()?;
    reader(SourceApp::GrokBuild).clear_cache()?;
    // 持久缓存一并清空：用户触发的刷新是"全量重建"的逃生通道。
    summary_cache::clear_all();
    Ok(())
}

pub(crate) fn exporter(source_app: SourceApp) -> Result<&'static dyn SessionExporter> {
    Ok(match source_app {
        SourceApp::Codex => &codex::BACKEND,
        SourceApp::ClaudeCode => &claude_code::BACKEND,
        SourceApp::OpenCode => &opencode::BACKEND,
        SourceApp::Pi => &pi::BACKEND,
        SourceApp::GrokBuild => bail!("Import to Grok Build is unsupported"),
    })
}

pub(crate) fn delete_session(source_app: SourceApp, path: &Path) -> Result<()> {
    match source_app {
        SourceApp::Codex => codex::delete_session(path),
        SourceApp::ClaudeCode => claude_code::delete_session(path),
        SourceApp::OpenCode => opencode::delete_session(path),
        SourceApp::Pi => pi::delete_session(path),
        SourceApp::GrokBuild => bail!("Grok Build session deletion is unsupported"),
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

// 导出时外层 entry timestamp 跟随消息时间;同值或倒序时前进 1ms,保证文件内单调。
pub(crate) struct ExportClock {
    last_ms: i64,
}

impl ExportClock {
    pub(crate) fn new(seed_ms: i64) -> Self {
        Self {
            last_ms: seed_ms.saturating_sub(1),
        }
    }

    pub(crate) fn next_iso(&mut self, desired_ms: i64) -> String {
        self.last_ms = desired_ms.max(self.last_ms.saturating_add(1));
        crate::support::time::utc_timestamp_from_millis(self.last_ms)
    }

    pub(crate) fn last_ms(&self) -> i64 {
        self.last_ms
    }
}
