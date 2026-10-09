import { defineStore } from "pinia";
import type { SourceApp, UsageStats } from "../types";

type UsageStoreState = {
  stats: UsageStats | null;
  loading: boolean;
  error: string | null;
  // 1 表示今日窗口(轴与指标切到小时粒度);0 表示"全部";7/30/90 为最近 N 天。
  windowDays: number;
  // 曲线中隐藏的来源:筛选只影响曲线,指标卡与占比始终按全部来源计算。
  hiddenSources: SourceApp[];
};

export const useUsageStore = defineStore("usage", {
  state: (): UsageStoreState => ({
    stats: null,
    loading: false,
    error: null,
    windowDays: 1,
    hiddenSources: [],
  }),
  actions: {
    setLoading(loading: boolean) {
      this.loading = loading;
    },
    setStats(stats: UsageStats | null) {
      this.stats = stats;
    },
    setError(error: string | null) {
      this.error = error;
    },
    setWindowDays(windowDays: number) {
      this.windowDays = windowDays;
    },
    toggleSource(sourceApp: SourceApp) {
      this.hiddenSources = this.hiddenSources.includes(sourceApp)
        ? this.hiddenSources.filter((item: SourceApp) => item !== sourceApp)
        : [...this.hiddenSources, sourceApp];
    },
  },
});
