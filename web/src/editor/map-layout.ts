/**
 * Pure layout for the editor's Map view: one node per page/surface in the
 * checked interaction graph, positioned BY the graph rather than by example
 * grouping. Columns are navigation depth (BFS from the app's entry page over
 * `navigate` edges), surfaces hang below the page that presents them, and
 * pages the entry can't reach land in a final trailing column. Everything is
 * deterministic in (graph, sizes) — no DOM, no randomness.
 */

/** The graph subset the map layout reads (a structural InteractionGraph). */
export interface MapGraph {
  entry?: string;
  nodes: ReadonlyArray<{ id: string; kind: string }>;
  edges: ReadonlyArray<{ kind: string; from: string; to: string; event: string }>;
}

export interface MapNodeSize {
  width: number;
  height: number;
}

export interface MapNodePosition {
  x: number;
  y: number;
  /** Navigation depth; unreachable nodes share the final column. */
  column: number;
}

/**
 * Map nodes render at this CSS scale (top-left origin), so a phone frame
 * occupies a fraction of its natural footprint and the whole map fits on
 * roughly one screen at moderate zoom. Layout positions are computed from the
 * SCALED footprints, keeping the gaps below proportional to what is drawn.
 */
export const MAP_NODE_SCALE = 0.4;

/**
 * The on-screen footprint of a map node: its raw box under MAP_NODE_SCALE.
 * `offsetWidth`/`offsetHeight` ignore CSS transforms, so callers measuring
 * raw boxes scale them here before layout; `getBoundingClientRect` already
 * reflects the transform and needs no adjustment.
 */
export const scaledMapNodeSize = (
  size: MapNodeSize,
  scale = MAP_NODE_SCALE,
): MapNodeSize => ({
  width: size.width * scale,
  height: size.height * scale,
});

/** Horizontal gap between navigation-depth columns, board units. Sized
 * against MAP_NODE_SCALE-scaled footprints. */
export const MAP_COLUMN_GAP = 72;
/** Vertical gap between page cells stacked in one column, board units. */
export const MAP_ROW_GAP = 48;
/** Vertical gap between a page and the surfaces it presents, board units. */
export const MAP_SURFACE_GAP = 32;
/**
 * Maximum stacked height of one depth column before its cells wrap into a
 * side-by-side sub-column, board units — about four scaled phone frames. A
 * tab bar reaching six pages puts them all at depth 1; without wrapping that
 * single column stretches the map into a strip no zoom can take in at once.
 */
export const MAP_COLUMN_MAX_HEIGHT = 1500;
/** Horizontal gap between wrapped sub-columns inside one depth column. */
export const MAP_SUBCOLUMN_GAP = 48;

/**
 * BFS from the entry page over `navigate` edges between pages, discovering
 * neighbors in edge order. Returns pages in discovery order with their depth.
 * A missing or unknown entry falls back to the graph's first page node so the
 * map never degenerates to a single unreachable pile.
 */
const pageDepths = (
  pages: readonly string[],
  edges: MapGraph["edges"],
  entry: string | undefined,
): Map<string, number> => {
  const pageSet = new Set(pages);
  const neighbors = new Map<string, string[]>();
  for (const edge of edges) {
    if (edge.kind !== "navigate") continue;
    if (!pageSet.has(edge.from) || !pageSet.has(edge.to) || edge.from === edge.to) continue;
    const list = neighbors.get(edge.from) ?? [];
    neighbors.set(edge.from, [...list, edge.to]);
  }
  const start = entry !== undefined && pageSet.has(entry) ? entry : pages[0];
  const depths = new Map<string, number>();
  if (start === undefined) return depths;
  depths.set(start, 0);
  const queue = [start];
  for (let index = 0; index < queue.length; index += 1) {
    const page = queue[index]!;
    for (const next of neighbors.get(page) ?? []) {
      if (depths.has(next)) continue;
      depths.set(next, depths.get(page)! + 1);
      queue.push(next);
    }
  }
  return depths;
};

/**
 * The page each surface hangs below: the source of its first `present` edge
 * (in edge order) whose presenter is a page. Surfaces nothing presents — or
 * only other surfaces present — return undefined and fall to the last column.
 */
const surfaceOpeners = (
  surfaces: readonly string[],
  pages: readonly string[],
  edges: MapGraph["edges"],
): Map<string, string> => {
  const pageSet = new Set(pages);
  const surfaceSet = new Set(surfaces);
  const openers = new Map<string, string>();
  for (const edge of edges) {
    if (edge.kind !== "present") continue;
    if (!surfaceSet.has(edge.to) || !pageSet.has(edge.from)) continue;
    if (!openers.has(edge.to)) openers.set(edge.to, edge.from);
  }
  return openers;
};

/** One column cell: a page (or orphan surface) plus its attached surfaces. */
interface MapCell {
  head: string;
  surfaces: string[];
}

/**
 * Positions every page/surface node of the graph. Columns run left to right
 * by navigation depth from the entry (unreachable pages, then orphan
 * surfaces, in a final name-sorted column); within a column, cells stack in
 * BFS discovery order, wrapping into side-by-side sub-columns once the stack
 * would exceed MAP_COLUMN_MAX_HEIGHT (a cell — page plus its surfaces — is
 * atomic and never splits). Nodes are keyed by graph id; `sizeOf` supplies
 * each node's rendered footprint so columns clear their widest member.
 */
export const layoutInteractionMap = (
  graph: MapGraph,
  sizeOf: (nodeId: string) => MapNodeSize,
): Map<string, MapNodePosition> => {
  const pages = graph.nodes.filter((node) => node.kind === "page").map((node) => node.id);
  const surfaces = graph.nodes.filter((node) => node.kind === "surface").map((node) => node.id);
  const depths = pageDepths(pages, graph.edges, graph.entry);
  const openers = surfaceOpeners(surfaces, pages, graph.edges);

  const reachable = [...depths.keys()];
  const reachableDepth = reachable.reduce((max, page) => Math.max(max, depths.get(page)!), 0);
  const unreachablePages = pages.filter((page) => !depths.has(page)).sort();
  const orphanSurfaces = surfaces.filter((surface) => !openers.has(surface)).sort();
  const trailing = [...unreachablePages, ...orphanSurfaces];
  const trailingColumn = reachable.length > 0 ? reachableDepth + 1 : 0;

  const columnOf = (head: string): number => depths.get(head) ?? trailingColumn;
  const heads: string[] = [...reachable, ...trailing];

  const cells = new Map<string, MapCell>(
    heads.map((head) => [head, { head, surfaces: [] }]),
  );
  for (const surface of surfaces) {
    const opener = openers.get(surface);
    if (opener === undefined) continue;
    cells.get(opener)?.surfaces.push(surface);
  }

  const columns = new Map<number, MapCell[]>();
  for (const head of heads) {
    const column = columnOf(head);
    const list = columns.get(column) ?? [];
    columns.set(column, [...list, cells.get(head)!]);
  }

  const columnIndexes = [...columns.keys()].sort((left, right) => left - right);
  const positions = new Map<string, MapNodePosition>();
  let x = 0;
  for (const column of columnIndexes) {
    const columnCells = columns.get(column)!;
    let subX = x;
    let subWidth = 0;
    let y = 0;
    let rightEdge = x;
    for (const cell of columnCells) {
      const headSize = sizeOf(cell.head);
      const surfaceSizes = cell.surfaces.map((surface) => sizeOf(surface));
      const cellHeight = headSize.height
        + surfaceSizes.reduce((sum, size) => sum + MAP_SURFACE_GAP + size.height, 0);
      // Wrap BEFORE overflowing, never an empty sub-column: a first cell
      // taller than the cap still lands at the top of its sub-column.
      if (y > 0 && y + MAP_ROW_GAP + cellHeight > MAP_COLUMN_MAX_HEIGHT) {
        subX += subWidth + MAP_SUBCOLUMN_GAP;
        subWidth = 0;
        y = 0;
      } else if (y > 0) {
        y += MAP_ROW_GAP;
      }
      positions.set(cell.head, { x: subX, y, column });
      subWidth = Math.max(subWidth, headSize.width);
      y += headSize.height;
      for (const [surfaceIndex, surface] of cell.surfaces.entries()) {
        y += MAP_SURFACE_GAP;
        positions.set(surface, { x: subX, y, column });
        subWidth = Math.max(subWidth, surfaceSizes[surfaceIndex]!.width);
        y += surfaceSizes[surfaceIndex]!.height;
      }
      rightEdge = Math.max(rightEdge, subX + subWidth);
    }
    x = rightEdge + MAP_COLUMN_GAP;
  }
  return positions;
};

/**
 * Maps `page:`/`surface:` graph nodes to the first board preview of the same
 * definition — the frame the Map view shows for that node. Mirrors the
 * structure-connector frame resolution so every drawn edge lands on a map
 * node. Nodes without a preview are absent (the map shows a placeholder).
 */
export const mapNodePreviewIds = (
  nodes: MapGraph["nodes"],
  previews: ReadonlyArray<{
    id: string;
    identity: { kind: string; subject: string };
  }>,
): Map<string, string> => {
  const byNode = new Map<string, string>();
  for (const preview of previews) {
    const kind = preview.identity.kind;
    if (kind !== "page" && kind !== "surface") continue;
    const nodeId = `${kind}:${preview.identity.subject}`;
    if (!byNode.has(nodeId)) byNode.set(nodeId, preview.id);
  }
  const ids = new Map<string, string>();
  for (const node of nodes) {
    if (node.kind !== "page" && node.kind !== "surface") continue;
    const previewId = byNode.get(node.id);
    if (previewId !== undefined) ids.set(node.id, previewId);
  }
  return ids;
};
