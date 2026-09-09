import { reactive } from "vue";
import {
  applyMcpToTarget,
  createWorkspaceMcp,
  deleteWorkspaceMcp,
  previewMcpTarget,
  removeMcpFromTarget,
  updateWorkspaceMcp,
} from "../api";
import {
  DEFAULT_MCP_APPLY_PREVIEW_DIALOG,
  DEFAULT_MCP_FORM,
  formatTargetLabel,
  type McpApplyPreviewDialogState,
  type McpDeleteDialogState,
  type McpFormState,
} from "../model";
import type {
  AgentTargetId,
  McpConfigView,
} from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
import { useWorkspaceNotice } from "./useWorkspaceNotice";
import { useWorkspaceState } from "./useWorkspaceState";
import { useWorkspaceAction } from "./useWorkspaceAction";

type WorkspaceMcpPayload = {
  name: string;
  enabled: boolean;
  transport: McpConfigView["transport"];
  homepage: string | null;
  command: string | null;
  args: string[];
  env: Record<string, string>;
  url: string | null;
  headers: Record<string, string>;
  timeout: number | null;
};

type McpSyncConfirmDialogState = {
  open: boolean;
  loading: boolean;
  originalName: string;
  nextName: string;
  payload: WorkspaceMcpPayload | null;
  targetIds: AgentTargetId[];
};

type McpTargetRemoveDialogState = {
  open: boolean;
  loading: boolean;
  serverName: string;
  targetId: AgentTargetId | null;
};

const DEFAULT_MCP_SYNC_CONFIRM_DIALOG: McpSyncConfirmDialogState = {
  open: false,
  loading: false,
  originalName: "",
  nextName: "",
  payload: null,
  targetIds: [],
};

function parseKeyValueText(value: string): Record<string, string> {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .reduce<Record<string, string>>((result, line) => {
      const separatorIndex = line.indexOf("=");
      if (separatorIndex <= 0) {
        throw new Error(`格式错误: ${line}`);
      }
      const key = line.slice(0, separatorIndex).trim();
      const nextValue = line.slice(separatorIndex + 1).trim();
      if (key) {
        result[key] = nextValue;
      }
      return result;
    }, {});
}

function parseLineList(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

export function useMcpMutations() {
  const { showNotice } = useWorkspaceNotice();
  const { inspection } = useWorkspaceState();
  const { runWorkspaceAction } = useWorkspaceAction();

  const mcpCreateDialog = reactive<{
    open: boolean;
    loading: boolean;
    form: McpFormState;
  }>({
    open: false,
    loading: false,
    form: { ...DEFAULT_MCP_FORM },
  });

  const mcpDeleteDialog = reactive<McpDeleteDialogState>({
    open: false,
    loading: false,
    serverNames: [],
  });

  const mcpApplyPreviewDialog = reactive<McpApplyPreviewDialogState>({
    ...DEFAULT_MCP_APPLY_PREVIEW_DIALOG,
  });

  const mcpSyncConfirmDialog = reactive<McpSyncConfirmDialogState>({
    ...DEFAULT_MCP_SYNC_CONFIRM_DIALOG,
  });

  const mcpTargetRemoveDialog = reactive<McpTargetRemoveDialogState>({
    open: false,
    loading: false,
    serverName: "",
    targetId: null,
  });

  function getInstalledMcpTargetIds(serverName: string): AgentTargetId[] {
    return Array.from(
      new Set(
        inspection.value?.mcps
          .find((mcp) => mcp.name === serverName)
          ?.targets.filter((target) => target.state === "present")
          .map((target) => target.targetId) ?? [],
      ),
    );
  }

  // Serial-apply each target id: concurrent writes would clobber each other's
  // config file mutations. Keep the loop `for await` — do not Promise.all it.
  async function persistWorkspaceMcp(
    payload: WorkspaceMcpPayload,
    currentServerName: string | null,
    syncTargetIds: AgentTargetId[],
  ) {
    if (currentServerName) {
      await updateWorkspaceMcp(currentServerName, payload);
    } else {
      await createWorkspaceMcp(payload);
    }

    if (!currentServerName || syncTargetIds.length === 0) {
      return;
    }

    for (const targetId of syncTargetIds) {
      if (!payload.enabled) {
        await removeMcpFromTarget(currentServerName, targetId);
        continue;
      }

      await applyMcpToTarget(payload.name, targetId);

      if (currentServerName !== payload.name) {
        await removeMcpFromTarget(currentServerName, targetId);
      }
    }
  }

  function openMcpCreateDialog() {
    mcpCreateDialog.form = { ...DEFAULT_MCP_FORM };
    mcpCreateDialog.open = true;
  }

  function openMcpEditDialog(mcp: McpConfigView) {
    mcpCreateDialog.form = {
      originalName: mcp.name,
      name: mcp.name,
      enabled: mcp.enabled,
      transport: mcp.transport,
      createdAt: mcp.createdAt,
      homepage: mcp.homepage ?? "",
      command: mcp.command ?? "",
      args: mcp.args.join("\n"),
      env: Object.entries(mcp.env ?? {})
        .map(([key, value]) => `${key}=${value}`)
        .join("\n"),
      url: mcp.url ?? "",
      headers: Object.entries(mcp.headers)
        .map(([key, value]) => `${key}=${value}`)
        .join("\n"),
      timeout: mcp.timeout === null ? "" : String(mcp.timeout),
    };
    mcpCreateDialog.open = true;
  }

  function closeMcpCreateDialog() {
    Object.assign(mcpSyncConfirmDialog, DEFAULT_MCP_SYNC_CONFIRM_DIALOG);
    mcpCreateDialog.open = false;
    mcpCreateDialog.form = { ...DEFAULT_MCP_FORM };
  }

  function closeMcpSyncConfirmDialog() {
    Object.assign(mcpSyncConfirmDialog, DEFAULT_MCP_SYNC_CONFIRM_DIALOG);
  }

  async function handleCreateWorkspaceMcp() {
    const form = mcpCreateDialog.form;
    const name = form.name.trim();

    if (!name) {
      showNotice("请填写 mcp 名称。", "error");
      return;
    }

    if (form.transport === "stdio" && !form.command.trim()) {
      showNotice("stdio 模式需要填写 command。", "error");
      return;
    }

    if (form.transport !== "stdio" && !form.url.trim()) {
      showNotice("远程模式需要填写 mcp 链接。", "error");
      return;
    }

    const timeout = form.timeout.trim() ? Number(form.timeout.trim()) : null;
    if (timeout !== null && (!Number.isFinite(timeout) || timeout < 0)) {
      showNotice("timeout 需要是大于等于 0 的数字。", "error");
      return;
    }

    let payload: WorkspaceMcpPayload;
    try {
      payload = {
        name,
        enabled: form.enabled,
        transport: form.transport,
        homepage: form.homepage.trim() || null,
        command: form.transport === "stdio" ? form.command.trim() : null,
        args: form.transport === "stdio" ? parseLineList(form.args) : [],
        env: form.transport === "stdio" ? parseKeyValueText(form.env) : {},
        url: form.transport === "stdio" ? null : form.url.trim(),
        headers: form.transport === "stdio" ? {} : parseKeyValueText(form.headers),
        timeout,
      };
    } catch (error) {
      showNotice(extractErrorMessage(error, "解析 mcp 配置失败。"), "error");
      return;
    }

    if (form.originalName) {
      const targetIds = getInstalledMcpTargetIds(form.originalName);
      if (targetIds.length) {
        mcpSyncConfirmDialog.open = true;
        mcpSyncConfirmDialog.loading = false;
        mcpSyncConfirmDialog.originalName = form.originalName;
        mcpSyncConfirmDialog.nextName = payload.name;
        mcpSyncConfirmDialog.payload = payload;
        mcpSyncConfirmDialog.targetIds = targetIds;
        return;
      }
    }

    const isEdit = form.originalName !== null;
    mcpCreateDialog.loading = true;
    await runWorkspaceAction({
      action: () => persistWorkspaceMcp(payload, form.originalName, []),
      success: isEdit ? `已更新 ${name}。` : `已添加 ${name}。`,
      error: isEdit ? "更新 mcp 失败。" : "添加 mcp 失败。",
      after: () => closeMcpCreateDialog(),
    });
    mcpCreateDialog.loading = false;
  }

  async function handleConfirmSyncMcpConfig() {
    const { originalName, payload, targetIds } = mcpSyncConfirmDialog;
    if (!originalName || !payload) return;

    mcpSyncConfirmDialog.loading = true;
    mcpCreateDialog.loading = true;
    await runWorkspaceAction({
      action: () => persistWorkspaceMcp(payload, originalName, targetIds),
      success: payload.enabled
        ? `已更新 ${payload.name}，并同步 ${targetIds.length} 个应用配置。`
        : `已更新 ${payload.name}，并清理 ${targetIds.length} 个应用配置。`,
      error: "更新 mcp 并同步应用配置失败。",
      after: () => {
        closeMcpSyncConfirmDialog();
        closeMcpCreateDialog();
      },
    });
    mcpSyncConfirmDialog.loading = false;
    mcpCreateDialog.loading = false;
  }

  function openMcpDeleteDialog(serverNames: string[]) {
    mcpDeleteDialog.open = true;
    mcpDeleteDialog.loading = false;
    mcpDeleteDialog.serverNames = serverNames;
  }

  function closeMcpDeleteDialog() {
    mcpDeleteDialog.open = false;
    mcpDeleteDialog.loading = false;
    mcpDeleteDialog.serverNames = [];
  }

  async function handleConfirmDeleteMcp() {
    const serverName = mcpDeleteDialog.serverNames[0];
    if (!serverName) {
      closeMcpDeleteDialog();
      return;
    }

    mcpDeleteDialog.loading = true;
    await runWorkspaceAction({
      action: () => deleteWorkspaceMcp(serverName),
      success: (result) => ({ message: `已删除 ${result.serverName}。` }),
      error: "删除 mcp 失败。",
      after: () => closeMcpDeleteDialog(),
    });
    mcpDeleteDialog.loading = false;
  }

  function openMcpTargetRemoveDialog(serverName: string, targetId: AgentTargetId) {
    mcpTargetRemoveDialog.open = true;
    mcpTargetRemoveDialog.loading = false;
    mcpTargetRemoveDialog.serverName = serverName;
    mcpTargetRemoveDialog.targetId = targetId;
  }

  function closeMcpTargetRemoveDialog() {
    mcpTargetRemoveDialog.open = false;
    mcpTargetRemoveDialog.loading = false;
    mcpTargetRemoveDialog.serverName = "";
    mcpTargetRemoveDialog.targetId = null;
  }

  async function confirmMcpTargetRemove() {
    const { serverName, targetId } = mcpTargetRemoveDialog;
    if (!serverName || !targetId) return;

    const targetLabel = formatTargetLabel(targetId);
    mcpTargetRemoveDialog.loading = true;
    await runWorkspaceAction({
      action: () => removeMcpFromTarget(serverName, targetId),
      success: (result) =>
        result.action === "noop"
          ? { message: `${serverName} 在 ${targetLabel} 未安装。`, tone: "info" }
          : { message: `已从 ${targetLabel} 移除 ${serverName}。` },
      error: `移除 ${serverName} 从 ${targetLabel} 失败。`,
      after: () => closeMcpTargetRemoveDialog(),
    });
    mcpTargetRemoveDialog.loading = false;
  }

  function handleToggleMcpTarget(serverName: string, targetId: AgentTargetId) {
    const targetItem =
      inspection.value?.mcps
        .find((mcp) => mcp.name === serverName)
        ?.targets.find((item) => item.targetId === targetId) ?? null;

    if (targetItem?.state === "present") {
      openMcpTargetRemoveDialog(serverName, targetId);
      return;
    }

    const targetLabel = formatTargetLabel(targetId);
    // Preview-only fetch: no `success` declaration, so no reload and no toast.
    void runWorkspaceAction({
      action: () => previewMcpTarget(serverName, targetId),
      error: `读取 ${serverName} 写入 ${targetLabel} 的预览失败。`,
      after: (preview) => {
        mcpApplyPreviewDialog.open = true;
        mcpApplyPreviewDialog.loading = false;
        mcpApplyPreviewDialog.submitting = false;
        mcpApplyPreviewDialog.serverName = serverName;
        mcpApplyPreviewDialog.targetId = targetId;
        mcpApplyPreviewDialog.preview = preview;
      },
    });
  }

  function closeMcpApplyPreviewDialog() {
    Object.assign(mcpApplyPreviewDialog, DEFAULT_MCP_APPLY_PREVIEW_DIALOG);
  }

  async function handleConfirmApplyMcpPreview() {
    const { serverName, targetId } = mcpApplyPreviewDialog;
    if (!serverName || !targetId) return;

    const targetLabel = formatTargetLabel(targetId);
    mcpApplyPreviewDialog.loading = true;
    mcpApplyPreviewDialog.submitting = true;
    await runWorkspaceAction({
      action: () => applyMcpToTarget(serverName, targetId),
      success: (result) =>
        result.action === "noop"
          ? { message: `${serverName} 在 ${targetLabel} 已是最新状态。`, tone: "info" }
          : { message: `已添加 ${serverName} 到 ${targetLabel}。` },
      error: `添加 ${serverName} 到 ${targetLabel} 失败。`,
      after: () => closeMcpApplyPreviewDialog(),
    });
    mcpApplyPreviewDialog.loading = false;
    mcpApplyPreviewDialog.submitting = false;
  }

  return {
    mcpCreateDialog,
    mcpDeleteDialog,
    mcpApplyPreviewDialog,
    mcpSyncConfirmDialog,
    mcpTargetRemoveDialog,
    openMcpCreateDialog,
    openMcpEditDialog,
    closeMcpCreateDialog,
    closeMcpSyncConfirmDialog,
    handleCreateWorkspaceMcp,
    handleConfirmSyncMcpConfig,
    openMcpDeleteDialog,
    closeMcpDeleteDialog,
    handleConfirmDeleteMcp,
    openMcpTargetRemoveDialog,
    closeMcpTargetRemoveDialog,
    confirmMcpTargetRemove,
    handleToggleMcpTarget,
    closeMcpApplyPreviewDialog,
    handleConfirmApplyMcpPreview,
  };
}
