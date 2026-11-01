# Pi 改为 family 归族来源（pi-subagents 扩展的子会话折叠进主会话）

- 日期：2026-10-29
- 相关背景：`context/2026-09-11-pi-data-storage-and-subagent-extension.md`（旧扩展机制）、`context/2026-10-29-pi-subagents-extension-migration.md`（新扩展数据形状）
- 状态：已采纳并实现
- 替代：`decisions/2026-09-11-pi-subagent-details-rendering.md` 中"Pi 不走标准聚合模型"的结论（该路径保留用于旧扩展的历史会话）

## 决策

Pi 来源改为 family 归族形态（与 Codex/Claude Code/OpenCode 同口径）：`@tintinweb/pi-subagents` 扩展派生的子代理会话（header 带 `parentSession` 指向父会话文件、`session_info.name` 形如 `<type>#<agentId 前 8 位>`）折叠进主会话。列表只出主会话一条（标题 `(+N subagents)`），时间线为每个子代理注入 marker 入口消息，子代理会话消息按 `session_id` 收拢进组；agent 标签页与过程弹窗（`parse_agent_messages`）与其他 family 来源共用同一条前端路径。

实现形态是 bespoke backend + `FamilyIndex<PiRow>`（grokbuild 模式），不经 `ReaderEngine`：引擎的 `list_entries` 会强制产出整族摘要，而 Pi 的列表摘要必须保持按需懒加载（本机 sessions 目录 605MB，detect 阶段全文解析不可接受）。时间线与摘要缓存继续复用引擎的 `DualHalfCache` / `SummaryMemCache`。

## 归族判据

- **归属**：`header.parentSession` 是 pi 写下的父会话文件绝对路径，两边 canonicalize 后按路径对表反查父 id；父缺失（已删）的子会话保持独立条目。嵌套 spawn 沿链上溯到最近的顶层主会话（环按到达者收束）。
- **只归族扩展派生的子代理**（通用判据，2026-10-29 从"# 后缀"放宽）：子代理有 `session_info` 名（各代扩展都创建后立即 setSessionName，格式不同——新版 `<type>#<id8>`、旧版 `subagent-<agent>-<runId>-1`），而 pi 原生 fork/branch 的子会话**没有** session_info，且复制父会话全部 entry、折叠会整段重复——fork 不归族、不出 marker，保持独立条目（parent_session 事件保留，那是它们唯一的血缘表达）。索引行扫描只对带 `parentSession` 的文件读开头 32 行找 `session_info`（实测写在第 4 行），主会话与 fork 只付首行成本。已知边界：用户对 fork 手工 /rename 会被误判为子代理——极罕见且损害有限（重复内容收在 marker 组里仍可读）。
- **subagent-tasks runner 源**（旧版扩展的后台 run，`<agent 根>/subagent-tasks/<taskId>/`）：不是会话文件而是 pi `--mode json` 事件流（`task.json.sessionId` 直连父 id、`output.jsonl` 承载过程）。时间线只取 `message_end` 的最终消息（`message_start/update` 是流式增量）；摘要从 assistant 消息合成（usage 求和 + 模型集合，与子会话文件口径一致）。父缺失的 run 直接跳过——它不是会话文件，不能独立成条目。指纹把 task.json 与 output.jsonl 计入（tasks 跟 agent 根走，不在 sessions 根内）。
- **连接键验证**（2026-10-29，本机 25 个子会话）：子会话名后缀在父会话 `toolName=Agent` 的 toolResult `details.agentId` 前 8 位中唯一命中 25/25、歧义 0。该 details 不进归族链路（索引层不读全文），仅作验证记录。

## 取舍

- **fork 不归族**是本方案最重要的一条边界：family 成员集合 = 主会话 + 扩展子代理会话 + tasks run，fork 留在列表顶层。代价是 fork 与父的血缘在列表上不可见（详情页 parent_session 事件仍可见）。
- **成员 usage/模型在打开详情时才合并**（`family_usage` 逐成员走三级缓存摘要），列表阶段不付解析成本。
- **删除单位从单文件变为 family**：删主会话连全部子代理会话文件一起删；fork 不受影响。tasks run 的转录在 agent 根下，不在删除范围内（与"删除只清 sessions 根内文件"的安全边界一致）。`PiRow.member_is_root` 在归族构造 family 时回填，供 `DeletePlanInput::from_family` 折叠成员。
- 旧扩展的内嵌 `details.results[].messages` 解析路径（`parse_subagent_run_block`、`pi_entry_usages` 的 subagent 分支）原样保留，历史会话的入口行、用量统计不受影响。新扩展的 `Agent` toolResult details 无 `results` 字段，自然落回普通 tool_result 块（时间线上一行"子代理"工具行 + marker 入口，与 Claude Code 的 Task 工具行为一致）。
- **`subagent-artifacts/` 不接**：它是旧版 workflow run 的另一份记录（meta + 自定义行格式 transcript），与 `<主会话stem>/<runId>/run-0/session.jsonl`（标准会话格式、更完整）同一 runId 二选一即可，接两份会重复展示。用量统计（usage 页）同样不接 tasks run（`usage_hours` 只枚举 sessions 根内的会话文件，该口径在本次之前就如此，变更留待用量页自己提）。
