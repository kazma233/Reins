import { describe, expect, it } from "vitest";
import {
  IMPORT_TARGET_APPS,
  importTargetCopy,
  manualOpenCommand
} from "./model";
import { formatSourceAppName } from "./source-app";

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
