# Pi 数据存储与 subagent 扩展机制

- 日期：2026-09-11
- 范围：Pi 官方文档（pi.dev/docs/latest：sessions / session-format / extensions）+ 本机实测（Pi session 01a08b37、01a08e29，扩展源码 `/Users/fanggeek/projects/.pi/extensions/subagent/`）
- 结论用途：指导 Reins 对 Pi subagent 数据的解析与展示；修正 2026-09-05 review 时"子代理过程未持久化"的误判

## 会话存储格式（官方契约 + 本机验证）

- 路径：`~/.pi/agent/sessions/<编码后cwd>/<时间戳>_<sessionId>.jsonl`（自定义 sessionDir 时按官方优先级 env > settings.json.sessionDir，落在目录根部）。
- 结构：首行 header（`type:session, version, id, timestamp, cwd, parentSession?`），后续每行一个 entry，靠 `id/parentId` 构成**树**；"当前分支" = 从最后一条 entry 沿 parentId 回溯到根的链，其余为历史分支。
- entry 类型：`message` / `custom_message` / `custom` / `compaction`（summary + firstKeptEntryId + retainedTail）/ `branch_summary` / `model_change` / `thinking_level_change` / `label` / `session_info` / `gc`。v1→v2→v3 迁移规则在官方 session-manager 中，Reins 已做内存迁移。
- `parentSession` 只表示 fork/clone/newSession(parentSession) 的**会话血缘**，值为文件路径，纯 informational；core 不维护任何父子 agent 树。不能当作 subagent 归属关系用。
- 标题：官方可存 session KV 元数据，但本机 `sessions/` 目录下没有任何 KV/sqlite 文件；实际落在文件内 `session_info` entry 的 `name` 字段。Reins 读 `session_info` 的口径正确。
- 工具结果的官方结构：`ToolResultMessage = { content, isError?, details?, usage?, addedToolNames? }`，其中：
  - `details`："Tool-specific metadata for rendering and state reconstruction"，扩展存放自有渲染/状态数据的标准通道；
  - `usage`："Nested LLM work performed by the tool"，为工具内嵌套 LLM 工作设计。

## subagent 不是 core 概念，是扩展

- Pi core 工具集只有 read/bash/edit 等最小集；subagent、MCP 等高级能力全部由 TS 扩展提供（registerTool / registerCommand / 事件 / 自定义渲染）。
- 官方扩展示例清单自带 `examples/extensions/subagent/`（Spawn sub-agents | registerTool, exec）——用扩展实现 subagent 是官方姿势。因此各 agent 工具的 subagent 数据形状互不兼容，Reins 不能假设统一的 subagent 契约。

## 本机 subagent 扩展的数据流（读源码确认）

扩展位置：`/Users/fanggeek/projects/.pi/extensions/subagent/`（项目级），agent 定义在 `/Users/fanggeek/projects/.pi/agents/*.md`（planner/reviewer/scout/worker）。

1. 扩展注册名为 `subagent` 的工具，参数 `{agent, agentScope, task, cwd, mode(single/parallel/chain), ...}`。
2. 子代理通过 `pi --mode json -p --no-session` 运行：`--no-session` **故意不写子会话文件**，扩展解析 JSON 事件流，把子代理全部消息累积进内存。
3. 最终写回主会话的 toolResult：
   - `content`：最终报告文本；
   - `details`：
     ```text
     { mode, agentScope, projectAgentsDir,
       results: [{ agent, agentSource, task, exitCode, stderr,
                   messages: [全部子代理消息，标准 Pi AgentMessage],
                   usage: {input, output, cacheRead, cacheWrite, cost, contextTokens, turns},
                   model, started, liveActivity, stopReason }] }
     ```
4. 实测（会话 01a08e29）：scout 单次运行内嵌 **144 条完整消息**（含 thinking、toolCall、toolResult、逐轮 usage、时间戳）。

**修正记录**：2026-09-05 review 与 2026-09-11 初次分析均以为"子代理过程未持久化"；实际是完整内嵌在 `toolResult.details` 里，Reins 此前只解析 `content` 数组导致 `details` 被丢弃，界面上才"看不见过程"。

## 与其他来源 subagent 的差异

- OpenCode/Codex/Claude Code 的子代理是**独立会话文件**，Reins 用 family index/timeline 聚合出 agent 标签页。
- Pi 的 subagent 是**主会话内嵌的工具结果**，没有子 session id、没有 parentSession 指向——标准聚合模型无数据可用，必须走 `details` 渲染路线（见 `decisions/2026-09-11-pi-subagent-details-rendering.md`）。
