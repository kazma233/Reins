import { AGENT_LABELS } from "@shared/lib/agent-labels";
import type {
  AgentTargetId,
  McpTargetPreviewResult,
  McpTransport,
  BatchGitSkillImportResult,
  SkillDiscoveryResult,
  SkillSourceKind,

} from "./types";

export const WORKSPACE_TAB_COPY = [
  { id: "targets", label: "targets" },
  { id: "skills", label: "skills" },
  { id: "mcp", label: "mcp" },
] as const;

// agent 名按统一的产品名渲染（生成物，与历史会话来源、providers 同一份
// 常量）；target id 即规范键，未知 id（用户自定义 target）原样返回。
export function formatTargetName(agentId: string): string {
  return AGENT_LABELS[agentId] ?? agentId;
}

// target id 的展示形式：全局 target 就是 agent 名，
// 项目 target 带项目前缀，两者都用展示名而不是配置里的原始 id。
export function formatTargetLabel(targetId: AgentTargetId) {
  const colonPos = targetId.indexOf(":");
  if (colonPos >= 0) {
    const projectId = targetId.substring(0, colonPos);
    const agentId = targetId.substring(colonPos + 1);
    return `${projectId}/${formatTargetName(agentId)}`;
  }
  return formatTargetName(targetId);
}

export type TargetGroup = {
  key: string;
  label: string;
  targetIds: AgentTargetId[];
};

// 目标按项目分组：项目 target 各归自己的 project，全局 target 单独一组。
// 分组顺序跟随入参（配置里的安装顺序），全局组始终在最前。
export function groupTargetIds(targetIds: AgentTargetId[]): TargetGroup[] {
  const globalTargets: AgentTargetId[] = [];
  const projectGroups = new Map<string, AgentTargetId[]>();
  for (const targetId of targetIds) {
    const label = formatTargetLabel(targetId);
    const slashPos = label.indexOf("/");
    if (slashPos < 0) {
      globalTargets.push(targetId);
      continue;
    }
    const projectId = label.slice(0, slashPos);
    const agents = projectGroups.get(projectId) ?? [];
    agents.push(targetId);
    projectGroups.set(projectId, agents);
  }
  return [
    { key: "global", label: "全局", targetIds: globalTargets },
    ...[...projectGroups].map(([projectId, targets]) => ({
      key: projectId,
      label: projectId,
      targetIds: targets,
    })),
  ].filter((group) => group.targetIds.length > 0);
}

// 七个内置工具的稳定 target id 契约；具体的创建预设（路径/prefix/configType）
// 由后端 get_target_presets 下发，前端不再静态维护。
export type BuiltinTargetPresetId =
  | "codex"
  | "claude"
  | "opencode"
  | "zcode"
  | "pi"
  | "grokbuild"
  | "dsh";

export type SkillSourceFilter = "all" | "new" | `source:${string}`;


export type SkillTargetToggleKind = "install" | "replace" | "uninstall";

export type SkillDiscoveryPreviewState = {
  discovery: SkillDiscoveryResult | null;
  includeNamePatternsText: string;
  includePathPatternsText: string;
  previewLoading: boolean;
  // 预览读取失败：常驻在预览区域，下一次成功读取时清空。
  previewError: string | null;
};

type SkillSourcePreviewDraft = {
  sourceType: SkillSourceKind;
  repo: string;
  rootPath: string;
  ref: string;
};

export type SkillImportDialogState = {
  open: boolean;
  loading: boolean;
  mode: "single" | "batch-git";
  batchYamlText: string;
  batchResult: BatchGitSkillImportResult | null;
  preview: SkillDiscoveryPreviewState;
} & SkillSourcePreviewDraft;

export type SkillDetailDialogState = {
  open: boolean;
  loading: boolean;
  detail: string;
  skillId: string | null;
};

export type SkillSourceEditDialogState = {
  open: boolean;
  loading: boolean;
  sourceId: string | null;
  title: string;
  preview: SkillDiscoveryPreviewState;
} & SkillSourcePreviewDraft;

export type TargetDeleteDialogState = {
  open: boolean;
  loading: boolean;
  error: string | null;
  targetId: string | null;
};

export type TargetFormState = {
  originalTargetId: string | null;
  targetId: string;
  enabled: boolean;
  skillDir: string;
  configPath: string;
  mcpConfigPrefix: string;
  // 字段级校验错误：提交时一次算出全部，字段值变化时清空该字段。
  errors: FieldErrors;
};

export type FieldErrors = Partial<Record<string, string>>;

export type McpDeleteDialogState = {
  open: boolean;
  loading: boolean;
  error: string | null;
  serverNames: string[];
};

export type McpApplyPreviewDialogState = {
  open: boolean;
  loading: boolean;
  submitting: boolean;
  error: string | null;
  serverName: string;
  targetId: AgentTargetId | null;
  preview: McpTargetPreviewResult | null;
};

export type McpFormState = {
  originalName: string | null;
  name: string;
  enabled: boolean;
  transport: McpTransport;
  createdAt: number | null;
  homepage: string;
  command: string;
  args: string;
  env: string;
  url: string;
  headers: string;
  timeout: string;
  errors: FieldErrors;
};

export function parseCommaSeparatedList(value: string): string[] {
  return value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}


export function createSkillImportDialogState(sourceType: SkillSourceKind = "git"): SkillImportDialogState {
  return {
    open: false,
    loading: false,
    mode: "single",
    batchYamlText: "",
    batchResult: null,
    sourceType,
    repo: "",
    rootPath: "",
    ref: sourceType === "git" ? "main" : "",
    preview: createSkillDiscoveryPreviewState(),
  };
}

export function createSkillSourceEditDialogState(): SkillSourceEditDialogState {
  return {
    open: false,
    loading: false,
    sourceId: null,
    sourceType: "git",
    title: "",
    repo: "",
    rootPath: "",
    ref: "main",
    preview: createSkillDiscoveryPreviewState(),
  };
}

export function createSkillDiscoveryPreviewState(): SkillDiscoveryPreviewState {
  return {
    discovery: null,
    includeNamePatternsText: "",
    includePathPatternsText: "",
    previewLoading: false,
    previewError: null,
  };
}

export const DEFAULT_MCP_FORM: McpFormState = {
  originalName: null,
  name: "",
  enabled: true,
  transport: "stdio",
  createdAt: null,
  homepage: "",
  command: "",
  args: "",
  env: "",
  url: "",
  headers: "",
  timeout: "",
  errors: {},
};

export const DEFAULT_TARGET_FORM: TargetFormState = {
  originalTargetId: null,
  targetId: "",
  enabled: true,
  skillDir: "",
  configPath: "",
  mcpConfigPrefix: "",
  errors: {},
};

export const DEFAULT_MCP_APPLY_PREVIEW_DIALOG: McpApplyPreviewDialogState = {
  open: false,
  loading: false,
  submitting: false,
  error: null,
  serverName: "",
  targetId: null,
  preview: null,
};


export const AVAILABLE_PROJECT_AGENTS = ["claude", "codex", "opencode", "zcode", "pi", "grokbuild", "dsh"] as const;

export type ProjectFormState = {
  originalProjectId: string | null;
  projectId: string;
  path: string;
  agents: string[];
  errors: FieldErrors;
};

export const DEFAULT_PROJECT_FORM: ProjectFormState = {
  originalProjectId: null,
  projectId: "",
  path: "",
  // 新增项目不预选 agent：分发到哪些 agent 由用户显式选择，
  // 不替用户预设一个他可能没装的目标。
  agents: [],
  errors: {},
};

export type ProjectDeleteDialogState = {
  open: boolean;
  loading: boolean;
  error: string | null;
  projectId: string | null;
};

export type ProjectAgentPickerDialogState = {
  open: boolean;
  loading: boolean;
  contextName: string;
  projectId: string | null;
  serverName: string | null;
  // 当前 mcp 的 transport；pi 不支持 sse，弹窗据此置灰 pi 按钮。
  transport: McpTransport | null;
  // 期望终态：应用后应安装该 mcp 的复合目标 id（projectId:agentId）
  desiredAgentIds: AgentTargetId[];
  confirmOpen: boolean;
};

export const DEFAULT_PROJECT_AGENT_PICKER_DIALOG: ProjectAgentPickerDialogState = {
  open: false,
  loading: false,
  contextName: "",
  projectId: null,
  serverName: null,
  transport: null,
  desiredAgentIds: [],
  confirmOpen: false,
};

export function createProjectAgentPickerDialogState(): ProjectAgentPickerDialogState {
  return { ...DEFAULT_PROJECT_AGENT_PICKER_DIALOG };
}
