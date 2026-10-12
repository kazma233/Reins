import { describe, expect, it } from "vitest";

import type { McpConfigView } from "./generated/McpConfigView";
import {
  buildMcpErrors,
  buildMcpPayload,
  emptyMcpForm,
  formFromMcp,
  formatKeyValueText,
  keyValueError,
  parseKeyValueText,
  parseLineList,
  parseTimeoutText,
  type McpFormState,
} from "./mcpForm";

function form(overrides: Partial<McpFormState> = {}): McpFormState {
  return { ...emptyMcpForm(), name: "fs-server", command: "npx", ...overrides };
}

function mcpView(overrides: Partial<McpConfigView> = {}): McpConfigView {
  return {
    name: "fs-server",
    transport: "stdio",
    createdAt: 1,
    homepage: null,
    command: "npx",
    args: ["-y", "server-filesystem"],
    env: {},
    url: null,
    headers: {},
    timeout: null,
    ...overrides,
  };
}

describe("mcp form key-value lines", () => {
  it("parses KEY=VALUE lines, trimming blanks and whitespace", () => {
    expect(parseKeyValueText(" A = 1 \n\nB=2\n")).toEqual({ A: "1", B: "2" });
    expect(parseKeyValueText("")).toEqual({});
  });

  it("keeps the value part intact when it contains equals signs", () => {
    // base_url 这类值自身带 `=` 很常见：只按第一个等号切分。
    expect(parseKeyValueText("URL=https://x.test/a?b=1&c=2")).toEqual({
      URL: "https://x.test/a?b=1&c=2",
    });
  });

  it("rejects a line without a key part", () => {
    expect(() => parseKeyValueText("novalue")).toThrow();
    expect(() => parseKeyValueText("=value")).toThrow();
  });

  it("names the offending line when reporting a field error", () => {
    expect(keyValueError("A=1\nBAD\nB=2", "Env")).toBe(
      "Env 第「BAD」行缺少 “=”，每行需要写成 KEY=VALUE。",
    );
    expect(keyValueError("", "Env")).toBeNull();
    expect(keyValueError("A=1", "Env")).toBeNull();
  });

  it("round-trips a value map through the编辑态 text", () => {
    expect(formatKeyValueText({ A: "1", B: "2" })).toBe("A=1\nB=2");
    expect(parseKeyValueText(formatKeyValueText({ A: "1", B: "2" }))).toEqual({ A: "1", B: "2" });
  });

  it("splits argument lines and drops blanks", () => {
    expect(parseLineList("-y\n\n server-filesystem \n")).toEqual(["-y", "server-filesystem"]);
  });
});

describe("mcp form validation", () => {
  it("requires name and a transport-specific field", () => {
    expect(buildMcpErrors(form({ name: " " })).name).toBeDefined();
    expect(buildMcpErrors(form({ command: " " })).command).toBeDefined();
    // stdio 下 url 不参与校验；远程模式反过来。
    expect(buildMcpErrors(form({ url: "" })).url).toBeUndefined();
    expect(buildMcpErrors(form({ transport: "http", command: "" })).url).toBeDefined();
  });

  it("rejects only non-numeric or negative timeouts", () => {
    expect(buildMcpErrors(form({ timeout: "2" })).timeout).toBeUndefined();
    expect(buildMcpErrors(form({ timeout: "0" })).timeout).toBeUndefined();
    expect(buildMcpErrors(form({ timeout: "" })).timeout).toBeUndefined();
    expect(buildMcpErrors(form({ timeout: "abc" })).timeout).toBeDefined();
    expect(buildMcpErrors(form({ timeout: "-1" })).timeout).toBeDefined();
  });

  it("checks the key-value field that belongs to the chosen transport", () => {
    const stdioBroken = buildMcpErrors(form({ env: "BAD" }));
    expect(stdioBroken.env).toBeDefined();
    // stdio 下 headers 不参与，JSON 里也不会带它。
    expect(buildMcpErrors(form({ headers: "BAD" })).headers).toBeUndefined();

    const remoteBroken = buildMcpErrors(form({ transport: "http", url: "https://x", headers: "BAD" }));
    expect(remoteBroken.headers).toBeDefined();
    expect(buildMcpErrors(form({ transport: "http", url: "https://x", env: "BAD" })).env).toBeUndefined();
  });

  it("collects every problem at once instead of stopping at the first", () => {
    const errors = buildMcpErrors(form({ name: "", command: "", timeout: "x", env: "BAD" }));

    expect(Object.keys(errors).sort()).toEqual(["command", "env", "name", "timeout"]);
  });
});

describe("mcp form payload", () => {
  it("returns null and the field errors when the form is invalid", () => {
    const form = { ...emptyMcpForm(), errors: {} } as McpFormState;

    expect(buildMcpPayload(form)).toBeNull();
  });

  it("keeps only the fields the transport uses", () => {
    const payload = buildMcpPayload(
      form({ args: "-y\nserver-filesystem", env: "KEY=value", url: "https://ignored", headers: "X=1" }),
    );

    expect(payload).toEqual({
      name: "fs-server",
      transport: "stdio",
      homepage: null,
      command: "npx",
      args: ["-y", "server-filesystem"],
      env: { KEY: "value" },
      url: null,
      headers: {},
      timeout: null,
    });
  });

  it("builds a remote payload without stdio fields", () => {
    const payload = buildMcpPayload(
      form({
        transport: "http",
        command: "npx",
        args: "-y",
        env: "KEY=value",
        url: " https://example.com/mcp ",
        headers: "X-Key=value",
        timeout: "5",
      }),
    );

    expect(payload).toEqual({
      name: "fs-server",
      transport: "http",
      homepage: null,
      command: null,
      args: [],
      env: {},
      url: "https://example.com/mcp",
      headers: { "X-Key": "value" },
      timeout: 5,
    });
  });

  it("trims the user-visible fields and maps blank ones to null", () => {
    const payload = buildMcpPayload(
      form({ name: " fs-server ", homepage: " https://x.test ", timeout: "" }),
    );

    expect(payload?.name).toBe("fs-server");
    expect(payload?.homepage).toBe("https://x.test");
    expect(payload?.timeout).toBeNull();
    expect(buildMcpPayload(form({ homepage: " " }))?.homepage).toBeNull();
  });
});

describe("mcp form backfill", () => {
  it("fills the transport-specific text fields from a stored mcp", () => {
    const state = formFromMcp(
      mcpView({ env: { A: "1" }, headers: { X: "1" }, timeout: 3, homepage: "https://x.test" }),
    );

    expect(state.originalName).toBe("fs-server");
    expect(state.args).toBe("-y\nserver-filesystem");
    expect(state.env).toBe("A=1");
    expect(state.headers).toBe("X=1");
    expect(state.timeout).toBe("3");
    expect(state.homepage).toBe("https://x.test");
  });

  it("round-trips a stored mcp back into an identical payload", () => {
    const view = mcpView({ env: { A: "1" }, timeout: 2, homepage: "https://x.test" });
    const payload = buildMcpPayload(formFromMcp(view));

    expect(payload).toEqual({
      name: view.name,
      transport: view.transport,
      homepage: view.homepage,
      command: view.command,
      args: view.args,
      env: view.env,
      url: null,
      headers: {},
      timeout: view.timeout,
    });
  });

  it("maps an absent timeout to an empty edit field rather than the string null", () => {
    expect(formFromMcp(mcpView({ timeout: null })).timeout).toBe("");
    expect(parseTimeoutText("")).toBeNull();
  });
});
