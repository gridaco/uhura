import type { InteractionGraph } from "../protocol/types.js";
import type { EditorPreview } from "./editor-state.js";
import {
  assignConnectorLanes,
  assignConnectorPorts,
  type ConnectorPort,
} from "./workflow-connectors.js";

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
  lane: number;
  sourcePort: ConnectorPort;
  targetPort: ConnectorPort;
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
 * firing event. Candidates carry no lane geometry yet — the board shows only
 * the selection-scoped subset, and `layoutStructureConnectors` packs lanes
 * over that subset so rails stay shallow.
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
    const connector: StructureConnector = {
      ...edge,
      extraCount: 0,
      lane: 0,
      sourcePort: { slot: 0, count: 1 },
      targetPort: { slot: 0, count: 1 },
    };
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

/**
 * Packs lanes and fan-out ports over the visible subset only, so a selected
 * page's handful of arrows never inherits the deep rail tower the full graph
 * would need. Lanes continue after `laneOffset` so structural rails stack
 * above the replay-provenance rails instead of colliding with them.
 */
export const layoutStructureConnectors = <T extends StructureConnector>(
  connectors: readonly T[],
  frameIndex: ReadonlyMap<string, number>,
  laneOffset = 0,
): T[] => {
  const lanes = assignConnectorLanes(connectors, frameIndex, laneOffset);
  const laid = connectors.map((connector, index) => ({
    ...connector,
    lane: lanes[index]!,
    sourcePort: { slot: 0, count: 1 },
    targetPort: { slot: 0, count: 1 },
  }));
  assignConnectorPorts(laid, frameIndex);
  return laid;
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
