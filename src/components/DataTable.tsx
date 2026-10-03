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
import { type CSSProperties, type Key, type MouseEvent, type ReactNode, useMemo, useRef, useState } from "react";

import { cx } from "../lib/cx";
import { popupMenu } from "../lib/menu";

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
  onMenu?: (row: T, event: MouseEvent) => void;
  initialSort?: { id: string; desc: boolean };
  empty?: ReactNode;
  rowHeight?: number;
  /**
   * For a table inside a scrolling page: grow to fit the rows so the page is the only scroll,
   * with the header pinned while the table is in view. Past FIT_LIMIT rows it scrolls on its
   * own (virtualized) so a huge list cannot make the page slow.
   */
  fit?: boolean;
};

const features = tableFeatures({
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  sortFns: { alphanumeric: sortFn_alphanumeric, basic: sortFn_basic },
});

const HEADER_HEIGHT = 32;
const FIT_LIMIT = 1000;

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
  onMenu,
  initialSort,
  empty,
  rowHeight = 36,
  fit = false,
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

  const renderRow = (original: T, key: Key, style: CSSProperties, positioned: boolean) => (
    <div
      key={key}
      role="row"
      tabIndex={0}
      onClick={() => onOpen?.(original)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onOpen?.(original);
      }}
      onContextMenu={(e) => {
        if (!onMenu) return;
        e.preventDefault();
        onMenu(original, e);
      }}
      className={cx("grid items-center px-6", positioned && "absolute inset-x-0", onOpen && "hover:bg-hover")}
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
      className={cx(!grow && "overflow-auto", !fit && "h-full")}
      // Past the limit, a fitted table scrolls inside a box rather than growing the page.
      style={fit && !grow ? { maxHeight: "70vh" } : undefined}
      role="grid"
      aria-rowcount={sorted.length}
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
          sorted.map((row) => renderRow(row.original, row.id, { height: rowHeight }, false))
        ) : (
          <div className="relative" style={{ height: virtualizer.getTotalSize() }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = sorted[item.index];
              if (!row) return null;
              return renderRow(
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
