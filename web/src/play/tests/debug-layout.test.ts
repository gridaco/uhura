import assert from "node:assert/strict";
import { test } from "vitest";

import type {
  DebugGraphEdge,
  DebugGraphNode,
  DebugLane,
} from "../debug-model.js";
import {
  cubicDebugPath,
  layoutDebugGraph,
  orthogonalDebugPath,
} from "../debug-layout.js";

function node(
  id: string,
  kind: DebugGraphNode["kind"],
  lane: DebugLane,
  order = 0,
): DebugGraphNode {
  return {
    id,
    kind,
    lane,
    definitionId: id.includes("/") ? "pages.feed" : null,
    label: id,
    detail: null,
    order,
    span: null,
    runtime: {
      active: false,
      current: false,
      selected: false,
      consulted: null,
      written: false,
      sent: false,
      pending: 0,
      projectionApply: null,
      projectionReady: 0,
      projectionFailures: 0,
      structural: false,
    },
  };
}

function edge(
  id: string,
  kind: DebugGraphEdge["kind"],
  from: string,
  to: string,
): DebugGraphEdge {
  return {
    id,
    kind,
    from,
    to,
    label: kind,
    order: null,
    mode: null,
    activity: "idle",
  };
}

function topology(): {
  nodes: readonly DebugGraphNode[];
  edges: readonly DebugGraphEdge[];
} {
  return {
    // Deliberately scrambled: layout owns deterministic semantic ordering.
    nodes: [
      node("commands.save", "command", "effect"),
      node("pages.feed/handler/1", "handler", "handler", 1),
      node("projections.feed", "projection", "input"),
      node("pages.feed/state/output", "state", "effect"),
      node("pages.feed/event/submit", "event", "input"),
      node("pages.detail", "definition", "effect"),
      node("pages.feed/state/input", "state", "input"),
      node("pages.feed/handler/0", "handler", "handler", 0),
    ],
    edges: [
      edge(
        "handles",
        "handles",
        "pages.feed/event/submit",
        "pages.feed/handler/0",
      ),
      edge(
        "guard",
        "guard-reads",
        "pages.feed/state/input",
        "pages.feed/handler/0",
      ),
      edge(
        "projection",
        "body-reads",
        "projections.feed",
        "pages.feed/handler/1",
      ),
      edge(
        "write",
        "writes",
        "pages.feed/handler/0",
        "pages.feed/state/output",
      ),
      edge(
        "navigate",
        "navigates",
        "pages.feed/handler/0",
        "pages.detail",
      ),
      edge(
        "send",
        "sends",
        "pages.feed/handler/1",
        "commands.save",
      ),
      edge(
        "read-after-write",
        "body-reads",
        "pages.feed/state/output",
        "pages.feed/handler/0",
      ),
      edge(
        "settle",
        "settles",
        "commands.save",
        "pages.feed/event/submit",
      ),
    ],
  };
}

function pathById(
  layout: ReturnType<typeof layoutDebugGraph>,
  id: string,
) {
  const found = layout.edges.find((candidate) => candidate.edge.id === id);
  assert.ok(found, `missing ${id}`);
  return found;
}

test("lays out semantic lanes at exact deterministic coordinates", () => {
  const layout = layoutDebugGraph(topology());

  assert.equal(layout.width, 880);
  assert.equal(layout.height, 300);
  assert.equal(layout.viewBox, "0 0 880 300");
  assert.deepEqual(layout.lanes, [
    { id: "input", label: "Events & dependencies", x: 32, width: 208 },
    { id: "handler", label: "Handlers", x: 336, width: 208 },
    { id: "effect", label: "Effects", x: 640, width: 208 },
  ]);

  assert.deepEqual(
    layout.nodes.map((item) => ({
      id: item.node.id,
      x: item.x,
      y: item.y,
      width: item.width,
      height: item.height,
    })),
    [
      { id: "pages.feed/event/submit", x: 32, y: 32, width: 208, height: 52 },
      { id: "pages.feed/state/input", x: 32, y: 104, width: 208, height: 52 },
      { id: "projections.feed", x: 32, y: 176, width: 208, height: 52 },
      { id: "pages.feed/handler/0", x: 336, y: 68, width: 208, height: 52 },
      { id: "pages.feed/handler/1", x: 336, y: 140, width: 208, height: 52 },
      { id: "pages.feed/state/output", x: 640, y: 32, width: 208, height: 52 },
      { id: "pages.detail", x: 640, y: 104, width: 208, height: 52 },
      { id: "commands.save", x: 640, y: 176, width: 208, height: 52 },
    ],
  );
});

test("emits exact cubic forward paths and tracked orthogonal cycles", () => {
  const layout = layoutDebugGraph(topology());

  assert.deepEqual(pathById(layout, "handles"), {
    edge: topology().edges[0],
    route: "cubic",
    path: "M 240 58 C 288 58 288 94 336 94",
  });
  assert.equal(pathById(layout, "read-after-write").route, "orthogonal");
  assert.equal(
    pathById(layout, "read-after-write").path,
    "M 744 84 L 744 252 L 440 252 L 440 120",
  );
  assert.equal(pathById(layout, "settle").route, "orthogonal");
  assert.equal(
    pathById(layout, "settle").path,
    "M 744 228 L 744 268 L 136 268 L 136 84",
  );

  assert.equal(
    cubicDebugPath(
      { x: 0, y: 10, width: 100, height: 40 },
      { x: 200, y: 30, width: 100, height: 40 },
    ),
    "M 100 30 C 150 30 150 50 200 50",
  );
  assert.equal(
    orthogonalDebugPath(
      { x: 200, y: 10, width: 100, height: 40 },
      { x: 0, y: 30, width: 100, height: 40 },
      90,
    ),
    "M 250 50 L 250 90 L 50 90 L 50 70",
  );
  assert.throws(
    () => orthogonalDebugPath(
      { x: 0, y: 0, width: 10, height: 10 },
      { x: 20, y: 0, width: 10, height: 10 },
      10,
    ),
    /track must sit below/,
  );
});

test("runtime-only changes cannot alter node positions or edge paths", () => {
  const graph = topology();
  const first = layoutDebugGraph(graph);
  const changedNodes = graph.nodes.map((item) => ({
    ...item,
    detail: `changed ${item.id}`,
    runtime: {
      ...item.runtime,
      active: true,
      current: true,
      pending: 9,
    },
  }));
  const changedEdges = graph.edges.map((item) => ({
    ...item,
    activity: "taken" as const,
  }));
  const second = layoutDebugGraph({ nodes: changedNodes, edges: changedEdges });

  assert.deepEqual(
    second.nodes.map((item) => ({
      id: item.node.id,
      x: item.x,
      y: item.y,
      width: item.width,
      height: item.height,
    })),
    first.nodes.map((item) => ({
      id: item.node.id,
      x: item.x,
      y: item.y,
      width: item.width,
      height: item.height,
    })),
  );
  assert.deepEqual(
    second.edges.map((item) => ({
      id: item.edge.id,
      route: item.route,
      path: item.path,
    })),
    first.edges.map((item) => ({
      id: item.edge.id,
      route: item.route,
      path: item.path,
    })),
  );
});

test("validates metrics and returns a useful empty canvas", () => {
  assert.throws(
    () => layoutDebugGraph({ nodes: [], edges: [] }, { nodeWidth: 0 }),
    /nodeWidth must be a positive finite number/,
  );
  assert.throws(
    () => layoutDebugGraph({ nodes: [], edges: [] }, { rowGap: Number.NaN }),
    /rowGap must be a positive finite number/,
  );
  const empty = layoutDebugGraph({ nodes: [], edges: [] });
  assert.equal(empty.width, 880);
  assert.equal(empty.height, 116);
  assert.equal(empty.viewBox, "0 0 880 116");
  assert.deepEqual(empty.nodes, []);
  assert.deepEqual(empty.edges, []);
});
