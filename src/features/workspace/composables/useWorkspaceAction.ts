import { extractErrorMessage } from "@shared/lib/errors";
import { useWorkspaceStore } from "../stores/workspace";
import { useWorkspaceState } from "./useWorkspaceState";

export type RunWorkspaceActionOptions<T> = {
  action: () => Promise<T>;
  // 失败文案的兜底。调用方用 onError 把它落到表单字段或面板错误槽；
  // 不提供 onError 时错误由调用方自行捕获（例如已在 action 内处理）。
  error?: string;
  onError?: (message: string) => void;
  // 写操作：action 成功后重读工作区状态。在 onSuccess 之前执行，保证
  // 界面与回调看到的是新状态。
  reload?: boolean;
  // 成功后回调（重读之后）。弹窗关闭、流程推进都放这里，失败时跳过。
  onSuccess?: (result: T) => void;
};

export function useWorkspaceAction() {
  const store = useWorkspaceStore();
  const { reloadWorkspaceState } = useWorkspaceState();

  async function runWorkspaceAction<T>(options: RunWorkspaceActionOptions<T>): Promise<void> {
    store.setRunningAction(true);

    try {
      const result = await options.action();

      if (options.reload) {
        await reloadWorkspaceState();
      }

      options.onSuccess?.(result);
    } catch (error) {
      const message = extractErrorMessage(error, options.error ?? "操作失败。");
      options.onError?.(message);
    } finally {
      store.setRunningAction(false);
    }
  }

  return { runWorkspaceAction };
}
