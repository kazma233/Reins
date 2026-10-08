import { extractErrorMessage } from "@shared/lib/errors";
import type { ProviderAppId } from "../generated";
import { useProvidersStore } from "../stores/providers";
import { useProvidersState } from "./useProvidersState";

export type RunProvidersActionOptions<T> = {
  action: () => Promise<T>;
  // 错误对象本身没有可读消息时的兜底文案。
  error?: string;
  // 失败文案落到调用方的内联错误槽；不给则不产生任何界面反馈。
  onError?: (message: string) => void;
  // 成功后回调，在刷新之后执行：写操作的收尾（关弹窗、进入下一步）放这里。
  onSuccess?: (result: T) => void;
  // 写操作：action 成功后刷新状态。
  reload?: boolean;
  // 只重读单个工具卡片（隐式代表 reload），其余卡片与整页保持挂载。
  reloadApp?: ProviderAppId;
};

export function useProvidersAction() {
  const store = useProvidersStore();
  const { reloadProvidersState, reloadProviderAppState } = useProvidersState();

  async function runProvidersAction<T>(options: RunProvidersActionOptions<T>): Promise<void> {
    store.setRunningAction(true);

    try {
      const result = await options.action();

      if (options.reload || options.reloadApp) {
        if (options.reloadApp) {
          await reloadProviderAppState(options.reloadApp);
        } else {
          await reloadProvidersState();
        }
      }

      options.onSuccess?.(result);
    } catch (error) {
      const message = extractErrorMessage(error, options.error ?? "操作失败。");
      options.onError?.(message);
    } finally {
      store.setRunningAction(false);
    }
  }

  return { runProvidersAction };
}
