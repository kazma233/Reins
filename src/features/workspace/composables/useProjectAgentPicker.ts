import { computed, reactive } from "vue";
import { applyMcpToTarget, removeMcpFromTarget } from "../api";
import {
  createProjectAgentPickerDialogState,
  formatTargetLabel,
  type ProjectAgentPickerDialogState,
} from "../model";
import type { AgentTargetId, TargetConfigView } from "../types";
import { useWorkspaceNotice } from "./useWorkspaceNotice";
import { useWorkspaceState } from "./useWorkspaceState";

export type ProjectTargetEntry = {
  id: string;
  agents: TargetConfigView[];
};

export type ProjectAgentPickerDiff = {
  toAdd: AgentTargetId[];
  toRemove: AgentTargetId[];
};

export function useProjectAgentPicker() {
  const { showNotice } = useWorkspaceNotice();
  const { reloadWorkspaceState, inspection, configDocument } = useWorkspaceState();

  const projectAgentPickerDialog = reactive<ProjectAgentPickerDialogState>(
    createProjectAgentPickerDialogState(),
  );

  // Derived list of (projectId, enabledAgents) for use by McpPanel and the
  // agent picker. Computed caches the projection so we don't recompute it
  // every render.
  const configProjects = computed(() => configDocument.value?.config?.projects ?? []);
  const projectEntries = computed<ProjectTargetEntry[]>(() =>
    configProjects.value.map((project) => ({
      id: project.id,
      agents: project.agents.filter((agent) => agent.enabled),
    })),
  );
  const enabledProjectEntries = computed(() =>
    projectEntries.value.filter((entry) => entry.agents.length > 0),
  );
  const projectAgentsByProjectId = computed(
    () => new Map(projectEntries.value.map((entry) => [entry.id, entry.agents] as const)),
  );

  // Agent ids that already have the MCP installed in this project. Empty when
  // the picker is closed or there's no matching project context.
  const pickerInstalledAgentIds = computed<Set<AgentTargetId>>(() => {
    const set = new Set<AgentTargetId>();
    if (!projectAgentPickerDialog.open) return set;
    const { projectId, serverName } = projectAgentPickerDialog;
    if (!projectId || !serverName) return set;
    const mcp = inspection.value?.mcps.find((item) => item.name === serverName);
    if (!mcp) return set;
    const prefix = `${projectId}:`;
    for (const target of mcp.targets) {
      if (target.targetId.startsWith(prefix) && target.state === "present") {
        set.add(target.targetId as AgentTargetId);
      }
    }
    return set;
  });

  // 弹窗展示的是期望终态，这里算出终态与现状的差异；顺序跟随项目 agents
  // 列表，确认弹窗里的展示顺序与编辑弹窗的按钮一致。
  const pickerPendingDiff = computed<ProjectAgentPickerDiff>(() => {
    const state = projectAgentPickerDialog;
    const agents = projectAgentsByProjectId.value.get(state.projectId ?? "") ?? [];
    const desired = new Set(state.desiredAgentIds);
    const toAdd: AgentTargetId[] = [];
    const toRemove: AgentTargetId[] = [];
    for (const agent of agents) {
      const compositeId = `${state.projectId}:${agent.id}` as AgentTargetId;
      const isDesired = desired.has(compositeId);
      const isInstalled = pickerInstalledAgentIds.value.has(compositeId);
      if (isDesired && !isInstalled) toAdd.push(compositeId);
      if (!isDesired && isInstalled) toRemove.push(compositeId);
    }
    return { toAdd, toRemove };
  });

  function openProjectAgentPickerForMcp(serverName: string, projectId: string) {
    const agents = projectAgentsByProjectId.value.get(projectId) ?? [];
    if (agents.length === 0) return;

    Object.assign(projectAgentPickerDialog, createProjectAgentPickerDialogState());
    projectAgentPickerDialog.open = true;
    projectAgentPickerDialog.loading = false;
    projectAgentPickerDialog.contextName = serverName;
    projectAgentPickerDialog.projectId = projectId;
    projectAgentPickerDialog.serverName = serverName;
    projectAgentPickerDialog.transport =
      configDocument.value?.config?.mcps.find((mcp) => mcp.name === serverName)?.transport ?? null;
    projectAgentPickerDialog.desiredAgentIds = [...pickerInstalledAgentIds.value];
  }

  function closeProjectAgentPickerDialog() {
    projectAgentPickerDialog.open = false;
    projectAgentPickerDialog.loading = false;
    projectAgentPickerDialog.confirmOpen = false;
    projectAgentPickerDialog.desiredAgentIds = [];
  }

  function toggleProjectAgentPickerAgent(agentId: AgentTargetId) {
    const state = projectAgentPickerDialog;
    if (!state.projectId) return;
    const compositeId = `${state.projectId}:${agentId}` as AgentTargetId;
    state.desiredAgentIds = state.desiredAgentIds.includes(compositeId)
      ? state.desiredAgentIds.filter((id) => id !== compositeId)
      : [...state.desiredAgentIds, compositeId];
  }

  function openProjectAgentPickerConfirm() {
    projectAgentPickerDialog.confirmOpen = true;
  }

  function closeProjectAgentPickerConfirm() {
    projectAgentPickerDialog.confirmOpen = false;
  }

  async function handleApplyProjectAgentPicker() {
    const state = projectAgentPickerDialog;
    if (state.loading || !state.serverName) return;
    const { toAdd, toRemove } = pickerPendingDiff.value;
    if (toAdd.length === 0 && toRemove.length === 0) {
      closeProjectAgentPickerDialog();
      return;
    }

    state.loading = true;

    // 单目标命令没有批量契约，逐个写入；单个失败不中断其余目标，
    // 结束后以 reload 回来的实际状态为准，失败目标在 notice 中点名。
    const failed: AgentTargetId[] = [];
    try {
      for (const targetId of toRemove) {
        try {
          await removeMcpFromTarget(state.serverName, targetId);
        } catch {
          failed.push(targetId);
        }
      }
      for (const targetId of toAdd) {
        try {
          await applyMcpToTarget(state.serverName, targetId);
        } catch {
          failed.push(targetId);
        }
      }
    } finally {
      state.loading = false;
      state.confirmOpen = false;
      state.open = false;
      state.desiredAgentIds = [];
    }

    await reloadWorkspaceState({ preserveNotice: true });

    if (failed.length > 0) {
      showNotice(
        `部分目标同步失败：${failed.map((id) => formatTargetLabel(id)).join("、")}`,
        "error",
      );
    } else {
      showNotice(
        `已同步 MCP ${state.contextName}：新增 ${toAdd.length} 个、移除 ${toRemove.length} 个。`,
        "success",
      );
    }
  }

  return {
    projectAgentPickerDialog,
    enabledProjectEntries,
    projectAgentsByProjectId,
    pickerInstalledAgentIds,
    pickerPendingDiff,
    openProjectAgentPickerForMcp,
    closeProjectAgentPickerDialog,
    toggleProjectAgentPickerAgent,
    openProjectAgentPickerConfirm,
    closeProjectAgentPickerConfirm,
    handleApplyProjectAgentPicker,
  };
}
