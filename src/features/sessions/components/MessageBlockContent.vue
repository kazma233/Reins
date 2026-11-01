<script setup lang="ts">
import { computed } from "vue";
import { marked, type Tokens } from "marked";
import { isMarkdownBlock } from "../timeline-group";

// 沿用原先的 escape 策略：原始 HTML 一律转义为可见文本。
// marked 18 已移除 html 开关，默认会放行原始 HTML，只能用 renderer 覆盖兜住；
// breaks 开启让会话文本里的单换行像聊天场景一样显示为换行
marked.use({
  gfm: true,
  breaks: true,
  renderer: {
    html({ text }: Tokens.HTML | Tokens.Tag) {
      return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");
    }
  }
});

type MessageBlockContentProps = {
  contentText: string;
  kind: string;
};

const props = defineProps<MessageBlockContentProps>();

const html = computed(() =>
  isMarkdownBlock(props.kind)
    ? marked.parse(props.contentText, { async: false })
    : ""
);
</script>

<template>
  <div class="message-block-body">
    <!-- v-html 的内容来自 html:false 的 marked，原始 HTML 已被转义 -->
    <div
      v-if="isMarkdownBlock(kind)"
      class="message-markdown"
      v-html="html"
    />
    <pre v-else>{{ contentText }}</pre>
  </div>
</template>
