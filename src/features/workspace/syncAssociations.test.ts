import { describe, expect, it } from "vitest";
import type { SkillLinkAssociation, SyncTargetOption } from "./types";
import {
  buildSkillLinkSourceLabels,
  canRemoveSkillLink,
  defaultSelectedSyncTargetIds,
  hasCurrentSourceSkillLinks,
  hasUnmanagedSkillLinks,
  isSelectableSyncTarget,
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

function makeTarget(overrides: Partial<SyncTargetOption> & { id: string }): SyncTargetOption {
  return {
    label: overrides.id,
    skillDir: `/targets/${overrides.id}`,
    enabled: true,
    links: [],
    ...overrides,
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

  it("marks disabled and inherited targets as unselectable", () => {
    expect(isSelectableSyncTarget(makeTarget({ id: "enabled" }))).toBe(true);
    expect(isSelectableSyncTarget(makeTarget({ id: "disabled", enabled: false }))).toBe(false);
    expect(isSelectableSyncTarget(makeTarget({ id: "inherited", linkedTargetId: "enabled" }))).toBe(false);
  });

  it("preselects selectable targets already linked to the current source", () => {
    const targets = [
      makeTarget({ id: "linked", links: [makeLink("linked", ["source-a"])] }),
      makeTarget({
        id: "cleanup",
        links: [makeLink("excluded", ["source-a"]), makeLink("sourceMissing", ["source-a"])],
      }),
      makeTarget({ id: "other-source", links: [makeLink("linked", ["source-b"])] }),
      makeTarget({ id: "unmanaged", links: [makeLink("unmanaged")] }),
      makeTarget({ id: "empty" }),
      makeTarget({ id: "disabled", enabled: false, links: [makeLink("linked", ["source-a"])] }),
      makeTarget({ id: "inherited", linkedTargetId: "linked", links: [makeLink("linked", ["source-a"])] }),
    ];

    expect(defaultSelectedSyncTargetIds(targets, "source-a")).toEqual(["linked", "cleanup"]);
    expect(defaultSelectedSyncTargetIds(targets)).toEqual([]);
  });

  it("allows removing only unmanaged links one by one", () => {
    expect(canRemoveSkillLink(makeLink("unmanaged"))).toBe(true);
    expect(canRemoveSkillLink(makeLink("linked", ["source-a"]))).toBe(false);
    expect(canRemoveSkillLink(makeLink("excluded", ["source-a"]))).toBe(false);
    expect(canRemoveSkillLink(makeLink("sourceMissing", ["source-a"]))).toBe(false);
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
