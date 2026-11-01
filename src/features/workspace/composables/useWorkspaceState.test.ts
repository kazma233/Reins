import { beforeEach, describe, expect, it, vi } from "vitest";
import { getWorkspaceState } from "../api";
import { useWorkspaceState } from "./useWorkspaceState";

// Only getWorkspaceState is reachable from useWorkspaceState's import graph;
// mocking the module keeps Tauri invoke out of tests.
vi.mock("../api", () => ({
  getWorkspaceState: vi.fn(),
}));

const mockedGetWorkspaceState = vi.mocked(getWorkspaceState);

function setup() {
  const workspace = useWorkspaceState();
  const { reloadWorkspaceState, retryWorkspaceState } = workspace;
  return { workspace, reloadWorkspaceState, retryWorkspaceState };
}

// 面板按这两个标记渲染「正在检查状态」：读取期间进入加载态，列表就会闪动位移。
function captureLoadingDuringRead(workspace: ReturnType<typeof useWorkspaceState>) {
  const observed: { config: boolean; inspection: boolean }[] = [];
  mockedGetWorkspaceState.mockImplementation(async () => {
    observed.push({
      config: workspace.loadingConfig.value,
      inspection: workspace.loadingInspection.value,
    });
    return { document: null, inspection: null };
  });
  return observed;
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedGetWorkspaceState.mockResolvedValue({ document: null, inspection: null });
});

describe("reloadWorkspaceState loading state", () => {
  it("首次加载进入整页加载态，读完复位", async () => {
    const { workspace, reloadWorkspaceState } = setup();
    const observed = captureLoadingDuringRead(workspace);

    await reloadWorkspaceState();

    expect(observed).toEqual([{ config: true, inspection: true }]);
    expect(workspace.loadingConfig.value).toBe(false);
    expect(workspace.loadingInspection.value).toBe(false);
  });

  it("背景重读全程不进入整页加载态", async () => {
    const { workspace, reloadWorkspaceState } = setup();
    const observed = captureLoadingDuringRead(workspace);

    await reloadWorkspaceState({ background: true });

    expect(observed).toEqual([{ config: false, inspection: false }]);
  });

  it("重试读取仍进入加载态（错误条的重试按钮要能显示进度）", async () => {
    const { workspace, retryWorkspaceState } = setup();
    const observed = captureLoadingDuringRead(workspace);

    await retryWorkspaceState();

    expect(observed).toEqual([{ config: true, inspection: true }]);
    expect(workspace.loadingConfig.value).toBe(false);
    expect(workspace.loadingInspection.value).toBe(false);
  });

  it("背景重读照常替换数据并清掉上一次的读取错误", async () => {
    const { workspace, reloadWorkspaceState } = setup();
    workspace.loadError.value = "上一次读取失败。";
    mockedGetWorkspaceState.mockResolvedValue({
      document: {
        configPath: "/tmp/workspace.yaml",
        exists: true,
        rawContent: "",
        config: null,
        validation: { valid: true, errors: [], warnings: [] },
      },
      inspection: null,
    });

    await reloadWorkspaceState({ background: true });

    expect(workspace.configDocument.value?.configPath).toBe("/tmp/workspace.yaml");
    expect(workspace.loadError.value).toBeNull();
  });
});
