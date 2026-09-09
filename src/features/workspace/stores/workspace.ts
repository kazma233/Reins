import { defineStore } from "pinia";
import type {
  WorkspaceConfigDocument,
  WorkspaceInspection,
  WorkspaceTab,
} from "../types";

type WorkspaceStoreState = {
  workspaceTab: WorkspaceTab;
  configDocument: WorkspaceConfigDocument | null;
  inspection: WorkspaceInspection | null;
  loadingConfig: boolean;
  loadingInspection: boolean;
  runningAction: boolean;
};

export const useWorkspaceStore = defineStore("workspace", {
  state: (): WorkspaceStoreState => ({
    workspaceTab: "targets",
    configDocument: null,
    inspection: null,
    loadingConfig: false,
    loadingInspection: false,
    runningAction: false,
  }),
  actions: {
    setWorkspaceTab(tab: WorkspaceTab) {
      this.workspaceTab = tab;
    },
    setConfigDocument(document: WorkspaceConfigDocument | null) {
      this.configDocument = document;
    },
    setInspection(inspection: WorkspaceInspection | null) {
      this.inspection = inspection;
    },
    setLoadingConfig(loading: boolean) {
      this.loadingConfig = loading;
    },
    setLoadingInspection(loading: boolean) {
      this.loadingInspection = loading;
    },
    setRunningAction(running: boolean) {
      this.runningAction = running;
    },
  },
});
