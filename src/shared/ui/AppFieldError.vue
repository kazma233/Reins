<script setup lang="ts">
// 表单字段错误的统一展示：字段旁的常驻红字，替代跳动的全局 toast。
// id 由调用方与输入框的 aria-describedby 配对，保证读屏能关联到字段。
// tone: success 用于操作成功回执，与错误共用同一个位置。
import "./app-field-error.css";

type AppFieldErrorProps = {
  id?: string;
  message: string | null;
  tone?: "danger" | "success";
};

withDefaults(defineProps<AppFieldErrorProps>(), {
  id: undefined,
  tone: "danger",
});
</script>

<template>
  <small
    v-if="message"
    :id="id"
    :class="`app-field-error${tone === 'success' ? ' app-field-error--success' : ''}`"
    role="alert"
  >
    {{ message }}
  </small>
</template>
