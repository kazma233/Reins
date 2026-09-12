import { reactive } from "vue";
import {
  createWorkspaceTarget,
  deleteWorkspaceTarget,
  selectTargetMcpConfigFile,
  selectTargetSkillDirectory,
  updateWorkspaceTarget,
} from "../api";
import {
  BUILTIN_TARGET_PRESETS,
  DEFAULT_TARGET_FORM,
  type BuiltinTargetPresetId,
  type TargetDeleteDialogState,
  type TargetFormState,
} from "../model";
import type { TargetConfigView } from "../types";
import { useWorkspaceNotice } from "./useWorkspaceNotice";
import { useWorkspaceAction } from "./useWorkspaceAction";

export function useTargetMutations() {
  const { showNotice } = useWorkspaceNotice();
  const { runWorkspaceAction } = useWorkspaceAction();

  const targetCreateDialog = reactive<{
    open: boolean;
    loading: boolean;
    form: TargetFormState;
  }>({
    open: false,
    loading: false,
    form: { ...DEFAULT_TARGET_FORM },
  });

  const targetDeleteDialog = reactive<TargetDeleteDialogState>({
    open: false,
    loading: false,
    targetId: null,
  });

  function openTargetCreateDialog() {
    targetCreateDialog.form = { ...DEFAULT_TARGET_FORM };
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
    };
    targetCreateDialog.open = true;
  }

  function closeTargetCreateDialog() {
    targetCreateDialog.open = false;
    targetCreateDialog.form = { ...DEFAULT_TARGET_FORM };
  }

  function handleApplyBuiltinTargetPreset(presetId: BuiltinTargetPresetId) {
    const preset = BUILTIN_TARGET_PRESETS[presetId];
    Object.assign(targetCreateDialog.form, preset);
  }

  async function toggleTargetEnabled(target: TargetConfigView) {
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
      success: target.enabled ? "已停用 target。" : "已启用 target。",
      error: target.enabled ? "停用 target 失败。" : "启用 target 失败。",
    });
  }

  function openTargetDeleteDialog(targetId: string) {
    targetDeleteDialog.open = true;
    targetDeleteDialog.loading = false;
    targetDeleteDialog.targetId = targetId;
  }

  function closeTargetDeleteDialog() {
    targetDeleteDialog.open = false;
    targetDeleteDialog.loading = false;
    targetDeleteDialog.targetId = null;
  }

  async function handlePickTargetSkillDirectory() {
    await runWorkspaceAction({
      action: async () => {
        const currentPath = targetCreateDialog.form.skillDir.trim() || undefined;
        const selected = await selectTargetSkillDirectory(currentPath);
        if (!selected) return;
        targetCreateDialog.form.skillDir = selected.workspaceDir;
      },
      error: "选择 skills 目录失败。",
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
      },
      error: "选择 MCP 配置文件失败。",
    });
  }

  async function handleSubmitTarget() {
    const form = targetCreateDialog.form;
    const targetId = form.targetId.trim();
    const skillDir = form.skillDir.trim();

    if (!targetId) {
      showNotice("请填写 target id。", "error");
      return;
    }
    if (!skillDir) {
      showNotice("请填写 skills 目录。", "error");
      return;
    }
    // MCP 配置文件和 configPrefix 成对填写；pi 这类不主动支持 MCP 的
    // target 允许两者都为空，此时只做 skill 分发。
    const configPath = form.configPath.trim();
    const mcpConfigPrefix = form.mcpConfigPrefix.trim();
    if (configPath && !mcpConfigPrefix) {
      showNotice("请填写 configPrefix。", "error");
      return;
    }
    if (!configPath && mcpConfigPrefix) {
      showNotice("填写了 configPrefix 时需要同时填写 MCP 配置文件路径。", "error");
      return;
    }

    const originalTargetId = form.originalTargetId;
    const isEdit = originalTargetId !== null;
    const payload = {
      targetId,
      enabled: form.enabled,
      skillDir,
      configPath: configPath || null,
      mcpConfigPrefix,
      mcpConfigType: form.mcpConfigType,
    };

    targetCreateDialog.loading = true;
    await runWorkspaceAction({
      action: () =>
        isEdit
          ? updateWorkspaceTarget(originalTargetId, payload)
          : createWorkspaceTarget(payload),
      success: isEdit ? `已更新 ${targetId}。` : `已添加 ${targetId}。`,
      error: isEdit ? "更新 target 失败。" : "添加 target 失败。",
      after: () => closeTargetCreateDialog(),
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
      success: (result) => ({ message: `已删除 ${result.targetId}。` }),
      error: "删除 target 失败。",
      after: () => closeTargetDeleteDialog(),
    });
    targetDeleteDialog.loading = false;
  }

  return {
    targetCreateDialog,
    targetDeleteDialog,
    openTargetCreateDialog,
    openTargetEditDialog,
    closeTargetCreateDialog,
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
