<script setup lang="ts">
import { computed } from "vue";
import DialogShell from "@shared/ui/DialogShell.vue";
import { formatTargetName } from "../../model";
import type { AgentTargetId, McpTransport, TargetConfigView } from "../../types";
import type { ProjectAgentPickerDiff } from "../../composables/useProjectAgentPicker";

type ProjectAgentPickerDialogProps = {
  open: boolean;
  loading: boolean;
  agents: TargetConfigView[];
  contextName: string;
  projectId: string;
  installedAgentIds: Set<AgentTargetId>;
  // 按钮展示的是期望终态而非现状：在终态集合里的按钮呈已应用样式
  desiredAgentIds: AgentTargetId[];
  pendingDiff: ProjectAgentPickerDiff;
  transport: McpTransport | null;
};

const props = defineProps<ProjectAgentPickerDialogProps>();

const emit = defineEmits<{
  close: [];
  toggleAgent: [agentId: AgentTargetId];
  apply: [];
}>();

const title = `${props.contextName} · MCP 项目目标:${props.projectId}`;
const eyebrow = "MCP · Project";

const canApply = computed(
  () => props.pendingDiff.toAdd.length > 0 || props.pendingDiff.toRemove.length > 0,
);

// pi 的 mcp.json 不接受 legacy SSE transport，写进去 pi 会拒绝连接，直接置灰。
function isSseUnsupportedAgent(agent: TargetConfigView): boolean {
  return props.transport === "sse" && agent.id === "pi";
}

function agentButtonTitle(agent: TargetConfigView): string {
  return isSseUnsupportedAgent(agent)
    ? `${formatTargetName(agent.id)} 不支持 SSE transport 的 MCP。`
    : "";
}

function agentButtonClass(agent: TargetConfigView): string {
  const compositeId = `${props.projectId}:${agent.id}` as AgentTargetId;
  return [
    "secondary-button",
    "manager-target-button",
    props.desiredAgentIds.includes(compositeId) && "is-installed",
  ]
    .filter(Boolean)
    .join(" ");
}
</script>

<template>
  <DialogShell
    :open="open"
    dialog-class-name="manager-import-dialog"
    :eyebrow="eyebrow"
    :title="title"
    title-id="project-agent-picker-dialog-title"
    :close-disabled="loading"
    @close="$emit('close')"
  >
    <template #actions>
      <button
        class="primary-button"
        :disabled="!canApply || loading"
        type="button"
        @click="$emit('apply')"
      >
        应用
      </button>
    </template>

    <div class="manager-stack">
      <p class="manager-field__hint">
        展示的是应用后的目标状态：亮起的 agent 将安装该 MCP，点击按钮可切换。应用前会先展示变更预览。
      </p>
      <div class="manager-target-buttons">
        <button
          v-for="agent in agents"
          :key="agent.id"
          :class="agentButtonClass(agent)"
          :disabled="loading || isSseUnsupportedAgent(agent)"
          :title="agentButtonTitle(agent)"
          type="button"
          @click="$emit('toggleAgent', agent.id)"
        >
          <span class="manager-target-button__label">{{ formatTargetName(agent.id) }}</span>
        </button>
      </div>
    </div>
  </DialogShell>
</template>
