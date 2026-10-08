import { reactive } from "vue";
import {
  createWorkspaceProject,
  deleteWorkspaceProject,
  selectProjectPath,
  updateWorkspaceProject,
} from "../api";
import {
  DEFAULT_PROJECT_FORM,
  type FieldErrors,
  type ProjectDeleteDialogState,
  type ProjectFormState,
} from "../model";
import type { ProjectConfigView } from "../types";
import { useWorkspaceAction } from "./useWorkspaceAction";

function buildProjectErrors(form: ProjectFormState): FieldErrors {
  const errors: FieldErrors = {};
  if (!form.projectId.trim()) {
    errors.projectId = "请填写项目 ID，或先选择项目路径自动带出。";
  }
  if (!form.path.trim()) {
    errors.path = "请选择项目路径。";
  }
  return errors;
}

export function useProjectMutations() {
  const { runWorkspaceAction } = useWorkspaceAction();

  const projectCreateDialog = reactive<{
    open: boolean;
    loading: boolean;
    error: string | null;
    form: ProjectFormState;
  }>({
    open: false,
    loading: false,
    error: null,
    form: { ...DEFAULT_PROJECT_FORM, errors: {} },
  });

  const projectDeleteDialog = reactive<ProjectDeleteDialogState>({
    open: false,
    loading: false,
    error: null,
    projectId: null,
  });

  function openProjectCreateDialog() {
    projectCreateDialog.form = { ...DEFAULT_PROJECT_FORM, errors: {} };
    projectCreateDialog.error = null;
    projectCreateDialog.open = true;
  }

  function openProjectEditDialog(project: ProjectConfigView) {
    projectCreateDialog.form = {
      originalProjectId: project.id,
      projectId: project.id,
      path: project.path,
      agents: project.agents.filter((a) => a.enabled).map((a) => a.id),
      errors: {},
    };
    projectCreateDialog.error = null;
    projectCreateDialog.open = true;
  }

  function closeProjectCreateDialog() {
    projectCreateDialog.open = false;
    projectCreateDialog.error = null;
    projectCreateDialog.form = { ...DEFAULT_PROJECT_FORM, errors: {} };
  }

  function clearProjectFormError(field: string) {
    const errors = projectCreateDialog.form.errors;
    if (!errors[field]) return;
    const next = { ...errors };
    delete next[field];
    projectCreateDialog.form.errors = next;
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
        clearProjectFormError("path");
        clearProjectFormError("projectId");
      },
      error: "选择项目路径失败。",
      onError: (message) => {
        projectCreateDialog.error = message;
      },
    });
  }

  async function handleSubmitProject() {
    const form = projectCreateDialog.form;
    const errors = buildProjectErrors(form);
    form.errors = errors;
    if (Object.keys(errors).length > 0) {
      return;
    }

    const isEdit = form.originalProjectId !== null;

    projectCreateDialog.loading = true;
    await runWorkspaceAction({
      action: () =>
        isEdit
          ? updateWorkspaceProject(form.projectId, form.path, form.agents)
          : createWorkspaceProject(form.projectId, form.path, form.agents),
      reload: true,
      error: "保存项目失败。",
      onSuccess: () => closeProjectCreateDialog(),
      onError: (message) => {
        projectCreateDialog.error = message;
      },
    });
    projectCreateDialog.loading = false;
  }

  function openProjectDeleteDialog(projectId: string) {
    projectDeleteDialog.open = true;
    projectDeleteDialog.loading = false;
    projectDeleteDialog.error = null;
    projectDeleteDialog.projectId = projectId;
  }

  function closeProjectDeleteDialog() {
    projectDeleteDialog.open = false;
    projectDeleteDialog.loading = false;
    projectDeleteDialog.error = null;
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
      reload: true,
      error: "删除项目失败。",
      onSuccess: () => closeProjectDeleteDialog(),
      onError: (message) => {
        projectDeleteDialog.error = message;
      },
    });
    projectDeleteDialog.loading = false;
  }

  return {
    projectCreateDialog,
    projectDeleteDialog,
    openProjectCreateDialog,
    openProjectEditDialog,
    closeProjectCreateDialog,
    clearProjectFormError,
    handlePickProjectPath,
    handleSubmitProject,
    openProjectDeleteDialog,
    closeProjectDeleteDialog,
    handleConfirmDeleteProject,
  };
}
