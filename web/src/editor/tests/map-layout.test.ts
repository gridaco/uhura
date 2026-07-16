import assert from "node:assert/strict";

import { test } from "vitest";

import {
  layoutInteractionMap,
  MAP_COLUMN_GAP,
  MAP_COLUMN_MAX_HEIGHT,
  MAP_NODE_SCALE,
  MAP_ROW_GAP,
  MAP_SUBCOLUMN_GAP,
  MAP_SURFACE_GAP,
  mapNodePreviewIds,
  scaledMapNodeSize,
  type MapGraph,
} from "../map-layout.js";

// Fixture footprints mirror what the editor feeds the layout: raw frames
// already reduced by MAP_NODE_SCALE (≈ 390x844 pages -> ~156x338).
const PAGE = { width: 160, height: 360 };
const SURFACE = { width: 160, height: 240 };

const uniformSize = (nodeId: string): { width: number; height: number } =>
  nodeId.startsWith("surface:") ? SURFACE : PAGE;

/**
 * Instagram-shaped fixture: feed is the entry, feed navigates to post and
 * profile, profile navigates deeper to followers, orphan is unreachable, and
 * the comments sheet is presented by feed first (then post).
 */
const instagramGraph = (): MapGraph => ({
  entry: "page:feed",
  nodes: [
    { id: "page:orphan", kind: "page" },
    { id: "page:feed", kind: "page" },
    { id: "page:post", kind: "page" },
    { id: "page:profile", kind: "page" },
    { id: "page:followers", kind: "page" },
    { id: "surface:comments-sheet", kind: "surface" },
    { id: "command:feed.like", kind: "command" },
    { id: "dynamic:opener", kind: "dynamic" },
  ],
  edges: [
    { kind: "navigate", from: "page:feed", to: "page:post", event: "post-tapped" },
    { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "author-tapped" },
    { kind: "navigate", from: "page:profile", to: "page:followers", event: "followers-tapped" },
    { kind: "navigate", from: "page:post", to: "page:feed", event: "tab-selected" },
    { kind: "present", from: "page:post", to: "surface:comments-sheet", event: "comments-requested" },
    { kind: "send-command", from: "page:feed", to: "command:feed.like", event: "like-toggled" },
    { kind: "dismiss", from: "surface:comments-sheet", to: "dynamic:opener", event: "dismiss-requested" },
  ],
});

test("columns are navigation depth from the entry page", () => {
  const positions = layoutInteractionMap(instagramGraph(), uniformSize);

  assert.equal(positions.get("page:feed")?.column, 0);
  assert.equal(positions.get("page:post")?.column, 1);
  assert.equal(positions.get("page:profile")?.column, 1);
  assert.equal(positions.get("page:followers")?.column, 2);
  assert.deepEqual(
    { x: positions.get("page:feed")?.x, y: positions.get("page:feed")?.y },
    { x: 0, y: 0 },
    "the entry anchors the map at the origin",
  );
});

test("command and dynamic graph nodes never place", () => {
  const positions = layoutInteractionMap(instagramGraph(), uniformSize);

  assert.equal(positions.has("command:feed.like"), false);
  assert.equal(positions.has("dynamic:opener"), false);
  assert.equal(positions.size, 6, "five pages plus one surface");
});

test("column x advances by the widest member plus the column gap", () => {
  const positions = layoutInteractionMap(instagramGraph(), (nodeId) =>
    nodeId === "page:post" ? { width: 700, height: 900 } : uniformSize(nodeId));

  assert.equal(positions.get("page:post")?.x, PAGE.width + MAP_COLUMN_GAP);
  assert.equal(
    positions.get("page:followers")?.x,
    PAGE.width + MAP_COLUMN_GAP + 700 + MAP_COLUMN_GAP,
    "column 2 clears column 1's widest frame",
  );
});

test("cells in one column stack in BFS discovery order with the row gap", () => {
  const positions = layoutInteractionMap(instagramGraph(), uniformSize);

  assert.equal(positions.get("page:post")?.y, 0, "post is discovered before profile");
  assert.equal(positions.get("page:profile")?.y, PAGE.height + MAP_ROW_GAP);
});

test("a surface hangs below its first presenter by edge order", () => {
  const positions = layoutInteractionMap(instagramGraph(), uniformSize);
  const feed = positions.get("page:feed")!;
  const sheet = positions.get("surface:comments-sheet")!;

  assert.equal(sheet.column, feed.column, "feed presents the sheet before post does");
  assert.equal(sheet.x, feed.x);
  assert.equal(sheet.y, feed.y + PAGE.height + MAP_SURFACE_GAP);
});

test("several surfaces under one page stack with the surface gap", () => {
  const graph = instagramGraph();
  const stacked: MapGraph = {
    ...graph,
    nodes: [...graph.nodes, { id: "surface:share-sheet", kind: "surface" }],
    edges: [
      ...graph.edges,
      { kind: "present", from: "page:feed", to: "surface:share-sheet", event: "share-tapped" },
    ],
  };
  const positions = layoutInteractionMap(stacked, uniformSize);

  assert.equal(
    positions.get("surface:share-sheet")?.y,
    PAGE.height + MAP_SURFACE_GAP + SURFACE.height + MAP_SURFACE_GAP,
  );
  assert.equal(
    positions.get("page:orphan")?.y,
    0,
    "surfaces below feed never push other columns down",
  );
});

test("unreachable pages land in a final column, sorted by name", () => {
  const graph = instagramGraph();
  const twoOrphans: MapGraph = {
    ...graph,
    nodes: [...graph.nodes, { id: "page:admin", kind: "page" }],
  };
  const positions = layoutInteractionMap(twoOrphans, uniformSize);

  assert.equal(positions.get("page:orphan")?.column, 3, "one past the deepest reachable page");
  assert.equal(positions.get("page:admin")?.column, 3);
  assert.equal(positions.get("page:admin")?.y, 0, "admin sorts before orphan");
  assert.equal(positions.get("page:orphan")?.y, PAGE.height + MAP_ROW_GAP);
});

test("a surface no page presents joins the trailing column after pages", () => {
  const graph = instagramGraph();
  const detached: MapGraph = {
    ...graph,
    edges: graph.edges.filter((edge) => edge.kind !== "present"),
  };
  const positions = layoutInteractionMap(detached, uniformSize);

  assert.equal(positions.get("surface:comments-sheet")?.column, 3);
  assert.equal(
    positions.get("surface:comments-sheet")?.y,
    PAGE.height + MAP_ROW_GAP,
    "the trailing column lists unreachable pages first",
  );
});

test("a missing or unknown entry falls back to the first page node", () => {
  const graph = instagramGraph();
  const withoutEntry: MapGraph = { ...graph, entry: undefined };
  const positions = layoutInteractionMap(withoutEntry, uniformSize);

  assert.equal(positions.get("page:orphan")?.column, 0, "the first page node becomes the root");
  assert.equal(positions.get("page:feed")?.column, 1, "everything else is unreachable from it");

  const unknownEntry: MapGraph = { ...graph, entry: "page:nope" };
  assert.equal(layoutInteractionMap(unknownEntry, uniformSize).get("page:orphan")?.column, 0);
});

test("duplicate and self navigate edges never distort depth", () => {
  const graph = instagramGraph();
  const noisy: MapGraph = {
    ...graph,
    edges: [
      { kind: "navigate", from: "page:feed", to: "page:feed", event: "refresh" },
      { kind: "navigate", from: "page:feed", to: "page:post", event: "post-tapped" },
      ...graph.edges,
    ],
  };
  const positions = layoutInteractionMap(noisy, uniformSize);

  assert.equal(positions.get("page:feed")?.column, 0);
  assert.equal(positions.get("page:post")?.column, 1);
});

/** A tab-bar-like fan-out: the entry navigates to six depth-1 pages. */
const fanOutGraph = (): MapGraph => {
  const names = ["a", "b", "c", "d", "e", "f"];
  return {
    entry: "page:feed",
    nodes: [
      { id: "page:feed", kind: "page" },
      ...names.map((name) => ({ id: `page:${name}`, kind: "page" })),
    ],
    edges: names.map((name) => ({
      kind: "navigate",
      from: "page:feed",
      to: `page:${name}`,
      event: `${name}-tapped`,
    })),
  };
};

test("a fan-out column wraps into sub-columns at the height cap", () => {
  const positions = layoutInteractionMap(fanOutGraph(), uniformSize);
  // Pages a/b/c stack to 1176; adding d would reach 1584 > MAP_COLUMN_MAX_HEIGHT.
  assert.ok(
    3 * PAGE.height + 2 * MAP_ROW_GAP <= MAP_COLUMN_MAX_HEIGHT
      && 4 * PAGE.height + 3 * MAP_ROW_GAP > MAP_COLUMN_MAX_HEIGHT,
    "fixture wraps after the third cell",
  );
  const columnX = PAGE.width + MAP_COLUMN_GAP;
  assert.equal(positions.get("page:a")?.x, columnX);
  assert.equal(positions.get("page:c")?.y, (PAGE.height + MAP_ROW_GAP) * 2);
  assert.equal(
    positions.get("page:d")?.x,
    columnX + PAGE.width + MAP_SUBCOLUMN_GAP,
    "the fourth cell starts a side-by-side sub-column",
  );
  assert.equal(positions.get("page:d")?.y, 0);
  assert.equal(positions.get("page:d")?.column, 1, "sub-columns stay in the depth column");
  assert.equal(positions.get("page:e")?.y, PAGE.height + MAP_ROW_GAP);
});

test("the next depth column clears every wrapped sub-column", () => {
  const graph = fanOutGraph();
  const deeper: MapGraph = {
    ...graph,
    nodes: [...graph.nodes, { id: "page:g", kind: "page" }],
    edges: [
      ...graph.edges,
      { kind: "navigate", from: "page:a", to: "page:g", event: "g-tapped" },
    ],
  };
  const positions = layoutInteractionMap(deeper, uniformSize);

  assert.equal(positions.get("page:g")?.column, 2);
  assert.equal(
    positions.get("page:g")?.x,
    PAGE.width + MAP_COLUMN_GAP
      + PAGE.width + MAP_SUBCOLUMN_GAP + PAGE.width
      + MAP_COLUMN_GAP,
    "column 2 starts past both sub-columns of column 1",
  );
});

test("scaledMapNodeSize shrinks a raw footprint by MAP_NODE_SCALE", () => {
  assert.deepEqual(
    scaledMapNodeSize({ width: 390, height: 844 }),
    { width: 390 * MAP_NODE_SCALE, height: 844 * MAP_NODE_SCALE },
  );
  assert.deepEqual(
    scaledMapNodeSize({ width: 100, height: 50 }, 0.5),
    { width: 50, height: 25 },
    "an explicit factor overrides the default",
  );
});

test("scaled footprints compact the whole layout, gaps staying fixed", () => {
  const scaledSize = (nodeId: string): { width: number; height: number } =>
    scaledMapNodeSize(uniformSize(nodeId));
  const positions = layoutInteractionMap(instagramGraph(), scaledSize);

  assert.equal(
    positions.get("page:post")?.x,
    PAGE.width * MAP_NODE_SCALE + MAP_COLUMN_GAP,
    "column 1 starts after the scaled entry frame plus the column gap",
  );
  assert.equal(
    positions.get("page:profile")?.y,
    PAGE.height * MAP_NODE_SCALE + MAP_ROW_GAP,
    "rows stack by scaled heights",
  );
  assert.equal(
    positions.get("surface:comments-sheet")?.y,
    PAGE.height * MAP_NODE_SCALE + MAP_SURFACE_GAP,
    "surfaces hang below the scaled page footprint",
  );
});

test("an empty graph lays out nothing", () => {
  const positions = layoutInteractionMap({ nodes: [], edges: [] }, uniformSize);
  assert.equal(positions.size, 0);
});

test("mapNodePreviewIds picks each definition's first preview", () => {
  const ids = mapNodePreviewIds(instagramGraph().nodes, [
    { id: "component/button/default", identity: { kind: "component", subject: "button" } },
    { id: "page/feed/base", identity: { kind: "page", subject: "feed" } },
    { id: "page/feed/extra", identity: { kind: "page", subject: "feed" } },
    { id: "surface/comments-sheet/open", identity: { kind: "surface", subject: "comments-sheet" } },
    { id: "page/post/default", identity: { kind: "page", subject: "post" } },
  ]);

  assert.deepEqual([...ids], [
    ["page:feed", "page/feed/base"],
    ["page:post", "page/post/default"],
    ["surface:comments-sheet", "surface/comments-sheet/open"],
  ], "definitions without previews (profile, followers, orphan) are absent");
});
