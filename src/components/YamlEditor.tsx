import { lazy, Suspense } from "react";

import type { YamlEditorProps } from "./CodeMirrorYaml";

// CodeMirror is a large share of the bundle and only the YAML tab needs it,
// so it loads the first time an editor is shown.
const CodeMirrorYaml = lazy(() => import("./CodeMirrorYaml"));

export function YamlEditor(props: YamlEditorProps) {
  return (
    <Suspense fallback={<div className="h-full" />}>
      <CodeMirrorYaml {...props} />
    </Suspense>
  );
}
