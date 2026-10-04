// Removes everything the repo builds or caches; the next build, `pnpm app` or `pnpm check` makes
// it again.
//
//   pnpm clean    (this script overrides pnpm's built-in `clean`, which is still `pnpm purge`)
//
// Installed dependencies stay: only the Vite and Vitest caches inside node_modules go.
import { existsSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));

const TARGETS = [
  ["dist", "UI build"],
  ["src-tauri/target", "Rust builds and app bundles, every profile"],
  ["src-tauri/gen", "schemas Tauri generates while building"],
  ["src-tauri/icons", "app icons rendered from app-icon.svg"],
  ["node_modules/.vite", "Vite and Vitest caches"],
  ["node_modules/.vite-temp", "bundled Vite config"],
];

let removed = 0;
for (const [relative, what] of TARGETS) {
  const target = path.join(root, relative);
  if (!existsSync(target)) continue;
  // Retries ride out Windows briefly holding a file open (an antivirus scan, a closing app).
  rmSync(target, { recursive: true, force: true, maxRetries: 5 });
  console.log(`removed ${relative} (${what})`);
  removed++;
}
if (removed === 0) console.log("nothing to clean");
