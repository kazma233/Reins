import { beforeEach, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { createWorkspaceTarget, getBuiltinTargetPreset, getWorkspaceState } from "../api";
import { AVAILABLE_PROJECT_AGENTS, BUILTIN_TARGET_PRESETS, formatMcpConfigType } from "../model";
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
