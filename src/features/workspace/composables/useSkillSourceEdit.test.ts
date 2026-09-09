import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { getWorkspaceState, refreshGitSkillSource } from "../api";
import type { SkillSourceConfigView } from "../types";
import { useSkillSourceEdit } from "./useSkillSourceEdit";

vi.mock("../api", () => ({
  deleteSkillSource: vi.fn(),
  discoverGitSkills: vi.fn(),
  discoverLocalSkills: vi.fn(),
  filterDiscoveredSkills: vi.fn(),
  getWorkspaceState: vi.fn(),
  refreshGitSkillSource: vi.fn(),
  updateSkillSource: vi.fn(),
}));

const mockedGetWorkspaceState = vi.mocked(getWorkspaceState);
const mockedRefreshGitSkillSource = vi.mocked(refreshGitSkillSource);

const gitSource: SkillSourceConfigView = {
  type: "git",
  id: "remote-skills",
  repo: "https://github.com/acme/skills.git",
  ref: "main",
  includeNamePatterns: [],
  includePathPatterns: [],
  label: "https://github.com/acme/skills.git · main",
};

const localSource: SkillSourceConfigView = {
  type: "local",
  id: "local-skills",
  rootPath: "C:/skills",
  includeNamePatterns: [],
  includePathPatterns: [],
  label: "C:/skills",
};

beforeEach(() => {
  setActivePinia(createPinia());
  vi.clearAllMocks();
  mockedGetWorkspaceState.mockResolvedValue({ document: null, inspection: null });
  mockedRefreshGitSkillSource.mockResolvedValue(undefined);
});

describe("useSkillSourceEdit git refresh", () => {
  it("forces a refresh for git sources and reloads workspace state", async () => {
    const { handleRefreshGitSkillSource } = useSkillSourceEdit();

    await handleRefreshGitSkillSource(gitSource);

    expect(mockedRefreshGitSkillSource).toHaveBeenCalledWith("remote-skills");
    expect(mockedGetWorkspaceState).toHaveBeenCalledTimes(1);
  });

  it("does not call the git refresh command for local sources", async () => {
    const { handleRefreshGitSkillSource } = useSkillSourceEdit();

    await handleRefreshGitSkillSource(localSource);

    expect(mockedRefreshGitSkillSource).not.toHaveBeenCalled();
    expect(mockedGetWorkspaceState).not.toHaveBeenCalled();
  });
});
