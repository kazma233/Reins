# 架构评审第 2 轮收尾:寻址层归档与同步链契约面

- 日期:2026-10-12
- 状态:生效中
- 来源:2026-10-12 架构评审第 2 轮 07 号(寻址层搬家)与 08 号(同步链契约面)候选

## 结论

### 卡 07:寻址层归档(行为零变化)

`TargetRef` / `TargetSkip` / `SkillDirLink` / `target_unavailable` 与
`ResolvedManagerConfig` 的寻址 impl(约 200 行)从 types.rs 迁入
`workspace/addressing.rs`,TargetRef 直测随迁;types.rs 回归纯 DTO
(975 → 744 行)。判定早已单点(C2 达成),本项是住址归档:域红线
「types.rs 只定义 DTO」从口头约定变结构事实。

### 卡 08:同步链契约面(六小项 + 一件顺手)

1. **`skill_dir_links` 返三元组** `(TargetRef, &ResolvedTargetConfig,
   SkillDirLink)`:两条独立序列的 zip(skills.rs)原靠实现巧合维持配对,
   类型上不再可错位;`SkillDirLink` 的冗余 `id` 字段(三元组已携带)删除。
2. **删除 MCP 走寻址层**:`delete_workspace_mcp_inner` 的全局+项目两层
   手写循环换 `target_refs()`——「全部 target」的定义只有一处。
3. **MCP 在场状态改封闭枚举** `McpTargetState`(present/missing/error/
   unconfigured,snake_case 过 wire):前端拼错取值从运行期静默错变编译
   错,对照同域 `SkillLinkState` 先例。
4. **读取链收窄**:`read_existing_entries` / `read_file_entries` 只认生效
   前缀;「空前缀回落内置布局」上移为编排侧 `effective_prefix`(回落
   不出的非内置 id 按空前缀读空表、不报错——该语义由既有用例钉住,首版
   实现报错即被它拦下);为传一个字符串构造假 `ResolvedTargetConfig`
   的兼容路径删除,`env` 从读取链签名退场。
5. **地址格式单点**:前端 `formatTargetId` 与 `parseTargetId` 组成互逆
   对(共享用例集),五文件七处手拼 `${projectId}:${agentId}` 删除。
6. **相对路径后端下发**:`SkillLinkAssociation.source_relatives` 按归属
   来源下发相对路径(与安装面同一套 `relative_to_source_root` 口径),
   前端 `syncAssociations` 的 25 行 Windows 归一化重实现删除;
   `SyncTargetGroups` 的 `sourceRoot` prop 随之退场。
7. **删除按回执**:移除同步的就地更新按后端回执的 `destination_path`
   精确键过滤(全局唯一,单键足够),不再用 `matchedSourceIds` 归属
   判定重推后端删了什么——两条口径分叉会让列表与盘上事实静默不一致;
   归属判定留给预选等纯展示。

## 取舍

- **卡 02(grokbuild 全量迁引擎)维持不做**:C7 最小步已吃掉成本收益,
  全量迁移需引擎加「成员惰性」钩子、牵连六家读取器;除非 grok 读取形态
  再出真 bug,不单独立项。
- 摘掉 inspection 死字段与前端归一化删除都是**收窄契约**;若产品后续
  要「目录不存在」提示或相对路径的其它展示形态,按需在对应接缝重算。

## 证据

- 提交:`889f46f`(卡 07)、`64f046b`(卡 08 后端)、`0533db8`(卡 08 前端)。
- 测试:全量 cargo test 456 passed、前端 vitest 213 passed、
  `vue-tsc --noEmit` 干净;clippy 71(优于基线 73:顺带折叠 providers
  三处可嵌套 if)。
- 行为变化点:MCP state 的 wire 值不变(snake_case 拼写相同);
  `SkillLinkAssociation` 变宽(新增 source_relatives,向后兼容);
  其余为纯收窄。
- 「删除按回执」的弹窗交互链已于 2026-10-13 由用户手测通过。

## 2026-10-13 补记:目录不存在提示落地

上述「产品决定」已拍板:加提示。`TargetInspection` 补
`skill_dir_exists`(inspect 每行一次 `exists()`),面板按行渲染琥珀色
「目录不存在」pill;语义刻意中性——可能是待安装(安装会建目录),
也可能是路径写错,不下结论。inspection 行索引(skillDirLink.ts)随之
每行必有条目(原先是「有软链接才进表」),软链接提示的判断从
「在不在表里」改为「place 是否为空」。
