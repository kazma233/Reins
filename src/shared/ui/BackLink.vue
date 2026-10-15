<script setup lang="ts">
import "./back-link.css";

// 页面级返回链接:整屏子页(用量统计、关于)共用一份样式,箭头与文案只有一处。
type BackLinkProps = {
  label?: string;
};

withDefaults(defineProps<BackLinkProps>(), {
  label: "返回"
});

defineEmits<{ back: [] }>();
</script>

<template>
  <button class="back-link" type="button" @click="$emit('back')">
    <span aria-hidden="true">←</span>
    {{ label }}
  </button>
</template>
