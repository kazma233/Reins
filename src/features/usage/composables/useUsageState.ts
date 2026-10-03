import { storeToRefs } from "pinia";
import { getUsageStats } from "../api";
import { useUsageStore } from "../stores/usage";
import { extractErrorMessage } from "@shared/lib/errors";

// 用量页只有一份全量日序列:窗口切换、来源筛选都是纯前端裁剪,
// 只有首次加载与手动刷新会发 invoke。
export function useUsageState() {
  const store = useUsageStore();
  const { stats, loading, error, windowDays, hiddenSources } = storeToRefs(store);

  async function loadUsageStats() {
    store.setLoading(true);
    store.setError(null);
    try {
      store.setStats(await getUsageStats());
    } catch (caught) {
      store.setError(extractErrorMessage(caught, "用量数据加载失败"));
    } finally {
      store.setLoading(false);
    }
  }

  return {
    stats,
    loading,
    error,
    windowDays,
    hiddenSources,
    loadUsageStats,
    setWindowDays: store.setWindowDays,
    toggleSource: store.toggleSource,
  };
}
