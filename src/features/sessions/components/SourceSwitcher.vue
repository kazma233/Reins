<script setup lang="ts">
import { computed } from "vue";
import AppSelect, { type AppSelectOption } from "@shared/ui/AppSelect.vue";
import { joinClasses } from "@shared/lib/join-classes";
import {
  ALL_SOURCES,
  formatSourceAppName,
  type SourceSelection
} from "../source-app";
import type { SourceStatus } from "../types";
import "./source-switcher.css";

type SourceSwitcherProps = {
  sources: SourceStatus[];
  selectedSource: SourceSelection;
  disabled?: boolean;
  refreshing?: boolean;
  loading?: boolean;
};

const props = withDefaults(defineProps<SourceSwitcherProps>(), {
  disabled: false,
  refreshing: false,
  loading: false
});

const emit = defineEmits<{
  change: [source: SourceSelection];
  refresh: [];
}>();

const options = computed<AppSelectOption[]>(() => {
  const availableCount = props.sources
    .filter((source) => source.available)
    .reduce((sum, source) => sum + source.sessionCount, 0);
  return [
    { value: ALL_SOURCES, label: `全部 · ${availableCount}` },
    ...props.sources.map((source) => ({
      value: source.app,
      label: `${formatSourceAppName(source.app)} · ${source.sessionCount}`,
      disabled: !source.available
    }))
  ];
});

// Bridge the string-based AppSelect model to the typed selectedSource prop.
const selectedValue = computed<string>({
  get: () => props.selectedSource,
  set: (next) => {
    emit("change", next as SourceSelection);
  }
});
</script>

<template>
  <div class="source-switcher">
    <div v-if="loading" class="source-switcher__loading" role="status" aria-label="正在检测会话来源" />
    <div class="source-switcher__control">
      <AppSelect
        v-model="selectedValue"
        aria-label="选择会话来源"
        class-name="source-switcher__select"
        :disabled="disabled"
        label="会话来源"
        :options="options"
      />
      <button
        :aria-label="refreshing ? '刷新中' : '刷新会话来源'"
        :class="joinClasses('source-switcher__refresh', refreshing && 'is-refreshing')"
        :disabled="disabled"
        type="button"
        @click="emit('refresh')"
      >
        {{ refreshing ? "刷新中" : "刷新" }}
      </button>
    </div>
  </div>
</template>
