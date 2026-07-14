import assert from "node:assert/strict";
import { test } from "vitest";

import type { SurfaceLoader, SurfaceMount } from "./router.js";
import { createRouteRenderer } from "./router.js";

const deferred = (): {
  load: SurfaceLoader;
  resolve(mount: SurfaceMount): void;
} => {
  let resolve!: (mount: SurfaceMount) => void;
  const promise = new Promise<SurfaceMount>((done) => { resolve = done; });
  return { load: () => promise, resolve };
};

test("a stale slow route never mounts over or clears the winning route", async () => {
  const play = deferred();
  const editor = deferred();
  const content: string[] = [];
  const host = {
    replaceChildren(): void { content.length = 0; },
  } as unknown as HTMLElement;
  const renderer = createRouteRenderer({
    root: host,
    loadEditor: editor.load,
    loadPlay: play.load,
  });

  const stalePlay = renderer.render("/play");
  const winningEditor = renderer.render("/");
  editor.resolve(() => { content.push("editor"); });
  await winningEditor;
  assert.deepEqual(content, ["editor"]);

  play.resolve(() => { content.push("play"); });
  await stalePlay;
  assert.deepEqual(content, ["editor"]);
});

test("only a committed route owns disposal", async () => {
  let editorDisposals = 0;
  const renderer = createRouteRenderer({
    root: { replaceChildren() {} } as unknown as HTMLElement,
    loadEditor: async () => () => () => { editorDisposals += 1; },
    loadPlay: async () => () => undefined,
  });

  await renderer.render("/");
  await renderer.render("/play");
  assert.equal(editorDisposals, 1);
});
