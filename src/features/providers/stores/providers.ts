import { defineStore } from "pinia";
import type { ProviderAppState, ProvidersState } from "../generated";
import type { ProvidersTab } from "../model";

type ProvidersStoreState = {
  providersTab: ProvidersTab;
  state: ProvidersState | null;
  loadingState: boolean;
  runningAction: boolean;
};

export const useProvidersStore = defineStore("providers", {
  state: (): ProvidersStoreState => ({
    providersTab: "apps",
    state: null,
    loadingState: false,
    runningAction: false,
  }),
  actions: {
    setProvidersTab(tab: ProvidersTab) {
      this.providersTab = tab;
    },
    setState(state: ProvidersState | null) {
      this.state = state;
    },
    // 定点替换单个工具的卡片：写操作只改一个工具的配置文件，其余卡片
    // 不需要重新挂载。
    patchAppState(appState: ProviderAppState) {
      if (!this.state) {
        return;
      }
      this.state = {
        ...this.state,
        apps: this.state.apps.map((item) => (item.app === appState.app ? appState : item)),
      };
    },
    setLoadingState(loading: boolean) {
      this.loadingState = loading;
    },
    setRunningAction(running: boolean) {
      this.runningAction = running;
    },
  },
});
