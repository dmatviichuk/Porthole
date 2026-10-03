import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { loadLabels, saveLabels } from "./clusterLabels";

describe("cluster labels", () => {
  const stored = new Map<string, string>();
  beforeEach(() => {
    globalThis.localStorage = {
      getItem: (k: string) => stored.get(k) ?? null,
      setItem: (k: string, v: string) => void stored.set(k, v),
      removeItem: (k: string) => void stored.delete(k),
      clear: () => stored.clear(),
      key: () => null,
      length: 0,
    } as Storage;
  });
  afterEach(() => stored.clear());

  it("round-trips labels by context name", () => {
    saveLabels({ "prd-cluster": { tag: "Production", color: "red", confirmDeletes: true } });
    expect(loadLabels()).toEqual({ "prd-cluster": { tag: "Production", color: "red", confirmDeletes: true } });
  });

  it("drops entries it cannot use instead of failing", () => {
    stored.set(
      "porthole.clusterLabels",
      JSON.stringify({
        ok: { tag: "Dev", color: "blue", confirmDeletes: false },
        "bad-color": { tag: "X", color: "neon", confirmDeletes: false },
        "missing-field": { tag: "Y", color: "red" },
      }),
    );
    expect(Object.keys(loadLabels())).toEqual(["ok"]);
    stored.set("porthole.clusterLabels", "not json");
    expect(loadLabels()).toEqual({});
  });
});
