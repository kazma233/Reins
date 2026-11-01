import { beforeEach, describe, expect, it, vi } from "vitest";
import { getWorkspaceState } from "../api";
import { useWorkspaceAction } from "./useWorkspaceAction";
import { useWorkspaceState } from "./useWorkspaceState";

// Only getWorkspaceState is reachable from useWorkspaceAction's import graph
// (via useWorkspaceState); mocking the module keeps Tauri invoke out of tests.
vi.mock("../api", () => ({
  getWorkspaceState: vi.fn(),
}));

const mockedGetWorkspaceState = vi.mocked(getWorkspaceState);

function setup() {
  const workspace = useWorkspaceState();
  const { runWorkspaceAction } = useWorkspaceAction(workspace);
  return { workspace, runWorkspaceAction };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedGetWorkspaceState.mockResolvedValue({ document: null, inspection: null });
});

describe("runWorkspaceAction success path", () => {
  it("locks runningAction during the action and resets it in the end", async () => {
    const { workspace, runWorkspaceAction } = setup();
    const observed: boolean[] = [];

    await runWorkspaceAction({
      action: async () => {
        observed.push(workspace.runningAction.value);
        return "payload";
      },
      reload: true,
      error: "fallback",
    });

    expect(observed).toEqual([true]);
    expect(workspace.runningAction.value).toBe(false);
  });

  it("reloads before onSuccess so callers see fresh state", async () => {
    const { runWorkspaceAction } = setup();
    const order: string[] = [];
    mockedGetWorkspaceState.mockImplementation(async () => {
      order.push("reload");
      return { document: null, inspection: null };
    });

    await runWorkspaceAction({
      action: async () => {
        order.push("action");
        return "payload";
      },
      reload: true,
      error: "fallback",
      onSuccess: () => order.push("onSuccess"),
    });

    expect(order).toEqual(["action", "reload", "onSuccess"]);
    expect(mockedGetWorkspaceState).toHaveBeenCalledTimes(1);
  });

  it("passes the action result to onSuccess", async () => {
    const { runWorkspaceAction } = setup();
    const onSuccess = vi.fn();

    await runWorkspaceAction({
      action: async () => 42,
      reload: true,
      error: "fallback",
      onSuccess,
    });

    expect(onSuccess).toHaveBeenCalledWith(42);
  });

  it("skips the workspace reload but still runs onSuccess when asked to", async () => {
    const { workspace, runWorkspaceAction } = setup();
    const onSuccess = vi.fn();

    await runWorkspaceAction({
      action: async () => "payload",
      error: "fallback",
      onSuccess,
    });

    expect(mockedGetWorkspaceState).not.toHaveBeenCalled();
    expect(onSuccess).toHaveBeenCalledWith("payload");
    expect(workspace.runningAction.value).toBe(false);
  });

  it("does not surface a load error when the action itself succeeded", async () => {
    const { workspace, runWorkspaceAction } = setup();

    await runWorkspaceAction({
      action: async () => null,
      reload: true,
      error: "fallback",
    });

    expect(workspace.loadError.value).toBeNull();
  });

  it("keeps the page out of its loading state while reloading after the action", async () => {
    const { workspace, runWorkspaceAction } = setup();
    const observed: boolean[] = [];
    mockedGetWorkspaceState.mockImplementation(async () => {
      observed.push(workspace.loadingConfig.value || workspace.loadingInspection.value);
      return { document: null, inspection: null };
    });

    await runWorkspaceAction({
      action: async () => null,
      reload: true,
      error: "fallback",
    });

    expect(observed).toEqual([false]);
    expect(workspace.loadingConfig.value).toBe(false);
    expect(workspace.loadingInspection.value).toBe(false);
  });

  it("keeps the page lock off for card-scoped actions", async () => {
    const { workspace, runWorkspaceAction } = setup();
    const observed: boolean[] = [];
    mockedGetWorkspaceState.mockImplementation(async () => {
      observed.push(workspace.runningAction.value);
      return { document: null, inspection: null };
    });

    await runWorkspaceAction({
      action: async () => {
        observed.push(workspace.runningAction.value);
        return null;
      },
      reload: true,
      pageLock: false,
      error: "fallback",
    });

    expect(observed).toEqual([false, false]);
    expect(mockedGetWorkspaceState).toHaveBeenCalledTimes(1);
    expect(workspace.runningAction.value).toBe(false);
  });
});

describe("runWorkspaceAction failure path", () => {
  it("hands the extracted message to onError, skips reload and onSuccess, resets runningAction", async () => {
    const { workspace, runWorkspaceAction } = setup();
    const onSuccess = vi.fn();
    const onError = vi.fn();

    await runWorkspaceAction({
      action: () => {
        expect(workspace.runningAction.value).toBe(true);
        return Promise.reject(new Error("boom"));
      },
      reload: true,
      error: "fallback",
      onSuccess,
      onError,
    });

    expect(workspace.runningAction.value).toBe(false);
    expect(onError).toHaveBeenCalledWith("boom");
    expect(onSuccess).not.toHaveBeenCalled();
    expect(mockedGetWorkspaceState).not.toHaveBeenCalled();
  });

  it("falls back to the declared error text when the error carries no message", async () => {
    const { runWorkspaceAction } = setup();
    const onError = vi.fn();

    await runWorkspaceAction({
      action: () => Promise.reject(undefined),
      error: "删除失败。",
      onError,
    });

    expect(onError).toHaveBeenCalledWith("删除失败。");
  });

  it("swallows the failure when no onError is provided", async () => {
    const { workspace, runWorkspaceAction } = setup();

    await expect(
      runWorkspaceAction({
        action: () => Promise.reject(new Error("boom")),
        error: "fallback",
      }),
    ).resolves.toBeUndefined();

    expect(workspace.runningAction.value).toBe(false);
  });
});
