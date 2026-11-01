<script setup lang="ts">
// 操作结果的紧凑标识：只占一个圆形图标，完整文案放 hover 气泡里。
// 卡片操作行的宽度有限，长文案直接铺开会把按钮挤走，这里让图标固定宽度。
import AppTooltip from "./AppTooltip.vue";
import "./app-result-badge.css";

type AppResultBadgeProps = {
  tone: "success" | "danger";
  message: string;
  // 传 "top" 让气泡向上开：卡片 overflow: hidden 会裁掉向下的气泡
  placement?: "bottom" | "top";
};

withDefaults(defineProps<AppResultBadgeProps>(), {
  placement: "top",
});
</script>

<template>
  <AppTooltip align="end" :placement="placement" :tip="message">
    <span
      :class="`app-result-badge app-result-badge--${tone}`"
      :aria-label="message"
      role="status"
      tabindex="0"
    >
      <svg aria-hidden="true" focusable="false" viewBox="0 0 16 16">
        <circle class="app-result-badge__fill" cx="8" cy="8" r="8" />
        <path
          v-if="tone === 'success'"
          class="app-result-badge__mark"
          d="M4.9 8.3 7 10.4l4.2-4.8"
        />
        <path
          v-else
          class="app-result-badge__mark"
          d="M8 4.3v4.4M8 11.3v.4"
        />
      </svg>
    </span>
  </AppTooltip>
</template>
