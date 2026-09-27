import { extractErrorMessage } from "@shared/lib/errors";
import type { AppToastNotice } from "@shared/ui/AppToast.vue";
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
};

export function useProvidersAction() {
  const store = useProvidersStore();
  const { showNotice, clearNotice } = useProvidersNotice();
  const { reloadProvidersState } = useProvidersState();

  async function runProvidersAction<T>(options: RunProvidersActionOptions<T>): Promise<void> {
    store.setRunningAction(true);
    clearNotice();

    try {
      const result = await options.action();

      if (options.success !== undefined) {
        await reloadProvidersState({ preserveNotice: true });
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
