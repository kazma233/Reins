<script setup lang="ts">
import { computed, ref } from "vue";
import { useElementSize } from "@vueuse/core";
import { formatTokenCount } from "@shared/lib/format";
import { clampCenteredOffset } from "@shared/lib/tooltip-position";
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

// 小时桶是区间量（"23" = 23:00–24:00）：刻度画 0..24 的边界时刻，折线两端
// 补半个桶的平段，从 0 点铺满到 24 点，避免末桶 "23" 被读成"统计到 23:00
// 截止"，也避免线段在轴两端留空。
const isHourMode = computed(() => props.xLabelMode === "hour");
const HOUR_BOUNDS = 24;

// 宽度跟随容器、高度固定:SVG 用真实像素绘制(viewBox 与渲染尺寸 1:1),
// 文字不随窗口缩放变形;容器未测出前用兑底宽,挂载后立即校正。
const chartEl = ref<HTMLElement | null>(null);
const { width: measuredWidth } = useElementSize(chartEl);
const VIEW_HEIGHT = 260;
const PAD = { top: 16, right: 16, bottom: 30, left: 60 };
const viewWidth = computed(() => Math.max(measuredWidth.value, 320));
const innerWidth = computed(() => viewWidth.value - PAD.left - PAD.right);
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

// 小时模式数据点落在桶中点(区间量),日模式落在桶位置。
function xAt(index: number): number {
  if (isHourMode.value) {
    return PAD.left + ((index + 0.5) / props.axis.length) * innerWidth.value;
  }
  const last = props.axis.length - 1;
  if (last <= 0) {
    return PAD.left + innerWidth.value / 2;
  }
  return PAD.left + (index / last) * innerWidth.value;
}

function yAt(value: number): number {
  return PAD.top + INNER_HEIGHT - (value / maxY.value) * INNER_HEIGHT;
}

// 折线顶点落在桶中点,再在两端各补一个与首/末桶同高的端点(轴两端),
// 线段因此铺满 0 点到 24 点,顶点与悬停圆点位置一致。
function hourLinePoints(values: number[]): string {
  if (values.length === 0) {
    return "";
  }
  const vertices = values.map(
    (value, index) => [xAt(index), yAt(value)] as [number, number]
  );
  const points: Array<[number, number]> = [
    [PAD.left, vertices[0][1]],
    ...vertices,
    [PAD.left + innerWidth.value, vertices[vertices.length - 1][1]],
  ];
  return points.map(([x, y]) => `${x},${y}`).join(" ");
}

const lines = computed(() =>
  props.series.map((item) => ({
    ...item,
    points: isHourMode.value
      ? hourLinePoints(item.values)
      : item.values.map((value, index) => `${xAt(index)},${yAt(value)}`).join(" "),
  }))
);

// x 轴标签密度:文字已按真实像素渲染,按 11px 字号下标签实宽估算
// (day 标签"MM-DD"约 33px、hour"HH"约 14px,留余量取间距 40/24);
// 轴点超过上限时按固定步长抽稀,保证相邻标签严格等距。
// 小时模式是边界刻度:步长取 24 的因子,末刻度 24 与首刻度 0 必然都在。
const xTicks = computed(() => {
  const count = props.axis.length;
  if (count === 0) {
    return [];
  }
  const minSpacing = isHourMode.value ? 24 : 40;
  const maxLabels = Math.floor(innerWidth.value / minSpacing) + 1;
  if (isHourMode.value) {
    const step = [1, 2, 3, 4, 6, 8, 12, 24].find(
      (divisor) => HOUR_BOUNDS / divisor + 1 <= maxLabels
    ) ?? HOUR_BOUNDS;
    const ticks: Array<{ x: number; label: string }> = [];
    for (let hour = 0; hour <= HOUR_BOUNDS; hour += step) {
      ticks.push({
        x: PAD.left + (hour / HOUR_BOUNDS) * innerWidth.value,
        label: String(hour),
      });
    }
    return ticks;
  }
  const step = Math.max(1, Math.ceil(count / maxLabels));
  const ticks: Array<{ x: number; label: string }> = [];
  for (let index = 0; index < count; index += step) {
    const bucket = props.axis[index];
    ticks.push({ x: xAt(index), label: bucket.slice(5) });
  }
  return ticks;
});

const hoverIndex = ref<number | null>(null);

function handleMove(event: MouseEvent) {
  const target = event.currentTarget as SVGSVGElement;
  const bounds = target.getBoundingClientRect();
  // viewBox 与渲染像素 1:1,无需比例折算
  const viewX = event.clientX - bounds.left;
  const last = props.axis.length - 1;
  if (last <= 0) {
    hoverIndex.value = props.axis.length === 1 ? 0 : null;
    return;
  }
  const ratio = (viewX - PAD.left) / innerWidth.value;
  if (isHourMode.value) {
    // 桶中点几何下按区间归属取桶,而不是就近取点。
    const clamped = Math.min(1, Math.max(0, ratio));
    hoverIndex.value = Math.min(last, Math.floor(clamped * props.axis.length));
    return;
  }
  hoverIndex.value = Math.min(last, Math.max(0, Math.round(ratio * last)));
}

const hoverX = computed(() =>
  hoverIndex.value === null ? null : xAt(hoverIndex.value)
);

// 小时桶标题按区间展示（"23:00–24:00"），日期桶标题原样。
const tooltipTitle = computed(() => {
  if (hoverIndex.value === null) {
    return "";
  }
  const bucket = props.axis[hoverIndex.value];
  return isHourMode.value
    ? `${bucket}:00–${Number(bucket) + 1}:00`
    : bucket;
});

// tooltip 跟随悬停竖线,viewBox 已是像素坐标,直接定位。
// 气泡以光标线居中（CSS translateX(-50%)），悬停到最左/最右时会被容器边缘裁掉，
// 因此把中心点夹取到容器内；宽度实测，首帧按 CSS 的 min-width 兜底。
const CHART_TOOLTIP_MIN_WIDTH = 148;
const tooltipEl = ref<HTMLElement | null>(null);
const { width: tooltipWidth } = useElementSize(tooltipEl);

const tooltipLeft = computed(() => {
  if (hoverX.value === null) return 0;
  return clampCenteredOffset({
    center: hoverX.value,
    bubbleWidth: Math.max(tooltipWidth.value, CHART_TOOLTIP_MIN_WIDTH),
    containerWidth: viewWidth.value,
  });
});
</script>

<template>
  <div ref="chartEl" class="usage-chart">
    <svg
      :viewBox="`0 0 ${viewWidth} ${VIEW_HEIGHT}`"
      class="usage-chart__svg"
      role="img"
      :aria-label="isHourMode ? '各 agent 每小时 token 消耗曲线' : '各 agent 每日 token 消耗曲线'"
      @mousemove="handleMove"
      @mouseleave="hoverIndex = null"
    >
      <g v-for="tick in gridTicks" :key="tick.label">
        <line
          class="usage-chart__grid"
          :x1="PAD.left"
          :x2="viewWidth - PAD.right"
          :y1="tick.y"
          :y2="tick.y"
        />
        <text class="usage-chart__axis-label" :x="PAD.left - 8" :y="tick.y + 4" text-anchor="end">
          {{ tick.label }}
        </text>
      </g>

      <text
        v-for="tick in xTicks"
        :key="tick.label"
        class="usage-chart__axis-label"
        :x="tick.x"
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
      ref="tooltipEl"
      class="usage-chart__tooltip"
      :style="{ left: `${tooltipLeft}px` }"
    >
      <div class="usage-chart__tooltip-day">{{ tooltipTitle }}</div>
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
