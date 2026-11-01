# Reins 界面设计语言:「安静的仪表台」

2026-10 整体 UI 精修时确立,后续所有界面改动遵循这里的口径。token 定义在 `src/styles.css`,这里的规则解释的是 token 背后的取舍。

## 定位

Reins 是开发者本地常驻的控制台:数据是主角,框架退后。视觉上「安静、精密、工具感」,不追营销页的氛围。

参考过 SAP 官网的公开设计分析(单动作色、色带分区、文本阶梯、平面 + hairline),只借了这些**原则**;品牌、间距、字号、控件尺寸全部是 Reins 自己的,不照抄。

## 核心规则

1. **主色 #0052ff 是唯一的动作色**:主按钮、文字链接、勾选框、进行中指示。实心蓝只出现在真正的主按钮上。
2. **选中态一律淡蓝 tint**(`--color-primary-tint` + `--color-primary-line` 描边),不用实心蓝填充——左栏、页签、分段控件、时间线 tab、排序开关都如此。选中态用实心蓝会让常驻框架比内容抢眼。
3. **色带做结构,不用阴影**:导航栏、卡片 header/footer 条用 `--color-surface-soft`,内容区保持白;结构分隔靠 hairline,阴影只表达「浮在内容之上」(下拉面板 `--shadow-pop`、弹窗 `--shadow-overlay`)。
4. **圆角四档**:控件 8(`--radius-control/button`)、小面板 6(`--radius-sm`)、卡片 12(`--radius-md`)、弹窗 16(`--radius-xl`)。pill 只留给状态徽标与骨架线,页签/分段控件不做胶囊。
5. **字号七级**(11/12/13/14/16/20):页面级标题 20px/600(`--font-size-2xl`),区块/卡片标题 14–16px/600,正文 13,辅助 12,徽标 11。按钮沿用三档高度 36/32/28。
6. **文本阶梯**:标题 `--color-ink`(近黑冷灰)、正文 `--color-body`、辅助 `--color-muted`;机器值(URL、路径、模型名、Session ID)一律 `--font-mono` + 弱化色。
7. **语义色分「本色 / 深字 / tint / 描边」四件套**(success、warning、danger 各一组):浅底上的文字用 `-ink` 深字档(warning 尤其,`#ea8a1f` 在白底读不清),徽标底用 `-tint`,描边用 `-line`。危险与警告按钮统一描边式,不实底。
8. **标签不转大写**:界面文案中英混排(「全局 targets」「Agent」),`text-transform: uppercase` 会让英文在中文旁边显得突兀;纯英文的徽标(SUBAGENT 等)可保留大写但字重 600、字距 0.04em。
9. **键盘焦点环统一** `2px solid var(--color-focus-ring)`;全局 `:focus-visible` 是兜底,组件级规则覆盖它。
10. **按钮按压回缩到 0.96**:只在 `prefers-reduced-motion: no-preference` 下生效,禁用态不缩;高频交互(行悬停、页签切换)不加动效,只做瞬时底色。

## 组件口径速查

- 页签(同一层级的视图切换):soft 底 + hairline 的按钮,选中换 tint 蓝字 600。
- 排序/开关类小按钮:开启态 tint,不实心。
- 错误/加载失败条:`--color-danger-tint` 底 + `--color-danger-line` 描边 + danger 字(AppLoadError、usage-error 同口径)。
- 输入控件:hover 时边框加深一档,mfocus 时主色边 + 焦点环。
- 弹窗:16px 圆角、`backdrop-filter: blur(2px)` 的深色幕布、滚动区 `overscroll-behavior: contain`。

## 已知未做

- 会话页顶部两个加载失败条会同时出现(列表 + 来源各一个请求各自报错),属于行为层问题,本次样式精修未动。
