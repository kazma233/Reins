# grokbuild 索引缓存:指纹门控、失效自管

- 日期:2026-10-12
- 状态:生效中
- 来源:2026-10-12 架构评审第 2 轮 C7 号候选(上轮清单最后一项)

## 结论

grokbuild 的族索引加指纹门控缓存,读取成本收回域内:

1. **`GrokBuildBackend` 改「按根 slot 缓存实例」**(`slot: Mutex<Option<
   (PathBuf, Arc<GrokIndex>)>>`,换根即整实例重建),形态照抄 pi 迁引擎
   部件时的 reader slot。
2. **`GrokIndex` 持指纹门控的族索引缓存**:指纹覆盖 summary.json、
   chat_history.jsonl、subagent meta.json 三类文件的(路径, mtime, 大小)
   ——家族结构(集合、归属)与可读性(history 在不在)全部由它们决定。
   指纹命中返回克隆,失配才重建;解析成本只发生在失配时。
3. **`family_index()` 自由函数转为转发**,八处调用点(family_for_id、
   parse_*、delete、delete_plan_input)零改动;构建逻辑与迁入前逐条对齐。
4. **失效**:外部写入靠指纹失配;reins 删除在 `delete_session` 末尾显式
   清(契约写在动作发生地);通用清缓存流程经 `SessionReader::clear_cache`
   覆写到达。

顺带:`sessions_fingerprint`(文件集合新鲜度指纹)原是 pi/dsh 各一份逐字
相同拷贝,grok 是第三份——收进 `session/mod.rs` 单份。

## 关键取舍

- **不迁引擎的决定不变**:grok 的「主时间线只载父消息+入口、子会话流式
  分页」是刻意的内存契约,进引擎需要「成员惰性」钩子;C7 的诉求是成本
  自管,不是机械统一。reader-engine ADR 决策 6 与偏差记录已补记。
- **指纹失配对删除是兜底而非契约**:实测摘掉 delete 末尾的显式清,删除
  用例仍通过——删除本身移除文件、指纹必然失配。显式清保留的理由是把
  「删除 → 索引失效」写成动作发生地的显式契约,不依赖「指纹口径恰好
  覆盖删除的全部副作用」这个隐含前提(将来指纹若改粗,删除仍正确)。
  测试因此只钉行为契约(删除后列表不见),注释如实说明双保险关系。
- **`SessionIndexState` 保留**:它是列表条目的跨来源通用缓存(合并视图、
  detect 都用),不是 grok 放错位置的索引缓存;grok 有了自己的 freshness
  后,它对五家引擎来源仍只是省一次 freshness 检查。
- **`commands.rs` 的 refresh 默认值不动**:它是用户触发的列表刷新语义,
  与来源自管 freshness 是两层。

## 证据

- 提交:`e2b8abc`(指纹收编)、`0ad706c`(grok 缓存)。
- 测试:外部新增会话 → 指纹失配重建(`grokbuild_index_rebuilds_when_
  new_session_appears`);删除后列表不见已删会话(`grokbuild_delete_
  invalidates_index_cache`);全量 cargo test 452 passed(基线 450 +
  2),既有 grok 用例(折叠、占位、孤儿)全部原样通过;clippy 保持
  基线 73。
- 性能主张如实标注:本轮未做可感的延迟测量,收益的证据是结构性的
  (详情路径不再每次全量解析整棵 `~/.grok/sessions` 树),不是基准数字。
