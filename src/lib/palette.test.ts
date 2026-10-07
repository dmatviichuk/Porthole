import { describe, expect, it } from "vitest";

import { type PaletteItem, rank, score } from "./palette";

const item = (group: string, label: string, extra: Partial<PaletteItem> = {}): PaletteItem => ({
  id: `${group}/${label}`,
  group,
  label,
  run: () => undefined,
  ...extra,
});

const labels = (items: PaletteItem[]) => items.map((i) => i.label);

const openGetsTwo = (group: string) => (group === "Open" ? 2 : 1);

describe("score", () => {
  it("needs every word somewhere in the label, detail or keywords", () => {
    const pod = item("Open", "checkout-7f9c", { detail: "Pod · shop" });
    expect(score(pod, ["checkout", "shop"])).not.toBeNull();
    expect(score(pod, ["checkout", "data"])).toBeNull();
  });

  it("ranks an exact label over a prefix, a word start and a match inside", () => {
    expect(score(item("g", "pods"), ["pods"])).toBe(3);
    expect(score(item("g", "pods-viewer"), ["pods"])).toBe(2);
    expect(score(item("g", "all pods"), ["pods"])).toBe(1);
    expect(score(item("g", "deployments"), ["loy"])).toBe(0);
  });

  it("finds by keywords such as kubectl short names", () => {
    expect(score(item("Resource types", "Deployments", { keywords: "deploy apps" }), ["deploy"])).toBe(2);
    expect(score(item("Resource types", "Persistent Volume Claims", { keywords: "pvc" }), ["pvc"])).toBe(0);
  });
});

describe("rank", () => {
  const items = [
    item("Go to", "Applications"),
    item("Go to", "Overview"),
    item("Namespaces", "storefront"),
    item("Namespaces", "kube-system"),
    item("Open", "storefront-api"),
    item("Open", "my-storefront"),
    item("Open", "storefront"),
  ];

  it("keeps everything, in order, for an empty query", () => {
    expect(labels(rank(items, "  ", () => 10))).toEqual(labels(items));
  });

  it("keeps the group order and sorts each group by score", () => {
    expect(labels(rank(items, "storefront", () => 10))).toEqual(["storefront", "storefront", "storefront-api", "my-storefront"]);
  });

  it("caps each group by its own limit", () => {
    expect(labels(rank(items, "store", openGetsTwo))).toEqual(["storefront", "storefront-api", "storefront"]);
    expect(labels(rank(items, "s", () => 1))).toEqual(["Applications", "storefront", "storefront-api"]);
  });

  it("drops groups with no match", () => {
    expect(rank(items, "zzz", () => 10)).toEqual([]);
  });
});
