import assert from "node:assert/strict";

import { test } from "vitest";

import {
  roundedOrthogonalPath,
  roundedStructurePath,
  shouldReplayStructureDraw,
  structureConnectorGlyph,
  structureConnectorLabelSegments,
  structureDrawDelayMs,
  structurePathWaypoints,
} from "../structure-presentation.js";

test("a straight run renders without any corner arc", () => {
  assert.equal(
    roundedOrthogonalPath([{ x: 0, y: 0 }, { x: 100, y: 0 }]),
    "M 0 0 L 100 0",
  );
});

test("a single 90° turn rounds with the full 8-unit radius", () => {
  assert.equal(
    roundedOrthogonalPath([
      { x: 0, y: 0 },
      { x: 100, y: 0 },
      { x: 100, y: 100 },
    ]),
    "M 0 0 L 92 0 Q 100 0 100 8 L 100 100",
  );
});

test("the radius clamps to half the shorter adjacent segment", () => {
  assert.equal(
    roundedOrthogonalPath([
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 10, y: 100 },
    ]),
    "M 0 0 L 5 0 Q 10 0 10 5 L 10 100",
    "a 10-unit inbound segment caps the arc at 5",
  );
});

test("consecutive corners sharing a short segment never overlap their arcs", () => {
  const path = roundedOrthogonalPath([
    { x: 0, y: 0 },
    { x: 10, y: 0 },
    { x: 10, y: 10 },
    { x: 100, y: 10 },
  ]);
  assert.equal(path, "M 0 0 L 5 0 Q 10 0 10 5 L 10 5 Q 10 10 15 10 L 100 10");
});

test("zero-length segments drop and collinear or reversing waypoints stay straight", () => {
  assert.equal(
    roundedOrthogonalPath([
      { x: 0, y: 0 },
      { x: 0, y: 0 },
      { x: 50, y: 0 },
      { x: 100, y: 0 },
    ]),
    "M 0 0 L 50 0 L 100 0",
    "duplicates dedupe, collinear midpoints draw no arc",
  );
  assert.equal(
    roundedOrthogonalPath([
      { x: 0, y: 0 },
      { x: 100, y: 0 },
      { x: 50, y: 0 },
    ]),
    "M 0 0 L 100 0 L 50 0",
    "a 180° reversal draws straight through rather than arcing",
  );
});

test("degenerate inputs stay total", () => {
  assert.equal(roundedOrthogonalPath([]), "");
  assert.equal(roundedOrthogonalPath([{ x: 4, y: 5 }]), "M 4 5");
  assert.equal(
    roundedOrthogonalPath([{ x: 4, y: 5 }, { x: 4, y: 5 }]),
    "M 4 5",
    "an all-duplicate route collapses to its single point",
  );
});

test("rounding a routed path string preserves every waypoint endpoint", () => {
  const routed = "M 100 100 L 156 100 L 156 650 L 400 650";
  const rounded = roundedStructurePath(routed);
  assert.ok(rounded.startsWith("M 100 100"), "the origin never moves");
  assert.ok(rounded.endsWith("L 400 650"), "the arrow anchor never moves");
  assert.equal([...rounded.matchAll(/Q/g)].length, 2, "each interior turn arcs once");
  assert.equal(
    rounded,
    "M 100 100 L 148 100 Q 156 100 156 108 L 156 642 Q 156 650 164 650 L 400 650",
  );
});

test("waypoint parsing round-trips decimals and negatives", () => {
  assert.deepEqual(
    structurePathWaypoints("M -105 12.5 L -105 -3 L 42.25 -3"),
    [{ x: -105, y: 12.5 }, { x: -105, y: -3 }, { x: 42.25, y: -3 }],
  );
});

test("draw-in delays stagger 40ms per deterministic slot-order index", () => {
  assert.deepEqual([0, 1, 2].map(structureDrawDelayMs), [0, 40, 80]);
  assert.equal(structureDrawDelayMs(-1), 0, "defensive: no negative delays");
});

test("the draw-in replays only when the selected preview changes", () => {
  assert.equal(shouldReplayStructureDraw(null, "page/feed/base"), true);
  assert.equal(shouldReplayStructureDraw("page/feed/base", "page/feed/base"), false);
  assert.equal(shouldReplayStructureDraw("page/feed/base", "page/profile/default"), true);
});

test("kind glyphs distinguish navigation direction and presentation", () => {
  assert.equal(structureConnectorGlyph("navigate", "outgoing"), "→");
  assert.equal(structureConnectorGlyph("navigate", "incoming"), "←");
  assert.equal(structureConnectorGlyph("present", "outgoing"), "⤓");
  assert.equal(structureConnectorGlyph("present", "incoming"), "⤓");
});

const connector = {
  kind: "navigate" as const,
  event: "profile-opened",
  extraCount: 0,
  sourceNode: "page:feed",
  targetNode: "page:profile",
};

test("outgoing labels lead with the glyph and bold the far target", () => {
  assert.deepEqual(structureConnectorLabelSegments(connector, "outgoing"), {
    glyph: "→",
    lead: "→ profile-opened · ",
    farName: "profile",
    suffix: "",
    text: "→ profile-opened · profile",
  });
});

test("incoming labels name the source and keep the dedup suffix at the end", () => {
  const segments = structureConnectorLabelSegments(
    { ...connector, extraCount: 2 },
    "incoming",
  );
  assert.deepEqual(segments, {
    glyph: "←",
    lead: "← profile-opened · ",
    farName: "feed",
    suffix: " +2",
    text: "← profile-opened · feed +2",
  });
});

test("present labels prefix the drop glyph for surfaces", () => {
  const segments = structureConnectorLabelSegments({
    kind: "present",
    event: "comments-requested",
    extraCount: 0,
    sourceNode: "page:feed",
    targetNode: "surface:comments-sheet",
  });
  assert.equal(segments.text, "⤓ comments-requested · comments-sheet");
  assert.equal(segments.farName, "comments-sheet");
});
