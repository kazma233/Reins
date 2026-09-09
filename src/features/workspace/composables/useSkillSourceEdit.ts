import { reactive } from "vue";
import { deleteSkillSource, refreshGitSkillSource, updateSkillSource } from "../api";
import {
  createSkillSourceEditDialogState,
  parseCommaSeparatedList,
  type SkillSourceEditDialogState,
} from "../model";
import type { SkillSourceConfigView } from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
import { useSkillPreview } from "./useSkillPreview";
import { useWorkspaceNotice } from "./useWorkspaceNotice";
import { useWorkspaceAction } from "./useWorkspaceAction";

export function useSkillSourceEdit() {
  const { showNotice, clearNotice } = useWorkspaceNotice();
  const { runWorkspaceAction } = useWorkspaceAction();

  const skillSourceEditDialog = reactive<SkillSourceEditDialogState>(createSkillSourceEditDialogState());
  const skillSourceDeleteDialog = reactive<{
    open: boolean;
    loading: boolean;
    source: SkillSourceConfigView | null;
  }>({
    open: false,
    loading: false,
    source: null,
  });

  const { discoverAndFilterSkills, invalidatePreviewRequests, setPreviewLoading } = useSkillPreview(
    skillSourceEditDialog,
    "刷新导入来源预览失败。",
  );

  function openSkillSourceEditDialog(source: SkillSourceConfigView) {
    const baseDraft: Pick<SkillSourceEditDialogState, "sourceType" | "repo" | "rootPath" | "ref"> =
      source.type === "git"
        ? {
            sourceType: "git",
            repo: source.repo,
            rootPath: "",
            ref: source.ref ?? "",
          }
        : {
            sourceType: "local",
            repo: "",
            rootPath: source.rootPath,
            ref: "",
          };
    Object.assign(skillSourceEditDialog, createSkillSourceEditDialogState());
    skillSourceEditDialog.open = true;
    skillSourceEditDialog.loading = false;
    skillSourceEditDialog.sourceId = source.id;
    skillSourceEditDialog.title = `编辑导入来源: ${source.label}`;
    skillSourceEditDialog.sourceType = baseDraft.sourceType;
    skillSourceEditDialog.repo = baseDraft.repo;
    skillSourceEditDialog.rootPath = baseDraft.rootPath;
    skillSourceEditDialog.ref = baseDraft.ref;
    skillSourceEditDialog.preview.discovery = null;
    skillSourceEditDialog.preview.includeNamePatternsText = source.includeNamePatterns.join(", ");
    skillSourceEditDialog.preview.includePathPatternsText = source.includePathPatterns.join(", ");
    skillSourceEditDialog.preview.previewLoading = false;

    // Auto-refresh so the user immediately sees what the current include
    // filters resolve to (including the excluded list), without having to
    // remember to click the "刷新预览" button.
    void handleRefreshSkillSourceEditPreview();
  }

  function closeSkillSourceEditDialog() {
    invalidatePreviewRequests();
    Object.assign(skillSourceEditDialog, createSkillSourceEditDialogState());
  }

  async function handleRefreshSkillSourceEditPreview() {
    const { preview, sourceType, repo, rootPath, ref } = skillSourceEditDialog;
    setPreviewLoading(true);
    clearNotice();

    try {
      await discoverAndFilterSkills(
        { sourceType, repo, rootPath, ref },
        preview.includeNamePatternsText,
        preview.includePathPatternsText,
      );
    } catch (error) {
      showNotice(extractErrorMessage(error, "刷新导入来源预览失败。"), "error");
    }
  }

  async function handleConfirmSkillSourceEdit() {
    const sourceId = skillSourceEditDialog.sourceId;
    if (!sourceId) return;

    skillSourceEditDialog.loading = true;
    await runWorkspaceAction({
      action: () =>
        updateSkillSource({
          sourceId,
          ref: skillSourceEditDialog.sourceType === "git" ? skillSourceEditDialog.ref.trim() || null : null,
          includeNamePatterns: parseCommaSeparatedList(skillSourceEditDialog.preview.includeNamePatternsText),
          includePathPatterns: parseCommaSeparatedList(skillSourceEditDialog.preview.includePathPatternsText),
        }),
      success: "已更新导入来源。",
      error: "更新导入来源失败。",
      after: () => closeSkillSourceEditDialog(),
    });
    skillSourceEditDialog.loading = false;
  }

  async function handleRefreshGitSkillSource(source: SkillSourceConfigView) {
    if (source.type !== "git") return;

    await runWorkspaceAction({
      action: () => refreshGitSkillSource(source.id),
      success: `已从远端拉取 ${source.label}。`,
      error: "拉取远端来源失败。",
    });
  }

  function openSkillSourceDeleteDialog(source: SkillSourceConfigView) {
    skillSourceDeleteDialog.open = true;
    skillSourceDeleteDialog.loading = false;
    skillSourceDeleteDialog.source = source;
  }

  function closeSkillSourceDeleteDialog() {
    skillSourceDeleteDialog.open = false;
    skillSourceDeleteDialog.loading = false;
    skillSourceDeleteDialog.source = null;
  }

  async function handleConfirmDeleteSkillSource() {
    const source = skillSourceDeleteDialog.source;
    if (!source) {
      closeSkillSourceDeleteDialog();
      return;
    }

    skillSourceDeleteDialog.loading = true;
    await runWorkspaceAction({
      action: () => deleteSkillSource(source.id),
      success: "已删除来源。",
      error: `删除来源 ${source.label} 失败。`,
      after: () => closeSkillSourceDeleteDialog(),
    });
    skillSourceDeleteDialog.loading = false;
  }

  return {
    skillSourceEditDialog,
    skillSourceDeleteDialog,
    openSkillSourceEditDialog,
    closeSkillSourceEditDialog,
    handleRefreshSkillSourceEditPreview,
    handleConfirmSkillSourceEdit,
    handleRefreshGitSkillSource,
    openSkillSourceDeleteDialog,
    closeSkillSourceDeleteDialog,
    handleConfirmDeleteSkillSource,
  };
}
