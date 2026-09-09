import { extractErrorMessage } from "@shared/lib/errors";
import type { AppToastNotice } from "@shared/ui/AppToast.vue";
import { useWorkspaceStore } from "../stores/workspace";
import { useWorkspaceNotice } from "./useWorkspaceNotice";
import { useWorkspaceState } from "./useWorkspaceState";

export type WorkspaceActionSuccessNotice = {
  message: string;
  // Defaults to "success". Handlers whose outcome depends on the api result
  // (noop / partial failure) report a different tone from the same code path.
  tone?: AppToastNotice["tone"];
};

export type RunWorkspaceActionOptions<T> = {
  action: () => Promise<T>;
  // Declaring `success` marks the action as a workspace mutation: state is
  // reloaded (notice preserved) between the api call and the toast, so the
  // toast reflects fresh store data and is never wiped by the reload. Omit it
  // for side-effect tasks (pickers, previews) — no reload, no toast.
  success?: string | ((result: T) => WorkspaceActionSuccessNotice);
  // Fallback text handed to extractErrorMessage when the error carries no
  // readable message of its own.
  error?: string;
  // Success-only epilogue (usually closing a dialog). For mutations it runs
  // after the reload so panels never render stale store data behind the
  // dialog; it is skipped entirely when the action fails.
  after?: (result: T) => void;
};

export function useWorkspaceAction() {
  const store = useWorkspaceStore();
  const { showNotice, clearNotice } = useWorkspaceNotice();
  const { reloadWorkspaceState } = useWorkspaceState();

  async function runWorkspaceAction<T>(options: RunWorkspaceActionOptions<T>): Promise<void> {
    store.setRunningAction(true);
    clearNotice();

    try {
      const result = await options.action();

      if (options.success !== undefined) {
        // preserveNotice keeps the reload's own clearNotice() from racing the
        // success toast; the toast is shown last so it always survives.
        await reloadWorkspaceState({ preserveNotice: true });
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

  return { runWorkspaceAction };
}
