import { reactive } from "vue";
import {
  importBatchGitSkills,
  importDiscoveredSkills,
  selectLocalSkillSourceDirectory,
} from "../api";
import {
  createSkillImportDialogState,
  parseCommaSeparatedList,
  type SkillImportDialogState,
} from "../model";
import { useSkillPreview, type SkillSourcePreviewInput } from "./useSkillPreview";
import { useWorkspaceAction } from "./useWorkspaceAction";

function buildImportSourcePreviewInput(dialog: SkillImportDialogState): SkillSourcePreviewInput {
  return {
    sourceType: dialog.sourceType,
    repo: dialog.repo,
    rootPath: dialog.rootPath,
    ref: dialog.ref,
  };
}

export function useSkillImport() {
  const { runWorkspaceAction } = useWorkspaceAction();

  const skillImportDialog = reactive<SkillImportDialogState>(createSkillImportDialogState());
  const { discoverAndFilterSkills, invalidatePreviewRequests } = useSkillPreview(
    skillImportDialog,
    "刷新导入预览失败。",
  );

  function openImportDialog() {
    Object.assign(skillImportDialog, createSkillImportDialogState(skillImportDialog.sourceType));
    skillImportDialog.open = true;
  }

  function closeImportDialog() {
    invalidatePreviewRequests();
    Object.assign(skillImportDialog, createSkillImportDialogState(skillImportDialog.sourceType));
  }

  async function scanGitSkills() {
    const source = buildImportSourcePreviewInput(skillImportDialog);
    // Reset preview so the loading state is visible while discovery runs.
    skillImportDialog.preview.previewLoading = true;
    skillImportDialog.preview.discovery = null;

    await runWorkspaceAction({
      action: () =>
        discoverAndFilterSkills(
          source,
          skillImportDialog.preview.includeNamePatternsText,
          skillImportDialog.preview.includePathPatternsText,
        ),
      error: "发现 git skills 失败。",
    });
  }

  async function selectLocalDirectory() {
    await runWorkspaceAction({
      action: async () => {
        const selected = await selectLocalSkillSourceDirectory(skillImportDialog.rootPath);
        if (!selected) return;

        skillImportDialog.sourceType = "local";
        skillImportDialog.rootPath = selected.workspaceDir;
        skillImportDialog.preview.previewLoading = true;
        skillImportDialog.preview.discovery = null;

        await discoverAndFilterSkills(
          buildImportSourcePreviewInput(skillImportDialog),
          skillImportDialog.preview.includeNamePatternsText,
          skillImportDialog.preview.includePathPatternsText,
        );
      },
      error: "选择本地目录失败。",
    });
  }

  async function handleImportSkills() {
    if (skillImportDialog.mode === "batch-git") {
      await runWorkspaceAction({
        action: () => importBatchGitSkills(skillImportDialog.batchYamlText),
        success: (result) => ({
          message: result.failedCount
            ? `已导入 ${result.importedCount} 个 skills，${result.failedCount} 个来源失败。`
            : `已导入 ${result.importedCount} 个 skills。`,
          tone: result.failedCount ? "error" : "success",
        }),
        error: "导入 skills 失败。",
        after: (result) => {
          // Batch results stay in the dialog so the user can review failures.
          skillImportDialog.loading = false;
          skillImportDialog.batchResult = result;
        },
      });
      return;
    }

    await runWorkspaceAction({
      action: async () => {
        const discoveryId = skillImportDialog.preview.discovery?.discoveryId ?? "";
        const namePatterns = parseCommaSeparatedList(skillImportDialog.preview.includeNamePatternsText);
        const pathPatterns = parseCommaSeparatedList(skillImportDialog.preview.includePathPatternsText);

        if (skillImportDialog.sourceType === "local" && !skillImportDialog.rootPath.trim()) {
          throw new Error("缺少本地来源目录。");
        }

        return importDiscoveredSkills(discoveryId, namePatterns, pathPatterns);
      },
      success: (result) => ({
        message: `已导入 ${Array.isArray(result) ? result.length : result.skills.length} 个 skills。`,
      }),
      error: "导入 skills 失败。",
      after: () => closeImportDialog(),
    });
  }

  return {
    skillImportDialog,
    openImportDialog,
    closeImportDialog,
    scanGitSkills,
    selectLocalDirectory,
    handleImportSkills,
  };
}
