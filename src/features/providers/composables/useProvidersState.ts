import { storeToRefs } from "pinia";
import { getProviderAppState, getProvidersState } from "../api";
import type { ProviderAppId } from "../generated";
import { useProvidersStore } from "../stores/providers";
import { extractErrorMessage } from "@shared/lib/errors";

export function useProvidersState() {
  const store = useProvidersStore();
  const { state, error, appErrors, loadingState, runningAction } = storeToRefs(store);

  // 单次加载由 ProvidersWorkspace.vue 的 onMounted 触发；其余组件通过
  // action composable 在操作成功后刷新。读取失败写进 store：错误态常驻
  // 在列表原位等待重试，不再弹一次性提示。
  async function reloadProvidersState() {
    store.setLoadingState(true);

    try {
      const nextState = await getProvidersState();
      store.setState(nextState);
    } catch (err) {
      store.setError(extractErrorMessage(err, "读取模型配置失败。"));
    } finally {
      store.setLoadingState(false);
    }
  }

  // 重试前先清掉旧错误，避免错误条与「读取中…」同时出现。
  function retryProvidersState() {
    store.setError(null);
    return reloadProvidersState();
  }

  // 写操作后的定点刷新：只重读发生变化的工具，不进入整页 loading，
  // 其余卡片保持挂载；失败只落到这张卡片的错误提示上。
  async function reloadProviderAppState(app: ProviderAppId) {
    try {
      store.patchAppState(await getProviderAppState(app));
    } catch (err) {
      store.setAppError(app, extractErrorMessage(err, "读取模型配置失败。"));
    }
  }

  return {
    state,
    error,
    appErrors,
    loadingState,
    runningAction,
    reloadProvidersState,
    retryProvidersState,
    reloadProviderAppState,
  };
}
