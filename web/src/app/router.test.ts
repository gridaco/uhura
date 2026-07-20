import assert from "node:assert/strict";
import { test } from "vitest";

import type { SurfaceLoader, SurfaceMount } from "./router.js";
import {
  createRouteRenderer,
  EDITOR_PATH,
  routeFor,
} from "./router.js";

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

test("only reserved editor entry points select Editor", () => {
  assert.equal(routeFor("/").surface, "editor");
  assert.equal(routeFor(EDITOR_PATH).surface, "editor");
  assert.equal(routeFor(`${EDITOR_PATH}/`).surface, "editor");

  assert.equal(routeFor("/play").surface, "play");
  assert.equal(routeFor("/play/").surface, "play");
  assert.equal(routeFor("/returns/return-100").surface, "play");
  assert.equal(routeFor("/_uhura/editor/preferences").surface, "play");
});

test("real application locations keep one running Play surface", async () => {
  let playLoads = 0;
  let playMounts = 0;
  let playDisposals = 0;
  const commits: string[] = [];
  const renderer = createRouteRenderer({
    root: { replaceChildren() {} } as unknown as HTMLElement,
    loadEditor: async () => () => undefined,
    loadPlay: async () => {
      playLoads += 1;
      return () => {
        playMounts += 1;
        return () => { playDisposals += 1; };
      };
    },
    committed(route) {
      commits.push(route.pathname);
    },
  });

  await renderer.render("/play");
  await renderer.render("/returns");
  await renderer.render("/returns/return-100");

  assert.equal(playLoads, 1);
  assert.equal(playMounts, 1);
  assert.equal(playDisposals, 0);
  assert.deepEqual(commits, [
    "/play",
    "/returns",
    "/returns/return-100",
  ]);

  await renderer.render(EDITOR_PATH);
  assert.equal(playDisposals, 1);
});
