import { storeToRefs } from "pinia";
import { getWorkspaceState } from "../api";
import { useWorkspaceStore } from "../stores/workspace";
import { extractErrorMessage } from "@shared/lib/errors";
import type { WorkspaceState } from "../types";

export function useWorkspaceState() {
  const store = useWorkspaceStore();
  const {
    configDocument,
    inspection,
    loadingConfig,
    loadingInspection,
    runningAction,
    loadError,
  } = storeToRefs(store);

  // NOTE: bootstrap is intentionally NOT wired to onMounted here. Multiple
  // composables call useWorkspaceState() during Workspace.vue setup, so an
  // onMounted hook here would fire N times and trigger N duplicate
  // getWorkspaceState() requests. Workspace.vue owns the single initial load.
  function applyWorkspaceState(state: WorkspaceState) {
    store.setConfigDocument(state.document);
    store.setInspection(state.inspection);
  }

  // 读取失败是持久状态：错误条常驻到下次成功读取，重试入口在面板里。
  async function reloadWorkspaceState() {
    store.setLoadingConfig(true);
    store.setLoadingInspection(true);

    try {
      applyWorkspaceState(await getWorkspaceState());
      store.setLoadError(null);
    } catch (error) {
      store.setLoadError(extractErrorMessage(error, "读取配置失败。"));
    } finally {
      store.setLoadingConfig(false);
      store.setLoadingInspection(false);
    }
  }

  async function retryWorkspaceState() {
    if (store.loadingConfig || store.loadingInspection) {
      return;
    }
    await reloadWorkspaceState();
  }

  function clearWorkspaceError() {
    store.setLoadError(null);
  }

  return {
    configDocument,
    inspection,
    loadingConfig,
    loadingInspection,
    runningAction,
    loadError,
    applyWorkspaceState,
    reloadWorkspaceState,
    retryWorkspaceState,
    clearWorkspaceError,
  };
}
