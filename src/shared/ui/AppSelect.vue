<script lang="ts">
export type AppSelectOption = {
  value: string;
  label: string;
  disabled?: boolean;
};
</script>

<script setup lang="ts">
import { joinClasses } from "../lib/join-classes";
import "./app-select.css";

type AppSelectProps = {
  ariaLabel?: string;
  className?: string;
  disabled?: boolean;
  labelClassName?: string;
  label?: string;
  placeholder?: string;
  size?: "md" | "sm";
  tone?: "elevated" | "plain";
  title?: string;
  variant?: "floating" | "stacked";
  options: AppSelectOption[];
};

withDefaults(defineProps<AppSelectProps>(), {
  disabled: false,
  size: "md",
  tone: "elevated",
  variant: "floating",
});

const model = defineModel<string>({ required: true });
</script>

<template>
  <label
    :class="joinClasses(
      'app-select',
      variant === 'stacked' && 'app-select--stacked',
      size === 'sm' && 'app-select--sm',
      tone === 'plain' && 'app-select--plain',
      className,
    )"
    :title="title"
  >
    <span :class="joinClasses('app-select__label', labelClassName)">
      <slot name="label">{{ label }}</slot>
    </span>
    <select v-model="model" :aria-label="ariaLabel" :disabled="disabled">
      <option v-if="placeholder" disabled :value="''">
        {{ placeholder }}
      </option>
      <option
        v-for="option in options"
        :key="option.value"
        :disabled="option.disabled"
        :value="option.value"
      >
        {{ option.label }}
      </option>
    </select>
    <span aria-hidden="true" class="app-select__chevron" />
  </label>
</template>
