<script setup lang="ts">
import { joinClasses } from "../lib/join-classes";
import "./app-card.css";

type AppCardProps = {
  className?: string;
};

defineProps<AppCardProps>();
</script>

<template>
  <section :class="joinClasses('app-card', className)">
    <header v-if="$slots.header || $slots.ext || $slots.headerMeta" class="app-card__header">
      <div class="app-card__header-row">
        <div class="app-card__header-main">
          <slot name="header" />
        </div>
        <div v-if="$slots.ext" class="app-card__ext">
          <slot name="ext" />
        </div>
      </div>
      <div v-if="$slots.headerMeta" class="app-card__header-meta">
        <slot name="headerMeta" />
      </div>
    </header>
    <div class="app-card__content">
      <slot />
    </div>
    <footer v-if="$slots.footer || $slots.actions" class="app-card__footer">
      <slot name="footer" />
      <div v-if="$slots.actions" class="app-card__actions">
        <slot name="actions" />
      </div>
    </footer>
  </section>
</template>
