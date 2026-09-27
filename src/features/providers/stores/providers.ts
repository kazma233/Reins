import { defineStore } from "pinia";
import type { ProvidersState } from "../generated";
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
    setLoadingState(loading: boolean) {
      this.loadingState = loading;
    },
    setRunningAction(running: boolean) {
      this.runningAction = running;
    },
  },
});
