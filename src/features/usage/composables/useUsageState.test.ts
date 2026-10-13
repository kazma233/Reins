import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { ALL_SOURCES } from "../../sessions/source-app";
import { ALL_MODELS } from "../model";
import { useUsageState } from "./useUsageState";

// Only getUsageStats is reachable from useUsageState's import graph;
// mocking the module keeps Tauri invoke out of tests.
vi.mock("../api", () => ({
  getUsageStats: vi.fn(),
}));

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
});

describe("useUsageState 筛选", () => {
  it("切换来源会把模型筛选重置回全部", () => {
    const { selectedSource, selectedModel, setSelectedSource, setSelectedModel } =
      useUsageState();
    expect(selectedSource.value).toBe(ALL_SOURCES);
    expect(selectedModel.value).toBe(ALL_MODELS);

    setSelectedModel("gpt-5.3");
    setSelectedSource("codex");

    expect(selectedSource.value).toBe("codex");
    expect(selectedModel.value).toBe(ALL_MODELS);
  });
});
