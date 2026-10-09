<script setup lang="ts">
// 表单 / 弹窗错误的统一展示：错误旁的常驻红字，替代跳动的全局 toast。
// id 由调用方与输入框的 aria-describedby 配对，保证读屏能关联到字段。
import "./app-field-error.css";

type AppFieldErrorProps = {
  id?: string;
  message: string | null;
};

withDefaults(defineProps<AppFieldErrorProps>(), {
  id: undefined,
});
</script>

<template>
  <small v-if="message" :id="id" class="app-field-error" role="alert">
    {{ message }}
  </small>
</template>
