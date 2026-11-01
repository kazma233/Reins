import { ref } from "vue";
import { getWorkspaceState } from "../api";
import { extractErrorMessage } from "@shared/lib/errors";
import type {
  WorkspaceConfigDocument,
  WorkspaceInspection,
  WorkspaceState
} from "../types";

// 配置与分发页的数据状态工厂:由 Workspace.vue 调用一次,状态随根组件挂载
// 重建;面板经 props 读取,写操作经 useWorkspaceAction 刷新同一份状态。
export function useWorkspaceState() {
  const configDocument = ref<WorkspaceConfigDocument | null>(null);
  const inspection = ref<WorkspaceInspection | null>(null);
  const loadingConfig = ref(false);
  const loadingInspection = ref(false);
  const runningAction = ref(false);
  // 整页读取失败：错误态由面板就地渲染并提供重试。
  const loadError = ref<string | null>(null);
  // 项目 agent 选择器的部分失败目标：错误必须留在触发它的 mcp 卡片上，
  // 因为弹窗关闭后失败目标已不在任何弹窗里。
  const projectPickerWarnings = ref<Record<string, string>>({});

  function setConfigDocument(document: WorkspaceConfigDocument | null) {
    configDocument.value = document;
  }

  function setInspection(nextInspection: WorkspaceInspection | null) {
    inspection.value = nextInspection;
  }

  function setProjectPickerWarning(serverName: string, message: string | null) {
    const next = { ...projectPickerWarnings.value };
    if (message === null) {
      delete next[serverName];
    } else {
      next[serverName] = message;
    }
    projectPickerWarnings.value = next;
  }

  // NOTE: bootstrap is intentionally NOT wired to onMounted here. Workspace.vue
  // owns the single initial load; composables receive this context instead of
  // creating their own.
  function applyWorkspaceState(state: WorkspaceState) {
    setConfigDocument(state.document);
    setInspection(state.inspection);
  }

  // 读取失败是持久状态：错误条常驻到下次成功读取，重试入口在面板里。
  // background 用于写操作后的重读：界面上已有内容可看，进入整页加载态只会让
  // 面板闪一下加载提示（列表随之位移），所以只静默替换数据。
  async function reloadWorkspaceState(options?: { background?: boolean }) {
    const showLoading = !options?.background;
    if (showLoading) {
      loadingConfig.value = true;
      loadingInspection.value = true;
    }

    try {
      applyWorkspaceState(await getWorkspaceState());
      loadError.value = null;
    } catch (error) {
      loadError.value = extractErrorMessage(error, "读取配置失败。");
    } finally {
      if (showLoading) {
        loadingConfig.value = false;
        loadingInspection.value = false;
      }
    }
  }

  async function retryWorkspaceState() {
    if (loadingConfig.value || loadingInspection.value) {
      return;
    }
    await reloadWorkspaceState();
  }

  function clearWorkspaceError() {
    loadError.value = null;
  }

  return {
    configDocument,
    inspection,
    loadingConfig,
    loadingInspection,
    runningAction,
    loadError,
    projectPickerWarnings,
    setInspection,
    setProjectPickerWarning,
    applyWorkspaceState,
    reloadWorkspaceState,
    retryWorkspaceState,
    clearWorkspaceError,
  };
}

export type WorkspaceStateContext = ReturnType<typeof useWorkspaceState>;
