import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { applyMcpToTarget, getWorkspaceState, updateWorkspaceMcp } from "../api";
import { useWorkspaceStore } from "../stores/workspace";
import { useMcpMutations } from "./useMcpMutations";

vi.mock("../api", () => ({
  applyMcpToTarget: vi.fn(),
  createWorkspaceMcp: vi.fn(),
  deleteWorkspaceMcp: vi.fn(),
  getWorkspaceState: vi.fn(),
  previewMcpTarget: vi.fn(),
  removeMcpFromTarget: vi.fn(),
  updateWorkspaceMcp: vi.fn(),
}));

const mockedGetWorkspaceState = vi.mocked(getWorkspaceState);
const mockedUpdateWorkspaceMcp = vi.mocked(updateWorkspaceMcp);
const mockedApplyMcpToTarget = vi.mocked(applyMcpToTarget);

// 编辑已有 mcp 的前提：检查结果里该 mcp 已安装到 codex。
function inspectionWithInstalledTarget() {
  return {
    configPath: "/tmp/config.yaml",
    targets: [],
    projects: [],
    warnings: [],
    mcps: [
      {
        name: "filesystem",
        enabled: true,
        targets: [
          {
            targetId: "codex" as const,
            configPath: "/tmp/codex.toml",
            configExists: true,
            state: "present",
            detail: "",
          },
        ],
      },
    ],
  };
}

function setup() {
  const store = useWorkspaceStore();
  store.setInspection(inspectionWithInstalledTarget());
  const mutations = useMcpMutations();
  return { store, mutations };
}

function openExistingMcp(mutations: ReturnType<typeof useMcpMutations>) {
  mutations.openMcpEditDialog({
    name: "filesystem",
    enabled: true,
    transport: "stdio",
    createdAt: 1,
    homepage: null,
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem"],
    env: {},
    url: null,
    headers: {},
    timeout: null,
  });
}

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
  mockedGetWorkspaceState.mockResolvedValue({ document: null, inspection: null });
  mockedUpdateWorkspaceMcp.mockResolvedValue({
    serverName: "filesystem",
    updatedPaths: [],
  });
});

it("saves directly without the rename dialog when the name is unchanged", async () => {
  const { mutations } = setup();
  openExistingMcp(mutations);
  mutations.mcpCreateDialog.form.command = "uvx";

  await mutations.handleCreateWorkspaceMcp();

  expect(mutations.mcpSyncConfirmDialog.open).toBe(false);
  expect(mockedUpdateWorkspaceMcp).toHaveBeenCalledWith(
    "filesystem",
    expect.objectContaining({ name: "filesystem", command: "uvx" }),
  );
  expect(mutations.mcpCreateDialog.open).toBe(false);
});

it("asks to sync installed targets only when the name actually changes", async () => {
  const { mutations } = setup();
  openExistingMcp(mutations);
  mutations.mcpCreateDialog.form.name = "filesystem-renamed";

  await mutations.handleCreateWorkspaceMcp();

  expect(mutations.mcpSyncConfirmDialog).toMatchObject({
    open: true,
    originalName: "filesystem",
    nextName: "filesystem-renamed",
    targetIds: ["codex"],
  });
  // 确认前不能落库：用户取消时不应产生半套写入。
  expect(mockedUpdateWorkspaceMcp).not.toHaveBeenCalled();
});

it("saves a renamed mcp directly when it is not installed anywhere", async () => {
  const { store, mutations } = setup();
  store.setInspection({ ...inspectionWithInstalledTarget(), mcps: [] });
  openExistingMcp(mutations);
  mutations.mcpCreateDialog.form.name = "filesystem-renamed";

  await mutations.handleCreateWorkspaceMcp();

  expect(mutations.mcpSyncConfirmDialog.open).toBe(false);
  expect(mockedUpdateWorkspaceMcp).toHaveBeenCalledWith(
    "filesystem",
    expect.objectContaining({ name: "filesystem-renamed" }),
  );
});

describe("卡片上的同步到目标", () => {
  const mcpView = {
    name: "filesystem",
    enabled: true,
    transport: "stdio" as const,
    createdAt: 1,
    homepage: null,
    command: "npx",
    args: ["-y", "@modelcontextprotocol/server-filesystem"],
    env: {},
    url: null,
    headers: {},
    timeout: null,
  };

  beforeEach(() => {
    mockedApplyMcpToTarget.mockResolvedValue({
      serverName: "filesystem",
      targetId: "codex",
      updatedPath: null,
      action: "applied",
      detail: "",
    });
  });

  it("opens a confirm dialog listing the installed targets before writing", () => {
    const { mutations } = setup();

    mutations.openMcpCardSyncDialog(mcpView);

    expect(mutations.mcpCardSyncDialog).toMatchObject({
      open: true,
      serverName: "filesystem",
      targetIds: ["codex"],
    });
    // 确认前不写任何目标文件。
    expect(mockedApplyMcpToTarget).not.toHaveBeenCalled();
    expect(mockedUpdateWorkspaceMcp).not.toHaveBeenCalled();
  });

  it("writes the current config to every installed target and reports the result on the card", async () => {
    const { mutations } = setup();
    mutations.openMcpCardSyncDialog(mcpView);

    await mutations.confirmMcpCardSync();

    expect(mockedApplyMcpToTarget).toHaveBeenCalledWith("filesystem", "codex");
    // 卡片同步不重写 config.yaml：payload 就是已落库的同一份配置。
    expect(mockedUpdateWorkspaceMcp).not.toHaveBeenCalled();
    expect(mutations.mcpCardSyncDialog.open).toBe(false);
    expect(mutations.mcpCardSyncNotice.value).toMatchObject({
      serverName: "filesystem",
      text: "已同步到 1 个目标。",
      failed: false,
    });
  });

  it("names the targets that failed instead of dropping the failure", async () => {
    mockedApplyMcpToTarget.mockRejectedValue(new Error("write boom"));
    const { mutations } = setup();
    mutations.openMcpCardSyncDialog(mcpView);

    await mutations.confirmMcpCardSync();

    expect(mutations.mcpCardSyncNotice.value).toMatchObject({
      serverName: "filesystem",
      text: "同步失败：Codex",
      failed: true,
    });
  });

  it("does not open the dialog when the mcp is not installed anywhere", () => {
    const { store, mutations } = setup();
    store.setInspection({ ...inspectionWithInstalledTarget(), mcps: [] });

    mutations.openMcpCardSyncDialog(mcpView);

    expect(mutations.mcpCardSyncDialog.open).toBe(false);
  });
});
