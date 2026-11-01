<script setup lang="ts">
import { computed, ref } from "vue";
import { formatPayloadText, type ImageItem } from "../timeline-group";

// data: URL 自包含，直接渲染；外链/路径不自动请求（本地优先，不让会话文件触发
// 网络访问），只给引用行并支持展开看原始报文。
type ImageRowProps = {
  item: ImageItem;
};

const props = defineProps<ImageRowProps>();

const expanded = ref(false);
const inlineSource = computed(() =>
  props.item.reference.startsWith("data:image/") ? props.item.reference : null
);
const payloadText = computed(() => (expanded.value ? formatPayloadText(props.item.payload) : ""));
</script>

<template>
  <div class="flow-image">
    <template v-if="inlineSource">
      <img alt="会话中的图片" class="flow-image__preview" :src="inlineSource" />
      <span class="flow-image__caption">图片</span>
    </template>
    <template v-else>
      <button
        :aria-expanded="expanded"
        class="flow-image-row"
        type="button"
        @click="expanded = !expanded"
      >
        <span class="flow-row-tag">图片</span>
        <span class="flow-image__reference" :title="item.reference">{{ item.reference }}</span>
        <span class="flow-tool-chevron">{{ expanded ? "▾" : "▸" }}</span>
      </button>
      <pre v-if="expanded" class="flow-tool-detail">{{ payloadText }}</pre>
    </template>
  </div>
</template>
