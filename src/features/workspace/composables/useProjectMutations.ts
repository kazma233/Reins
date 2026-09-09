import { reactive } from "vue";
import {
  createWorkspaceProject,
  deleteWorkspaceProject,
  selectProjectPath,
  updateWorkspaceProject,
} from "../api";
import { DEFAULT_PROJECT_FORM, type ProjectDeleteDialogState, type ProjectFormState } from "../model";
import type { ProjectConfigView } from "../types";
import { useWorkspaceAction } from "./useWorkspaceAction";

export function useProjectMutations() {
  const { runWorkspaceAction } = useWorkspaceAction();

  const projectCreateDialog = reactive<{
    open: boolean;
    loading: boolean;
    form: ProjectFormState;
  }>({
    open: false,
    loading: false,
    form: { ...DEFAULT_PROJECT_FORM },
  });

  const projectDeleteDialog = reactive<ProjectDeleteDialogState>({
    open: false,
    loading: false,
    projectId: null,
  });

  function openProjectCreateDialog() {
    projectCreateDialog.form = { ...DEFAULT_PROJECT_FORM };
    projectCreateDialog.open = true;
  }

  function openProjectEditDialog(project: ProjectConfigView) {
    projectCreateDialog.form = {
      originalProjectId: project.id,
      projectId: project.id,
      path: project.path,
      agents: project.agents.filter((a) => a.enabled).map((a) => a.id),
    };
    projectCreateDialog.open = true;
  }

  function closeProjectCreateDialog() {
    projectCreateDialog.open = false;
    projectCreateDialog.form = { ...DEFAULT_PROJECT_FORM };
  }

  async function handlePickProjectPath() {
    await runWorkspaceAction({
      action: async () => {
        const currentPath = projectCreateDialog.form.path.trim() || undefined;
        const selected = await selectProjectPath(currentPath);
        if (!selected) return;
        const newPath = selected.workspaceDir;
        const shouldAutoFillId = !projectCreateDialog.form.projectId.trim();
        projectCreateDialog.form.path = newPath;
        if (shouldAutoFillId) {
          projectCreateDialog.form.projectId = newPath.split("/").pop() ?? "";
        }
      },
      error: "选择项目路径失败。",
    });
  }

  async function handleSubmitProject() {
    if (!projectCreateDialog.form.projectId.trim()) return;

    const form = projectCreateDialog.form;
    const isEdit = form.originalProjectId !== null;

    projectCreateDialog.loading = true;
    await runWorkspaceAction({
      action: () =>
        isEdit
          ? updateWorkspaceProject(form.projectId, form.path, form.agents)
          : createWorkspaceProject(form.projectId, form.path, form.agents),
      success: isEdit ? "项目配置已更新。" : "项目已创建。",
      error: "保存项目失败。",
      after: () => closeProjectCreateDialog(),
    });
    projectCreateDialog.loading = false;
  }

  function openProjectDeleteDialog(projectId: string) {
    projectDeleteDialog.open = true;
    projectDeleteDialog.loading = false;
    projectDeleteDialog.projectId = projectId;
  }

  function closeProjectDeleteDialog() {
    projectDeleteDialog.open = false;
    projectDeleteDialog.loading = false;
    projectDeleteDialog.projectId = null;
  }

  async function handleConfirmDeleteProject() {
    const projectId = projectDeleteDialog.projectId;
    if (!projectId) {
      closeProjectDeleteDialog();
      return;
    }

    projectDeleteDialog.loading = true;
    await runWorkspaceAction({
      action: () => deleteWorkspaceProject(projectId),
      success: "项目已删除。",
      error: "删除项目失败。",
      after: () => closeProjectDeleteDialog(),
    });
    projectDeleteDialog.loading = false;
  }

  return {
    projectCreateDialog,
    projectDeleteDialog,
    openProjectCreateDialog,
    openProjectEditDialog,
    closeProjectCreateDialog,
    handlePickProjectPath,
    handleSubmitProject,
    openProjectDeleteDialog,
    closeProjectDeleteDialog,
    handleConfirmDeleteProject,
  };
}
