import { reactive, ref } from "vue";
import {
  createWorkspaceTarget,
  deleteWorkspaceTarget,
  getBuiltinTargetPreset,
  selectTargetMcpConfigFile,
  selectTargetSkillDirectory,
  updateWorkspaceTarget,
} from "../api";
import {
  BUILTIN_TARGET_PRESETS,
  DEFAULT_TARGET_FORM,
  type BuiltinTargetPresetId,
  type FieldErrors,
  type TargetDeleteDialogState,
  type TargetFormState,
} from "../model";
import type { TargetConfigView } from "../types";
import { useWorkspaceStore } from "../stores/workspace";
import { useWorkspaceAction } from "./useWorkspaceAction";

function buildTargetErrors(form: TargetFormState): FieldErrors {
  const errors: FieldErrors = {};
  const targetId = form.targetId.trim();
  const skillDir = form.skillDir.trim();
  const configPath = form.configPath.trim();
  const mcpConfigPrefix = form.mcpConfigPrefix.trim();

  if (!targetId) {
    errors.targetId = "请填写 target id。";
  }
  if (!skillDir) {
    errors.skillDir = "请填写 skills 目录。";
  }
  // MCP 配置文件和 configPrefix 成对填写；不需要 MCP 分发的 target 允许
  // 两者都为空，此时只做 skill 分发。dsh 的 Cordis patch 按条目定位 server，
  // 没有 configPrefix，允许“有路径 + 空 prefix”。
  if (configPath && !mcpConfigPrefix && form.mcpConfigType !== "dsh") {
    errors.mcpConfigPrefix = "请填写 configPrefix。";
  }
  if (!configPath && mcpConfigPrefix) {
    errors.configPath = "填写了 configPrefix 时需要同时填写 MCP 配置文件路径。";
  }

  return errors;
}

export function useTargetMutations() {
  const store = useWorkspaceStore();
  const { runWorkspaceAction } = useWorkspaceAction();

  const targetCreateDialog = reactive<{
    open: boolean;
    loading: boolean;
    error: string | null;
    form: TargetFormState;
  }>({
    open: false,
    loading: false,
    error: null,
    form: { ...DEFAULT_TARGET_FORM, errors: {} },
  });

  const targetDeleteDialog = reactive<TargetDeleteDialogState>({
    open: false,
    loading: false,
    error: null,
    targetId: null,
  });

  // 正在启停的 target：只给这些卡片显示「停用中/启用中」，页面其余部分不变。
  const pendingToggleTargetIds = ref<Set<string>>(new Set());
  // 启停依次执行：并发时两次写入与两次重读会互相覆盖快照。
  let toggleQueue: Promise<void> = Promise.resolve();

  function markTogglePending(targetId: string, pending: boolean) {
    const next = new Set(pendingToggleTargetIds.value);
    if (pending) next.add(targetId);
    else next.delete(targetId);
    pendingToggleTargetIds.value = next;
  }

  function openTargetCreateDialog() {
    targetCreateDialog.form = { ...DEFAULT_TARGET_FORM, errors: {} };
    targetCreateDialog.error = null;
    targetCreateDialog.open = true;
  }

  function openTargetEditDialog(target: TargetConfigView) {
    targetCreateDialog.form = {
      originalTargetId: target.id,
      targetId: target.id,
      enabled: target.enabled,
      skillDir: target.skillDir ?? "",
      configPath: target.configPath ?? "",
      mcpConfigPrefix: target.mcpConfigPrefix,
      mcpConfigType: target.mcpConfigType,
      errors: {},
    };
    targetCreateDialog.error = null;
    targetCreateDialog.open = true;
  }

  function closeTargetCreateDialog() {
    targetCreateDialog.open = false;
    targetCreateDialog.error = null;
    targetCreateDialog.form = { ...DEFAULT_TARGET_FORM, errors: {} };
  }

  function clearTargetFormError(field: string) {
    const errors = targetCreateDialog.form.errors;
    if (!errors[field]) return;
    const next = { ...errors };
    delete next[field];
    targetCreateDialog.form.errors = next;
  }

  async function handleApplyBuiltinTargetPreset(presetId: BuiltinTargetPresetId) {
    if (targetCreateDialog.loading) return;
    const preset = BUILTIN_TARGET_PRESETS[presetId];
    if (preset) {
      Object.assign(targetCreateDialog.form, preset, { errors: {} });
      return;
    }
    targetCreateDialog.loading = true;
    try {
      await runWorkspaceAction({
        action: () => getBuiltinTargetPreset(presetId),
        error: "读取内置 target 默认值失败。",
        onSuccess: (target) => Object.assign(targetCreateDialog.form, {
          targetId: target.id,
          enabled: target.enabled,
          skillDir: target.skillDir,
          configPath: target.configPath ?? "",
          mcpConfigPrefix: target.mcpConfigPrefix,
          mcpConfigType: target.mcpConfigType,
          errors: {},
        }),
        onError: (message) => {
          targetCreateDialog.error = message;
        },
      });
    } finally {
      targetCreateDialog.loading = false;
    }
  }

  // 启停是显式按钮操作：按钮文案、状态 pill 与列表都会随 reload 更新，
  // 不需要成功提示；失败常驻在面板上（卡片随 reload 已更新，错误不能挂在卡片上）。
  // 它只影响一张卡，因此不占用整页忙碌态：卡片自己显示进行中状态。
  function toggleTargetEnabled(target: TargetConfigView): Promise<void> {
    const resultKey = `target-toggle:${target.id}`;
    store.setActionResult(resultKey, null);
    markTogglePending(target.id, true);

    toggleQueue = toggleQueue.then(async () => {
      try {
        await runWorkspaceAction({
          action: () =>
            updateWorkspaceTarget(target.id, {
              targetId: target.id,
              enabled: !target.enabled,
              skillDir: target.skillDir,
              configPath: target.configPath,
              mcpConfigPrefix: target.mcpConfigPrefix,
              mcpConfigType: target.mcpConfigType,
            }),
          reload: true,
          pageLock: false,
          error: target.enabled ? "停用 target 失败。" : "启用 target 失败。",
          onError: (message) => {
            store.setActionResult(resultKey, { message, failed: true });
          },
        });
      } finally {
        markTogglePending(target.id, false);
      }
    });

    return toggleQueue;
  }

  function openTargetDeleteDialog(targetId: string) {
    targetDeleteDialog.open = true;
    targetDeleteDialog.loading = false;
    targetDeleteDialog.error = null;
    targetDeleteDialog.targetId = targetId;
  }

  function closeTargetDeleteDialog() {
    targetDeleteDialog.open = false;
    targetDeleteDialog.loading = false;
    targetDeleteDialog.error = null;
    targetDeleteDialog.targetId = null;
  }

  async function handlePickTargetSkillDirectory() {
    await runWorkspaceAction({
      action: async () => {
        const currentPath = targetCreateDialog.form.skillDir.trim() || undefined;
        const selected = await selectTargetSkillDirectory(currentPath);
        if (!selected) return;
        targetCreateDialog.form.skillDir = selected.workspaceDir;
        clearTargetFormError("skillDir");
      },
      error: "选择 skills 目录失败。",
      onError: (message) => {
        targetCreateDialog.error = message;
      },
    });
  }

  async function handlePickTargetMcpConfigFile() {
    await runWorkspaceAction({
      action: async () => {
        const selected = await selectTargetMcpConfigFile(
          targetCreateDialog.form.configPath.trim() || undefined,
        );
        if (!selected) return;
        targetCreateDialog.form.configPath = selected.workspaceDir;
        clearTargetFormError("configPath");
      },
      error: "选择 MCP 配置文件失败。",
      onError: (message) => {
        targetCreateDialog.error = message;
      },
    });
  }

  async function handleSubmitTarget() {
    const form = targetCreateDialog.form;
    const errors = buildTargetErrors(form);
    form.errors = errors;
    if (Object.keys(errors).length > 0) {
      return;
    }

    const targetId = form.targetId.trim();
    const originalTargetId = form.originalTargetId;
    const isEdit = originalTargetId !== null;
    const payload = {
      targetId,
      enabled: form.enabled,
      skillDir: form.skillDir.trim(),
      configPath: form.configPath.trim() || null,
      mcpConfigPrefix: form.mcpConfigPrefix.trim(),
      mcpConfigType: form.mcpConfigType,
    };

    targetCreateDialog.loading = true;
    await runWorkspaceAction({
      action: () =>
        isEdit
          ? updateWorkspaceTarget(originalTargetId, payload)
          : createWorkspaceTarget(payload),
      reload: true,
      error: isEdit ? "更新 target 失败。" : "添加 target 失败。",
      onSuccess: () => closeTargetCreateDialog(),
      onError: (message) => {
        targetCreateDialog.error = message;
      },
    });
    targetCreateDialog.loading = false;
  }

  async function handleConfirmDeleteTarget() {
    const targetId = targetDeleteDialog.targetId;
    if (!targetId) {
      closeTargetDeleteDialog();
      return;
    }

    targetDeleteDialog.loading = true;
    await runWorkspaceAction({
      action: () => deleteWorkspaceTarget(targetId),
      reload: true,
      error: "删除 target 失败。",
      onSuccess: () => closeTargetDeleteDialog(),
      onError: (message) => {
        targetDeleteDialog.error = message;
      },
    });
    targetDeleteDialog.loading = false;
  }

  return {
    targetCreateDialog,
    targetDeleteDialog,
    pendingToggleTargetIds,
    openTargetCreateDialog,
    openTargetEditDialog,
    closeTargetCreateDialog,
    clearTargetFormError,
    handleApplyBuiltinTargetPreset,
    toggleTargetEnabled,
    openTargetDeleteDialog,
    closeTargetDeleteDialog,
    handlePickTargetSkillDirectory,
    handlePickTargetMcpConfigFile,
    handleSubmitTarget,
    handleConfirmDeleteTarget,
  };
}
