<script setup lang="ts">
import { joinClasses } from "../lib/join-classes";
import "./app-input.css";

type AppInputProps = {
  ariaLabel?: string;
  className?: string;
  disabled?: boolean;
  placeholder?: string;
  size?: "md" | "sm";
  type?: string;
};

withDefaults(defineProps<AppInputProps>(), {
  disabled: false,
  placeholder: undefined,
  size: "md",
  type: "text",
});

// modifiers 支持 v-model.number：与原生 v-model.number 一致，
// 解析失败或空串时保留原字符串，避免 number 字段被写成 0。
// model 放宽到 null/undefined：调用方的表单字段存在可空数字（如未填的上下文窗口），
// 原生 input 对空值渲染为空串，行为不变。
const [model, modifiers] = defineModel<string | number | null | undefined>({ default: "" });

function toModelValue(raw: string): string | number {
  if (!modifiers.number) {
    return raw;
  }
  const parsed = Number.parseFloat(raw);
  return raw === "" || Number.isNaN(parsed) ? raw : parsed;
}
</script>

<template>
  <input
    :value="model"
    :aria-label="ariaLabel"
    :class="joinClasses('app-input', size === 'sm' && 'app-input--sm', className)"
    :disabled="disabled"
    :placeholder="placeholder"
    :type="type"
    @input="model = toModelValue(($event.target as HTMLInputElement).value)"
  />
</template>
