import { reactive } from "vue";
import { deleteSkillSource, updateSkillSource } from "../api";
import {
  createSkillSourceEditDialogState,
  parseCommaSeparatedList,
  type SkillSourceEditDialogState,
} from "../model";
import type { SkillSourceConfigView } from "../types";
import { useSkillPreview } from "./useSkillPreview";
import { useWorkspaceAction } from "./useWorkspaceAction";

export function useSkillSourceEdit() {
  const { runWorkspaceAction } = useWorkspaceAction();

  const skillSourceEditDialog = reactive<SkillSourceEditDialogState>(createSkillSourceEditDialogState());
  const skillSourceDeleteDialog = reactive<{
    open: boolean;
    loading: boolean;
    error: string | null;
    source: SkillSourceConfigView | null;
  }>({
    open: false,
    loading: false,
    error: null,
    source: null,
  });

  const { discoverAndFilterSkills, invalidatePreviewRequests, setPreviewLoading, failPreview } =
    useSkillPreview(skillSourceEditDialog);

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
    skillSourceEditDialog.preview.previewError = null;

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
    const refreshErrorText = "刷新导入来源预览失败。";
    setPreviewLoading(true);
    preview.previewError = null;

    try {
      await discoverAndFilterSkills(
        { sourceType, repo, rootPath, ref },
        preview.includeNamePatternsText,
        preview.includePathPatternsText,
      );
    } catch (error) {
      failPreview(error, refreshErrorText);
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
      reload: true,
      error: "更新导入来源失败。",
      onSuccess: () => closeSkillSourceEditDialog(),
      onError: (message) => {
        skillSourceEditDialog.preview.previewError = message;
      },
    });
    skillSourceEditDialog.loading = false;
  }

  function openSkillSourceDeleteDialog(source: SkillSourceConfigView) {
    skillSourceDeleteDialog.open = true;
    skillSourceDeleteDialog.loading = false;
    skillSourceDeleteDialog.error = null;
    skillSourceDeleteDialog.source = source;
  }

  function closeSkillSourceDeleteDialog() {
    skillSourceDeleteDialog.open = false;
    skillSourceDeleteDialog.loading = false;
    skillSourceDeleteDialog.error = null;
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
      reload: true,
      error: `删除来源 ${source.label} 失败。`,
      onSuccess: () => closeSkillSourceDeleteDialog(),
      onError: (message) => {
        skillSourceDeleteDialog.error = message;
      },
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
    openSkillSourceDeleteDialog,
    closeSkillSourceDeleteDialog,
    handleConfirmDeleteSkillSource,
  };
}
