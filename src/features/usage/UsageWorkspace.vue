<script setup lang="ts">
import { computed, onMounted, watch } from "vue";
import { useAppStore } from "../../app/stores/app";
import { useLoadElapsed } from "@shared/lib/use-load-elapsed";
import AppCard from "@shared/ui/AppCard.vue";
import AppSelect, { type AppSelectOption } from "@shared/ui/AppSelect.vue";
import BackLink from "@shared/ui/BackLink.vue";
import UsageMetricsRow from "./components/UsageMetricsRow.vue";
import UsageTrendChart from "./components/UsageTrendChart.vue";
import { useUsageState } from "./composables/useUsageState";
import {
  ALL_SOURCES,
  formatSourceAppName,
  type SourceSelection
} from "../sessions/source-app";
import {
  ALL_MODELS,
  USAGE_WINDOWS,
  buildDayAxis,
  buildHourAxis,
  computeMetrics,
  localTodayDay,
  modelFilteredSources,
  sourceColor,
  sourceSeries,
  todaySources,
  windowModelSummaries,
  windowSourceSummaries,
  windowSources,
  type NormalizedUsageSource,
  type UsageWindowDays
} from "./model";
import type { UsageSourceStats } from "./types";
import { totalConsumedTokens } from "../sessions/token-usage";
import "./styles.css";

const appStore = useAppStore();

const {
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
} = useUsageState();

onMounted(loadUsageStats);

// 载入中状态的实时耗时:首扫大体积转录时让用户看到等了多久。
const loadElapsed = useLoadElapsed(loading);

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

// 后端给的是全周期序列:窗口模式下裁剪到轴内,今日模式取今日小时桶。
function windowNormalized(sources: UsageSourceStats[]): NormalizedUsageSource[] {
  return isTodayMode.value ? todaySources(sources) : windowSources(sources, axis.value);
}

const allSources = computed(() => stats.value?.sources ?? []);

// 来源筛选先裁剪来源;模型候选随之变化(选中某 agent 后只看它的模型)。
const agentSources = computed(() =>
  allSources.value.filter(
    (source) =>
      selectedSource.value === ALL_SOURCES || source.sourceApp === selectedSource.value
  )
);

// 模型未筛选时用来源总量;选中模型后落到模型切片口径(切片只覆盖能取到
// 模型信息的消耗)。
const normalizedSources = computed(() =>
  selectedModel.value === ALL_MODELS
    ? windowNormalized(agentSources.value)
    : modelFilteredSources(agentSources.value, axis.value, selectedModel.value, isTodayMode.value)
);
const metrics = computed(() => computeMetrics(normalizedSources.value, axis.value));

// 两个下拉只列窗口内在用的项,按消耗降序并带窗口占比;来源占比是各来源的
// 窗口总量口径,不随模型筛选变化,模型占比分母是模型切片总量。
const sourceOptions = computed<AppSelectOption[]>(() => [
  { value: ALL_SOURCES, label: "全部" },
  ...windowSourceSummaries(windowNormalized(allSources.value), axis.value).map((item) => ({
    value: item.sourceApp,
    label: `${formatSourceAppName(item.sourceApp)} · ${Math.round(item.share * 100)}%`
  }))
]);

const modelOptions = computed<AppSelectOption[]>(() => [
  { value: ALL_MODELS, label: "全部" },
  ...windowModelSummaries(agentSources.value, axis.value, isTodayMode.value).map((item) => ({
    value: item.model,
    label: `${item.model} · ${Math.round(item.share * 100)}%`
  }))
]);

// 候选项随窗口与来源变化:已选项不再可用时回到"全部",避免控件停在列表外的值。
watch(sourceOptions, (options) => {
  if (!options.some((option) => option.value === selectedSource.value)) {
    setSelectedSource(ALL_SOURCES);
  }
});

watch(modelOptions, (options) => {
  if (!options.some((option) => option.value === selectedModel.value)) {
    setSelectedModel(ALL_MODELS);
  }
});

// AppSelect 的 model 是字符串,这里桥接到类型化的选择值。
const sourceSelectValue = computed<string>({
  get: () => selectedSource.value,
  set: (next) => setSelectedSource(next as SourceSelection)
});

const modelSelectValue = computed<string>({
  get: () => selectedModel.value,
  set: (next) => setSelectedModel(next)
});

// 窗口内是否有任何来源产生过消耗:决定空态文案与曲线是否绘制;不随筛选
// 变化,筛选后无数据走"当前筛选"空态。
const hasWindowUsage = computed(() => {
  const sources = isTodayMode.value
    ? todaySources(allSources.value)
    : windowSources(allSources.value, axis.value);
  return sources.some((source) =>
    source.points.some((point) => totalConsumedTokens(point.usage) > 0)
  );
});

const chartSeries = computed(() =>
  normalizedSources.value
    // 窗口内无消耗的来源不画曲线:全 0 线没有信息量,tooltip 还会冒出 0 行
    .filter((source) => source.points.some((point) => totalConsumedTokens(point.usage) > 0))
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
      <div class="usage-header__inner">
        <div class="usage-header__main">
          <BackLink @back="appStore.setAppMode('sessions')" />
          <h2 class="usage-header__title">用量统计</h2>
          <p class="usage-header__meta">按会话内消耗事件的实际时间归日统计</p>
        </div>
        <div class="usage-header__actions">
          <AppSelect
            v-model="sourceSelectValue"
            aria-label="选择 Agent 工具"
            class-name="usage-filter-select"
            label="Agent"
            :options="sourceOptions"
          />
          <AppSelect
            v-model="modelSelectValue"
            aria-label="选择模型"
            class-name="usage-filter-select"
            label="模型"
            :options="modelOptions"
          />
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
          <!-- 加载中只置灰,不改文案:文案一变按钮宽度会跳 -->
          <button
            type="button"
            class="secondary-button usage-refresh-button"
            :disabled="loading"
            @click="loadUsageStats"
          >
            刷新
          </button>
        </div>
      </div>
    </header>

    <p v-if="error" class="usage-error">
      {{ error }}
      <button type="button" class="usage-error__retry" @click="loadUsageStats">重试</button>
    </p>

    <div v-if="loading && !stats" class="usage-loading">
      正在扫描各 agent 的会话用量,首次扫描大体积转录需要一点时间…
      <span class="usage-loading__elapsed">已用时 {{ loadElapsed.text }}</span>
      <div class="usage-loading__bar" />
    </div>

    <template v-else-if="stats">
      <UsageMetricsRow :metrics="metrics" :mode="isTodayMode ? 'today' : 'window'" />

      <AppCard class="usage-chart-card">
        <template #header>{{ isTodayMode ? "今日每小时消耗" : "每日总消耗" }}</template>
        <template #headerMeta>{{
          isTodayMode ? "按消耗事件的实际小时归桶" : "跨天会话按消息时间拆分到天"
        }}</template>
        <div v-if="!hasWindowUsage" class="usage-empty">暂无用量数据</div>
        <div v-else-if="chartSeries.length === 0" class="usage-empty">
          当前筛选条件下没有消耗数据
        </div>
        <UsageTrendChart
          v-else
          :axis="axis"
          :series="chartSeries"
          :x-label-mode="isTodayMode ? 'hour' : 'day'"
        />
      </AppCard>
    </template>

    <div v-else class="usage-empty">暂无用量数据</div>
  </div>
</template>
