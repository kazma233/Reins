# 从 nyleen 迁移数据到 agent-tools

- 用途：旧 nyleen 用户升级到 agent-tools 后，把用户数据（config.yaml + git cache）迁移到新路径
- 前置条件：已升级到 agent-tools 新版本
- 风险：迁移后 target 里已分发的 skill 软链会断（指向旧 nyleen 路径），需要重建

## 背景

agent-tools 把数据目录从 `nyleen/` 改名为 `agent-tools/`（破坏性改动，详见 [decisions/2026-08-18-rename-nyleen-to-agent-tools.md](../decisions/2026-08-18-rename-nyleen-to-agent-tools.md)）。

关键约束：
- `config.yaml` 内容**不依赖目录名**，可以直接迁移
- `git/` 缓存目录可以直接迁移
- target 里的 skill 软链存的是 **canonicalize 后的绝对路径**（含 `nyleen/`），迁移后软链会断

## 路径对照

| 系统 | 旧路径 | 新路径 |
|------|--------|--------|
| macOS | `~/Library/Application Support/nyleen/` | `~/Library/Application Support/agent-tools/` |
| Linux | `~/.config/nyleen/` | `~/.config/agent-tools/` |
| Windows | `%APPDATA%\nyleen\` | `%APPDATA%\agent-tools\` |

## 方案 A：保留配置 + git cache，重建软链（推荐）

适合：已经配置过 sources/targets，不想重新来的用户。

### 步骤 1：关闭应用

退出 agent-tools（和旧的 nyleen，如果还开着）。

### 步骤 2：迁移数据目录

macOS / Linux：
```bash
# 如果新目录已存在（首次启动 agent-tools 会生成默认 config.yaml），先删掉
rm -rf ~/Library/Application\ Support/agent-tools   # macOS
# rm -rf ~/.config/agent-tools                       # Linux

# 迁移旧目录
mv ~/Library/Application\ Support/nyleen ~/Library/Application\ Support/agent-tools   # macOS
# mv ~/.config/nyleen ~/.config/agent-tools                                           # Linux
```

Windows（PowerShell）：
```powershell
# 如果新目录已存在，先删掉
Remove-Item -Recurse -Force "$env:APPDATA\agent-tools"
# 迁移旧目录
Move-Item "$env:APPDATA\nyleen" "$env:APPDATA\agent-tools"
```

### 步骤 3：启动 agent-tools，重建 git source 的软链

迁移后 config.yaml 里的 sources/targets/projects/mcps 配置都在，但 **git source 在 target 里分发的软链全断了**（指向旧 `nyleen/git/` 路径）。

对每个 git source：
1. 在 skills tab 找到该 source
2. 点「移除同步」→ 勾选所有 target → 确认（清掉断链）
3. 点「同步」→ 勾选 skills + targets → 确认（重建软链指向新 `agent-tools/git/` 路径）

local source 的软链指向用户自己的目录，不受影响，无需重建。

### 步骤 4（可选）：验证

在 mcp tab 检查每个 mcp 在各 target 的 state，应该是 `present` 或 `unconfigured`。如果是 `broken` 说明软链还没重建。

## 方案 B：从头配置（干净）

适合：配置不多，或者想顺便清理的用户。

### 步骤 1：删除旧目录（可选）

macOS / Linux：
```bash
rm -rf ~/Library/Application\ Support/nyleen          # macOS
# rm -rf ~/.config/nyleen                              # Linux
```

Windows：
```powershell
Remove-Item -Recurse -Force "$env:APPDATA\nyleen"
```

### 步骤 2：清理 target 里的旧软链

旧 nyleen 在 target 的 skill_dir 里创建的软链现在全断了，手动清理：

```bash
# 例：codex 全局 target
find ~/.agents/skills -maxdepth 1 -type l ! -exec test -e {} \; -print -delete
# claude 全局 target
find ~/.claude/skills -maxdepth 1 -type l ! -exec test -e {} \; -print -delete
# opencode 全局 target
find ~/.config/opencode/skills -maxdepth 1 -type l ! -exec test -e {} \; -print -delete
# zcode 全局 target
find ~/.zcode/skills -maxdepth 1 -type l ! -exec test -e {} \; -print -delete
```

项目 target 同理，在 `<project>/.opencode/skills/` 等目录下清理断链。

### 步骤 3：启动 agent-tools，重新配置

按正常流程在 UI 里新增 sources / targets / projects / mcps。git source 会重新 clone 到 `agent-tools/git/<owner>/<repo>/`。

## 方案 C：只迁移 config.yaml，重建 git cache

适合：想保留配置但 git cache 太大不想搬的用户。

```bash
# 只迁移 config.yaml
mkdir -p ~/Library/Application\ Support/agent-tools
cp ~/Library/Application\ Support/nyleen/config.yaml ~/Library/Application\ Support/agent-tools/config.yaml
# git cache 不迁移，让 agent-tools 重新 clone
```

启动后对每个 git source 点「同步」，agent-tools 会自动 clone 到新路径。

## 排障

### 迁移后 agent-tools 启动报错

检查新目录权限和 config.yaml 是否完整：
```bash
ls -la ~/Library/Application\ Support/agent-tools/
cat ~/Library/Application\ Support/agent-tools/config.yaml
```

### 同步时报「skill 路径不属于来源根目录」

旧软链没清理干净，target 里残留断链。先按方案 B 步骤 2 清理断链，再同步。

### 日志位置

`~/Library/Application Support/agent-tools/logs/app.log`（macOS）/ `~/.config/agent-tools/logs/app.log`（Linux）。
