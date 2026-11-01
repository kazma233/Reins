# providers 移除策略轻量收编:文案构造器、dsh 守卫、classify 口径钩子

- 日期:2026-10-12
- 状态:生效中
- 来源:2026-10-12 架构评审第 2 轮 C9 号候选(上轮清单遗留)

## 结论

reins 条目移除生命周期(移除 / 移除外部条目)的三个散落点收编:

1. **三句报错文案构造器**(`apps/mod.rs`):「已被手工修改」「没有可识别
   条目」「没有外部条目」收成 `modified_entry_error` /
   `unrecognized_entry_error` / `entry_not_found_error`,替换五家适配器
   十四处手拼。`app_label` 是各家文案既有主语(Codex/OpenCode/Pi/Grok/
   dsh),不做大小写统一;dsh 的 remove_external 文案「路由里没有条目」
   随统一改为「dsh 中没有外部条目」。
2. **dsh 补「可识别」守卫**(行为修复):此前 dsh 只查注册键存在,手工
   改过内容的条目会被静默删掉——其余四家(codex/opencode/pi/grok)都
   报「已被手工修改」。守卫谓词与 pi 同型:条目必须仍带 `baseURL`
   (apply 必写,pi-ai 方言同源)。守卫用例先在无守卫实现上失败。
3. **classify 比较口径钩子**:`classify_reins_entry` 拆出 `_with` 变体收
   `base_url_matches` 参数(entry 与平台的 base_url 怎么算一致,默认恒
   等)。Grok 的 Anthropic 写入口径会把平台 base_url 补成 `/v1` 结尾,
   恒等比较会错判成漂移;`grok_base_url_matches` 比较器取代
   `grok_providers_view` 的全量视图克隆(每个 ResolvedProvider 含 models
   整体克隆)。原签名保持并委托默认口径,其余四家调用点不动。

文件内逐字重复顺手收:pi 默认三元组清理两份 → `clear_default_refs`;
opencode `remove_external` 改调 `remove_registration_from`(加 bool 返回
供「键是否存在」判定);grok `models.default` 清理两份 →
`clear_models_default`;dsh 的 llm-pi-ai 重写与 agent-default-model 清理
两份各收共享助手,并修掉 profile 同路径双 push(原先靠后写胜出)。

## 关键取舍

- **守卫测试按覆盖分工,不做全量复制**:五家带注册键适配器的「无条目」
  语义由一张表驱动用例钉住同型(apply → 移除 → 再移除/外部移除,断言
  文案);「可识别删除」与「已被手工修改」的深语义由各家既有专项用例
  覆盖(codex 的 modified 拒删、dsh 的新守卫用例等)。claude 没有注册键,
  契约是 base_url 归属校验 + 元数据缺失拒删,不进这张表。
- **claude 旁路保留**:settings.json 没有 reins- 注册键,反读靠 base_url
  `find` 归属——它不是 classify 这条接缝的用户(一条 seam 有两个 adapter
  才是真的,这条接缝现有五个真用户)。已知限制:同 base_url 的两个平台
  会归属到 find 的第一个。
- **classify 钩子用函数指针而非泛型**:六家只有 grok 传非默认口径,单态
  化没有收益。
- **`remove(provider)` 参数的三种语义不动**:同一参数在 claude(必需)、
  codex(条件清理)、pi/grok/dsh(忽略)处含义不同,trait interface 偏
  浅;统一它要动 AppAdapter trait,超出轻量收编范围,记录为已知限制。
- **codex apply↔remove 的键集合正反两写不在本轮处理**:若后续上「注册
  树描述」(apply 写入面与 remove 清理面描述在同一张表),自然归置;单独
  收它的收益撑不起一次接口变化。

## 证据

- 提交:`1165835`(文案构造器 + 文件内收编)、`74cb0dc`(dsh 守卫)、
  `441157c`(classify 钩子)、`a2a11e6`(同型守卫测试)。
- 测试:全量 cargo test 450 passed(基线 447 + dsh 守卫用例 + 同型守卫
  用例);clippy 保持基线 73。dsh 守卫用例先在无守卫实现上失败(条目被
  静默删),加守卫后通过并断言拒绝时配置原样。
