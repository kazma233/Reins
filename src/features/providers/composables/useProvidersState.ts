import { ref } from "vue";
import { getProviderAppState, getProvidersState } from "../api";
import type { ProviderAppId, ProviderAppState, ProvidersState } from "../generated";
import { extractErrorMessage } from "@shared/lib/errors";

// 模型配置页的数据状态工厂:由 ProvidersWorkspace 调用一次,状态随根组件
// 挂载重建;面板经 props 读取,写操作经 useProvidersAction 刷新同一份状态。
export function useProvidersState() {
  const state = ref<ProvidersState | null>(null);
  // 整页读取失败：列表区域原位展示错误与重试。
  const error = ref<string | null>(null);
  // 单卡片刷新失败：key 为工具；卡片内原位展示错误与重试。
  const appErrors = ref<Partial<Record<ProviderAppId, string>>>({});
  const loadingState = ref(false);
  const runningAction = ref(false);

  // 读到新状态即代表上一次读取失败已恢复。
  function setState(nextState: ProvidersState | null) {
    state.value = nextState;
    error.value = null;
  }

  function setAppError(app: ProviderAppId, message: string | null) {
    if (message === null) {
      delete appErrors.value[app];
      return;
    }
    appErrors.value[app] = message;
  }

  // 定点替换单个工具的卡片：写操作只改一个工具的配置文件，其余卡片
  // 不需要重新挂载；重读成功也清掉这张卡片的刷新错误。
  function patchAppState(appState: ProviderAppState) {
    if (!state.value) {
      return;
    }
    state.value = {
      ...state.value,
      apps: state.value.apps.map((item) => (item.app === appState.app ? appState : item)),
    };
    delete appErrors.value[appState.app];
  }

  // 单次加载由 ProvidersWorkspace.vue 的 onMounted 触发；其余组件通过
  // action composable 在操作成功后刷新。读取失败写进状态：错误态常驻
  // 在列表原位等待重试，不再弹一次性提示。
  async function reloadProvidersState() {
    loadingState.value = true;

    try {
      setState(await getProvidersState());
    } catch (err) {
      error.value = extractErrorMessage(err, "读取模型配置失败。");
    } finally {
      loadingState.value = false;
    }
  }

  // 重试前先清掉旧错误，避免错误条与「读取中…」同时出现。
  function retryProvidersState() {
    error.value = null;
    return reloadProvidersState();
  }

  // 写操作后的定点刷新：只重读发生变化的工具，不进入整页 loading，
  // 其余卡片保持挂载；失败只落到这张卡片的错误提示上。
  async function reloadProviderAppState(app: ProviderAppId) {
    try {
      patchAppState(await getProviderAppState(app));
    } catch (err) {
      setAppError(app, extractErrorMessage(err, "读取模型配置失败。"));
    }
  }

  return {
    state,
    error,
    appErrors,
    loadingState,
    runningAction,
    setState,
    setAppError,
    reloadProvidersState,
    retryProvidersState,
    reloadProviderAppState,
  };
}

export type ProvidersStateContext = ReturnType<typeof useProvidersState>;
