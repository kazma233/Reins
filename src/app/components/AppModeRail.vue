<script setup lang="ts">
import { computed } from "vue";
import type { AppMode } from "../stores/app";
import "./app-mode-rail.css";

type AppModeRailProps = {
  mode: AppMode;
};

const props = defineProps<AppModeRailProps>();

defineEmits<{
  change: [mode: AppMode];
  about: [];
}>();

// 用量统计不在左栏:它的入口在会话列表顶栏,返回入口在统计页顶部,两条路成对
// 出现,不必再占一个常驻条目。停在统计页时把"历史会话"点亮——它就是从那里进去
// 的子页,否则整条栏没有任何选中项,看起来像坏了。
const MODE_COPY: Array<{ id: AppMode; label: string }> = [
  { id: "workspace", label: "配置与分发" },
  { id: "providers", label: "模型配置" },
  { id: "sessions", label: "历史会话" },
];

const activeMode = computed<AppMode>(() =>
  props.mode === "usage" ? "sessions" : props.mode
);
</script>

<template>
  <nav class="app-mode-rail" aria-label="功能切换">
    <button
      v-for="item in MODE_COPY"
      :key="item.id"
      :class="`app-mode-rail__button${item.id === activeMode ? ' is-active' : ''}`"
      type="button"
      :aria-current="item.id === activeMode ? 'true' : undefined"
      @click="$emit('change', item.id)"
    >
      <!-- puzzle：Skills & MCP 属于 agent 的扩展能力（lucide 线性图标风格）；cpu：模型/Provider 语义；history：会话语义 -->
      <span class="app-mode-rail__icon">
        <svg
          v-if="item.id === 'workspace'"
          aria-hidden="true"
          fill="none"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <path
            d="M15.39 4.39a1 1 0 0 0 1.68-.474 2.5 2.5 0 1 1 3.014 3.015 1 1 0 0 0-.474 1.68l1.683 1.682a2.414 2.414 0 0 1 0 3.414L19.61 15.39a1 1 0 0 1-1.68-.474 2.5 2.5 0 1 0-3.014 3.015 1 1 0 0 1 .474 1.68l-1.683 1.682a2.414 2.414 0 0 1-3.414 0L8.61 19.61a1 1 0 0 0-1.68.474 2.5 2.5 0 1 1-3.014-3.015 1 1 0 0 0 .474-1.68l-1.683-1.682a2.414 2.414 0 0 1 0-3.414L4.39 8.61a1 1 0 0 1 1.68.474 2.5 2.5 0 1 0 3.014-3.015 1 1 0 0 1-.474-1.68l1.683-1.682a2.414 2.414 0 0 1 3.414 0z"
          />
        </svg>
        <svg
          v-else-if="item.id === 'providers'"
          aria-hidden="true"
          fill="none"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <!-- lucide cpu -->
          <rect x="4" y="4" width="16" height="16" rx="2" />
          <rect x="9" y="9" width="6" height="6" />
          <path d="M15 2v2" />
          <path d="M15 20v2" />
          <path d="M2 15h2" />
          <path d="M2 9h2" />
          <path d="M20 15h2" />
          <path d="M20 9h2" />
          <path d="M9 2v2" />
          <path d="M9 20v2" />
        </svg>
        <svg
          v-else
          aria-hidden="true"
          fill="none"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <!-- lucide history -->
          <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
          <path d="M3 3v5h5" />
          <path d="M12 7v5l4 2" />
        </svg>
      </span>
      <span class="app-mode-rail__label">{{ item.label }}</span>
    </button>

    <!-- 关于钉在底部:它是信息入口,不属于功能模式,所以不参与选中态 -->
    <div class="app-mode-rail__footer">
      <button
        :class="`app-mode-rail__button${mode === 'about' ? ' is-active' : ''}`"
        type="button"
        :aria-current="mode === 'about' ? 'true' : undefined"
        @click="$emit('about')"
      >
        <span class="app-mode-rail__icon">
          <svg
            aria-hidden="true"
            fill="none"
            stroke="currentColor"
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            viewBox="0 0 24 24"
          >
            <!-- lucide info -->
            <circle cx="12" cy="12" r="10" />
            <path d="M12 16v-4" />
            <path d="M12 8h.01" />
          </svg>
        </span>
        <span class="app-mode-rail__label">关于</span>
      </button>
    </div>
  </nav>
</template>
