<script setup lang="ts">
import { ref } from "vue";
// 基础控件样式复用 AppInput，本组件 css 只保留差异（右侧按钮位等）。
import "./app-input.css";
import "./app-secret-input.css";

type AppSecretInputProps = {
  ariaLabel?: string;
  disabled?: boolean;
};

withDefaults(defineProps<AppSecretInputProps>(), {
  disabled: false,
});

const model = defineModel<string>({ required: true });
// 密钥默认不可见，仅在用户点击眼睛后临时切换为明文。
const visible = ref(false);
</script>

<template>
  <span class="app-secret-input">
    <input
      v-model="model"
      class="app-input app-secret-input__field"
      :type="visible ? 'text' : 'password'"
      :aria-label="ariaLabel"
      autocomplete="off"
      :disabled="disabled"
    />
    <button
      class="app-secret-input__toggle"
      type="button"
      :aria-label="visible ? '隐藏密钥' : '显示密钥'"
      :aria-pressed="visible"
      :disabled="disabled"
      @click="visible = !visible"
    >
      <svg
        aria-hidden="true"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z" />
        <circle cx="12" cy="12" r="3" />
        <line v-if="!visible" x1="4" y1="4" x2="20" y2="20" />
      </svg>
    </button>
  </span>
</template>
