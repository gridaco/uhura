import type { RenderNode } from "../renderer/projection.js";
import type { EditorPreview } from "./editor-state.js";

export interface MountedSurface {
  /** Stable semantic identity from the canonical projection. */
  key: string;
  /** Best available authored label, falling back to the semantic key. */
  definition: string;
  /** Honest rendered modality; canonical Surface currently projects to dialog. */
  modality: string;
  /** Deterministic pre-order among every mounted surface in this projection. */
  stackIndex: number;
  /** Comparison with the direct evidence parent, when one exists. */
  relation: "introduced" | "retained" | "present";
}

export interface SurfaceHierarchyNode {
  surface: MountedSurface;
  /** Nearest containing surface key, not an inferred runtime opener. */
  opener: string | null;
  children: SurfaceHierarchyNode[];
}

export interface SurfaceHierarchy {
  presentation: string;
  /** Deterministic pre-order over canonical `surface: true` nodes. */
  surfaces: MountedSurface[];
  /** Render-tree containment forest. */
  roots: SurfaceHierarchyNode[];
}

const textAttribute = (
  node: Extract<RenderNode, { kind: "element" }>,
  names: readonly string[],
): string | null => {
  for (const name of names) {
    const value = node.attributes.find((attribute) => attribute.name === name)?.value;
    if (typeof value === "string" && value.length > 0) return value;
  }
  return null;
};

const surfaceKeys = (nodes: readonly RenderNode[]): Set<string> => {
  const keys = new Set<string>();
  const visit = (node: RenderNode): void => {
    if (node.kind !== "element") return;
    if (node.surface) keys.add(node.key);
    node.children.forEach(visit);
  };
  nodes.forEach(visit);
  return keys;
};

const parentPreview = (
  preview: EditorPreview,
  relatedPreviews: readonly EditorPreview[],
): EditorPreview | null => {
  if (preview.from === null) return null;
  return relatedPreviews.find((candidate) =>
    candidate.identity.kind === preview.identity.kind
    && candidate.identity.subject === preview.identity.subject
    && candidate.identity.example === preview.from
  ) ?? null;
};

export const surfaceHierarchy = (
  preview: EditorPreview,
  relatedPreviews: readonly EditorPreview[] = [preview],
): SurfaceHierarchy | null => {
  const document = preview.content.value.document;
  const parent = parentPreview(preview, relatedPreviews);
  const retainedKeys = parent === null
    ? null
    : surfaceKeys(parent.content.value.document.nodes);
  const surfaces: MountedSurface[] = [];
  const roots: SurfaceHierarchyNode[] = [];

  const visit = (
    nodes: readonly RenderNode[],
    parent: SurfaceHierarchyNode | null,
  ): void => {
    for (const node of nodes) {
      if (node.kind !== "element") continue;
      let childParent = parent;
      if (node.surface) {
        const surface: MountedSurface = {
          key: node.key,
          definition: textAttribute(node, ["aria-label", "title", "name", "id"])
            ?? `Surface ${surfaces.length + 1}`,
          modality: textAttribute(node, ["data-modality", "role"]) ?? node.element,
          stackIndex: surfaces.length,
          relation: retainedKeys === null
            ? "present"
            : retainedKeys.has(node.key)
              ? "retained"
              : "introduced",
        };
        const hierarchyNode: SurfaceHierarchyNode = {
          surface,
          opener: parent?.surface.key ?? null,
          children: [],
        };
        surfaces.push(surface);
        if (parent === null) roots.push(hierarchyNode);
        else parent.children.push(hierarchyNode);
        childParent = hierarchyNode;
      }
      visit(node.children, childParent);
    }
  };

  visit(document.nodes, null);
  if (surfaces.length === 0) return null;
  return {
    presentation: document.presentation,
    surfaces,
    roots,
  };
};

/** Surfaces present in a derived projection but absent from its direct parent. */
export const introducedSurfaces = (
  preview: EditorPreview,
  relatedPreviews: readonly EditorPreview[] = [preview],
): MountedSurface[] =>
  surfaceHierarchy(preview, relatedPreviews)?.surfaces.filter(
    (surface) => surface.relation === "introduced",
  ) ?? [];
