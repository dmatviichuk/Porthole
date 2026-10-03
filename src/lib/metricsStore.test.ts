import { describe, expect, it } from "vitest";

import { describeMetricsError } from "./metricsStore";

describe("describeMetricsError", () => {
  it("tells apart a missing metrics-server, missing access and an unhealthy one", () => {
    expect(describeMetricsError("the server could not find the requested resource").kind).toBe("missing");
    // Unreachable clusters are not a missing metrics-server.
    expect(describeMetricsError("error trying to connect: dns error: host not found").kind).toBe("unavailable");
    expect(
      describeMetricsError(
        'nodes.metrics.k8s.io is forbidden: User "dev" cannot list resource "nodes" in API group "metrics.k8s.io" at the cluster scope',
      ).kind,
    ).toBe("forbidden");
    const down = describeMetricsError("the server is currently unable to handle the request");
    expect(down.kind).toBe("unavailable");
    expect(down.message).toContain("currently unable to handle the request");
  });
});
