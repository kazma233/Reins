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
  // 界面与回调看到的是新状态；重读静默进行，不把整页切到加载态。
  reload?: boolean;
  // 成功后回调（重读之后）。弹窗关闭、流程推进都放这里，失败时跳过。
  onSuccess?: (result: T) => void;
  // 卡片级操作置 false：不占用整页忙碌态，页面上其余控件保持可用。
  // 整页忙碌会把所有按钮（含头部与其它卡片）一起置灰，点一张卡看起来
  // 像整页闪了一下。关闭时由调用方自己维护该项的进行中状态。
  pageLock?: boolean;
};

export function useWorkspaceAction() {
  const store = useWorkspaceStore();
  const { reloadWorkspaceState } = useWorkspaceState();

  async function runWorkspaceAction<T>(options: RunWorkspaceActionOptions<T>): Promise<void> {
    const pageLock = options.pageLock ?? true;
    if (pageLock) {
      store.setRunningAction(true);
    }

    try {
      const result = await options.action();

      if (options.reload) {
        await reloadWorkspaceState({ background: true });
      }

      options.onSuccess?.(result);
    } catch (error) {
      const message = extractErrorMessage(error, options.error ?? "操作失败。");
      options.onError?.(message);
    } finally {
      if (pageLock) {
        store.setRunningAction(false);
      }
    }
  }

  return { runWorkspaceAction };
}
