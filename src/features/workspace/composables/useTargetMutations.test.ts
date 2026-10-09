import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import {
  createWorkspaceTarget,
  getTargetPresets,
  getWorkspaceState,
  updateWorkspaceTarget,
} from "../api";
import type { TargetPreset } from "../types";
import { AVAILABLE_PROJECT_AGENTS } from "../model";
import { useWorkspaceStore } from "../stores/workspace";
import { useTargetMutations } from "./useTargetMutations";

vi.mock("../api", () => ({
  createWorkspaceTarget: vi.fn(),
  deleteWorkspaceTarget: vi.fn(),
  getTargetPresets: vi.fn(),
  getWorkspaceState: vi.fn(),
  selectTargetMcpConfigFile: vi.fn(),
  selectTargetSkillDirectory: vi.fn(),
  updateWorkspaceTarget: vi.fn(),
}));

beforeEach(() => {
  setActivePinia(createPinia());
  vi.resetAllMocks();
  vi.mocked(getWorkspaceState).mockResolvedValue({ document: null, inspection: null });
});

// 七个内置工具各一条预设；路径模拟 env 解析后的真实值（含 grokbuild/pi
// 的重定向路径，前端静态预设无法预填的根因）。
function mockPresets(): TargetPreset[] {
  return [
    { targetId: "codex", label: "Codex", enabled: true, skillDir: "/home/.agents/skills", configPath: "/home/.codex/config.toml", mcpConfigPrefix: "mcp_servers" },
    { targetId: "claude", label: "Claude Code", enabled: true, skillDir: "/home/.claude/skills", configPath: "/home/.claude.json", mcpConfigPrefix: "mcpServers" },
    { targetId: "opencode", label: "OpenCode", enabled: true, skillDir: "/home/.config/opencode/skills", configPath: "/home/.config/opencode/opencode.json", mcpConfigPrefix: "mcp.servers" },
    { targetId: "zcode", label: "ZCode", enabled: true, skillDir: "/home/.zcode/skills", configPath: "/home/.zcode/cli/config.json", mcpConfigPrefix: "mcp.servers" },
    { targetId: "grokbuild", label: "Grok Build", enabled: true, skillDir: "/custom-grok/skills", configPath: "/custom-grok/config.toml", mcpConfigPrefix: "mcp_servers" },
    { targetId: "pi", label: "Pi", enabled: true, skillDir: "/custom-pi/skills", configPath: "/custom-pi/mcp.json", mcpConfigPrefix: "mcpServers" },
    { targetId: "dsh", label: "DeepSeek Harness", enabled: true, skillDir: "/home/.dsh/skills", configPath: "/home/.dsh/cordis.patch.yml", mcpConfigPrefix: "" },
  ];
}

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

it("loads backend presets when the create dialog opens and fills the form on click", async () => {
  vi.mocked(getTargetPresets).mockResolvedValue(mockPresets());
  const mutations = useTargetMutations();
  await mutations.openTargetCreateDialog();
  expect(getTargetPresets).toHaveBeenCalledTimes(1);
  expect(mutations.targetCreateDialog.presets.map((preset) => preset.label)).toContain("Grok Build");

  // grokbuild / pi 的路径来自下发值（env 解析后的绝对路径）。
  await mutations.handleApplyBuiltinTargetPreset("grokbuild");
  expect(mutations.targetCreateDialog.form).toMatchObject({
    targetId: "grokbuild",
    enabled: true,
    skillDir: "/custom-grok/skills",
    configPath: "/custom-grok/config.toml",
    mcpConfigPrefix: "mcp_servers",
  });
});

it("keeps explicit edits when submitting a preset-filled target", async () => {
  vi.mocked(getTargetPresets).mockResolvedValue(mockPresets());
  vi.mocked(createWorkspaceTarget).mockResolvedValue({ targetId: "grokbuild", updatedPaths: [] });
  const mutations = useTargetMutations();
  await mutations.openTargetCreateDialog();
  await mutations.handleApplyBuiltinTargetPreset("grokbuild");
  mutations.targetCreateDialog.form.skillDir = "/explicit/skills";
  mutations.targetCreateDialog.form.configPath = "/explicit/config.toml";
  await mutations.handleSubmitTarget();
  expect(createWorkspaceTarget).toHaveBeenCalledWith({
    targetId: "grokbuild", enabled: true, skillDir: "/explicit/skills", configPath: "/explicit/config.toml",
    mcpConfigPrefix: "mcp_servers",
  });
});

it("surfaces a preset load failure with a retry that restores the list", async () => {
  vi.mocked(getTargetPresets).mockRejectedValue(new Error("unavailable"));
  const mutations = useTargetMutations();
  await mutations.openTargetCreateDialog();
  expect(mutations.targetCreateDialog.presetsError).toContain("unavailable");
  expect(mutations.targetCreateDialog.presets).toEqual([]);
  expect(mutations.targetCreateDialog.presetsLoading).toBe(false);

  // 点击预设前表单保持空白，不会被错误数据污染。
  await mutations.handleApplyBuiltinTargetPreset("grokbuild");
  expect(mutations.targetCreateDialog.form.targetId).toBe("");

  vi.mocked(getTargetPresets).mockResolvedValue(mockPresets());
  await mutations.loadTargetPresets();
  expect(mutations.targetCreateDialog.presetsError).toBeNull();
  expect(mutations.targetCreateDialog.presets).toHaveLength(7);
  await mutations.handleApplyBuiltinTargetPreset("grokbuild");
  expect(mutations.targetCreateDialog.form.skillDir).toBe("/custom-grok/skills");
});

it("registers dsh as a preset that submits without configPrefix", async () => {
  // dsh 用户级 target 含全局 Cordis patch，列表同样提供。
  expect(AVAILABLE_PROJECT_AGENTS).toContain("dsh");
  vi.mocked(getTargetPresets).mockResolvedValue(mockPresets());
  vi.mocked(createWorkspaceTarget).mockResolvedValue({ targetId: "dsh", updatedPaths: [] });
  const mutations = useTargetMutations();
  await mutations.openTargetCreateDialog();
  await mutations.handleApplyBuiltinTargetPreset("dsh");
  await mutations.handleSubmitTarget();
  expect(createWorkspaceTarget).toHaveBeenCalledWith({
    targetId: "dsh",
    enabled: true,
    skillDir: "/home/.dsh/skills",
    configPath: "/home/.dsh/cordis.patch.yml",
    mcpConfigPrefix: "",
  });
});

it("carries the mcp format description when editing an existing target", async () => {
  vi.mocked(updateWorkspaceTarget).mockResolvedValue({ targetId: "my-gateway", updatedPaths: [] });
  const mutations = useTargetMutations();
  mutations.openTargetEditDialog({
    id: "my-gateway",
    enabled: true,
    skillDir: "/gateway/skills",
    configPath: "/gateway/config.json",
    mcpConfigPrefix: "mcpServers",
    mcpFormatDescription: "通用 MCP 条目（标准 command/args/env），按配置文件扩展名写入 JSON 或 TOML 的 mcp 节点下。",
    mcpFormatExample: "{\n  \"mcpServers\": {\n    \"my-server\": { ... }\n  }\n}",
  });

  // 编辑模式不拉预设，格式说明来自 target 视图。
  expect(getTargetPresets).not.toHaveBeenCalled();
  expect(mutations.targetCreateDialog.mcpFormatDescription).toContain("通用 MCP 条目");
  expect(mutations.targetCreateDialog.mcpFormatExample).toContain("mcpServers");
  expect(mutations.targetCreateDialog.form.originalTargetId).toBe("my-gateway");

  await mutations.handleSubmitTarget();
  expect(updateWorkspaceTarget).toHaveBeenCalledWith("my-gateway", {
    targetId: "my-gateway",
    enabled: true,
    skillDir: "/gateway/skills",
    configPath: "/gateway/config.json",
    mcpConfigPrefix: "mcpServers",
  });
});

it("reports missing required target fields as field errors without calling the api", async () => {
  vi.mocked(getTargetPresets).mockResolvedValue(mockPresets());
  const mutations = useTargetMutations();
  await mutations.openTargetCreateDialog();

  await mutations.handleSubmitTarget();

  expect(mutations.targetCreateDialog.form.errors).toMatchObject({
    targetId: "请填写 target id。",
    skillDir: "请填写 skills 目录。",
  });
  expect(createWorkspaceTarget).not.toHaveBeenCalled();
});

it("clears a field error once the user edits that field", async () => {
  vi.mocked(getTargetPresets).mockResolvedValue(mockPresets());
  const mutations = useTargetMutations();
  await mutations.openTargetCreateDialog();
  await mutations.handleSubmitTarget();

  mutations.clearTargetFormError("targetId");

  expect(mutations.targetCreateDialog.form.errors.targetId).toBeUndefined();
  expect(mutations.targetCreateDialog.form.errors.skillDir).toBeDefined();
});

it("keeps a failed enable/disable on the panel result slot", async () => {
  vi.mocked(getWorkspaceState).mockResolvedValue({ document: null, inspection: null });
  vi.mocked(updateWorkspaceTarget).mockRejectedValue(new Error("toggle boom"));
  const store = useWorkspaceStore();
  const mutations = useTargetMutations();

  await mutations.toggleTargetEnabled({
    id: "codex",
    enabled: true,
    skillDir: "~/.agents/skills",
    configPath: null,
    mcpConfigPrefix: "",
    mcpFormatDescription: "通用 MCP 条目（标准 command/args/env），按配置文件扩展名写入 JSON 或 TOML 的 mcp 节点下。",
    mcpFormatExample: "{\n  \"mcpServers\": {\n    \"my-server\": { ... }\n  }\n}",
  });

  expect(store.actionResults["target-toggle:codex"]).toMatchObject({
    message: "toggle boom",
    failed: true,
  });
});

it("marks only the toggled card as pending and keeps the page lock off", async () => {
  const store = useWorkspaceStore();
  const mutations = useTargetMutations();
  const runningActionDuringWrite: boolean[] = [];
  let pendingDuringWrite: string[] = [];
  vi.mocked(updateWorkspaceTarget).mockImplementation(async (targetId) => {
    runningActionDuringWrite.push(store.runningAction);
    pendingDuringWrite = [...mutations.pendingToggleTargetIds.value];
    return { targetId, updatedPaths: [] };
  });

  await mutations.toggleTargetEnabled({
    id: "codex",
    enabled: true,
    skillDir: "~/.agents/skills",
    configPath: null,
    mcpConfigPrefix: "",
    mcpFormatDescription: "通用 MCP 条目（标准 command/args/env），按配置文件扩展名写入 JSON 或 TOML 的 mcp 节点下。",
    mcpFormatExample: "{\n  \"mcpServers\": {\n    \"my-server\": { ... }\n  }\n}",
  });

  expect(runningActionDuringWrite).toEqual([false]);
  expect(pendingDuringWrite).toEqual(["codex"]);
  expect([...mutations.pendingToggleTargetIds.value]).toEqual([]);
  expect(store.runningAction).toBe(false);
});

it("runs queued enable/disable writes in order", async () => {
  const first = deferred();
  const second = deferred();
  const order: string[] = [];
  vi.mocked(updateWorkspaceTarget).mockImplementation(async (targetId) => {
    order.push(`start:${targetId}`);
    await (targetId === "codex" ? first.promise : second.promise);
    order.push(`done:${targetId}`);
    return { targetId, updatedPaths: [] };
  });

  const mutations = useTargetMutations();
  const toggle = (id: "codex" | "zcode") =>
    mutations.toggleTargetEnabled({
      id,
      enabled: true,
      skillDir: "~/.agents/skills",
      configPath: null,
      mcpConfigPrefix: "",
      mcpFormatDescription: "通用 MCP 条目（标准 command/args/env），按配置文件扩展名写入 JSON 或 TOML 的 mcp 节点下。",
      mcpFormatExample: "{\n  \"mcpServers\": {\n    \"my-server\": { ... }\n  }\n}",
    });

  const codexToggle = toggle("codex");
  const zcodeToggle = toggle("zcode");
  expect([...mutations.pendingToggleTargetIds.value].sort()).toEqual(["codex", "zcode"]);

  first.resolve();
  await codexToggle;
  // 第二次写入不得在第一次完成前开始，否则两次重读会互相覆盖快照
  expect(order.slice(0, 2)).toEqual(["start:codex", "done:codex"]);

  second.resolve();
  await zcodeToggle;
  expect(order).toEqual(["start:codex", "done:codex", "start:zcode", "done:zcode"]);
  expect([...mutations.pendingToggleTargetIds.value]).toEqual([]);
});
