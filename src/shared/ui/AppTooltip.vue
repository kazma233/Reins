<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { joinClasses } from "../lib/join-classes";
import { computeTooltipOffset } from "../lib/tooltip-position";
import "./app-tooltip.css";

type AppTooltipProps = {
  tip: string;
  // 卡片 / 弹窗设了 overflow: hidden，就地渲染的气泡会被裁掉，
  // 因此气泡统一 Teleport 到 body 并用 fixed 定位。
  placement?: "bottom" | "top";
  // 靠右侧的触发元素用 end：气泡右边界对齐触发元素，向左展开
  align?: "start" | "end";
};

const props = withDefaults(defineProps<AppTooltipProps>(), {
  placement: "bottom",
  align: "start",
});

const triggerRef = ref<HTMLElement | null>(null);
const bubbleRef = ref<HTMLElement | null>(null);
const visible = ref(false);
const offset = ref({ top: 0, left: 0 });

const bubbleStyle = computed(() => ({
  top: `${offset.value.top}px`,
  left: `${offset.value.left}px`,
}));

function updateOffset() {
  const trigger = triggerRef.value;
  const bubble = bubbleRef.value;
  if (!trigger || !bubble) return;

  const triggerRect = trigger.getBoundingClientRect();
  const bubbleRect = bubble.getBoundingClientRect();
  offset.value = computeTooltipOffset({
    trigger: {
      top: triggerRect.top,
      left: triggerRect.left,
      right: triggerRect.right,
      bottom: triggerRect.bottom,
    },
    bubble: { width: bubbleRect.width, height: bubbleRect.height },
    viewport: { width: window.innerWidth, height: window.innerHeight },
    placement: props.placement,
    align: props.align,
  });
}

function hide() {
  visible.value = false;
  document.removeEventListener("scroll", hide, true);
  window.removeEventListener("resize", hide);
}

function show() {
  if (!props.tip) return;
  updateOffset();
  visible.value = true;
  // 触发元素会随滚动 / 改窗口移动，浮层不跟随，直接收起避免指向错位置
  document.addEventListener("scroll", hide, true);
  window.addEventListener("resize", hide);
}

watch(
  () => props.tip,
  (tip) => {
    if (!tip) hide();
  },
);

onBeforeUnmount(hide);
</script>

<template>
  <span
    ref="triggerRef"
    class="app-tooltip"
    @mouseenter="show"
    @mouseleave="hide"
    @focusin="show"
    @focusout="hide"
  >
    <slot />
  </span>
  <Teleport to="body">
    <span
      v-if="tip"
      ref="bubbleRef"
      :class="joinClasses('app-tooltip__bubble', visible && 'app-tooltip__bubble--visible')"
      :style="bubbleStyle"
      aria-hidden="true"
    >
      {{ tip }}
    </span>
  </Teleport>
</template>
