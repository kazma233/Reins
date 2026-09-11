import type {
  SessionAgent,
  SessionEvent,
  SessionMessage
} from "../types";

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

// --- block-kind labels ---

// --- stable keys ---

export function eventStableKey(event: SessionEvent): string {
  return `${eventSessionId(event) ?? event.sessionId ?? "root"}:${event.id}`;
}

// --- event timeline preparation ---

export type PreparedTimelineEvent = {
  key: string;
  event: SessionEvent;
  agentSessionId: string | null;
  isSubagentLifecycle: boolean;
  payloadText: string | null;
  searchText: string;
};

export function sessionIdFromPayload(value: unknown): string | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  const sessionId = Reflect.get(value, "session_id");
  return typeof sessionId === "string" ? sessionId : null;
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

// 文档流时间线：visible/total 均为渲染单元（文本段 + 工具行 + 思考行…）数
export function formatVisibleMessageLabel(
  visibleItemCount: number,
  totalItemCount: number,
  timelineFilter: string
): string {
  if (timelineFilter.length > 0) {
    return `匹配 ${visibleItemCount} / ${totalItemCount} 条消息`;
  }

  return `已显示 ${visibleItemCount} 条消息`;
}

export function formatVisibleEventLabel(
  filteredEventCount: number,
  nextEventOffset: number | null,
  timelineFilter: string
): string {
  if (timelineFilter.length > 0) {
    return `匹配 ${filteredEventCount} 条事件`;
  }

  if (filteredEventCount === 0 && nextEventOffset === 0) {
    return `未加载事件`;
  }

  return `已显示 ${filteredEventCount} 条事件`;
}

export function emptyMessageText(timelineFilter: string): string {
  if (timelineFilter.length > 0) {
    return `没有匹配“${timelineFilter}”的消息。`;
  }

  return `当前暂无消息。`;
}

export function emptyEventText(timelineFilter: string): string {
  if (timelineFilter.length > 0) {
    return `没有匹配“${timelineFilter}”的事件。`;
  }
  return `当前暂无事件。`;
}

export function shouldShowEventAgentChip(
  agentSessionId: string | null,
  rootSessionId: string
): boolean {
  return agentSessionId !== null && agentSessionId !== rootSessionId;
}
