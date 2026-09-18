<script setup lang="ts">
import "./app-tooltip.css";

type AppTooltipProps = {
  tip: string;
};

defineProps<AppTooltipProps>();
</script>

<template>
  <span class="app-tooltip">
    <slot />
    <span v-if="tip" aria-hidden="true" class="app-tooltip__bubble">{{ tip }}</span>
  </span>
</template>
