import type { EditorPreview } from "./editor-state.js";
import { introducedSurfaces, type MountedSurface } from "./surface-hierarchy.js";

export interface WorkflowConnector {
  groupId: string;
  sourceId: string;
  targetId: string;
  steps: string[];
  introducedSurfaces: MountedSurface[];
  lane: number;
  sourcePort: ConnectorPort;
  targetPort: ConnectorPort;
}

interface Interval {
  start: number;
  end: number;
}

export interface ConnectorPort {
  slot: number;
  count: number;
}

export interface WorkflowRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface WorkflowConnectorRoute {
  path: string;
  arrow: string;
  origin: { x: number; y: number };
  label: { x: number; y: number };
  railY: number;
}

export const WORKFLOW_LANE_GAP = 20;
export const WORKFLOW_RAIL_CLEARANCE = 18;
const WORKFLOW_RAIL_PADDING = 28;
const WORKFLOW_PORT_SPREAD = 96;

export const workflowRailHeight = (laneCount: number): number =>
  laneCount > 0 ? WORKFLOW_RAIL_PADDING + laneCount * WORKFLOW_LANE_GAP : 0;

const intervalsOverlap = (left: Interval, right: Interval): boolean =>
  left.start <= right.end && right.start <= left.end;

/** A routed edge between two board frames, whatever it means semantically. */
export interface ConnectorEnds {
  sourceId: string;
  targetId: string;
  sourcePort: ConnectorPort;
  targetPort: ConnectorPort;
}

/**
 * Packs connectors first-fit into rail lanes so overlapping horizontal frame
 * spans never share a lane. Returns one lane per connector, in input order,
 * shifted by `laneOffset` so independent connector families can stack.
 */
export const assignConnectorLanes = (
  connectors: readonly Pick<ConnectorEnds, "sourceId" | "targetId">[],
  frameIndex: ReadonlyMap<string, number>,
  laneOffset = 0,
): number[] => {
  const lanes: Interval[][] = [];
  return connectors.map((connector) => {
    const sourceIndex = frameIndex.get(connector.sourceId) ?? 0;
    const targetIndex = frameIndex.get(connector.targetId) ?? 0;
    const interval = {
      start: Math.min(sourceIndex, targetIndex),
      end: Math.max(sourceIndex, targetIndex),
    };
    let lane = lanes.findIndex((used) =>
      used.every((other) => !intervalsOverlap(interval, other)));
    if (lane < 0) {
      lane = lanes.length;
      lanes.push([]);
    }
    lanes[lane]!.push(interval);
    return laneOffset + lane;
  });
};

/**
 * Fans connectors sharing a frame endpoint across deterministic ports so
 * siblings never overlap the same vertical segment. Assigns the nearest
 * rightward opposite to the rightmost source port so deeper stems remain
 * left of shallower rails. Writes the ports onto the given connectors.
 */
export const assignConnectorPorts = (
  connectors: readonly ConnectorEnds[],
  frameIndex: ReadonlyMap<string, number>,
): void => {
  const assign = (
    endpoint: "sourceId" | "targetId",
    port: "sourcePort" | "targetPort",
    opposite: "sourceId" | "targetId",
    order: 1 | -1,
  ): void => {
    const byEndpoint = new Map<string, ConnectorEnds[]>();
    for (const connector of connectors) {
      const group = byEndpoint.get(connector[endpoint]) ?? [];
      group.push(connector);
      byEndpoint.set(connector[endpoint], group);
    }
    for (const group of byEndpoint.values()) {
      group.sort((left, right) =>
        order * ((frameIndex.get(left[opposite]) ?? 0) - (frameIndex.get(right[opposite]) ?? 0)));
      group.forEach((connector, slot) => {
        connector[port] = { slot, count: group.length };
      });
    }
  };
  assign("sourceId", "sourcePort", "targetId", -1);
  assign("targetId", "targetPort", "sourceId", 1);
};

/**
 * Builds checked replay-provenance edges for one subject group.
 * Parents must precede their children, matching the checker invariant.
 */
export const buildWorkflowConnectors = (
  groupId: string,
  previews: readonly EditorPreview[],
): WorkflowConnector[] => {
  const idByExample = new Map<string, string>();
  const frameIndex = new Map<string, number>();
  previews.forEach((preview, index) => {
    idByExample.set(preview.identity.example, preview.id);
    frameIndex.set(preview.id, index);
  });

  const unplaced = previews.flatMap((preview): WorkflowConnector[] => {
    if (!preview.from) return [];
    const sourceId = idByExample.get(preview.from);
    if (sourceId === undefined || !frameIndex.has(sourceId) || !frameIndex.has(preview.id)) {
      return [];
    }
    return [{
      groupId,
      sourceId,
      targetId: preview.id,
      steps: [...preview.replaySteps],
      introducedSurfaces: introducedSurfaces(preview, previews),
      lane: 0,
      sourcePort: { slot: 0, count: 1 },
      targetPort: { slot: 0, count: 1 },
    }];
  });

  const lanes = assignConnectorLanes(unplaced, frameIndex);
  const connectors = unplaced.map((connector, index) => ({
    ...connector,
    lane: lanes[index]!,
  }));
  assignConnectorPorts(connectors, frameIndex);
  return connectors;
};

const portX = (rect: WorkflowRect, port: ConnectorPort): number => {
  const center = rect.x + rect.width / 2;
  if (port.count <= 1) return center;
  const count = Math.max(2, port.count);
  const slot = Math.min(Math.max(0, port.slot), count - 1);
  const spread = Math.min(WORKFLOW_PORT_SPREAD, rect.width * 0.5);
  return center - spread / 2 + spread * slot / (count - 1);
};

const overlapsHorizontalSpan = (
  rect: WorkflowRect,
  startX: number,
  endX: number,
): boolean => rect.x <= endX && rect.x + rect.width >= startX;

/**
 * Routes a connector through the reserved rail above every frame between its
 * endpoints. Fan-out ports prevent siblings from sharing the same vertical
 * segment; the lane keeps overlapping horizontal spans separate.
 */
export const routeWorkflowConnector = (
  connector: Pick<WorkflowConnector, "lane" | "sourcePort" | "targetPort">,
  source: WorkflowRect,
  target: WorkflowRect,
  obstacles: readonly WorkflowRect[],
): WorkflowConnectorRoute => {
  const startX = portX(source, connector.sourcePort);
  const startY = source.y;
  const endX = portX(target, connector.targetPort);
  const endY = target.y;
  const spanStart = Math.min(startX, endX);
  const spanEnd = Math.max(startX, endX);
  const obstacleTop = obstacles
    .filter((rect) => overlapsHorizontalSpan(rect, spanStart, spanEnd))
    .reduce((top, rect) => Math.min(top, rect.y), Math.min(startY, endY));
  const railY = obstacleTop - WORKFLOW_RAIL_CLEARANCE
    - connector.lane * WORKFLOW_LANE_GAP;

  return {
    path: `M ${startX} ${startY} L ${startX} ${railY} L ${endX} ${railY} L ${endX} ${endY}`,
    arrow: `M ${endX - 4} ${endY - 8} L ${endX} ${endY} L ${endX + 4} ${endY - 8} Z`,
    origin: { x: startX, y: startY },
    label: { x: (startX + endX) / 2, y: railY - 6 },
    railY,
  };
};

export const workflowConnectorLabel = (
  steps: readonly string[],
  surfaces: readonly Pick<MountedSurface, "definition">[] = [],
): string => {
  const replay = steps.length === 0
    ? "derived"
    : steps.length === 1
      ? steps[0]!
      : `${steps[0]} +${steps.length - 1}`;
  if (surfaces.length === 0) return replay;
  return `${replay} · introduces ${surfaces.map((surface) => surface.definition).join(", ")}`;
};

export const workflowConnectorDescription = (
  connector: Pick<WorkflowConnector, "steps" | "introducedSurfaces">,
): string => {
  const replay = connector.steps.length === 0
    ? "derived example"
    : connector.steps.join(" → ");
  if (connector.introducedSurfaces.length === 0) return replay;
  const children = connector.introducedSurfaces
    .map((surface) => `${surface.modality} ${surface.definition}`)
    .join(", ");
  return `${replay}; projection introduces ${children}`;
};
