/**
 * Pure view-state math for Map-mode interactions: drag-vs-click
 * classification, pointer-to-board coordinate conversion, and the per-node
 * position overrides a drag produces. Overrides are session view state — they
 * survive mode toggles and revision reloads (for still-existing nodes) but
 * never persist to disk; the derived layout stays the default.
 */

import type { MapNodePosition } from "./map-layout.js";

/** A point in either screen (client px) or board coordinates. */
export interface MapPoint {
  x: number;
  y: number;
}

/** Pointer travel (screen px) below which a press-release reads as a click. */
export const MAP_DRAG_THRESHOLD_PX = 4;

/**
 * True once the pointer has travelled far enough from its press point to be
 * a drag rather than a click. Distance is Euclidean, in screen pixels, so
 * zoom never changes how a gesture classifies.
 */
export const isDragGesture = (
  start: MapPoint,
  current: MapPoint,
  threshold = MAP_DRAG_THRESHOLD_PX,
): boolean => Math.hypot(current.x - start.x, current.y - start.y) >= threshold;

/**
 * Where a dragged node sits now: its position when the drag began plus the
 * pointer delta converted from screen pixels into board units by the camera
 * scale. A degenerate (non-positive) scale falls back to identity so a bad
 * camera can never fling nodes to infinity.
 */
export const draggedMapPosition = (
  origin: MapPoint,
  start: MapPoint,
  current: MapPoint,
  cameraScale: number,
): MapPoint => {
  const scale = cameraScale > 0 ? cameraScale : 1;
  return {
    x: origin.x + (current.x - start.x) / scale,
    y: origin.y + (current.y - start.y) / scale,
  };
};

/** A new override map with `nodeId` pinned at `position`; input untouched. */
export const setMapOverride = (
  overrides: ReadonlyMap<string, MapPoint>,
  nodeId: string,
  position: MapPoint,
): Map<string, MapPoint> => new Map([...overrides, [nodeId, position]]);

/**
 * The overrides worth carrying into a re-derived map: only nodes the new
 * graph still has. Nodes that vanished drop silently — their override would
 * otherwise pin a future node of the same name to a stale position.
 */
export const retainMapOverrides = (
  overrides: ReadonlyMap<string, MapPoint>,
  nodeIds: ReadonlySet<string>,
): Map<string, MapPoint> =>
  new Map([...overrides].filter(([nodeId]) => nodeIds.has(nodeId)));

/**
 * The layout the map actually renders: derived positions with any dragged
 * node moved to its override. Columns (and every non-overridden node) keep
 * their derived values; both inputs stay untouched.
 */
export const applyMapOverrides = (
  positions: ReadonlyMap<string, MapNodePosition>,
  overrides: ReadonlyMap<string, MapPoint>,
): Map<string, MapNodePosition> =>
  new Map([...positions].map(([nodeId, position]) => {
    const override = overrides.get(nodeId);
    return [nodeId, override ? { ...position, x: override.x, y: override.y } : position];
  }));
