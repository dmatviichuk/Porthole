// Builds the desktop app for this machine.
//
//   pnpm build:dev      `fast` cargo profile, one bundle type: seconds once dependencies are built
//   pnpm build:release  `release` profile (LTO, one codegen unit) and every bundle type, as CI ships
//
// The two profiles build into separate directories (target/fast, target/release), so switching
// between them never recompiles the other's dependencies.
import { spawnSync } from "node:child_process";
import process from "node:process";

const mode = process.argv[2];
if (mode !== "dev" && mode !== "release") {
  console.error("usage: node scripts/build.mjs <dev|release>");
  process.exit(2);
}

// One quick-to-make bundle for trying a build locally; the full set for a release.
const BUNDLES = {
  darwin: { dev: ["app"], release: ["app", "dmg"] },
  win32: { dev: ["nsis"], release: ["nsis", "msi"] },
  linux: { dev: ["deb"], release: ["appimage", "deb", "rpm"] },
};
const bundles = BUNDLES[process.platform]?.[mode];

const run = (args) => {
  const result = spawnSync("pnpm", ["exec", ...args], { stdio: "inherit", shell: process.platform === "win32" });
  if (result.status !== 0) process.exit(result.status ?? 1);
};

run(["tauri", "icon", "src-tauri/app-icon.svg"]);
run([
  "tauri",
  "build",
  ...(bundles ? ["--bundles", bundles.join(",")] : []),
  ...(mode === "dev" ? ["--", "--profile", "fast"] : []),
]);
