import { useCallback, useEffect, useState } from "react";

import { ClusterLabelDialog } from "./components/ClusterLabelDialog";
import { CommandPalette } from "./components/CommandPalette";
import { ConfirmDelete } from "./components/ConfirmDelete";
import { Dock } from "./components/Dock";
import { Notices } from "./components/Notices";
import { Shortcuts } from "./components/Shortcuts";
import { ShortcutsDialog } from "./components/ShortcutsDialog";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { listContexts, reloadKubeconfig } from "./lib/ipc";
import type { Contexts } from "./lib/types";
import { forgetDiscovery } from "./lib/discovery";
import { warmContext } from "./lib/warm";
import { closeAllWatches } from "./lib/watchStore";
import { lastContext, useApp, useView } from "./store";
import { ApplicationsView } from "./views/ApplicationsView";
import { Placeholder } from "./views/common";
import { ResourcePage } from "./views/ResourcePage";
import { ResourcesView } from "./views/ResourcesView";
import { ClusterOverview } from "./views/ClusterOverview";

export default function App() {
  const [contexts, setContexts] = useState<Contexts | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Bumped on kubeconfig reload so every view remounts against fresh watches.
  const [generation, setGeneration] = useState(0);
  const context = useApp((s) => s.context);
  const setContext = useApp((s) => s.setContext);

  const adopt = useCallback(
    (result: Contexts) => {
      setContexts(result);
      setError(null);
      const names = result.contexts.map((c) => c.name);
      const current = useApp.getState().context;
      const remembered = lastContext();
      const pick = [current, remembered, result.current, names[0]].find((n) => n && names.includes(n));
      if (pick && pick !== current) setContext(pick);
    },
    [setContext],
  );

  useEffect(() => {
    listContexts().then(adopt, (err: unknown) => setError(String(err)));
  }, [adopt]);

  const reload = useCallback(() => {
    closeAllWatches();
    forgetDiscovery();
    reloadKubeconfig().then(
      (result) => {
        adopt(result);
        setGeneration((g) => g + 1);
      },
      (err: unknown) => setError(String(err)),
    );
  }, [adopt]);

  return (
    <div className="flex h-full">
      {context && <Warm key={generation} context={context} />}
      <Sidebar key={`sidebar-${generation}`} contexts={contexts?.contexts ?? []} onReload={reload} />
      <main className="flex min-w-0 flex-1 flex-col">
        <TopBar />
        <div key={`${context}-${generation}`} className="min-h-0 flex-1">
          {error ? (
            <Placeholder>
              <span className="selectable">
                Could not read your kubeconfig: {error}. Porthole reads $KUBECONFIG or ~/.kube/config.
              </span>
            </Placeholder>
          ) : contexts && contexts.contexts.length === 0 ? (
            <Placeholder>No contexts in your kubeconfig. Add a cluster with kubectl, then reload.</Placeholder>
          ) : (
            <CurrentView />
          )}
        </div>
        <Dock />
      </main>
      <ConfirmDelete />
      <ClusterLabelDialog />
      <CommandPalette contexts={contexts?.contexts ?? []} onReload={reload} />
      <ShortcutsDialog />
      <Shortcuts />
      <Notices />
    </div>
  );
}

/** Starts everything the main views read as soon as a context is picked, and keeps it live. */
function Warm({ context }: { context: string }) {
  useEffect(() => warmContext(context), [context]);
  return null;
}

function CurrentView() {
  const view = useView();
  switch (view.name) {
    case "applications":
      return <ApplicationsView />;
    case "overview":
      return <ClusterOverview />;
    case "resources":
      return <ResourcesView resource={view.resource} />;
    case "resource":
      return <ResourcePage key={`${view.resource.plural}/${view.namespace}/${view.object}`} view={view} />;
  }
}
