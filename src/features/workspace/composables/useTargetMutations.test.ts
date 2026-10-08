import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import {
  createWorkspaceTarget,
  getBuiltinTargetPreset,
  getWorkspaceState,
  updateWorkspaceTarget,
} from "../api";
import { AVAILABLE_PROJECT_AGENTS, BUILTIN_TARGET_PRESETS, formatMcpConfigType } from "../model";
import { useWorkspaceStore } from "../stores/workspace";
import { useTargetMutations } from "./useTargetMutations";

vi.mock("../api", () => ({
  createWorkspaceTarget: vi.fn(),
  deleteWorkspaceTarget: vi.fn(),
  getBuiltinTargetPreset: vi.fn(),
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

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

it("uses backend Grok paths and preserves explicit edits when creating a target", async () => {
  vi.mocked(getBuiltinTargetPreset).mockResolvedValue({
    id: "grokbuild", enabled: true, skillDir: "/custom-grok/skills",
    configPath: "/custom-grok/config.toml", mcpConfigPrefix: "mcp_servers", mcpConfigType: "grokbuild",
  });
  vi.mocked(createWorkspaceTarget).mockResolvedValue({ targetId: "grokbuild", updatedPaths: [] });
  const mutations = useTargetMutations();
  mutations.openTargetCreateDialog();
  await mutations.handleApplyBuiltinTargetPreset("grokbuild");
  expect(getBuiltinTargetPreset).toHaveBeenCalledWith("grokbuild");
  expect(mutations.targetCreateDialog.form.skillDir).toBe("/custom-grok/skills");
  expect(mutations.targetCreateDialog.form.configPath).toBe("/custom-grok/config.toml");
  mutations.targetCreateDialog.form.skillDir = "/explicit/skills";
  mutations.targetCreateDialog.form.configPath = "/explicit/config.toml";
  await mutations.handleSubmitTarget();
  expect(createWorkspaceTarget).toHaveBeenCalledWith({
    targetId: "grokbuild", enabled: true, skillDir: "/explicit/skills", configPath: "/explicit/config.toml",
    mcpConfigPrefix: "mcp_servers", mcpConfigType: "grokbuild",
  });
});

it("does not substitute hardcoded paths when the backend preset fails", async () => {
  vi.mocked(getBuiltinTargetPreset).mockRejectedValue(new Error("unavailable"));
  const mutations = useTargetMutations();
  mutations.openTargetCreateDialog();
  const before = { ...mutations.targetCreateDialog.form };
  await mutations.handleApplyBuiltinTargetPreset("grokbuild");
  expect(mutations.targetCreateDialog.form).toEqual(before);
  expect(mutations.targetCreateDialog.loading).toBe(false);
});

it("registers Grok project and format options without changing existing static presets", async () => {
  expect(AVAILABLE_PROJECT_AGENTS).toContain("grokbuild");
  expect(formatMcpConfigType("grokbuild")).toContain("Grok Build");
  const mutations = useTargetMutations();
  await mutations.handleApplyBuiltinTargetPreset("codex");
  expect(mutations.targetCreateDialog.form).toMatchObject(BUILTIN_TARGET_PRESETS.codex!);
  expect(getBuiltinTargetPreset).not.toHaveBeenCalled();
});

it("reports missing required target fields as field errors without calling the api", async () => {
  const mutations = useTargetMutations();
  mutations.openTargetCreateDialog();

  await mutations.handleSubmitTarget();

  expect(mutations.targetCreateDialog.form.errors).toMatchObject({
    targetId: "请填写 target id。",
    skillDir: "请填写 skills 目录。",
  });
  expect(createWorkspaceTarget).not.toHaveBeenCalled();
});

it("clears a field error once the user edits that field", async () => {
  const mutations = useTargetMutations();
  mutations.openTargetCreateDialog();
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
    mcpConfigType: "common",
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
    mcpConfigType: "common",
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
      mcpConfigType: "common",
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

it("registers dsh as a user-level preset that skips configPrefix pairing", async () => {
  // dsh 用户级 target 含全局 Cordis patch;项目层只有 skills,列表同样提供。
  expect(AVAILABLE_PROJECT_AGENTS).toContain("dsh");
  expect(formatMcpConfigType("dsh")).toContain("DeepSeek Harness");
  vi.mocked(createWorkspaceTarget).mockResolvedValue({ targetId: "dsh", updatedPaths: [] });
  const mutations = useTargetMutations();
  mutations.openTargetCreateDialog();
  await mutations.handleApplyBuiltinTargetPreset("dsh");
  expect(mutations.targetCreateDialog.form).toMatchObject(BUILTIN_TARGET_PRESETS.dsh!);
  expect(getBuiltinTargetPreset).not.toHaveBeenCalled();
  await mutations.handleSubmitTarget();
  expect(createWorkspaceTarget).toHaveBeenCalledWith({
    targetId: "dsh",
    enabled: true,
    skillDir: "~/.dsh/skills",
    configPath: "~/.dsh/cordis.patch.yml",
    mcpConfigPrefix: "",
    mcpConfigType: "dsh",
  });
});
