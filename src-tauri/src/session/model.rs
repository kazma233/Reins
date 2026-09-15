use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

// Contract types below cross the Tauri invoke boundary, so ts-rs exports their
// TypeScript bindings straight into the frontend tree (src/features/sessions/
// generated). The path is relative to TS_RS_EXPORT_DIR (./bindings under
// src-tauri), keeping the generated types next to the code that consumes them
// so the hand-written mirrors cannot drift from serde output.

pub(crate) const IMPORTER_VERSION: &str = "reins/0.1.0";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) enum SourceApp {
    Codex,
    ClaudeCode,
    #[serde(rename = "opencode")]
    OpenCode,
    Pi,
    #[serde(rename = "grokbuild")]
    GrokBuild,
}

impl FromStr for SourceApp {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "codex" => Ok(Self::Codex),
            "claude_code" => Ok(Self::ClaudeCode),
            "opencode" => Ok(Self::OpenCode),
            "pi" => Ok(Self::Pi),
            "grokbuild" => Ok(Self::GrokBuild),
            _ => Err(anyhow!("Unsupported source app: {value}")),
        }
    }
}

impl SourceApp {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::ClaudeCode => "claude_code",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
            Self::GrokBuild => "grokbuild",
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SourceStatus {
    pub(crate) app: SourceApp,
    pub(crate) available: bool,
    pub(crate) root_path: Option<String>,
    pub(crate) session_count: usize,
    pub(crate) note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionSummary {
    pub(crate) source_app: SourceApp,
    pub(crate) source_session_id: String,
    pub(crate) title: String,
    pub(crate) cwd: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) transcript_path: String,
    // i64 timestamps travel as JSON numbers over invoke; pin them to `number`
    // instead of ts-rs' default bigint mapping.
    #[ts(type = "number | null")]
    pub(crate) created_at: Option<i64>,
    #[ts(type = "number | null")]
    pub(crate) updated_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct ContentBlock {
    pub(crate) kind: String,
    pub(crate) text: Option<String>,
    pub(crate) tool_name: Option<String>,
    pub(crate) tool_call_id: Option<String>,
    // Pi toolResult 等来源携带工具失败标记,跨来源导出时需要原样透传。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) is_error: Option<bool>,
    // A recursive JsonValue binding would make Vue's UnwrapRef (used by every
    // ref<SessionMessage[]>) recurse in turn and blow up with TS2589, so the
    // payload stays opaque, matching the hand-written contract.
    #[ts(type = "unknown | null")]
    pub(crate) payload: Option<Value>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionMessage {
    pub(crate) id: String,
    pub(crate) role: String,
    #[ts(type = "number | null")]
    pub(crate) timestamp: Option<i64>,
    pub(crate) blocks: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) session_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionEvent {
    pub(crate) id: String,
    pub(crate) kind: String,
    #[ts(type = "number | null")]
    pub(crate) timestamp: Option<i64>,
    pub(crate) summary: String,
    // See ContentBlock::payload for why this stays `unknown`.
    #[ts(type = "unknown | null")]
    pub(crate) payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) session_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionAgent {
    pub(crate) session_id: String,
    pub(crate) label: String,
    pub(crate) is_root: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionDetail {
    pub(crate) summary: SessionSummary,
    pub(crate) source_paths: Vec<String>,
    pub(crate) messages: Vec<SessionMessage>,
    pub(crate) events: Vec<SessionEvent>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionOverview {
    pub(crate) summary: SessionSummary,
    pub(crate) source_paths: Vec<String>,
    pub(crate) message_count: Option<usize>,
    pub(crate) event_count: Option<usize>,
    pub(crate) agents: Vec<SessionAgent>,
}

#[derive(Clone, Copy, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) enum ImportLevel {
    /// Contract placeholder for the planned full-fidelity import; the current
    /// importer only ever produces Partial or Unsupported.
    #[allow(dead_code)]
    Full,
    Partial,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct ImportPreview {
    pub(crate) source_app: SourceApp,
    pub(crate) source_session_id: String,
    pub(crate) target_app: SourceApp,
    pub(crate) supported: bool,
    pub(crate) import_level: ImportLevel,
    pub(crate) warnings: Vec<String>,
    pub(crate) created_paths: Vec<String>,
    pub(crate) backup_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct ImportResult {
    pub(crate) target_app: SourceApp,
    pub(crate) created_session_id: String,
    pub(crate) created_paths: Vec<String>,
    pub(crate) backup_paths: Vec<String>,
    pub(crate) resume_cwd: Option<String>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct DeleteSessionResult {
    pub(crate) source_app: SourceApp,
    pub(crate) deleted_session_id: String,
    pub(crate) deleted_paths: Vec<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionMessagePage {
    pub(crate) messages: Vec<SessionMessage>,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) next_offset: Option<usize>,
    pub(crate) total_count: usize,
    pub(crate) has_more: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionEventPage {
    pub(crate) events: Vec<SessionEvent>,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) next_offset: Option<usize>,
    pub(crate) total_count: usize,
    pub(crate) has_more: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionPage {
    // None 表示合并视图（跨来源分页），Some 表示单一来源。
    pub(crate) source_app: Option<SourceApp>,
    pub(crate) sessions: Vec<SessionSummary>,
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) next_offset: Option<usize>,
    pub(crate) total_count: usize,
    pub(crate) has_more: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionRefreshResult {
    // None 表示合并视图（跨来源分页）。
    pub(crate) selected_source: Option<SourceApp>,
    pub(crate) sources: Vec<SourceStatus>,
    pub(crate) page: SessionPage,
}

#[derive(Clone, Debug)]
pub(crate) struct SessionFileEntry {
    pub(crate) path: PathBuf,
    pub(crate) sort_timestamp: i64,
    pub(crate) summary: Option<SessionSummary>,
}

pub(crate) enum TimelineRecord {
    Message(SessionMessage),
    Event(SessionEvent),
}

#[derive(Default)]
pub(crate) struct SummaryAccumulator {
    pub(crate) session_id: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) created_at: Option<i64>,
    pub(crate) updated_at: Option<i64>,
}
