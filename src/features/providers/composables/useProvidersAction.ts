import { extractErrorMessage } from "@shared/lib/errors";
import type { AppToastNotice } from "@shared/ui/AppToast.vue";
import type { ProviderAppId } from "../generated";
import { useProvidersStore } from "../stores/providers";
import { useProvidersNotice } from "./useProvidersNotice";
import { useProvidersState } from "./useProvidersState";

export type ProvidersActionSuccessNotice = {
  message: string;
  tone?: AppToastNotice["tone"];
};

export type RunProvidersActionOptions<T> = {
  action: () => Promise<T>;
  // 带 success 的操作是写操作：先刷新状态再弹 toast，保证界面不展示
  // 过期数据。省略 success 表示副作用任务（预览、拉取列表等）。
  success?: string | ((result: T) => ProvidersActionSuccessNotice);
  error?: string;
  after?: (result: T) => void;
  // 写操作只影响单个工具时指定它：只重读该工具卡片，其余卡片与整页
  // 保持挂载，不做全量重读。
  refreshApp?: ProviderAppId;
};

export function useProvidersAction() {
  const store = useProvidersStore();
  const { showNotice, clearNotice } = useProvidersNotice();
  const { reloadProvidersState, reloadProviderAppState } = useProvidersState();

  async function runProvidersAction<T>(options: RunProvidersActionOptions<T>): Promise<void> {
    store.setRunningAction(true);
    clearNotice();

    try {
      const result = await options.action();

      if (options.success !== undefined) {
        if (options.refreshApp) {
          await reloadProviderAppState(options.refreshApp);
        } else {
          await reloadProvidersState({ preserveNotice: true });
        }
      }

      options.after?.(result);

      if (options.success !== undefined) {
        const notice =
          typeof options.success === "function"
            ? options.success(result)
            : { message: options.success };
        showNotice(notice.message, notice.tone ?? "success");
      }
    } catch (error) {
      showNotice(extractErrorMessage(error, options.error ?? "操作失败。"), "error");
    } finally {
      store.setRunningAction(false);
    }
  }

  return { runProvidersAction };
}
