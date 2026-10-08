<script setup lang="ts">
import { computed } from "vue";
import { joinClasses } from "@shared/lib/join-classes";
import { formatTargetLabel } from "../model";
import type { AgentTargetId, McpTransport, McpTargetInspection } from "../types";

type McpTargetButtonProps = {
  serverName: string;
  targetId: AgentTargetId;
  targetItem: McpTargetInspection | null;
  transport: McpTransport;
  loading?: boolean;
};

const props = withDefaults(defineProps<McpTargetButtonProps>(), {
  loading: false,
});

defineEmits<{
  toggle: [serverName: string, targetId: AgentTargetId];
}>();

const installed = computed(() => props.targetItem?.state === "present");
// 未配置 MCP 的 target 没有可写入的配置文件，禁用而不是点击后报错。
const unconfigured = computed(() => props.targetItem?.state === "unconfigured");
// pi 的 mcp.json 不接受 legacy SSE transport，写进去 pi 会拒绝连接，直接置灰。
const sseUnsupported = computed(() => props.transport === "sse" && props.targetId === "pi");
// dsh 分发只映射 stdio 形态；远端 transport 写出的条目 dsh 无法启动。
const remoteUnsupported = computed(
  () => props.transport !== "stdio" && props.targetId === "dsh",
);
const warning = computed(
  () => props.targetItem?.state === "error" || props.targetItem?.state === "unconfigured",
);
const buttonStateClass = computed(() => {
  if (installed.value) return " is-installed";
  if (warning.value) return " is-warning";
  return "";
});
const label = computed(() => formatTargetLabel(props.targetId));
const detail = computed(() => {
  if (sseUnsupported.value) return "pi 不支持 SSE transport 的 MCP。";
  if (remoteUnsupported.value) return "dsh 仅支持 stdio transport 的 MCP。";
  return props.targetItem?.detail ?? `${props.serverName} 在 ${label.value} 的状态未知`;
});
</script>

<template>
  <button
    :class="joinClasses('secondary-button', 'manager-target-button', buttonStateClass)"
    :disabled="loading || unconfigured || sseUnsupported || remoteUnsupported"
    :title="detail"
    type="button"
    @click="$emit('toggle', serverName, targetId)"
  >
    {{ label }}
  </button>
</template>
