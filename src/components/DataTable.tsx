import {
  createColumnHelper,
  createSortedRowModel,
  type RowData,
  rowSortingFeature,
  sortFn_alphanumeric,
  sortFn_basic,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronDown, ChevronUp } from "lucide-react";
import {
  type CSSProperties,
  type KeyboardEvent,
  type Key,
  type ReactNode,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";

import { cx } from "../lib/cx";
import { mod } from "../lib/keys";
import { entryForKey, type MenuEntry, popupMenu } from "../lib/menu";

export type Column<T> = {
  id: string;
  header: string;
  /** CSS grid track, e.g. "minmax(240px, 3fr)" or "72px". */
  width: string;
  sortValue: (row: T) => string | number | null;
  numeric?: boolean;
  cell: (row: T) => ReactNode;
};

type Props<T extends RowData> = {
  /** Persists the column order under this key. */
  id: string;
  columns: Column<T>[];
  rows: T[];
  getRowId: (row: T) => string;
  onOpen?: (row: T) => void;
  /** The row's right-click menu; its keyed entries also run from the keyboard. */
  menu?: (row: T) => MenuEntry[];
  initialSort?: { id: string; desc: boolean };
  empty?: ReactNode;
  rowHeight?: number;
  /**
   * For a table inside a scrolling page: grow to fit the rows so the page is the only scroll,
   * with the header pinned while the table is in view. Past FIT_LIMIT rows it scrolls on its
   * own (virtualized) so a huge list cannot make the page slow.
   */
  fit?: boolean;
  /** The view's main table: it takes focus when the view opens (see focusPrimary). */
  primary?: boolean;
  /** The search the rows are filtered by: a new search puts the cursor back on the first row. */
  search?: string;
};

const features = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: { alphanumeric: sortFn_alphanumeric, basic: sortFn_basic },
});

const HEADER_HEIGHT = 32;
const FIT_LIMIT = 1000;

/** The focused row of each table, so coming back to a view lands on the row you left. */
const cursors = new Map<string, string>();

function useColumnOrder(tableId: string, ids: string[]) {
  const storageKey = `porthole.columns.${tableId}`;
  const [saved, setSaved] = useState<string[] | null>(() => {
    try {
      const raw = localStorage.getItem(storageKey);
      return raw ? (JSON.parse(raw) as string[]) : null;
    } catch {
      return null;
    }
  });
  // Saved ids first, in their saved order; columns added since then go at the end.
  const order = useMemo(() => {
    const known = (saved ?? []).filter((id) => ids.includes(id));
    return [...known, ...ids.filter((id) => !known.includes(id))];
  }, [saved, ids]);
  const save = (next: string[] | null) => {
    setSaved(next);
    try {
      if (next) localStorage.setItem(storageKey, JSON.stringify(next));
      else localStorage.removeItem(storageKey);
    } catch {
      // Storage blocked: the order lasts until the view closes.
    }
  };
  return [order, save] as const;
}

export function DataTable<T extends RowData>({
  id,
  columns,
  rows,
  getRowId,
  onOpen,
  menu,
  initialSort,
  empty,
  rowHeight = 36,
  fit = false,
  primary = false,
  search = "",
}: Props<T>) {
  const ids = useMemo(() => columns.map((c) => c.id), [columns]);
  const [order, saveOrder] = useColumnOrder(id, ids);
  const ordered = useMemo(
    () => order.map((cid) => columns.find((c) => c.id === cid)).filter((c): c is Column<T> => c !== undefined),
    [order, columns],
  );

  const columnDefs = useMemo(() => {
    const helper = createColumnHelper<typeof features, T>();
    return helper.columns(
      columns.map((c) =>
        helper.accessor((row: T) => c.sortValue(row) ?? undefined, {
          id: c.id,
          sortFn: c.numeric ? "basic" : "alphanumeric",
          sortUndefined: "last",
        }),
      ),
    );
  }, [columns]);

  const table = useTable({
    features,
    columns: columnDefs,
    data: rows,
    getRowId: (row: T) => getRowId(row),
    initialState: { sorting: initialSort ? [initialSort] : [] },
  });
  const sorted = table.getRowModel().rows;
  // Grow with the page: every row in normal flow, no scroll of our own.
  const grow = fit && sorted.length <= FIT_LIMIT;

  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: grow ? 0 : sorted.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
    scrollMargin: HEADER_HEIGHT,
    // Rows scrolled to by the keyboard stop below the sticky header, not under it.
    scrollPaddingStart: HEADER_HEIGHT,
    getItemKey: (index) => sorted[index]?.id ?? index,
  });

  const [dragging, setDragging] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<{ id: string; after: boolean } | null>(null);

  const moveColumn = (from: string, to: string, after: boolean) => {
    if (from === to) return;
    const next = order.filter((c) => c !== from);
    const index = next.indexOf(to) + (after ? 1 : 0);
    next.splice(index, 0, from);
    saveOrder(next);
  };

  const template = ordered.map((c) => c.width).join(" ");

  // The keyboard cursor: one row of the table, kept by id so live updates and re-sorts do not
  // move it. A row that disappears hands the cursor to whichever row takes its place; no cursor
  // (yet, or after a new search) means the first row.
  const domId = useId();
  const rowDomId = (rowId: string) => `${domId}-${rowId}`;
  const [cursorId, setCursorId] = useState<string | null>(() => cursors.get(id) ?? null);
  const [searched, setSearched] = useState(search);
  if (search !== searched) {
    setSearched(search);
    setCursorId(null);
  }
  const lastIndex = useRef(0);
  const found = cursorId === null ? -1 : sorted.findIndex((r) => r.id === cursorId);
  const cursor =
    sorted.length === 0
      ? -1
      : found >= 0
        ? found
        : cursorId === null
          ? 0
          : Math.min(lastIndex.current, sorted.length - 1);
  if (cursor >= 0) lastIndex.current = cursor;
  const cursorRow = cursor >= 0 ? sorted[cursor] : undefined;

  const reveal = (index: number) => {
    const row = sorted[index];
    if (!row) return;
    if (grow) document.getElementById(rowDomId(row.id))?.scrollIntoView({ block: "nearest" });
    else virtualizer.scrollToIndex(index, { align: "auto" });
  };

  const moveTo = (index: number) => {
    const next = sorted[Math.max(0, Math.min(sorted.length - 1, index))];
    if (!next) return;
    setCursorId(next.id);
    cursors.set(id, next.id);
    reveal(sorted.indexOf(next));
  };

  // Opening a view focuses its main table on the row it left from, scrolled into sight.
  useEffect(() => {
    if (!primary) return;
    scrollRef.current?.focus({ preventScroll: true });
    if (cursor > 0) reveal(cursor);
    // Only on mount: later changes move the cursor through moveTo.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const showMenu = (row: T, anchor?: HTMLElement) => {
    if (menu) void popupMenu(menu(row), anchor);
  };

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    // Keys typed into a button or field inside a row belong to it.
    if (e.target !== e.currentTarget || e.altKey || cursor < 0) return;
    const page = Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? 0) / rowHeight) - 1);
    const command = mod(e);
    if (e.ctrlKey && !command) return;
    let handled = true;
    if (command) {
      if (e.key === "ArrowUp") moveTo(0);
      else if (e.key === "ArrowDown") moveTo(sorted.length - 1);
      else handled = false;
    } else if (e.key === "ArrowDown" || e.key === "j") moveTo(cursor + 1);
    else if (e.key === "ArrowUp" || e.key === "k") moveTo(cursor - 1);
    else if (e.key === "Home") moveTo(0);
    else if (e.key === "End") moveTo(sorted.length - 1);
    else if (e.key === "PageDown") moveTo(cursor + page);
    else if (e.key === "PageUp") moveTo(cursor - page);
    else if (e.key === "Enter" && onOpen && cursorRow) onOpen(cursorRow.original);
    else if (cursorRow && menu && (e.key === "m" || (e.key === "F10" && e.shiftKey))) {
      showMenu(cursorRow.original, document.getElementById(rowDomId(cursorRow.id)) ?? undefined);
    } else if (cursorRow && menu && !e.shiftKey && /^[a-z]$/.test(e.key)) {
      const entry = entryForKey(menu(cursorRow.original), e.key);
      if (!entry) handled = false;
      else if ("items" in entry) void popupMenu(entry.items, document.getElementById(rowDomId(cursorRow.id)) ?? undefined);
      else entry.action();
    } else handled = false;
    if (handled) e.preventDefault();
  };

  const renderRow = (rowId: string, original: T, key: Key, style: CSSProperties, positioned: boolean) => (
    <div
      key={key}
      id={rowDomId(rowId)}
      role="row"
      aria-selected={rowId === cursorRow?.id}
      onClick={() => {
        setCursorId(rowId);
        cursors.set(id, rowId);
        onOpen?.(original);
      }}
      onContextMenu={(e) => {
        if (!menu) return;
        e.preventDefault();
        setCursorId(rowId);
        cursors.set(id, rowId);
        showMenu(original);
      }}
      className={cx(
        "grid scroll-mt-8 items-center px-6",
        positioned && "absolute inset-x-0",
        onOpen && "hover:bg-hover",
      )}
      style={{ gridTemplateColumns: template, ...style }}
    >
      {ordered.map((c) => (
        <div key={c.id} role="gridcell" className="flex h-full min-w-0 items-center border-b border-line pr-3">
          <span className="min-w-0 truncate">{c.cell(original)}</span>
        </div>
      ))}
    </div>
  );

  return (
    <div
      ref={scrollRef}
      // One tab stop for the whole table; the arrow keys move between rows.
      tabIndex={0}
      data-primary={primary || undefined}
      onKeyDown={onKeyDown}
      // The context-menu key (or Shift+F10 where the web view turns it into this event) on the
      // focused table opens the menu of the row under the cursor.
      onContextMenu={(e) => {
        if (e.target !== e.currentTarget || !cursorRow) return;
        e.preventDefault();
        showMenu(cursorRow.original, document.getElementById(rowDomId(cursorRow.id)) ?? undefined);
      }}
      className={cx("outline-none", !grow && "overflow-auto", !fit && "h-full")}
      // Past the limit, a fitted table scrolls inside a box rather than growing the page.
      style={fit && !grow ? { maxHeight: "70vh" } : undefined}
      role="grid"
      aria-rowcount={sorted.length}
      aria-activedescendant={cursorRow ? rowDomId(cursorRow.id) : undefined}
    >
      <div className="min-w-fit">
        <div
          role="row"
          className="sticky top-0 z-10 grid border-b border-line bg-bg px-6"
          style={{ gridTemplateColumns: template, height: HEADER_HEIGHT }}
          onContextMenu={(e) => {
            e.preventDefault();
            void popupMenu([{ label: "Reset column order", action: () => saveOrder(null) }]);
          }}
        >
          {ordered.map((c) => {
            const column = table.getColumn(c.id);
            const direction = column?.getIsSorted();
            const marker = dropTarget?.id === c.id && dragging !== c.id;
            return (
              <div
                key={c.id}
                role="columnheader"
                aria-sort={direction === "asc" ? "ascending" : direction === "desc" ? "descending" : "none"}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.effectAllowed = "move";
                  e.dataTransfer.setData("text/plain", c.id);
                  setDragging(c.id);
                }}
                onDragOver={(e) => {
                  if (!dragging) return;
                  e.preventDefault();
                  const rect = e.currentTarget.getBoundingClientRect();
                  setDropTarget({ id: c.id, after: e.clientX > rect.left + rect.width / 2 });
                }}
                onDragLeave={() => setDropTarget((t) => (t?.id === c.id ? null : t))}
                onDrop={(e) => {
                  e.preventDefault();
                  if (dragging && dropTarget) moveColumn(dragging, dropTarget.id, dropTarget.after);
                  setDragging(null);
                  setDropTarget(null);
                }}
                onDragEnd={() => {
                  setDragging(null);
                  setDropTarget(null);
                }}
                className={cx(
                  "relative flex min-w-0 items-center pr-3",
                  dragging === c.id && "opacity-40",
                  marker && (dropTarget.after ? "shadow-[inset_-2px_0_0_var(--accent)]" : "shadow-[inset_2px_0_0_var(--accent)]"),
                )}
              >
                <button
                  type="button"
                  onClick={column?.getToggleSortingHandler()}
                  className={cx(
                    "flex min-w-0 items-center gap-1 text-left text-xs",
                    direction ? "font-semibold text-text" : "text-muted hover:text-text",
                  )}
                  title="Sort; drag to reorder"
                >
                  <span className="truncate">{c.header}</span>
                  {direction === "asc" && <ChevronUp size={12} aria-hidden />}
                  {direction === "desc" && <ChevronDown size={12} aria-hidden />}
                </button>
              </div>
            );
          })}
        </div>

        {sorted.length === 0 ? (
          <div className="px-6 py-10 text-muted">{empty}</div>
        ) : grow ? (
          sorted.map((row) => renderRow(row.id, row.original, row.id, { height: rowHeight }, false))
        ) : (
          <div className="relative" style={{ height: virtualizer.getTotalSize() }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = sorted[item.index];
              if (!row) return null;
              return renderRow(
                row.id,
                row.original,
                item.key,
                { height: item.size, transform: `translateY(${item.start - HEADER_HEIGHT}px)` },
                true,
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
