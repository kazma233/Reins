<script setup lang="ts">
import { joinClasses } from "../lib/join-classes";
import "./app-checkbox.css";

type AppCheckboxProps = {
  className?: string;
  disabled?: boolean;
};

withDefaults(defineProps<AppCheckboxProps>(), {
  disabled: false,
});

const checked = defineModel<boolean>({ required: true });
</script>

<template>
  <label :class="joinClasses('app-checkbox', className)">
    <input v-model="checked" type="checkbox" :disabled="disabled" />
    <span aria-hidden="true" class="app-checkbox__box" />
    <span class="app-checkbox__label">
      <slot />
    </span>
  </label>
</template>
