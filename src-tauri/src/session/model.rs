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
    #[serde(rename = "dsh")]
    Dsh,
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
            "dsh" => Ok(Self::Dsh),
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
            Self::Dsh => "dsh",
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

// 会话用过的模型。provider 是各工具自己配置里的 provider id,不是统一命名:
// Codex/OpenCode/Pi/DSH/ZCode 有该字段,Claude Code 与 Grok Build 的转录里
// 根本没有。一个会话先后换过模型时会有多项。
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct SessionModel {
    pub(crate) model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) provider: Option<String>,
}

impl SessionModel {
    pub(crate) fn new(provider: Option<&str>, model: &str) -> Self {
        Self {
            model: model.to_string(),
            provider: provider
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        }
    }
}

// 会话内模型计数:同一 (provider, model) 归一项,输出按使用次数降序、同次数
// 按首次出现顺序,首项即用于列表展示的主模型。会话级与 family 级合并都走
// 这里,避免两处各写一份排序口径。
#[derive(Default)]
pub(crate) struct ModelTally {
    entries: Vec<(SessionModel, usize)>,
}

impl ModelTally {
    pub(crate) fn record(&mut self, provider: Option<&str>, model: &str) {
        let model = model.trim();
        if model.is_empty() {
            return;
        }

        let candidate = SessionModel::new(provider, model);
        match self.entries.iter_mut().find(|(entry, _)| *entry == candidate) {
            Some((_, count)) => *count += 1,
            None => self.entries.push((candidate, 1)),
        }
    }

    /// family 级合并:成员各自的列表已按用量排好序,这里只做保序去重——跨成员
    /// 的调用次数不可比,若让"出现在更多成员里"的模型顶上去,列表角标会指到
    /// 子代理偶尔一用的模型上。会话内计数仍走 record。
    pub(crate) fn absorb(&mut self, models: &[SessionModel]) {
        for entry in models {
            if !self.entries.iter().any(|(existing, _)| existing == entry) {
                self.entries.push((entry.clone(), 0));
            }
        }
    }

    /// 计数为 0 的是 absorb 保序并入的项,排在计数项之后;两种语义各自成对使用
    /// (会话内 record、family 内 absorb),不混用。
    pub(crate) fn into_models(mut self) -> Vec<SessionModel> {
        // sort_by 稳定:同次数的项保持首次出现顺序。
        self.entries.sort_by(|left, right| right.1.cmp(&left.1));
        self.entries.into_iter().map(|(entry, _)| entry).collect()
    }
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
    // 按模型的日切片:只有取得到模型信息的消耗事件进入,取不到的来源为
    // 空数组,模型筛选对其不可见,也不与 days 总量强行对齐。
    pub(crate) model_days: Vec<UsageModelDayPoint>,
    // 按模型的今日小时切片,口径同 model_days。
    pub(crate) model_today_hours: Vec<UsageModelHourPoint>,
}

// 模型维度的日聚合点:同一模型跨小时合并成天。
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/usage/generated/")]
pub(crate) struct UsageModelDayPoint {
    pub(crate) model: String,
    pub(crate) day: String,
    pub(crate) usage: SessionTokenUsage,
}

// 模型维度的今日小时点。
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/usage/generated/")]
pub(crate) struct UsageModelHourPoint {
    pub(crate) model: String,
    pub(crate) hour: u8,
    pub(crate) usage: SessionTokenUsage,
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

// 单个小时桶:usage 是该小时全量;models 是按模型的切片,取不到模型信息
// 的事件只进总量。持久缓存直接序列化整个结构,字段变更须 bump 缓存版本。
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub(crate) struct UsageHourBucket {
    pub(crate) usage: SessionTokenUsage,
    #[serde(default)]
    pub(crate) models: std::collections::BTreeMap<String, SessionTokenUsage>,
}

impl UsageHourBucket {
    pub(crate) fn accumulate(&mut self, next: &Self) {
        self.usage.accumulate(&next.usage);
        for (model, usage) in &next.models {
            self.models
                .entry(model.clone())
                .or_default()
                .accumulate(usage);
        }
    }

    pub(crate) fn accumulate_event(&mut self, model: Option<&str>, usage: &SessionTokenUsage) {
        self.usage.accumulate(usage);
        if let Some(model) = model {
            self.models
                .entry(model.to_string())
                .or_default()
                .accumulate(usage);
        }
    }
}

// 单个转录文件(或 SQL 来源)的小时桶中间形态;day 序列与 today_hours 都由
// 它聚合而来。key 形如 "YYYY-MM-DDTHH"(本地时区),BTreeMap 保证升序。
pub(crate) type UsageHourBuckets = std::collections::BTreeMap<String, UsageHourBucket>;

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
    // 会话用过的模型(family 含子代理线程),首项是主模型;取不到模型信息
    // 的来源为空。Claude Code 与 Grok Build 的 provider 恒为 None。
    #[serde(default)]
    pub(crate) models: Vec<SessionModel>,
    // 会话在当前来源里的存储状态,由各读取器按自家契约填写:
    // archived = 来源已把会话移出默认集合(codex 移到 archived_sessions/、
    // dsh 写进 workspace.json 的归档集、zcode/opencode 的 time_archived);
    // compressed = 转录在磁盘上不是该来源的明文常态(codex 的 .jsonl.zst)。
    // 无归档/无压缩契约的来源恒为 false(pi、grokbuild、claude code)。
    #[serde(default)]
    pub(crate) archived: bool,
    #[serde(default)]
    pub(crate) compressed: bool,
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
    // 这条消息所属轮次用的模型;过程消息与取不到模型信息的来源(Grok Build)
    // 为 None,由各读取器按自家契约填写。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) model: Option<SessionModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) session_id: Option<String>,
}

// 用户输入自身不带模型:它的归属由这一轮的回答决定——同一次扫描里其后第一条
// 带模型的消息。助手消息在读取器里已按来源直接标注(Codex/DSH 是整轮继承),
// 这里只补 user 角色,已经标注过的不动。
pub(crate) fn fill_user_message_models(messages: &mut [SessionMessage]) {
    let mut following: Option<SessionModel> = None;
    for message in messages.iter_mut().rev() {
        if let Some(model) = message.model.as_ref() {
            following = Some(model.clone());
            continue;
        }
        if message.role == "user" {
            message.model = following.clone();
        }
    }
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

// 一次会话删除的预演:动作清单与说明文案都由后端来源注册表给出,前端确认框
// 只渲染,不再持有 per-source 删除语义。
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) struct DeletePlan {
    pub(crate) source_app: SourceApp,
    /// false 时仅 reason 有意义:删除入口隐藏,不进对话框。
    pub(crate) supported: bool,
    #[ts(optional)]
    pub(crate) reason: Option<String>,
    pub(crate) description: String,
    pub(crate) details: Vec<String>,
    pub(crate) command_label: String,
    pub(crate) actions: Vec<DeletePlanAction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/sessions/generated/")]
pub(crate) enum DeletePlanAction {
    RemoveFile { path: String },
    RemoveDirectory { path: String },
    RunCli { program: String, args: Vec<String> },
    Sqlite { db_path: String, sql: String },
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
    /// 条目自身的会话身份;列表层用它按 id 取 summary(pi 的条目不带
    /// summary,身份只能来自扫描时的 header)。
    pub(crate) source_session_id: String,
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
    pub(crate) models: ModelTally,
    pub(crate) archived: bool,
    pub(crate) compressed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, role: &str, model: Option<(&str, &str)>) -> SessionMessage {
        SessionMessage {
            id: id.to_string(),
            role: role.to_string(),
            timestamp: None,
            blocks: Vec::new(),
            model: model.map(|(provider, model)| SessionModel::new(Some(provider), model)),
            session_id: None,
        }
    }

    #[test]
    fn model_tally_orders_by_calls_then_first_seen() {
        let mut tally = ModelTally::default();
        tally.record(Some("custom"), "gpt-5.6-terra");
        tally.record(Some("custom"), "deepseek-flash");
        tally.record(Some("custom"), "gpt-5.6-terra");

        let models = tally.into_models();
        // 调用次数多的排前;并列时保持首次出现顺序。
        assert_eq!(
            models,
            vec![
                SessionModel::new(Some("custom"), "gpt-5.6-terra"),
                SessionModel::new(Some("custom"), "deepseek-flash"),
            ]
        );
    }

    #[test]
    fn model_tally_keeps_same_model_from_different_providers_apart() {
        let mut tally = ModelTally::default();
        tally.record(Some("first"), "deepseek-flash");
        tally.record(Some("second"), "deepseek-flash");
        tally.record(None, "deepseek-flash");
        tally.record(Some("first"), "deepseek-flash");

        let models = tally.into_models();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0], SessionModel::new(Some("first"), "deepseek-flash"));
    }

    #[test]
    fn model_tally_skips_blank_model_names() {
        let mut tally = ModelTally::default();
        tally.record(None, "   ");
        assert!(tally.into_models().is_empty());
    }

    #[test]
    fn absorb_keeps_root_primary_first() {
        let mut tally = ModelTally::default();
        tally.absorb(&[
            SessionModel::new(Some("custom"), "gpt-5.6-terra"),
            SessionModel::new(Some("custom"), "gpt-5.5"),
        ]);
        tally.absorb(&[SessionModel::new(Some("custom"), "gpt-5.5")]);

        // 并列次数时保持并入顺序:root 的主模型仍排最前。
        assert_eq!(
            tally.into_models()[0],
            SessionModel::new(Some("custom"), "gpt-5.6-terra")
        );
    }

    #[test]
    fn user_messages_take_the_model_of_the_following_answer() {
        let mut messages = vec![
            message("u1", "user", None),
            message("a1", "assistant", Some(("custom", "deepseek-flash"))),
            message("u2", "user", None),
            message("t1", "tool", None),
            message("a2", "assistant", Some(("custom", "gpt-6-luna"))),
            message("u3", "user", None),
        ];
        fill_user_message_models(&mut messages);

        let models: Vec<Option<&str>> = messages
            .iter()
            .map(|message| message.model.as_ref().map(|model| model.model.as_str()))
            .collect();
        assert_eq!(
            models,
            vec![
                Some("deepseek-flash"),
                Some("deepseek-flash"),
                Some("gpt-6-luna"),
                None,
                Some("gpt-6-luna"),
                // 末尾没有后续回答,保持未知而不是猜。
                None,
            ]
        );
    }
}
