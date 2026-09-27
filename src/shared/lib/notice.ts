import { ref } from "vue";
import type { AppToastNotice } from "@shared/ui/AppToast.vue";

// 页面级 toast 单例工厂：调用方在模块顶层实例化一次，
// 同一页面的所有 composable 共享同一个 notice ref。
export function createNoticeSingleton() {
  const notice = ref<AppToastNotice | null>(null);

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
