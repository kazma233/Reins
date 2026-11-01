# 删除侧线收编:CLI 运行器、prune 与结构化删除计划

- 日期:2026-10-12
- 状态:生效中
- 来源:2026-10-12 架构评审第 2 轮 01 号候选(上轮清单的 C6)

## 结论

删除侧线的三个散落点收进 `session/delete.rs` 一个接缝:

1. **`run_official_cli(what, command)`**:官方 CLI 删除的执行机械(跑命令、
   判状态、拼失败文案)唯一实现。「用哪个二进制」是各家知识——grok 的
   home 内优先解析与幂等语义注释留在 grokbuild.rs,调用方只构造 `Command`
   与命令描述。失败文案统一为 `<命令描述> failed. exit_code: …`。
2. **`prune_empty_parents(root, start)`**:三份本地实现(此前已漂移)收成
   一份,统一带 `current != root` 保护。
3. **`DeletePlanInput`**:删除预演的输入是 family 的非渲染折叠(root_id +
   成员的 id/路径/root 归属),不再是渲染 DTO `SessionOverview`。plan 的
   成员级推导(逐 id 官方命令、成员文件/目录、root 排除、SQL child_ids)
   只认这个结构。

## 关键取舍

- **claude 的 prune 保护是行为修复**:旧实现缺 `current != root`,没有子
  代理的会话删除后项目目录删空,prune 上溯会把 `~/.claude/projects` 一起
  删掉。pi(显式配置的会话根)与 grok 本有保护,claude 是唯一分叉;回归
  用例在旧实现上失败、新实现上通过。
- **claude 的 meta.json 预演从路径嗅探改为镜像执行侧**:旧 plan 只对
  「subagents 目录下的 .jsonl」列 meta 动作(因为 overview 里没有 root
  归属),执行侧却对所有非 root 成员「存在才删」。结构化后预演对所有
  非 root 成员列出,回归「完整意图」契约;若存在 subagents 之外的非 root
  成员,预演会比旧版多列执行侧本就会删的条目。
- **折叠保留成员全集、不做全局去重**:同 id 的 codex 续跑段各有自己的
  文件,plan 的文件清单不能丢路径;`session_ids()` 单独做全局保序去重
  (root 先见),与展示名册 `family_agents` 的 Root 标签去重是同一条规则
  的两个消费方。
- **折叠跳过「声明 root 且与 root 同 id」的成员行**:claude 的分组把
  root 行同时放进 `members`(`root` 字段是另挑的一份),不跳过会让 root
  文件在 plan 里出现两次。codex 的续跑段同 id 但不声明 root,不受影响。
- **执行侧两次解析不合并**:`delete_session_inner` 仍经 `parse_overview`
  取展示字段(`deleted_paths`)。执行函数签名(`fn(&str) -> Result<()>`)
  不动;合并的收益(少一次 family 解析,缓存命中)撑不起接口变化。

## 明确不做

- 「注册树描述」(apply/remove 共用一张键表描述)是 providers 域 C9 的
  观察方向,删除侧线没有对等诉求——五家 plan 的差异是真实来源差异,
  `DeletePlanInput` 之上的 plan 函数已经各只有一份。

## 证据

- 提交:`18dca58`(CLI 运行器)、`93be8ed`(prune + claude 保护)、
  `2b24d44`(plan 结构化)。
- 测试:plan 直测(手构输入,不再逐来源种目录树);`from_family` 折叠
  契约单测(含 claude 重复 root 行与 codex 续跑段两种同 id 形态);
  claude 留一条真后端端到端接缝用例;prune root 保护直测 + claude 回归
  用例;假 CLI 装卸收成 `FakeCli` guard。
- 全量 cargo test 446 passed(基线 444 + 新增 prune 直测与 claude 回归);
  clippy 保持基线 73。
