import { describe, expect, it } from "vitest";
import {
  IMPORT_TARGET_APPS,
  canDeleteSession,
  importTargetCopy,
  manualOpenCommand
} from "./model";
import { formatSourceAppName } from "./source-app";

describe("Grok Build session contract", () => {
  it("exposes a read-only source without an import or delete target", () => {
    expect(formatSourceAppName("grokbuild")).toBe("Grok Build");
    expect(IMPORT_TARGET_APPS).not.toContain("grokbuild");
    expect(canDeleteSession("grokbuild")).toBe(false);
    expect(canDeleteSession("pi")).toBe(true);
    expect(() => importTargetCopy("grokbuild")).toThrow("不支持导入");
  });

  it("quotes cwd and session id when resuming", () => {
    expect(manualOpenCommand("grokbuild", "id'$(echo x)", "/tmp/project's folder"))
      .toBe(`grok --cwd '/tmp/project'"'"'s folder' --resume 'id'"'"'$(echo x)'`);
    expect(manualOpenCommand("grokbuild", "id", null)).toBe("grok --resume 'id'");
  });
});

describe("Pi session contract", () => {
  it("exposes the Pi source and import target copy", () => {
    expect(formatSourceAppName("pi")).toBe("Pi");
    expect(IMPORT_TARGET_APPS).toContain("pi");
    expect(importTargetCopy("pi").optionLabel).toBe("Pi");
  });

  it("resumes imported Pi sessions by transcript path", () => {
    expect(
      manualOpenCommand(
        "pi",
        "session-id",
        "/tmp/project",
        "/tmp/project/.pi/session.jsonl"
      )
    ).toBe(
      "cd '/tmp/project' && pi --session '/tmp/project/.pi/session.jsonl'"
    );
  });
});
