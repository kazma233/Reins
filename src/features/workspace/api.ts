import { invoke } from "@tauri-apps/api/core";
import type {
  AgentTargetId,
  TargetConfigView,
  WorkspaceState,
  McpConfigType,
  McpTransport,
  McpTargetPreviewResult,
  McpTargetMutationResult,
  BatchGitSkillImportResult,
  SkillDiscoveryResult,
  WorkspaceSelection,
  WorkspaceMcpMutationResult,
  WorkspaceTargetMutationResult,
  SyncTargetOption,
  SyncSkillOptionsResult,
  SourceSyncInput,
  SourceSyncConflict,
  SourceSyncResult,
} from "./types";

type WorkspaceTargetPayload = {
  targetId: string;
  enabled: boolean;
  skillDir: string;
  configPath?: string | null;
  mcpConfigPrefix: string;
  mcpConfigType: McpConfigType;
};

type WorkspaceMcpPayload = {
  name: string;
  enabled?: boolean;
  transport: McpTransport;
  homepage?: string | null;
  command?: string | null;
  args?: string[];
  env?: Record<string, string>;
  url?: string | null;
  headers?: Record<string, string>;
  timeout?: number | null;
};

type SkillSourceMutationInput = {
  sourceId: string;
  ref?: string | null;
  includeNamePatterns: string[];
  includePathPatterns: string[];
};

function buildWorkspaceMcpPayload(payload: WorkspaceMcpPayload) {
  return {
    name: payload.name,
    enabled: payload.enabled ?? true,
    transport: payload.transport,
    homepage: payload.homepage ?? null,
    command: payload.command ?? null,
    args: payload.args ?? [],
    env: payload.env ?? {},
    url: payload.url ?? null,
    headers: payload.headers ?? {},
    timeout: payload.timeout ?? null
  };
}

function buildWorkspaceTargetPayload(payload: WorkspaceTargetPayload) {
  return {
    targetId: payload.targetId,
    enabled: payload.enabled,
    skillDir: payload.skillDir,
    configPath: payload.configPath ?? null,
    mcpConfigPrefix: payload.mcpConfigPrefix,
    mcpConfigType: payload.mcpConfigType,
  };
}

export function getBuiltinTargetPreset(targetId: string): Promise<TargetConfigView> {
  return invoke("get_builtin_target_preset", { targetId });
}

export function getWorkspaceState(): Promise<WorkspaceState> {
  return invoke("get_workspace_state");
}

export function selectProjectPath(
  projectPath?: string
): Promise<WorkspaceSelection | null> {
  return invoke("select_project_path", { projectPath: projectPath ?? null });
}

export function selectLocalSkillSourceDirectory(
  sourcePath?: string
): Promise<WorkspaceSelection | null> {
  return invoke("select_local_skill_source_directory", { sourcePath: sourcePath ?? null });
}

export function selectTargetSkillDirectory(
  currentPath?: string
): Promise<WorkspaceSelection | null> {
  return invoke("select_target_skill_directory", { currentPath: currentPath ?? null });
}

export function selectTargetMcpConfigFile(
  currentPath?: string
): Promise<WorkspaceSelection | null> {
  return invoke("select_target_mcp_config_file", { currentPath: currentPath ?? null });
}

export function discoverGitSkills(
  repo: string,
  ref?: string
): Promise<SkillDiscoveryResult> {
  return invoke("discover_git_skills", {
    repo,
    reference: ref ?? null,
  });
}

export function discoverLocalSkills(
  sourcePath: string
): Promise<SkillDiscoveryResult> {
  return invoke("discover_local_skills", {
    sourcePath,
  });
}

export function filterDiscoveredSkills(
  discoveryId: string,
  includeNamePatterns?: string[],
  includePathPatterns?: string[]
): Promise<SkillDiscoveryResult> {
  return invoke("filter_discovered_skills", {
    discoveryId,
    includeNamePatterns: includeNamePatterns ?? null,
    includePathPatterns: includePathPatterns ?? null,
  });
}

export function importDiscoveredSkills(
  discoveryId: string,
  includeNamePatterns?: string[],
  includePathPatterns?: string[]
): Promise<SkillDiscoveryResult> {
  return invoke("import_discovered_skills", {
    discoveryId,
    includeNamePatterns: includeNamePatterns ?? null,
    includePathPatterns: includePathPatterns ?? null,
  });
}

export function importBatchGitSkills(
  yamlContent: string
): Promise<BatchGitSkillImportResult> {
  return invoke("import_batch_git_skills", {
    input: {
      yamlContent
    }
  });
}

export function createWorkspaceTarget(
  payload: WorkspaceTargetPayload
): Promise<WorkspaceTargetMutationResult> {
  return invoke("create_workspace_target", {
    ...buildWorkspaceTargetPayload(payload)
  });
}

export function updateWorkspaceTarget(
  currentTargetId: string,
  payload: WorkspaceTargetPayload
): Promise<WorkspaceTargetMutationResult> {
  return invoke("update_workspace_target", {
    currentTargetId,
    ...buildWorkspaceTargetPayload(payload)
  });
}

export function deleteWorkspaceTarget(
  targetId: string
): Promise<WorkspaceTargetMutationResult> {
  return invoke("delete_workspace_target", { targetId });
}

export function createWorkspaceMcp(
  payload: WorkspaceMcpPayload
): Promise<WorkspaceMcpMutationResult> {
  return invoke("create_workspace_mcp", {
    ...buildWorkspaceMcpPayload(payload)
  });
}

export function updateWorkspaceMcp(
  serverName: string,
  payload: WorkspaceMcpPayload
): Promise<WorkspaceMcpMutationResult> {
  return invoke("update_workspace_mcp", {
    serverName,
    ...buildWorkspaceMcpPayload(payload)
  });
}

export function deleteWorkspaceMcp(
  serverName: string
): Promise<WorkspaceMcpMutationResult> {
  return invoke("delete_workspace_mcp", { serverName });
}

export function updateSkillSource(input: SkillSourceMutationInput): Promise<WorkspaceState> {
  return invoke("update_skill_source", { input });
}

export function refreshGitSkillSource(sourceId: string): Promise<void> {
  return invoke("refresh_git_skill_source", { sourceId });
}

export function applyMcpToTarget(
  serverName: string,
  targetId: AgentTargetId
): Promise<McpTargetMutationResult> {
  return invoke("apply_mcp_to_target", { serverName, targetId });
}

export function removeMcpFromTarget(
  serverName: string,
  targetId: AgentTargetId
): Promise<McpTargetMutationResult> {
  return invoke("remove_mcp_from_target", { serverName, targetId });
}

export function previewMcpTarget(
  serverName: string,
  targetId: AgentTargetId
): Promise<McpTargetPreviewResult> {
  return invoke("preview_mcp_target", { serverName, targetId });
}

export function createWorkspaceProject(
  projectId: string,
  path: string,
  agents: string[]
): Promise<WorkspaceTargetMutationResult> {
  return invoke("create_workspace_project", {
    input: { projectId, path, agents },
  });
}

export function updateWorkspaceProject(
  projectId: string,
  path: string,
  agents: string[]
): Promise<WorkspaceTargetMutationResult> {
  return invoke("update_workspace_project", {
    input: { projectId, path, agents },
  });
}

export function deleteWorkspaceProject(
  projectId: string
): Promise<WorkspaceTargetMutationResult> {
  return invoke("delete_workspace_project", { projectId });
}

export function deleteSkillSource(sourceId: string): Promise<WorkspaceState> {
  return invoke("delete_skill_source", { sourceId });
}

export function deleteSkillSources(sourceIds: string[]): Promise<void> {
  return invoke("delete_skill_sources", { sourceIds });
}

export function getSyncTargetOptions(): Promise<SyncTargetOption[]> {
  return invoke("get_sync_target_options");
}

export function getSyncSkillOptions(sourceId: string): Promise<SyncSkillOptionsResult> {
  return invoke("get_sync_skill_options", { sourceId });
}

export function syncSourceToTargets(
  input: SourceSyncInput
): Promise<SourceSyncResult> {
  return invoke("sync_source_to_targets", { input });
}

export function previewSourceSyncConflicts(
  input: SourceSyncInput
): Promise<SourceSyncConflict[]> {
  return invoke("preview_source_sync_conflicts", { input });
}

export function removeSourceSync(
  sourceId: string,
  targetIds: string[]
): Promise<SourceSyncResult> {
  return invoke("remove_source_sync", { sourceId, targetIds });
}
