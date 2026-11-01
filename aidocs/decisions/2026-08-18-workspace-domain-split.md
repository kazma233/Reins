# workspace/mod.rs 按域拆分为 targets / skills / mcps 子模块

- 日期：2026-08-18
- 状态：生效中

## 背景

config 守门人批次（42ce030）之后，`workspace/mod.rs` 仍有约 3341 行，targets / skills / mcps 三个业务域的函数与公共底座（路径解析、fs 原语、normalize helpers）混在一个文件里，改任何一个域都要在巨文件里定位，review diff 噪音大。

## 决策

按业务域拆分为同目录子模块，**纯移动、零逻辑改动**：

| 文件 | 职责 |
|---|---|
| `workspace/targets.rs` | target/project 的 config.yaml CRUD、builtin per-agent 路径默认值、directory-link skill_dir 检测 |
| `workspace/skills.rs` | skill 发现（local/git）、导入 config、跨 target 安装/移除目录链接、git cache、sync 对话框数据 |
| `workspace/mcps.rs` | MCP server 的 config.yaml CRUD、格式无关（JSON/TOML/OpenCode）的 per-target MCP 配置文件读写 |
| `workspace/mod.rs`（773 行） | 公共底座：YAML 模板与解析、App state 加载、rfd 路径选择器、fs 原语、normalize helpers、测试挂载 |

边界规则：多域共用的函数（`resolve_target_from_id`、fs 原语、`normalize_patterns` 等）留 mod.rs；仅单域使用的 helper 随域走。

**命名空间保持扁平**：mod.rs 用 `pub(crate) use` 把全部 `*_inner` 入口 re-export 回 workspace 命名空间，`commands.rs` 的 `use super::{...}`、`lib.rs` 的 `workspace::WorkspaceConfigStore`、两个 `#[path]` 挂载的测试文件的 `use super::*` 均不需要改动。

## 生效约束

- 新的 workspace 业务函数放进对应域文件；确属跨域的才进 mod.rs。
- 域内私有 helper 不提升可见性；跨域使用提 `pub(super)`，不要把内部 helper 挂到 `pub(crate)` re-export 里。

## 验证

- `cargo fmt --check`、`cargo check` 零警告零错误；`cargo test --lib workspace` 24 通过 1 ignored（网络 smoke）。
- 纯移动抽查：10 个跨文件搬移的函数与 HEAD 逐字对比，8 个完全一致，2 个（`create_workspace_mcp_inner`、`read_existing_mcp_entries`）仅可见性修饰符按规则调整。
- `commands.rs` / `inspect.rs` / `config.rs` / `types.rs` / `lib.rs` / 两个测试文件零改动。
