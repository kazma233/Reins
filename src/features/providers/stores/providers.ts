import { defineStore } from "pinia";
import type { ProviderAppId, ProviderAppState, ProvidersState } from "../generated";
import type { ProvidersTab } from "../model";

type ProvidersStoreState = {
  providersTab: ProvidersTab;
  state: ProvidersState | null;
  // 整页读取失败：列表区域原位展示错误与重试。
  error: string | null;
  // 单卡片刷新失败：key 为工具；卡片内原位展示错误与重试。
  appErrors: Partial<Record<ProviderAppId, string>>;
  loadingState: boolean;
  runningAction: boolean;
};

export const useProvidersStore = defineStore("providers", {
  state: (): ProvidersStoreState => ({
    providersTab: "apps",
    state: null,
    error: null,
    appErrors: {},
    loadingState: false,
    runningAction: false,
  }),
  actions: {
    setProvidersTab(tab: ProvidersTab) {
      this.providersTab = tab;
    },
    // 读到新状态即代表上一次读取失败已恢复。
    setState(state: ProvidersState | null) {
      this.state = state;
      this.error = null;
    },
    setError(error: string | null) {
      this.error = error;
    },
    setAppError(app: ProviderAppId, message: string | null) {
      if (message === null) {
        delete this.appErrors[app];
        return;
      }
      this.appErrors[app] = message;
    },
    // 定点替换单个工具的卡片：写操作只改一个工具的配置文件，其余卡片
    // 不需要重新挂载；重读成功也清掉这张卡片的刷新错误。
    patchAppState(appState: ProviderAppState) {
      if (!this.state) {
        return;
      }
      this.state = {
        ...this.state,
        apps: this.state.apps.map((item) => (item.app === appState.app ? appState : item)),
      };
      delete this.appErrors[appState.app];
    },
    setLoadingState(loading: boolean) {
      this.loadingState = loading;
    },
    setRunningAction(running: boolean) {
      this.runningAction = running;
    },
  },
});
