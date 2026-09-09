import { storeToRefs } from "pinia";
import { getWorkspaceState } from "../api";
import { useWorkspaceStore } from "../stores/workspace";
import { extractErrorMessage } from "@shared/lib/errors";
import type { WorkspaceState } from "../types";
import { useWorkspaceNotice } from "./useWorkspaceNotice";

export function useWorkspaceState() {
  const store = useWorkspaceStore();
  const { configDocument, inspection, loadingConfig, loadingInspection, runningAction } =
    storeToRefs(store);
  const { showNotice, clearNotice } = useWorkspaceNotice();

  // NOTE: bootstrap is intentionally NOT wired to onMounted here. Multiple
  // composables call useWorkspaceState() during Workspace.vue setup, so an
  // onMounted hook here would fire N times and trigger N duplicate
  // getWorkspaceState() requests. Workspace.vue owns the single initial load.
  function applyWorkspaceState(state: WorkspaceState) {
    store.setConfigDocument(state.document);
    store.setInspection(state.inspection);
  }

  async function reloadWorkspaceState(options?: { preserveNotice?: boolean }) {
    store.setLoadingConfig(true);
    store.setLoadingInspection(true);

    if (!options?.preserveNotice) {
      clearNotice();
    }

    try {
      const state = await getWorkspaceState();
      applyWorkspaceState(state);
    } catch (error) {
      showNotice(extractErrorMessage(error, "读取配置失败。"), "error");
    } finally {
      store.setLoadingConfig(false);
      store.setLoadingInspection(false);
    }
  }

  return {
    configDocument,
    inspection,
    loadingConfig,
    loadingInspection,
    runningAction,
    applyWorkspaceState,
    reloadWorkspaceState,
  };
}
