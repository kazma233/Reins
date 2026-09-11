import type { SessionMessage } from "./types";
import {
  isFailedSubagentRun,
  parseSubagentRunPayload,
  type SubagentRun
} from "./subagent-payload";

// 文档流时间线的分组器：把各来源 backend 归一化后的 SessionMessage 流
// 重排成"文本 / 工具行 / 思考行 / 子代理入口"的渲染单元。
// 工具配对建立在块级别（tool_call_id）上，兼容四种来源的差异：
// - Pi / Claude Code：调用在 assistant 消息，结果在独立消息（Claude 结果挂在 user 角色上）
// - Codex：调用与结果都是 role="tool" 的消息，且结果块没有 tool_name（从配对调用继承）
// - OpenCode：调用与结果在同一条 assistant 消息里（callID 相同），可能只有调用（运行中）

export const TOOL_CALL_KINDS = new Set(["tool_use", "function_call"]);
export const TOOL_RESULT_KINDS = new Set(["tool_result", "function_call_output"]);
// 对话正文块：既是分组器的文本段来源，也是唯一走 markdown 渲染的块
export const CONVERSATION_TEXT_KINDS = new Set([
  "text",
  "input_text",
  "output_text"
]);

// 其余块（工具输出、思考原文等）按原文展示
export function isMarkdownBlock(kind: string): boolean {
  return CONVERSATION_TEXT_KINDS.has(kind);
}

export type ToolRowStatus = "ok" | "error" | "no-result";

export type ToolRowItem = {
  kind: "tool";
  key: string;
  toolName: string;
  summary: string;
  detailText: string | null;
  status: ToolRowStatus;
};

// 一个 run 一行：并行派出的多个子代理各自有任务与成败，合并成一行会丢失这些差异
export type SubagentEntryItem = {
  kind: "subagent";
  key: string;
  label: string;
  status: "ok" | "error";
  title: string | null;
  run: SubagentRun;
};

// family 聚合型来源（Codex/Claude Code/OpenCode）的子代理：独立会话文件按
// sessionId 收拢成一组，主时间线里只留入口行，内容由弹窗按需取全量。
// items 仅保留已加载部分的索引用途（搜索），不作为弹窗数据来源。
export type SubagentGroupItem = {
  kind: "subagent-group";
  key: string;
  label: string;
  sessionId: string;
  items: TimelineItem[];
};

export type CollapsedBlockItem = {
  kind: "collapsed-block";
  key: string;
  text: string;
  // 行首标签：思考块显示"思考"，其余杂项块显示块类型
  label: string;
  // 思考类内容用 markdown 渲染，杂项块按原文展示
  markdown: boolean;
};

export type TextItem = {
  kind: "text";
  key: string;
  role: "user" | "assistant" | "other";
  message: SessionMessage;
  blocks: SessionMessage["blocks"];
};

export type TimelineItem =
  | TextItem
  | ToolRowItem
  | SubagentEntryItem
  | SubagentGroupItem
  | CollapsedBlockItem;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function stringifyValue(value: unknown): string | null {
  if (typeof value === "string") {
    return value;
  }
  if (value === null || value === undefined) {
    return null;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function firstLine(text: string, maxLength = 80): string {
  const line = text.split("\n").find((part) => part.trim().length > 0) ?? "";
  const trimmed = line.trim();
  return trimmed.length > maxLength ? `${trimmed.slice(0, maxLength)}…` : trimmed;
}

// 工具调用参数里常见的"对象"字段：摘要行尽量显示操作对象而不是原始 JSON
const ARGUMENT_OBJECT_KEYS = [
  "command",
  "file_path",
  "path",
  "filename",
  "url",
  "query",
  "pattern",
  "description",
  "prompt"
];

function argumentObject(args: unknown): string | null {
  if (typeof args === "string") {
    // Codex 的 arguments 是 JSON 字符串，先解一层
    try {
      return argumentObject(JSON.parse(args));
    } catch {
      return firstLine(args);
    }
  }
  if (!isRecord(args)) {
    return null;
  }
  for (const key of ARGUMENT_OBJECT_KEYS) {
    const value = args[key];
    if (typeof value === "string" && value.trim().length > 0) {
      return firstLine(value);
    }
  }
  return firstLine(JSON.stringify(args));
}

function argumentTextFromBlock(block: SessionMessage["blocks"][number]): string | null {
  if (!isRecord(block.payload)) {
    return null;
  }
  return argumentObject(block.payload.arguments ?? block.payload.input ?? null);
}

function callInputText(block: SessionMessage["blocks"][number]): string | null {
  if (block.text) {
    return block.text;
  }
  if (!isRecord(block.payload)) {
    return null;
  }
  const payload = block.payload;
  return (
    stringifyValue(payload.arguments) ??
    stringifyValue(payload.input) ??
    stringifyValue(payload.state)
  );
}

function resultOutputText(block: SessionMessage["blocks"][number]): string | null {
  if (block.text) {
    return block.text;
  }
  if (!isRecord(block.payload)) {
    return null;
  }
  const payload = block.payload;
  const output =
    payload.output ??
    (isRecord(payload.state) ? payload.state.output : undefined) ??
    payload.content;
  return stringifyValue(output);
}

function resultIsError(block: SessionMessage["blocks"][number]): boolean {
  if (block.isError === true) {
    return true;
  }
  return isRecord(block.payload) && block.payload.is_error === true;
}

// 摘要动词按工具名映射；未知道具回退到工具名本身
const TOOL_VERBS: Array<[RegExp, string]> = [
  [/^(read|view|cat)$/i, "已读取"],
  [/^(bash|shell|terminal|exec|command)$/i, "运行命令"],
  [/^(edit|write|apply|str_replace|save)$/i, "已编辑"],
  [/^(grep|search|rg|find)$/i, "已搜索"],
  [/^(ls|list|glob|tree)$/i, "已列出"],
  [/^(web|websearch|search_web|fetch|browser)$/i, "已检索网页"],
  [/^(todo|todowrite|plan)$/i, "更新计划"],
  [/^(subagent|task|agent)$/i, "子代理"]
];

function toolVerb(toolName: string): string {
  for (const [pattern, verb] of TOOL_VERBS) {
    if (pattern.test(toolName)) {
      return verb;
    }
  }
  return toolName;
}

function toolRowSummary(toolName: string, objectText: string | null): string {
  const verb = toolVerb(toolName);
  return objectText ? `${verb} · ${objectText}` : verb;
}

function makeToolRow(
  message: SessionMessage,
  blockIndex: number,
  block: SessionMessage["blocks"][number],
  toolName: string
): ToolRowItem {
  return {
    kind: "tool",
    key: `${message.id}:${blockIndex}`,
    toolName,
    summary: toolRowSummary(toolName, argumentTextFromBlock(block)),
    detailText: callInputText(block),
    status: "no-result"
  };
}

function applyResultToRow(row: ToolRowItem, block: SessionMessage["blocks"][number]) {
  const outputText = resultOutputText(block);
  row.status = resultIsError(block) ? "error" : "ok";
  row.detailText = [row.detailText, outputText].filter(Boolean).join("\n\n─── 结果 ───\n\n");
}

function subagentTitle(run: SubagentRun): string | null {
  const task = run.task;
  if (task === undefined || task.trim().length === 0) {
    return null;
  }
  return firstLine(task, 60);
}

export function buildTimelineItems(messages: SessionMessage[]): TimelineItem[] {
  // 子代理入口即 subagent 工具的"结果"（Pi 用 details 替代了 tool_result 块），
  // 记录其 callId，让对应的调用块不再单独出行。
  // 调用消息在入口消息之前出现，所以先全量扫一遍再走主循环。
  const subagentCallIds = new Set<string>();
  for (const message of messages) {
    for (const block of message.blocks) {
      if (block.kind === "subagent_run" && block.toolCallId) {
        subagentCallIds.add(block.toolCallId);
      }
    }
  }

  // family 聚合型来源：marker 消息声明一个子代理会话，其后同 sessionId 的
  // 消息都归属该子代理，收拢成组（按时间戳排序时会与根会话消息交错）
  const markers = new Map<string, { label: string; sessionId: string }>();
  for (const message of messages) {
    const marker = markerInfo(message);
    if (marker) {
      markers.set(message.id, marker);
    }
  }
  const childSessionIds = new Set(
    [...markers.values()].map((marker) => marker.sessionId)
  );
  const buckets = new Map<string, SessionMessage[]>();
  const mainFlow: SessionMessage[] = [];
  for (const message of messages) {
    if (markers.has(message.id)) {
      mainFlow.push(message);
      continue;
    }
    if (message.sessionId && childSessionIds.has(message.sessionId)) {
      const bucket = buckets.get(message.sessionId) ?? [];
      bucket.push(message);
      buckets.set(message.sessionId, bucket);
      continue;
    }
    mainFlow.push(message);
  }

  const items: TimelineItem[] = [];
  const pendingCalls = new Map<string, ToolRowItem>();
  for (const message of mainFlow) {
    const marker = markers.get(message.id);
    if (marker) {
      const bucket = buckets.get(marker.sessionId) ?? [];
      const inner: TimelineItem[] = [];
      const bucketPending = new Map<string, ToolRowItem>();
      for (const bucketMessage of bucket) {
        processMessage(bucketMessage, inner, subagentCallIds, bucketPending);
      }
      items.push({
        kind: "subagent-group",
        key: `${message.id}:group`,
        label: marker.label,
        sessionId: marker.sessionId,
        items: inner
      });
      continue;
    }
    processMessage(message, items, subagentCallIds, pendingCalls);
  }

  return items;
}

// 单条消息 → 渲染单元；调用-结果配对依赖调用方传入的 pendingCalls（跨消息共享）
function processMessage(
  message: SessionMessage,
  items: TimelineItem[],
  subagentCallIds: Set<string>,
  pendingCalls: Map<string, ToolRowItem>
) {
  if (message.blocks.some(isSubagentRunBlock)) {
    pushSubagentEntry(items, message);
    return;
  }

  if (markerInfo(message)) {
    // 主流程里不会被走到（marker 已被分组消费），组内也不会出现
    return;
  }

  let textBlocks: SessionMessage["blocks"] = [];

  const flushText = () => {
    if (textBlocks.length === 0) {
      return;
    }
    items.push({
      kind: "text",
      key: `${message.id}:text-${items.length}`,
      role:
        message.role === "user"
          ? "user"
          : message.role === "assistant"
            ? "assistant"
            : "other",
      message,
      blocks: textBlocks
    });
    textBlocks = [];
  };

  message.blocks.forEach((block, blockIndex) => {
    if (CONVERSATION_TEXT_KINDS.has(block.kind)) {
      textBlocks.push(block);
      return;
    }

    if (block.kind === "thinking") {
      flushText();
      if (block.text) {
        items.push({
          kind: "collapsed-block",
          key: `${message.id}:${blockIndex}`,
          text: block.text,
          label: "思考",
          markdown: true
        });
      }
      return;
    }

    if (TOOL_CALL_KINDS.has(block.kind)) {
      flushText();
      const toolName = block.toolName ?? "tool";
      const callId = block.toolCallId;
      if (callId && subagentCallIds.has(callId)) {
        // 子代理入口已代表这次调用，不重复出工具行
        return;
      }
      const row = makeToolRow(message, blockIndex, block, toolName);
      items.push(row);
      if (callId) {
        pendingCalls.set(callId, row);
      }
      return;
    }

    if (TOOL_RESULT_KINDS.has(block.kind)) {
      flushText();
      const callId = block.toolCallId;
      const pending = callId ? pendingCalls.get(callId) : undefined;
      if (pending) {
        applyResultToRow(pending, block);
        if (callId) {
          pendingCalls.delete(callId);
        }
      } else {
        // 分页边界或孤儿结果：单独成行，工具名缺失时用占位
        const toolName = block.toolName ?? "tool";
        items.push({
          kind: "tool",
          key: `${message.id}:${blockIndex}`,
          toolName,
          summary: toolRowSummary(toolName, null),
          detailText: resultOutputText(block),
          status: resultIsError(block) ? "error" : "ok"
        });
      }
      return;
    }

    // 其余块（compactionSummary、patch、file、unsupported…）按原文折叠行渲染
    flushText();
    const text = block.text ?? stringifyValue(block.payload);
    if (text) {
      items.push({
        kind: "collapsed-block",
        key: `${message.id}:${blockIndex}`,
        text,
        label: block.kind,
        markdown: false
      });
    }
  });

  flushText();
}

function isSubagentRunBlock(block: SessionMessage["blocks"][number]): boolean {
  return block.kind === "subagent_run";
}

function pushSubagentEntry(
  items: TimelineItem[],
  message: SessionMessage
) {
  message.blocks
    .filter(isSubagentRunBlock)
    .forEach((block, index) => {
      const runs = parseSubagentRunPayload(block.payload)?.runs ?? [];
      runs.forEach((run, runIndex) => {
        items.push({
          kind: "subagent",
          key: `${message.id}:subagent-${index}-${runIndex}`,
          label: run.agent ?? "子代理",
          status: isFailedSubagentRun(run) ? "error" : "ok",
          title: subagentTitle(run),
          run
        });
      });
    });
}

// family 型子代理的 marker 消息：三家 backend 格式一致
// （role=assistant + payload.type="subagent_started" + title + session_id）
function markerInfo(message: SessionMessage): {
  label: string;
  sessionId: string;
} | null {
  for (const block of message.blocks) {
    if (!isRecord(block.payload)) {
      continue;
    }
    if (block.payload.type !== "subagent_started") {
      continue;
    }
    const sessionId =
      typeof block.payload.session_id === "string"
        ? block.payload.session_id
        : message.sessionId;
    if (!sessionId) {
      continue;
    }
    const title =
      typeof block.payload.title === "string" && block.payload.title.trim()
        ? block.payload.title
        : "子代理会话";
    return { label: title, sessionId };
  }
  return null;
}

// --- 检索与过滤辅助 ---

export function itemSearchText(item: TimelineItem): string {
  const parts: string[] = [item.key];
  switch (item.kind) {
    case "text":
      parts.push(item.role, ...item.blocks.map((block) => block.text ?? ""));
      break;
    case "tool":
      parts.push(item.toolName, item.summary, item.detailText ?? "");
      break;
    case "collapsed-block":
      parts.push(item.text);
      break;
    case "subagent": {
      const { nestedMessages, task, stderr, errorMessage } = item.run;
      // 最后一条嵌套消息即该 run 的结论，按原来的报告口径继续可搜
      const conclusion = (nestedMessages[nestedMessages.length - 1]?.blocks ?? [])
        .map((block) => block.text ?? "")
        .join("\n");
      parts.push(
        item.label,
        item.title ?? "",
        task ?? "",
        stderr ?? "",
        errorMessage ?? "",
        conclusion
      );
      break;
    }
    case "subagent-group":
      parts.push(item.label, ...item.items.map(itemSearchText));
      break;
  }
  return parts
    .map((part) => part.trim().toLocaleLowerCase("zh-CN"))
    .filter((part) => part.length > 0)
    .join("\n");
}

