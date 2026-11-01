# Target 配置路径

应用向以下位置分发 Skills 与 MCP 配置，是 workspace target 的读写契约。

## OpenCode

- 全局 skills：`~/.config/opencode/skills/<name>/SKILL.md`
- 项目 skills：`<project>/.opencode/skills/<name>/SKILL.md`
- 全局 MCP：`~/.config/opencode/opencode.json`
- 项目 MCP：`<project>/opencode.json`

## ZCode

- 全局 skills：`~/.zcode/skills/<name>/SKILL.md`
- 项目 skills：`<project>/.zcode/skills/<name>/SKILL.md`
- 全局 MCP：`~/.zcode/cli/config.json` 的 `mcp.servers`
- 项目 MCP：`<project>/.zcode/config.json` 的 `mcp.servers`

节点路径相同不代表字段名相同（例如 Grok 的远端 `headers`、timeout 类型与 Codex 不一致），字段差异与后续动态映射方向见 `2026-09-15-mcp-config-mapping.md`。
