<script setup lang="ts">
import type { TimelineItem } from "../timeline-group";
import MessageBlockContent from "./MessageBlockContent.vue";
import CollapsedBlockRow from "./CollapsedBlockRow.vue";
import ToolRow from "./ToolRow.vue";

// 两种子代理过程弹窗（Pi 的 details 形态、family 聚合形态）共用的只读时间线。
// 弹窗内不会再出现子代理入口，故只分发这三种渲染单元。
type SubagentFlowProps = {
  items: TimelineItem[];
};

defineProps<SubagentFlowProps>();
</script>

<template>
  <div class="flow">
    <template v-for="item in items" :key="item.key">
      <div
        v-if="item.kind === 'text'"
        :class="item.role === 'user' ? 'flow-user' : 'flow-text'"
      >
        <MessageBlockContent
          v-for="(block, index) in item.blocks"
          :key="`${item.key}-${index}`"
          :content-text="block.text ?? ''"
          :kind="block.kind"
        />
      </div>
      <ToolRow v-else-if="item.kind === 'tool'" :item="item" />
      <CollapsedBlockRow v-else-if="item.kind === 'collapsed-block'" :item="item" />
    </template>
  </div>
</template>
