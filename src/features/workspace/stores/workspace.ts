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
  // 整页读取失败：错误态由面板就地渲染并提供重试。
  loadError: string | null;
  // 项目 agent 选择器的部分失败目标：错误必须留在触发它的 mcp 卡片上，
  // 因为弹窗关闭后失败目标已不在任何弹窗里。
  projectPickerWarnings: Record<string, string>;
  // 面板直发操作的失败与提示（启停、拾取路径等）：没有弹窗可承载，
  // 按操作 key 常驻在面板顶部，同 key 的下一次操作覆盖它。
  actionResults: Record<string, { message: string; failed: boolean }>;
};

export const useWorkspaceStore = defineStore("workspace", {
  state: (): WorkspaceStoreState => ({
    workspaceTab: "targets",
    configDocument: null,
    inspection: null,
    loadingConfig: false,
    loadingInspection: false,
    runningAction: false,
    loadError: null,
    projectPickerWarnings: {},
    actionResults: {},
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
    setLoadError(error: string | null) {
      this.loadError = error;
    },
    setProjectPickerWarning(serverName: string, message: string | null) {
      const next = { ...this.projectPickerWarnings };
      if (message === null) {
        delete next[serverName];
      } else {
        next[serverName] = message;
      }
      this.projectPickerWarnings = next;
    },
    setActionResult(key: string, result: { message: string; failed: boolean } | null) {
      const next = { ...this.actionResults };
      if (result === null) {
        delete next[key];
      } else {
        next[key] = result;
      }
      this.actionResults = next;
    },
  },
});
