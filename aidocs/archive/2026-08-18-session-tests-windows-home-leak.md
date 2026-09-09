# session 域测试在 Windows 上因 HOME 重定向不完整而失败

- 日期：2026-08-18
- 状态：已修复（同日），修复见 `support/fs.rs` 的 `user_home_dir` / `path_key` 与 `tests/support.rs` 的 `TestEnvGuard`。本文仅作排障过程存档。

## 补充（修复时确认的完整根因链）

1. `dirs` v6 在 Windows 上 `home_dir()` 走 `SHGetKnownFolderPath(FOLDERID_Profile)`，**完全不读 HOME/USERPROFILE 环境变量**，环境变量重定向方案无效。
2. 第二层：`sessions_by_path` 用 display 字符串做 key，测试经 `PathBuf::join("a/b")` 拼出的路径保留 `/`，WalkDir 枚举返回 `\`，字符串不匹配。修复为 `path_key()`（canonicalize 优先），对生产也是加固（前端传入 `/` 分隔路径可正常命中）。
3. 第三层：`deleting_opencode_family_uses_cli` 依赖 sh 脚本假 CLI + Unix PATH 语法，Windows 上会调到真实 opencode CLI，已标 `#[cfg(unix)]`；一处断言用裸字符串比较路径，改 Path 语义比较。

## 以下为原始排障记录

## 现象

`cargo test` 中 18 个 session 域测试稳定失败（session_readers / session_delete / session_listing / session_import），三类失败模式：

1. 计数偏大：seed 2 个 session 断言列出 2 个，实际列出 7–9 个（真实用户目录的 session 混入）。
2. 找不到文件：`Could not find Claude Code session for %TEMP%\agent-tools-test-<uuid>\.claude\...`（seed 与查找使用了不同的 home 解析）。
3. `table session already exists`：并行测试同时向同一个真实 opencode 数据库建表。

## 根因

`src/tests/support.rs` 的 `TestEnvGuard` 只设置 `HOME` 环境变量。Windows 上 `dirs::home_dir()` / `dirs::config_dir()` 优先走 `USERPROFILE` / known folders / `APPDATA`，`HOME` 被忽略，测试读到真实用户目录（`C:\Users\ly`）。seed 侧与查找侧对 home 的解析路径不一致，进一步造成"写到 temp、找到真实目录"或反之的错位。

证据：`effective_cwd_prefers_explicit_value_and_falls_back_to_home` 断言失败输出 `left: "C:\Users\ly"`（真实 home），期望 temp 目录。

## 影响范围

仅测试隔离，不影响运行时逻辑。workspace 域测试不受影响（config 守门人改造后用 `WorkspaceConfigStore::at(temp)` 显式隔离，不依赖环境变量）。

## 修复方向（待做）

`TestEnvGuard` 在 Windows 上需同时重定向 `USERPROFILE`（`dirs::home_dir`）、`APPDATA` / `LOCALAPPDATA`（`dirs::config_dir`，opencode SQLite 路径），drop 时恢复。注意 `env::set_var` 是进程级全局，测试间必须继续持 `test_env_lock` 串行。修复后 `cargo test` 应全绿。

## 关联

- 2026-08-18 config 守门人批次提交时发现并确认（stash 验证 HEAD 上失败集合一致，非该批次引入）。
