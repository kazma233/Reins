<script setup lang="ts">
import { computed } from "vue";
import type { SessionOverview } from "../types";
import { formatSessionModel, messageModelLabelKeys } from "../session-models";
import type { TimelineItem } from "../timeline-group";
import MessageBlockContent from "./MessageBlockContent.vue";
import CollapsedBlockRow from "./CollapsedBlockRow.vue";
import ImageRow from "./ImageRow.vue";
import MessageErrorRow from "./MessageErrorRow.vue";
import ProcessGroupRow from "./ProcessGroupRow.vue";
import ToolRow from "./ToolRow.vue";
import SubagentEntryRow from "./SubagentEntryRow.vue";
import SubagentGroupRow from "./SubagentGroupRow.vue";

// 文档流时间线：助手/工具内容靠左，用户输入右侧气泡；
// 工具与思考默认折叠成一行，子代理只暴露入口。
type MessageTimelineProps = {
  activeDetail: SessionOverview;
  // 只看对话：过程行(思考/工具/错误)收成一行，仍可逐条点开
  conversationOnly: boolean;
  emptyText: string;
  items: TimelineItem[];
  messageError: string | null;
  messagesLoading: boolean;
  visibleMessageLabel: string;
};

const props = defineProps<MessageTimelineProps>();

defineEmits<{ openSubagent: [sessionId: string, label: string] }>();

// 模型只标在输入与输出上，过程行不标；一条消息被过程行切成多段文本时只挂首段。
// 归属来自后端：助手消息直接带该次调用的模型，用户输入取这一轮回答的模型。
const modelLabelKeys = computed(() => messageModelLabelKeys(props.items));

function modelLabel(item: TimelineItem): string | null {
  if (item.kind !== "text" || !modelLabelKeys.value.has(item.key)) {
    return null;
  }
  return item.message.model ? formatSessionModel(item.message.model) : null;
}
</script>

<template>
  <p v-if="messageError" class="error-text">{{ messageError }}</p>
  <div v-if="messagesLoading && items.length === 0" class="flow">
    <article
      v-for="(_, index) in 3"
      :key="index"
      class="skeleton-card"
    >
      <span class="skeleton-line skeleton-title" />
      <span class="skeleton-line" />
      <span class="skeleton-line skeleton-short" />
    </article>
  </div>
  <p v-else-if="items.length === 0" class="muted-text">{{ emptyText }}</p>
  <div v-else class="flow">
    <template v-for="item in items" :key="item.key">
      <template v-if="item.kind === 'text'">
        <div v-if="item.role === 'user'" class="flow-user">
          <span v-if="modelLabel(item)" class="flow-model">{{ modelLabel(item) }}</span>
          <MessageBlockContent
            v-for="(block, index) in item.blocks"
            :key="`${item.key}-${index}`"
            :content-text="block.text ?? ''"
            :kind="block.kind"
          />
        </div>
        <div v-else class="flow-text">
          <span v-if="modelLabel(item)" class="flow-model">{{ modelLabel(item) }}</span>
          <MessageBlockContent
            v-for="(block, index) in item.blocks"
            :key="`${item.key}-${index}`"
            :content-text="block.text ?? ''"
            :kind="block.kind"
          />
        </div>
      </template>
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
      <ProcessGroupRow
        v-else-if="item.kind === 'process-group'"
        :conversation-only="conversationOnly"
        :item="item"
      />
      <ImageRow v-else-if="item.kind === 'image'" :item="item" />
      <SubagentEntryRow v-else-if="item.kind === 'subagent'" :item="item" />
      <SubagentGroupRow
        v-else-if="item.kind === 'subagent-group'"
        :item="item"
        @open="
          (sessionId: string, label: string) => $emit('openSubagent', sessionId, label)
        "
      />
    </template>
  </div>
  <div class="detail-pagination">
    <span class="muted-text">{{ visibleMessageLabel }}</span>
  </div>
</template>
