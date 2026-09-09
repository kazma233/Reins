import { computed, ref, watch, type Ref } from "vue";
import { deleteSession, importSession, previewImport } from "../api";
import { defaultImportTarget } from "../model";
import type {
  ImportPreview,
  ImportResult,
  SessionOverview,
  SourceApp
} from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
import { createKeyGuard } from "@shared/lib/request-guard";

export type SessionDetailActionsState = {
  targetApp: Ref<SourceApp>;
  preview: Ref<ImportPreview | null>;
  previewTargetApp: Ref<SourceApp | null>;
  importResult: Ref<ImportResult | null>;
  previewError: Ref<string | null>;
  importError: Ref<string | null>;
  deleteError: Ref<string | null>;
  previewLoading: Ref<boolean>;
  importLoading: Ref<boolean>;
  deleteLoading: Ref<boolean>;
  deleteDialogOpen: Ref<boolean>;
  importDialogOpen: Ref<boolean>;
  previewReady: Ref<boolean>;
};

export type SessionDetailActions = SessionDetailActionsState & {
  openImportDialog: () => void;
  closeImportDialog: () => void;
  closeImportResultDialog: () => void;
  handleImport: () => Promise<void>;
  openDeleteDialog: () => void;
  closeDeleteDialog: () => void;
  handleDelete: () => Promise<void>;
};

// Import/delete flows for the session detail panel. All dialog state and the
// async actions live here; the panel only renders dialogs and header buttons.
// Guard key combines the detail key and the target app: a response is only
// applied when both are unchanged since the request was issued.
export function useSessionDetailActions(
  overview: Ref<SessionOverview | null>,
  detailKey: Ref<string | null>,
  onDeleted: () => void
): SessionDetailActions {
  const targetApp = ref<SourceApp>("claude_code");
  const preview = ref<ImportPreview | null>(null);
  const previewTargetApp = ref<SourceApp | null>(null);
  const importResult = ref<ImportResult | null>(null);
  const previewError = ref<string | null>(null);
  const importError = ref<string | null>(null);
  const deleteError = ref<string | null>(null);
  const previewLoading = ref(false);
  const importLoading = ref(false);
  const deleteLoading = ref(false);
  const deleteDialogOpen = ref(false);
  const importDialogOpen = ref(false);

  const requestGuard = createKeyGuard(
    () => `${detailKey.value ?? ""}::${targetApp.value}`
  );

  const previewReady = computed(() => previewTargetApp.value === targetApp.value);

  function resetState() {
    preview.value = null;
    previewTargetApp.value = null;
    importResult.value = null;
    previewError.value = null;
    importError.value = null;
    deleteError.value = null;
    previewLoading.value = false;
    importLoading.value = false;
    deleteLoading.value = false;
    deleteDialogOpen.value = false;
    importDialogOpen.value = false;
  }

  // Reset dialog/import/delete state when the displayed overview changes.
  watch(overview, (nextOverview) => {
    if (nextOverview) {
      targetApp.value = defaultImportTarget(nextOverview.summary.sourceApp);
    }
    resetState();
  });

  async function loadImportPreview() {
    const detail = overview.value;
    if (!detail || !detailKey.value) {
      return;
    }

    const requestKey = requestGuard.capture();
    previewLoading.value = true;
    previewTargetApp.value = null;
    previewError.value = null;
    importError.value = null;

    try {
      const nextPreview = await previewImport(
        detail.summary.sourceApp,
        detail.summary.sourceSessionId,
        targetApp.value,
        detail.summary.transcriptPath
      );

      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }

      preview.value = nextPreview;
      previewTargetApp.value = targetApp.value;
    } catch (error) {
      if (requestGuard.isCurrent(requestKey)) {
        previewError.value = extractErrorMessage(error, "导入预览失败。");
      }
    } finally {
      if (requestGuard.isCurrent(requestKey)) {
        previewLoading.value = false;
      }
    }
  }

  // Reload the preview whenever the dialog opens or the target app changes.
  watch([importDialogOpen, targetApp], ([isImportDialogOpen]) => {
    previewError.value = null;
    importError.value = null;

    if (isImportDialogOpen) {
      void loadImportPreview();
    }
  });

  function openImportDialog() {
    const detail = overview.value;
    if (!detail || importLoading.value || deleteLoading.value) {
      return;
    }
    importDialogOpen.value = true;
  }

  function closeImportDialog() {
    if (importLoading.value) {
      return;
    }
    importDialogOpen.value = false;
  }

  function closeImportResultDialog() {
    importResult.value = null;
  }

  async function handleImport() {
    const detail = overview.value;
    if (!detail || !detailKey.value) {
      return;
    }

    const requestKey = requestGuard.capture();
    importLoading.value = true;
    importError.value = null;

    try {
      const nextImportResult = await importSession(
        detail.summary.sourceApp,
        detail.summary.sourceSessionId,
        targetApp.value,
        detail.summary.transcriptPath
      );

      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }

      importDialogOpen.value = false;
      importResult.value = nextImportResult;
    } catch (error) {
      if (requestGuard.isCurrent(requestKey)) {
        importError.value = extractErrorMessage(error, "导入会话失败。");
      }
    } finally {
      if (requestGuard.isCurrent(requestKey)) {
        importLoading.value = false;
      }
    }
  }

  function openDeleteDialog() {
    const detail = overview.value;
    if (!detail || deleteLoading.value || importLoading.value) {
      return;
    }
    deleteError.value = null;
    deleteDialogOpen.value = true;
  }

  function closeDeleteDialog() {
    if (deleteLoading.value) {
      return;
    }
    deleteDialogOpen.value = false;
  }

  async function handleDelete() {
    const detail = overview.value;
    if (
      !detail ||
      !detailKey.value ||
      deleteLoading.value ||
      importLoading.value
    ) {
      return;
    }

    const requestKey = requestGuard.capture();
    deleteLoading.value = true;
    deleteError.value = null;

    try {
      await deleteSession(
        detail.summary.sourceApp,
        detail.summary.sourceSessionId,
        detail.summary.transcriptPath
      );

      if (!requestGuard.isCurrent(requestKey)) {
        return;
      }

      deleteDialogOpen.value = false;
      onDeleted();
    } catch (error) {
      if (requestGuard.isCurrent(requestKey)) {
        deleteError.value = extractErrorMessage(error, "删除会话失败。");
      }
    } finally {
      if (requestGuard.isCurrent(requestKey)) {
        deleteLoading.value = false;
      }
    }
  }

  return {
    targetApp,
    preview,
    previewTargetApp,
    importResult,
    previewError,
    importError,
    deleteError,
    previewLoading,
    importLoading,
    deleteLoading,
    deleteDialogOpen,
    importDialogOpen,
    previewReady,
    openImportDialog,
    closeImportDialog,
    closeImportResultDialog,
    handleImport,
    openDeleteDialog,
    closeDeleteDialog,
    handleDelete
  };
}
