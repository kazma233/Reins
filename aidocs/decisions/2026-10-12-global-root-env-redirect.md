# 工具全局目录的重定向口径统一到清单

- 日期：2026-10-12
- 状态：已实施（对应 `context/2026-10-12-architecture-review-backlog.md` 队列第 1 项 C10 的 env 分叉部分）

## 背景

「某个工具的全局配置在哪」在代码里有三份说法：

1. `agents.rs` 的 `GlobalRoot`（workspace 的 target defaults 与默认模板从这里派生）；
2. `providers/apps/mod.rs` 的 `ToolEnv`（模型配置读写）；
3. `tests/providers.rs` 的路径助手。

三份中只有 `GlobalRoot` 只认 `GROK_HOME` 与 `PI_CODING_AGENT_DIR`，而 `ToolEnv` 还认 `CODEX_HOME` / `CLAUDE_CONFIG_DIR` / `DSH_HOME`。用户设置这些变量后同一个文件被两个域解析到两个位置，例如 workspace 的 MCP 分发写 `~/.codex/config.toml`，providers 的模型配置写 `$CODEX_HOME/config.toml`。

定性结论：知识漂移，不是隐含决策。`agents.rs` 的注释自称「env 重定向…与 support::fs 同一口径」，但清单只覆盖了两个变量；`template_global_path` 也只为 pi 处理了重定向，说明重定向是既有设计，只是没铺全。

## 决策

1. **清单持重定向知识**：`GlobalRoot` 增补 `CodexHome` / `ClaudeConfigDir` / `DshHome`，`redirect()` 是「该根对应哪个重定向」的唯一来源；路径解析与模板渲染共用它，避免「解析认重定向、渲染不认」的新分叉。
2. **环境视图是进程环境的唯一读取点**：`support::environment::Environment` 持有 home 与五个重定向值，`Redirect` 枚举同时持有「变量名 + 未重定向默认目录」。生产用 `Environment::from_env()`，测试用具名字段构造或从夹具派生；`agents` / `workspace` / `providers` 都不再直接读 `std::env` 里的这些变量。
3. **不是所有路径都随重定向搬移**：`GlobalLayout` 的路径值支持 `~/` 前缀，表示「落在 HOME 而非 root 下」。当前两处：codex 的 `~/.agents/skills`（skills 不在 `CODEX_HOME` 内）、claude 的 `~/.claude.json`。
4. **providers 从同一清单派生**：`providers::apps` 的路径函数不再重述相对布局——根经 `Environment::resolve(Redirect)`、配置文件名经 `support::environment::mcp_config_path(app)`（读清单的 `mcp_config_path`）；只有「同一目录下的另一个文件」（codex 的 reins-models.json、pi 的 models.json / settings.json、claude 的 settings.json）在 providers 侧显式拼接。原 `ToolEnv` 类型删除，适配器直接收 `Environment`。
5. **workspace 的环境挂在 store 上**：`WorkspaceConfigStore` 持有环境视图（`app()` 读进程环境，`at()` 为空视图，`with_env()` 注入），`parse_manager_config` / 模板生成 / inspect 都从它取，避免把 `env` 参数穿过三十个命令入口。

### 对既有决策的修订

`agents.rs` 原模块定位是「只收静态知识，不吸收任何行为」。本次让清单持有「哪个工具认哪个重定向」，但解析逻辑与进程环境读取都在 `support::environment`，清单仍只描述静态事实（路径的相对布局 + 重定向归属），不承担行为。

## 证据与验证

- 复现测试：`src-tauri/src/tests/workspace.rs::codex_mcp_apply_follows_codex_home_redirect`。设置 `CODEX_HOME` 后断言 workspace 的 MCP preview/apply 落在 `$CODEX_HOME/config.toml`，且不再写 `~/.codex`。实施前该断言失败（实测 left `~/.codex/config.toml`、right `$CODEX_HOME/config.toml`）。
- 派生快照：`tests/agents.rs::snapshot_builtin_target_defaults`（无 env 时行为与改造前逐字节一致）与 `::snapshot_builtin_target_defaults_with_codex_claude_dsh_redirect`（三个新变量逐项生效）。
- 模板同口径：`tests/workspace.rs::default_config_template_parses_with_builtin_targets` 锁定「未设变量写 ~ 缩写、设了才固化绝对路径」。
- providers 派生等价：`tests/providers.rs` 删掉手写的第三份路径助手后，全部用例改调生产路径函数并以夹具环境注入，行为不变（同一批断言全绿）。
- 两域同源不变量：`tests/agents.rs::workspace_and_providers_agree_on_codex_config_path` 直接断言设置 `CODEX_HOME` 后 `builtin_target_defaults`（workspace）与 `app_config_paths`（providers）解析到同一路径。
- 官方语义核对（2026-10-12，本机 codex-cli 0.158.0）：`CODEX_HOME=<tmp> codex doctor` 报 config 位于 `<tmp>/config.toml`，印证 `CODEX_HOME` 重定位整个 codex 配置目录。

## 未纳入本次（已记录，不在 C10 范围）

`session` 域仍各自解析同一批变量（`session/pi.rs`、`session/grokbuild.rs` 读 env，`session/codex.rs` / `claude_code.rs` / `dsh.rs` 拼默认目录）。这些属于会话存储读取路径，与 C10 的两域分叉无关；若要收敛到 `Environment`，应按 session 域单独一项做，避免把会话来源的存储契约一起改动。

## unverified（待有环境时核对）

- `CLAUDE_CONFIG_DIR` 重定位 `~/.claude/skills`（清单按此实现）与 `~/.claude.json` 是否随之搬移（清单按「不搬移」实现）。`context/2026-09-15-mcp-config-mapping.md` 已记录「未实测（无 CLI）」。核对方式：隔离 HOME 设置该变量跑一次 claude，观察 `settings.json`、skills 目录与 `.claude.json` 三处落点；若 `.claude.json` 确实随目录搬移，只需把该条改为 `".claude.json"` 并把 `skill_dir` 恢复为 root 相对。
- opencode / zcode 是否存在未记录的目录重定位变量（当前清单按「无」处理，与 providers 一致）。

## 影响

- 用户改 `CODEX_HOME` / `CLAUDE_CONFIG_DIR` / `DSH_HOME` 后，workspace 的 MCP 分发与默认模板跟 providers 落到同一文件；已存在的 `config.yaml` 里写死的路径不自动迁移（模板只在首次生成时固化，之后 config.yaml 是权威）。
- 测试夹具：`TestEnvGuard::set_home` 现在会清空五个工具重定向变量并在 Drop 恢复，避免真实环境变量漏进断言。
