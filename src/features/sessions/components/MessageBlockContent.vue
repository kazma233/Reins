<script setup lang="ts">
import MarkdownRender from "markstream-vue";
import "markstream-vue/index.css";
import { isMarkdownBlock } from "../timeline-group";

type MessageBlockContentProps = {
  contentText: string;
  kind: string;
};

defineProps<MessageBlockContentProps>();
</script>

<template>
  <div class="message-block-body">
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
