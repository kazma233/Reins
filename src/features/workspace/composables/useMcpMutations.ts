import { computed, reactive, ref } from "vue";
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
  type FieldErrors,
  type McpApplyPreviewDialogState,
  type McpDeleteDialogState,
  type McpFormState,
} from "../model";
import type {
  AgentTargetId,
  McpConfigView,
} from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
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
  error: string | null;
  originalName: string;
  nextName: string;
  payload: WorkspaceMcpPayload | null;
  targetIds: AgentTargetId[];
};

type McpTargetRemoveDialogState = {
  open: boolean;
  loading: boolean;
  error: string | null;
  serverName: string;
  targetId: AgentTargetId | null;
};

const DEFAULT_MCP_SYNC_CONFIRM_DIALOG: McpSyncConfirmDialogState = {
  open: false,
  loading: false,
  error: null,
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

// env/headers 是每行一条 KEY=VALUE，错误定位到具体那一行；字段错误挂在
// 对应输入框下方，用户改内容时清空。
function keyValueError(value: string, label: string): string | null {
  if (!value.trim()) return null;
  try {
    parseKeyValueText(value);
    return null;
  } catch (error) {
    const line = extractErrorMessage(error, "格式错误").replace(/^格式错误:\s*/, "");
    return `${label} 第「${line}」行缺少 “=”，每行需要写成 KEY=VALUE。`;
  }
}

function buildMcpErrors(form: McpFormState): FieldErrors {
  const errors: FieldErrors = {};

  if (!form.name.trim()) {
    errors.name = "请填写 mcp 名称。";
  }
  if (form.transport === "stdio" && !form.command.trim()) {
    errors.command = "stdio 模式需要填写 command。";
  }
  if (form.transport !== "stdio" && !form.url.trim()) {
    errors.url = "远程模式需要填写 mcp 链接。";
  }

  const timeout = form.timeout.trim() ? Number(form.timeout.trim()) : null;
  if (timeout !== null && (!Number.isFinite(timeout) || timeout < 0)) {
    errors.timeout = "timeout 需要是大于等于 0 的数字。";
  }

  const envError = form.transport === "stdio" ? keyValueError(form.env, "Env") : null;
  if (envError) {
    errors.env = envError;
  }
  const headersError = form.transport === "stdio" ? null : keyValueError(form.headers, "Headers");
  if (headersError) {
    errors.headers = headersError;
  }

  return errors;
}

export function useMcpMutations() {
  const { inspection, reloadWorkspaceState } = useWorkspaceState();
  const { runWorkspaceAction } = useWorkspaceAction();

  const mcpCreateDialog = reactive<{
    open: boolean;
    loading: boolean;
    form: McpFormState;
  }>({
    open: false,
    loading: false,
    form: { ...DEFAULT_MCP_FORM, errors: {} },
  });

  const mcpDeleteDialog = reactive<McpDeleteDialogState>({
    open: false,
    loading: false,
    error: null,
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
    error: null,
    serverName: "",
    targetId: null,
  });

  // 卡片上的「同步到目标」：把当前已落库的配置重新写入该 mcp 已安装的
  // 每个 target，确认弹窗先列出目标，完成后结果留在卡片上。
  const mcpCardSyncDialog = reactive<{
    open: boolean;
    loading: boolean;
    error: string | null;
    serverName: string;
    payload: WorkspaceMcpPayload | null;
    targetIds: AgentTargetId[];
  }>({
    open: false,
    loading: false,
    error: null,
    serverName: "",
    payload: null,
    targetIds: [],
  });
  // 结果按 mcp 名索引：卡片在弹窗关闭后仍要看到上一次同步写到了哪个目标。
  const mcpCardSyncNotice = ref<{ serverName: string; text: string; failed: boolean } | null>(null);

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
    mcpCreateDialog.form = { ...DEFAULT_MCP_FORM, errors: {} };
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
      errors: {},
    };
    mcpCreateDialog.open = true;
  }

  function clearMcpFormError(field: string) {
    if (!mcpCreateDialog.form.errors[field]) return;
    const next = { ...mcpCreateDialog.form.errors };
    delete next[field];
    mcpCreateDialog.form.errors = next;
  }

  function closeMcpCreateDialog() {
    Object.assign(mcpSyncConfirmDialog, DEFAULT_MCP_SYNC_CONFIRM_DIALOG);
    mcpCreateDialog.open = false;
    mcpCreateDialog.form = { ...DEFAULT_MCP_FORM, errors: {} };
  }

  function closeMcpSyncConfirmDialog() {
    Object.assign(mcpSyncConfirmDialog, DEFAULT_MCP_SYNC_CONFIRM_DIALOG);
  }

  // 校验一次算出全部错误：只在字段旁列出问题，不再逐条弹全局提示，
  // 把表单校验与 payload 组装抽出来：「保存」与「同步到已安装目标」共用
  // 同一套口径，避免两条路径对空值与格式的处理不一致。
  function buildMcpPayloadOrErrors(): WorkspaceMcpPayload | null {
    const form = mcpCreateDialog.form;

    let payload: WorkspaceMcpPayload;
    try {
      payload = {
        name: form.name.trim(),
        enabled: form.enabled,
        transport: form.transport,
        homepage: form.homepage.trim() || null,
        command: form.transport === "stdio" ? form.command.trim() : null,
        args: form.transport === "stdio" ? parseLineList(form.args) : [],
        env: form.transport === "stdio" ? parseKeyValueText(form.env) : {},
        url: form.transport === "stdio" ? null : form.url.trim(),
        headers: form.transport === "stdio" ? {} : parseKeyValueText(form.headers),
        timeout: form.timeout.trim() ? Number(form.timeout.trim()) : null,
      };
    } catch {
      // 行级格式错误由 buildMcpErrors 精准定位到 Env / Headers 字段。
      form.errors = buildMcpErrors(form);
      return null;
    }

    const errors = buildMcpErrors(form);
    form.errors = errors;
    return Object.keys(errors).length > 0 ? null : payload;
  }

  // 用户能看到所有待改字段而不是修一个才发现下一个。
  async function handleCreateWorkspaceMcp() {
    const form = mcpCreateDialog.form;
    const payload = buildMcpPayloadOrErrors();
    if (!payload) {
      return;
    }
    const name = payload.name;

    if (form.originalName) {
      // 「重命名并同步」只在名字真的变了才有意义：只是改连接参数时名字没变，
      // 弹一个重命名确认会误导用户，直接保存即可。
      const isRename = payload.name !== form.originalName;
      const targetIds = isRename ? getInstalledMcpTargetIds(form.originalName) : [];
      if (targetIds.length) {
        mcpSyncConfirmDialog.open = true;
        mcpSyncConfirmDialog.loading = false;
        mcpSyncConfirmDialog.error = null;
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
      reload: true,
      error: isEdit ? "更新 mcp 失败。" : "添加 mcp 失败。",
      onSuccess: () => closeMcpCreateDialog(),
      onError: (message) => {
        mcpCreateDialog.form.errors = { form: message };
      },
    });
    mcpCreateDialog.loading = false;
  }

  // 卡片上的同步入口：只对已安装到 target 的 mcp 有意义，先弹确认窗列出目标。
  function openMcpCardSyncDialog(mcp: McpConfigView) {
    const targetIds = getInstalledMcpTargetIds(mcp.name);
    if (targetIds.length === 0) return;

    mcpCardSyncDialog.open = true;
    mcpCardSyncDialog.loading = false;
    mcpCardSyncDialog.error = null;
    mcpCardSyncDialog.serverName = mcp.name;
    mcpCardSyncDialog.payload = {
      name: mcp.name,
      enabled: mcp.enabled,
      transport: mcp.transport,
      homepage: mcp.homepage,
      command: mcp.command,
      args: mcp.args,
      env: mcp.env,
      url: mcp.url,
      headers: mcp.headers,
      timeout: mcp.timeout,
    };
    mcpCardSyncDialog.targetIds = targetIds;
  }

  function closeMcpCardSyncDialog() {
    mcpCardSyncDialog.open = false;
    mcpCardSyncDialog.loading = false;
    mcpCardSyncDialog.error = null;
    mcpCardSyncDialog.serverName = "";
    mcpCardSyncDialog.payload = null;
    mcpCardSyncDialog.targetIds = [];
  }

  async function confirmMcpCardSync() {
    const { serverName, targetIds } = mcpCardSyncDialog;
    if (!serverName || targetIds.length === 0) return;

    // 逐个写入：并发改同一份配置文件会互相覆盖，失败目标不中断其余写入。
    mcpCardSyncDialog.loading = true;
    const failed: AgentTargetId[] = [];
    try {
      for (const targetId of targetIds) {
        try {
          await applyMcpToTarget(serverName, targetId);
        } catch {
          failed.push(targetId);
        }
      }
    } finally {
      mcpCardSyncDialog.loading = false;
    }

    await reloadWorkspaceState({ background: true });
    mcpCardSyncNotice.value = failed.length
      ? {
          serverName,
          text: `同步失败：${failed.map((id) => formatTargetLabel(id)).join("、")}`,
          failed: true,
        }
      : {
          serverName,
          text: `已同步到 ${targetIds.length} 个目标。`,
          failed: false,
        };
    closeMcpCardSyncDialog();
  }

  async function handleConfirmSyncMcpConfig() {
    const { originalName, payload, targetIds } = mcpSyncConfirmDialog;
    if (!originalName || !payload) return;

    mcpSyncConfirmDialog.loading = true;
    mcpCreateDialog.loading = true;
    await runWorkspaceAction({
      action: () => persistWorkspaceMcp(payload, originalName, targetIds),
      reload: true,
      error: "更新 mcp 并同步应用配置失败。",
      onSuccess: () => {
        closeMcpSyncConfirmDialog();
        closeMcpCreateDialog();
      },
      onError: (message) => {
        mcpSyncConfirmDialog.error = message;
      },
    });
    mcpSyncConfirmDialog.loading = false;
    mcpCreateDialog.loading = false;
  }

  function openMcpDeleteDialog(serverNames: string[]) {
    mcpDeleteDialog.open = true;
    mcpDeleteDialog.loading = false;
    mcpDeleteDialog.error = null;
    mcpDeleteDialog.serverNames = serverNames;
  }

  function closeMcpDeleteDialog() {
    mcpDeleteDialog.open = false;
    mcpDeleteDialog.loading = false;
    mcpDeleteDialog.error = null;
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
      reload: true,
      error: "删除 mcp 失败。",
      onSuccess: () => closeMcpDeleteDialog(),
      onError: (message) => {
        mcpDeleteDialog.error = message;
      },
    });
    mcpDeleteDialog.loading = false;
  }

  function openMcpTargetRemoveDialog(serverName: string, targetId: AgentTargetId) {
    mcpTargetRemoveDialog.open = true;
    mcpTargetRemoveDialog.loading = false;
    mcpTargetRemoveDialog.error = null;
    mcpTargetRemoveDialog.serverName = serverName;
    mcpTargetRemoveDialog.targetId = targetId;
  }

  function closeMcpTargetRemoveDialog() {
    mcpTargetRemoveDialog.open = false;
    mcpTargetRemoveDialog.loading = false;
    mcpTargetRemoveDialog.error = null;
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
      reload: true,
      error: `移除 ${serverName} 从 ${targetLabel} 失败。`,
      onSuccess: () => closeMcpTargetRemoveDialog(),
      onError: (message) => {
        mcpTargetRemoveDialog.error = message;
      },
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
    mcpApplyPreviewDialog.loading = true;
    // Preview-only fetch: no reload afterwards, the dialog opens with the
    // result; failures stay on the button that started it.
    void runWorkspaceAction({
      action: () => previewMcpTarget(serverName, targetId),
      error: `读取 ${serverName} 写入 ${targetLabel} 的预览失败。`,
      onSuccess: (preview) => {
        mcpApplyPreviewDialog.open = true;
        mcpApplyPreviewDialog.loading = false;
        mcpApplyPreviewDialog.submitting = false;
        mcpApplyPreviewDialog.error = null;
        mcpApplyPreviewDialog.serverName = serverName;
        mcpApplyPreviewDialog.targetId = targetId;
        mcpApplyPreviewDialog.preview = preview;
      },
      onError: (message) => {
        mcpApplyPreviewDialog.loading = false;
        mcpApplyPreviewDialog.error = message;
        mcpApplyPreviewDialog.serverName = serverName;
        mcpApplyPreviewDialog.targetId = targetId;
        mcpApplyPreviewDialog.preview = null;
        mcpApplyPreviewDialog.open = true;
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
      reload: true,
      error: `添加 ${serverName} 到 ${targetLabel} 失败。`,
      onSuccess: () => closeMcpApplyPreviewDialog(),
      onError: (message) => {
        mcpApplyPreviewDialog.error = message;
      },
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
    clearMcpFormError,
    mcpCardSyncDialog,
    mcpCardSyncNotice,
    openMcpCardSyncDialog,
    closeMcpCardSyncDialog,
    confirmMcpCardSync,
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
