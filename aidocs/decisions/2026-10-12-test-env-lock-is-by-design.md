# 测试环境锁是设计，不是待清理的债

- 日期：2026-10-12
- 状态：**决定不做**（「测试隔离去进程环境依赖」立项取消）

## 决定

`tests/support.rs` 的 `TestEnvGuard` / `test_env_lock` 维持现状：持 guard 的测试继续串行，继续通过改写进程环境变量来隔离。**不为「让测试可并行」而重做测试隔离机制。**

## 机制（已实测确认）

持 guard 的测试都在覆写同一组**进程级**变量：

- `HOME`、`REINS_TEST_HOME`（`set_home` 写）
- `PATH`、`PI_CODING_AGENT_SESSION_DIR`（`prepend_to_path`、`set_pi_session_dir` 写）
- 5 个工具 home 变量（`set_home` 清空）：`CODEX_HOME`、`CLAUDE_CONFIG_DIR`、`GROK_HOME`、`PI_CODING_AGENT_DIR`、`DSH_HOME`

`env::set_var` 的作用域是进程，没有「按测试隔离」的粒度；而各域的夹具**需要不同的值**（pi 夹具要 `HOME` 指向 A、codex 夹具要指向 B）。因此这组变量在并行下必然互相踩，只有串行才自洽。

## 证据

全量 `cargo test`（含 bin 产物）：

| 条件 | 结果 |
| --- | --- |
| 有锁 + 并行（现状） | 442 passed，约 1.2s |
| 去锁 + 并行 | 87 failed / 355 passed |
| 去锁 + 串行（`--test-threads=1`） | **442 passed / 0 failed** |

去锁串行全绿 ⇒ **不存在顺序依赖**，失败全部是并发争用。受控实验印证：把一个本来会通过的 pi 测试与「另一线程反复覆写 `HOME`/`REINS_TEST_HOME`」并行运行，该测试失败；单独跑通过。

失败按模块分布（83 个样本）：`tests::pi` 18、`providers::tests` 16、`tests::grokbuild` 13、`tests::session_delete` 7、其余 session_* 约 26、`workspace::tests` 2。其中 `providers` 几乎不用 guard 却仍有 16 个失败——它的 `Environment` 从 guard 设好的临时 home 派生，因此同样吃这组进程变量。

## 已否决的方案

### 让 `user_home_dir()` 的进程内解析只认 `home_override` 静态

想法是「环境变量只作跨进程通道，不作本地开关」。实现后全量 442 全绿，但**去锁并行仍是 86 个失败（原 87）**——因为 `set_home` 把 override 与 `REINS_TEST_HOME` 设成同一路径，交换优先级在测试套件里从不产生差异，是个空操作。已撤回。

真正的阻塞点是上节那组变量本身，不是它们之间的优先级。

### 完整重做（四步：`set_home` 停止写 `HOME`、子进程显式传值、providers 等自建夹具改构造注入、各域分锁）

技术上可行，但收益只有约 0.6s 墙钟（会话域 129 个测试共占 0.45s），而代价是重写 4 个域的测试隔离机制。与本仓「只在存在真实契约时增加抽象」的取舍相冲突，且没有任何用户可见故障。

## 不要重开的依据

`aidocs/archive/2026-08-18-session-tests-windows-home-leak.md` 在修复 Windows home 泄漏时已写下同一结论：

> 注意 `env::set_var` 是进程级全局，测试间必须**继续持 `test_env_lock` 串行**。

当时解决的正是「测试读到真实用户目录」这类真实故障，采取的方案就是「重定向 + 继续串行」。本次复核没有推翻它，只是把「为什么并行不行」量化了。

## 补充：Windows 上不再需要 `USERPROFILE`/`APPDATA` 重定向

上述存档建议「在 Windows 上同时重定向 `USERPROFILE`、`APPDATA`/`LOCALAPPDATA`」。这条**已过时**：现在的进程内隔离走 `support/fs.rs` 的 `home_override()` 静态（`user_home_dir` 优先读它），绕开了 `dirs::home_dir()` 在 Windows 上不读环境变量的问题。`REINS_TEST_HOME` 只承担「把临时 home 传给测试子进程」这一件事，Windows 上读它的也只有测试二进制自身。

依赖 `dirs::config_dir()` / `LOCALAPPDATA` 的测试路径（若将来新增）不受 `set_home` 覆盖；opencode 的 SQLite 路径在测试里走的是 spawn CLI，那条用例已标 `#[cfg(unix)]`。

## 复核过程中的两个测量方法错误（勿重犯）

1. 曾用 `cargo test --lib` 测「去锁」效果，得到 82 个失败并据此立项。**`--lib` 不构建 bin 产物**，而 `session_delete` 的假 CLI 来自 bin（`fake_cli_source` 注释已写明项目脚本须用全量 `cargo test`）。全量下是 87，且失败清单不同。
2. 由同一错误派生出「4 个顺序依赖测试，集中在删除侧」的说法。`--lib` 下假 CLI 定位不到，失败在装 CLI 那一步；全量 + 串行则 0 失败。**不存在顺序依赖。**
