import { defineStore } from "pinia";
import { ALL_SOURCES, type SourceSelection } from "../../sessions/source-app";
import type { UsageStats } from "../types";
import { ALL_MODELS } from "../model";

type UsageStoreState = {
  stats: UsageStats | null;
  loading: boolean;
  error: string | null;
  // 1 表示今日窗口(轴与指标切到小时粒度);0 表示"全部";7/30/90 为最近 N 天。
  windowDays: number;
  // 筛选是单值选择,页面上以两个 select 呈现:"all" 表示不筛选。
  selectedSource: SourceSelection;
  selectedModel: string;
};

export const useUsageStore = defineStore("usage", {
  state: (): UsageStoreState => ({
    stats: null,
    loading: false,
    error: null,
    windowDays: 1,
    selectedSource: ALL_SOURCES,
    selectedModel: ALL_MODELS,
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
    // 模型候选按来源切分:换来源后旧模型在新来源下不成立,一并重置回全部。
    setSelectedSource(source: SourceSelection) {
      this.selectedSource = source;
      this.selectedModel = ALL_MODELS;
    },
    setSelectedModel(model: string) {
      this.selectedModel = model;
    },
  },
});
