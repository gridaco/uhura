import assert from "node:assert/strict";
import { test } from "vitest";

import {
  clipAnnotationRect,
  placeAnnotations,
  unionAnnotationRects,
} from "../annotation-layout.js";

test("clips anchors through every containing rectangle and unions block anchors", () => {
  const clipped = clipAnnotationRect(
    { left: 0, top: 0, width: 100, height: 100 },
    [
      { left: 10, top: 5, width: 70, height: 80 },
      { left: 20, top: 15, width: 80, height: 30 },
    ],
  );
  assert.deepEqual(clipped, { left: 20, top: 15, width: 60, height: 30 });
  assert.deepEqual(
    unionAnnotationRects([
      { left: 20, top: 15, width: 10, height: 10 },
      { left: 50, top: 30, width: 20, height: 5 },
    ]),
    { left: 20, top: 15, width: 50, height: 20 },
  );
});

test("placement is deterministic, avoids clean local collisions, and falls back to gutters", () => {
  const inputs = [
    {
      id: "later",
      sourceOrder: 2,
      anchor: { left: 80, top: 20, width: 20, height: 20 },
      card: { width: 80, height: 50 },
    },
    {
      id: "first",
      sourceOrder: 1,
      anchor: { left: 80, top: 20, width: 20, height: 20 },
      card: { width: 80, height: 50 },
    },
  ];
  const viewport = { left: 0, top: 0, width: 300, height: 180 };
  const first = placeAnnotations(inputs, viewport);
  assert.deepEqual(placeAnnotations(inputs, viewport), first);
  assert.deepEqual(first.map((item) => item.id), ["first", "later"]);
  assert.equal(first[0]?.gutter, false);
  assert.notDeepEqual(first[0]?.card, first[1]?.card);
  assert.notDeepEqual(first[0]?.marker, first[1]?.marker, "shared-anchor markers stack");

  const dense = placeAnnotations([{
    id: "dense",
    sourceOrder: 0,
    anchor: { left: 35, top: 35, width: 10, height: 10 },
    card: { width: 90, height: 70 },
  }], { left: 0, top: 0, width: 100, height: 80 });
  assert.equal(dense[0]?.gutter, true);

  const chromeAware = placeAnnotations([{
    id: "chrome-aware",
    sourceOrder: 0,
    anchor: { left: 40, top: 40, width: 20, height: 20 },
    card: { width: 60, height: 40 },
  }], viewport, 12, 10, [{ left: 70, top: 40, width: 80, height: 50 }]);
  assert.notEqual(chromeAware[0]?.card.left, 70, "placement avoids fixed Editor chrome");
});

test("leader length breaks clean-candidate ties before candidate ordinal", () => {
  const placement = placeAnnotations([{
    id: "leader",
    sourceOrder: 0,
    anchor: { left: 100, top: 100, width: 60, height: 10 },
    card: { width: 80, height: 40 },
  }], { left: 0, top: 0, width: 400, height: 300 }, 12, 10, [
    { left: 170, top: 100, width: 80, height: 40 },
  ]);

  assert.equal(placement[0]?.candidate, "above");
});

test("previous-candidate hysteresis keeps a comparable clean placement stable", () => {
  const input = [{
    id: "stable",
    sourceOrder: 0,
    anchor: { left: 100, top: 100, width: 40, height: 30 },
    card: { width: 80, height: 50 },
  }];
  const viewport = { left: 0, top: 0, width: 400, height: 300 };
  const first = placeAnnotations(input, viewport);
  assert.equal(first[0]?.candidate, "right", "ordinal resolves the initial equal-length choice");

  const priorAbove = new Map([
    ["stable", { ...first[0]!, candidate: "above" as const }],
  ]);
  const stable = placeAnnotations(input, viewport, 12, 10, [], priorAbove);
  assert.equal(stable[0]?.candidate, "above");

  const blockedAbove = placeAnnotations(input, viewport, 12, 10, [
    { left: 100, top: 40, width: 80, height: 50 },
  ], new Map(stable.map((placement) => [placement.id, placement])));
  assert.equal(blockedAbove[0]?.candidate, "right", "hard collisions override hysteresis");
});

test("marker-only realizations stack badges without reserving hidden card collisions", () => {
  const active = {
    id: "active",
    sourceOrder: 1,
    anchor: { left: 80, top: 40, width: 20, height: 20 },
    card: { width: 90, height: 50 },
  };
  const viewport = { left: 0, top: 0, width: 320, height: 200 };
  const alone = placeAnnotations([active], viewport)[0];
  const together = placeAnnotations([{
    ...active,
    id: "marker-only",
    sourceOrder: 0,
    showCard: false,
  }, active], viewport);
  const markerOnly = together.find((placement) => placement.id === "marker-only");
  const placedActive = together.find((placement) => placement.id === "active");

  assert.deepEqual(markerOnly?.card, {
    left: markerOnly?.marker.x,
    top: markerOnly?.marker.y,
    width: 0,
    height: 0,
  });
  assert.deepEqual(placedActive?.card, alone?.card);
  assert.notDeepEqual(
    markerOnly?.marker,
    placedActive?.marker,
    "badges sharing an anchor still receive distinct hit targets",
  );
});
