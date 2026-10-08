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
  const { discoverAndFilterSkills, invalidatePreviewRequests, clearPreview, failPreview } =
    useSkillPreview(skillImportDialog);

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
    clearPreview();

    await runWorkspaceAction({
      action: () =>
        discoverAndFilterSkills(
          source,
          skillImportDialog.preview.includeNamePatternsText,
          skillImportDialog.preview.includePathPatternsText,
        ),
      error: "发现 git skills 失败。",
      onError: (message) => failPreview(message, "发现 git skills 失败。"),
    });
  }

  async function selectLocalDirectory() {
    await runWorkspaceAction({
      action: async () => {
        const selected = await selectLocalSkillSourceDirectory(skillImportDialog.rootPath);
        if (!selected) return;

        skillImportDialog.sourceType = "local";
        skillImportDialog.rootPath = selected.workspaceDir;
        clearPreview();

        await discoverAndFilterSkills(
          buildImportSourcePreviewInput(skillImportDialog),
          skillImportDialog.preview.includeNamePatternsText,
          skillImportDialog.preview.includePathPatternsText,
        );
      },
      error: "选择本地目录失败。",
      // 取消选择也会走到这里（selected 为空直接返回），只有目录本身读取失败
      // 才需要预览区域提示；两者都是「没有结果」，用同一处常驻文案表达。
      onError: (message) => failPreview(message, "选择本地目录失败。"),
    });
  }

  async function handleImportSkills() {
    if (skillImportDialog.mode === "batch-git") {
      await runWorkspaceAction({
        action: () => importBatchGitSkills(skillImportDialog.batchYamlText),
        reload: true,
        error: "导入 skills 失败。",
        onSuccess: (result) => {
          // Batch results stay in the dialog so the user can review failures.
          skillImportDialog.loading = false;
          skillImportDialog.batchResult = result;
        },
        onError: (message) => {
          skillImportDialog.loading = false;
          skillImportDialog.preview.previewError = message;
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
      reload: true,
      error: "导入 skills 失败。",
      onSuccess: () => closeImportDialog(),
      onError: (message) => {
        skillImportDialog.preview.previewError = message;
      },
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
