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
    #[serde(rename = "zcode")]
    Zcode,
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
            "zcode" => Ok(Self::Zcode),
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
            Self::Zcode => "zcode",
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

// 各来源转录的 usage 字段口径不一(Codex 的 input 含缓存命中、OpenCode 把
// reasoning 与 output 分开),读取器负责归一到这四项后再进入统计。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionTokenUsage {
    #[ts(type = "number")]
    pub(crate) input_tokens: u64,
    // 输出统一含 reasoning tokens,否则跨来源不可比。
    #[ts(type = "number")]
    pub(crate) output_tokens: u64,
    #[ts(type = "number")]
    pub(crate) cache_read_tokens: u64,
    #[ts(type = "number")]
    pub(crate) cache_write_tokens: u64,
}

impl SessionTokenUsage {
    pub(crate) fn accumulate(&mut self, next: &Self) {
        self.input_tokens += next.input_tokens;
        self.output_tokens += next.output_tokens;
        self.cache_read_tokens += next.cache_read_tokens;
        self.cache_write_tokens += next.cache_write_tokens;
    }
}

// 用量统计的日聚合点:day 是本地时区 YYYY-MM-DD,由消耗事件(消息/turn/
// token_count 事件)的时间戳归桶而来,跨天会话天然按天拆分。
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/usage/generated/")]
pub(crate) struct UsageDayPoint {
    pub(crate) day: String,
    pub(crate) usage: SessionTokenUsage,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/usage/generated/")]
pub(crate) struct UsageSourceStats {
    pub(crate) source_app: SourceApp,
    pub(crate) days: Vec<UsageDayPoint>,
    // 今日窗口的小时序列:本地时区今天有消耗的小时,前端补齐 0..23。
    pub(crate) today_hours: Vec<UsageHourPoint>,
    // 有消耗记录的会话数;jsonl 来源按产出日桶的转录文件计(resume 段与
    // subagent 线程是独立文件,与列表页 family 求和口径一致)。
    pub(crate) session_count: usize,
    // 今日产生消耗的会话数,今日窗口的会话指标用它而不是全周期值。
    pub(crate) today_session_count: usize,
}

// 今日窗口的小时点;hour 是本地时区 0..23。
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/usage/generated/")]
pub(crate) struct UsageHourPoint {
    pub(crate) hour: u8,
    pub(crate) usage: SessionTokenUsage,
}

// 全量日序列是唯一事实源:时间窗裁剪与指标推导都在前端完成。
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/usage/generated/")]
pub(crate) struct UsageStats {
    // 只含当前可用且有数据的来源。
    pub(crate) sources: Vec<UsageSourceStats>,
}

// 单个转录文件(或 SQL 来源)的小时桶中间形态;day 序列与 today_hours 都由
// 它聚合而来。key 形如 "YYYY-MM-DDTHH"(本地时区),BTreeMap 保证升序。
pub(crate) type UsageHourBuckets = std::collections::BTreeMap<String, SessionTokenUsage>;

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
    // 列表扫描时聚合;无 usage 数据的来源(GrokBuild)与旧缓存行为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) token_usage: Option<SessionTokenUsage>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct ContentBlock {
    pub(crate) kind: String,
    pub(crate) text: Option<String>,
    pub(crate) tool_name: Option<String>,
    pub(crate) tool_call_id: Option<String>,
    // Pi toolResult 等来源携带工具失败标记,读取时原样透出,前端据此展示失败状态。
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
    pub(crate) token_usage: Option<SessionTokenUsage>,
}
