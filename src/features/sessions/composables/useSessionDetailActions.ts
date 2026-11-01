import { ref, watch, type Ref } from "vue";
import { deleteSession } from "../api";
import type { DeletePlan, SessionOverview } from "../types";
import { extractErrorMessage } from "@shared/lib/errors";
import { createKeyGuard } from "@shared/lib/request-guard";

export type SessionDetailActionsState = {
  deleteError: Ref<string | null>;
  deleteLoading: Ref<boolean>;
  deleteDialogOpen: Ref<boolean>;
};

export type SessionDetailActions = SessionDetailActionsState & {
  openDeleteDialog: () => void;
  closeDeleteDialog: () => void;
  handleDelete: () => Promise<void>;
};

// Delete flow for the session detail panel. All dialog state and the async
// action live here; the panel only renders the dialog and header button.
// The guard key is the detail key: a response is only applied when the
// displayed session is unchanged since the request was issued.
export function useSessionDetailActions(
  overview: Ref<SessionOverview | null>,
  detailKey: Ref<string | null>,
  deletePlan: Ref<DeletePlan | null>,
  onDeleted: () => void
): SessionDetailActions {
  const deleteError = ref<string | null>(null);
  const deleteLoading = ref(false);
  const deleteDialogOpen = ref(false);

  const requestGuard = createKeyGuard(() => detailKey.value ?? "");

  function resetState() {
    deleteError.value = null;
    deleteLoading.value = false;
    deleteDialogOpen.value = false;
  }

  // Reset dialog/delete state when the displayed overview changes.
  watch(overview, () => {
    resetState();
  });

  // 删除是否支持由后端 plan 判定；无 plan（尚未取到或获取失败）时隐藏入口。
  function deleteSupported(): boolean {
    return deletePlan.value?.supported === true;
  }

  function openDeleteDialog() {
    const detail = overview.value;
    if (!detail || !deleteSupported() || deleteLoading.value) {
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
      !deleteSupported() ||
      deleteLoading.value
    ) {
      return;
    }

    const requestKey = requestGuard.capture();
    deleteLoading.value = true;
    deleteError.value = null;

    try {
      await deleteSession(detail.summary.sourceApp, detail.summary.sourceSessionId);

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
    deleteError,
    deleteLoading,
    deleteDialogOpen,
    openDeleteDialog,
    closeDeleteDialog,
    handleDelete
  };
}
