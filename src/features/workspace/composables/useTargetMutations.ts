import { reactive, ref } from "vue";
import {
  createWorkspaceTarget,
  deleteWorkspaceTarget,
  getTargetPresets,
  selectTargetMcpConfigFile,
  selectTargetSkillDirectory,
  updateWorkspaceTarget,
} from "../api";
import {
  DEFAULT_TARGET_FORM,
  type FieldErrors,
  type TargetDeleteDialogState,
  type TargetFormState,
} from "../model";
import type { TargetConfigView, TargetPreset } from "../types";
import { useWorkspaceStore } from "../stores/workspace";
import { useWorkspaceAction } from "./useWorkspaceAction";

function buildTargetErrors(form: TargetFormState): FieldErrors {
  const errors: FieldErrors = {};
  const targetId = form.targetId.trim();
  const skillDir = form.skillDir.trim();

  if (!targetId) {
    errors.targetId = "请填写 target id。";
  }
  if (!skillDir) {
    errors.skillDir = "请填写 skills 目录。";
  }
  // MCP 配置文件与 configPrefix 的成对校验由后端按 target 的实际格式执行
  //（dsh 等格式允许空 prefix），错误信息直接展示在弹窗错误区。

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
    // 内置工具预设由后端下发（路径是 env 解析后的真实值），仅创建模式使用。
    presets: TargetPreset[];
    presetsLoading: boolean;
    presetsError: string | null;
    // 编辑模式下后端下发的 MCP 配置格式说明，创建模式为 null。
    mcpFormatDescription: string | null;
    // 编辑模式下后端下发的写入形状示例片段，创建模式为 null。
    mcpFormatExample: string | null;
  }>({
    open: false,
    loading: false,
    error: null,
    form: { ...DEFAULT_TARGET_FORM, errors: {} },
    presets: [],
    presetsLoading: false,
    presetsError: null,
    mcpFormatDescription: null,
    mcpFormatExample: null,
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

  function openTargetCreateDialog(): Promise<void> {
    targetCreateDialog.form = { ...DEFAULT_TARGET_FORM, errors: {} };
    targetCreateDialog.error = null;
    targetCreateDialog.mcpFormatDescription = null;
    targetCreateDialog.mcpFormatExample = null;
    targetCreateDialog.open = true;
    return loadTargetPresets();
  }

  function openTargetEditDialog(target: TargetConfigView) {
    targetCreateDialog.form = {
      originalTargetId: target.id,
      targetId: target.id,
      enabled: target.enabled,
      skillDir: target.skillDir ?? "",
      configPath: target.configPath ?? "",
      mcpConfigPrefix: target.mcpConfigPrefix,
      errors: {},
    };
    targetCreateDialog.error = null;
    targetCreateDialog.mcpFormatDescription = target.mcpFormatDescription;
    targetCreateDialog.mcpFormatExample = target.mcpFormatExample;
    targetCreateDialog.open = true;
  }

  function closeTargetCreateDialog() {
    targetCreateDialog.open = false;
    targetCreateDialog.error = null;
    targetCreateDialog.mcpFormatDescription = null;
    targetCreateDialog.mcpFormatExample = null;
    targetCreateDialog.form = { ...DEFAULT_TARGET_FORM, errors: {} };
  }

  function clearTargetFormError(field: string) {
    const errors = targetCreateDialog.form.errors;
    if (!errors[field]) return;
    const next = { ...errors };
    delete next[field];
    targetCreateDialog.form.errors = next;
  }

  // 弹窗打开时拉取内置预设；失败常驻在预设区并给重试入口（不阻塞弹窗
  // 其余部分），成功前预设按钮为空。拉取不占整页忙碌态。
  async function loadTargetPresets() {
    targetCreateDialog.presetsLoading = true;
    targetCreateDialog.presetsError = null;
    await runWorkspaceAction({
      action: () => getTargetPresets(),
      pageLock: false,
      error: "读取内置工具预设失败。",
      onSuccess: (presets) => {
        targetCreateDialog.presets = presets;
      },
      onError: (message) => {
        targetCreateDialog.presetsError = message;
      },
    });
    targetCreateDialog.presetsLoading = false;
  }

  // 点击预设即回填表单：id 锁定为预设 id，路径/prefix 允许在回填后手改。
  // presetId 是下发预设里的 targetId（封闭集合），故用 string 接收。
  function handleApplyBuiltinTargetPreset(presetId: string) {
    const preset = targetCreateDialog.presets.find((item) => item.targetId === presetId);
    if (!preset) return;
    Object.assign(targetCreateDialog.form, {
      targetId: preset.targetId,
      enabled: preset.enabled,
      skillDir: preset.skillDir,
      configPath: preset.configPath ?? "",
      mcpConfigPrefix: preset.mcpConfigPrefix,
      errors: {},
    });
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
    loadTargetPresets,
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
