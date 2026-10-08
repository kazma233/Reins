<script setup lang="ts">
// 区域级加载失败态：常驻在出错区域原位，并给出重试入口。
// 与 AppFieldError 分开是因为它承载动作（重试），不只是文字。
import "./app-load-error.css";

type AppLoadErrorProps = {
  message: string;
  retrying?: boolean;
  retryLabel?: string;
};

withDefaults(defineProps<AppLoadErrorProps>(), {
  retrying: false,
  retryLabel: "重试",
});

defineEmits<{ retry: [] }>();
</script>

<template>
  <div class="app-load-error" role="alert">
    <p class="app-load-error__message">{{ message }}</p>
    <button
      class="secondary-button app-load-error__retry"
      :disabled="retrying"
      type="button"
      @click="$emit('retry')"
    >
      {{ retrying ? "重试中…" : retryLabel }}
    </button>
  </div>
</template>
