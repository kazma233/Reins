<script setup lang="ts">
import { computed, onMounted } from "vue";
import { formatTokenCount } from "@shared/lib/format";
import AppCard from "@shared/ui/AppCard.vue";
import UsageMetricsRow from "./components/UsageMetricsRow.vue";
import UsageTrendChart from "./components/UsageTrendChart.vue";
import { useUsageState } from "./composables/useUsageState";
import { formatSourceAppName } from "../sessions/source-app";
import {
  USAGE_WINDOWS,
  buildDayAxis,
  buildHourAxis,
  computeMetrics,
  localTodayDay,
  sourceColor,
  sourceSeries,
  todaySources,
  windowSourceSummaries,
  windowSources,
  type UsageWindowDays
} from "./model";
import "./styles.css";

const {
  stats,
  loading,
  error,
  windowDays,
  hiddenSources,
  loadUsageStats,
  setWindowDays,
  toggleSource
} = useUsageState();

onMounted(loadUsageStats);

// days=1 是今日窗口:轴与序列切换为小时粒度,今日序列以后端统计时刻的
// "今天"为准,跨午夜后点刷新即可拿到新一天。
const isTodayMode = computed(() => windowDays.value === 1);

const axis = computed(() =>
  isTodayMode.value
    ? buildHourAxis()
    : buildDayAxis(
        (stats.value?.sources ?? []).flatMap((source) => source.days.map((point) => point.day)),
        windowDays.value as UsageWindowDays,
        localTodayDay()
      )
);

// 指标与占比始终按全部来源计算;来源筛选只影响曲线。
const normalizedSources = computed(() =>
  isTodayMode.value
    ? todaySources(stats.value?.sources ?? [])
    : windowSources(stats.value?.sources ?? [], axis.value)
);
const metrics = computed(() => computeMetrics(normalizedSources.value, axis.value));
const summaries = computed(() =>
  windowSourceSummaries(normalizedSources.value, axis.value)
);

const chartSeries = computed(() =>
  normalizedSources.value
    .filter((source) => !hiddenSources.value.includes(source.sourceApp))
    .map((source) => ({
      sourceApp: source.sourceApp,
      label: formatSourceAppName(source.sourceApp),
      color: sourceColor(source.sourceApp),
      values: sourceSeries(source.points, axis.value)
    }))
);
</script>

<template>
  <div class="usage-workspace">
    <header class="usage-header">
      <div class="usage-header__main">
        <h2 class="usage-header__title">用量统计</h2>
        <p class="usage-header__meta">按会话内消耗事件的实际时间归日统计,不估算金额</p>
      </div>
      <div class="usage-header__actions">
        <div class="usage-window-switcher" role="group" aria-label="时间窗口">
          <button
            v-for="option in USAGE_WINDOWS"
            :key="option.days"
            type="button"
            :class="`usage-window-switcher__button${option.days === windowDays ? ' is-active' : ''}`"
            @click="setWindowDays(option.days)"
          >
            {{ option.label }}
          </button>
        </div>
        <button
          type="button"
          class="secondary-button usage-refresh-button"
          :disabled="loading"
          @click="loadUsageStats"
        >
          {{ loading ? "扫描中…" : "刷新" }}
        </button>
      </div>
    </header>

    <p v-if="error" class="usage-error">
      {{ error }}
      <button type="button" class="usage-error__retry" @click="loadUsageStats">重试</button>
    </p>

    <div v-if="loading && !stats" class="usage-loading">
      正在扫描各 agent 的会话用量,首次扫描大体积转录需要一点时间…
      <div class="usage-loading__bar" />
    </div>

    <template v-else-if="stats">
      <UsageMetricsRow :metrics="metrics" :mode="isTodayMode ? 'today' : 'window'" />

      <AppCard class="usage-chart-card">
        <template #header>{{ isTodayMode ? "今日每小时消耗" : "每日总消耗" }}</template>
        <template #headerMeta
          >{{ isTodayMode ? "按消耗事件的实际小时归桶" : "跨天会话按消息时间拆分到天;点击来源切换曲线显示" }}</template
        >
        <div v-if="axis.length === 0" class="usage-empty">暂无用量数据</div>
        <div v-else-if="chartSeries.length === 0" class="usage-empty">
          已隐藏全部来源,点击下方来源恢复曲线
        </div>
        <UsageTrendChart
          v-else
          :axis="axis"
          :series="chartSeries"
          :x-label-mode="isTodayMode ? 'hour' : 'day'"
        />

        <div v-if="summaries.length > 0" class="usage-source-chips">
          <button
            v-for="item in summaries"
            :key="item.sourceApp"
            type="button"
            :class="`usage-source-chip${
              hiddenSources.includes(item.sourceApp) ? ' is-hidden' : ''
            }`"
            :title="`${formatSourceAppName(item.sourceApp)} · 窗口内消耗 ${formatTokenCount(
              item.tokens
            )} · 有用量会话 ${item.sessionCount}`"
            @click="toggleSource(item.sourceApp)"
          >
            <span class="usage-source-chip__dot" :style="{ background: item.color }" />
            <span class="usage-source-chip__name">{{ formatSourceAppName(item.sourceApp) }}</span>
            <span class="usage-source-chip__share">{{ Math.round(item.share * 100) }}%</span>
            <span class="usage-source-chip__tokens">{{ formatTokenCount(item.tokens) }}</span>
          </button>
        </div>
      </AppCard>
    </template>

    <div v-else class="usage-empty">暂无用量数据</div>
  </div>
</template>
