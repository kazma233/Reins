import { describe, expect, it } from "vitest";
import { canDeleteSession } from "./model";
import { formatSourceAppName } from "./source-app";

describe("Grok Build session contract", () => {
  it("exposes the Grok Build source as deletable", () => {
    expect(formatSourceAppName("grokbuild")).toBe("Grok Build");
    expect(canDeleteSession("grokbuild")).toBe(true);
  });
});

describe("Pi session contract", () => {
  it("exposes the Pi source as deletable", () => {
    expect(formatSourceAppName("pi")).toBe("Pi");
    expect(canDeleteSession("pi")).toBe(true);
  });
});
