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

## 后续优化实施(2026-10-24,Windows 本机)

上述遗留与 GrokBuild 冷启动瓶颈均已处理,方案与实测如下。

### GrokBuild:扫描路径的重复验证消除(不是加缓存)

分解计时(release、热缓存)发现 family_index 全程 0.59s 中 0.46s 花在逐会话装配:每个会话经 `summary()` → `validate_path`(每次都重新 canonicalize 根 + 逐组件 symlink 检查)×3 次(summary、usage、models 各一次)、usage.json 读两遍;subagent child 校验还会整份重读 summary.json。walkdir 与指纹本身只要 ~30ms,持久行缓存救不了这部分。

修复:扫描路径的会话目录来自自家 walkdir(不跟符号链接、天然在根内),路径信任枚举来源——`sibling` 拆出只校验兄弟文件本身的 `sibling_at`;每会话一次 `read_summary` + 一次 `read_usage_file`,装配共用 `summary_from_parts`(按 id 的单会话读取仍走完整 validate 的 `summary()`,安全边界不削弱);child 校验复用解析时留存的 (attempt_id, session_kind),不再重读文件。usage_hours 的直采同享 `sibling_at`。

实测:冷启动 list_entries 0.59s → 0.345s(release、热);剩余成本是每会话一次的必要读取与 sibling 符号链接防线。

### OpenCode:消息级贡献表(活跃写入不再全表聚合)

先证伪了两个直觉方案(本机 34MB/1919 行、45 会话的真实数据):

- 按会话 `session_v2.time_updated` 做增量键:162 条消息的 time_created 晚于所属会话的 time_updated——opencode 写消息不总 bump 会话时间,会漏。
- 按消息 rowid 水位累加合并:1553/1919 条消息行 time_updated > time_created——行会被回写,累加会重复计数。

采用**消息级贡献表**:rows_cache 库文件新增 `row_contrib` 表,键 (app, root, id),行级失效键 stamp = 消息行的 `time_updated`(data 变化伴随它推进)。每次同步:轻扫 `SELECT id, time_updated`(整数列,不碰 data 大 JSON)与贡献表 diff,仅变化行 json_extract 取 tokens/model;消失行删除。聚合(usage 求和、模型分组)在内存完成,口径与原 SQL 逐条对齐(SUM 后再钳制、calls DESC + 模型名升序)。幂等:同 (id, stamp) 不重提取,回写行按 stamp 变化整行覆盖,任何撕裂下轮 diff 自愈。贡献表读写失败按 miss 退回全量提取,权威数据始终是 opencode.db;显式清缓存连贡献表一起清。

接入:`FamilySpec::list_rows` 增加 `cache: &RowsDb` 参数(引擎实例的持久缓存句柄注入物理扫描);`usage_hours`(用量曲线的 SQL 直采)同走贡献同步,与列表共享一份数据。

实测(db 副本模拟三段):贡献表冷启动全量提取 0.056s(34MB);db mtime 变化但消息集合未变时 0 提取(<5ms);usage_hours 0.069s。原全表聚合(活跃写入后每次)在 3.9GB 库上是 ~6s,现在只与变化行数成正比。增量结果与全新 store 的全量提取一致(有回归测试)。

## 同因优化:用量统计(同日实施)

用量统计(`get_usage_stats`)有同样的结构性问题:三种采集方式里只有 CachedJsonl(codex/claude/pi)走 `usage_day_cache` 持久层,实测热缓存状态下 DSH 3.8s(RawFiles 每次整份解压转录)、OpenCode 3.0s(SQL 每次全表 json_extract)、Zcode 0.7s。修复:

- `UsageKind::Sql` 增加 db 路径声明,`days_from_sql` 以 db 文件 mtime 为键把整份统计(桶 + 两级会话数)存进 `usage_day_cache`;缓存值带采集日期,跨午夜后「今日会话数」口径过期,按 miss 重查。
- DSH 从 RawFiles 挪到 CachedJsonl:usage 只依赖转录文件本身,`(路径, mtime)` 完全决定输出;归档账本不影响 usage,不参与失效。
- grok 保持 RawFiles 直采(0.13s):其 extract 读 summary.json 旁边的 usage.json,路径键看不到兄弟文件变化,mtime 缓存有盲区,不值得为 0.13s 冒险。

实测:OpenCode 2.98s→0.004s、Zcode 0.71s→0.001s、DSH 3.75s→0.002s,会话数一致。显式清缓存已连 usage_day_cache 一起清(既有编排)。

## 观测手段

扫描计时已加日志(本次一并交付):所有扫描入口都经 `build_session_catalog`,每来源一条 `session_scan source=X seconds=N.NN sessions=N`;`detect_sources_inner` 收尾一条 `session_scan_total seconds=N.NN`。见 `~/Library/Application Support/reins/logs/app.log`。

## 证据

诊断用临时 bench harness(#[ignore] 测试,跑真实 HOME 数据,已删除):分来源冷启动计时、codex 索引/摘要分解、OpenCode SQL 级分解、rusqlite 与 sqlite3 CLI 对照。
