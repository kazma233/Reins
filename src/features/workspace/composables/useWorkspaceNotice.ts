import { ref } from "vue";
import type { AppToastNotice } from "@shared/ui/AppToast.vue";

// Module-level singleton: every composable that calls `useWorkspaceNotice`
// shares the same notice ref, so Workspace.vue can render a single AppToast.
const notice = ref<AppToastNotice | null>(null);

export function useWorkspaceNotice() {
  function showNotice(message: string, tone: AppToastNotice["tone"]) {
    notice.value = {
      id: Date.now(),
      message,
      tone,
    };
  }

  function clearNotice() {
    notice.value = null;
  }

  return {
    notice,
    showNotice,
    clearNotice,
  };
}
