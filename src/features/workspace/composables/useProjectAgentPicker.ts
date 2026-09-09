import { computed, reactive } from "vue";
import { applyMcpToTarget, removeMcpFromTarget } from "../api";
import {
  createProjectAgentPickerDialogState,
  formatTargetLabel,
  type ProjectAgentPickerDialogState,
} from "../model";
import type { AgentTargetId, TargetConfigView } from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
import { useWorkspaceNotice } from "./useWorkspaceNotice";
import { useWorkspaceState } from "./useWorkspaceState";

export type ProjectTargetEntry = {
  id: string;
  agents: TargetConfigView[];
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

  function openProjectAgentPickerForMcp(serverName: string, projectId: string) {
    const agents = projectAgentsByProjectId.value.get(projectId) ?? [];
    if (agents.length === 0) return;

    Object.assign(projectAgentPickerDialog, createProjectAgentPickerDialogState());
    projectAgentPickerDialog.open = true;
    projectAgentPickerDialog.loading = false;
    projectAgentPickerDialog.contextName = serverName;
    projectAgentPickerDialog.projectId = projectId;
    projectAgentPickerDialog.serverName = serverName;
  }

  function closeProjectAgentPickerDialog() {
    projectAgentPickerDialog.open = false;
    projectAgentPickerDialog.loading = false;
    projectAgentPickerDialog.selectedAgentId = null;
  }

  function setProjectAgentPickerSelectedAgent(agentId: AgentTargetId) {
    projectAgentPickerDialog.selectedAgentId = agentId;
  }

  async function handleConfirmProjectAgentPicker(agentId: AgentTargetId) {
    const state = projectAgentPickerDialog;
    if (state.loading || !state.projectId) return;

    const compositeTargetId = `${state.projectId}:${agentId}` as AgentTargetId;

    // MCP changes are reversible config writes, so confirm inline without a
    // second dialog.
    projectAgentPickerDialog.loading = true;

    try {
      if (state.serverName) {
        const targetItem =
          inspection.value?.mcps
            .find((item) => item.name === state.serverName)
            ?.targets.find((target) => target.targetId === compositeTargetId) ?? null;
        const installed = targetItem?.state === "present";
        if (installed) {
          await removeMcpFromTarget(state.serverName, compositeTargetId);
          showNotice(
            `已从 ${formatTargetLabel(compositeTargetId)} 卸载 MCP ${state.contextName}。`,
            "success",
          );
        } else {
          await applyMcpToTarget(state.serverName, compositeTargetId);
          showNotice(
            `已应用 MCP ${state.contextName} 到 ${formatTargetLabel(compositeTargetId)}。`,
            "success",
          );
        }
      }
      closeProjectAgentPickerDialog();
      await reloadWorkspaceState({ preserveNotice: true });
    } catch (error) {
      showNotice(extractErrorMessage(error, "同步项目目标失败"), "error");
      projectAgentPickerDialog.loading = false;
    }
  }

  return {
    projectAgentPickerDialog,
    enabledProjectEntries,
    projectAgentsByProjectId,
    pickerInstalledAgentIds,
    openProjectAgentPickerForMcp,
    closeProjectAgentPickerDialog,
    setProjectAgentPickerSelectedAgent,
    handleConfirmProjectAgentPicker,
  };
}
