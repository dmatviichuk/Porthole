import { Check } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { type ClusterLabel, LABEL_COLORS, type LabelColor } from "../lib/clusterLabels";
import { cx } from "../lib/cx";
import { useApp } from "../store";

const COLORS = Object.keys(LABEL_COLORS) as LabelColor[];
const BLANK: ClusterLabel = { tag: "", color: "gray", confirmDeletes: false };

/** Tag, colour and delete protection for one cluster, set by the user. */
export function ClusterLabelDialog() {
  const context = useApp((s) => s.labelling);
  const labels = useApp((s) => s.labels);
  const setLabel = useApp((s) => s.setLabel);
  const editLabel = useApp((s) => s.editLabel);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [draft, setDraft] = useState<ClusterLabel>(BLANK);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (context && !dialog.open) {
      setDraft(labels[context] ?? BLANK);
      dialog.showModal();
    } else if (!context && dialog.open) {
      dialog.close();
    }
  }, [context, labels]);

  if (!context) return <dialog ref={dialogRef} />;

  const existing = labels[context];
  const close = () => editLabel(null);
  const save = () => {
    const tag = draft.tag.trim();
    // A label with nothing to show and no protection is no label at all.
    setLabel(context, tag || draft.confirmDeletes ? { ...draft, tag } : null);
    close();
  };

  return (
    <dialog
      ref={dialogRef}
      onClose={close}
      className="m-auto w-[420px] rounded-xl border border-line-strong bg-bg p-0 text-text shadow-2xl"
    >
      <form
        method="dialog"
        className="space-y-4 p-5"
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
      >
        <div>
          <h2 className="text-[15px] font-semibold">Label cluster</h2>
          <p className="selectable truncate text-muted" title={context}>
            {context}
          </p>
        </div>

        <label className="block space-y-1.5">
          <span className="text-muted">Tag</span>
          <input
            autoFocus
            value={draft.tag}
            onChange={(e) => setDraft({ ...draft, tag: e.target.value })}
            placeholder="e.g. Production, Staging, Team A"
            maxLength={40}
            spellCheck={false}
            className="block w-full rounded-md border border-line-strong bg-raised px-2 py-1.5 outline-none focus:border-accent"
          />
        </label>

        <fieldset className="space-y-1.5">
          <legend className="mb-1.5 text-muted">Colour</legend>
          <div className="flex gap-2">
            {COLORS.map((color) => (
              <button
                key={color}
                type="button"
                role="radio"
                aria-checked={draft.color === color}
                aria-label={color}
                title={color}
                onClick={() => setDraft({ ...draft, color })}
                className={cx(
                  "flex size-7 items-center justify-center rounded-full ring-offset-2 ring-offset-bg",
                  draft.color === color && "ring-2 ring-text",
                )}
                style={{ backgroundColor: LABEL_COLORS[color] }}
              >
                {draft.color === color && <Check size={14} className="text-white" aria-hidden />}
              </button>
            ))}
          </div>
        </fieldset>

        <label className="flex items-start gap-2">
          <input
            type="checkbox"
            checked={draft.confirmDeletes}
            onChange={(e) => setDraft({ ...draft, confirmDeletes: e.target.checked })}
            className="mt-0.5 accent-accent"
          />
          <span>
            Ask to type the name before deleting anything
            <span className="block text-xs text-muted">For clusters where a slip is expensive.</span>
          </span>
        </label>

        {draft.tag.trim() && (
          <p className="text-xs text-muted">
            Preview:{" "}
            <span className="font-medium" style={{ color: LABEL_COLORS[draft.color] }}>
              {draft.tag.trim()}
            </span>
          </p>
        )}

        <div className="flex items-center gap-2 pt-1">
          {existing && (
            <button
              type="button"
              onClick={() => {
                setLabel(context, null);
                close();
              }}
              className="mr-auto rounded-md px-2 py-1.5 text-muted hover:bg-hover hover:text-text"
            >
              Remove label
            </button>
          )}
          <button
            type="button"
            onClick={close}
            className="ml-auto rounded-md border border-line-strong px-3 py-1.5 hover:bg-hover"
          >
            Cancel
          </button>
          <button type="submit" className="rounded-md bg-accent px-3 py-1.5 font-medium text-white">
            Save
          </button>
        </div>
      </form>
    </dialog>
  );
}
