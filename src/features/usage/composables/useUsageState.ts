import { ref } from "vue";
import { ALL_SOURCES, type SourceSelection } from "../../sessions/source-app";
import { getUsageStats } from "../api";
import { extractErrorMessage } from "@shared/lib/errors";
import type { UsageStats } from "../types";
import { ALL_MODELS } from "../model";

// 筛选偏好跟着应用走(模块级):离开用量页再回来,窗口与筛选保持上次选择。
const windowDays = ref(1);
const selectedSource = ref<SourceSelection>(ALL_SOURCES);
const selectedModel = ref(ALL_MODELS);

// 用量页只有一份全量日序列:窗口切换、来源与模型筛选都是纯前端裁剪,
// 只有首次加载与手动刷新会发 invoke。stats 随页面挂载重拉。
export function useUsageState() {
  const stats = ref<UsageStats | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function loadUsageStats() {
    loading.value = true;
    error.value = null;
    try {
      stats.value = await getUsageStats();
    } catch (caught) {
      error.value = extractErrorMessage(caught, "用量数据加载失败");
    } finally {
      loading.value = false;
    }
  }

  function setWindowDays(days: number) {
    windowDays.value = days;
  }

  // 模型候选按来源切分:换来源后旧模型在新来源下不成立,一并重置回全部。
  function setSelectedSource(source: SourceSelection) {
    selectedSource.value = source;
    selectedModel.value = ALL_MODELS;
  }

  function setSelectedModel(model: string) {
    selectedModel.value = model;
  }

  return {
    stats,
    loading,
    error,
    windowDays,
    selectedSource,
    selectedModel,
    loadUsageStats,
    setWindowDays,
    setSelectedSource,
    setSelectedModel
  };
}
