# Pi subagent 按 toolResult.details 专属渲染，不走标准聚合模型

- 日期：2026-09-11
- 相关背景：`context/2026-09-11-pi-data-storage-and-subagent-extension.md`
- 状态：已采纳并实现

## 决策

Pi 来源的 subagent 不套用 OpenCode/Codex/Claude Code 的子会话聚合模型（family index → agent 标签页），改为在 Pi 消息解析层识别 `toolName == "subagent"` 的 toolResult，把 `details` 解析成结构化的 `subagent_run` 内容块；消息页改为文档流布局后，`details.results` 里**每个 run 各占一行入口**（代理名 + 状态 + 任务标题），点击打开该 run 的过程大弹窗。并行派出的多个子代理不合并成一行：实测并行场景里 6 个 run 同名（均为 `research`）但任务与成败各不相同，合并后既读不出区别也点不到单个 run。

## 理由

- Pi 的 subagent 由扩展实现（官方姿势），子代理运行 `pi --no-session`，**不产生子会话文件**：没有 session id、没有 parentSession 指向，标准聚合模型（按子会话文件聚合）没有数据源。
- 官方 session-format 契约明确 `ToolResultMessage.details` 是扩展存放"渲染与状态重建元数据"的标准通道；解析它不是 hack，是该契约的消费方。
- `details.results[].messages` 是标准 Pi AgentMessage 数组，可直接复用现有 `parse_message_entry` 解析，不需要新解析器。

## family 聚合型来源（Codex/Claude Code/OpenCode）的对齐口径

这三家的子代理是独立会话文件按 family 聚合，marker 消息（payload.type="subagent_started"，三家格式一致）声明子会话。分组器把 marker 升级为与 Pi 同款的入口行（徽章 + 标题），子会话消息按 sessionId 收拢进组；入口行与顶部子代理 tab 点击时都打开过程弹窗（SubagentGroupDialog，与 Pi 的弹窗共用 DialogShell 与行内展示组件），不使用行内展开；tab 不做时间线筛选（agent 过滤机制随之移除，时间线始终展示全部消息，"主 Agent"tab 也不再出现）。

弹窗内容由 `get_session_agent_messages` 按 agent session id 从后端取**完整**消息（`parse_agent_messages`，Pi 默认返回空），不依赖时间线分页已加载范围——弹窗入口来自 family 元数据（始终可用），若把内容绑定在分页进度上，首屏之外出现的子代理会点不开。入口行只展示标题（不显示状态与条数）：这三家的 `is_error` 读侧未填充，无法据此断言"完成/异常"，条数也只在加载完整后才准确；真实条数在弹窗里显示。

## 取舍

- 放弃跨来源统一的 subagent UI 语义：Pi 的 agent 标签页/家族时间线对 Pi 来源不出现（数据不存在）；agent tab 栏只列子代理（根会话被过滤掉），Pi 来源过滤后为空，tab 栏整体隐藏。
- 嵌套消息随块 payload 一次性下发给前端（非懒加载）：实现简单、离线数据一致；代价是含大型子代理运行的页 payload 偏大（单个 run 可达 MB 级）。subagent 工具通常每会话仅少数几次，可接受；出现性能问题时再做按需加载。
- `details` 中的原始 `messages` 数组不进 payload（由解析后的嵌套消息替代，避免双份数据），run 内其余字段（task/usage/liveActivity/stderr/exitCode 等）原样保留。`details.mode`（single/parallel/chain）不进 payload：弹窗已按 run 分区展示，模式本身无渲染消费方。
- 时间线不再提供"仅看对话"开关（该功能已废弃，后端 `filter_conversation_messages` 与前端 `isVisibleConversationItem` 同时移除）：消息页始终展示完整消息流，工具与思考已各自压缩成一行，不再需要筛选口径；子代理的完整过程不在时间线内联展开，统一放到大弹窗（DialogShell，右上角/Esc 关闭）中浏览，避免上百条嵌套消息撑爆时间线。工具调用行的通用配对逻辑见 `src/features/sessions/timeline-group.ts`，subagent 入口即其调用-结果合并的一种特例（details 替代了 tool_result 块）。
- 展示边界：子代理的中间过程只读浏览，不在 Reins 内重建子代理的 parentId 树（扩展捕获的事件流已是线性链）。
- toolResult 的 `content`（聚合报告，并行时形如 `Parallel: 5/6 succeeded` + 各 run 报告拼接）不再单独渲染：入口行按 run 拆开后它无归属，而每个 run 的结论就是它最后一条嵌套消息，已在过程里展示。搜索口径改由 run 的 task/stderr/errorMessage/结论段支撑。
- 失败原因有**两种存放位置**，弹窗两者都渲染：启动失败进 `stderr`（`exitCode` 非 0，如 agent 名写错）；运行中报错进 `errorMessage`（`exitCode` 仍为 0、`stopReason="error"`，如上游 503）。只看 `exitCode` 会把后者误判为成功。
