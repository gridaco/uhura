import assert from "node:assert/strict";

import { test } from "vitest";

import type { InteractionGraph, InteractionGraphEdge } from "../../protocol/types.js";
import type { EditorPreview, PreviewKind } from "../editor-state.js";
import {
  buildStructureConnectors,
  layoutStructureConnectors,
  routeStructureConnector,
  structureConnectorDescription,
  structureConnectorLabel,
  visibleStructureConnectors,
  type StructureConnectorPlacement,
  type StructureRect,
} from "../structure-connectors.js";

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
  content: kind === "page"
    ? {
      protocol: "uhura-view/0",
      revision: 0,
      page: {
        route: subject,
        root: { key: "root", element: "view", props: {} },
      },
      surfaces: [],
    }
    : { key: "root", element: "view", props: {} },
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
  assert.equal(structureConnectorLabel(connectors[0]!), "a-event +2");
  assert.equal(structureConnectorLabel(connectors[1]!), "open-comments");
  assert.equal(
    structureConnectorDescription(connectors[0]!),
    "navigates on a-event (+2 more)",
  );
  assert.equal(
    structureConnectorDescription(connectors[1]!),
    "presents on open-comments",
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

test("marker scale grows label offsets for low-zoom readability", () => {
  const far: StructureRect = { x: 300, y: 40, width: 100, height: 100 };
  const route = routeStructureConnector(placement({}), selectedRect, far, 4);

  assert.equal(route.path, "M 100 100 L 200 100 L 200 90 L 300 90", "geometry stays put");
  assert.deepEqual(route.label, { x: 132, y: 100, anchor: "start" });
});
