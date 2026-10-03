<script setup lang="ts">
import { computed, ref } from "vue";
import { formatTokenCount } from "@shared/lib/format";
import { niceCeil } from "../model";
import "./usage-trend-chart.css";

export type UsageChartSeries = {
  sourceApp: string;
  label: string;
  color: string;
  values: number[];
};

type UsageTrendChartProps = {
  axis: string[];
  series: UsageChartSeries[];
  // day 轴的标签取 "MM-DD",hour 轴的 bucket 本身就是 "HH"。
  xLabelMode?: "day" | "hour";
};

const props = defineProps<UsageTrendChartProps>();

// viewBox 固定、宽度 100%:鼠标换算按渲染宽度比例折回 viewBox 坐标。
const VIEW_WIDTH = 720;
const VIEW_HEIGHT = 260;
const PAD = { top: 16, right: 16, bottom: 30, left: 60 };
const INNER_WIDTH = VIEW_WIDTH - PAD.left - PAD.right;
const INNER_HEIGHT = VIEW_HEIGHT - PAD.top - PAD.bottom;

const maxY = computed(() => {
  const peak = Math.max(
    0,
    ...props.series.flatMap((item) => item.values)
  );
  return niceCeil(peak);
});

const gridTicks = computed(() =>
  [0, 0.25, 0.5, 0.75, 1].map((ratio) => ({
    label: formatTokenCount(maxY.value * ratio),
    y: PAD.top + INNER_HEIGHT - ratio * INNER_HEIGHT,
  }))
);

function xAt(index: number): number {
  const last = props.axis.length - 1;
  if (last <= 0) {
    return PAD.left + INNER_WIDTH / 2;
  }
  return PAD.left + (index / last) * INNER_WIDTH;
}

function yAt(value: number): number {
  return PAD.top + INNER_HEIGHT - (value / maxY.value) * INNER_HEIGHT;
}

const lines = computed(() =>
  props.series.map((item) => ({
    ...item,
    points: item.values
      .map((value, index) => `${xAt(index)},${yAt(value)}`)
      .join(" "),
  }))
);

// x 轴标签稀疏化:窗口短到放得下时逐日,否则均分 6 个刻度。
const xTicks = computed(() => {
  const count = props.axis.length;
  if (count === 0) {
    return [];
  }
  const labelCount = count <= 14 ? count : 6;
  const step = (count - 1) / (labelCount - 1 || 1);
  return Array.from({ length: labelCount }, (_, i) => {
    const index = Math.round(i * step);
    const bucket = props.axis[index];
    return { index, label: props.xLabelMode === "hour" ? bucket : bucket.slice(5) };
  });
});

const hoverIndex = ref<number | null>(null);

function handleMove(event: MouseEvent) {
  const target = event.currentTarget as SVGSVGElement;
  const bounds = target.getBoundingClientRect();
  const viewX = ((event.clientX - bounds.left) / bounds.width) * VIEW_WIDTH;
  const last = props.axis.length - 1;
  if (last <= 0) {
    hoverIndex.value = props.axis.length === 1 ? 0 : null;
    return;
  }
  const ratio = (viewX - PAD.left) / INNER_WIDTH;
  hoverIndex.value = Math.min(last, Math.max(0, Math.round(ratio * last)));
}

const hoverX = computed(() =>
  hoverIndex.value === null ? null : xAt(hoverIndex.value)
);

// tooltip 跟随悬停竖线,定位换算回容器百分比,避免与 SVG 缩放脱钩。
const tooltipLeftPercent = computed(() => {
  if (hoverX.value === null) {
    return 0;
  }
  return (hoverX.value / VIEW_WIDTH) * 100;
});
</script>

<template>
  <div class="usage-chart">
    <svg
      :viewBox="`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`"
      class="usage-chart__svg"
      role="img"
      aria-label="各 agent 每日 token 消耗曲线"
      @mousemove="handleMove"
      @mouseleave="hoverIndex = null"
    >
      <g v-for="tick in gridTicks" :key="tick.label">
        <line
          class="usage-chart__grid"
          :x1="PAD.left"
          :x2="VIEW_WIDTH - PAD.right"
          :y1="tick.y"
          :y2="tick.y"
        />
        <text class="usage-chart__axis-label" :x="PAD.left - 8" :y="tick.y + 4" text-anchor="end">
          {{ tick.label }}
        </text>
      </g>

      <text
        v-for="tick in xTicks"
        :key="tick.index"
        class="usage-chart__axis-label"
        :x="xAt(tick.index)"
        :y="VIEW_HEIGHT - 8"
        text-anchor="middle"
      >
        {{ tick.label }}
      </text>

      <!-- 单点窗口画不出线段,退化为每来源一个圆点。 -->
      <template v-if="axis.length > 1">
        <polyline
          v-for="line in lines"
          :key="line.sourceApp"
          class="usage-chart__line"
          :points="line.points"
          :stroke="line.color"
        />
      </template>

      <template v-if="hoverIndex !== null">
        <line
          class="usage-chart__cursor"
          :x1="hoverX!"
          :x2="hoverX!"
          :y1="PAD.top"
          :y2="PAD.top + INNER_HEIGHT"
        />
        <circle
          v-for="line in lines"
          :key="`hover-${line.sourceApp}`"
          class="usage-chart__dot"
          :cx="hoverX!"
          :cy="yAt(line.values[hoverIndex] ?? 0)"
          :fill="line.color"
          r="3.5"
        />
      </template>
    </svg>

    <div
      v-if="hoverIndex !== null"
      class="usage-chart__tooltip"
      :style="{ left: `${tooltipLeftPercent}%` }"
    >
      <div class="usage-chart__tooltip-day">{{ axis[hoverIndex] }}</div>
      <div
        v-for="line in series"
        :key="line.sourceApp"
        class="usage-chart__tooltip-row"
      >
        <span class="usage-chart__tooltip-dot" :style="{ background: line.color }" />
        <span class="usage-chart__tooltip-label">{{ line.label }}</span>
        <span class="usage-chart__tooltip-value">
          {{ formatTokenCount(line.values[hoverIndex] ?? 0) }}
        </span>
      </div>
    </div>
  </div>
</template>
