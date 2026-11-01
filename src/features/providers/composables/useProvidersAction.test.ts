import { beforeEach, describe, expect, it, vi } from "vitest";
import { getProviderAppState, getProvidersState } from "../api";
import type { ProviderAppId, ProviderAppState, ProvidersState } from "../generated";
import { useProvidersAction } from "./useProvidersAction";
import { useProvidersState } from "./useProvidersState";

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
    behaviorNotes: [],
    entries: [],
    loadError: null,
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
  const providers = useProvidersState();
  const { runProvidersAction } = useProvidersAction(providers);
  return { providers, runProvidersAction };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("runProvidersAction refresh scope", () => {
  it("reloadApp 只刷新目标卡片：不重读整页、不进入整页 loading", async () => {
    const { providers, runProvidersAction } = setup();
    providers.setState(initialState());
    const refreshed = { ...appState("grokbuild"), configExists: true };
    mockedGetProviderAppState.mockResolvedValue(refreshed);

    let runningDuringAction = false;
    await runProvidersAction({
      action: async () => {
        runningDuringAction = providers.runningAction.value;
        return { detail: "已应用。" };
      },
      reloadApp: "grokbuild",
      onSuccess: (result) => {
        // onSuccess 在刷新之后：卡片此刻已是新状态。
        expect(providers.state.value?.apps[1]).toMatchObject({ configExists: true });
        expect(providers.runningAction.value).toBe(true);
        expect(result).toEqual({ detail: "已应用。" });
      },
    });

    expect(runningDuringAction).toBe(true);
    expect(mockedGetProviderAppState).toHaveBeenCalledWith("grokbuild");
    expect(mockedGetProvidersState).not.toHaveBeenCalled();
    // 整页 loading 未被触发，卡片列表保持挂载。
    expect(providers.loadingState.value).toBe(false);
    expect(providers.state.value?.apps.map((item) => item.app)).toEqual(["codex", "grokbuild"]);
    expect(providers.state.value?.apps[0]).toMatchObject({ configExists: false });
    expect(providers.runningAction.value).toBe(false);
  });

  it("reloadApp 刷新失败时保留旧卡片，把错误落到该卡片且不跳过 onSuccess", async () => {
    const { providers, runProvidersAction } = setup();
    providers.setState(initialState());
    mockedGetProviderAppState.mockRejectedValue(new Error("boom"));
    const onSuccess = vi.fn();

    await runProvidersAction({
      action: async () => null,
      reloadApp: "codex",
      onSuccess,
    });

    expect(providers.state.value?.apps[0]).toMatchObject({ configExists: false });
    expect(providers.appErrors.value.codex).toBe("boom");
    expect(onSuccess).toHaveBeenCalledWith(null);
    expect(providers.runningAction.value).toBe(false);
  });

  it("reload 在 onSuccess 之前完成整页刷新", async () => {
    const { providers, runProvidersAction } = setup();
    providers.setState(initialState());
    mockedGetProvidersState.mockResolvedValue({
      ...initialState(),
      configPath: "next.yaml",
    });

    await runProvidersAction({
      action: async () => "payload",
      reload: true,
      onSuccess: (result) => {
        expect(result).toBe("payload");
        expect(providers.state.value?.configPath).toBe("next.yaml");
        expect(providers.runningAction.value).toBe(true);
      },
    });

    expect(mockedGetProvidersState).toHaveBeenCalledTimes(1);
    expect(mockedGetProviderAppState).not.toHaveBeenCalled();
    expect(providers.error.value).toBeNull();
    expect(providers.runningAction.value).toBe(false);
  });

  it("reload 失败写入全局 error，但不跳过 onSuccess", async () => {
    const { providers, runProvidersAction } = setup();
    providers.setState(initialState());
    mockedGetProvidersState.mockRejectedValue(new Error("读取炸了"));
    const onSuccess = vi.fn();

    await runProvidersAction({
      action: async () => "ok",
      reload: true,
      onSuccess,
    });

    expect(providers.error.value).toBe("读取炸了");
    expect(onSuccess).toHaveBeenCalledWith("ok");
    expect(providers.runningAction.value).toBe(false);
  });

  it("不声明刷新时不请求任何状态接口", async () => {
    const { providers, runProvidersAction } = setup();
    providers.setState(initialState());
    const onSuccess = vi.fn();

    await runProvidersAction({ action: async () => "done", onSuccess });

    expect(mockedGetProvidersState).not.toHaveBeenCalled();
    expect(mockedGetProviderAppState).not.toHaveBeenCalled();
    expect(onSuccess).toHaveBeenCalledWith("done");
    expect(providers.runningAction.value).toBe(false);
  });
});

describe("runProvidersAction error handling", () => {
  it("action 失败时把 extractErrorMessage 的结果交给 onError，并复位 runningAction", async () => {
    const { providers, runProvidersAction } = setup();
    providers.setState(initialState());
    const onError = vi.fn();
    const onSuccess = vi.fn();

    await runProvidersAction({
      action: async () => {
        throw new Error("保存接口拒绝");
      },
      error: "保存失败。",
      onError,
      onSuccess,
    });

    expect(onError).toHaveBeenCalledWith("保存接口拒绝");
    expect(onSuccess).not.toHaveBeenCalled();
    expect(mockedGetProvidersState).not.toHaveBeenCalled();
    expect(providers.runningAction.value).toBe(false);
  });

  it("错误不可读时用 error 选项作为兜底文案", async () => {
    const { runProvidersAction } = setup();
    const onError = vi.fn();

    await runProvidersAction({
      action: async () => {
        throw "";
      },
      error: "保存失败。",
      onError,
    });

    expect(onError).toHaveBeenCalledWith("保存失败。");
  });

  it("没有 onError 时失败不抛出、不产生界面副作用", async () => {
    const { providers, runProvidersAction } = setup();

    await expect(
      runProvidersAction({
        action: async () => {
          throw new Error("boom");
        },
      })
    ).resolves.toBeUndefined();

    expect(providers.runningAction.value).toBe(false);
    expect(providers.error.value).toBeNull();
  });
});
