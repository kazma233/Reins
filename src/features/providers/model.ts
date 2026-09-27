import type {
  ModelsDevMatchResult,
  ProviderAppEntry,
  ProviderAppEntryStatus,
  ProviderAppId,
  ProviderAppState,
  ProviderModelInput,
  ProviderModelView,
  ProviderProtocol,
  ProviderUpsertInput,
  ProviderView,
  ProvidersState,
  ReasoningLevel,
} from "./generated";

// providers 域前端展示口径：标签文案、能力判定、应用弹窗可选项。

export type ProvidersTab = "providers" | "apps";

export const PROVIDERS_TAB_COPY: Array<{ id: ProvidersTab; label: string }> = [
  { id: "apps", label: "Agent" },
  { id: "providers", label: "提供商" },
];

export const APP_LABELS: Record<ProviderAppId, string> = {
  codex: "Codex",
  claude: "Claude Code",
  opencode: "OpenCode",
  pi: "Pi",
  grokbuild: "Grok Build",
};

export const PROTOCOL_LABELS: Record<ProviderProtocol, string> = {
  openai_responses: "OpenAI Responses",
  openai_chat_completions: "OpenAI Chat Completions",
  anthropic_messages: "Anthropic Messages",
};

export const PROTOCOL_OPTIONS: Array<{ value: ProviderProtocol; label: string }> = [
  { value: "openai_chat_completions", label: "OpenAI Chat Completions" },
  { value: "openai_responses", label: "OpenAI Responses" },
  { value: "anthropic_messages", label: "Anthropic Messages" },
];

export const REASONING_LEVEL_LABELS: Record<ReasoningLevel, string> = {
  off: "无",
  minimal: "最低",
  low: "低",
  medium: "中",
  high: "高",
  xhigh: "超高",
  max: "最高",
};

export const ENTRY_STATUS_LABELS: Record<ProviderAppEntryStatus, string> = {
  applied: "已应用",
  drifted: "配置有偏差",
  external: "外部配置",
};

export function protocolLabel(protocol: ProviderProtocol): string {
  return PROTOCOL_LABELS[protocol];
}

export function findAppState(
  state: ProvidersState | null,
  app: ProviderAppId
): ProviderAppState | undefined {
  return state?.apps.find((item) => item.app === app);
}

// 工具与提供商协议是否兼容；不兼容时应用按钮禁用。
export function protocolCompatible(appState: ProviderAppState, provider: ProviderView): boolean {
  return appState.supportedProtocols.includes(provider.protocol);
}

// 应用弹窗的思考等级可选项：模型 reasoning_levels 与工具支持集取交集；
// 模型未填等级时视为未约束，直接用工具支持集。为空表示该模型对这个
// 工具没有可写等级（写入时省略字段）。
export function reasoningLevelOptions(
  appState: ProviderAppState,
  model: Pick<ProviderModelView, "reasoningLevels"> | Pick<ProviderModelInput, "reasoningLevels">
): ReasoningLevel[] {
  const supported = appState.supportedReasoningLevels;
  const levels = model.reasoningLevels;
  if (!levels) {
    return [...supported];
  }
  return supported.filter((level) => levels.includes(level));
}

// 应用弹窗思考等级档位在该提供商的实际写入值；后端按各工具 apply 的
// 写入逻辑同源下发（如 Grok max→xhigh、Pi off→off），前端不复刻规则。
// 支持集为空的工具（OpenCode）查不到时回退为枚举名本身。
export function reasoningEffortWrite(
  appState: ProviderAppState,
  level: ReasoningLevel
): string {
  return (
    appState.reasoningLevelWrites.find((item) => item.level === level)
      ?.writes ?? level
  );
}

// 提供商卡片条目行的思考等级映射文案：只列出写入值与档位名不同的档
// （如 Grok 的 max→xhigh）；全同名直写的提供商返回 null，不产生噪音。
export function reasoningEffortMappingText(
  appState: ProviderAppState
): string | null {
  const mappings = appState.reasoningLevelWrites.filter(
    (item) => item.writes !== item.level
  );
  if (mappings.length === 0) {
    return null;
  }
  return mappings.map((item) => `${item.level}→${item.writes}`).join("、");
}

// 勾选语义两处模型表单共用：选中且未填等级时给常用默认集，
// 取消时连同思考等级一并清空（应用时不写该元数据）。
export function toggleReasoning(
  model: Pick<ProviderModelInput, "reasoning" | "reasoningLevels">,
  value: boolean
): void {
  model.reasoning = value;
  if (!value) {
    model.reasoningLevels = null;
  } else if (!model.reasoningLevels) {
    model.reasoningLevels = ["low", "medium", "high"];
  }
}

// 应用前置条件清单；非空时应用按钮禁用并逐条展示原因。
export function applyBlockers(
  appState: ProviderAppState,
  provider: ProviderView,
  selectedModelIds: string[]
): string[] {
  const blockers: string[] = [];
  if (!protocolCompatible(appState, provider)) {
    blockers.push(
      `${APP_LABELS[appState.app]} 不支持协议 ${protocolLabel(provider.protocol)}。`
    );
  }
  if (!provider.keyPresent) {
    blockers.push("尚未设置该提供商的 API Key。");
  }
  if (selectedModelIds.length === 0) {
    blockers.push("至少选择一个模型。");
  }
  return blockers;
}

// 工具卡片上属于指定提供商的条目（已应用 / 配置有偏差），外部配置不算。
export function entriesForProvider(
  appState: ProviderAppState,
  providerId: string
): ProviderAppEntry[] {
  return appState.entries.filter((entry) => entry.providerId === providerId);
}

// 「应用提供商」候选列表：提供商在本工具已写入 reins- 条目（已应用或配
// 置有偏差）时不再重复出现，应用状态由条目行 tag 表达；外部配置不阻止
// 候选，因为它不属于任何 Reins 平台。
export function applyCandidates(
  appState: ProviderAppState,
  providers: ProviderView[]
): ProviderView[] {
  const managedIds = new Set(
    appState.entries
      .map((entry) => entry.providerId)
      .filter((id): id is string => typeof id === "string" && id !== "")
  );
  return providers.filter((provider) => !managedIds.has(provider.id));
}

// ---------------------------------------------------------------------------
// 编辑弹窗表单状态
// ---------------------------------------------------------------------------

export type ProviderModelForm = ProviderModelInput & {
  // v-for 稳定键；id 在用户输入过程中可能为空。
  rowKey: string;
};

export type ProviderFormState = {
  // null = 新建；非 null = 编辑且 ID 不可改（凭据 account 绑定 ID）。
  originalProviderId: string | null;
  providerId: string;
  label: string;
  protocol: ProviderProtocol;
  baseUrl: string;
  models: ProviderModelForm[];
};

let modelRowSeq = 0;

export function emptyModelForm(modelId = ""): ProviderModelForm {
  modelRowSeq += 1;
  return {
    rowKey: `model-${modelRowSeq}`,
    id: modelId,
    label: "",
    contextWindow: null,
    maxOutputTokens: null,
    supportsImages: null,
    reasoning: null,
    reasoningLevels: null,
  };
}

// models.dev 元数据只预填空缺字段（null/undefined 视为空缺），不覆盖用户已填值；
// 行内补全与手工新增弹窗共用。
export function applyModelsDevMeta(
  target: ProviderModelInput,
  meta: NonNullable<ModelsDevMatchResult["meta"]>
): void {
  target.contextWindow ??= meta.contextWindow ?? null;
  target.maxOutputTokens ??= meta.maxOutputTokens ?? null;
  target.supportsImages ??= meta.supportsImages ?? null;
  target.reasoning ??= meta.reasoning ?? null;
  target.reasoningLevels ??= meta.reasoningLevels ?? null;
}

export function reasoningLevelsText(levels: ProviderModelInput["reasoningLevels"]): string {
  return (levels ?? []).join(",");
}

// 只保留已知等级，全部无效时视为未设置。
export function parseReasoningLevels(text: string): ProviderModelInput["reasoningLevels"] {
  const levels = text
    .split(",")
    .map((part) => part.trim())
    .filter((part): part is ReasoningLevel => part in REASONING_LEVEL_LABELS);
  return levels.length > 0 ? levels : null;
}

export function emptyProviderForm(): ProviderFormState {
  return {
    originalProviderId: null,
    providerId: "",
    label: "",
    protocol: "openai_chat_completions",
    baseUrl: "",
    models: [],
  };
}

export function formFromProvider(provider: ProviderView): ProviderFormState {
  return {
    originalProviderId: provider.id,
    providerId: provider.id,
    label: provider.label,
    protocol: provider.protocol,
    baseUrl: provider.baseUrl,
    models: provider.models.map((model) => ({
      ...emptyModelForm(model.id),
      id: model.id,
      label: model.label,
      contextWindow: model.contextWindow ?? null,
      maxOutputTokens: model.maxOutputTokens ?? null,
      supportsImages: model.supportsImages ?? null,
      reasoning: model.reasoning ?? null,
      reasoningLevels: model.reasoningLevels ?? null,
    })),
  };
}

// number input 空值会得到 ""；统一把非有限值收敛为 null。
function numberOrNull(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim() !== "" && Number.isFinite(Number(value))) {
    return Number(value);
  }
  return null;
}

// 输入即时归一化，保证界面所见 ID 与落库 ID 一致；
// 字符集规则以后端 normalize_provider_id 为准，这里只做同样的转换。
export function normalizeProviderIdInput(value: string): string {
  return value
    .toLowerCase()
    .replace(/_/g, "-")
    .replace(/[^a-z0-9-]/g, "");
}

export function formToInput(form: ProviderFormState): ProviderUpsertInput {
  return {
    providerId: form.providerId,
    label: form.label,
    protocol: form.protocol,
    baseUrl: form.baseUrl,
    models: form.models.map((model) => ({
      id: model.id,
      label: model.label,
      contextWindow: numberOrNull(model.contextWindow),
      maxOutputTokens: numberOrNull(model.maxOutputTokens),
      supportsImages: model.supportsImages ?? null,
      reasoning: model.reasoning ?? null,
      reasoningLevels: model.reasoningLevels ?? null,
    })),
  };
}

