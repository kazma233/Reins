<script lang="ts">
export type AppMultiSelectOption = {
  value: string;
  label: string;
  // 面板 label 可带补充说明（如键名），触发器摘要优先用更短的标签。
  summary?: string;
};
</script>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { joinClasses } from "../lib/join-classes";
import AppCheckbox from "./AppCheckbox.vue";
import "./app-multi-select.css";

type AppMultiSelectProps = {
  ariaLabel?: string;
  className?: string;
  disabled?: boolean;
  size?: "md" | "sm";
  placeholder?: string;
  options: AppMultiSelectOption[];
};

const props = withDefaults(defineProps<AppMultiSelectProps>(), {
  disabled: false,
  size: "md",
  placeholder: "未设置",
});

const selected = defineModel<string[]>({ required: true });

const open = ref(false);
const root = ref<HTMLElement | null>(null);

const summary = computed(() => {
  const chosen = props.options.filter((option) => selected.value.includes(option.value));
  if (chosen.length === 0) {
    return props.placeholder;
  }
  return chosen.map((option) => option.summary ?? option.label).join(" / ");
});

function toggle(value: string, checked: boolean) {
  const next = new Set(selected.value);
  if (checked) {
    next.add(value);
  } else {
    next.delete(value);
  }
  // 选中集合按 options 展示顺序排序，触发器摘要与面板保持一致。
  selected.value = props.options
    .map((option) => option.value)
    .filter((optionValue) => next.has(optionValue));
}

// 勾选即时生效，面板靠点击外部区域收起。
function onDocumentPointerDown(event: PointerEvent) {
  if (root.value && !root.value.contains(event.target as Node)) {
    open.value = false;
  }
}

watch(open, (value) => {
  if (value) {
    document.addEventListener("pointerdown", onDocumentPointerDown);
  } else {
    document.removeEventListener("pointerdown", onDocumentPointerDown);
  }
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onDocumentPointerDown);
});
</script>

<template>
  <div ref="root" :class="joinClasses('app-multi-select', className)">
    <button
      type="button"
      :aria-label="ariaLabel"
      :aria-expanded="open"
      :class="joinClasses(
        'app-multi-select__trigger',
        size === 'sm' && 'app-multi-select__trigger--sm',
      )"
      :disabled="disabled"
      @click="open = !open"
    >
      <span class="app-multi-select__summary">{{ summary }}</span>
    </button>
    <div v-if="open" class="app-multi-select__panel">
      <AppCheckbox
        v-for="option in options"
        :key="option.value"
        class-name="app-multi-select__option"
        :disabled="disabled"
        :model-value="selected.includes(option.value)"
        @update:model-value="toggle(option.value, $event)"
      >
        {{ option.label }}
      </AppCheckbox>
    </div>
  </div>
</template>
