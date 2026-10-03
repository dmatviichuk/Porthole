export type Level = "error" | "warn" | "info" | "debug";

export type ParsedLog =
  | {
      kind: "json";
      level: Level | null;
      /** The level as the app wrote it, e.g. "INFO" or "warning". */
      levelText: string | null;
      message: string | null;
      /** Everything else, in the order the app wrote it. */
      fields: [string, unknown][];
    }
  | {
      kind: "text";
      level: Level | null;
      /** Where the level word sits in the line, to colour just that word. */
      levelAt: [number, number] | null;
    };

const MESSAGE_KEYS = ["message", "msg", "log", "event", "text"];
const LEVEL_KEYS = ["level", "severity", "lvl", "levelname", "loglevel", "log.level", "severity_text"];
// The kubelet timestamp is shown already; repeating the app's own one is noise.
const TIME_KEYS = new Set(["timestamp", "time", "ts", "@timestamp", "datetime", "asctime", "date"]);

const LEVEL_WORDS: Record<string, Level> = {
  fatal: "error",
  panic: "error",
  critical: "error",
  crit: "error",
  emerg: "error",
  alert: "error",
  error: "error",
  err: "error",
  warning: "warn",
  warn: "warn",
  notice: "info",
  info: "info",
  debug: "debug",
  trace: "debug",
};

/** pino/bunyan numeric levels: 10 trace, 20 debug, 30 info, 40 warn, 50 error, 60 fatal. */
function numericLevel(value: number): Level {
  if (value >= 50) return "error";
  if (value >= 40) return "warn";
  if (value >= 30) return "info";
  return "debug";
}

export function levelOf(value: unknown): Level | null {
  if (typeof value === "number") return numericLevel(value);
  if (typeof value !== "string") return null;
  return LEVEL_WORDS[value.trim().toLowerCase()] ?? null;
}

const TEXT_LEVEL = /\b(FATAL|PANIC|CRITICAL|ERROR|ERR|WARNING|WARN|INFO|NOTICE|DEBUG|TRACE)\b/;

const cache = new Map<string, ParsedLog>();
const CACHE_LIMIT = 5_000;

/**
 * Structured view of a log line: JSON objects are split into level, message and the other
 * fields; plain text gets its level word located. Results are cached because the same
 * visible lines render again on every scroll and update.
 */
export function parseLog(text: string): ParsedLog {
  const cached = cache.get(text);
  if (cached) return cached;
  const parsed = parseJson(text) ?? parseText(text);
  if (cache.size >= CACHE_LIMIT) cache.clear();
  cache.set(text, parsed);
  return parsed;
}

function parseJson(text: string): ParsedLog | null {
  const trimmed = text.trim();
  if (!trimmed.startsWith("{") || !trimmed.endsWith("}")) return null;
  let value: unknown;
  try {
    value = JSON.parse(trimmed);
  } catch {
    return null;
  }
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  const entries = Object.entries(value as Record<string, unknown>);
  const pick = (keys: string[]) => entries.find(([k]) => keys.includes(k.toLowerCase()));
  const levelEntry = pick(LEVEL_KEYS);
  const messageEntry = pick(MESSAGE_KEYS);
  const fields = entries.filter(
    ([k]) => k !== levelEntry?.[0] && k !== messageEntry?.[0] && !TIME_KEYS.has(k.toLowerCase()),
  );
  return {
    kind: "json",
    level: levelEntry ? levelOf(levelEntry[1]) : null,
    levelText: levelEntry ? String(levelEntry[1]) : null,
    message: messageEntry ? stringify(messageEntry[1]) : null,
    fields,
  };
}

function parseText(text: string): ParsedLog {
  const match = TEXT_LEVEL.exec(text);
  if (!match) return { kind: "text", level: null, levelAt: null };
  return {
    kind: "text",
    level: levelOf(match[1]),
    levelAt: [match.index, match.index + match[0].length],
  };
}

export const stringify = (value: unknown) => (typeof value === "string" ? value : JSON.stringify(value));

/**
 * About how many characters a line takes on screen, for estimating wrapped heights before
 * they are measured: formatted JSON drops quotes, braces and the time field.
 */
export function displayLength(text: string, formatted: boolean): number {
  if (!formatted) return text.length;
  const parsed = parseLog(text);
  if (parsed.kind === "text") return text.length;
  let length = 0;
  if (parsed.levelText !== null) length += parsed.levelText.length + 1;
  if (parsed.message !== null) length += parsed.message.length + 2;
  for (const [key, value] of parsed.fields) length += key.length + 1 + stringify(value).length + 2;
  return length;
}
