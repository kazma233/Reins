use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// 跨 Tauri invoke 边界的契约类型由 ts-rs 导出到
// src/features/providers/generated/。Raw 类型（不带 TS derive）镜像
// providers.yaml 落盘的 snake_case 键；View 类型对前端用 camelCase，
// 与 workspace 域的划分口径一致。

// ---------------------------------------------------------------------------
// 共享枚举
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) enum ProviderProtocol {
    OpenaiResponses,
    OpenaiChatCompletions,
    AnthropicMessages,
}

impl ProviderProtocol {
    pub(crate) fn all() -> &'static [ProviderProtocol] {
        &[
            ProviderProtocol::OpenaiResponses,
            ProviderProtocol::OpenaiChatCompletions,
            ProviderProtocol::AnthropicMessages,
        ]
    }

    // 各工具模型目录接口使用的鉴权头风格；聚合平台 /v1/models 通常兼容
    // OpenAI 形态，anthropic 协议按官方约定走 x-api-key。
    pub(crate) fn uses_api_key_header(self) -> bool {
        matches!(self, ProviderProtocol::AnthropicMessages)
    }
}

// 归一化思考等级集合（参考 models.dev reasoning_options 口径）；写入各工具
// 时与工具支持集取交集。Rust 侧避免叫 None，防止与 Option::None 混淆。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) enum ReasoningLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReasoningLevel {
    pub(crate) fn all() -> &'static [ReasoningLevel] {
        &[
            ReasoningLevel::Off,
            ReasoningLevel::Minimal,
            ReasoningLevel::Low,
            ReasoningLevel::Medium,
            ReasoningLevel::High,
            ReasoningLevel::Xhigh,
            ReasoningLevel::Max,
        ]
    }

    // 写入工具配置文件时的小写字符串形式。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ReasoningLevel::Off => "none",
            ReasoningLevel::Minimal => "minimal",
            ReasoningLevel::Low => "low",
            ReasoningLevel::Medium => "medium",
            ReasoningLevel::High => "high",
            ReasoningLevel::Xhigh => "xhigh",
            ReasoningLevel::Max => "max",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<ReasoningLevel> {
        let normalized = value.trim().to_lowercase();
        ReasoningLevel::all()
            .iter()
            .copied()
            .find(|level| level.as_str() == normalized)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) enum ProviderAppId {
    Codex,
    Claude,
    Opencode,
    Pi,
    Grokbuild,
    Dsh,
}

pub(crate) const PROVIDER_APPS: [ProviderAppId; 6] = [
    ProviderAppId::Codex,
    ProviderAppId::Claude,
    ProviderAppId::Opencode,
    ProviderAppId::Pi,
    ProviderAppId::Grokbuild,
    ProviderAppId::Dsh,
];

impl ProviderAppId {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ProviderAppId::Codex => "Codex",
            ProviderAppId::Claude => "Claude Code",
            ProviderAppId::Opencode => "OpenCode",
            ProviderAppId::Pi => "Pi",
            ProviderAppId::Grokbuild => "Grok Build",
            ProviderAppId::Dsh => "DeepSeek Harness",
        }
    }
}

// ---------------------------------------------------------------------------
// Raw 类型：providers.yaml 落盘文档（snake_case 键）
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RawProvidersDocument {
    pub(crate) version: i64,
    #[serde(default)]
    pub(crate) providers: BTreeMap<String, RawProvider>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RawProvider {
    pub(crate) label: String,
    pub(crate) protocol: ProviderProtocol,
    pub(crate) base_url: String,
    // 明文 API Key：用户确认的取舍是有意不带任何保护地存在本机配置里。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) api_key: Option<String>,
    #[serde(default)]
    pub(crate) models: Vec<ProviderModelRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ProviderModelRecord {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) context_window: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) max_output_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) supports_images: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reasoning_levels: Option<Vec<ReasoningLevel>>,
}

// 交给适配器与状态构建的解析后平台数据。
#[derive(Clone, Debug)]
pub(crate) struct ResolvedProvider {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) protocol: ProviderProtocol,
    pub(crate) base_url: String,
    pub(crate) api_key: Option<String>,
    pub(crate) models: Vec<ProviderModelRecord>,
}

// 落库与下发共用同一个空值口径：trim 后为空一律视为「没有 Key」。
impl ResolvedProvider {
    pub(crate) fn stored_api_key(&self) -> Option<String> {
        self.api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }
}

// ---------------------------------------------------------------------------
// View 类型：Tauri 边界（camelCase）
// ---------------------------------------------------------------------------

// i64 经 invoke 序列化为 JSON number，TS 侧固定为 number 而不是 bigint
//（与 workspace 域 created_at 的处理一致）。

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderModelView {
    pub(crate) id: String,
    pub(crate) label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null", optional = nullable)]
    pub(crate) context_window: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null", optional = nullable)]
    pub(crate) max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) supports_images: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) reasoning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) reasoning_levels: Option<Vec<ReasoningLevel>>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderView {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) protocol: ProviderProtocol,
    pub(crate) base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) api_key: Option<String>,
    pub(crate) models: Vec<ProviderModelView>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) enum ProviderAppEntryStatus {
    Applied,
    Drifted,
    External,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderAppEntry {
    // 注册键：Reins 写入的条目为 reins-<平台ID>；外部条目为原样键名。
    pub(crate) key: String,
    pub(crate) status: ProviderAppEntryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) base_url: Option<String>,
    pub(crate) model_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) default_model_id: Option<String>,
    pub(crate) notes: Vec<String>,
    // 条目实际使用的协议：按工具配置文件里的协议字段反推，
    // 识别不出（手工改过或未知取值）时留空不展示。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) protocol: Option<ProviderProtocol>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderAppState {
    pub(crate) app: ProviderAppId,
    pub(crate) config_paths: Vec<String>,
    pub(crate) config_exists: bool,
    pub(crate) supported_protocols: Vec<ProviderProtocol>,
    // 工具支持的思考等级集合；与模型 reasoning_levels 取交集后作为
    // 应用弹窗里的可选项。
    pub(crate) supported_reasoning_levels: Vec<ReasoningLevel>,
    // 每个支持档位在配置文件里的实际写入值（与各平台 apply 写入逻辑
    // 同源下发）；应用弹窗据此展示映射，前端不复刻映射规则。
    pub(crate) reasoning_level_writes: Vec<ReasoningEffortWrite>,
    // true = 多 Provider 并存（新增）；false = 单活动 Provider（应用即替换）。
    pub(crate) additive: bool,
    // 该工具不会写入的模型元数据字段标签（能力表同源下发）；应用弹窗
    // 据此明示用户填写的这些值不影响该工具。
    pub(crate) unwritten_model_fields: Vec<String>,
    // 工具配置里当前写入的默认思考等级（反读）；应用弹窗据此预选，重新
    // 应用时不会因为弹窗默认值而悄悄改掉它。识别不出或未设置时为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) default_reasoning_level: Option<ReasoningLevel>,
    pub(crate) entries: Vec<ProviderAppEntry>,
    // 单个工具配置损坏时不拖垮整个页面，把错误放进对应卡片。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) load_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ReasoningEffortWrite {
    pub(crate) level: ReasoningLevel,
    pub(crate) writes: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProvidersState {
    pub(crate) config_path: String,
    pub(crate) providers: Vec<ProviderView>,
    pub(crate) apps: Vec<ProviderAppState>,
}

// ---------------------------------------------------------------------------
// 输入
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderModelInput {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default)]
    #[ts(type = "number | null", optional = nullable)]
    pub(crate) context_window: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null", optional = nullable)]
    pub(crate) max_output_tokens: Option<i64>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub(crate) supports_images: Option<bool>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub(crate) reasoning: Option<bool>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub(crate) reasoning_levels: Option<Vec<ReasoningLevel>>,
}

// 调用方的写入意图：新增时拒绝覆盖同名记录，更新时要求记录已存在。
// 不传按 upsert 处理（既有内部调用方保持原语义）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) enum ProviderWriteMode {
    #[default]
    Upsert,
    Create,
    Update,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderUpsertInput {
    pub(crate) provider_id: String,
    #[serde(default)]
    pub(crate) mode: ProviderWriteMode,
    pub(crate) label: String,
    pub(crate) protocol: ProviderProtocol,
    pub(crate) base_url: String,
    // 所见即所得：表单里的值就是落盘值，留空即清除已存 Key。
    #[serde(default)]
    pub(crate) api_key: String,
    #[serde(default)]
    pub(crate) models: Vec<ProviderModelInput>,
}

// 一次应用动作选择的模型与默认值；预览与实际写入共用。
#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ApplyProviderInput {
    pub(crate) provider_id: String,
    pub(crate) app: ProviderAppId,
    pub(crate) model_ids: Vec<String>,
    pub(crate) default_model_id: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub(crate) default_reasoning_level: Option<ReasoningLevel>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderMutationResult {
    pub(crate) app: Option<ProviderAppId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) provider_id: Option<String>,
    pub(crate) action: String,
    pub(crate) detail: String,
}

// ---------------------------------------------------------------------------
// 预览：文件路径 + 行级 diff + 能力警告（密钥脱敏）
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) enum DiffLineKind {
    Context,
    Add,
    Remove,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct DiffLine {
    pub(crate) kind: DiffLineKind,
    pub(crate) text: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderFilePreview {
    pub(crate) path: String,
    pub(crate) exists: bool,
    pub(crate) diff: Vec<DiffLine>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ProviderApplyPreview {
    pub(crate) app: ProviderAppId,
    pub(crate) files: Vec<ProviderFilePreview>,
    pub(crate) warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// 模型目录拉取 + models.dev 补全
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct FetchedModel {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) name: Option<String>,
}

// 拉取命中时可能经历多级候选路径回退，url 是最终成功的那一个，
// 前端弹窗展示它以便确认实际请求打到哪里。
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct FetchedModelsResult {
    pub(crate) url: String,
    pub(crate) models: Vec<FetchedModel>,
}

#[derive(Clone, Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ModelsDevMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null", optional = nullable)]
    pub(crate) context_window: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null", optional = nullable)]
    pub(crate) max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) supports_images: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) reasoning: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) reasoning_levels: Option<Vec<ReasoningLevel>>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ModelsDevCandidate {
    pub(crate) provider: String,
    pub(crate) model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) label: Option<String>,
    pub(crate) meta: ModelsDevMeta,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) enum ModelsDevMatchStatus {
    Exact,
    Candidates,
    NotFound,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/features/providers/generated/")]
pub(crate) struct ModelsDevMatchResult {
    pub(crate) status: ModelsDevMatchStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub(crate) meta: Option<ModelsDevMeta>,
    pub(crate) candidates: Vec<ModelsDevCandidate>,
}
