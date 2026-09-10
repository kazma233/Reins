<script setup lang="ts">
import { computed } from "vue";
import MarkdownRender from "markstream-vue";
import "markstream-vue/index.css";
import { joinClasses } from "@shared/lib/join-classes";
import { isMarkdownBlock } from "../composables/session-detail-helpers";

type MessageBlockContentProps = {
  contentText: string;
  kind: string;
  expanded: boolean;
  needsExpand: boolean;
};

const props = defineProps<MessageBlockContentProps>();

const emit = defineEmits<{
  expand: [];
}>();

// 展开后不再响应内容区点击，避免选中文字、点链接等操作误触收起
const collapsed = computed(() => props.needsExpand && !props.expanded);
const wrapperClass = computed(() =>
  joinClasses("message-block-body", collapsed.value ? "collapsed" : null)
);

function handleBodyClick() {
  if (collapsed.value) {
    emit("expand");
  }
}
</script>

<template>
  <div :class="wrapperClass" @click="handleBodyClick">
    <MarkdownRender
      v-if="isMarkdownBlock(kind)"
      class="message-markdown"
      :content="contentText"
      :final="true"
      html-policy="escape"
      mode="chat"
    />
    <pre v-else>{{ contentText }}</pre>
  </div>
</template>
