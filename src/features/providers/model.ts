import { agentDisplayName } from "@shared/lib/agent-labels";
import type {
  ApplyProviderInput,
  FetchedModel,
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

// 工具名与 target / 会话来源共用同一份产品名常量。
export const APP_LABELS: Record<ProviderAppId, string> = {
  codex: agentDisplayName("codex"),
  claude: agentDisplayName("claude"),
  opencode: agentDisplayName("opencode"),
  pi: agentDisplayName("pi"),
  grokbuild: agentDisplayName("grokbuild"),
  dsh: agentDisplayName("dsh"),
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

// 落库与展示共用同一个空值口径：trim 后为空视为没有 Key。
export function hasApiKey(provider: ProviderView): boolean {
  return Boolean(provider.apiKey?.trim());
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

// 应用弹窗提示：该工具不会写入的模型元数据（标签由后端能力表下发）。
// 无此类字段的提供商返回 null，不产生噪音。
export function unwrittenModelFieldsText(
  appState: ProviderAppState
): string | null {
  if (appState.unwrittenModelFields.length === 0) {
    return null;
  }
  return `${APP_LABELS[appState.app]} 不会写入这些模型元数据：${appState.unwrittenModelFields.join("、")}；在模型里填写的对应值不会应用到该工具。`;
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
  if (!hasApiKey(provider)) {
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

// 条目行上重新打开应用弹窗的目标：已应用与配置有偏差的条目都指向某个
// 平台，可以据此改默认模型（多提供商并存时也用它切换默认）；外部配置
// 不属于任何平台、平台已删除的偏差条目查不到元数据，都返回 null。
export function reapplyProvider(
  entry: ProviderAppEntry,
  providers: ProviderView[]
): ProviderView | null {
  if (!entry.providerId) {
    return null;
  }
  return providers.find((provider) => provider.id === entry.providerId) ?? null;
}

// 应用弹窗的思考等级可选项：选中模型在该工具下可用等级的交集，外加当前
// 已写入的等级——当前值即使不在交集里（模型目录后来收窄）也要能显示与
// 保留，否则重新应用会把它写没。
export function reasoningLevelChoices(
  appState: ProviderAppState,
  provider: ProviderView,
  selectedModelIds: string[],
  current: ReasoningLevel | null
): ReasoningLevel[] {
  const selected = provider.models.filter((model) =>
    selectedModelIds.includes(model.id)
  );
  // 取所有选中模型可用等级的交集；任一模型未约束等级则不收紧。
  let options: ReasoningLevel[] | null = null;
  for (const model of selected) {
    const modelOptions = reasoningLevelOptions(appState, model);
    options = options === null ? modelOptions : options.filter((level) => modelOptions.includes(level));
  }
  const levels = options ?? [];
  return current && !levels.includes(current) ? [...levels, current] : levels;
}

// 应用弹窗的初始选择：优先沿用该平台已写入的模型，并以当前默认模型为
// 默认项，避免重新应用时顺带换掉默认模型；没有可复用的写入记录（首次
// 应用）时预选平台目录里的全部模型。默认思考等级取该工具配置里的当前
// 值（工具支持的前提下），重新应用不会因为弹窗默认值把它改掉。
export function initialApplySelection(
  appState: ProviderAppState | undefined,
  provider: ProviderView
): {
  modelIds: string[];
  defaultModelId: string;
  defaultReasoningLevel: ReasoningLevel | null;
} {
  const applied = appState ? entriesForProvider(appState, provider.id) : [];
  const known = applied
    .flatMap((entry) => entry.modelIds)
    .filter((modelId) => provider.models.some((model) => model.id === modelId));
  const modelIds = known.length > 0 ? known : provider.models.map((model) => model.id);
  const currentDefault = applied.find((entry) => entry.defaultModelId)?.defaultModelId;
  const defaultModelId =
    currentDefault && modelIds.includes(currentDefault) ? currentDefault : (modelIds[0] ?? "");
  const currentLevel = appState?.defaultReasoningLevel ?? null;
  const defaultReasoningLevel =
    currentLevel && appState && appState.supportedReasoningLevels.includes(currentLevel)
      ? currentLevel
      : null;
  return { modelIds, defaultModelId, defaultReasoningLevel };
}

// 提供商同步计划：已应用与配置有偏差的条目都属于该提供商，逐个 Agent 生成
// 应用输入；无法安全同步的 Agent 返回跳过原因，由调用方在确认弹窗展示。
export type ProviderSyncTarget = { app: ProviderAppId; skip: false; input: ApplyProviderInput };
export type ProviderSyncSkip = { app: ProviderAppId; skip: true; reason: string };
export type ProviderSyncPlanItem = ProviderSyncTarget | ProviderSyncSkip;

export function providerSyncPlan(
  state: ProvidersState,
  provider: ProviderView
): ProviderSyncPlanItem[] {
  const plan: ProviderSyncPlanItem[] = [];
  for (const appState of state.apps) {
    const applied = entriesForProvider(appState, provider.id);
    if (applied.length === 0) {
      continue;
    }
    // 已应用的模型全部不在目录中时 initialApplySelection 会回退成整个目录，
    // 自动同步不替用户扩大选择，直接跳过。
    const appliedModelIds = applied.flatMap((entry) => entry.modelIds);
    const knownModelIds = appliedModelIds.filter((modelId) =>
      provider.models.some((model) => model.id === modelId)
    );
    if (appliedModelIds.length > 0 && knownModelIds.length === 0) {
      plan.push({
        app: appState.app,
        skip: true,
        reason: "已应用的模型均不在当前目录中。",
      });
      continue;
    }

    const selection = initialApplySelection(appState, provider);
    const blockers = applyBlockers(appState, provider, selection.modelIds);
    if (blockers.length > 0) {
      plan.push({ app: appState.app, skip: true, reason: blockers.join("") });
      continue;
    }
    // 与应用弹窗一致：默认思考等级取交集，当前写入值保留在可选项里。
    const levels = reasoningLevelChoices(
      appState,
      provider,
      selection.modelIds,
      selection.defaultReasoningLevel
    );
    const defaultReasoningLevel =
      selection.defaultReasoningLevel && levels.includes(selection.defaultReasoningLevel)
        ? selection.defaultReasoningLevel
        : null;
    plan.push({
      app: appState.app,
      skip: false,
      input: {
        providerId: provider.id,
        app: appState.app,
        modelIds: selection.modelIds,
        defaultModelId: selection.defaultModelId,
        defaultReasoningLevel,
      },
    });
  }
  return plan;
}

// ---------------------------------------------------------------------------
// 编辑弹窗表单状态
// ---------------------------------------------------------------------------

export type ProviderModelForm = ProviderModelInput & {
  // v-for 稳定键；id 在用户输入过程中可能为空。
  rowKey: string;
};

export type ProviderFormState = {
  // null = 新建；非 null = 编辑且 ID 不可改（落库 ID 同时是 Key 的归属字段）。
  originalProviderId: string | null;
  providerId: string;
  label: string;
  protocol: ProviderProtocol;
  baseUrl: string;
  // 明文回显：表单里的值就是 providers.yaml 里的值，留空保存即清除。
  apiKey: string;
  models: ProviderModelForm[];
  // 字段级校验错误：ProviderEditDialog 内联展示，用户改动对应字段时清空。
  errors?: {
    providerId?: string;
    label?: string;
    baseUrl?: string;
    models?: string;
  };
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
    apiKey: "",
    models: [],
    errors: {},
  };
}

export function formFromProvider(provider: ProviderView): ProviderFormState {
  return {
    originalProviderId: provider.id,
    providerId: provider.id,
    label: provider.label,
    protocol: provider.protocol,
    baseUrl: provider.baseUrl,
    apiKey: provider.apiKey ?? "",
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
    errors: {},
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
    apiKey: form.apiKey.trim(),
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

// ---------------------------------------------------------------------------
// 模型目录加入
// ---------------------------------------------------------------------------

// 模型 ID 比较口径与后端 normalize_model_record 一致：trim 后精确匹配。
function modelIdKey(id: string): string {
  return id.trim();
}

export function hasModelId(ids: readonly string[], id: string): boolean {
  const key = modelIdKey(id);
  return ids.some((existing) => modelIdKey(existing) === key);
}

// 拉取结果并入模型目录：与目录已有 ID（含本次先加入的）重复的模型默认忽略，
// 忽略项返回给调用方做弹窗提示。
export function mergeFetchedModels(
  existing: ProviderModelForm[],
  fetched: FetchedModel[]
): { added: ProviderModelForm[]; skipped: FetchedModel[] } {
  const seen = new Set(existing.map((model) => modelIdKey(model.id)));
  const added: ProviderModelForm[] = [];
  const skipped: FetchedModel[] = [];
  for (const model of fetched) {
    const key = modelIdKey(model.id);
    if (seen.has(key)) {
      skipped.push(model);
      continue;
    }
    seen.add(key);
    added.push({ ...emptyModelForm(model.id), id: model.id, label: model.name ?? model.id });
  }
  return { added, skipped };
}

