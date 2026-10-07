import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState } from "react";

import { cx } from "../lib/cx";
import { startShell } from "../lib/ipc";
import { type ShellTab, useApp } from "../store";

function terminalTheme() {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(name).trim();
  return {
    background: v("--bg"),
    foreground: v("--text"),
    cursor: v("--text"),
    selectionBackground: `${v("--accent")}55`,
  };
}

/** One interactive terminal; kept mounted (hidden) while another tab is active. */
export default function ShellView({ shell, visible }: { shell: ShellTab; visible: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const termRef = useRef<Terminal | null>(null);
  const [ended, setEnded] = useState(false);

  useEffect(() => {
    if (!host.current) return;
    const term = new Terminal({
      fontFamily: '"SF Mono", ui-monospace, Menlo, monospace',
      fontSize: 12.5,
      cursorBlink: true,
      scrollback: 10_000,
      theme: terminalTheme(),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host.current);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;

    const dim = (text: string) => term.write(`\r\n\x1b[2m${text}\x1b[0m\r\n`);
    const session = startShell(
      shell,
      { cols: term.cols, rows: term.rows },
      (bytes) => term.write(bytes),
      (event) => {
        if (event.type === "exited") {
          setEnded(true);
          dim(event.code !== null ? `Shell exited with code ${event.code}.` : `Shell ended. ${event.message ?? ""}`);
        } else if (event.type === "error") {
          setEnded(true);
          dim(`Could not open a shell: ${event.message}`);
        }
      },
    );
    const input = term.onData((data) => session.write(data));
    const resize = term.onResize(({ cols, rows }) => session.resize(cols, rows));
    const observer = new ResizeObserver(() => {
      if (host.current?.offsetParent) fit.fit();
    });
    observer.observe(host.current);
    term.focus();

    return () => {
      observer.disconnect();
      input.dispose();
      resize.dispose();
      session.stop();
      term.dispose();
      termRef.current = null;
    };
  }, [shell]);

  useEffect(() => {
    if (visible) {
      fitRef.current?.fit();
      termRef.current?.focus();
    }
  }, [visible]);

  // Follow theme switches, including the OS switching in auto mode.
  useEffect(() => {
    const apply = () => {
      if (termRef.current) termRef.current.options.theme = terminalTheme();
    };
    const unsubscribe = useApp.subscribe((state, previous) => {
      if (state.theme !== previous.theme) apply();
    });
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", apply);
    return () => {
      unsubscribe();
      media.removeEventListener("change", apply);
    };
  }, []);

  return (
    <div data-shell={shell.key} className={cx("absolute inset-0", !visible && "invisible")} aria-hidden={!visible}>
      <div ref={host} className="h-full" />
      {ended && <span className="sr-only">Shell ended</span>}
    </div>
  );
}
