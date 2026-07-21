import assert from "node:assert/strict";

import { test } from "vitest";

import type { InteractionGraph, InteractionGraphEdge } from "../../protocol/types.js";
import type { EditorPreview, PreviewKind } from "../editor-state.js";
import {
  buildStructureConnectors,
  incomingLeftLabelShift,
  layoutStructureConnectors,
  logicalRoutePreviewNode,
  routeStructureConnector,
  structureConnectorDescription,
  structureConnectorLabel,
  visibleStructureConnectors,
  type StructureConnectorPlacement,
  type StructureRect,
} from "../structure-connectors.js";
import { projectionContent } from "./fixtures/projection.js";

const preview = (
  kind: PreviewKind,
  subject: string,
  example: string,
): EditorPreview => ({
  id: `${kind}/${subject}/${example}`,
  identity: { kind, subject, example },
  sourceFile: `pages/${subject}.uhura`,
  default: true,
  pinned: false,
  derived: false,
  inFlight: 0,
  from: null,
  replaySteps: [],
  replay: [],
  note: null,
  data: [],
  interactions: [],
  documentation: { declarationDocId: null, exampleDocId: null },
  provenance: { occurrences: [] },
  evidence: null,
  content: projectionContent(),
});

const graph = (edges: InteractionGraphEdge[]): InteractionGraph => ({
  protocol: "uhura-interaction-graph/0",
  nodes: [
    { id: "page:feed", kind: "page", label: "feed" },
    { id: "page:profile", kind: "page", label: "profile" },
    { id: "page:settings", kind: "page", label: "settings" },
    { id: "surface:comments-sheet", kind: "surface", label: "comments-sheet" },
    { id: "command:posts.like", kind: "command", label: "posts.like" },
    { id: "dynamic:opener", kind: "dynamic", label: "surface opener" },
  ],
  edges,
});

const boardPreviews = [
  preview("page", "feed", "base"),
  preview("page", "feed", "extra"),
  preview("page", "profile", "default"),
  preview("surface", "comments-sheet", "open"),
  preview("page", "settings", "default"),
];

test("builds only navigate and present candidates between mapped frames", () => {
  const connectors = buildStructureConnectors(graph([
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "profile-opened" },
    { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
    { kind: "state-change", from: "page:feed", to: "page:feed", event: "liked" },
    { kind: "send-command", from: "page:feed", to: "command:posts.like", event: "liked" },
    { kind: "receive-outcome", from: "command:posts.like", to: "page:feed", event: "like.ok" },
    { kind: "dismiss", from: "surface:comments-sheet", to: "dynamic:opener", event: "dismiss-requested" },
    { kind: "navigate-back", from: "page:profile", to: "dynamic:previous-page", event: "back-requested" },
  ]), boardPreviews);

  assert.deepEqual(connectors, [{
    kind: "navigate",
    sourceNode: "page:feed",
    targetNode: "page:profile",
    sourceId: "page/feed/base",
    targetId: "page/profile/default",
    event: "profile-opened",
    extraCount: 0,
  }, {
    kind: "present",
    sourceNode: "page:feed",
    targetNode: "surface:comments-sheet",
    sourceId: "page/feed/base",
    targetId: "surface/comments-sheet/open",
    event: "comments-requested",
    extraCount: 0,
  }]);
});

test("maps a node to the subject's first frame and skips unmapped or self edges", () => {
  const connectors = buildStructureConnectors(graph([
    { kind: "navigate", from: "page:profile", to: "page:feed", event: "home-requested" },
    { kind: "navigate", from: "page:feed", to: "page:feed", event: "self-refresh" },
    { kind: "navigate", from: "page:feed", to: "page:missing", event: "gone" },
    { kind: "present", from: "page:missing", to: "surface:comments-sheet", event: "gone" },
  ]), boardPreviews);

  assert.deepEqual(
    connectors.map(({ sourceId, targetId }) => [sourceId, targetId]),
    [["page/profile/default", "page/feed/base"]],
    "the first feed example is the frame; missing and self edges draw nothing",
  );
});

test("dedupes edges sharing endpoints and kind into one labeled connector", () => {
  const connectors = buildStructureConnectors(graph([
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "b-event" },
    { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "open-comments" },
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "a-event" },
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "c-event" },
  ]), boardPreviews);

  assert.equal(connectors.length, 2);
  assert.equal(connectors[0]?.event, "a-event");
  assert.equal(connectors[0]?.extraCount, 2);
  assert.equal(
    structureConnectorLabel(connectors[0]!),
    "a-event → profile +2",
    "the dedup suffix stays at the end, after the far endpoint",
  );
  assert.equal(structureConnectorLabel(connectors[1]!), "open-comments → comments-sheet");
  assert.equal(
    structureConnectorDescription(connectors[0]!),
    "navigates on a-event (+2 more)",
  );
  assert.equal(
    structureConnectorDescription(connectors[1]!),
    "presents on open-comments",
  );
});

test("labels name the far endpoint per direction: → target, ← source", () => {
  const connector = {
    event: "author-tapped",
    extraCount: 0,
    sourceNode: "page:feed",
    targetNode: "page:profile",
  };

  assert.equal(structureConnectorLabel(connector, "outgoing"), "author-tapped → profile");
  assert.equal(structureConnectorLabel(connector, "incoming"), "author-tapped ← feed");
  assert.equal(
    structureConnectorLabel({ ...connector, extraCount: 2 }, "incoming"),
    "author-tapped ← feed +2",
    "the +N dedup suffix follows the far endpoint",
  );
  assert.equal(
    structureConnectorLabel({
      ...connector,
      targetNode: "surface:comments-sheet",
    }, "outgoing"),
    "author-tapped → comments-sheet",
    "surface: prefixes strip like page: prefixes",
  );
});

test("orders candidates deterministically regardless of edge input order", () => {
  const edges: InteractionGraphEdge[] = [
    { kind: "navigate", from: "page:feed", to: "page:profile", event: "profile-opened" },
    { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
    { kind: "navigate", from: "page:profile", to: "page:feed", event: "home-requested" },
  ];

  assert.deepEqual(
    buildStructureConnectors(graph(edges), boardPreviews),
    buildStructureConnectors(graph([...edges].reverse()), boardPreviews),
  );
});

const fullGraph = graph([
  { kind: "navigate", from: "page:feed", to: "page:profile", event: "profile-opened" },
  { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
  { kind: "navigate", from: "page:profile", to: "page:feed", event: "home-requested" },
  { kind: "present", from: "page:profile", to: "surface:comments-sheet", event: "profile-comments" },
]);

test("scopes visibility to the selected definition, incoming and outgoing", () => {
  const all = buildStructureConnectors(fullGraph, boardPreviews);

  const feed = visibleStructureConnectors(all, { kind: "page", subject: "feed" });
  assert.deepEqual(
    feed.map(({ sourceNode, targetNode }) => [sourceNode, targetNode]),
    [
      ["page:feed", "page:profile"],
      ["page:feed", "surface:comments-sheet"],
      ["page:profile", "page:feed"],
    ],
    "feed keeps its outgoing arrows and the incoming one from profile",
  );

  const sheet = visibleStructureConnectors(all, {
    kind: "surface",
    subject: "comments-sheet",
  });
  assert.deepEqual(
    sheet.map(({ sourceNode, targetNode }) => [sourceNode, targetNode]),
    [
      ["page:feed", "surface:comments-sheet"],
      ["page:profile", "surface:comments-sheet"],
    ],
  );
});

test("selection scoping matches kind and subject, never subject alone", () => {
  const all = buildStructureConnectors(fullGraph, boardPreviews);
  assert.deepEqual(
    visibleStructureConnectors(all, { kind: "surface", subject: "feed" }),
    [],
    "a surface named like a page must not adopt the page's arrows",
  );
  assert.deepEqual(
    visibleStructureConnectors(all, { kind: "component", subject: "feed" }),
    [],
  );
});

test("preview-backed route nodes map and select the exact application state", () => {
  const feedBase = "page/feed/base";
  const feedExtra = "page/feed/extra";
  const profile = "page/profile/default";
  const connectors = buildStructureConnectors(graph([
    {
      kind: "navigate",
      from: logicalRoutePreviewNode(feedBase),
      to: logicalRoutePreviewNode(profile),
      event: "base-profile",
    },
    {
      kind: "navigate",
      from: logicalRoutePreviewNode(feedExtra),
      to: logicalRoutePreviewNode(profile),
      event: "extra-profile",
    },
  ]), boardPreviews);

  assert.deepEqual(
    connectors.map(({ sourceId, targetId, event }) => [sourceId, targetId, event]),
    [
      [feedBase, profile, "base-profile"],
      [feedExtra, profile, "extra-profile"],
    ],
  );

  const visible = visibleStructureConnectors(connectors, {
    kind: "page",
    subject: "feed",
    previewId: feedExtra,
  });
  assert.deepEqual(
    visible.map(({ sourceId, event }) => [sourceId, event]),
    [[feedExtra, "extra-profile"]],
    "selecting one logical state must not adopt a sibling preview's route",
  );

  const laid = layoutStructureConnectors(visible, {
    node: logicalRoutePreviewNode(feedExtra),
    aliases: ["page:feed"],
    previewId: feedExtra,
  });
  assert.equal(laid[0]?.placement.direction, "outgoing");
  assert.equal(laid[0]?.placement.selectedId, feedExtra);
  assert.equal(laid[0]?.placement.farId, profile);
});

test("empty or unrelated selection hides every structural connector", () => {
  const all = buildStructureConnectors(fullGraph, boardPreviews);
  assert.deepEqual(visibleStructureConnectors(all, null), []);
  assert.deepEqual(
    visibleStructureConnectors(all, { kind: "page", subject: "settings" }),
    [],
  );
});

const feedSelection = { node: "page:feed", previewId: "page/feed/extra" };

const fanGraph = graph([
  { kind: "navigate", from: "page:feed", to: "page:profile", event: "profile-opened" },
  { kind: "navigate", from: "page:feed", to: "page:settings", event: "settings-opened" },
  { kind: "navigate", from: "page:profile", to: "page:feed", event: "home-requested" },
  { kind: "navigate", from: "page:settings", to: "page:feed", event: "home-from-settings" },
  { kind: "present", from: "page:feed", to: "surface:comments-sheet", event: "comments-requested" },
]);

test("anchors every placement at the clicked frame, not the definition's first frame", () => {
  const visible = visibleStructureConnectors(
    buildStructureConnectors(fanGraph, boardPreviews),
    { kind: "page", subject: "feed" },
  );
  const laid = layoutStructureConnectors(visible, feedSelection);

  assert.ok(laid.length > 0);
  assert.ok(
    laid.every((connector) => connector.placement.selectedId === "page/feed/extra"),
    "the near end anchors at the clicked feed frame, not page/feed/base",
  );
  assert.deepEqual(
    laid.map((connector) => connector.placement.farId).sort(),
    [
      "page/profile/default",
      "page/profile/default",
      "page/settings/default",
      "page/settings/default",
      "surface/comments-sheet/open",
    ],
    "far ends still anchor at the far definition's first frame",
  );
});

test("classifies direction and fans slots per selected edge", () => {
  const visible = visibleStructureConnectors(
    buildStructureConnectors(fanGraph, boardPreviews),
    { kind: "page", subject: "feed" },
  );
  const laid = layoutStructureConnectors(visible, feedSelection);

  assert.deepEqual(
    laid.map(({ event, placement }) => [
      event,
      placement.direction,
      placement.side,
      placement.slot,
      placement.slotCount,
    ]),
    [
      ["home-requested", "incoming", "left", 0, 2],
      ["home-from-settings", "incoming", "left", 1, 2],
      ["profile-opened", "outgoing", "right", 0, 2],
      ["settings-opened", "outgoing", "right", 1, 2],
      ["comments-requested", "outgoing", "bottom", 0, 1],
    ],
    "outgoing navigate fans on the right, incoming on the left, present below",
  );
});

test("a selected surface receives incoming presents at its top edge", () => {
  const visible = visibleStructureConnectors(
    buildStructureConnectors(fullGraph, boardPreviews),
    { kind: "surface", subject: "comments-sheet" },
  );
  const laid = layoutStructureConnectors(visible, {
    node: "surface:comments-sheet",
    previewId: "surface/comments-sheet/open",
  });

  assert.deepEqual(
    laid.map(({ placement }) => [
      placement.direction,
      placement.side,
      placement.slot,
      placement.slotCount,
      placement.farId,
    ]),
    [
      ["incoming", "top", 0, 2, "page/feed/base"],
      ["incoming", "top", 1, 2, "page/profile/default"],
    ],
  );
});

test("fan layout is deterministic and never mutates its inputs", () => {
  const visible = visibleStructureConnectors(
    buildStructureConnectors(fanGraph, boardPreviews),
    { kind: "page", subject: "feed" },
  );
  const laid = layoutStructureConnectors(visible, feedSelection);

  assert.deepEqual(
    layoutStructureConnectors([...visible].reverse(), feedSelection),
    laid,
    "slot assignment ignores input order",
  );
  assert.ok(laid.every((connector) => visible.every((input) => input !== connector)));
  assert.ok(visible.every((connector) => !("placement" in connector)));
});

const placement = (
  overrides: Partial<StructureConnectorPlacement>,
): StructureConnectorPlacement => ({
  direction: "outgoing",
  side: "right",
  slot: 0,
  slotCount: 1,
  selectedId: "selected",
  farId: "far",
  ...overrides,
});

const selectedRect: StructureRect = { x: 0, y: 0, width: 100, height: 200 };

test("outgoing navigate exits the right edge into a rightward target's left edge", () => {
  const far: StructureRect = { x: 300, y: 40, width: 100, height: 100 };
  const route = routeStructureConnector(placement({}), selectedRect, far);

  assert.equal(route.path, "M 100 100 L 200 100 L 200 90 L 300 90");
  assert.deepEqual(route.origin, { x: 100, y: 100 });
  assert.ok(route.arrow.includes("L 300 90"), "arrowhead tip sits at the target's left edge");
  assert.deepEqual(route.label, { x: 108, y: 100, anchor: "start" });
});

test("outgoing navigate to a non-rightward target enters its top edge", () => {
  const far: StructureRect = { x: -400, y: 400, width: 100, height: 100 };
  const route = routeStructureConnector(placement({}), selectedRect, far);

  assert.equal(route.path, "M 100 100 L 128 100 L 128 372 L -350 372 L -350 400");
  assert.ok(route.arrow.includes("L -350 400"), "arrowhead tip sits at the target's top edge");
  assert.deepEqual(route.label, { x: 108, y: 100, anchor: "start" });
});

test("right-edge exits fan evenly down the edge from the top", () => {
  const far: StructureRect = { x: 300, y: 0, width: 100, height: 100 };
  const first = routeStructureConnector(
    placement({ slot: 0, slotCount: 3 }),
    selectedRect,
    far,
  );
  const second = routeStructureConnector(
    placement({ slot: 1, slotCount: 3 }),
    selectedRect,
    far,
  );
  const third = routeStructureConnector(
    placement({ slot: 2, slotCount: 3 }),
    selectedRect,
    far,
  );

  assert.deepEqual(
    [first.origin, second.origin, third.origin],
    [{ x: 100, y: 50 }, { x: 100, y: 100 }, { x: 100, y: 150 }],
  );
  assert.deepEqual(
    [first.label.y, second.label.y, third.label.y],
    [50, 100, 150],
    "labels stack along the fan so up to a dozen never collide",
  );
});

test("incoming navigate arrives at the selected left edge, label anchored end", () => {
  const far: StructureRect = { x: -300, y: 0, width: 100, height: 100 };
  const route = routeStructureConnector(
    placement({ direction: "incoming", side: "left" }),
    selectedRect,
    far,
  );

  assert.equal(route.path, "M -200 50 L -100 50 L -100 100 L 0 100");
  assert.deepEqual(route.origin, { x: -200, y: 50 }, "the dot departs the source frame");
  assert.ok(route.arrow.includes("L 0 100"), "arrowhead tip sits on the selected left edge");
  assert.deepEqual(route.label, { x: -8, y: 100, anchor: "end" });
});

test("outgoing present drops from the bottom edge to the surface's top edge", () => {
  const far: StructureRect = { x: 0, y: 400, width: 100, height: 100 };
  const route = routeStructureConnector(
    placement({ side: "bottom" }),
    selectedRect,
    far,
  );

  assert.equal(route.path, "M 50 200 L 50 300 L 50 400");
  assert.ok(route.arrow.includes("L 50 400"), "arrowhead tip sits at the surface's top edge");
  assert.deepEqual(route.label, { x: 56, y: 210, anchor: "start" });
});

test("bottom-edge labels stack downward per slot", () => {
  const far: StructureRect = { x: 0, y: 400, width: 100, height: 100 };
  const first = routeStructureConnector(
    placement({ side: "bottom", slot: 0, slotCount: 2 }),
    selectedRect,
    far,
  );
  const second = routeStructureConnector(
    placement({ side: "bottom", slot: 1, slotCount: 2 }),
    selectedRect,
    far,
  );

  assert.ok(second.label.y > first.label.y);
  assert.ok(second.label.x > first.label.x, "each slot exits further along the bottom edge");
});

test("incoming present arrives at the selected surface's top edge", () => {
  const surfaceRect: StructureRect = { x: 0, y: 0, width: 100, height: 100 };
  const far: StructureRect = { x: 0, y: -300, width: 100, height: 100 };
  const route = routeStructureConnector(
    placement({ direction: "incoming", side: "top" }),
    surfaceRect,
    far,
  );

  assert.equal(route.path, "M 50 -200 L 50 -100 L 50 0");
  assert.ok(route.arrow.includes("L 50 0"), "arrowhead tip sits on the selected top edge");
  assert.deepEqual(route.label, { x: 56, y: -10, anchor: "start" });
});

/** The (x, y) waypoints of an orthogonal `M … L …` route path. */
const pathPoints = (path: string): Array<[number, number]> =>
  [...path.matchAll(/[ML] (-?\d+(?:\.\d+)?) (-?\d+(?:\.\d+)?)/g)]
    .map((match) => [Number(match[1]), Number(match[2])]);

test("parallel rightward routes stagger their vertical corridor x per slot", () => {
  const far: StructureRect = { x: 300, y: 0, width: 100, height: 100 };
  const corridorXs = [0, 1, 2].map((slot) => {
    const route = routeStructureConnector(
      placement({ slot, slotCount: 3 }),
      selectedRect,
      far,
    );
    return pathPoints(route.path)[1]![0];
  });

  assert.deepEqual(
    corridorXs,
    [190, 200, 210],
    "corridors spread symmetrically around the gap midpoint",
  );
  assert.equal(new Set(corridorXs).size, 3, "no two slots share a vertical x");
});

test("staggered corridors clamp inside a narrow inter-frame gap", () => {
  const far: StructureRect = { x: 112, y: 0, width: 100, height: 100 };
  const corridorXs = [0, 1, 2].map((slot) => {
    const route = routeStructureConnector(
      placement({ slot, slotCount: 3 }),
      selectedRect,
      far,
    );
    return pathPoints(route.path)[1]![0];
  });

  assert.ok(
    corridorXs.every((x) => x > 100 && x < 112),
    `every corridor stays inside the gap, got ${corridorXs.join(", ")}`,
  );
  assert.equal(new Set(corridorXs).size, 3, "clamping keeps the slots distinct");
});

test("a degenerate gap falls back to the shared midpoint corridor", () => {
  const far: StructureRect = { x: 104, y: 0, width: 100, height: 100 };
  const corridorXs = [0, 1].map((slot) => {
    const route = routeStructureConnector(
      placement({ slot, slotCount: 2 }),
      selectedRect,
      far,
    );
    return pathPoints(route.path)[1]![0];
  });

  assert.deepEqual(corridorXs, [102, 102], "too thin to stagger: both use the midpoint");
});

test("incoming left-edge routes stagger their corridor x per slot", () => {
  const far: StructureRect = { x: -300, y: 0, width: 100, height: 100 };
  const corridorXs = [0, 1].map((slot) => {
    const route = routeStructureConnector(
      placement({ direction: "incoming", side: "left", slot, slotCount: 2 }),
      selectedRect,
      far,
    );
    return pathPoints(route.path)[1]![0];
  });

  assert.deepEqual(corridorXs, [-105, -95]);
  assert.equal(new Set(corridorXs).size, 2, "no two incoming slots share a vertical x");
});

test("bottom and top midpoint corridors stagger their y per slot", () => {
  const below: StructureRect = { x: 0, y: 400, width: 100, height: 100 };
  const bottomYs = [0, 1].map((slot) => {
    const route = routeStructureConnector(
      placement({ side: "bottom", slot, slotCount: 2 }),
      selectedRect,
      below,
    );
    return pathPoints(route.path)[1]![1];
  });
  assert.deepEqual(bottomYs, [295, 305]);

  const above: StructureRect = { x: 0, y: -300, width: 100, height: 100 };
  const topYs = [0, 1].map((slot) => {
    const route = routeStructureConnector(
      placement({ direction: "incoming", side: "top", slot, slotCount: 2 }),
      { x: 0, y: 0, width: 100, height: 100 },
      above,
    );
    return pathPoints(route.path)[1]![1];
  });
  assert.deepEqual(topYs, [-105, -95]);
});

test("right-edge stub fans clamp inside the gap to the right-row neighbor", () => {
  const far: StructureRect = { x: -400, y: 400, width: 100, height: 100 };
  const rightNeighbor: StructureRect = { x: 140, y: 0, width: 100, height: 200 };
  const stubXs = [0, 1, 2, 3, 4].map((slot) => {
    const route = routeStructureConnector(
      placement({ slot, slotCount: 5 }),
      selectedRect,
      far,
      1,
      [rightNeighbor],
    );
    return pathPoints(route.path)[1]![0];
  });

  assert.deepEqual(stubXs, [104, 112, 120, 128, 136], "fan compresses into the 40px gap");
  assert.ok(
    stubXs.every((x) => x! > 100 && x! < 140),
    `no stub drop enters the neighbor frame, got ${stubXs.join(", ")}`,
  );
  assert.equal(new Set(stubXs).size, 5, "clamping keeps the slots distinct");
});

test("a wide neighbor gap keeps the ideal stub fan", () => {
  const far: StructureRect = { x: -400, y: 400, width: 100, height: 100 };
  const rightNeighbor: StructureRect = { x: 400, y: 0, width: 100, height: 200 };
  const stubXs = [0, 1].map((slot) => pathPoints(routeStructureConnector(
    placement({ slot, slotCount: 2 }),
    selectedRect,
    far,
    1,
    [rightNeighbor],
  ).path)[1]![0]);

  assert.deepEqual(stubXs, [128, 142], "open space keeps the 28 + slot * 14 fan");
});

test("a stub gap too thin to fan hugs the selected frame's edge", () => {
  const far: StructureRect = { x: -400, y: 400, width: 100, height: 100 };
  const rightNeighbor: StructureRect = { x: 106, y: 0, width: 100, height: 200 };
  const stubXs = [0, 1].map((slot) => pathPoints(routeStructureConnector(
    placement({ slot, slotCount: 2 }),
    selectedRect,
    far,
    1,
    [rightNeighbor],
  ).path)[1]![0]);

  assert.deepEqual(stubXs, [103, 103], "no room to stagger: both hug the edge");
});

test("stub fans ignore frames outside the vertical's span", () => {
  const far: StructureRect = { x: -400, y: 400, width: 100, height: 100 };
  // Below the drop's whole span (fan y 100 → approach y 372): no obstacle.
  const offSpan: StructureRect = { x: 110, y: 380, width: 100, height: 100 };
  const route = routeStructureConnector(placement({}), selectedRect, far, 1, [offSpan]);

  assert.equal(pathPoints(route.path)[1]![0], 128, "the ideal stub distance survives");
});

test("a cross-row stem that would cross a frame shifts to a free gap", () => {
  const far: StructureRect = { x: -400, y: 400, width: 100, height: 100 };
  // In a row between the endpoints, straddling the ideal stub x of 128.
  const midRow: StructureRect = { x: 110, y: 300, width: 100, height: 100 };
  const route = routeStructureConnector(placement({}), selectedRect, far, 1, [midRow]);

  const points = pathPoints(route.path);
  assert.equal(points[1]![0], 106, "the stem snaps to the frame's left-4 boundary");
  assert.deepEqual(points[2], [106, 372], "the whole drop runs on the shifted x");
  assert.ok(points[1]![0] < midRow.x, "the vertical clears the frame body");
});

test("a blocked rightward corridor shifts to the nearest free column gap", () => {
  const far: StructureRect = { x: 400, y: 600, width: 100, height: 100 };
  const blocker: StructureRect = { x: 160, y: 250, width: 200, height: 200 };
  const route = routeStructureConnector(placement({}), selectedRect, far, 1, [blocker]);

  assert.equal(
    route.path,
    "M 100 100 L 156 100 L 156 650 L 400 650",
    "the corridor leaves the blocked midpoint 250 for the frame's left-4 boundary",
  );
});

test("parallel shifted corridors spread per slot inside the free gap", () => {
  const far: StructureRect = { x: 400, y: 600, width: 100, height: 100 };
  const blocker: StructureRect = { x: 160, y: 250, width: 200, height: 200 };
  const corridorXs = [0, 1].map((slot) => pathPoints(routeStructureConnector(
    placement({ slot, slotCount: 2 }),
    selectedRect,
    far,
    1,
    [blocker],
  ).path)[1]![0]);

  assert.deepEqual(corridorXs, [156, 146], "slots fan away from the frame edge");
  assert.equal(new Set(corridorXs).size, 2, "no two shifted slots share one vertical");
});

test("blocked vertical routing is deterministic and never crosses the frame", () => {
  const far: StructureRect = { x: 400, y: 600, width: 100, height: 100 };
  const blocker: StructureRect = { x: 160, y: 250, width: 200, height: 200 };
  const first = routeStructureConnector(placement({}), selectedRect, far, 1, [blocker]);
  const second = routeStructureConnector(placement({}), selectedRect, far, 1, [blocker]);

  assert.deepEqual(first, second, "identical inputs route identically");
  const verticalX = pathPoints(first.path)[1]![0];
  assert.ok(
    verticalX <= blocker.x || verticalX >= blocker.x + blocker.width,
    "the corridor stays outside the frame's horizontal extent",
  );
});

test("the selected and far frames never count as routing obstacles", () => {
  // The far frame overlaps the corridor's span; excluding it keeps the
  // plain staggered midpoint corridor between the two columns.
  const far: StructureRect = { x: 300, y: 40, width: 100, height: 100 };
  const withEndpoints = routeStructureConnector(
    placement({}),
    selectedRect,
    far,
    1,
    [selectedRect, far],
  );
  const without = routeStructureConnector(placement({}), selectedRect, far, 1, []);

  assert.deepEqual(withEndpoints, without);
});

test("a blocked present drop jogs sideways around the frame below", () => {
  const far: StructureRect = { x: 0, y: 600, width: 100, height: 100 };
  const blocker: StructureRect = { x: 0, y: 300, width: 100, height: 100 };
  const route = routeStructureConnector(
    placement({ side: "bottom" }),
    selectedRect,
    far,
    1,
    [blocker],
  );

  assert.equal(
    route.path,
    "M 50 200 L 50 228 L -4 228 L -4 572 L 50 572 L 50 600",
    "stub below the selected frame, free corridor at the blocker's left-4, "
      + "approach above the far frame",
  );
  assert.ok(route.arrow.includes("L 50 600"), "arrowhead still enters the far top edge");
});

test("an unobstructed present drop keeps the straight three-point path", () => {
  const far: StructureRect = { x: 300, y: 600, width: 100, height: 100 };
  // Off to the side of both verticals: no jog.
  const bystander: StructureRect = { x: 120, y: 300, width: 100, height: 100 };
  const route = routeStructureConnector(
    placement({ side: "bottom" }),
    selectedRect,
    far,
    1,
    [bystander],
  );

  assert.equal(route.path, "M 50 200 L 50 400 L 350 400 L 350 600");
});

test("a blocked incoming present climb jogs around the frame above", () => {
  const surfaceRect: StructureRect = { x: 0, y: 600, width: 100, height: 200 };
  const far: StructureRect = { x: 0, y: 0, width: 100, height: 100 };
  const blocker: StructureRect = { x: 0, y: 300, width: 100, height: 100 };
  const route = routeStructureConnector(
    placement({ direction: "incoming", side: "top" }),
    surfaceRect,
    far,
    1,
    [blocker],
  );

  const points = pathPoints(route.path);
  assert.equal(points[0]![1], 100, "the route departs the presenting frame's bottom edge");
  assert.equal(points[points.length - 1]![1], 600, "the route enters the selected top edge");
  const corridorX = points[2]![0];
  assert.ok(
    corridorX <= blocker.x || corridorX >= blocker.x + blocker.width,
    `the climb corridor clears the blocker, got x ${corridorX}`,
  );
});

test("left, bottom, and top stub fallbacks clamp to their nearest neighbor", () => {
  // Left: the far frame's right edge sits past the selected left edge, so the
  // route stubs left instead of using the inter-frame corridor.
  const leftFar: StructureRect = { x: 50, y: 400, width: 100, height: 100 };
  const leftNeighbor: StructureRect = { x: -30, y: 0, width: 20, height: 200 };
  const leftXs = [0, 1].map((slot) => pathPoints(routeStructureConnector(
    placement({ direction: "incoming", side: "left", slot, slotCount: 2 }),
    selectedRect,
    leftFar,
    1,
    [leftNeighbor],
  ).path)[1]![0]);
  assert.deepEqual(leftXs, [-4, -6], "left stubs stay inside the 10px gap");

  // Bottom: the surface sits closer than the 2-stub threshold, so the route
  // stubs down; the neighbor below caps the drop.
  const bottomFar: StructureRect = { x: 300, y: 210, width: 100, height: 100 };
  const bottomNeighbor: StructureRect = { x: 0, y: 230, width: 100, height: 100 };
  const bottomYs = [0, 1].map((slot) => pathPoints(routeStructureConnector(
    placement({ side: "bottom", slot, slotCount: 2 }),
    selectedRect,
    bottomFar,
    1,
    [bottomNeighbor],
  ).path)[1]![1]);
  assert.deepEqual(bottomYs, [212, 226], "bottom stubs stay inside the 30px gap");

  // Top: the presenting frame's bottom edge sits below the selected top edge,
  // so the route stubs up; the neighbor above caps the rise.
  const topFar: StructureRect = { x: 300, y: -50, width: 100, height: 100 };
  const topNeighbor: StructureRect = { x: 0, y: -40, width: 100, height: 20 };
  const topYs = [0, 1].map((slot) => pathPoints(routeStructureConnector(
    placement({ direction: "incoming", side: "top", slot, slotCount: 2 }),
    selectedRect,
    topFar,
    1,
    [topNeighbor],
  ).path)[1]![1]);
  assert.deepEqual(topYs, [-4, -16], "top stubs stay inside the 20px gap");
});

test("an incoming pill recenters inside a gap wide enough to hold it", () => {
  const selected: StructureRect = { x: 200, y: 0, width: 100, height: 200 };
  const leftNeighbor: StructureRect = { x: 0, y: 0, width: 50, height: 200 };
  // Flush placement: pill right edge 8 inside the label gap, width 60.
  const shift = incomingLeftLabelShift(
    { left: 132, right: 192 },
    selected,
    [leftNeighbor],
    1,
  );

  assert.equal(shift, -37, "the pill centers in the 150-wide gap: left 95, right 155");
});

test("an incoming pill keeps flush placement over a too-narrow gap", () => {
  const selected: StructureRect = { x: 200, y: 0, width: 100, height: 200 };
  const leftNeighbor: StructureRect = { x: 0, y: 0, width: 150, height: 200 };
  const shift = incomingLeftLabelShift(
    { left: 132, right: 192 },
    selected,
    [leftNeighbor],
    1,
  );

  assert.equal(shift, 0, "60 + 2x8 clearance exceeds the 50-wide gap: keep and z-lift");
});

test("an incoming pill without a left neighbor never moves", () => {
  const selected: StructureRect = { x: 200, y: 0, width: 100, height: 200 };
  const offRow: StructureRect = { x: 0, y: 400, width: 100, height: 100 };

  assert.equal(incomingLeftLabelShift({ left: 132, right: 192 }, selected, [], 1), 0);
  assert.equal(
    incomingLeftLabelShift({ left: 132, right: 192 }, selected, [offRow], 1),
    0,
    "frames outside the cross-axis span are not gap boundaries",
  );
});

test("marker scale grows label offsets for low-zoom readability", () => {
  const far: StructureRect = { x: 300, y: 40, width: 100, height: 100 };
  const route = routeStructureConnector(placement({}), selectedRect, far, 4);

  assert.equal(route.path, "M 100 100 L 200 100 L 200 90 L 300 90", "geometry stays put");
  assert.deepEqual(route.label, { x: 132, y: 100, anchor: "start" });
});
