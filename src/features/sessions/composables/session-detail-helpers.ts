import type {
  SessionAgent,
  SessionEvent,
  SessionMessage,
  SessionOverview
} from "../types";

export const CONVERSATION_BLOCK_KINDS = new Set([
  "text",
  "input_text",
  "output_text"
]);

export type MessageTag = {
  key: string;
  label: string;
  kind: "block" | "tool";
};

export type PreparedMessageBlock = {
  block: SessionMessage["blocks"][number];
  contentText: string | null;
  needsExpand: boolean;
};

export type PreparedTimelineMessage = {
  key: string;
  message: SessionMessage;
  isSubagentMarker: boolean;
  blocks: PreparedMessageBlock[];
};

export type VisibleTimelineMessage = PreparedTimelineMessage & {
  blocksToRender: PreparedMessageBlock[];
  needsExpand: boolean;
  searchText: string;
  tags: MessageTag[];
};

export type PreparedTimelineEvent = {
  key: string;
  event: SessionEvent;
  agentSessionId: string | null;
  isSubagentLifecycle: boolean;
  payloadText: string | null;
  searchText: string;
};

// --- scroll helpers ---

export function isNearBottom(element: HTMLElement, threshold = 240): boolean {
  return element.scrollHeight - element.scrollTop - element.clientHeight <= threshold;
}

// --- payload / search helpers ---

export function renderPayload(value: unknown): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  return JSON.stringify(value, null, 2);
}

export function normalizeFilterValue(value: string): string {
  return value.trim().toLocaleLowerCase("zh-CN");
}

export function buildSearchText(parts: Array<string | null | undefined>): string {
  return parts
    .map((part) => (part ? normalizeFilterValue(part) : ""))
    .filter((part) => part.length > 0)
    .join("\n");
}

// --- message block helpers ---

export function blockContentText(block: SessionMessage["blocks"][number]): string | null {
  if (block.text) {
    return block.text;
  }
  return renderPayload(block.payload);
}

export function blockNeedsExpand(block: SessionMessage["blocks"][number]): boolean {
  const content = blockContentText(block);
  if (!content) {
    return false;
  }
  return content.split("\n").length > 10 || content.length > 800;
}

export function isSubagentMarkerMessage(message: SessionMessage): boolean {
  return message.blocks.some((block) => {
    if (typeof block.payload !== "object" || block.payload === null) {
      return false;
    }
    const payload = block.payload as Record<string, unknown>;
    return payload.type === "subagent_started";
  });
}

export function sessionIdFromPayload(value: unknown): string | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const sessionId = Reflect.get(value, "session_id");
  return typeof sessionId === "string" ? sessionId : null;
}

export function extractSubagentLabel(message: SessionMessage): string | null {
  for (const block of message.blocks) {
    if (typeof block.payload !== "object" || block.payload === null) {
      continue;
    }
    const payload = block.payload as Record<string, unknown>;
    if (payload.type === "subagent_started") {
      const title = typeof payload.title === "string" ? payload.title : null;
      const id =
        typeof payload.session_id === "string"
          ? payload.session_id
          : message.sessionId ?? null;
      if (title) {
        return title;
      }
      if (id) {
        return `Sub-agent ${id.slice(0, 8)}`;
      }
    }
  }
  return null;
}

export function isSubagentLifecycleEvent(event: SessionEvent): boolean {
  return (
    event.kind === "subagent_started" ||
    event.kind === "subagent_spawned" ||
    event.kind === "subagent_closed" ||
    event.kind === "subagent_notification"
  );
}

export function eventSessionId(event: SessionEvent): string | null {
  return event.sessionId ?? sessionIdFromPayload(event.payload);
}

// --- agent filtering ---

export function messageMatchesAgent(
  message: SessionMessage,
  selectedAgentId: string | "all",
  rootSessionId: string
): boolean {
  if (selectedAgentId === "all") {
    return true;
  }
  if (message.sessionId) {
    return message.sessionId === selectedAgentId;
  }
  return selectedAgentId === rootSessionId;
}

export function eventMatchesAgent(
  event: SessionEvent,
  selectedAgentId: string | "all",
  rootSessionId: string
): boolean {
  if (selectedAgentId === "all") {
    return true;
  }
  const sessionId = eventSessionId(event);
  if (sessionId) {
    return sessionId === selectedAgentId;
  }
  return selectedAgentId === rootSessionId;
}

// --- block-kind labels ---

export function formatBlockKind(kind: string): string {
  switch (kind) {
    case "text":
      return "文本";
    case "reasoning":
      return "思考";
    case "tool":
      return "工具";
    case "patch":
      return "补丁";
    case "file":
      return "文件";
    case "input_text":
      return "输入文本";
    case "output_text":
      return "输出文本";
    case "unsupported_content":
      return "未知内容";
    case "unsupported_block":
      return "未知块";
    case "empty_message":
      return "空消息";
    case "empty_tool":
      return "空工具";
    default:
      return kind;
  }
}

export function buildMessageTags(blocks: PreparedMessageBlock[]): MessageTag[] {
  const seen = new Set<string>();
  const tags: MessageTag[] = [];

  for (const { block } of blocks) {
    const blockKey = `block:${block.kind}`;
    if (!seen.has(blockKey)) {
      seen.add(blockKey);
      tags.push({
        key: blockKey,
        label: formatBlockKind(block.kind),
        kind: "block"
      });
    }

    if (block.toolName) {
      const toolKey = `tool:${block.toolName}`;
      if (!seen.has(toolKey)) {
        seen.add(toolKey);
        tags.push({
          key: toolKey,
          label: block.toolName,
          kind: "tool"
        });
      }
    }
  }

  return tags;
}

// --- stable keys ---

export function messageStableKey(message: SessionMessage): string {
  return `${message.sessionId ?? "root"}:${message.id}`;
}

export function eventStableKey(event: SessionEvent): string {
  return `${eventSessionId(event) ?? event.sessionId ?? "root"}:${event.id}`;
}

// --- timeline preparation ---

export function prepareMessageBlock(
  block: SessionMessage["blocks"][number]
): PreparedMessageBlock {
  return {
    block,
    contentText: blockContentText(block),
    needsExpand: blockNeedsExpand(block)
  };
}

export function prepareTimelineMessage(message: SessionMessage): PreparedTimelineMessage {
  return {
    key: messageStableKey(message),
    message,
    isSubagentMarker: isSubagentMarkerMessage(message),
    blocks: message.blocks.map(prepareMessageBlock)
  };
}

export function messageSearchText(
  message: SessionMessage,
  blocks: PreparedMessageBlock[]
): string {
  return buildSearchText([
    message.id,
    message.role,
    message.sessionId,
    ...blocks.flatMap(({ block, contentText }) => [
      block.kind,
      formatBlockKind(block.kind),
      block.toolName,
      block.toolCallId,
      contentText
    ])
  ]);
}

export function toVisibleTimelineMessage(
  prepared: PreparedTimelineMessage,
  conversationOnly: boolean
): VisibleTimelineMessage | null {
  if (
    conversationOnly &&
    prepared.message.role !== "user" &&
    prepared.message.role !== "assistant"
  ) {
    return null;
  }

  const blocksToRender = conversationOnly
    ? prepared.blocks.filter(({ block }) => CONVERSATION_BLOCK_KINDS.has(block.kind))
    : prepared.blocks;

  if (blocksToRender.length === 0) {
    return null;
  }

  return {
    ...prepared,
    blocksToRender,
    needsExpand: blocksToRender.some((block) => block.needsExpand),
    searchText: messageSearchText(prepared.message, blocksToRender),
    tags: buildMessageTags(blocksToRender)
  };
}

export function prepareTimelineEvent(event: SessionEvent): PreparedTimelineEvent {
  const payloadText = renderPayload(event.payload);
  const agentSessionId = eventSessionId(event);

  return {
    key: eventStableKey(event),
    event,
    agentSessionId,
    isSubagentLifecycle: isSubagentLifecycleEvent(event),
    payloadText,
    searchText: buildSearchText([
      event.id,
      event.kind,
      event.summary,
      event.sessionId,
      agentSessionId,
      payloadText
    ])
  };
}

// --- agent display helpers ---

export function fallbackAgent(overview: SessionOverview): SessionAgent {
  return {
    sessionId: overview.summary.sourceSessionId,
    label: "主 Agent",
    isRoot: true
  };
}

export function agentLabel(sessionId: string, agents: SessionAgent[]): string {
  const match = agents.find((agent) => agent.sessionId === sessionId);
  if (match) {
    return match.label;
  }
  return `Agent ${sessionId.slice(0, 8)}`;
}

export function agentDisplayLabel(sessionId: string, agents: SessionAgent[]): string {
  const label = agentLabel(sessionId, agents);
  if (label === "主 Agent" || label.startsWith("Agent ")) {
    return label;
  }

  const duplicateCount = agents.filter((agent) => agent.label === label).length;
  if (duplicateCount <= 1) {
    return label;
  }
  return `${label} · ${sessionId.slice(0, 8)}`;
}

// --- visible label / empty text helpers ---

export function formatVisibleMessageLabel(
  selectedAgentLabel: string,
  conversationOnly: boolean,
  visibleMessageCount: number,
  totalMessageCount: number,
  timelineFilter: string
): string {
  const unit = conversationOnly ? "轮对话" : "条消息";

  if (timelineFilter.length > 0) {
    return `${selectedAgentLabel} · 匹配 ${visibleMessageCount} / ${totalMessageCount} ${unit}`;
  }

  if (conversationOnly) {
    return `${selectedAgentLabel} · 已显示 ${visibleMessageCount} ${unit}`;
  }

  return `${selectedAgentLabel} · 已显示 ${totalMessageCount} ${unit}`;
}

export function formatVisibleEventLabel(
  selectedAgentLabel: string,
  filteredEventCount: number,
  totalEventCount: number,
  nextEventOffset: number | null,
  timelineFilter: string
): string {
  if (timelineFilter.length > 0) {
    return `${selectedAgentLabel} · 匹配 ${filteredEventCount} / ${totalEventCount} 条事件`;
  }

  if (totalEventCount === 0 && nextEventOffset === 0) {
    return `${selectedAgentLabel} · 未加载事件`;
  }

  return `${selectedAgentLabel} · 已显示 ${totalEventCount} 条事件`;
}

export function emptyMessageText(
  selectedAgentLabel: string,
  conversationOnly: boolean,
  timelineFilter: string,
  hiddenMessageCount: number
): string {
  if (timelineFilter.length > 0) {
    return `${selectedAgentLabel} 中没有匹配“${timelineFilter}”的${
      conversationOnly ? "用户/助手对话" : "消息"
    }。`;
  }

  if (conversationOnly && hiddenMessageCount > 0) {
    return `${selectedAgentLabel} 当前只包含工具、思考或诊断消息；关闭“仅看对话”可查看完整内容。`;
  }

  if (conversationOnly) {
    return `${selectedAgentLabel} 当前暂无用户/助手对话。`;
  }

  return `${selectedAgentLabel} 当前暂无消息。`;
}

export function emptyEventText(
  selectedAgentLabel: string,
  timelineFilter: string
): string {
  if (timelineFilter.length > 0) {
    return `${selectedAgentLabel} 中没有匹配“${timelineFilter}”的事件。`;
  }
  return `${selectedAgentLabel} 当前暂无事件。`;
}

// --- message card class helpers ---

export function messageCardClassName(
  isMarker: boolean,
  isInSubagent: boolean
): string {
  if (isMarker) {
    return "timeline-card subagent-marker-card";
  }
  if (isInSubagent) {
    return "timeline-card subagent-message-card";
  }
  return "timeline-card";
}

export function blockClassName(isExpanded: boolean): string {
  return isExpanded ? "message-block-text expanded" : "message-block-text";
}

export function shouldShowEventAgentChip(
  agentSessionId: string | null,
  rootSessionId: string
): boolean {
  return agentSessionId !== null && agentSessionId !== rootSessionId;
}
