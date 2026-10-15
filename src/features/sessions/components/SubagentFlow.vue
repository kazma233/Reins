<script setup lang="ts">
import type { TimelineItem } from "../timeline-group";
import MessageBlockContent from "./MessageBlockContent.vue";
import CollapsedBlockRow from "./CollapsedBlockRow.vue";
import ImageRow from "./ImageRow.vue";
import MessageErrorRow from "./MessageErrorRow.vue";
import ToolRow from "./ToolRow.vue";

// 两种子代理过程弹窗（Pi 的 details 形态、family 聚合形态）共用的只读时间线。
// 弹窗内不会再出现子代理入口，故不分发 subagent 单元；其余单元与主时间线保持一致。
type SubagentFlowProps = {
  items: TimelineItem[];
  // 弹窗是"看这次过程"的语境，不跟随主时间线的只看对话开关，由调用方显式给值。
  conversationOnly: boolean;
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
      <ToolRow
        v-else-if="item.kind === 'tool'"
        :conversation-only="conversationOnly"
        :item="item"
      />
      <CollapsedBlockRow
        v-else-if="item.kind === 'collapsed-block'"
        :conversation-only="conversationOnly"
        :item="item"
      />
      <MessageErrorRow
        v-else-if="item.kind === 'message-error'"
        :conversation-only="conversationOnly"
        :item="item"
      />
      <ImageRow v-else-if="item.kind === 'image'" :item="item" />
    </template>
  </div>
</template>
