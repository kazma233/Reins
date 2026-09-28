import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { getProviderAppState, getProvidersState } from "../api";
import type { ProviderAppId, ProviderAppState, ProvidersState } from "../generated";
import { useProvidersStore } from "../stores/providers";
import { useProvidersAction } from "./useProvidersAction";
import { useProvidersNotice } from "./useProvidersNotice";

// useProvidersAction 只经这两个 api 读状态；mock 掉以免引入 Tauri invoke。
vi.mock("../api", () => ({
  getProvidersState: vi.fn(),
  getProviderAppState: vi.fn(),
}));

const mockedGetProvidersState = vi.mocked(getProvidersState);
const mockedGetProviderAppState = vi.mocked(getProviderAppState);

function appState(app: ProviderAppId): ProviderAppState {
  return {
    app,
    configPaths: [`$HOME/${app}`],
    configExists: false,
    supportedProtocols: [],
    supportedReasoningLevels: [],
    reasoningLevelWrites: [],
    additive: true,
    unwrittenModelFields: [],
    entries: [],
  };
}

function initialState(): ProvidersState {
  return {
    configPath: "providers.yaml",
    providers: [],
    apps: [appState("codex"), appState("grokbuild")],
  };
}

function setup() {
  const store = useProvidersStore();
  const { notice } = useProvidersNotice();
  const { runProvidersAction } = useProvidersAction();
  return { store, notice, runProvidersAction };
}

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
  useProvidersNotice().clearNotice();
});

describe("runProvidersAction refresh scope", () => {
  it("refreshes only the targeted app: no full reload, no whole-page loading", async () => {
    const { store, notice, runProvidersAction } = setup();
    store.setState(initialState());
    const refreshed = { ...appState("grokbuild"), configExists: true };
    mockedGetProviderAppState.mockResolvedValue(refreshed);

    await runProvidersAction({
      action: async () => ({ detail: "已应用。" }),
      success: (result) => ({ message: result.detail }),
      refreshApp: "grokbuild",
    });

    expect(mockedGetProviderAppState).toHaveBeenCalledWith("grokbuild");
    expect(mockedGetProvidersState).not.toHaveBeenCalled();
    // 整页 loading 未被触发，卡片列表保持挂载。
    expect(store.loadingState).toBe(false);
    expect(store.state?.apps.map((item) => item.app)).toEqual(["codex", "grokbuild"]);
    expect(store.state?.apps[1]).toMatchObject({ configExists: true });
    expect(store.state?.apps[0]).toMatchObject({ configExists: false });
    expect(store.runningAction).toBe(false);
    expect(notice.value).toMatchObject({ message: "已应用。", tone: "success" });
  });

  it("leaves the card untouched when the targeted refresh fails", async () => {
    const { store, notice, runProvidersAction } = setup();
    store.setState(initialState());
    mockedGetProviderAppState.mockRejectedValue(new Error("boom"));

    await runProvidersAction({
      action: async () => null,
      success: "已删除。",
      refreshApp: "codex",
    });

    expect(store.state?.apps[0]).toMatchObject({ configExists: false });
    expect(store.runningAction).toBe(false);
    expect(notice.value).toMatchObject({ message: "已删除。" });
  });

  it("still reloads the whole state when no app is targeted", async () => {
    const { store, runProvidersAction } = setup();
    store.setState(initialState());
    mockedGetProvidersState.mockResolvedValue(initialState());

    await runProvidersAction({
      action: async () => "payload",
      success: "已保存。",
    });

    expect(mockedGetProvidersState).toHaveBeenCalledTimes(1);
    expect(mockedGetProviderAppState).not.toHaveBeenCalled();
  });
});
