import type { McpConfigView } from "./generated/McpConfigView";
import type { McpTransport } from "./generated/McpTransport";
import type { FieldErrors } from "./model";

// mcp 表单的解析 / 格式化 / 校验。此前这三件事困在 useMcpMutations 的私有
// 函数里，行级校验零直接测试；照 providers/model.ts 的形态收成纯模块，输入输出
// 都是值，测试不需要挂载组件或模拟 invoke。
export type McpFormState = {
  originalName: string | null;
  name: string;
  transport: McpTransport;
  createdAt: number | null;
  homepage: string;
  command: string;
  args: string;
  env: string;
  url: string;
  headers: string;
  timeout: string;
  errors: FieldErrors;
};

// 提交给后端之前的口径：字段齐全、空值用 null（不是 undefined），
// 这样「表单校验通过」与「payload 可序列化」是同一件事。
export type McpFormPayload = {
  name: string;
  transport: McpTransport;
  homepage: string | null;
  command: string | null;
  args: string[];
  env: Record<string, string>;
  url: string | null;
  headers: Record<string, string>;
  timeout: number | null;
};

export function emptyMcpForm(): McpFormState {
  return {
    originalName: null,
    name: "",
    transport: "stdio",
    createdAt: null,
    homepage: "",
    command: "",
    args: "",
    env: "",
    url: "",
    headers: "",
    timeout: "",
    errors: {},
  };
}

// env / headers 的编辑态：每行一条 KEY=VALUE。
export function formatKeyValueText(value: Record<string, string>): string {
  return Object.entries(value)
    .map(([key, entryValue]) => `${key}=${entryValue}`)
    .join("\n");
}

// 从配置视图回填表单（编辑已有 mcp）。
export function formFromMcp(mcp: McpConfigView): McpFormState {
  return {
    originalName: mcp.name,
    name: mcp.name,
    transport: mcp.transport,
    createdAt: mcp.createdAt,
    homepage: mcp.homepage ?? "",
    command: mcp.command ?? "",
    args: mcp.args.join("\n"),
    env: formatKeyValueText(mcp.env ?? {}),
    url: mcp.url ?? "",
    headers: formatKeyValueText(mcp.headers),
    timeout: mcp.timeout === null ? "" : String(mcp.timeout),
    errors: {},
  };
}

// 抛错而不是返回部分结果：调用方要的是「要么全对要么指出哪一行」，
// 静默丢行会让用户以为已经存进去了。
export function parseKeyValueText(value: string): Record<string, string> {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .reduce<Record<string, string>>((result, line) => {
      const separatorIndex = line.indexOf("=");
      if (separatorIndex <= 0) {
        throw new Error(`格式错误: ${line}`);
      }
      const key = line.slice(0, separatorIndex).trim();
      const nextValue = line.slice(separatorIndex + 1).trim();
      if (key) {
        result[key] = nextValue;
      }
      return result;
    }, {});
}

export function parseLineList(value: string): string[] {
  return value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

// 每行一条 KEY=VALUE，错误定位到具体那一行；字段错误挂在对应输入框下方。
export function keyValueError(value: string, label: string): string | null {
  if (!value.trim()) return null;
  try {
    parseKeyValueText(value);
    return null;
  } catch {
    const line = value
      .split("\n")
      .map((entry) => entry.trim())
      .find((entry) => entry.length > 0 && entry.indexOf("=") <= 0);
    return `${label} 第「${line ?? ""}」行缺少 “=”，每行需要写成 KEY=VALUE。`;
  }
}

// timeout 允许空（不写该字段），非空则必须是大于等于 0 的有限数。
export function parseTimeoutText(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;
  return Number(trimmed);
}

export function buildMcpErrors(form: McpFormState): FieldErrors {
  const errors: FieldErrors = {};

  if (!form.name.trim()) {
    errors.name = "请填写 mcp 名称。";
  }
  if (form.transport === "stdio" && !form.command.trim()) {
    errors.command = "stdio 模式需要填写 command。";
  }
  if (form.transport !== "stdio" && !form.url.trim()) {
    errors.url = "远程模式需要填写 mcp 链接。";
  }

  const timeout = parseTimeoutText(form.timeout);
  if (timeout !== null && (!Number.isFinite(timeout) || timeout < 0)) {
    errors.timeout = "timeout 需要是大于等于 0 的数字。";
  }

  const envError = form.transport === "stdio" ? keyValueError(form.env, "Env") : null;
  if (envError) {
    errors.env = envError;
  }
  const headersError = form.transport === "stdio" ? null : keyValueError(form.headers, "Headers");
  if (headersError) {
    errors.headers = headersError;
  }

  return errors;
}

// 校验一次算出全部错误，只在字段旁列出问题。返回 payload 或 null——
// 「有没有错」与「能不能提交」由这一次调用回答，调用方不再各自判空。
export function buildMcpPayload(form: McpFormState): McpFormPayload | null {
  const errors = buildMcpErrors(form);
  if (Object.keys(errors).length > 0) return null;

  const isStdio = form.transport === "stdio";
  return {
    name: form.name.trim(),
    transport: form.transport,
    homepage: form.homepage.trim() || null,
    command: isStdio ? form.command.trim() : null,
    args: isStdio ? parseLineList(form.args) : [],
    env: isStdio ? parseKeyValueText(form.env) : {},
    url: isStdio ? null : form.url.trim(),
    headers: isStdio ? {} : parseKeyValueText(form.headers),
    timeout: parseTimeoutText(form.timeout),
  };
}
