import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDownToLine, Search } from "lucide-react";
import { type ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { cx } from "../lib/cx";
import { useFind } from "../lib/find";
import { logTime, sourceLabels } from "../lib/format";
import { type Stop, streamLogs } from "../lib/ipc";
import { displayLength, type Level, matchRanges, parseLog, type Span, stringify } from "../lib/logFormat";
import { useBooleanPref } from "../lib/prefs";
import type { LogTarget } from "../lib/types";

type Line = { source: number; ts: string | null; text: string; system?: boolean };

const MAX_LINES = 50_000;
/** Lines fetched when the panel opens, split across containers: enough context, quick to show. */
const TAIL_BUDGET = 1_000;
const MIN_TAIL = 50;
const LINE_HEIGHT = 19;
/** Room the row takes besides the text: px-6 padding, the timestamp and the source label. */
const PADDING = 48;
const SOURCE_WIDTH = 150 + 12;
// eslint-disable-next-line no-control-regex
const ANSI = /\x1b\[[0-9;?]*[A-Za-z]/g;
const SOURCE_COLORS = ["#4f9cf9", "#e5a03c", "#3cc97c", "#c47cf0", "#f06f8a", "#3cc3c9"];

const targetKey = (t: LogTarget) => `${t.namespace}/${t.pod}/${t.container}`;

/**
 * Follows every target at once, so a workload's pods read as one log. Targets may
 * change while open (a rollout replaces pods): new ones start streaming, gone ones stop,
 * and the lines they already produced stay.
 */
export function LogsPanel({ context, targets }: { context: string; targets: LogTarget[] }) {
  const lines = useRef<Line[]>([]);
  const sources = useRef<string[]>([]);
  const sessions = useRef(new Map<string, Stop>());
  const [version, setVersion] = useState(0);
  const [filter, setFilter] = useState("");
  const [timestamps, setTimestamps] = useBooleanPref("logs.timestamps", true);
  const [wrap, setWrap] = useBooleanPref("logs.wrap", true);
  const [formatted, setFormatted] = useBooleanPref("logs.format", true);
  const [follow, setFollow] = useState(true);
  // Read by the layout effect that pins the view; state alone would lag a render behind.
  const followRef = useRef(true);
  const [revealed, setRevealed] = useState(false);
  const [box, setBox] = useState({ width: 0, charWidth: 7.2 });
  const scrollRef = useRef<HTMLDivElement>(null);
  const filterRef = useRef<HTMLInputElement>(null);
  useFind(() => {
    filterRef.current?.focus();
    filterRef.current?.select();
  });

  const keys = targets.map(targetKey).toSorted().join("\n");
  const multiple = targets.length > 1;
  const labels = useMemo(() => sourceLabels(targets), [targets]);

  useEffect(() => {
    let frame = 0;
    const bump = () => {
      if (!frame) frame = requestAnimationFrame(() => ((frame = 0), setVersion((v) => v + 1)));
    };
    const append = (more: Line[]) => {
      lines.current.push(...more);
      if (lines.current.length > MAX_LINES) lines.current.splice(0, lines.current.length - MAX_LINES);
      bump();
    };
    const wanted = new Map(targets.map((t) => [targetKey(t), t]));
    for (const [key, stop] of sessions.current) {
      if (!wanted.has(key)) {
        stop();
        sessions.current.delete(key);
      }
    }
    const tailLines = Math.max(MIN_TAIL, Math.floor(TAIL_BUDGET / Math.max(1, wanted.size)));
    for (const [key, target] of wanted) {
      if (sessions.current.has(key)) continue;
      let source = sources.current.indexOf(key);
      if (source < 0) source = sources.current.push(key) - 1;
      const label = `${target.pod}/${target.container}`;
      const stop: Stop = streamLogs(context, [target], { tailLines, previous: false, follow: true }, (event) => {
        // A stopped stream can still deliver a batch that was in flight; it is not ours anymore.
        if (sessions.current.get(key) !== stop) return;
        if (event.type === "lines") {
          append(event.lines.map((l) => ({ source, ts: l.ts, text: l.text.replace(ANSI, "") })));
        } else if (event.type === "ended") {
          append([{ source, ts: null, text: `${label}: log stream ended`, system: true }]);
        } else {
          append([{ source, ts: null, text: `${label}: ${event.message}`, system: true }]);
        }
      });
      sessions.current.set(key, stop);
    }
    return () => cancelAnimationFrame(frame);
    // `keys` captures the target set; `targets` itself is a new array on every pod update.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [context, keys]);

  useEffect(() => {
    const open = sessions.current;
    return () => {
      for (const stop of open.values()) stop();
      open.clear();
      lines.current = [];
    };
  }, [context]);

  const visible = useMemo(() => {
    void version;
    const q = filter.toLowerCase();
    return q ? lines.current.filter((l) => l.text.toLowerCase().includes(q)) : lines.current.slice();
  }, [version, filter]);

  // Track the panel width and the monospace character width, to predict where lines wrap.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const canvas = document.createElement("canvas").getContext("2d");
    if (canvas) canvas.font = getComputedStyle(el).font;
    const charWidth = canvas ? canvas.measureText("0".repeat(100)).width / 100 || 7.2 : 7.2;
    const observer = new ResizeObserver(() => setBox({ width: el.clientWidth, charWidth }));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const textWidth =
    box.width - PADDING - (timestamps ? 12 * box.charWidth + 12 : 0) - (multiple ? SOURCE_WIDTH : 0);
  const charsPerLine = Math.max(20, Math.floor(textWidth / box.charWidth));

  const virtualizer = useVirtualizer({
    count: visible.length,
    getScrollElement: () => scrollRef.current,
    // A close estimate means measuring hardly moves anything, so the view does not jump.
    estimateSize: (index) => {
      const line = visible[index];
      if (!wrap || !line) return LINE_HEIGHT;
      const length = displayLength(line.text, formatted && !line.system);
      return Math.max(1, Math.ceil(length / charsPerLine)) * LINE_HEIGHT;
    },
    overscan: 20,
  });

  // New estimates when what decides a line's height changes.
  useLayoutEffect(() => {
    virtualizer.measure();
  }, [virtualizer, wrap, formatted, timestamps, charsPerLine]);

  // Stay at the bottom while following. Before paint, on every render (new lines, measured
  // heights), so the view never visibly scrolls its way down.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el || !followRef.current) return;
    el.scrollTop = el.scrollHeight;
  });

  // Reveal once the first lines are laid out at the bottom, instead of showing them settle.
  useEffect(() => {
    if (revealed || visible.length === 0) return;
    const frame = requestAnimationFrame(() => setRevealed(true));
    return () => cancelAnimationFrame(frame);
  }, [revealed, visible.length]);

  const followNow = () => {
    followRef.current = true;
    setFollow(true);
  };

  return (
    <div className="relative flex h-full flex-col">
      <div className="flex items-center gap-3 px-6 pb-2">
        <label className="flex h-7 w-[260px] items-center gap-1.5 rounded-md border border-line-strong px-2 focus-within:border-accent">
          <Search size={13} className="text-muted" aria-hidden />
          <input
            ref={filterRef}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter lines"
            aria-label="Filter lines"
            spellCheck={false}
            className="min-w-0 flex-1 bg-transparent outline-none"
          />
        </label>
        <Toggle checked={timestamps} onChange={setTimestamps}>
          Timestamps
        </Toggle>
        <Toggle checked={wrap} onChange={setWrap}>
          Wrap lines
        </Toggle>
        <Toggle checked={formatted} onChange={setFormatted}>
          Format JSON
        </Toggle>
        <span className="ml-auto text-xs text-muted">
          {targets.length === 0 ? "No running containers" : `${targets.length} container${targets.length === 1 ? "" : "s"}`}
          {" · "}
          {visible.length.toLocaleString()} lines
        </span>
        <button
          type="button"
          onClick={() => {
            lines.current = [];
            setVersion((v) => v + 1);
          }}
          className="rounded-md border border-line-strong px-2.5 py-0.5 text-xs hover:bg-hover"
        >
          Clear
        </button>
      </div>
      <div
        ref={scrollRef}
        onScroll={(e) => {
          const el = e.currentTarget;
          const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
          followRef.current = atBottom;
          setFollow(atBottom);
        }}
        style={{ visibility: revealed ? "visible" : "hidden" }}
        className="selectable relative min-h-0 flex-1 overflow-auto border-t border-line font-mono text-[12px]"
      >
        <div className="relative" style={{ height: virtualizer.getTotalSize(), minWidth: wrap ? undefined : "max-content" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const line = visible[item.index];
            if (!line) return null;
            const color = SOURCE_COLORS[line.source % SOURCE_COLORS.length];
            return (
              <div
                key={item.key}
                data-index={item.index}
                ref={wrap ? virtualizer.measureElement : undefined}
                className={cx(
                  "absolute left-0 flex gap-3 px-6 leading-[19px]",
                  wrap ? "right-0" : "whitespace-pre",
                  line.system && "text-muted italic",
                )}
                style={{ transform: `translateY(${item.start}px)` }}
              >
                {timestamps && <span className="shrink-0 text-faint">{logTime(line.ts).padEnd(12, " ")}</span>}
                {multiple && (
                  <span className="w-[150px] shrink-0 truncate" style={{ color }} title={sources.current[line.source]}>
                    {labels.get(sources.current[line.source] ?? "") ?? sources.current[line.source]?.split("/")[1]}
                  </span>
                )}
                <span className={cx(wrap && "min-w-0 break-all whitespace-pre-wrap")}>
                  {formatted && !line.system ? (
                    <LogText text={line.text} filter={filter} />
                  ) : (
                    <Marked text={line.text} ranges={matchRanges(line.text, filter)} />
                  )}
                </span>
              </div>
            );
          })}
        </div>
      </div>
      {!follow && visible.length > 0 && (
        <button
          type="button"
          onClick={followNow}
          className="absolute right-8 bottom-6 flex items-center gap-1.5 rounded-full border border-line-strong bg-raised px-3 py-1.5 text-xs shadow-lg hover:bg-selected"
        >
          <ArrowDownToLine size={13} aria-hidden />
          Follow
        </button>
      )}
    </div>
  );
}

const LEVEL_CLASS: Record<Level, string> = {
  error: "text-failed",
  warn: "text-progress",
  info: "text-ok",
  debug: "text-faint",
};

/**
 * JSON lines as level, message and `key=value` fields; plain lines with their level word
 * coloured. The filter still matches the raw text; what it matched is marked where shown.
 */
function LogText({ text, filter }: { text: string; filter: string }) {
  const parsed = parseLog(text);
  if (parsed.kind === "text") {
    const ranges = matchRanges(text, filter);
    if (!parsed.level || !parsed.levelAt) return <Marked text={text} ranges={ranges} />;
    const [start, end] = parsed.levelAt;
    return (
      <>
        <Marked text={text.slice(0, start)} ranges={ranges} />
        <span className={cx("font-semibold", LEVEL_CLASS[parsed.level])}>
          <Marked text={text.slice(start, end)} ranges={ranges} offset={start} />
        </span>
        <Marked text={text.slice(end)} ranges={ranges} offset={end} />
      </>
    );
  }
  // Each part is shown differently from how it reads in the raw line, so each is matched on its own.
  const mark = (part: string) => <Marked text={part} ranges={matchRanges(part, filter)} />;
  return (
    <>
      {parsed.levelText !== null && (
        <span className={cx("mr-2 font-semibold", parsed.level ? LEVEL_CLASS[parsed.level] : "text-muted")}>
          {mark(parsed.levelText.toUpperCase())}
        </span>
      )}
      {parsed.message !== null && <span className="mr-3">{mark(parsed.message)}</span>}
      {parsed.fields.map(([key, value]) => (
        <span key={key} className="mr-3">
          <span className="text-[var(--syn-key)]">{mark(key)}</span>
          <span className="text-faint">=</span>
          <FieldValue value={value} mark={mark} />
        </span>
      ))}
    </>
  );
}

function FieldValue({ value, mark }: { value: unknown; mark: (part: string) => ReactNode }) {
  if (value === null) return <span className="text-faint">{mark("null")}</span>;
  if (typeof value === "string") return <span className="text-[var(--syn-string)]">{mark(value)}</span>;
  if (typeof value === "number" || typeof value === "boolean") {
    return <span className="text-[var(--syn-number)]">{mark(String(value))}</span>;
  }
  return <span className="text-muted">{mark(stringify(value))}</span>;
}

/** `text` with the filter matches marked; `ranges` count from the line start, `text` starts at `offset`. */
function Marked({ text, ranges, offset = 0 }: { text: string; ranges: Span[]; offset?: number }) {
  const parts: ReactNode[] = [];
  let at = 0;
  for (const [start, end] of ranges) {
    const from = Math.max(start - offset, at);
    const to = Math.min(end - offset, text.length);
    if (to <= from) continue;
    parts.push(
      text.slice(at, from),
      <mark key={from} className="rounded-[2px] bg-progress/30 text-inherit">
        {text.slice(from, to)}
      </mark>,
    );
    at = to;
  }
  if (parts.length === 0) return text;
  parts.push(text.slice(at));
  return <>{parts}</>;
}

function Toggle({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: string }) {
  return (
    <label className="flex items-center gap-1.5 text-xs text-muted">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} className="accent-accent" />
      {children}
    </label>
  );
}
