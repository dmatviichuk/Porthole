import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { yaml } from "@codemirror/lang-yaml";
import {
  bracketMatching,
  foldGutter,
  foldKeymap,
  HighlightStyle,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language";
import { highlightSelectionMatches, openSearchPanel, searchKeymap } from "@codemirror/search";
import { EditorState } from "@codemirror/state";
import {
  crosshairCursor,
  drawSelection,
  dropCursor,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { useEffect, useRef } from "react";

import { useFind } from "../lib/find";

// Colours come from CSS variables, so the editor follows the theme without being rebuilt.
const theme = EditorView.theme({
  "&": { backgroundColor: "var(--bg)", color: "var(--text)", fontSize: "12.5px" },
  ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.55" },
  ".cm-content": { caretColor: "var(--text)", padding: "10px 0" },
  ".cm-gutters": { backgroundColor: "var(--bg)", color: "var(--faint)", border: "none" },
  ".cm-activeLine": { backgroundColor: "var(--hover)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--muted)" },
  "&.cm-focused .cm-cursor": { borderLeftColor: "var(--text)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "color-mix(in srgb, var(--accent) 30%, transparent) !important",
  },
  ".cm-foldPlaceholder": { backgroundColor: "var(--selected)", border: "none", color: "var(--muted)" },
  ".cm-panels": { backgroundColor: "var(--raised)", color: "var(--text)" },
  ".cm-searchMatch": { backgroundColor: "color-mix(in srgb, var(--progress) 30%, transparent)" },
  ".cm-tooltip": { backgroundColor: "var(--raised)", border: "1px solid var(--line-strong)" },
});

const highlight = HighlightStyle.define([
  { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: "var(--syn-key)" },
  { tag: [tags.string, tags.special(tags.string), tags.content], color: "var(--syn-string)" },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--syn-number)" },
  { tag: [tags.comment, tags.meta, tags.punctuation, tags.separator], color: "var(--syn-meta)" },
  { tag: [tags.labelName, tags.typeName], color: "var(--syn-number)" },
]);

// What editing YAML needs, picked from the @codemirror packages: no autocompletion (there is
// nothing to complete), no lint keymap (nothing lints) and no auto-closed quotes.
const editing = [
  lineNumbers(),
  highlightActiveLineGutter(),
  highlightSpecialChars(),
  history(),
  foldGutter(),
  drawSelection(),
  dropCursor(),
  EditorState.allowMultipleSelections.of(true),
  indentOnInput(),
  bracketMatching(),
  rectangularSelection(),
  crosshairCursor(),
  highlightActiveLine(),
  highlightSelectionMatches(),
  keymap.of([...defaultKeymap, ...searchKeymap, ...historyKeymap, ...foldKeymap]),
];

export type YamlEditorProps = {
  value: string;
  onChange: (value: string) => void;
  /** Cmd/Ctrl+S */
  onSave?: () => void;
};

/** Uncontrolled CodeMirror; a new `value` that differs from the document replaces it. */
export default function CodeMirrorYaml({ value, onChange, onSave }: YamlEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const handlers = useRef({ onChange, onSave });
  // Cmd/Ctrl+F from outside the editor; inside it, the search keymap opens the panel itself.
  useFind(() => {
    if (view.current) openSearchPanel(view.current);
  });

  useEffect(() => {
    handlers.current = { onChange, onSave };
  });

  useEffect(() => {
    if (!host.current) return;
    const editor = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: "",
        extensions: [
          editing,
          yaml(),
          theme,
          syntaxHighlighting(highlight),
          EditorState.tabSize.of(2),
          EditorView.contentAttributes.of({ "data-primary": "" }),
          keymap.of([
            indentWithTab,
            {
              key: "Mod-s",
              run: () => {
                handlers.current.onSave?.();
                return true;
              },
            },
          ]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) handlers.current.onChange(update.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = editor;
    editor.focus();
    return () => {
      editor.destroy();
      view.current = null;
    };
  }, []);

  useEffect(() => {
    const editor = view.current;
    if (editor && editor.state.doc.toString() !== value) {
      editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: value } });
    }
  }, [value]);

  return <div ref={host} className="selectable h-full min-h-0 overflow-hidden" />;
}
