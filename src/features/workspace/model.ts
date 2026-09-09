import type {
  AgentTargetId,
  McpConfigType,
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

export function formatTargetLabel(targetId: AgentTargetId) {
  const colonPos = targetId.indexOf(":");
  if (colonPos >= 0) {
    const projectId = targetId.substring(0, colonPos);
    const agentId = targetId.substring(colonPos + 1);
    return `${projectId}/${agentId}`;
  }
  return targetId;
}

// Map configType to a human-readable description shown in the target row.
// common = standard MCP { command, args, env } shape used by codex/claude/zcode.
// opencode = OpenCode's own schema with $schema header.
export function formatMcpConfigType(type: McpConfigType): string {
  switch (type) {
    case "common":
      return "通用 MCP 配置格式（标准 command/args/env）";
    case "opencode":
      return "OpenCode 配置格式（带 $schema 标头）";
  }
}

export type BuiltinTargetPresetId = "codex" | "claude" | "opencode" | "zcode";

export const BUILTIN_TARGET_PRESETS: Record<
  BuiltinTargetPresetId,
  Omit<TargetFormState, "originalTargetId">
> = {
  codex: {
    targetId: "codex",
    enabled: true,
    skillDir: "~/.agents/skills",
    configPath: "~/.codex/config.toml",
    mcpConfigPrefix: "mcp_servers",
    mcpConfigType: "common",
  },
  claude: {
    targetId: "claude",
    enabled: true,
    skillDir: "~/.claude/skills",
    configPath: "~/.claude.json",
    mcpConfigPrefix: "mcpServers",
    mcpConfigType: "common",
  },
  opencode: {
    targetId: "opencode",
    enabled: true,
    skillDir: "~/.config/opencode/skills",
    configPath: "~/.config/opencode/opencode.json",
    mcpConfigPrefix: "mcp",
    mcpConfigType: "opencode",
  },
  zcode: {
    targetId: "zcode",
    enabled: true,
    skillDir: "~/.zcode/skills",
    configPath: "~/.zcode/cli/config.json",
    mcpConfigPrefix: "mcp.servers",
    mcpConfigType: "common",
  },
};

export type SkillSourceFilter = "all" | "new" | `source:${string}`;


export type SkillTargetToggleKind = "install" | "replace" | "uninstall";

export type SkillDiscoveryPreviewState = {
  discovery: SkillDiscoveryResult | null;
  includeNamePatternsText: string;
  includePathPatternsText: string;
  previewLoading: boolean;
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
  targetId: string | null;
};

export type TargetFormState = {
  originalTargetId: string | null;
  targetId: string;
  enabled: boolean;
  skillDir: string;
  configPath: string;
  mcpConfigPrefix: string;
  mcpConfigType: McpConfigType;
};

export type McpDeleteDialogState = {
  open: boolean;
  loading: boolean;
  serverNames: string[];
};

export type McpApplyPreviewDialogState = {
  open: boolean;
  loading: boolean;
  submitting: boolean;
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
};

export const DEFAULT_TARGET_FORM: TargetFormState = {
  originalTargetId: null,
  targetId: "",
  enabled: true,
  skillDir: "",
  configPath: "",
  mcpConfigPrefix: "",
  mcpConfigType: "common",
};

export const DEFAULT_MCP_APPLY_PREVIEW_DIALOG: McpApplyPreviewDialogState = {
  open: false,
  loading: false,
  submitting: false,
  serverName: "",
  targetId: null,
  preview: null,
};


export const AVAILABLE_PROJECT_AGENTS = ["claude", "codex", "opencode", "zcode"] as const;

export type ProjectFormState = {
  originalProjectId: string | null;
  projectId: string;
  path: string;
  agents: string[];
};

export const DEFAULT_PROJECT_FORM: ProjectFormState = {
  originalProjectId: null,
  projectId: "",
  path: "",
  agents: ["claude", "codex"],
};

export type ProjectDeleteDialogState = {
  open: boolean;
  loading: boolean;
  projectId: string | null;
};

export type ProjectAgentPickerDialogState = {
  open: boolean;
  loading: boolean;
  contextName: string;
  projectId: string | null;
  serverName: string | null;
  selectedAgentId: AgentTargetId | null;
};

export const DEFAULT_PROJECT_AGENT_PICKER_DIALOG: ProjectAgentPickerDialogState = {
  open: false,
  loading: false,
  contextName: "",
  projectId: null,
  serverName: null,
  selectedAgentId: null,
};

export function createProjectAgentPickerDialogState(): ProjectAgentPickerDialogState {
  return { ...DEFAULT_PROJECT_AGENT_PICKER_DIALOG };
}
