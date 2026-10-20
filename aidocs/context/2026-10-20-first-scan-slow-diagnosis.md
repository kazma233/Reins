# 首次扫描历史会话慢的排查记录(2026-10-20)

## 现象

每次启动 reins 后第一次进会话列表要等数秒。设计预期是「只扫描最新的没扫描过的会话」,应接近即时。

## 结论

「增量」只在**摘要层**成立;**目录索引层**是纯内存,每次进程启动全量重建,其中两个来源的重建成本失控,构成冷启动长杆。本机实测(2026-10-20,持久缓存全热):

| 来源 | 冷启动 list_entries | 条目 | 成本所在 |
| --- | --- | --- | --- |
| OpenCode | 6.0s | 356 | `list_rows` 对 3.9GB `opencode.db` 的 `session_message`(4.6 万行)跑两个全表 `json_extract` 聚合:模型分组 3.3s + usage 求和 0.7s + 全表扫描开销,无任何持久缓存 |
| DSH | 3.8s | 26 | `list_rows` 逐转录整份解码多帧 zstd(`~/.dsh` 400MB),无持久缓存 |
| Codex | 1.3s | 714 | `list_rows` 逐文件 open + 读首行(1330 个转录),结构性成本 |
| Zcode / GrokBuild / Pi / Claude | <0.7s | — | 无感 |

`detect_sources` 并行全来源,总时长 = 最长来源(OpenCode ≈ 6.5~7s),这就是用户感知的「第一次扫描很久」。手动刷新(refresh_sessions)同因,约 4.4s。

## 已排除

- 持久摘要缓存(SummaryDb,按 `(来源, 路径, mtime)`)工作正常:codex 冷启动 1.06s(命中持久层,不重解析转录);索引热后 `list_entries` 仅 0.08s。不存在「每次启动都全量重解析」的 bug。
- app.log 无坏文件 skip 记录,不是解析失败拖慢。
- 并行来源间无显著锁竞争(6.5s ≈ OpenCode 单独 6.0s)。

## 修复(同日实施)

读取器引擎(`reader_engine.rs`)的目录索引新增持久层 `rows_cache`(独立库文件 `~/.reins/session-rows-cache_v1.db`,模式对齐 summary_cache):

- 键 = (来源, 扫描根, freshness),失效口径与内存目录索引同一份 freshness(OpenCode: db mtime;DSH: 指纹;codex: 目录树 mtime 最大值),信任窗口从进程内延伸到进程外,与摘要缓存"信任 mtime"的口径一致。
- `family_index` 内存 miss 时先查持久行,命中则跳过 `list_rows` 的物理扫描;miss 才扫描并写回。行集合可序列化(`FamilySpec::Row` 加 serde 约束,五个 engine 来源的 Row 补 derive)。
- 显式清缓存(`clear_all_caches`)连行缓存一起清空;刷新(rescan)只丢内存,语义与摘要缓存一致。
- 行结构变更必须 bump `rows_cache::DB_FILE_NAME` 版本号(旧行反序列化失败按 miss 全量重建)。

实测(修复后,真实数据):detect_sources 冷启动 6.5s → **0.39s**(会话数一致,1362);持久缓存失效后的第一次物理扫描含写缓存约 8.4s(热页缓存),只发生在该来源数据实际变化后。

遗留:OpenCode 的 freshness 是整个 db 文件的 mtime,opencode 活跃写入时该来源仍要全表聚合一次(~6s);真正的增量聚合(按消息水位)未做,属于后续优化。

## 观测手段

扫描计时已加日志(本次一并交付):所有扫描入口都经 `build_session_catalog`,每来源一条 `session_scan source=X seconds=N.NN sessions=N`;`detect_sources_inner` 收尾一条 `session_scan_total seconds=N.NN`。见 `~/Library/Application Support/reins/logs/app.log`。

## 证据

诊断用临时 bench harness(#[ignore] 测试,跑真实 HOME 数据,已删除):分来源冷启动计时、codex 索引/摘要分解、OpenCode SQL 级分解、rusqlite 与 sqlite3 CLI 对照。
