# session 来源会话根走 agents 清单:重定向用户三域同源

- 日期:2026-10-12
- 状态:生效中
- 来源:2026-10-12 架构评审第 2 轮 03 号候选(上轮清单小项「session 域环境解析」升格)

## 结论

codex / claude / dsh / grok 四家的会话根 `root()` 统一走新增的
`session::spec_root(app)`:按来源查 agents 清单的 `GlobalRoot` 解析
(重定向变量优先、HOME 兜底),与 workspace / providers 的 C10 路径
来源同一份。grok 的 `grok_binary()`(home 内优先找 CLI)同源派生;
`support::fs::grok_home_path`——与 `Redirect::GrokHome` 平行的第三份
重定向知识——退役。

## 行为修复

此前 codex / claude / dsh 死拼 `HOME/.codex` 等目录、无视重定向变量:
设了 `CODEX_HOME` / `CLAUDE_CONFIG_DIR` 的用户,workspace 与 providers
读写都在变量目录下(C10 之后),而历史会话列表按旧路径找——转录实际
不在 `~/.claude/projects`,列表为空。grok / pi 原本就认变量,同一张
来源注册表里两种口径。

关键语义:**重定向变量的值即整个根**。官方 CLI 实测(codex 0.158.0,
2026-10-12)把 `sessions/年/月/日` 树直接建在 `$CODEX_HOME` 下,`.codex`
只是 HOME 兜底时的缺省名——与本实现及 workspace 侧的既有语义一致。

## 取舍与边界

- **pi 例外保留**:它的根有官方契约链(`--session-dir` >
  `PI_CODING_AGENT_SESSION_DIR` > `settings.json.sessionDir`),root
  函数还带 `expand_configured_path` 的展开语义;行为本身已认
  `PI_CODING_AGENT_DIR`,特有契约留在 pi.rs(reader-engine ADR 同款
  取向:来源特有契约不强行统一)。
- **codex 侧实测**(补 C10 ADR 的 unverified 点):空目录 CODEX_HOME 下
  `codex exec` 因无凭据 401,拿不到转录文件;但 CLI 在
  `$CODEX_HOME/sessions/2026/10/12/` 创建了完整年月日树——路径选择的
  直接行为证据,转录文件因认证未落,标注于此。
- **dsh 侧 unverified**:本机无 dsh CLI,`DSH_HOME` 是否搬 `sessions`
  未实测;实现按同型处理(与 workspace 侧的 DSH_HOME 语义同一份),
  测试只断言根解析。
- **claude 侧无需重测**:C10 ADR 已实测(2.1.285,`CLAUDE_CONFIG_DIR`
  下 settings.json / projects / sessions 整体搬移)。
- 重定向值不展开 `~`:与 workspace / providers 侧同口径(域间一致性
  优先于单域宽容)。

## 证据

- 提交:`ec764a9`。
- 测试:三个用例先在旧实现上失败(root 不认重定向、列表为空)——
  codex / claude 按完整读取器列表验证(种子在重定向根下,旧实现列表
  为空),dsh 无本地 CLI 按根解析验证;全量 cargo test 454 passed
  (基线 451 + 3),clippy 保持基线 73。
- 既有测试无回归:全部 session 测试在 `TestEnvGuard` 下运行(重定向
  变量被清空),`resolve` 落 HOME 兜底,行为与旧路径拼写一致。
