// Deterministic, dependency-free geometry for the Play behavior inspector.
// Node boxes have fixed metrics and are ordered only by static topology, so
// changing values/highlights never causes the graph to jump.

import type {
  DebugGraphEdge,
  DebugGraphModel,
  DebugGraphNode,
  DebugLane,
} from "./debug-model.js";

export interface DebugLayoutOptions {
  readonly padding?: number;
  readonly nodeWidth?: number;
  readonly nodeHeight?: number;
  readonly laneGap?: number;
  readonly rowGap?: number;
  readonly backwardTrackGap?: number;
}

export interface DebugLayoutMetrics {
  readonly padding: number;
  readonly nodeWidth: number;
  readonly nodeHeight: number;
  readonly laneGap: number;
  readonly rowGap: number;
  readonly backwardTrackGap: number;
}

export interface DebugBox {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface DebugLayoutLane {
  readonly id: DebugLane;
  readonly label: string;
  readonly x: number;
  readonly width: number;
}

export interface DebugLayoutNode extends DebugBox {
  readonly node: DebugGraphNode;
}

export type DebugEdgeRoute = "cubic" | "orthogonal";

export interface DebugLayoutEdge {
  readonly edge: DebugGraphEdge;
  readonly route: DebugEdgeRoute;
  readonly path: string;
}

export interface DebugGraphLayout {
  readonly width: number;
  readonly height: number;
  readonly viewBox: string;
  readonly metrics: DebugLayoutMetrics;
  readonly lanes: readonly DebugLayoutLane[];
  readonly nodes: readonly DebugLayoutNode[];
  readonly edges: readonly DebugLayoutEdge[];
}

export const DEFAULT_DEBUG_LAYOUT: DebugLayoutMetrics = Object.freeze({
  padding: 32,
  nodeWidth: 208,
  nodeHeight: 52,
  laneGap: 96,
  rowGap: 20,
  backwardTrackGap: 16,
});

const LANES: readonly DebugLane[] = ["input", "handler", "effect"];

const LANE_LABELS: Readonly<Record<DebugLane, string>> = {
  input: "Events & dependencies",
  handler: "Handlers",
  effect: "Effects",
};

const KIND_ORDER: Readonly<Record<DebugGraphNode["kind"], number>> = {
  module: -2,
  part: -1,
  port: 0,
  "ui-event": 1,
  input: 2,
  transition: 3,
  "commit-hook": 4,
  computed: 4.5,
  invariant: 4.55,
  observation: 4.6,
  update: 4.7,
  state: 5,
  command: 6,
  outcome: 7,
  presentation: 8,
  machine: 9,
};

function compareText(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function finitePositive(name: string, value: number): number {
  if (!Number.isFinite(value) || value <= 0) {
    throw new RangeError(`${name} must be a positive finite number`);
  }
  return value;
}

function resolveMetrics(options: DebugLayoutOptions): DebugLayoutMetrics {
  return {
    padding: finitePositive("padding", options.padding ?? DEFAULT_DEBUG_LAYOUT.padding),
    nodeWidth: finitePositive(
      "nodeWidth",
      options.nodeWidth ?? DEFAULT_DEBUG_LAYOUT.nodeWidth,
    ),
    nodeHeight: finitePositive(
      "nodeHeight",
      options.nodeHeight ?? DEFAULT_DEBUG_LAYOUT.nodeHeight,
    ),
    laneGap: finitePositive("laneGap", options.laneGap ?? DEFAULT_DEBUG_LAYOUT.laneGap),
    rowGap: finitePositive("rowGap", options.rowGap ?? DEFAULT_DEBUG_LAYOUT.rowGap),
    backwardTrackGap: finitePositive(
      "backwardTrackGap",
      options.backwardTrackGap ?? DEFAULT_DEBUG_LAYOUT.backwardTrackGap,
    ),
  };
}

function coordinate(value: number): string {
  const rounded = Math.round(value * 1_000) / 1_000;
  return String(Object.is(rounded, -0) ? 0 : rounded);
}

function centerY(box: DebugBox): number {
  return box.y + box.height / 2;
}

/** Left-to-right cubic path between the side centers of two node boxes. */
export function cubicDebugPath(source: DebugBox, target: DebugBox): string {
  const startX = source.x + source.width;
  const startY = centerY(source);
  const endX = target.x;
  const endY = centerY(target);
  const gap = Math.max(0, endX - startX);
  const control = Math.max(36, gap / 2);
  return [
    "M", coordinate(startX), coordinate(startY),
    "C", coordinate(startX + control), coordinate(startY),
    coordinate(endX - control), coordinate(endY),
    coordinate(endX), coordinate(endY),
  ].join(" ");
}

/** Bottom-routed orthogonal path for cycles and other non-forward edges. */
export function orthogonalDebugPath(
  source: DebugBox,
  target: DebugBox,
  trackY: number,
): string {
  const startX = source.x + source.width / 2;
  const startY = source.y + source.height;
  const endX = target.x + target.width / 2;
  const endY = target.y + target.height;
  if (trackY <= Math.max(startY, endY)) {
    throw new RangeError("orthogonal track must sit below both nodes");
  }
  return [
    "M", coordinate(startX), coordinate(startY),
    "L", coordinate(startX), coordinate(trackY),
    "L", coordinate(endX), coordinate(trackY),
    "L", coordinate(endX), coordinate(endY),
  ].join(" ");
}

function median(values: readonly number[]): number {
  if (values.length === 0) return Number.POSITIVE_INFINITY;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  const atMiddle = sorted[middle];
  if (atMiddle === undefined) return Number.POSITIVE_INFINITY;
  if (sorted.length % 2 === 1) return atMiddle;
  return ((sorted[middle - 1] ?? atMiddle) + atMiddle) / 2;
}

function connectedHandlerScore(
  node: DebugGraphNode,
  edges: readonly DebugGraphEdge[],
  handlerRanks: ReadonlyMap<string, number>,
): number {
  const ranks: number[] = [];
  for (const edge of edges) {
    if (edge.from === node.id) {
      const rank = handlerRanks.get(edge.to);
      if (rank !== undefined) ranks.push(rank);
    }
    if (edge.to === node.id) {
      const rank = handlerRanks.get(edge.from);
      if (rank !== undefined) ranks.push(rank);
    }
  }
  return median(ranks);
}

function orderedLanes(
  model: Pick<DebugGraphModel, "nodes" | "edges">,
): Readonly<Record<DebugLane, readonly DebugGraphNode[]>> {
  const handlers = model.nodes
    .filter((node) => node.lane === "handler")
    .sort((left, right) => left.order - right.order || compareText(left.id, right.id));
  const handlerRanks = new Map(handlers.map((node, index) => [node.id, index]));

  const orderPeers = (lane: Exclude<DebugLane, "handler">): DebugGraphNode[] =>
    model.nodes
      .filter((node) => node.lane === lane)
      .map((node) => ({
        node,
        score: connectedHandlerScore(node, model.edges, handlerRanks),
      }))
      .sort((left, right) =>
        left.score - right.score
        || KIND_ORDER[left.node.kind] - KIND_ORDER[right.node.kind]
        || left.node.order - right.node.order
        || compareText(left.node.id, right.node.id))
      .map(({ node }) => node);

  return {
    input: orderPeers("input"),
    handler: handlers,
    effect: orderPeers("effect"),
  };
}

/**
 * Lays out a focused debug graph into three semantic lanes. Geometry depends
 * only on node IDs/kinds/orders and static edges; runtime decoration is carried
 * through on each `DebugLayoutNode` but cannot affect coordinates or paths.
 */
export function layoutDebugGraph(
  model: Pick<DebugGraphModel, "nodes" | "edges">,
  options: DebugLayoutOptions = {},
): DebugGraphLayout {
  const metrics = resolveMetrics(options);
  const laneNodes = orderedLanes(model);
  const maxRows = Math.max(1, ...LANES.map((lane) => laneNodes[lane].length));
  const contentHeight = metrics.nodeHeight * maxRows
    + metrics.rowGap * Math.max(0, maxRows - 1);
  const rowStride = metrics.nodeHeight + metrics.rowGap;
  const laneStride = metrics.nodeWidth + metrics.laneGap;
  const lanes = LANES.map((lane, index): DebugLayoutLane => ({
    id: lane,
    label: LANE_LABELS[lane],
    x: metrics.padding + index * laneStride,
    width: metrics.nodeWidth,
  }));
  const laneX = new Map(lanes.map((lane) => [lane.id, lane.x]));

  const nodes: DebugLayoutNode[] = [];
  for (const lane of LANES) {
    const peers = laneNodes[lane];
    const peerHeight = peers.length === 0
      ? 0
      : metrics.nodeHeight * peers.length
        + metrics.rowGap * Math.max(0, peers.length - 1);
    const startY = metrics.padding + (contentHeight - peerHeight) / 2;
    for (const [index, node] of peers.entries()) {
      nodes.push({
        node,
        x: laneX.get(lane) ?? metrics.padding,
        y: startY + index * rowStride,
        width: metrics.nodeWidth,
        height: metrics.nodeHeight,
      });
    }
  }
  const boxes = new Map(nodes.map((node) => [node.node.id, node]));
  const backwardEdges = model.edges.filter((edge) => {
    const source = boxes.get(edge.from);
    const target = boxes.get(edge.to);
    return source !== undefined && target !== undefined && target.x <= source.x;
  });
  const backwardIndex = new Map(
    backwardEdges.map((edge, index) => [edge.id, index]),
  );
  const routeBaseY = metrics.padding + contentHeight + 24;
  const height = backwardEdges.length === 0
    ? metrics.padding * 2 + contentHeight
    : routeBaseY
      + (backwardEdges.length - 1) * metrics.backwardTrackGap
      + metrics.padding;
  const width = metrics.padding * 2
    + metrics.nodeWidth * LANES.length
    + metrics.laneGap * (LANES.length - 1);

  const edges = model.edges.flatMap((edge): DebugLayoutEdge[] => {
    const source = boxes.get(edge.from);
    const target = boxes.get(edge.to);
    if (!source || !target) return [];
    const index = backwardIndex.get(edge.id);
    if (index === undefined) {
      return [{ edge, route: "cubic", path: cubicDebugPath(source, target) }];
    }
    const trackY = routeBaseY + index * metrics.backwardTrackGap;
    return [{
      edge,
      route: "orthogonal",
      path: orthogonalDebugPath(source, target, trackY),
    }];
  });

  return {
    width,
    height,
    viewBox: `0 0 ${coordinate(width)} ${coordinate(height)}`,
    metrics,
    lanes,
    nodes,
    edges,
  };
}
