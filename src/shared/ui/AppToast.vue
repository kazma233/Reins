<script lang="ts">
export type AppToastNotice = {
  id: number;
  message: string;
  tone: "info" | "success" | "error";
};
</script>

<script setup lang="ts">
import { watch } from "vue";
import { useTimeoutFn } from "@vueuse/core";
import "./app-toast.css";

type AppToastProps = {
  notice: AppToastNotice | null;
};

const props = defineProps<AppToastProps>();

const emit = defineEmits<{ close: [] }>();

const { start, stop } = useTimeoutFn(() => {
  emit("close");
}, 3200);

// Reset the auto-dismiss timer whenever a new notice arrives.
watch(
  () => props.notice,
  (notice) => {
    stop();
    if (notice) {
      start();
    }
  },
  { immediate: true },
);
</script>

<template>
  <Teleport to="body">
    <div v-if="notice" class="app-toast-viewport" role="presentation">
      <div
        aria-live="polite"
        :class="`app-toast app-toast--${notice.tone}`"
        :role="notice.tone === 'error' ? 'alert' : 'status'"
      >
        <p>{{ notice.message }}</p>
        <button
          aria-label="关闭通知"
          class="app-toast__close"
          type="button"
          @click="$emit('close')"
        >
          关闭
        </button>
      </div>
    </div>
  </Teleport>
</template>
