import { beforeEach, describe, expect, it } from "vitest";

import { DEPLOYMENT } from "./lib/kinds";
import { useApp, type View } from "./store";

const view = () => useApp.getState().history[useApp.getState().index];

const start = (current: View) => useApp.setState({ namespace: null, history: [{ name: "applications" }, current], index: 1 });

describe("browseNamespace", () => {
  beforeEach(() => useApp.setState({ context: "dev" }));

  it("opens Browse from views that do not list namespaced resources", () => {
    const resourcePage: View = { name: "resource", resource: DEPLOYMENT, namespace: "shop", object: "web", tab: "logs" };
    for (const current of [{ name: "overview" }, resourcePage] satisfies View[]) {
      start(current);
      useApp.getState().browseNamespace("billing");
      expect(useApp.getState().namespace).toBe("billing");
      expect(view()).toEqual({ name: "applications" });
      // Back returns to where the click came from.
      useApp.getState().back();
      expect(view()).toEqual(current);
    }
  });

  it("filters in place on Browse", () => {
    for (const current of [{ name: "applications" }, { name: "resources", resource: null }] satisfies View[]) {
      start(current);
      useApp.getState().browseNamespace("shop");
      expect([useApp.getState().namespace, view()]).toEqual(["shop", current]);
    }
  });

  it("clears the filter with null", () => {
    start({ name: "overview" });
    useApp.getState().browseNamespace(null);
    expect([useApp.getState().namespace, view()]).toEqual([null, { name: "applications" }]);
  });
});

describe("browseAll", () => {
  beforeEach(() => useApp.setState({ context: "dev" }));

  it("returns to Applications across all namespaces from a page in a namespace", () => {
    const resourcePage: View = { name: "resource", resource: DEPLOYMENT, namespace: "shop", object: "web", tab: "overview" };
    for (const current of [resourcePage, { name: "resources", resource: null }, { name: "overview" }] satisfies View[]) {
      start(current);
      useApp.setState({ namespace: "shop" });
      useApp.getState().browseAll();
      expect([useApp.getState().namespace, view()]).toEqual([null, { name: "applications" }]);
      useApp.getState().back();
      expect(view()).toEqual(current);
    }
  });

  it("clears the filter and search in place on Applications", () => {
    start({ name: "applications" });
    useApp.setState({ namespace: "shop", search: "web" });
    useApp.getState().browseAll();
    const { namespace, search, history } = useApp.getState();
    expect([namespace, search, history.length, view()]).toEqual([null, "", 2, { name: "applications" }]);
  });
});
