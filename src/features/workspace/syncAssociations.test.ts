import { describe, expect, it } from "vitest";
import type { SkillLinkAssociation } from "./types";
import {
  buildSkillLinkSourceLabels,
  hasCurrentSourceSkillLinks,
  hasUnmanagedSkillLinks,
  pendingSkillLinkCleanupCount,
  skillLinkSourceLabel,
  skillLinkSourcePath,
  skillLinkStateInfo,
} from "./syncAssociations";

function makeLink(state: SkillLinkAssociation["state"], matchedSourceIds: string[] = []): SkillLinkAssociation {
  return {
    skillName: "alpha",
    destinationPath: "/target/alpha",
    sourcePath: "/source/alpha",
    matchedSourceIds,
    state,
  };
}

describe("skill link association presentation", () => {
  it("labels every scanned link state", () => {
    expect(skillLinkStateInfo("linked")).toMatchObject({ label: "已关联", pendingCleanup: false });
    expect(skillLinkStateInfo("excluded")).toMatchObject({ pendingCleanup: true });
    expect(skillLinkStateInfo("sourceMissing")).toMatchObject({ pendingCleanup: true });
    expect(skillLinkStateInfo("unmanaged")).toMatchObject({ label: "非受管来源", pendingCleanup: false });
  });

  it("counts only excluded and missing source links as pending cleanup", () => {
    const links = [
      makeLink("linked"),
      makeLink("excluded"),
      makeLink("sourceMissing"),
      makeLink("unmanaged"),
    ];

    expect(pendingSkillLinkCleanupCount(links)).toBe(2);
  });

  it("keeps every matching source label when source roots overlap", () => {
    expect(skillLinkSourceLabel(makeLink("linked", ["shared", "nested"]))).toBe("shared、nested");
    expect(skillLinkSourceLabel(makeLink("linked", ["shared", "nested"]), "shared")).toBe(
      "当前来源 · nested",
    );
    expect(skillLinkSourceLabel(makeLink("unmanaged"))).toBe("");
  });

  it("uses readable owner/repo labels for configured git sources", () => {
    const sourceLabels = buildSkillLinkSourceLabels([
      {
        type: "git",
        id: "source-a",
        repo: "https://github.com/acme/skills.git",
        ref: "main",
        includeNamePatterns: [],
        includePathPatterns: [],
        label: "https://github.com/acme/skills.git · main",
      },
      {
        type: "git",
        id: "source-b",
        repo: "git@github.com:other/tools.git",
        ref: "main",
        includeNamePatterns: [],
        includePathPatterns: [],
        label: "git@github.com:other/tools.git · main",
      },
    ]);

    expect(skillLinkSourceLabel(makeLink("linked", ["source-a", "source-b"]), "source-a", sourceLabels)).toBe(
      "当前来源 · other/tools",
    );
    expect(skillLinkSourceLabel(makeLink("linked", ["source-a", "source-b"]), undefined, sourceLabels)).toBe(
      "acme/skills、other/tools",
    );
  });

  it("detects target-level unmanaged and current-source tags", () => {
    const links = [
      makeLink("unmanaged"),
      makeLink("excluded", ["source-a"]),
    ];

    expect(hasUnmanagedSkillLinks(links)).toBe(true);
    expect(hasCurrentSourceSkillLinks(links, "source-a")).toBe(true);
    expect(hasCurrentSourceSkillLinks(links, "source-b")).toBe(false);
    expect(hasCurrentSourceSkillLinks(links)).toBe(false);
  });

  it("shows the source path relative to the current source root", () => {
    expect(skillLinkSourcePath("/source/alpha", "/source")).toBe("alpha");
    expect(skillLinkSourcePath("/source/nested/alpha", "/source/")).toBe("nested/alpha");
    expect(skillLinkSourcePath("C:\\Source\\alpha", "c:\\source")).toBe("alpha");
  });

  it("keeps paths from other roots intact", () => {
    expect(skillLinkSourcePath("/other/alpha", "/source")).toBe("/other/alpha");
    expect(skillLinkSourcePath("/source/alpha")).toBe("/source/alpha");
  });
});
