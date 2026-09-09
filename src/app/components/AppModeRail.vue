<script setup lang="ts">
import type { AppMode } from "../stores/app";
import "./app-mode-rail.css";

type AppModeRailProps = {
  mode: AppMode;
};

defineProps<AppModeRailProps>();

defineEmits<{
  change: [mode: AppMode];
}>();

const MODE_COPY: Array<{ id: AppMode; label: string }> = [
  { id: "workspace", label: "配置与分发" },
  { id: "sessions", label: "历史会话" },
];
</script>

<template>
  <nav class="app-mode-rail" aria-label="功能切换">
    <button
      v-for="item in MODE_COPY"
      :key="item.id"
      :class="`app-mode-rail__button${item.id === mode ? ' is-active' : ''}`"
      type="button"
      :aria-label="item.label"
      :title="item.label"
      :aria-current="item.id === mode ? 'true' : undefined"
      @click="$emit('change', item.id)"
    >
      <!-- puzzle：Skills & MCP 属于 agent 的扩展能力（lucide 线性图标风格）；history：会话语义 -->
      <svg
        v-if="item.id === 'workspace'"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path
          d="M15.39 4.39a1 1 0 0 0 1.68-.474 2.5 2.5 0 1 1 3.014 3.015 1 1 0 0 0-.474 1.68l1.683 1.682a2.414 2.414 0 0 1 0 3.414L19.61 15.39a1 1 0 0 1-1.68-.474 2.5 2.5 0 1 0-3.014 3.015 1 1 0 0 1 .474 1.68l-1.683 1.682a2.414 2.414 0 0 1-3.414 0L8.61 19.61a1 1 0 0 0-1.68.474 2.5 2.5 0 1 1-3.014-3.015 1 1 0 0 0 .474-1.68l-1.683-1.682a2.414 2.414 0 0 1 0-3.414L4.39 8.61a1 1 0 0 1 1.68.474 2.5 2.5 0 1 0 3.014-3.015 1 1 0 0 1-.474-1.68l1.683-1.682a2.414 2.414 0 0 1 3.414 0z"
        />
      </svg>
      <svg
        v-else
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
        <path d="M3 3v5h5" />
        <path d="M12 7v5l4 2" />
      </svg>
    </button>
  </nav>
</template>
