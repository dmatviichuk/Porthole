import { describe, expect, it } from "vitest";

import { age, duration, matches, sourceLabels } from "./format";
import { filterSections, pluralTitle, sections } from "./kinds";
import type { ResourceInfo } from "./types";

describe("age", () => {
  const now = 1_791_021_600_000;
  const ago = (seconds: number) => now / 1000 - seconds;

  it("matches kubectl's units", () => {
    expect(age(ago(42), now)).toBe("42s");
    expect(age(ago(12 * 60 + 5), now)).toBe("12m");
    expect(age(ago(3 * 3600 + 20 * 60), now)).toBe("3h20m");
    expect(age(ago(14 * 3600), now)).toBe("14h");
    expect(age(ago(5 * 86_400), now)).toBe("5d");
    expect(age(ago(800 * 86_400), now)).toBe("2y");
    expect(age(null, now)).toBe("");
  });

  it("never goes negative when clocks disagree", () => {
    expect(age(ago(-30), now)).toBe("0s");
  });
});

describe("duration", () => {
  it("drops zero parts", () => {
    expect([duration(42), duration(90), duration(120), duration(7300), duration(7200)]).toEqual([
      "42s",
      "1m30s",
      "2m",
      "2h1m",
      "2h",
    ]);
  });
});

describe("matches", () => {
  it("requires every term somewhere", () => {
    expect(matches("web prod", "web-7d9f8b", "production")).toBe(true);
    expect(matches("web staging", "web-7d9f8b", "production")).toBe(false);
    expect(matches("  ", "anything")).toBe(true);
  });
});

const info = (group: string, kind: string, plural: string): ResourceInfo => ({
  group,
  version: "v1",
  kind,
  plural,
  namespaced: true,
  verbs: [],
});

describe("kinds", () => {
  it("pluralises kinds for titles", () => {
    expect(["Deployment", "Ingress", "NetworkPolicy", "Gateway"].map(pluralTitle)).toEqual([
      "Deployments",
      "Ingresses",
      "NetworkPolicies",
      "Gateways",
    ]);
  });

  it("puts every discovered type in exactly one section", () => {
    const discovered = [
      info("", "Pod", "pods"),
      info("apps", "Deployment", "deployments"),
      info("coordination.k8s.io", "Lease", "leases"),
      info("events.k8s.io", "Event", "events"),
      info("gateway.networking.k8s.io", "Gateway", "gateways"),
      info("argoproj.io", "Application", "applications"),
    ];
    const result = sections(discovered).map((s) => [s.title, s.items.map((i) => i.kind)]);
    expect(result).toEqual([
      ["Workloads", ["Pod", "Deployment"]],
      ["Custom resources", ["Application", "Gateway"]],
      ["Other", ["Lease"]],
    ]);
  });
});

const t = (pod: string, container: string) => ({ namespace: "shop", pod, container });

describe("log source labels", () => {
  it("keeps only what tells pods and containers apart", () => {
    const workload = sourceLabels([t("web-7d9f8b-aaaa1", "app"), t("web-7d9f8b-aaab2", "app")]);
    expect([...workload.values()]).toEqual(["aaaa1", "aaab2"]);
    const sidecars = sourceLabels([t("web-7d9f8b-x1", "app"), t("web-7d9f8b-x1", "proxy"), t("web-7d9f8b-y2", "app")]);
    expect([...sidecars.values()]).toEqual(["x1/app", "x1/proxy", "y2/app"]);
    expect([...sourceLabels([t("debug", "a"), t("debug", "b")]).values()]).toEqual(["a", "b"]);
  });
});

describe("filterSections", () => {
  const listed = sections([
    info("", "Pod", "pods"),
    info("apps", "Deployment", "deployments"),
    info("autoscaling", "HorizontalPodAutoscaler", "horizontalpodautoscalers"),
    info("argoproj.io", "Application", "applications"),
    info("argoproj.io", "AppProject", "appprojects"),
  ]);
  const kinds = (query: string) => filterSections(listed, query).flatMap((s) => s.items.map((i) => i.kind));

  it("matches kind, plural, short name and group", () => {
    expect(kinds("deploy")).toEqual(["Deployment"]);
    expect(kinds("hpa")).toEqual(["HorizontalPodAutoscaler"]);
    expect(kinds("PODS")).toEqual(["Pod"]);
    expect(kinds("argo")).toEqual(["Application", "AppProject"]);
    expect(kinds("argo proj")).toEqual(["Application", "AppProject"]);
    expect(kinds("argo application")).toEqual(["Application"]);
  });

  it("drops empty sections and keeps everything for an empty query", () => {
    expect(filterSections(listed, "deploy").map((s) => s.title)).toEqual(["Workloads"]);
    expect(filterSections(listed, "  ")).toBe(listed);
    expect(kinds("nothing-like-this")).toEqual([]);
  });
});

