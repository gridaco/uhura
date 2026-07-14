import type {
  EditorPreview,
  EditorRender,
  SourceMetadataEntry,
  SourceTarget,
  TargetOccurrence,
} from "./editor-state.js";
import { compareUtf8 } from "./editor-order.js";

export interface PreviewOccurrence {
  previewId: string;
  occurrence: TargetOccurrence;
}

export interface AnnotationTarget {
  target: SourceTarget;
  entries: readonly SourceMetadataEntry[];
  occurrences: readonly PreviewOccurrence[];
  sourceOrder: number;
}

export interface PreparedAuthoring {
  targetsById: ReadonlyMap<string, SourceTarget>;
  entriesById: ReadonlyMap<string, SourceMetadataEntry>;
  entriesByTarget: ReadonlyMap<string, readonly SourceMetadataEntry[]>;
  occurrencesByTarget: ReadonlyMap<string, readonly PreviewOccurrence[]>;
  annotationTargets: readonly AnnotationTarget[];
  documentedTargets: readonly SourceTarget[];
}

const byMetadataOrder = (
  left: SourceMetadataEntry,
  right: SourceMetadataEntry,
): number => left.order - right.order || left.span.offset - right.span.offset;

const bySourceOrder = (left: SourceTarget, right: SourceTarget): number =>
  compareUtf8(left.file, right.file)
  || left.span.offset - right.span.offset
  || compareUtf8(left.id, right.id);

/** Builds all presentation indexes from one render-owned metadata snapshot. */
export const prepareAuthoring = (render: EditorRender | null): PreparedAuthoring => {
  const targets = render?.authoring.targets ?? [];
  const entries = render?.authoring.entries ?? [];
  const targetsById = new Map(targets.map((target) => [target.id, target]));
  const entriesById = new Map(entries.map((entry) => [entry.id, entry]));
  const mutableEntries = new Map<string, SourceMetadataEntry[]>();
  for (const entry of entries) {
    const current = mutableEntries.get(entry.targetId) ?? [];
    current.push(entry);
    mutableEntries.set(entry.targetId, current);
  }
  const entriesByTarget = new Map<string, readonly SourceMetadataEntry[]>();
  for (const [targetId, targetEntries] of mutableEntries) {
    entriesByTarget.set(targetId, targetEntries.toSorted(byMetadataOrder));
  }

  const mutableOccurrences = new Map<string, PreviewOccurrence[]>();
  for (const preview of render?.previews ?? []) {
    for (const occurrence of preview.provenance.occurrences) {
      const current = mutableOccurrences.get(occurrence.targetId) ?? [];
      current.push({ previewId: preview.id, occurrence });
      mutableOccurrences.set(occurrence.targetId, current);
    }
  }
  const occurrencesByTarget = new Map<string, readonly PreviewOccurrence[]>();
  for (const [targetId, occurrences] of mutableOccurrences) {
    occurrencesByTarget.set(targetId, occurrences);
  }

  const orderedTargets = targets.toSorted(bySourceOrder);
  const annotationTargets = orderedTargets.flatMap((target, sourceOrder) => {
    const annotationEntries = (entriesByTarget.get(target.id) ?? [])
      .filter((entry) => entry.class === "annotation");
    if (annotationEntries.length === 0) return [];
    return [{
      target,
      entries: annotationEntries,
      occurrences: occurrencesByTarget.get(target.id) ?? [],
      sourceOrder,
    }];
  });
  const documentedTargets = orderedTargets.filter((target) =>
    (entriesByTarget.get(target.id) ?? []).some((entry) => entry.class === "doc")
  );

  return {
    targetsById,
    entriesById,
    entriesByTarget,
    occurrencesByTarget,
    annotationTargets,
    documentedTargets,
  };
};

export interface PreviewDocumentationEntries {
  declaration: SourceMetadataEntry | null;
  example: SourceMetadataEntry | null;
}

export const documentationForPreview = (
  authoring: PreparedAuthoring,
  preview: EditorPreview,
): PreviewDocumentationEntries => ({
  declaration: preview.documentation.declarationDocId
    ? authoring.entriesById.get(preview.documentation.declarationDocId) ?? null
    : null,
  example: preview.documentation.exampleDocId
    ? authoring.entriesById.get(preview.documentation.exampleDocId) ?? null
    : null,
});

export interface MemberDocumentation {
  target: SourceTarget;
  entries: readonly SourceMetadataEntry[];
}

/** Doc-bearing members owned by the selected declaration, in canonical source order. */
export const memberDocumentationForPreview = (
  authoring: PreparedAuthoring,
  preview: EditorPreview,
): readonly MemberDocumentation[] => {
  const selected = documentationForPreview(authoring, preview);
  const selectedTargetIds = new Set(
    [selected.declaration, selected.example]
      .flatMap((entry) => entry ? [entry.targetId] : []),
  );
  return authoring.documentedTargets.flatMap((target) => {
    if (
      selectedTargetIds.has(target.id)
      || target.owner.kind !== preview.identity.kind
      || target.owner.name !== preview.identity.subject
    ) {
      return [];
    }
    const entries = (authoring.entriesByTarget.get(target.id) ?? [])
      .filter((entry) => entry.class === "doc");
    return entries.length > 0 ? [{ target, entries }] : [];
  });
};

export const sourceLocation = (target: SourceTarget): string =>
  `${target.file}:${target.span.start.line}:${target.span.start.col}`;

export const presentedSourceTargets = (
  authoring: PreparedAuthoring,
): readonly SourceTarget[] => {
  const targetIds = new Set<string>([
    ...authoring.documentedTargets.map((target) => target.id),
    ...authoring.annotationTargets.map((annotation) => annotation.target.id),
  ]);
  return [...targetIds]
    .flatMap((targetId) => {
      const target = authoring.targetsById.get(targetId);
      return target ? [target] : [];
    })
    .toSorted(bySourceOrder);
};

export interface PresentedSourceGroup {
  owner: SourceTarget["owner"];
  targets: readonly SourceTarget[];
}

export const presentedSourceGroups = (
  authoring: PreparedAuthoring,
): readonly PresentedSourceGroup[] => {
  const groups = new Map<string, { owner: SourceTarget["owner"]; targets: SourceTarget[] }>();
  for (const target of presentedSourceTargets(authoring)) {
    const key = `${target.owner.kind}\u0000${target.owner.name}`;
    const group = groups.get(key) ?? { owner: target.owner, targets: [] };
    group.targets.push(target);
    groups.set(key, group);
  }
  return [...groups.values()];
};

export const annotationRenderStatus = (
  occurrences: readonly PreviewOccurrence[],
): string => {
  const anchored = occurrences.filter((item) => item.occurrence.anchors.length > 0).length;
  if (occurrences.length === 0 || anchored === 0) return "Not rendered in any preview";
  return anchored === occurrences.length
    ? `${anchored} rendered instance${anchored === 1 ? "" : "s"}`
    : `${anchored} of ${occurrences.length} rendered`;
};

export const renderedOccurrences = (
  annotation: AnnotationTarget,
): readonly PreviewOccurrence[] => annotation.occurrences.filter(
  (item) => item.occurrence.anchors.length > 0,
);

export const sourceActionsEnabled = (render: EditorRender | null): boolean =>
  render?.freshness === "current";
