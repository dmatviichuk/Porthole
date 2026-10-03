import { useEffect, useRef, useState } from "react";

import { deleteResource } from "../lib/ipc";
import { useApp } from "../store";

const kindLabel = (kind: string) => kind.replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase();

/**
 * Confirms every delete. Namespaces, and anything on a cluster whose label asks for it, need the
 * name typed back: one slip there removes far more than a pod.
 */
export function ConfirmDelete() {
  const request = useApp((s) => s.pendingDelete);
  const requestDelete = useApp((s) => s.requestDelete);
  const notify = useApp((s) => s.notify);
  const labels = useApp((s) => s.labels);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (request && !dialog.open) {
      setTyped("");
      dialog.showModal();
    } else if (!request && dialog.open) {
      dialog.close();
    }
  }, [request]);

  if (!request) return <dialog ref={dialogRef} />;

  const kind = kindLabel(request.resource.kind);
  const protectedCluster = labels[request.context]?.confirmDeletes === true;
  const mustType = request.resource.kind === "Namespace" || protectedCluster;
  const ready = !mustType || typed === request.name;

  const confirm = async () => {
    setBusy(true);
    try {
      await deleteResource(request.context, request.resource, request.namespace, request.name);
      notify("ok", `Deleted ${kind} ${request.name}`);
      requestDelete(null);
    } catch (err) {
      notify("error", `Could not delete ${kind} ${request.name}: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <dialog
      ref={dialogRef}
      onClose={() => requestDelete(null)}
      className="m-auto w-[440px] rounded-xl border border-line-strong bg-bg p-0 text-text shadow-2xl"
    >
      <form
        method="dialog"
        className="space-y-3 p-5"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready && !busy) void confirm();
        }}
      >
        <h2 className="text-[15px] font-semibold">
          Delete {kind} {request.name}?
        </h2>
        <p className="text-muted">
          {request.resource.kind === "Namespace"
            ? "Every resource in this namespace is deleted with it. This cannot be undone."
            : request.resource.kind === "Pod"
              ? "If a controller manages this pod, it starts a replacement."
              : "This cannot be undone."}
        </p>
        <p className="text-muted">
          Cluster <span className="font-medium text-text">{request.context}</span>
          {request.namespace && (
            <>
              , namespace <span className="font-medium text-text">{request.namespace}</span>
            </>
          )}
          .
        </p>
        {mustType && (
          <label className="block space-y-1.5">
            <span className="text-muted">
              Type <span className="selectable font-mono text-text">{request.name}</span> to confirm
            </span>
            <input
              autoFocus
              value={typed}
              onChange={(e) => setTyped(e.target.value)}
              spellCheck={false}
              className="block w-full rounded-md border border-line-strong bg-raised px-2 py-1.5 font-mono outline-none focus:border-accent"
            />
          </label>
        )}
        <div className="flex justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={() => requestDelete(null)}
            className="rounded-md border border-line-strong px-3 py-1.5 hover:bg-hover"
          >
            Cancel
          </button>
          <button
            type="submit"
            autoFocus={!mustType}
            disabled={!ready || busy}
            className="rounded-md bg-failed px-3 py-1.5 font-medium text-white disabled:opacity-40"
          >
            {busy ? "Deleting…" : `Delete ${kind}`}
          </button>
        </div>
      </form>
    </dialog>
  );
}
