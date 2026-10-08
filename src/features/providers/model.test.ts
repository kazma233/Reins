import { describe, expect, it } from "vitest";
import type { ProviderAppEntry, ProviderAppState, ProviderView, ProvidersState } from "./generated";
import {
  applyBlockers,
  applyCandidates,
  emptyModelForm,
  entriesForProvider,
  hasModelId,
  initialApplySelection,
  mergeFetchedModels,
  normalizeProviderIdInput,
  protocolCompatible,
  providerSyncPlan,
  reasoningEffortMappingText,
  reasoningEffortWrite,
  reasoningLevelChoices,
  reasoningLevelOptions,
  reapplyProvider,
  unwrittenModelFieldsText,
} from "./model";

function appState(overrides: Partial<ProviderAppState> = {}): ProviderAppState {
  return {
    app: "codex",
    configPaths: [],
    configExists: false,
    supportedProtocols: ["openai_responses"],
    supportedReasoningLevels: ["minimal", "low", "medium", "high"],
    reasoningLevelWrites: [],
    additive: false,
    unwrittenModelFields: [],
    entries: [],
    loadError: null,
    ...overrides,
  };
}

function provider(overrides: Partial<ProviderView> = {}): ProviderView {
  return {
    id: "openrouter",
    label: "OpenRouter",
    protocol: "openai_responses",
    baseUrl: "https://openrouter.test/v1",
    apiKey: "sk-test-secret",
    models: [],
    ...overrides,
  };
}

describe("protocolCompatible", () => {
  it("returns false when the app does not accept the provider protocol", () => {
    const app = appState({ supportedProtocols: ["openai_chat_completions"] });
    expect(protocolCompatible(app, provider())).toBe(false);
  });

  it("returns true when the protocol is supported", () => {
    expect(protocolCompatible(appState(), provider())).toBe(true);
  });
});

describe("reasoningLevelOptions", () => {
  it("falls back to the app's supported levels when the model has none", () => {
    const options = reasoningLevelOptions(appState(), { reasoningLevels: undefined });
    expect(options).toEqual(["minimal", "low", "medium", "high"]);
  });

  it("intersects the model levels with the app support set", () => {
    const options = reasoningLevelOptions(appState(), {
      reasoningLevels: ["off", "low", "xhigh", "high"],
    });
    expect(options).toEqual(["low", "high"]);
  });

  it("returns an empty list when nothing intersects", () => {
    const options = reasoningLevelOptions(appState(), { reasoningLevels: ["max"] });
    expect(options).toEqual([]);
  });
});

describe("reasoningEffortWrite", () => {
  it("returns the mapped write value published by the backend", () => {
    const app = appState({
      reasoningLevelWrites: [
        { level: "low", writes: "low" },
        { level: "max", writes: "xhigh" },
      ],
    });
    expect(reasoningEffortWrite(app, "max")).toBe("xhigh");
    expect(reasoningEffortWrite(app, "low")).toBe("low");
  });

  it("falls back to the level name when the app publishes no mapping", () => {
    expect(reasoningEffortWrite(appState(), "high")).toBe("high");
  });
});

describe("reasoningEffortMappingText", () => {
  it("lists only the levels whose write value differs", () => {
    const app = appState({
      reasoningLevelWrites: [
        { level: "low", writes: "low" },
        { level: "max", writes: "xhigh" },
        { level: "off", writes: "none" },
      ],
    });
    expect(reasoningEffortMappingText(app)).toBe("max→xhigh、off→none");
  });

  it("returns null when every level writes its own name", () => {
    const app = appState({
      reasoningLevelWrites: [{ level: "low", writes: "low" }],
    });
    expect(reasoningEffortMappingText(app)).toBeNull();
  });
});

describe("unwrittenModelFieldsText", () => {
  it("lists the model metadata the app does not write", () => {
    const text = unwrittenModelFieldsText(
      appState({ app: "grokbuild", unwrittenModelFields: ["最大输出", "图像输入"] })
    );
    expect(text).toContain("Grok Build");
    expect(text).toContain("最大输出、图像输入");
  });

  it("returns null when the app writes every field", () => {
    expect(unwrittenModelFieldsText(appState({ app: "pi" }))).toBeNull();
  });
});

describe("applyBlockers", () => {
  it("lists protocol, key and model blockers", () => {
    const app = appState({ supportedProtocols: ["anthropic_messages"] });
    const blockers = applyBlockers(app, provider({ apiKey: null }), []);
    expect(blockers).toHaveLength(3);
    expect(blockers[0]).toContain("不支持协议");
    expect(blockers[1]).toContain("API Key");
    expect(blockers[2]).toContain("至少选择一个模型");
  });

  it("returns nothing when everything is satisfied", () => {
    expect(applyBlockers(appState(), provider(), ["model-a"])).toEqual([]);
  });
});

describe("normalizeProviderIdInput", () => {
  it("lowercases and converts underscores like the backend", () => {
    expect(normalizeProviderIdInput("GLM_OAI")).toBe("glm-oai");
  });

  it("strips characters outside the allowed set", () => {
    expect(normalizeProviderIdInput(" DeepSeek OAI ")).toBe("deepseekoai");
  });

  it("keeps already valid ids untouched", () => {
    expect(normalizeProviderIdInput("glm-oai-2")).toBe("glm-oai-2");
  });
});

describe("entriesForProvider", () => {
  it("filters entries that belong to the provider", () => {
    const app = appState({
      entries: [
        {
          key: "reins-openrouter",
          status: "applied",
          providerId: "openrouter",
          label: "OpenRouter",
          baseUrl: "https://x",
          modelIds: ["model-a"],
          defaultModelId: "model-a",
          notes: [],
        },
        {
          key: "someone-else",
          status: "external",
          providerId: null,
          label: null,
          baseUrl: "https://y",
          modelIds: [],
          defaultModelId: null,
          notes: [],
        },
      ],
    });
    expect(entriesForProvider(app, "openrouter")).toHaveLength(1);
    expect(entriesForProvider(app, "other")).toHaveLength(0);
  });
});

describe("reapplyProvider", () => {
  function entry(
    status: ProviderAppState["entries"][number]["status"],
    providerId: string | null
  ): ProviderAppState["entries"][number] {
    return {
      key: "entry",
      status,
      providerId,
      label: null,
      baseUrl: null,
      modelIds: [],
      defaultModelId: null,
      notes: [],
    };
  }

  it("resolves applied and drifted entries to their provider", () => {
    const providers = [provider({ id: "glm" })];
    expect(reapplyProvider(entry("applied", "glm"), providers)?.id).toBe("glm");
    expect(reapplyProvider(entry("drifted", "glm"), providers)?.id).toBe("glm");
  });

  it("returns null for external entries and for deleted providers", () => {
    const providers = [provider({ id: "glm" })];
    expect(reapplyProvider(entry("external", null), providers)).toBeNull();
    expect(reapplyProvider(entry("drifted", "gone"), providers)).toBeNull();
  });
});

describe("initialApplySelection", () => {
  function appliedEntry(
    modelIds: string[],
    defaultModelId: string | null
  ): ProviderAppState["entries"][number] {
    return {
      key: "reins-openrouter",
      status: "applied",
      providerId: "openrouter",
      label: "OpenRouter",
      baseUrl: "https://openrouter.test/v1",
      modelIds,
      defaultModelId,
      notes: [],
    };
  }

  const catalog = provider({
    models: [
      { id: "model-a", label: "Model A" },
      { id: "model-b", label: "Model B" },
      { id: "model-c", label: "Model C" },
    ],
  });

  it("reuses the applied models and keeps the current default model", () => {
    const app = appState({ entries: [appliedEntry(["model-b", "model-a"], "model-b")] });
    expect(initialApplySelection(app, catalog)).toEqual({
      modelIds: ["model-b", "model-a"],
      defaultModelId: "model-b",
      defaultReasoningLevel: null,
    });
  });

  it("drops applied models that are no longer in the catalog", () => {
    const app = appState({ entries: [appliedEntry(["model-a", "removed"], "removed")] });
    expect(initialApplySelection(app, catalog)).toEqual({
      modelIds: ["model-a"],
      defaultModelId: "model-a",
      defaultReasoningLevel: null,
    });
  });

  it("preselects the reasoning level currently written in the tool config", () => {
    const app = appState({ defaultReasoningLevel: "high" });
    expect(initialApplySelection(app, catalog).defaultReasoningLevel).toBe("high");
  });

  it("drops a current level the app does not support", () => {
    // fixture 的支持集是 minimal/low/medium/high；max 不在其中，写入会被后端拒绝。
    const app = appState({ defaultReasoningLevel: "max" });
    expect(initialApplySelection(app, catalog).defaultReasoningLevel).toBeNull();
  });

  it("preselects the whole catalog when nothing is applied yet", () => {
    expect(initialApplySelection(appState(), catalog)).toEqual({
      modelIds: ["model-a", "model-b", "model-c"],
      defaultModelId: "model-a",
      defaultReasoningLevel: null,
    });
    expect(initialApplySelection(undefined, catalog)).toEqual({
      modelIds: ["model-a", "model-b", "model-c"],
      defaultModelId: "model-a",
      defaultReasoningLevel: null,
    });
  });
});

describe("reasoningLevelChoices", () => {
  const models = provider({
    models: [
      { id: "model-a", label: "Model A", reasoningLevels: ["low", "high"] },
      { id: "model-b", label: "Model B", reasoningLevels: ["low", "medium"] },
    ],
  });

  it("intersects the selected models' levels with the app support set", () => {
    expect(reasoningLevelChoices(appState(), models, ["model-a", "model-b"], null)).toEqual(["low"]);
  });

  it("keeps the current level even when the intersection excludes it", () => {
    // model-a 单独选中时交集是 low/high，medium 只有当前值提供。
    expect(reasoningLevelChoices(appState(), models, ["model-a"], "high")).toEqual(["low", "high"]);
    expect(reasoningLevelChoices(appState(), models, ["model-a"], "medium")).toEqual([
      "low",
      "high",
      "medium",
    ]);
  });

  it("returns nothing when no model is selected", () => {
    expect(reasoningLevelChoices(appState(), models, [], null)).toEqual([]);
  });
});

describe("applyCandidates", () => {
  function entry(
    key: string,
    status: ProviderAppState["entries"][number]["status"],
    providerId: string | null
  ): ProviderAppState["entries"][number] {
    return {
      key,
      status,
      providerId,
      label: null,
      baseUrl: null,
      modelIds: [],
      defaultModelId: null,
      notes: [],
    };
  }

  it("hides providers that already have a reins entry, applied or drifted", () => {
    const app = appState({
      entries: [
        entry("reins-glm", "applied", "glm"),
        entry("reins-ds", "drifted", "deepseek"),
      ],
    });
    const candidates = applyCandidates(app, [
      provider({ id: "glm" }),
      provider({ id: "deepseek" }),
      provider({ id: "tokenflux" }),
    ]);
    expect(candidates.map((item) => item.id)).toEqual(["tokenflux"]);
  });

  it("keeps providers whose entries are only external", () => {
    const app = appState({ entries: [entry("someone-else", "external", null)] });
    const candidates = applyCandidates(app, [provider({ id: "glm" })]);
    expect(candidates.map((item) => item.id)).toEqual(["glm"]);
  });

  it("keeps every provider when the app has no entries", () => {
    const candidates = applyCandidates(appState(), [provider()]);
    expect(candidates).toHaveLength(1);
  });
});

describe("hasModelId", () => {
  it("matches ids after trimming both sides", () => {
    expect(hasModelId(["glm-4.5", "deepseek-chat"], " glm-4.5 ")).toBe(true);
    expect(hasModelId(["glm-4.5"], "glm-4.6")).toBe(false);
  });

  it("stays case sensitive like the backend", () => {
    expect(hasModelId(["glm-4.5"], "GLM-4.5")).toBe(false);
  });
});

describe("mergeFetchedModels", () => {
  it("adds new models and skips ids already in the catalog", () => {
    const existing = [{ ...emptyModelForm("glm-4.5"), id: "glm-4.5", label: "GLM 4.5" }];
    const { added, skipped } = mergeFetchedModels(existing, [
      { id: "glm-4.5", name: "GLM 4.5" },
      { id: "glm-4.6", name: "GLM 4.6" },
    ]);
    expect(skipped.map((model) => model.id)).toEqual(["glm-4.5"]);
    expect(added).toHaveLength(1);
    expect(added[0]).toMatchObject({ id: "glm-4.6", label: "GLM 4.6" });
  });

  it("falls back to the id when the fetched model has no name", () => {
    const { added } = mergeFetchedModels([], [{ id: "glm-4.6" }]);
    expect(added[0].label).toBe("glm-4.6");
  });

  it("skips repeated ids inside the fetched list", () => {
    const { added, skipped } = mergeFetchedModels([], [
      { id: "glm-4.6" },
      { id: "glm-4.6" },
    ]);
    expect(added).toHaveLength(1);
    expect(skipped.map((model) => model.id)).toEqual(["glm-4.6"]);
  });

  it("compares ids with the same trimming as the backend", () => {
    const existing = [{ ...emptyModelForm("glm-4.5 "), id: "glm-4.5 " }];
    const { added, skipped } = mergeFetchedModels(existing, [{ id: "glm-4.5" }]);
    expect(added).toHaveLength(0);
    expect(skipped).toHaveLength(1);
  });
});

describe("providerSyncPlan", () => {
  function entry(providerId: string, overrides: Partial<ProviderAppEntry> = {}): ProviderAppEntry {
    return {
      key: `reins-${providerId}`,
      status: "applied",
      providerId,
      label: null,
      baseUrl: null,
      modelIds: [],
      defaultModelId: null,
      notes: [],
      protocol: null,
      ...overrides,
    };
  }

  function stateOf(apps: ProviderAppState[], providers: ProviderView[]): ProvidersState {
    return { configPath: "providers.yaml", providers, apps };
  }

  function model(id: string): ProviderView["models"][number] {
    return { id, label: id };
  }

  it("plans applied and drifted entries and ignores external ones", () => {
    const target = provider({ id: "glm", models: [model("glm-4.5")] });
    const plan = providerSyncPlan(
      stateOf(
        [
          appState({ app: "codex", entries: [entry("glm")] }),
          appState({ app: "claude", entries: [entry("glm", { status: "drifted" })] }),
          appState({
            app: "opencode",
            entries: [
              {
                key: "someone-else",
                status: "external",
                providerId: null,
                label: null,
                baseUrl: null,
                modelIds: [],
                defaultModelId: null,
                notes: [],
              },
            ],
          }),
        ],
        [target]
      ),
      target
    );
    expect(plan.map((item) => item.app)).toEqual(["codex", "claude"]);
    expect(plan.some((item) => item.skip)).toBe(false);
  });

  it("keeps each agent's applied models, default model and reasoning level", () => {
    const target = provider({ id: "glm", models: [model("glm-4.5"), model("glm-4.6")] });
    const apps = [
      appState({
        supportedReasoningLevels: ["low", "high"],
        defaultReasoningLevel: "high",
        entries: [entry("glm", { modelIds: ["glm-4.5"], defaultModelId: "glm-4.5" })],
      }),
    ];
    const plan = providerSyncPlan(stateOf(apps, [target]), target);
    expect(plan).toHaveLength(1);
    const item = plan[0];
    if (item.skip) {
      throw new Error("expected a syncable item");
    }
    expect(item.input).toEqual({
      providerId: "glm",
      app: "codex",
      modelIds: ["glm-4.5"],
      defaultModelId: "glm-4.5",
      defaultReasoningLevel: "high",
    });
  });

  it("skips agents whose applied models all left the catalog", () => {
    const target = provider({ id: "glm", models: [model("glm-4.6")] });
    const apps = [appState({ entries: [entry("glm", { modelIds: ["glm-4.5"] })] })];
    const plan = providerSyncPlan(stateOf(apps, [target]), target);
    const item = plan[0];
    if (!item.skip) {
      throw new Error("expected a skipped item");
    }
    expect(item.reason).toContain("均不在当前目录中");
  });

  it("skips agents that cannot apply yet and reports the blockers", () => {
    const target = provider({ id: "glm", apiKey: "", models: [model("glm-4.5")] });
    const apps = [appState({ entries: [entry("glm")] })];
    const plan = providerSyncPlan(stateOf(apps, [target]), target);
    const item = plan[0];
    if (!item.skip) {
      throw new Error("expected a skipped item");
    }
    expect(item.reason).toContain("API Key");
  });

  it("skips agents whose tool cannot speak the provider protocol", () => {
    const target = provider({ id: "glm", models: [model("glm-4.5")] });
    const apps = [appState({ supportedProtocols: ["anthropic_messages"], entries: [entry("glm")] })];
    const plan = providerSyncPlan(stateOf(apps, [target]), target);
    const item = plan[0];
    if (!item.skip) {
      throw new Error("expected a skipped item");
    }
    expect(item.reason).toContain("不支持协议");
  });

  it("plans nothing for a provider that is not applied anywhere", () => {
    const target = provider({ id: "glm", models: [model("glm-4.5")] });
    const plan = providerSyncPlan(stateOf([appState()], [target]), target);
    expect(plan).toEqual([]);
  });
});
