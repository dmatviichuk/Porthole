import { describe, expect, it } from "vitest";

import { displayLength, parseLog } from "./logFormat";

describe("parseLog", () => {
  it("splits structured JSON into level, message and the remaining fields", () => {
    const line =
      '{"timestamp": "2026-10-03T09:58:56.051031", "level": "INFO", "message": "Worker exiting (pid: 176)", "file": "glogging.py", "line": 42}';
    expect(parseLog(line)).toEqual({
      kind: "json",
      level: "info",
      levelText: "INFO",
      message: "Worker exiting (pid: 176)",
      fields: [
        ["file", "glogging.py"],
        ["line", 42],
      ],
    });
  });

  it("understands other loggers' field names and numeric levels", () => {
    const pino = parseLog('{"level":50,"time":1791021600000,"msg":"db down","err":{"code":"ECONNREFUSED"}}');
    expect(pino).toMatchObject({ kind: "json", level: "error", message: "db down" });
    expect(pino.kind === "json" && pino.fields).toEqual([["err", { code: "ECONNREFUSED" }]]);
    expect(parseLog('{"severity":"WARNING","event":"retrying"}')).toMatchObject({ level: "warn", message: "retrying" });
  });

  it("finds the level word in plain text", () => {
    expect(parseLog("[2026-10-03 08:27:36 +0000] [176] [INFO] Booting worker with pid: 176")).toEqual({
      kind: "text",
      level: "info",
      levelAt: [35, 39],
    });
    expect(parseLog("panic: runtime error: index out of range")).toMatchObject({ level: null });
    expect(parseLog("ERROR could not connect")).toMatchObject({ level: "error", levelAt: [0, 5] });
  });

  it("leaves JSON that is not an object, or not JSON, as text", () => {
    expect(parseLog("[1, 2, 3]").kind).toBe("text");
    expect(parseLog("{not json}").kind).toBe("text");
    expect(parseLog('{"a": 1').kind).toBe("text");
  });
});

describe("displayLength", () => {
  it("counts what is shown, not the raw JSON", () => {
    const line = '{"timestamp": "2026-10-03T09:58:56", "level": "INFO", "message": "ready", "port": 8080}';
    // "INFO ready  port=8080  " -> level 4+1, message 5+2, field 4+1+4+2
    expect(displayLength(line, true)).toBe(23);
    expect(displayLength(line, false)).toBe(line.length);
    expect(displayLength("plain text line", true)).toBe(15);
  });
});

