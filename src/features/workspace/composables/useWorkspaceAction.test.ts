import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { getWorkspaceState } from "../api";
import { useWorkspaceStore } from "../stores/workspace";
import { useWorkspaceAction } from "./useWorkspaceAction";
import { useWorkspaceNotice } from "./useWorkspaceNotice";

// Only getWorkspaceState is reachable from useWorkspaceAction's import graph
// (via useWorkspaceState); mocking the module keeps Tauri invoke out of tests.
vi.mock("../api", () => ({
  getWorkspaceState: vi.fn(),
}));

const mockedGetWorkspaceState = vi.mocked(getWorkspaceState);

function setup() {
  const store = useWorkspaceStore();
  const { notice, showNotice, clearNotice } = useWorkspaceNotice();
  const { runWorkspaceAction } = useWorkspaceAction();
  return { store, notice, showNotice, clearNotice, runWorkspaceAction };
}

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
  mockedGetWorkspaceState.mockResolvedValue({ document: null, inspection: null });
  useWorkspaceNotice().clearNotice();
});

describe("runWorkspaceAction success path", () => {
  it("locks runningAction during the action and resets it in the end", async () => {
    const { store, runWorkspaceAction } = setup();
    const observed: boolean[] = [];

    await runWorkspaceAction({
      action: async () => {
        observed.push(store.runningAction);
        return "payload";
      },
      success: "done",
      error: "fallback",
    });

    expect(observed).toEqual([true]);
    expect(store.runningAction).toBe(false);
  });

  it("reloads between action and toast, so the success toast survives the reload", async () => {
    const { notice, runWorkspaceAction } = setup();
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
      success: (result) => ({ message: `done ${result}` }),
      error: "fallback",
      after: () => order.push("after"),
    });

    expect(order).toEqual(["action", "reload", "after"]);
    expect(mockedGetWorkspaceState).toHaveBeenCalledTimes(1);
    // The toast singleton still holds the success notice after the reload ran.
    expect(notice.value).toMatchObject({ message: "done payload", tone: "success" });
  });

  it("passes the action result to after and to the success resolver", async () => {
    const { runWorkspaceAction } = setup();
    const after = vi.fn();
    const success = vi.fn(() => ({ message: "ok" }));

    await runWorkspaceAction({
      action: async () => 42,
      success,
      error: "fallback",
      after,
    });

    expect(success).toHaveBeenCalledWith(42);
    expect(after).toHaveBeenCalledWith(42);
  });

  it("defaults the success tone to \"success\" and allows an override", async () => {
    const { notice, runWorkspaceAction } = setup();

    await runWorkspaceAction({
      action: async () => ({ noop: true }),
      success: () => ({ message: "nothing to do", tone: "info" }),
      error: "fallback",
    });

    expect(notice.value).toMatchObject({ message: "nothing to do", tone: "info" });
  });

  it("accepts a plain string success message", async () => {
    const { notice, runWorkspaceAction } = setup();

    await runWorkspaceAction({
      action: async () => null,
      success: "已删除。",
      error: "fallback",
    });

    expect(notice.value).toMatchObject({ message: "已删除。", tone: "success" });
  });

  it("skips the workspace reload but still toasts and runs after when asked to", async () => {
    const { store, notice, runWorkspaceAction } = setup();
    const after = vi.fn();

    await runWorkspaceAction({
      action: async () => "payload",
      success: (result) => ({ message: `done ${result}` }),
      error: "fallback",
      skipReload: true,
      after,
    });

    expect(mockedGetWorkspaceState).not.toHaveBeenCalled();
    expect(after).toHaveBeenCalledWith("payload");
    expect(notice.value).toMatchObject({ message: "done payload", tone: "success" });
    expect(store.runningAction).toBe(false);
  });
});

describe("runWorkspaceAction failure path", () => {
  it("toasts extractErrorMessage output, skips after and reload, resets runningAction", async () => {
    const { store, notice, runWorkspaceAction } = setup();
    const after = vi.fn();

    await runWorkspaceAction({
      action: () => {
        expect(store.runningAction).toBe(true);
        return Promise.reject(new Error("boom"));
      },
      success: "ok",
      error: "fallback",
      after,
    });

    expect(store.runningAction).toBe(false);
    expect(notice.value).toMatchObject({ message: "boom", tone: "error" });
    expect(after).not.toHaveBeenCalled();
    expect(mockedGetWorkspaceState).not.toHaveBeenCalled();
  });

  it("falls back to the declared error text when the error carries no message", async () => {
    const { notice, runWorkspaceAction } = setup();

    await runWorkspaceAction({
      action: () => Promise.reject(undefined),
      success: "ok",
      error: "删除失败。",
    });

    expect(notice.value).toMatchObject({ message: "删除失败。", tone: "error" });
  });
});

describe("runWorkspaceAction without success (side-effect task)", () => {
  it("does not reload or toast, but still runs after and resets runningAction", async () => {
    const { store, notice, runWorkspaceAction } = setup();
    const after = vi.fn();

    await runWorkspaceAction({
      action: async () => "picked",
      error: "选择失败。",
      after: (result) => after(result),
    });

    expect(mockedGetWorkspaceState).not.toHaveBeenCalled();
    expect(notice.value).toBeNull();
    expect(after).toHaveBeenCalledWith("picked");
    expect(store.runningAction).toBe(false);
  });
});

describe("runWorkspaceAction notice hygiene", () => {
  it("clears a stale notice before running", async () => {
    const { notice, showNotice, runWorkspaceAction } = setup();
    showNotice("旧提示", "info");

    await runWorkspaceAction({
      action: () => Promise.resolve(null),
      error: "fallback",
    });

    expect(notice.value).toBeNull();
  });
});
