import {
  structureNodeName,
  type StructureConnector,
  type StructureConnectorDirection,
} from "./structure-connectors.js";

/**
 * Pure presentation helpers over the structural routes: corner rounding,
 * draw-in staggering, and label segmentation. Routing GEOMETRY (waypoints)
 * lives in structure-connectors.ts and is never changed here — every helper
 * only re-renders the same waypoints with more polish.
 */

export interface StructureRoutePoint {
  x: number;
  y: number;
}

/** Corner arc radius for rounded orthogonal routes, board units. */
export const STRUCTURE_CORNER_RADIUS = 8;
/** Per-connector draw-in stagger, deterministic in slot order. */
export const STRUCTURE_DRAW_STAGGER_MS = 40;

/**
 * Renders an orthogonal waypoint list as a path with a quadratic arc at every
 * turn. The radius clamps to half the shorter adjacent segment so short
 * segments stay monotone; zero-length segments are dropped and collinear
 * (or reversing) waypoints draw straight through. Endpoints never move, so
 * arrowheads and origin dots keep their exact anchors.
 */
export const roundedOrthogonalPath = (
  points: readonly StructureRoutePoint[],
  radius: number = STRUCTURE_CORNER_RADIUS,
): string => {
  const route = points.filter((point, index) => {
    const previous = points[index - 1];
    return !previous || previous.x !== point.x || previous.y !== point.y;
  });
  const first = route[0];
  if (!first) return "";
  let path = `M ${first.x} ${first.y}`;
  for (let index = 1; index < route.length - 1; index += 1) {
    const previous = route[index - 1]!;
    const corner = route[index]!;
    const next = route[index + 1]!;
    const inX = corner.x - previous.x;
    const inY = corner.y - previous.y;
    const outX = next.x - corner.x;
    const outY = next.y - corner.y;
    const inLength = Math.hypot(inX, inY);
    const outLength = Math.hypot(outX, outY);
    const arc = Math.min(radius, inLength / 2, outLength / 2);
    const turns = inX * outY - inY * outX !== 0;
    if (!turns || arc <= 0) {
      path += ` L ${corner.x} ${corner.y}`;
      continue;
    }
    const entryX = corner.x - (inX / inLength) * arc;
    const entryY = corner.y - (inY / inLength) * arc;
    const exitX = corner.x + (outX / outLength) * arc;
    const exitY = corner.y + (outY / outLength) * arc;
    path += ` L ${entryX} ${entryY} Q ${corner.x} ${corner.y} ${exitX} ${exitY}`;
  }
  const last = route[route.length - 1];
  if (route.length > 1 && last) path += ` L ${last.x} ${last.y}`;
  return path;
};

/** The (x, y) waypoints of an orthogonal `M … L …` route path string. */
export const structurePathWaypoints = (path: string): StructureRoutePoint[] =>
  [...path.matchAll(
    /[ML] (-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?) (-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g,
  )].map((match) => ({ x: Number(match[1]), y: Number(match[2]) }));

/**
 * Rounds a routed orthogonal path string without touching its waypoints:
 * presentation-only sugar over `routeStructureConnector` output.
 */
export const roundedStructurePath = (
  path: string,
  radius: number = STRUCTURE_CORNER_RADIUS,
): string => roundedOrthogonalPath(structurePathWaypoints(path), radius);

/** Draw-in delay for the connector at a deterministic slot-order index. */
export const structureDrawDelayMs = (index: number): number =>
  Math.max(index, 0) * STRUCTURE_DRAW_STAGGER_MS;

/**
 * The draw-in animation replays only when the selected preview actually
 * changes: relayouts (pan, zoom, reconcile) of the same selection keep the
 * connectors steady, while deselecting and re-selecting re-triggers cleanly.
 */
export const shouldReplayStructureDraw = (
  previousPreviewId: string | null,
  nextPreviewId: string,
): boolean => previousPreviewId !== nextPreviewId;

/** Kind glyph prefixing the pill: presents drop in, navigation points. */
export const structureConnectorGlyph = (
  kind: StructureConnector["kind"],
  direction: StructureConnectorDirection,
): string => kind === "present" ? "⤓" : direction === "outgoing" ? "→" : "←";

export interface StructureLabelSegments {
  glyph: string;
  /** Glyph + event + separator, rendered at normal weight. */
  lead: string;
  /** The far endpoint's name, rendered as the bolder tspan segment. */
  farName: string;
  /** The `+N` dedup suffix, empty when the edge is unique. */
  suffix: string;
  /** The full plain-text label, for accessibility and measurement. */
  text: string;
}

/**
 * Splits the pill label into render segments: a kind glyph prefix, the firing
 * event, and the far endpoint's name (bolded by the caller via tspan), with
 * the dedup suffix kept at the end.
 */
export const structureConnectorLabelSegments = (
  connector: Pick<
    StructureConnector,
    "kind" | "event" | "extraCount" | "sourceNode" | "targetNode"
  >,
  direction: StructureConnectorDirection = "outgoing",
): StructureLabelSegments => {
  const glyph = structureConnectorGlyph(connector.kind, direction);
  const farName = structureNodeName(
    direction === "outgoing" ? connector.targetNode : connector.sourceNode,
  );
  const lead = `${glyph} ${connector.event} · `;
  const suffix = connector.extraCount > 0 ? ` +${connector.extraCount}` : "";
  return { glyph, lead, farName, suffix, text: `${lead}${farName}${suffix}` };
};
