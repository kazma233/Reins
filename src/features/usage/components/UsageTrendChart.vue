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

function xAt(index: number): number {
  const last = props.axis.length - 1;
  if (last <= 0) {
    return PAD.left + innerWidth.value / 2;
  }
  return PAD.left + (index / last) * innerWidth.value;
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

// x 轴标签密度:文字已按真实像素渲染,按 11px 字号下标签实宽估算
// (day 标签"MM-DD"约 33px、hour"HH"约 14px,留余量取间距 40/24);
// 轴点超过上限时按固定步长抽稀,保证相邻标签严格等距。
const xTicks = computed(() => {
  const count = props.axis.length;
  if (count === 0) {
    return [];
  }
  const minSpacing = props.xLabelMode === "hour" ? 24 : 40;
  const maxLabels = Math.floor(innerWidth.value / minSpacing) + 1;
  const step = Math.max(1, Math.ceil(count / maxLabels));
  const ticks: Array<{ index: number; label: string }> = [];
  for (let index = 0; index < count; index += step) {
    const bucket = props.axis[index];
    ticks.push({ index, label: props.xLabelMode === "hour" ? bucket : bucket.slice(5) });
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
  hoverIndex.value = Math.min(last, Math.max(0, Math.round(ratio * last)));
}

const hoverX = computed(() =>
  hoverIndex.value === null ? null : xAt(hoverIndex.value)
);

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
      aria-label="各 agent 每日 token 消耗曲线"
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
      ref="tooltipEl"
      class="usage-chart__tooltip"
      :style="{ left: `${tooltipLeft}px` }"
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
