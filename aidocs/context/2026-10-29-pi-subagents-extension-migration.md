# pi-subagents 新扩展的数据形状（2026-10-29 实测）

- 背景：Pi 的 subagent 扩展从项目级自研脚本（`$HOME/projects/.pi/extensions/subagent/`，已删除）换成 npm 包 `npm:@tintinweb/pi-subagents`（`~/.pi/agent/settings.json` 的 `packages` 声明，装在 `~/.pi/agent/npm/node_modules/`）。同一扩展的两代版本并存过：旧版（工具名 `subagent`，workflow/async 模式，止于 2026-10-23）与新版 v0.22+（工具名 `Agent`，始于 2026-10-26）。切换点：旧格式 `toolName=subagent` 的会话止于 10-23，新格式 `toolName=Agent` 从 10-26 起。
- 结论用途：Reins 对 Pi 子代理数据的解析（family 归族实现见 `decisions/2026-10-29-pi-family-aggregation.md`）。

## 数据形状全景（两代版本、四个落点）

| 落点 | 形态 | 父关联 | agent 名 | Reins 处理 |
|---|---|---|---|---|
| `sessions/<cwd>/*.jsonl`（新版 `Agent` 工具） | 标准 pi 子会话文件 | header.parentSession | session_info `<type>#<id8>` | 归族（子会话成员） |
| `sessions/<cwd>/<主会话stem>/<runId>/run-0/session.jsonl`（旧版 workflow/async run） | **同上，标准 pi 子会话文件** | header.parentSession | session_info `subagent-<agent>-<runId>-1` | 归族（同一判据覆盖） |
| `sessions/<cwd>/subagent-artifacts/<runId>_<agent>_*`（旧版，与 run-0 同 run 的另一份记录） | meta.json + 自定义行 transcript | meta.parentSessionId | meta.agent | 不接（与 run-0 二选一，避免重复） |
| `<agent 根>/subagent-tasks/<taskId>/`（旧版 tasks runner） | task.json + output.jsonl（pi `--mode json` 事件流）+ result.md | task.json.sessionId | task.json.agent | 归族（run 成员：时间线取 `message_end`，摘要从 assistant 消息合成） |

通用不变量：**每个 run 都有"指向父会话的 id/路径 + agent 名 + 过程转录"**；会话形态的产物都带 `session_info` 名，而 fork 子会话没有——「parentSession + session_info 有名」即扩展派生的通用判据，不需要逐家认名字格式。

## 扩展注册的工具

新版 `SUBAGENT_TOOL_NAMES`（src/agent-runner.ts）：`Agent`（派发）、`SubagentWorkflow`、`get_subagent_result`（取后台结果）、`steer_subagent`。旧版工具名为 `subagent`（workflow 模式 + 异步 run）。工具结果与最老的"内嵌 details"扩展不同源。

## 旧版扩展的其余落点（Reins 不依赖）

- `$TMPDIR/pi-subagents-uid-501/async-subagent-runs/<runId>/`（临时目录，易失）；`details.asyncDir` 指向它
- `sessions/<cwd>/subagent-artifacts/` 的 transcript 行格式：`{version:1, recordType:"message"|"tool_start"|"tool_end", source:"async", runId, agent, childIndex, ts(毫秒), message:{标准 pi AgentMessage}}`
- `subagent-tasks/<id>/output.jsonl` 事件流：`message_start/message_update/message_end`（只有 end 是最终版）、`turn_start/end`、`agent_end/agent_settled`；assistant 消息带 `usage`/`model`/`timestamp`（毫秒）
- 另有最老一代"内嵌 details"扩展（`toolName=subagent` + `details.results[].messages`），其解析路径保留在 pi.rs（历史会话仍走它）

## 主会话内的工具结果形状（新版）

`toolName=Agent` 的 toolResult：

- `details`：`{ displayName, description, subagentType, modelName, tags, toolUses, tokens, durationMs, status, agentId }`——**没有 messages、没有 usage 数字**
- `content`：后台运行时只是一句 `Agent started in background. Agent ID: <id> ...`；最终报告要等另一个工具 `get_subagent_result` 的结果（其 details 为 null、正文是纯文本报告）
- 前台运行（`run_in_background: false`）时 details.status 为 completed/error，content 是完整输出

## 关联键（归族依据的验证记录）

- 子会话 `parentSession` → 父会话文件路径 → 父 header id：路径 canonicalize 后对表
- 子会话 `session_info.name` 后缀 ↔ 父会话 `details.agentId` 前 8 位：本机 25 个子会话唯一命中 25/25、歧义 0（2026-10-29）
- tasks run：`task.json.sessionId` 直连父 id（本机 9 个 run，父会话 5 个）
- 子会话最后一条 assistant 消息带完整 `usage`/`model`/`stopReason`（家族用量合并的数据来源）
