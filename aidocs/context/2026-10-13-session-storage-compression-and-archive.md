# 会话存储状态:压缩与归档(七个来源)

- 日期:2026-10-13
- 性质:探索记录 + 落地说明。证据来源:各上游源码(按发布 tag,见 Ref.md)、本机实测数据目录。
- 触发:codex 打开 `features.local_thread_store_compression` 后,冷会话转录被原地压成
  `.jsonl.zst`,reins 列表只认 `.jsonl`,可见会话从上千个掉到 5 个。

## 结论(TL;DR)

1. codex 会**原地压缩冷会话**:mtime 超过 7 天的 rollout 被 zstd(level 3,整文件单帧)压成
   `<原名>.jsonl.zst`,`rename` 后删除明文,并保留原 mtime。由后台 worker 执行,启动或
   app-server `rollout/compress` RPC 触发,`.tmp/rollout-compression.lock` 保证 6 小时内
   最多跑一次(`rollout/src/compression.rs`)。读侧明文优先,其次压缩兄弟文件。
2. codex 的**归档**是文件移动:`sessions/` 与 `archived_sessions/` 之间 rename
   (`thread-store/src/local/archive_thread.rs`、`unarchive_thread.rs`),目录即权威。
3. dsh 的归档是 `storages/workspace.json` 的 `global.archivedSessionIds`(纯软状态,
   不动转录);其转录**默认就是压缩形态**,v0 的规范文件名是 `session[.v{N}].jsonl[.zstd]`
   —— v0 不带版本段,漏判会整族消失。
4. zcode / opencode 的归档是 sqlite 的 `time_archived` 列(后加列,老库可能没有)。
5. **pi、grokbuild、claude code 既无转录压缩也无归档状态**:pi 会话列表是 sessions 根下
   全部 `.jsonl`(上下文 compaction 只追加条目);grok 无 archived 字段;claude 的
   compaction 写在明文转录内部(`compact_boundary` / `isCompactSummary`),归档只存在于
   VSCode 扩展与云侧。

## 一、各来源契约

| 来源 | 压缩 | 归档 | 权威来源 |
| --- | --- | --- | --- |
| codex | 冷会话(≥7 天)⇒ `<name>.jsonl.zst`,单帧 zstd L3 | 文件在 `sessions/` ↔ `archived_sessions/` 间移动 | 转录文件;状态库 `threads.archived` 冗余 |
| dsh | 默认 `.zstd`,多帧拼接,尾部可截断 | `storages/workspace.json` → `global.archivedSessionIds` | 转录文件 + 归档账本 |
| zcode | 无(转录在 `db.sqlite` 的 `part.data`,明文 JSON) | `session.time_archived`(NULL=未归档) | `~/.zcode/cli/db/db.sqlite` |
| opencode | 无(`session_message.data` 明文 JSON) | `session_v2.time_archived` | `opencode.db` |
| pi | 无 | 无 | `sessions/**/*.jsonl` |
| grokbuild | 无 | 无(`Summary` 无 archived 字段) | `summary.json` + `updates.jsonl` |
| claude code | 无(压缩是转录内部的 compaction 边界) | 无(仅 VSCode/云侧) | `projects/**/*.jsonl` |

## 二、reins 侧落地(本次)

- `SessionSummary` 增 `archived` / `compressed` 两个状态位,各读取器按自家契约填;
  列表行渲染成小角标(归档=黄、压缩=灰)。
- 新增 `session/transcript.rs`:明文/压缩的路径归一(`resolve`)、按行流式读取
  (`open_lines`,codex)、整份解码(`read_frames`,dsh)。dsh 的解码从
  `dsh.rs` 迁入,多帧与截断容错口径不变(由 dsh 既有测试守住)。
- codex 转录发现覆盖 `sessions/` + `archived_sessions/` 两个根、两种后缀;同目录明文与
  压缩兄弟共存时只认明文(上游 `should_skip_compressed_sibling` 同语义);索引失效改为
  走两个根的整棵树(归档/压缩都只改父目录 mtime)。
- usage 采集的文件发现下放到来源注册表(`UsageKind::CachedJsonl.files`),codex 用它把
  压缩与归档会话一并计入。
- summary 持久缓存版本 bump 到 v3(旧行的新字段反序列化后恒为 false,缓存键只看 mtime)。
- 刷新语义拆分(`session/mod.rs`):刷新/删除走 `rescan_all_caches`(只丢内存层,持久摘要与
  用量缓存按 (路径, mtime) 自失效);只有显式清缓存命令走 `clear_all_caches`(连持久层
  一起清)。原先刷新=全量重建,在可见会话从个位数涨到上千后,一次刷新要重解 1329 个转录:
  本机实测 27.6s(清持久) vs 1.4s(只清内存,codex 侧)。

## 三、解压器选择(libzstd)

本机实测(codex 本机 1329 个转录 ≈ 3.3GB 解压量):

| 解码器 | 吞吐 | 读首行(只取 session_meta) | 目录索引冷建 |
| --- | --- | --- | --- |
| `ruzstd`(纯 Rust) | 83MB/s | 9.6ms/文件(实际解了大半个帧) | 62s |
| `zstd`(libzstd 绑定) | 425MB/s | 0.64ms/文件 | 28s |

`ruzstd` 对 dsh 的小文件足够,但 codex 压缩把每次列表/用量采集的解析量推到 GB 级,
故改用 libzstd 绑定。C 工具链不是新增要求:`rusqlite` 已启用 `bundled`。

冷建索引的其余成本来自既有设计(目录里每个会话都要整文件解析一次拿标题与用量,
见 `summary_cache.rs` 头注释),持久缓存命中后为 1.0s 量级;压缩前同样是这份成本,
不是本次引入。

## 四、未验证与边界

- 失效检测是毫秒精度 mtime/fingerprint:同一毫秒内的改写看不见(测试夹具需要显式推后
  mtime)。读取侧由 `transcript::resolve` 兜底(明文/压缩互为兄弟),所以目录行即使落后
  一个形态,详情仍读得到。
- 刷新耗时仍由 dsh 主导(本机 3.7s):dsh 的目录行要读完整份转录(header/标题/用量/
  子会话清单),而刷新会丢内存行缓存。这是该来源既有实现,与压缩无关;要压到 1 秒内需要
  拆 dsh 的"行解析"与"时间线解析",或让刷新复用读写器的 freshness 门控缓存。
- codex 压缩 worker 只由 app/CLI 启动或 app 的 RPC 触发,reins 不触发、也不解压落盘,
  只在读取时内存解压。
- `codex delete` 能删归档会话(上游 `delete_thread` 覆盖两个根),但归档会话出现在列表后,
  删除入口的实际表现未在真实归档样本上点过。
- claude code 的 `compact_boundary` / `isCompactSummary` 未接入(无归档/压缩状态位需求,
  仅影响时间线渲染)。
