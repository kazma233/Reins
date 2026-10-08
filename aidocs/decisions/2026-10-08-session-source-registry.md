# 会话来源分发收进来源注册表（sources.rs）

- 日期：2026-10-08
- 状态：生效中

## 背景

「支持哪些会话来源、根目录在哪、怎么探测/清缓存/删除/统计用量」这一份知识此前被打散成 6 处独立的 7 臂分发：`mod.rs` 的 `reader()` / `clear_all_caches()` / `delete_session()`，`catalog.rs` 的 `detect_sources_inner()`（7 段几乎逐字相同的 spawn 块）与 `available_sources()`（再抄一份 root 调用列表），`usage_stats.rs` 的 7 段 spawn 与四种接入姿势（`grokbuild_days` / `dsh_days` 是仅差三个函数引用的 40 行孪生函数）。zcode（9d87ea0）与 dsh（0116445）两次接入各自撞了一遍全部点位，漏改任何一处即静默不一致。

## 决策

1. **`session/sources.rs` 是来源分发的唯一事实**：`SourceSpec`（app / root / detect_note / reader / delete / usage）按值（`Copy`）持于 `static SOURCES`，7 条严格保持历史 join 顺序；`spec(app)` 用 match 实现，新增 `SourceApp` 变体而漏登记直接编译失败。
2. **删除策略上表**：`DeletePolicy::Deleter(fn)` / `Unsupported(&'static str)`；zcode/dsh 不支持删除的原因注释随策略值留在表项上，文案与原先逐字一致。
3. **用量采集三姿势收进 `UsageKind`**（定义在 usage_stats.rs，归桶机器同模块）：`CachedJsonl { subdir, extract }`（codex/claude/pi，mtime 持久缓存）、`RawFiles { paths, extract }`（grokbuild/dsh，两个孪生函数删除）、`Sql { collect }`（opencode/zcode）。三个 `days_from_*` 泛型 helper 唯一持桶聚合、缓存读写与跳过日志。
4. **并发统一 `parallel_collect(label, f)`**（sources.rs）：按 SOURCES 顺序 spawn、同序 join，线程 panic 折叠为携带 label 的错误（文案与原先一致）；detect 与 usage 共用。
5. **`SessionReader` 声明 `Sync`**：spec 会跨 scoped thread 共享，「读取器无内部可变性、并发靠各实现内部 static Mutex」这一不变量由 trait 显式承载。
6. 六个分发点全部改读表：`reader()` / `clear_all_caches` / `delete_session` / `detect_sources_inner` / `available_sources` / `usage_stats_inner`。

## 生效约束

- 新增来源 = 新 reader 文件 + `sources.rs` 一条注册（+ `SourceApp` 变体）；按来源的 match 不得在 `catalog.rs` / `usage_stats.rs` / `mod.rs` 复活。
- `SOURCES` 顺序是可观察行为（detect 输出顺序、usage sources 顺序），不得随意重排；`tests/session_sources.rs` 钉住覆盖、顺序与 `spec()` 路由。
- zcode/dsh 删除不可用的原因文案有断言钉住，改动需同步测试。
- `summary_cache` / `usage_day_cache` 两个跨来源持久缓存仍在 `clear_all_caches` 表外统一清理。

## 验证

- 先行测试：`unsupported_deletion_reports_app_reason` 对重构前代码即绿（钉住现状文案）；`sources_registry_covers_every_source_app_once_in_order` 先红（E0433 cannot find `sources`）后绿。
- `cargo fmt --check` / `cargo check --all-targets` 干净；`cargo test` 全绿：352 通过 / 0 失败 / 2 忽略（存量 350 + 新增 2，存量测试零改动）。
- 纯后端内部收拢：invoke 契约、`SourceApp` serde 形态、前端 `src/` 零变化。
