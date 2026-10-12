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

## 未纳入本次（已记录，不在 C10 范围）——已于 2026-10-12 完成

`session` 域仍各自解析同一批变量（`session/pi.rs`、`session/grokbuild.rs` 读 env，`session/codex.rs` / `claude_code.rs` / `dsh.rs` 拼默认目录）。这些属于会话存储读取路径，与 C10 的两域分叉无关；按「session 域单独一项」完成，见 `2026-10-12-session-roots-follow-redirect.md`：四家 root 走清单 GlobalRoot（本文件唯一的 unverified 收尾：claude skills 落点），codex 侧补实测——空目录 CODEX_HOME 下 `codex exec` 把 `sessions/年/月/日` 树建在变量目录下（转录文件因目录内无凭据 401 未落，路径选择已证）。

## 实测结论（2026-10-12，Claude Code 2.1.285）

本机装有真 CLI（`~/.vite-plus/packages/@anthropic-ai/claude-code/*/bin/claude.exe`，版本 2.1.285），
用隔离 HOME + `CLAUDE_CONFIG_DIR` 直接观察落点，此前三条 unverified 全部有结论：

- **`CLAUDE_CONFIG_DIR` 是完整的配置 home**：设置后 `<CFG>/.claude.json`、`<CFG>/projects/`、
  `<CFG>/sessions/`、`<CFG>/backups/`、`<CFG>/skills/` 全在变量目录下，宿主 HOME 下为空；
  `~/.claude` 是该变量的**缺省值**，不是第二种根。二进制里的错误文案印证同一口径
  （"the configuration home (CLAUDE_CONFIG_DIR, the HOME it defaults from, ...)"）。
- **`.claude.json` 随变量搬移**：此前按「落在 HOME」实现是错的，已改为 root 相对
  （`config_path: .claude.json`）。用户在 `CLAUDE_CONFIG_DIR` 下写 MCP 条目，reins 也会写到同一处。
- **`settings.json` 也在变量目录下**：往 `<CFG>/settings.json` 写一个标记后，debug 日志显示
  `CA certs: Config fallback - globalEnv keys: , settingsEnv keys: PROBE_MARKER`，确认它被读取，
  且 `env` 段生效。未设置变量时落点 `~/.claude/settings.json`。
- skills 落点由「目录整体搬移 + `<CFG>/skills/` 被 CLI 创建」推定，标 **unverified**；
  核对方式：把 skill 放到 `<CFG>/skills` 观察是否被发现（`claude skills list` 在未登录时不可用）。

顺带修掉一个口径分叉：config.yaml 里的 `~` 展开此前用 `dirs::home_dir()`（Windows 上忽略 HOME），
与清单解析用的 `support::fs::user_home_dir()` 不是同一个 home；两处已统一，模板写出的 `~/x`
与解析结果必然指向同一个文件。

## 仍未核对

- opencode / zcode 是否存在未记录的目录重定位变量（当前清单按「无」处理，与 providers 一致）。

## 影响

- 用户改 `CODEX_HOME` / `CLAUDE_CONFIG_DIR` / `DSH_HOME` 后，workspace 的 MCP 分发与默认模板跟 providers 落到同一文件；已存在的 `config.yaml` 里写死的路径不自动迁移（模板只在首次生成时固化，之后 config.yaml 是权威）。
- 测试夹具：`TestEnvGuard::set_home` 现在会清空五个工具重定向变量并在 Drop 恢复，避免真实环境变量漏进断言。
