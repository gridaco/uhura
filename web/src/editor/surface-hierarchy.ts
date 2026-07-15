import type { SurfaceView } from "../protocol/types.js";
import type { EditorPreview, JsonValue } from "./editor-state.js";

export interface MountedSurface {
  key: string;
  definition: string;
  modality: string;
  stackIndex: number;
  relation: "direct" | "inherited" | "mounted";
}

export interface SurfaceHierarchyNode {
  surface: MountedSurface;
  opener: string | null;
  children: SurfaceHierarchyNode[];
}

export interface SurfaceHierarchy {
  page: string;
  /** Bottom-to-top mounted surface order. */
  surfaces: MountedSurface[];
  /** Opener-derived parent/child forest rooted at the current page. */
  roots: SurfaceHierarchyNode[];
}

const stringField = (value: JsonValue, field: string): string | null => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const candidate = value[field];
  return typeof candidate === "string" ? candidate : null;
};

interface SurfaceOpen {
  surface: string;
  opener: string | null;
}

const openedSurfaces = (preview: EditorPreview): SurfaceOpen[] =>
  preview.replay.flatMap((step) => step.effects.structural.flatMap((effect) => {
    if (stringField(effect, "op") !== "open-surface") return [];
    const surface = stringField(effect, "surface");
    const opener = stringField(effect, "opener");
    return surface === null ? [] : [{ surface, opener }];
  }));

const previewLineage = (
  preview: EditorPreview,
  relatedPreviews: readonly EditorPreview[],
): EditorPreview[] => {
  const byExample = new Map(
    relatedPreviews
      .filter((candidate) =>
        candidate.identity.kind === preview.identity.kind
        && candidate.identity.subject === preview.identity.subject)
      .map((candidate) => [candidate.identity.example, candidate] as const),
  );
  byExample.set(preview.identity.example, preview);

  const lineage: EditorPreview[] = [];
  const visited = new Set<string>();
  let current: EditorPreview | undefined = preview;
  while (current && !visited.has(current.id)) {
    lineage.unshift(current);
    visited.add(current.id);
    current = current.from === null ? undefined : byExample.get(current.from);
  }
  return lineage;
};

const inheritedOpeners = (
  preview: EditorPreview,
  relatedPreviews: readonly EditorPreview[],
): Map<string, string> => {
  const openerBySurface = new Map<string, string>();
  for (const ancestor of previewLineage(preview, relatedPreviews)) {
    for (const step of ancestor.replay) {
      for (const effect of step.effects.structural) {
        const op = stringField(effect, "op");
        const surface = stringField(effect, "surface");
        if (op === "open-surface") {
          const opener = stringField(effect, "opener");
          if (surface !== null && opener !== null) openerBySurface.set(surface, opener);
        } else if ((op === "dismiss" || op === "force-close") && surface !== null) {
          openerBySurface.delete(surface);
        }
      }
    }
  }
  return openerBySurface;
};

const mountedSurface = (
  surface: SurfaceView,
  stackIndex: number,
  directlyOpened: ReadonlySet<string>,
  hasReplayParent: boolean,
): MountedSurface => ({
  key: surface.key,
  definition: surface.definition,
  modality: surface.modality,
  stackIndex,
  relation: directlyOpened.has(surface.key)
    ? "direct"
    : hasReplayParent
      ? "inherited"
      : "mounted",
});

export const surfaceHierarchy = (
  preview: EditorPreview,
  relatedPreviews: readonly EditorPreview[] = [preview],
): SurfaceHierarchy | null => {
  if (!("protocol" in preview.content) || preview.content.protocol !== "uhura-view/0") return null;
  const directlyOpened = new Set(openedSurfaces(preview).map(({ surface }) => surface));
  const surfaces = preview.content.surfaces.map((surface, index) =>
    mountedSurface(surface, index, directlyOpened, preview.from !== null));
  const nodeByKey = new Map<string, SurfaceHierarchyNode>();
  const nodeByScope = new Map<string, SurfaceHierarchyNode>();
  for (const [index, surface] of preview.content.surfaces.entries()) {
    const node: SurfaceHierarchyNode = {
      surface: surfaces[index]!,
      opener: null,
      children: [],
    };
    nodeByKey.set(surface.key, node);
    nodeByScope.set(surface.dismiss.scope, node);
  }

  const openerBySurface = inheritedOpeners(preview, relatedPreviews);
  const roots: SurfaceHierarchyNode[] = [];
  for (const surface of surfaces) {
    const node = nodeByKey.get(surface.key)!;
    node.opener = openerBySurface.get(surface.key) ?? null;
    const parent = node.opener === null ? undefined : nodeByScope.get(node.opener);
    if (parent && parent.surface.stackIndex < surface.stackIndex) parent.children.push(node);
    else roots.push(node);
  }
  return {
    page: preview.content.page.route,
    surfaces,
    roots,
  };
};

export const directlyOpenedSurfaces = (preview: EditorPreview): MountedSurface[] =>
  surfaceHierarchy(preview)?.surfaces.filter((surface) => surface.relation === "direct") ?? [];
