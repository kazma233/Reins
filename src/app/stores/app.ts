import { defineStore } from "pinia";

export type AppMode = "sessions" | "workspace" | "providers" | "usage" | "about";

export const useAppStore = defineStore("app", {
  state: () => ({
    appMode: "workspace" as AppMode,
  }),
  actions: {
    setAppMode(mode: AppMode) {
      this.appMode = mode;
    },
  },
});
