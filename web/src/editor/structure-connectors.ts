import type { InteractionGraph } from "../protocol/types.js";
import type { EditorPreview } from "./editor-state.js";

/**
 * The structural interaction kinds the board draws in v1. State changes,
 * commands, outcomes, dismissals, and back-navigation are deliberate noise
 * cuts: they do not add page/surface topology.
 */
export type StructureConnectorKind = "navigate" | "present";

/** The page/surface definition behind a board selection. */
export interface StructureDefinition {
  kind: string;
  subject: string;
}

/** One deduplicated structural edge between two board frames. */
export interface StructureConnector {
  kind: StructureConnectorKind;
  /** The `page:<name>`/`surface:<name>` graph node behind each endpoint. */
  sourceNode: string;
  targetNode: string;
  sourceId: string;
  targetId: string;
  /** The firing event of the first deduplicated edge, in sorted order. */
  event: string;
  /** How many further edges share the same (source, target, kind). */
  extraCount: number;
}

/**
 * Maps `page:<name>`/`surface:<name>` graph nodes to the first board frame
 * that previews the same definition. Command and dynamic nodes, and
 * definitions without previews, have no frame and draw nothing.
 */
const frameIdByGraphNode = (
  previews: readonly EditorPreview[],
): Map<string, string> => {
  const frames = new Map<string, string>();
  for (const preview of previews) {
    const kind = preview.identity.kind;
    if (kind !== "page" && kind !== "surface") continue;
    const nodeId = `${kind}:${preview.identity.subject}`;
    if (!frames.has(nodeId)) frames.set(nodeId, preview.id);
  }
  return frames;
};

const compareStrings = (left: readonly string[], right: readonly string[]): number => {
  for (let index = 0; index < left.length; index += 1) {
    if (left[index]! < right[index]!) return -1;
    if (left[index]! > right[index]!) return 1;
  }
  return 0;
};

/**
 * Projects the app's checked interaction graph onto the board: one candidate
 * connector per distinct (source frame, target frame, kind), labeled with its
 * firing event. Candidates carry no geometry yet — the board shows only the
 * selection-scoped subset, and `layoutStructureConnectors` fans that subset
 * around the frame the user actually clicked.
 */
export const buildStructureConnectors = (
  graph: InteractionGraph,
  previews: readonly EditorPreview[],
): StructureConnector[] => {
  const frames = frameIdByGraphNode(previews);

  const structural = graph.edges
    .flatMap((edge) => {
      if (edge.kind !== "navigate" && edge.kind !== "present") return [];
      const sourceId = frames.get(edge.from);
      const targetId = frames.get(edge.to);
      if (sourceId === undefined || targetId === undefined || sourceId === targetId) return [];
      return [{
        kind: edge.kind,
        sourceNode: edge.from,
        targetNode: edge.to,
        sourceId,
        targetId,
        event: edge.event,
      }];
    })
    .sort((left, right) => compareStrings(
      [left.sourceId, left.targetId, left.kind, left.event],
      [right.sourceId, right.targetId, right.kind, right.event],
    ));

  const byKey = new Map<string, StructureConnector>();
  const deduped: StructureConnector[] = [];
  for (const edge of structural) {
    const key = JSON.stringify([edge.sourceId, edge.targetId, edge.kind]);
    const existing = byKey.get(key);
    if (existing) {
      existing.extraCount += 1;
      continue;
    }
    const connector: StructureConnector = { ...edge, extraCount: 0 };
    byKey.set(key, connector);
    deduped.push(connector);
  }
  return deduped;
};

/** The graph node a selected preview's definition resolves to. */
export const structureDefinitionNode = (definition: StructureDefinition): string =>
  `${definition.kind}:${definition.subject}`;

/**
 * Figma-style selection scoping: with no selection nothing structural draws;
 * with a selected preview only the connectors entering or leaving that
 * preview's definition (kind + subject) remain.
 */
export const visibleStructureConnectors = <T extends StructureConnector>(
  connectors: readonly T[],
  selected: StructureDefinition | null,
): T[] => {
  if (!selected) return [];
  const node = structureDefinitionNode(selected);
  return connectors.filter((connector) =>
    connector.sourceNode === node || connector.targetNode === node);
};

export type StructureConnectorDirection = "outgoing" | "incoming";

/** The selected frame's edge a connector fans out on. */
export type StructureEdgeSide = "right" | "left" | "bottom" | "top";

/** The board frame the user actually clicked, plus its definition node. */
export interface StructureSelection {
  node: string;
  previewId: string;
}

/**
 * Where a visible connector attaches to the SELECTED frame. Outgoing
 * navigation exits the right edge, incoming navigation arrives at the left
 * edge, presentation exits the bottom edge, and a selected surface receives
 * incoming presents at its top edge. Slots fan siblings along that edge.
 */
export interface StructureConnectorPlacement {
  direction: StructureConnectorDirection;
  side: StructureEdgeSide;
  slot: number;
  slotCount: number;
  /** The clicked frame anchoring the near end of the connector. */
  selectedId: string;
  /** The far definition's first frame anchoring the other end. */
  farId: string;
}

export type PlacedStructureConnector<T extends StructureConnector> = T & {
  placement: StructureConnectorPlacement;
};

const placementSide = (
  kind: StructureConnectorKind,
  direction: StructureConnectorDirection,
): StructureEdgeSide => kind === "navigate"
  ? direction === "outgoing" ? "right" : "left"
  : direction === "outgoing" ? "bottom" : "top";

/**
 * Direction-aware layout over the visible subset only: classifies each
 * connector relative to the selected definition, assigns it the matching
 * edge of the CLICKED frame, and fans siblings sharing an edge into
 * deterministic slots (sorted by kind, far node, then event).
 */
export const layoutStructureConnectors = <T extends StructureConnector>(
  connectors: readonly T[],
  selected: StructureSelection,
): PlacedStructureConnector<T>[] => {
  const classified = connectors
    .map((connector) => {
      const direction: StructureConnectorDirection =
        connector.sourceNode === selected.node ? "outgoing" : "incoming";
      const farId = direction === "outgoing" ? connector.targetId : connector.sourceId;
      const farNode = direction === "outgoing" ? connector.targetNode : connector.sourceNode;
      return {
        connector,
        direction,
        farId,
        farNode,
        side: placementSide(connector.kind, direction),
      };
    })
    .sort((left, right) => compareStrings(
      [left.connector.kind, left.side, left.farNode, left.connector.event],
      [right.connector.kind, right.side, right.farNode, right.connector.event],
    ));

  const countBySide = new Map<StructureEdgeSide, number>();
  for (const entry of classified) {
    countBySide.set(entry.side, (countBySide.get(entry.side) ?? 0) + 1);
  }
  const usedBySide = new Map<StructureEdgeSide, number>();
  return classified.map((entry) => {
    const slot = usedBySide.get(entry.side) ?? 0;
    usedBySide.set(entry.side, slot + 1);
    return {
      ...entry.connector,
      placement: {
        direction: entry.direction,
        side: entry.side,
        slot,
        slotCount: countBySide.get(entry.side)!,
        selectedId: selected.previewId,
        farId: entry.farId,
      },
    };
  });
};

export interface StructureRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface StructureConnectorRoute {
  path: string;
  arrow: string;
  /** The dot marking the departure end of the connector. */
  origin: { x: number; y: number };
  /** Label anchor, clustered around the selected frame's end. */
  label: { x: number; y: number; anchor: "start" | "end" };
}

/** How far a route stubs out of a frame edge before turning, board units. */
const STRUCTURE_STUB = 28;
/** Per-slot stagger keeping sibling stubs off a shared vertical/horizontal. */
const STRUCTURE_SLOT_STAGGER = 14;
/** Gap between the selected frame's edge and its label pill, marker units. */
const STRUCTURE_LABEL_GAP = 8;
/** Vertical rhythm for labels stacked outside a horizontal edge. */
const STRUCTURE_LABEL_STACK = 18;
const STRUCTURE_ARROW_WIDTH = 4.5;
/** Per-slot spread of a shared midpoint corridor, board units. */
const STRUCTURE_CORRIDOR_STAGGER = 10;
/** Clearance a staggered corridor keeps from either frame edge. */
const STRUCTURE_CORRIDOR_CLEARANCE = 4;

/**
 * The turning corridor between two frame edges, staggered per fan slot so
 * sibling routes through the same gap never share one vertical (or
 * horizontal) line. The per-slot step shrinks when the gap is too narrow for
 * the full stagger, keeping every slot distinct yet inside the gap; gaps too
 * thin to stagger at all fall back to the shared midpoint.
 */
const staggeredCorridor = (
  from: number,
  to: number,
  slot: number,
  count: number,
): number => {
  const mid = (from + to) / 2;
  const low = Math.min(from, to) + STRUCTURE_CORRIDOR_CLEARANCE;
  const high = Math.max(from, to) - STRUCTURE_CORRIDOR_CLEARANCE;
  if (low >= high || count <= 1) return mid;
  const step = Math.min(STRUCTURE_CORRIDOR_STAGGER, (high - low) / (count - 1));
  const offset = (slot - (count - 1) / 2) * step;
  return Math.min(Math.max(mid + offset, low), high);
};

/**
 * The facing edge of the nearest neighboring frame past the selected edge in
 * the stub direction, or undefined when the fan has open space. Only frames
 * whose cross-axis span overlaps the selected frame's can block the fan; the
 * selected frame itself never qualifies (nothing of it lies past its edge).
 */
const nearestNeighborEdge = (
  selected: StructureRect,
  neighbors: readonly StructureRect[],
  side: StructureEdgeSide,
): number | undefined => {
  const horizontal = side === "right" || side === "left";
  const sign = side === "right" || side === "bottom" ? 1 : -1;
  const span = (rect: StructureRect): readonly [number, number] =>
    horizontal ? [rect.x, rect.x + rect.width] : [rect.y, rect.y + rect.height];
  const overlapsCrossAxis = (rect: StructureRect): boolean => horizontal
    ? rect.y < selected.y + selected.height && rect.y + rect.height > selected.y
    : rect.x < selected.x + selected.width && rect.x + rect.width > selected.x;
  const [selectedStart, selectedEnd] = span(selected);
  const edge = sign > 0 ? selectedEnd : selectedStart;
  const distances = neighbors
    .filter(overlapsCrossAxis)
    .flatMap((rect) => {
      const [start, end] = span(rect);
      const blocks = sign * ((sign > 0 ? end : start) - edge) > 0;
      return blocks ? [sign * ((sign > 0 ? start : end) - edge)] : [];
    });
  return distances.length > 0 ? edge + sign * Math.min(...distances) : undefined;
};

/**
 * The stub turn coordinate for one fan slot, clamped inside the free gap
 * between the selected frame's edge and its nearest neighbor in the stub
 * direction. Mirrors `staggeredCorridor`: open space keeps the ideal
 * `STRUCTURE_STUB + slot * STRUCTURE_SLOT_STAGGER` fan; a narrow gap
 * compresses the fan against the far clearance line, shrinking the per-slot
 * step so every slot stays distinct and inside the gap; a degenerate or
 * negative gap (overlapping neighbor) hugs the frame edge a few pixels out.
 */
const staggeredStub = (
  edge: number,
  sign: 1 | -1,
  neighborEdge: number | undefined,
  slot: number,
  count: number,
): number => {
  const ideal = STRUCTURE_STUB + slot * STRUCTURE_SLOT_STAGGER;
  if (neighborEdge === undefined) return edge + sign * ideal;
  const gap = sign * (neighborEdge - edge);
  const low = STRUCTURE_CORRIDOR_CLEARANCE;
  const high = gap - STRUCTURE_CORRIDOR_CLEARANCE;
  if (high <= low) return edge + sign * Math.max(Math.min(low, gap / 2), 0);
  const step = count > 1
    ? Math.min(STRUCTURE_SLOT_STAGGER, (high - low) / (count - 1))
    : 0;
  return edge + sign * Math.min(ideal, high - (count - 1 - slot) * step);
};

interface RoutePoint {
  x: number;
  y: number;
}

const fanPoint = (
  rect: StructureRect,
  side: StructureEdgeSide,
  slot: number,
  count: number,
): RoutePoint => {
  if (side === "right" || side === "left") {
    const step = rect.height / (count + 1);
    return {
      x: side === "right" ? rect.x + rect.width : rect.x,
      y: rect.y + step * (slot + 1),
    };
  }
  const step = rect.width / (count + 1);
  return {
    x: rect.x + step * (slot + 1),
    y: side === "bottom" ? rect.y + rect.height : rect.y,
  };
};

const orthogonalPath = (points: readonly RoutePoint[]): string => {
  const deduped = points.filter((point, index) => {
    const previous = points[index - 1];
    return !previous || previous.x !== point.x || previous.y !== point.y;
  });
  return deduped
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`)
    .join(" ");
};

/** A triangle at (x, y) pointing along the axis-aligned direction (dx, dy). */
const arrowHead = (
  tip: RoutePoint,
  dx: number,
  dy: number,
  markerScale: number,
): string => {
  const width = STRUCTURE_ARROW_WIDTH * markerScale;
  const length = width * 2;
  const baseX = tip.x - dx * length;
  const baseY = tip.y - dy * length;
  return `M ${baseX - dy * width} ${baseY + dx * width} `
    + `L ${tip.x} ${tip.y} `
    + `L ${baseX + dy * width} ${baseY - dx * width} Z`;
};

/**
 * Routes one placed structural connector between the CLICKED frame and the
 * far frame. Outgoing routes exit the selected edge and turn in the gap
 * between the frames, staggered per fan slot around the gap midpoint so
 * parallel siblings never bundle onto one line — no global lane stacking.
 * Arrowheads always sit at the target end; labels always sit just outside
 * the selected frame's edge so everything readable clusters at the click.
 * `markerScale` counter-scales arrowheads and label offsets so they keep a
 * constant on-screen size at low zoom. `neighbors` are the other board frame
 * rects; stub fans clamp inside the free gap to the nearest one so no drop
 * ever runs through an adjacent frame's content.
 */
export const routeStructureConnector = (
  placement: StructureConnectorPlacement,
  selected: StructureRect,
  far: StructureRect,
  markerScale = 1,
  neighbors: readonly StructureRect[] = [],
): StructureConnectorRoute => {
  const { side, slot, slotCount } = placement;
  const stub = (edge: number, sign: 1 | -1): number => staggeredStub(
    edge,
    sign,
    nearestNeighborEdge(selected, neighbors, side),
    slot,
    slotCount,
  );

  if (side === "right") {
    const start = fanPoint(selected, "right", slot, slotCount);
    const label = {
      x: start.x + STRUCTURE_LABEL_GAP * markerScale,
      y: start.y,
      anchor: "start" as const,
    };
    if (far.x >= start.x) {
      // Target column is to the right: enter its left edge through a
      // slot-staggered vertical corridor in the gap between the columns.
      const end = { x: far.x, y: far.y + far.height / 2 };
      const corridorX = staggeredCorridor(start.x, far.x, slot, slotCount);
      return {
        path: orthogonalPath([
          start,
          { x: corridorX, y: start.y },
          { x: corridorX, y: end.y },
          end,
        ]),
        arrow: arrowHead(end, 1, 0, markerScale),
        origin: start,
        label,
      };
    }
    // Target is left of or above the exit: stub right, then approach the
    // target's top edge from above.
    const end = { x: far.x + far.width / 2, y: far.y };
    const stubX = stub(start.x, 1);
    const approachY = far.y - STRUCTURE_STUB;
    return {
      path: orthogonalPath([
        start,
        { x: stubX, y: start.y },
        { x: stubX, y: approachY },
        { x: end.x, y: approachY },
        end,
      ]),
      arrow: arrowHead(end, 0, 1, markerScale),
      origin: start,
      label,
    };
  }

  if (side === "left") {
    const end = fanPoint(selected, "left", slot, slotCount);
    const exit = { x: far.x + far.width, y: far.y + far.height / 2 };
    const corridorX = exit.x <= selected.x
      ? staggeredCorridor(exit.x, selected.x, slot, slotCount)
      : stub(selected.x, -1);
    return {
      path: orthogonalPath([
        exit,
        { x: corridorX, y: exit.y },
        { x: corridorX, y: end.y },
        end,
      ]),
      arrow: arrowHead(end, 1, 0, markerScale),
      origin: exit,
      label: {
        x: end.x - STRUCTURE_LABEL_GAP * markerScale,
        y: end.y,
        anchor: "end",
      },
    };
  }

  if (side === "bottom") {
    const start = fanPoint(selected, "bottom", slot, slotCount);
    const end = { x: far.x + far.width / 2, y: far.y };
    const corridorY = far.y >= start.y + STRUCTURE_STUB * 2
      ? staggeredCorridor(start.y, far.y, slot, slotCount)
      : stub(start.y, 1);
    return {
      path: orthogonalPath([
        start,
        { x: start.x, y: corridorY },
        { x: end.x, y: corridorY },
        end,
      ]),
      arrow: arrowHead(end, 0, end.y >= corridorY ? 1 : -1, markerScale),
      origin: start,
      label: {
        x: start.x + (STRUCTURE_LABEL_GAP - 2) * markerScale,
        y: start.y + (STRUCTURE_LABEL_GAP + 2 + slot * STRUCTURE_LABEL_STACK) * markerScale,
        anchor: "start",
      },
    };
  }

  const end = fanPoint(selected, "top", slot, slotCount);
  const exit = { x: far.x + far.width / 2, y: far.y + far.height };
  const corridorY = exit.y <= selected.y
    ? staggeredCorridor(exit.y, selected.y, slot, slotCount)
    : stub(selected.y, -1);
  return {
    path: orthogonalPath([
      exit,
      { x: exit.x, y: corridorY },
      { x: end.x, y: corridorY },
      end,
    ]),
    arrow: arrowHead(end, 0, end.y >= corridorY ? 1 : -1, markerScale),
    origin: exit,
    label: {
      x: end.x + (STRUCTURE_LABEL_GAP - 2) * markerScale,
      y: end.y - (STRUCTURE_LABEL_GAP + 2 + slot * STRUCTURE_LABEL_STACK) * markerScale,
      anchor: "start",
    },
  };
};

export const structureConnectorLabel = (
  connector: Pick<StructureConnector, "event" | "extraCount">,
): string => connector.extraCount > 0
  ? `${connector.event} +${connector.extraCount}`
  : connector.event;

export const structureConnectorDescription = (
  connector: Pick<StructureConnector, "kind" | "event" | "extraCount">,
): string => {
  const action = connector.kind === "navigate" ? "navigates on" : "presents on";
  const more = connector.extraCount > 0 ? ` (+${connector.extraCount} more)` : "";
  return `${action} ${connector.event}${more}`;
};
