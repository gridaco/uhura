import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

import { test } from "vitest";

const webRoot = path.resolve(import.meta.dirname, "../..");

test("the built Play document preserves host markers and app-last CSS", () => {
  const html = readFileSync(path.join(webRoot, "dist/play/index.html"), "utf8");
  const shellStyle = html.indexOf('href="/shell/assets/');
  const appStyle = html.indexOf('href="/stylesheet.css"');

  assert.notEqual(shellStyle, -1, "built Play HTML must load bundled shell CSS");
  assert.ok(appStyle > shellStyle, "the authored app stylesheet must win the cascade");
  assert.equal(html.match(/uhura-editor-navigation/g)?.length, 1);
});

test("production bundles preserve the Wasm seam and single-file contracts", () => {
  const playAssets = path.join(webRoot, "dist/play/assets");
  const playScripts = readdirSync(playAssets).filter((file) => file.endsWith(".js"));
  assert.equal(playScripts.length, 1);
  const play = readFileSync(path.join(playAssets, playScripts[0]!), "utf8");
  assert.match(play, /\/wasm\/uhura_wasm\.js/);
  assert.match(play, /import\(/);

  assert.deepEqual(readdirSync(path.join(webRoot, "dist/editor")), [
    "canvas-chrome.js",
  ]);

  const provider = path.resolve(
    webRoot,
    "../examples/instagram-uhura/providers/dist/spock.js",
  );
  assert.match(readFileSync(provider, "utf8"), /export\s*\{[^}]*createDriver/);
});
