import { describe, expect, it } from "vitest";
import {
  buildTimelineItems,
  itemSearchText,
  type TimelineItem
} from "./timeline-group";
import type { ContentBlock, SessionMessage } from "./types";

function message(id: string, role: string, blocks: ContentBlock[]): SessionMessage {
  return { id, role, timestamp: 1, sessionId: "root", blocks };
}

function findItems(items: TimelineItem[], kind: TimelineItem["kind"]): TimelineItem[] {
  return items.filter((item) => item.kind === kind);
}

describe("buildTimelineItems", () => {
  it("merges pi-style separate call and result messages into one tool row", () => {
    const items = buildTimelineItems([
      message("m1", "user", [{ kind: "text", text: "帮我查", toolName: null, toolCallId: null, payload: null }]),
      message("m2", "assistant", [
        { kind: "thinking", text: "先看目录", toolName: null, toolCallId: null, payload: null },
        { kind: "tool_use", text: null, toolName: "bash", toolCallId: "call-1", payload: { type: "toolCall", id: "call-1", name: "bash", arguments: { command: "ls -la" } } }
      ]),
      message("m3", "toolResult", [
        { kind: "tool_result", text: "total 0", toolName: "bash", toolCallId: "call-1", isError: false, payload: null }
      ]),
      message("m4", "assistant", [{ kind: "output_text", text: "目录是空的", toolName: null, toolCallId: null, payload: null }])
    ]);

    const tools = findItems(items, "tool") as Extract<TimelineItem, { kind: "tool" }>[];
    expect(tools).toHaveLength(1);
    expect(tools[0].summary).toBe("运行命令 · ls -la");
    expect(tools[0].status).toBe("ok");
    expect(tools[0].detailText).toContain("ls -la");
    expect(tools[0].detailText).toContain("total 0");

    const texts = findItems(items, "text");
    expect(texts).toHaveLength(2);
    const thinking = findItems(items, "collapsed-block");
    expect(thinking).toHaveLength(1);
  });

  it("pairs claude-code style results carried by user-role messages", () => {
    const items = buildTimelineItems([
      message("c1", "assistant", [
        { kind: "tool_use", text: null, toolName: "Read", toolCallId: "tu_1", payload: { type: "tool_use", id: "tu_1", name: "Read", input: { file_path: "/tmp/a.py" } } }
      ]),
      message("c2", "user", [
        { kind: "tool_result", text: "file content", toolName: null, toolCallId: "tu_1", payload: { type: "tool_result", tool_use_id: "tu_1", is_error: true } }
      ])
    ]);

    const tools = findItems(items, "tool") as Extract<TimelineItem, { kind: "tool" }>[];
    expect(tools).toHaveLength(1);
    expect(tools[0].summary).toBe("已读取 · /tmp/a.py");
    expect(tools[0].status).toBe("error");
    // user 角色消息里的 tool_result 不应生成用户气泡
    expect(findItems(items, "text")).toHaveLength(0);
  });

  it("merges opencode style call and output blocks inside one message", () => {
    const items = buildTimelineItems([
      message("o1", "assistant", [
        { kind: "function_call", text: "args", toolName: "bash", toolCallId: "oc_1", payload: { type: "tool", tool: "bash", callID: "oc_1", input: { command: "pwd" } } },
        { kind: "function_call_output", text: "/home", toolName: "bash", toolCallId: "oc_1", payload: { type: "tool", tool: "bash", callID: "oc_1", output: "/home" } }
      ])
    ]);

    const tools = findItems(items, "tool") as Extract<TimelineItem, { kind: "tool" }>[];
    expect(tools).toHaveLength(1);
    expect(tools[0].status).toBe("ok");
    expect(tools[0].detailText).toContain("/home");
  });

  it("inherits tool name for codex style orphan outputs and keeps unmatched calls", () => {
    // 分页边界：调用在上一页，结果先到 → 单独成行
    const orphan = buildTimelineItems([
      message("x1", "tool", [
        { kind: "function_call_output", text: "done", toolName: null, toolCallId: "call_9", payload: { type: "function_call_output", call_id: "call_9", output: "done" } }
      ])
    ]);
    const orphanRows = findItems(orphan, "tool") as Extract<TimelineItem, { kind: "tool" }>[];
    expect(orphanRows).toHaveLength(1);
    expect(orphanRows[0].status).toBe("ok");

    // 调用没有结果（中断）→ 保留为无结果行
    const unmatched = buildTimelineItems([
      message("x2", "tool", [
        { kind: "function_call", text: null, toolName: "bash", toolCallId: "call_10", payload: { type: "function_call", name: "bash", call_id: "call_10", arguments: "{\"command\":\"rm -rf /\"}" } }
      ])
    ]);
    const rows = findItems(unmatched, "tool") as Extract<TimelineItem, { kind: "tool" }>[];
    expect(rows).toHaveLength(1);
    expect(rows[0].status).toBe("no-result");
    expect(rows[0].summary).toBe("运行命令 · rm -rf /");
  });

  it("renders pi subagent results as entries and suppresses their tool rows", () => {
    const items = buildTimelineItems([
      message("s1", "assistant", [
        { kind: "tool_use", text: null, toolName: "subagent", toolCallId: "call-sub", payload: { type: "toolCall", id: "call-sub", name: "subagent", arguments: { agent: "scout" } } }
      ]),
      message("s2", "toolResult", [
        {
          kind: "subagent_run",
          text: "报告",
          toolName: "subagent",
          toolCallId: "call-sub",
          payload: {
            mode: "single",
            runs: [{ agent: "scout", agentSource: "project", task: "梳理代码库\n第二步", exitCode: 0, stopReason: "stop", nestedMessages: [] }]
          }
        }
      ])
    ]);

    expect(findItems(items, "tool")).toHaveLength(0);
    const entries = findItems(items, "subagent") as Extract<TimelineItem, { kind: "subagent" }>[];
    expect(entries).toHaveLength(1);
    expect(entries[0].label).toBe("scout");
    expect(entries[0].status).toBe("ok");
    expect(entries[0].title).toBe("梳理代码库");
  });

  it("gives every parallel subagent run its own entry row", () => {
    // 并行派 6 个同名 research：合并成一行会丢失各自的任务与成败
    const entries = (runs: Array<Record<string, unknown>>) => {
      const items = buildTimelineItems([
        message("s1", "toolResult", [
          {
            kind: "subagent_run",
            text: "Parallel: 5/6 succeeded",
            toolName: "subagent",
            toolCallId: "call-sub",
            payload: { runs }
          }
        ])
      ]);
      return findItems(items, "subagent") as Extract<
        TimelineItem,
        { kind: "subagent" }
      >[];
    };

    const parallel = entries(
      Array.from({ length: 6 }, (_, index) => ({
        agent: "research",
        task: `只读研究任务 ${index}`,
        // 第 4 个跑挂：行与行之间的状态必须独立
        exitCode: index === 3 ? 1 : 0,
        nestedMessages: []
      }))
    );

    expect(parallel).toHaveLength(6);
    expect(parallel.map((entry) => entry.title)).toEqual([
      "只读研究任务 0",
      "只读研究任务 1",
      "只读研究任务 2",
      "只读研究任务 3",
      "只读研究任务 4",
      "只读研究任务 5"
    ]);
    expect(parallel.map((entry) => entry.status)).toEqual([
      "ok",
      "ok",
      "ok",
      "error",
      "ok",
      "ok"
    ]);
    // 同名 agent 并行时 key 必须仍然唯一
    expect(new Set(parallel.map((entry) => entry.key)).size).toBe(6);
    // 每行只携带自己那个 run，弹窗不会串到别的
    expect(parallel[3].run.task).toBe("只读研究任务 3");

    const single = entries([
      { agent: "scout", task: "梳理代码库", nestedMessages: [] }
    ]);
    expect(single).toHaveLength(1);
    expect(single[0].label).toBe("scout");
    expect(single[0].title).toBe("梳理代码库");
  });

  it("keeps a run searchable by task, failure reason and conclusion", () => {
    // 入口行不再携带聚合报告文本，搜索口径改由 run 自己的字段支撑
    const items = buildTimelineItems([
      message("s1", "toolResult", [
        {
          kind: "subagent_run",
          text: "聚合报告",
          toolName: "subagent",
          toolCallId: "call-sub",
          payload: {
            runs: [
              {
                agent: "scout",
                task: "核对物业类型字段",
                stopReason: "error",
                errorMessage: "OpenAI API error (503)",
                nestedMessages: [
                  {
                    id: "n1",
                    role: "assistant",
                    timestamp: 1,
                    sessionId: "root",
                    blocks: [
                      {
                        kind: "output_text",
                        text: "最终结论段落",
                        toolName: null,
                        toolCallId: null,
                        payload: null
                      }
                    ]
                  }
                ]
              }
            ]
          }
        }
      ])
    ]);

    const text = itemSearchText(findItems(items, "subagent")[0]);
    expect(text).toContain("核对物业类型字段");
    expect(text).toContain("503");
    expect(text).toContain("最终结论段落");
  });

  it("marks a pi subagent entry failed on any non-zero exit code", () => {
    // 实测退出码取 0/1/127：启动失败（如 Unknown agent）不会带 stopReason=error，
    // 只能从 exitCode 判定，否则入口行会把失败运行显示成"完成"
    const entryFor = (run: Record<string, unknown>) => {
      const items = buildTimelineItems([
        message("s1", "toolResult", [
          {
            kind: "subagent_run",
            text: "报告",
            toolName: "subagent",
            toolCallId: "call-sub",
            payload: { mode: "single", runs: [run] }
          }
        ])
      ]);
      return findItems(items, "subagent")[0] as Extract<
        TimelineItem,
        { kind: "subagent" }
      >;
    };

    expect(
      entryFor({
        agent: "general",
        task: "评审变更",
        exitCode: 1,
        stderr: 'Unknown agent: "general".',
        nestedMessages: []
      }).status
    ).toBe("error");
    expect(entryFor({ agent: "scout", exitCode: 127, nestedMessages: [] }).status).toBe(
      "error"
    );
    // 没有 exitCode 字段时不能凭空判失败
    expect(entryFor({ agent: "scout", nestedMessages: [] }).status).toBe("ok");
  });

  it("expands a pi subagent nested stream into every row, not just first blocks", () => {
    // 过程弹窗改为把 nestedMessages 交给同一个分组器：一条 assistant 里的
    // thinking 与多个 toolCall 都要成行，旧实现每条只取首块、工具参数全丢
    const nested = [
      message("e-r0-m0", "user", [
        { kind: "text", text: "树理代码库", toolName: null, toolCallId: null, payload: null }
      ]),
      message("e-r0-m1", "assistant", [
        { kind: "thinking", text: "先列目录", toolName: null, toolCallId: null, payload: null },
        { kind: "tool_use", text: null, toolName: "bash", toolCallId: "n1", payload: { arguments: { command: "ls src" } } },
        { kind: "tool_use", text: null, toolName: "bash", toolCallId: "n2", payload: { arguments: { command: "wc -l src/*" } } }
      ]),
      message("e-r0-m2", "toolResult", [
        { kind: "tool_result", text: "api.ts", toolName: "bash", toolCallId: "n1", payload: null }
      ]),
      message("e-r0-m3", "toolResult", [
        { kind: "tool_result", text: "120 total", toolName: "bash", toolCallId: "n2", payload: null }
      ]),
      message("e-r0-m4", "assistant", [
        { kind: "output_text", text: "结论", toolName: null, toolCallId: null, payload: null }
      ])
    ];

    const items = buildTimelineItems(nested);
    expect(items.map((item) => item.kind)).toEqual([
      "text",
      "collapsed-block",
      "tool",
      "tool",
      "text"
    ]);

    const tools = findItems(items, "tool") as Extract<TimelineItem, { kind: "tool" }>[];
    // 第二个 toolCall 以前完全不可见，且参数与结果都要能展开
    expect(tools[1].summary).toBe("运行命令 · wc -l src/*");
    expect(tools[1].detailText).toContain("120 total");
  });

  it("groups family-style subagent sessions under an entry row", () => {
    // Claude Code / OpenCode / Codex 的 family 聚合形态：
    // marker 消息声明子会话，子消息按 sessionId 归属（时间戳上与根消息交错）
    const rootMsg = message("r1", "user", [
      { kind: "text", text: "去做调研", toolName: null, toolCallId: null, payload: null }
    ]);
    const marker = message("mk1", "assistant", [
      {
        kind: "output_text",
        text: "Sub-agent session: research\nchild-1",
        toolName: null,
        toolCallId: null,
        payload: { type: "subagent_started", session_id: "child-1", title: "Research best practices" }
      }
    ]);
    marker.sessionId = "child-1";
    const childToolCall = message("ch0", "assistant", [
      { kind: "tool_use", text: null, toolName: "WebFetch", toolCallId: "w1", payload: { name: "WebFetch", id: "w1", input: { url: "https://example.com" } } }
    ]);
    childToolCall.sessionId = "child-1";
    const childMsg = message("ch1", "assistant", [
      { kind: "output_text", text: "调研结论", toolName: null, toolCallId: null, payload: null }
    ]);
    childMsg.sessionId = "child-1";

    const items = buildTimelineItems([rootMsg, marker, childToolCall, childMsg]);
    expect(items.map((item) => item.kind)).toEqual(["text", "subagent-group"]);

    const group = items[1] as Extract<TimelineItem, { kind: "subagent-group" }>;
    expect(group.label).toBe("Research best practices");
    expect(group.sessionId).toBe("child-1");
    // marker 的说明文本不作为正文渲染，子消息按时间序归入组内
    expect(group.items.map((item) => item.kind)).toEqual(["tool", "text"]);
    // 组内全文进搜索索引
    expect(itemSearchText(group)).toContain("调研结论");
  });

  it("indexes collapsed tool output into search text", () => {
    const items = buildTimelineItems([
      message("m2", "assistant", [
        { kind: "tool_use", text: null, toolName: "bash", toolCallId: "c1", payload: { arguments: { command: "npm test" } } }
      ]),
      message("m3", "toolResult", [
        { kind: "tool_result", text: "ALL TESTS PASSED", toolName: "bash", toolCallId: "c1", payload: null }
      ])
    ]);
    const tool = findItems(items, "tool")[0] as Extract<TimelineItem, { kind: "tool" }>;
    expect(itemSearchText(tool)).toContain("all tests passed");
  });
});
