import { create } from "zustand";

import { type ClusterLabel, loadLabels, saveLabels } from "./lib/clusterLabels";
import { applyTheme, savedTheme, type ThemePref } from "./lib/theme";
import type { ResourceType } from "./lib/types";

export type ResourceTab = "overview" | "logs" | "events" | "yaml";

export type View =
  | { name: "applications" }
  | { name: "overview" }
  | { name: "resources"; resource: ResourceType | null }
  | { name: "resource"; resource: ResourceType; namespace: string | null; object: string; tab: ResourceTab };

export type ShellTab = {
  key: string;
  context: string;
  namespace: string;
  pod: string;
  container: string;
};

export type DeleteRequest = {
  context: string;
  resource: ResourceType;
  namespace: string | null;
  name: string;
};

export type Notice = { id: number; tone: "ok" | "error"; text: string };

type AppState = {
  context: string | null;
  /** null means all namespaces. */
  namespace: string | null;
  search: string;
  history: View[];
  index: number;
  theme: ThemePref;
  shells: ShellTab[];
  activeShell: string | null;
  dockHeight: number;
  pendingDelete: DeleteRequest | null;
  /** User-set tag, colour and delete protection per kubeconfig context. */
  labels: Record<string, ClusterLabel>;
  /** The context whose label dialog is open. */
  labelling: string | null;
  notices: Notice[];

  setContext: (context: string) => void;
  setNamespace: (namespace: string | null) => void;
  /**
   * A namespace picked in the sidebar: filters Browse in place, and switches to Browse from
   * views that would otherwise not change (Overview, a resource page).
   */
  browseNamespace: (namespace: string | null) => void;
  setSearch: (search: string) => void;
  navigate: (view: View) => void;
  /** Changes the current view without a history entry (switching tabs of one resource). */
  replace: (view: View) => void;
  back: () => void;
  forward: () => void;
  setTheme: (theme: ThemePref) => void;
  openShell: (shell: Omit<ShellTab, "key">) => void;
  closeShell: (key: string) => void;
  setActiveShell: (key: string) => void;
  setDockHeight: (height: number) => void;
  requestDelete: (request: DeleteRequest | null) => void;
  setLabel: (context: string, label: ClusterLabel | null) => void;
  editLabel: (context: string | null) => void;
  notify: (tone: Notice["tone"], text: string) => void;
  dismiss: (id: number) => void;
};

// Per-viewer conveniences only; the app works the same when storage is unavailable.
const saved = {
  get(key: string): string | null {
    try {
      return localStorage.getItem(`porthole.${key}`);
    } catch {
      return null;
    }
  },
  set(key: string, value: string | null) {
    try {
      if (value === null) localStorage.removeItem(`porthole.${key}`);
      else localStorage.setItem(`porthole.${key}`, value);
    } catch {
      // Storage blocked: nothing to remember.
    }
  },
};

export const lastContext = () => saved.get("context");

const HOME: View = { name: "applications" };
let noticeId = 0;

export const useApp = create<AppState>((set, get) => ({
  context: null,
  namespace: null,
  search: "",
  history: [HOME],
  index: 0,
  theme: savedTheme(),
  shells: [],
  activeShell: null,
  dockHeight: Number(saved.get("dockHeight")) || 280,
  pendingDelete: null,
  labels: loadLabels(),
  labelling: null,
  notices: [],

  setContext: (context) => {
    saved.set("context", context);
    const namespace = saved.get(`namespace.${context}`);
    set({ context, namespace: namespace || null, history: [HOME], index: 0, search: "" });
  },
  setNamespace: (namespace) => {
    const { context } = get();
    if (context) saved.set(`namespace.${context}`, namespace);
    set({ namespace });
  },
  browseNamespace: (namespace) => {
    get().setNamespace(namespace);
    const { history, index } = get();
    const current = history[index]?.name;
    if (current === "applications" || current === "resources") return;
    get().navigate({ name: "applications" });
  },
  setSearch: (search) => set({ search }),
  navigate: (view) =>
    set((state) => ({
      history: [...state.history.slice(0, state.index + 1), view],
      index: state.index + 1,
      search: "",
    })),
  replace: (view) =>
    set((state) => ({ history: state.history.map((v, i) => (i === state.index ? view : v)) })),
  back: () => set((state) => ({ index: Math.max(0, state.index - 1), search: "" })),
  forward: () => set((state) => ({ index: Math.min(state.history.length - 1, state.index + 1), search: "" })),
  setTheme: (theme) => {
    applyTheme(theme);
    set({ theme });
  },
  openShell: (shell) => {
    const key = `${shell.context}/${shell.namespace}/${shell.pod}/${shell.container}/${Date.now()}`;
    set((state) => ({ shells: [...state.shells, { ...shell, key }], activeShell: key }));
  },
  closeShell: (key) =>
    set((state) => {
      const index = state.shells.findIndex((s) => s.key === key);
      const shells = state.shells.filter((s) => s.key !== key);
      const activeShell =
        state.activeShell === key ? (shells[Math.min(index, shells.length - 1)]?.key ?? null) : state.activeShell;
      return { shells, activeShell };
    }),
  setActiveShell: (activeShell) => set({ activeShell }),
  setDockHeight: (dockHeight) => {
    saved.set("dockHeight", String(Math.round(dockHeight)));
    set({ dockHeight });
  },
  requestDelete: (pendingDelete) => set({ pendingDelete }),
  setLabel: (context, label) =>
    set((state) => {
      const labels = { ...state.labels };
      if (label) labels[context] = label;
      else delete labels[context];
      saveLabels(labels);
      return { labels };
    }),
  editLabel: (labelling) => set({ labelling }),
  notify: (tone, text) => {
    const id = ++noticeId;
    set((state) => ({ notices: [...state.notices.slice(-3), { id, tone, text }] }));
    window.setTimeout(() => get().dismiss(id), tone === "error" ? 8000 : 4000);
  },
  dismiss: (id) => set((state) => ({ notices: state.notices.filter((n) => n.id !== id) })),
}));

export const useView = () => useApp((s) => s.history[s.index] ?? HOME);
