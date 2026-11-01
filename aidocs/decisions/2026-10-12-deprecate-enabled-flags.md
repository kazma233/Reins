# 废弃 target / MCP 的启停开关

- 日期：2026-10-12
- 状态：已实施（决策来自用户；实现覆盖 agents / workspace / providers 契约面与前端）

## 背景

「启用 / 停用」这一层概念在配置里存在三处（`targets.*.enabled`、`targets.*.agents.*.enabled`、`mcps[].enabled`），但三处都不是真正的开关：

- **target 停用**：只在分发侧生效——安装面跳过、MCP 应用直接报错「目标已禁用」，inspect 显示一个 `disabled` 状态。用户要的「先别同步到这个工具」其实是「删除线以上全部行为」的近似表达。
- **MCP 停用**：前端把 `enabled: false` 解释为「从所有已安装目标里删掉条目」（`useMcpMutations.ts` 的同步循环），不是写入 `enabled=false`；卡片上也从不显示这个状态。工具侧只有 codex / grok / opencode 会收到 `enabled` 键，claude 与 dsh 静默丢弃。
- **项目 agent 停用**：无 UI 入口，值是常量 `true`。

结论：这是一层没有真实语义、却让四条路径各自分叉的状态。用户决定废弃。

## 决策

1. **删除全部三个 `enabled` 字段**（视图、输入、存储、解析五类结构），`TargetSkip` 只剩「目标不存在」与「项目级目录继承」两种原因；inspect 的 target/MCP 状态里不再有 `disabled`。
2. **旧配置的键被忽略**：`RawTargetConfig` / `RawMcpConfig` / `RawProjectAgentConfig` 不再声明该字段，serde 默认忽略未知键——旧 `config.yaml` 照常加载、不报错、不提示；重新序列化后该键自然消失。`RawProjectAgentConfig` 保留为空占位结构，使项目成员在 YAML 里写作 `codex: {}`（`()` 会序列化成 `null`，与手工编辑的直觉不符）。
3. **不再写进工具配置**：TOML（codex / grok）与 OpenCode JSON 的 `enabled` 键不再写入，也不再有该键的形态示例；工具侧缺省即启用。
4. **前端删除交互**：target 卡片与同步弹窗的「停用 / 启用」按钮、`已停用 / 启用 / 禁用` 状态 pill、启停队列与 `pendingToggleTargetIds`、MCP 弹窗的「启用」勾选框全部移除。

## 语义变化（有意为之）

- 旧配置里 `enabled: false` 的 target / MCP / 项目 agent，升级后**重新参与分发、同步与应用**。
- 旧配置里 target 级 `enabled: false` 的 MCP 关联此前表现为「被删掉」，现在同样会参与。

## 证据与验证

- 兼容与语义：`tests/workspace.rs::legacy_enabled_key_is_ignored_and_never_disables` —— 旧配置带 `enabled: false`（target、mcp、项目 agent 三处）照常解析，线格式里不再有该字段，MCP 应用返回 `apply` 而不是此前的「目标已禁用」。
- 分发判定：`skill_install.rs` 的 `missing_targets_are_skipped` 取代原 `missing_and_disabled_targets_are_skipped`，断言 skip 只剩「不存在」一种。
- 写入面：`tests/workspace_grokbuild.rs` 断言 preview 的 TOML 里没有 `enabled` 键；OpenCode JSON 用例改名为 `opencode_mcp_entry_writes_transport_and_timeout_without_enabled`；`target_view_carries_mcp_format_example_per_config_type` 的形态示例同步去掉该键。
- 模板：`tests/agents.rs::snapshot_default_config_template` 重写为不带 `enabled` 的模板；项目示例写作 `codex: {}`。
- 前端：`vue-tsc --noEmit` 通过、`vitest run` 21 文件 / 184 用例全绿、`vite build` 通过；启停相关的 3 个测试（失败提示、卡片锁定、串行队列）随功能一起删除。
- 全量 Rust：430 passed / 0 failed。

## 未覆盖

- 降级不兼容：旧版本读到不含 `enabled` 的新配置会解析失败（旧结构该字段带默认值，但 `RawTargetConfig` 的其它字段不变，实测需旧版本验证）。已在本轮明确接受，不做双写。
- Windows 分支未在本机执行。
