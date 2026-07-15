import type { EditorPreview } from "./editor-state.js";
import { directlyOpenedSurfaces, type MountedSurface } from "./surface-hierarchy.js";

export interface WorkflowConnector {
  groupId: string;
  sourceId: string;
  targetId: string;
  steps: string[];
  openedSurfaces: MountedSurface[];
  lane: number;
}

interface Interval {
  start: number;
  end: number;
}

const intervalsOverlap = (left: Interval, right: Interval): boolean =>
  left.start <= right.end && right.start <= left.end;

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

  const lanes: Interval[][] = [];
  return previews.flatMap((preview): WorkflowConnector[] => {
    if (!preview.from) return [];
    const sourceId = idByExample.get(preview.from);
    const sourceIndex = sourceId === undefined ? undefined : frameIndex.get(sourceId);
    const targetIndex = frameIndex.get(preview.id);
    if (sourceId === undefined || sourceIndex === undefined || targetIndex === undefined) return [];

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
    return [{
      groupId,
      sourceId,
      targetId: preview.id,
      steps: [...preview.replaySteps],
      openedSurfaces: directlyOpenedSurfaces(preview),
      lane,
    }];
  });
};

export const workflowConnectorLabel = (
  steps: readonly string[],
  openedSurfaces: readonly Pick<MountedSurface, "definition">[] = [],
): string => {
  const replay = steps.length === 0
    ? "derived"
    : steps.length === 1
      ? steps[0]!
      : `${steps[0]} +${steps.length - 1}`;
  if (openedSurfaces.length === 0) return replay;
  return `${replay} · opens ${openedSurfaces.map((surface) => surface.definition).join(", ")}`;
};

export const workflowConnectorDescription = (
  connector: Pick<WorkflowConnector, "steps" | "openedSurfaces">,
): string => {
  const replay = connector.steps.length === 0
    ? "derived example"
    : connector.steps.join(" → ");
  if (connector.openedSurfaces.length === 0) return replay;
  const children = connector.openedSurfaces
    .map((surface) => `${surface.modality} ${surface.definition}`)
    .join(", ");
  return `${replay}; opens child ${children}`;
};
