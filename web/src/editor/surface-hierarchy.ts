import type { SurfaceView } from "../protocol/types.js";
import type { EditorPreview, JsonValue } from "./editor-state.js";

export interface MountedSurface {
  key: string;
  definition: string;
  modality: string;
  stackIndex: number;
  openedByDirectReplay: boolean;
}

export interface SurfaceHierarchy {
  page: string;
  surfaces: MountedSurface[];
}

const stringField = (value: JsonValue, field: string): string | null => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const candidate = value[field];
  return typeof candidate === "string" ? candidate : null;
};

const directlyOpenedSurfaceKeys = (preview: EditorPreview): ReadonlySet<string> =>
  new Set(preview.replay.flatMap((step) => step.effects.structural.flatMap((effect) =>
    stringField(effect, "op") === "open-surface"
      ? [stringField(effect, "surface")].filter((key): key is string => key !== null)
      : [])));

const mountedSurface = (
  surface: SurfaceView,
  stackIndex: number,
  directlyOpened: ReadonlySet<string>,
): MountedSurface => ({
  key: surface.key,
  definition: surface.definition,
  modality: surface.modality,
  stackIndex,
  openedByDirectReplay: directlyOpened.has(surface.key),
});

export const surfaceHierarchy = (preview: EditorPreview): SurfaceHierarchy | null => {
  if (!("protocol" in preview.content) || preview.content.protocol !== "uhura-view/0") return null;
  const directlyOpened = directlyOpenedSurfaceKeys(preview);
  return {
    page: preview.content.page.route,
    surfaces: preview.content.surfaces.map((surface, index) =>
      mountedSurface(surface, index, directlyOpened)),
  };
};

export const directlyOpenedSurfaces = (preview: EditorPreview): MountedSurface[] =>
  surfaceHierarchy(preview)?.surfaces.filter((surface) => surface.openedByDirectReplay) ?? [];
