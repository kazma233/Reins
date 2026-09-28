import { storeToRefs } from "pinia";
import { getProviderAppState, getProvidersState } from "../api";
import type { ProviderAppId } from "../generated";
import { useProvidersStore } from "../stores/providers";
import { extractErrorMessage } from "@shared/lib/errors";
import { useProvidersNotice } from "./useProvidersNotice";

export function useProvidersState() {
  const store = useProvidersStore();
  const { state, loadingState, runningAction } = storeToRefs(store);
  const { showNotice, clearNotice } = useProvidersNotice();

  // 单次加载由 ProvidersWorkspace.vue 的 onMounted 触发；其余组件通过
  // action composable 在操作成功后刷新。
  async function reloadProvidersState(options?: { preserveNotice?: boolean }) {
    store.setLoadingState(true);

    if (!options?.preserveNotice) {
      clearNotice();
    }

    try {
      const nextState = await getProvidersState();
      store.setState(nextState);
    } catch (error) {
      showNotice(extractErrorMessage(error, "读取模型配置失败。"), "error");
    } finally {
      store.setLoadingState(false);
    }
  }

  // 写操作后的定点刷新：只重读发生变化的工具，不进入整页 loading，
  // 其余卡片保持挂载。
  async function reloadProviderAppState(app: ProviderAppId) {
    try {
      store.patchAppState(await getProviderAppState(app));
    } catch (error) {
      showNotice(extractErrorMessage(error, "读取模型配置失败。"), "error");
    }
  }

  return {
    state,
    loadingState,
    runningAction,
    reloadProvidersState,
    reloadProviderAppState,
  };
}
