import assert from "node:assert/strict";

import { test } from "vitest";

import {
  applyMapOverrides,
  draggedMapPosition,
  isDragGesture,
  MAP_DRAG_THRESHOLD_PX,
  retainMapOverrides,
  setMapOverride,
} from "../map-interaction.js";
import type { MapNodePosition } from "../map-layout.js";

test("pointer movement below the threshold classifies as a click", () => {
  assert.equal(isDragGesture({ x: 10, y: 10 }, { x: 10, y: 10 }), false);
  assert.equal(isDragGesture({ x: 10, y: 10 }, { x: 12, y: 12 }), false, "≈2.8px diagonal");
  assert.equal(isDragGesture({ x: 10, y: 10 }, { x: 13, y: 13 }), true, "≈4.2px diagonal");
  assert.equal(
    isDragGesture({ x: 0, y: 0 }, { x: MAP_DRAG_THRESHOLD_PX, y: 0 }),
    true,
    "exactly the threshold drags",
  );
  assert.equal(isDragGesture({ x: 0, y: 0 }, { x: 0, y: 8 }, 10), false, "custom threshold");
});

test("draggedMapPosition converts the pointer delta into board units", () => {
  assert.deepEqual(
    draggedMapPosition({ x: 100, y: 40 }, { x: 0, y: 0 }, { x: 200, y: -50 }, 1),
    { x: 300, y: -10 },
  );
  assert.deepEqual(
    draggedMapPosition({ x: 100, y: 40 }, { x: 0, y: 0 }, { x: 200, y: 100 }, 0.5),
    { x: 500, y: 240 },
    "a zoomed-out camera doubles the board-space delta",
  );
});

test("draggedMapPosition treats a degenerate camera scale as identity", () => {
  assert.deepEqual(
    draggedMapPosition({ x: 0, y: 0 }, { x: 0, y: 0 }, { x: 10, y: 10 }, 0),
    { x: 10, y: 10 },
  );
});

test("setMapOverride returns a new map and never mutates the input", () => {
  const before = new Map([["page:feed", { x: 1, y: 2 }]]);
  const after = setMapOverride(before, "page:post", { x: 3, y: 4 });

  assert.notEqual(after, before);
  assert.equal(before.size, 1, "the original map is untouched");
  assert.deepEqual(after.get("page:post"), { x: 3, y: 4 });
  assert.deepEqual(after.get("page:feed"), { x: 1, y: 2 });
  assert.deepEqual(
    setMapOverride(after, "page:feed", { x: 9, y: 9 }).get("page:feed"),
    { x: 9, y: 9 },
    "re-dragging a node replaces its override",
  );
});

test("retainMapOverrides keeps overrides only for still-existing nodes", () => {
  const overrides = new Map([
    ["page:feed", { x: 1, y: 2 }],
    ["page:removed", { x: 3, y: 4 }],
  ]);
  const retained = retainMapOverrides(overrides, new Set(["page:feed", "page:post"]));

  assert.notEqual(retained, overrides);
  assert.deepEqual([...retained.keys()], ["page:feed"]);
  assert.equal(overrides.size, 2, "the original map is untouched");
});

test("applyMapOverrides moves overridden nodes and keeps the rest derived", () => {
  const derived = new Map<string, MapNodePosition>([
    ["page:feed", { x: 0, y: 0, column: 0 }],
    ["page:post", { x: 228, y: 0, column: 1 }],
  ]);
  const merged = applyMapOverrides(derived, new Map([["page:post", { x: 500, y: 60 }]]));

  assert.deepEqual(merged.get("page:post"), { x: 500, y: 60, column: 1 }, "column survives");
  assert.deepEqual(merged.get("page:feed"), { x: 0, y: 0, column: 0 });
  assert.deepEqual(
    derived.get("page:post"),
    { x: 228, y: 0, column: 1 },
    "the derived layout is untouched",
  );
});

test("applyMapOverrides ignores overrides for nodes the layout no longer has", () => {
  const derived = new Map<string, MapNodePosition>([["page:feed", { x: 0, y: 0, column: 0 }]]);
  const merged = applyMapOverrides(derived, new Map([["page:gone", { x: 5, y: 5 }]]));

  assert.deepEqual([...merged.keys()], ["page:feed"]);
});
