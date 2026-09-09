import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick, reactive } from "vue";
import {
  discoverGitSkills,
  discoverLocalSkills,
  filterDiscoveredSkills,
} from "../api";
import { createSkillDiscoveryPreviewState, type SkillDiscoveryPreviewState } from "../model";
import type { SkillDiscoveryResult } from "../types";
import { useSkillPreview } from "./useSkillPreview";

vi.mock("../api", () => ({
  discoverGitSkills: vi.fn(),
  discoverLocalSkills: vi.fn(),
  filterDiscoveredSkills: vi.fn(),
}));

const mockedDiscoverGitSkills = vi.mocked(discoverGitSkills);
const mockedFilterDiscoveredSkills = vi.mocked(filterDiscoveredSkills);

function makeDiscovery(discoveryId: string): SkillDiscoveryResult {
  return {
    discoveryId,
    repo: "https://example.com/repo.git",
    ref: "main",
    includeNamePatterns: [],
    includePathPatterns: [],
    skills: [],
  };
}

type Host = { open: boolean; preview: SkillDiscoveryPreviewState };

function setupHost(): Host {
  return reactive({ open: true, preview: createSkillDiscoveryPreviewState() });
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useSkillPreview discover + filter pipeline", () => {
  it("applies the discovery directly when no patterns are set", async () => {
    const host = setupHost();
    const { discoverAndFilterSkills } = useSkillPreview(host, "刷新预览失败。");
    const discovery = makeDiscovery("d1");
    mockedDiscoverGitSkills.mockResolvedValue(discovery);

    await discoverAndFilterSkills({ sourceType: "git", repo: "repo", rootPath: "", ref: "main" }, "", "");

    expect(host.preview.discovery?.discoveryId).toBe("d1");
    expect(host.preview.previewLoading).toBe(false);
    expect(mockedFilterDiscoveredSkills).not.toHaveBeenCalled();
  });

  it("drops a stale discovery response when a newer request superseded it", async () => {
    const host = setupHost();
    const { discoverAndFilterSkills } = useSkillPreview(host, "刷新预览失败。");
    let resolveFirst: (value: SkillDiscoveryResult) => void = () => {};
    mockedDiscoverGitSkills
      .mockImplementationOnce(() => new Promise((resolve) => (resolveFirst = resolve)))
      .mockResolvedValueOnce(makeDiscovery("d2"));

    const first = discoverAndFilterSkills(
      { sourceType: "git", repo: "repo", rootPath: "", ref: "main" },
      "",
      "",
    );
    const second = discoverAndFilterSkills(
      { sourceType: "git", repo: "repo", rootPath: "", ref: "main" },
      "",
      "",
    );
    await second;
    resolveFirst(makeDiscovery("d1-stale"));
    await first;

    expect(host.preview.discovery?.discoveryId).toBe("d2");
  });

  it("drops a stale filter response after the guard was invalidated", async () => {
    const host = setupHost();
    const { discoverAndFilterSkills, invalidatePreviewRequests } = useSkillPreview(host, "刷新预览失败。");
    let resolveDiscover!: (value: SkillDiscoveryResult) => void;
    let resolveFilter!: (value: SkillDiscoveryResult) => void;
    mockedDiscoverGitSkills.mockImplementationOnce(
      () => new Promise((resolve) => (resolveDiscover = resolve)) as Promise<SkillDiscoveryResult>,
    );
    mockedFilterDiscoveredSkills.mockImplementationOnce(
      () => new Promise((resolve) => (resolveFilter = resolve)) as Promise<SkillDiscoveryResult>,
    );

    const pending = discoverAndFilterSkills(
      { sourceType: "git", repo: "repo", rootPath: "", ref: "main" },
      "name-*",
      "",
    );
    resolveDiscover(makeDiscovery("d1"));
    // let the discover continuation reach the filter call (await spans two microtasks)
    await vi.waitFor(() => expect(resolveFilter).toBeDefined());

    invalidatePreviewRequests();
    resolveFilter({ ...makeDiscovery("d1"), includeNamePatterns: ["name-*"] });
    await pending;

    expect(host.preview.discovery).toBeNull();
  });

  it("does not apply a filter result when the pattern text changed meanwhile", async () => {
    const host = setupHost();
    const { discoverAndFilterSkills } = useSkillPreview(host, "刷新预览失败。");
    let resolveDiscover!: (value: SkillDiscoveryResult) => void;
    let resolveFilter!: (value: SkillDiscoveryResult) => void;
    mockedDiscoverGitSkills.mockImplementationOnce(
      () => new Promise((resolve) => (resolveDiscover = resolve)) as Promise<SkillDiscoveryResult>,
    );
    mockedFilterDiscoveredSkills.mockImplementationOnce(
      () => new Promise((resolve) => (resolveFilter = resolve)) as Promise<SkillDiscoveryResult>,
    );

    const pending = discoverAndFilterSkills(
      { sourceType: "git", repo: "repo", rootPath: "", ref: "main" },
      "old-*",
      "",
    );
    resolveDiscover(makeDiscovery("d1"));
    // let the discover continuation reach the filter call (await spans two microtasks)
    await vi.waitFor(() => expect(resolveFilter).toBeDefined());

    host.preview.includeNamePatternsText = "new-*";
    resolveFilter({ ...makeDiscovery("d1"), includeNamePatterns: ["old-*"] });
    await pending;

    expect(host.preview.discovery).toBeNull();
  });
});

describe("useSkillPreview debounced pattern re-filter", () => {
  it("coalesces rapid pattern edits into a single filter call", async () => {
    vi.useFakeTimers();
    const host = setupHost();
    useSkillPreview(host, "刷新预览失败。");
    host.preview.discovery = makeDiscovery("d1");
    await nextTick();

    host.preview.includeNamePatternsText = "a-*";
    await nextTick();
    host.preview.includeNamePatternsText = "a-*, b-*";
    await nextTick();

    expect(mockedFilterDiscoveredSkills).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(299);
    expect(mockedFilterDiscoveredSkills).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(mockedFilterDiscoveredSkills).toHaveBeenCalledTimes(1);
    expect(mockedFilterDiscoveredSkills).toHaveBeenCalledWith("d1", ["a-*", "b-*"], []);
  });

  it("skips the filter call when the loaded discovery already matches the patterns", async () => {
    vi.useFakeTimers();
    const host = setupHost();
    useSkillPreview(host, "刷新预览失败。");
    host.preview.discovery = { ...makeDiscovery("d1"), includeNamePatterns: ["x-*"] };
    await nextTick();

    host.preview.includeNamePatternsText = "x-*";
    await nextTick();
    await vi.advanceTimersByTimeAsync(300);

    expect(mockedFilterDiscoveredSkills).not.toHaveBeenCalled();
    expect(host.preview.previewLoading).toBe(false);
  });
});
