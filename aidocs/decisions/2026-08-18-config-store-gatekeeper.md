# config.yaml 收口到 WorkspaceConfigStore 守门人

- 日期：2026-08-18
- 状态：生效中

## 背景

`workspace/mod.rs` 里 34 个 `*_inner` 函数直接操作全局 config.yaml，存在三类问题：

1. **读侧序言手抄 15 处**：`app_config_path()` → `read_existing_or_template()` → `parse_manager_config()` 三行序列在 `mod.rs` / `inspect.rs` 重复 15 处，路径解析逻辑（`<config_dir>/agent-tools/config.yaml`）散落在 `app_dir` / `app_config_path` / `git_cache_root` / `git_cache_dir_for_repo` 四个自由函数里。
2. **写侧三种风格并存**：`mutate_raw_manager_config` + `FinalizeRawMutation` trait、直接 `read_existing_or_template` + `write_raw_manager_config`、以及 delete 系列的混合写法；`updated_paths` 靠 trait 事后补写。
3. **并发与崩溃安全缺失**：Tauri command 全部走 `run_blocking`（线程池），两个命令的读-改-写可以交错互相覆盖；`fs::write` 直接截断写，进程崩溃会留下半个 YAML。TOML/JSON 目标配置写入同样非原子。

## 决策

1. **`WorkspaceConfigStore` 深模块**（`workspace/config.rs`）：拥有 config.yaml 的全部磁盘交互——路径解析（`config_path` / `git_cache_root` / `git_cache_dir_for_repo`）、读取（`read_raw` / `parse` / `load_document`）、互斥与原子写。Tauri `manage` 单实例；`Clone` 共享锁与路径，可自由移入 `spawn_blocking`。测试经 `WorkspaceConfigStore::at(temp_dir)` 完全隔离，不碰真实用户目录。
2. **写侧统一 `store.locked(|config| ...)`**：整个读-改-写序列持进程级 `Mutex`，`ConfigLock` 是锁内唯一合法写句柄（`read_raw` / `parse_raw` / `write_raw`）。锁中毒不阻断（原子写保证文件要么旧要么新，继续使用是安全的）。删除 `mutate_raw_manager_config` 与 `FinalizeRawMutation`，`updated_paths` 在各闭包内直接构造。
3. **`write_atomic` 用于所有配置写入**：同目录临时文件 + rename，崩溃不留半个文件；YAML / TOML / JSON 配置写入统一走它。临时文件名带 uuid，多个原子写互不踩踏。
4. **批量删除单锁单写**：`delete_skill_sources_inner` 原先逐条调单删（N 次读 + N 次写），现在一次锁内完成全部条目、末尾单次写盘——中途失败则整批不落盘。

## 生效约束

- 新增 workspace 命令必须注入 `tauri::State<WorkspaceConfigStore>` 并把 store 传给 `*_inner`，不得再出现自由函数形式的路径解析或直接 `fs::write` 配置文件。
- 读配置用 `store.parse()`；改配置用 `store.locked()`，锁内经 `ConfigLock` 读写。
- git 缓存目录解析（owner/repo 布局）只能经 `store.git_cache_dir_for_repo`，保证 import/sync/delete 的缓存身份一致。

## 验证

- `cargo check` 通过；`cargo test` workspace 域全绿，含新增 `tests/workspace_config.rs` 9 个测试：单删/批删/未知 id/空批次/建 target 落盘、双线程并发读改写不丢更新（锁生效的直接证明）、`write_atomic` 无残留临时文件、首次加载 bootstrap 模板可解析。
- `pnpm test`（vitest 9 绿）、`pnpm build`（vue-tsc + vite）通过——Tauri command 增加的 `State` 参数不影响前端调用契约。
- 已知无关失败：18 个 session 域测试因 `TestEnvGuard` 在 Windows 上只重定向 `HOME` 而泄漏真实用户目录（见 `aidocs/context/2026-08-18-session-tests-windows-home-leak.md`），stash 前后失败集合一致，与本改动无关。
