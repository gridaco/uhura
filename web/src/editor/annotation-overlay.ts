import type {
  EditorPreview,
  EditorRender,
  SourceMetadataEntry,
  SourceTarget,
} from "./editor-state.js";
import {
  documentationForPreview,
  memberDocumentationForPreview,
  annotationRenderStatus,
  presentedSourceGroups,
  renderedOccurrences,
  sourceActionsEnabled,
  sourceLocation,
  type AnnotationTarget,
  type PreparedAuthoring,
  type PreviewOccurrence,
} from "./editor-authoring.js";
import {
  realizationKey,
  type RealizationResources,
} from "./editor-realization.js";
import {
  clipAnnotationRect,
  placeAnnotations,
  unionAnnotationRects,
  type AnnotationPlacement,
  type AnnotationRect,
} from "./annotation-layout.js";

interface OverlayMarkerRecord {
  id: string;
  annotation: AnnotationTarget;
  occurrence: PreviewOccurrence;
  marker: HTMLButtonElement;
  line: SVGLineElement;
  highlight: SVGRectElement;
}

interface OverlayRecord {
  annotation: AnnotationTarget;
  markers: readonly OverlayMarkerRecord[];
  card: HTMLElement;
}

export interface AnnotationOverlayInstall {
  render: EditorRender | null;
  authoring: PreparedAuthoring;
  resourcesByPreviewId: ReadonlyMap<string, RealizationResources>;
}

export interface AnnotationOverlayOptions {
  viewport: HTMLElement;
  root: HTMLElement;
  chrome?: readonly HTMLElement[];
  focusPreview(previewId: string, anchors?: readonly HTMLElement[]): void;
  focusSourceTarget?(targetId: string): void;
}

const element = <K extends keyof HTMLElementTagNameMap>(
  document: Document,
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
};

const clientRect = (rect: DOMRect): AnnotationRect => ({
  left: rect.left,
  top: rect.top,
  width: rect.width,
  height: rect.height,
});

const localRect = (rect: AnnotationRect, viewport: DOMRect): AnnotationRect => ({
  left: rect.left - viewport.left,
  top: rect.top - viewport.top,
  width: rect.width,
  height: rect.height,
});

const isClipping = (style: CSSStyleDeclaration): boolean => {
  const values = `${style.overflow} ${style.overflowX} ${style.overflowY}`;
  return /(?:auto|scroll|hidden|clip)/.test(values);
};

export const composedParent = (node: Node): Node | null => {
  if (node.nodeType === 11 && "host" in node) {
    return (node as ShadowRoot).host;
  }
  const parent = node.parentNode;
  if (
    parent
    && parent.nodeType === 11
    && "host" in parent
  ) {
    return (parent as ShadowRoot).host;
  }
  return parent;
};

const unresolvedAnchorError = (
  targetId: string,
  occurrence: PreviewOccurrence,
  anchorIndex: number,
  key: string,
  missingResources: boolean,
): Error => new Error(
  `Uhura Editor internal error: annotation target ${JSON.stringify(targetId)} occurrence ${JSON.stringify(occurrence.occurrence.id)} in preview ${JSON.stringify(occurrence.previewId)} references anchor ${anchorIndex} (${key}), but ${missingResources ? "the preview has no direct realization resources" : "the direct renderer did not register that semantic node"}`,
);

/** Confirms wire-level semantic anchors have direct renderer realizations. */
export const validateAnnotationRealizations = (
  install: AnnotationOverlayInstall,
): void => {
  for (const [targetId, occurrences] of install.authoring.occurrencesByTarget) {
    for (const occurrence of occurrences) {
      const resources = install.resourcesByPreviewId.get(occurrence.previewId);
      for (const [anchorIndex, anchor] of occurrence.occurrence.anchors.entries()) {
        const key = realizationKey(anchor);
        if (!resources) {
          throw unresolvedAnchorError(targetId, occurrence, anchorIndex, key, true);
        }
        if (!resources.resolve(anchor)) {
          throw unresolvedAnchorError(targetId, occurrence, anchorIndex, key, false);
        }
      }
    }
  }
};

const isMeasuredElement = (node: Node): node is HTMLElement =>
  node.nodeType === 1
  && "getBoundingClientRect" in node
  && "style" in node;

const visibleElementRect = (
  target: HTMLElement,
  viewport: HTMLElement,
  window: Window,
): AnnotationRect | null => {
  if (!target.isConnected) return null;
  const viewportRect = clientRect(viewport.getBoundingClientRect());
  const clips: AnnotationRect[] = [viewportRect];
  let ancestor: Node | null = composedParent(target);
  while (ancestor && ancestor !== viewport) {
    if (isMeasuredElement(ancestor) && isClipping(window.getComputedStyle(ancestor))) {
      clips.push(clientRect(ancestor.getBoundingClientRect()));
    }
    ancestor = composedParent(ancestor);
  }
  const clipped = clipAnnotationRect(clientRect(target.getBoundingClientRect()), clips);
  return clipped ? localRect(clipped, viewport.getBoundingClientRect()) : null;
};

const entryNode = (document: Document, entry: SourceMetadataEntry): HTMLElement => {
  const item = element(document, "li", "annotation-entry");
  item.append(
    element(document, "span", "annotation-kind", `@${entry.kind}`),
    element(document, "p", "annotation-text", entry.text),
  );
  return item;
};

const sourceAction = (
  document: Document,
  target: SourceTarget,
  stale: boolean,
): HTMLButtonElement => {
  const button = element(document, "button", "source-location", sourceLocation(target));
  button.type = "button";
  button.disabled = stale;
  button.setAttribute("aria-label", `Copy source location ${sourceLocation(target)}`);
  button.title = stale
    ? "Source navigation is disabled for a stale render"
    : "Copy source location";
  button.addEventListener("click", () => {
    void document.defaultView?.navigator.clipboard?.writeText(sourceLocation(target));
  });
  return button;
};

const sourceTargetAction = (
  document: Document,
  target: SourceTarget,
  occurrences: readonly PreviewOccurrence[],
  selectTarget: ((targetId: string) => void) | undefined,
): HTMLButtonElement => {
  const button = element(document, "button", "source-target-select", "Show");
  button.type = "button";
  button.setAttribute("data-source-target-id", target.id);
  button.setAttribute("aria-label", `Show ${target.label} annotation on canvas`);
  button.title = "Show annotation on canvas";
  button.disabled = !selectTarget
    || !occurrences.some((item) => item.occurrence.anchors.length > 0);
  button.addEventListener("click", () => selectTarget?.(target.id));
  return button;
};

export const renderPreviewDocumentation = (
  container: HTMLElement,
  authoring: PreparedAuthoring,
  preview: EditorPreview,
  stale: boolean,
): void => {
  const document = container.ownerDocument;
  const docs = documentationForPreview(authoring, preview);
  const annotations = authoring.annotationTargets.filter((annotation) =>
    annotation.occurrences.some((item) => item.previewId === preview.id)
  );
  const nodes: HTMLElement[] = [];
  for (const [label, entry] of [
    ["Declaration", docs.declaration],
    ["Example", docs.example],
  ] as const) {
    if (!entry) continue;
    const target = authoring.targetsById.get(entry.targetId);
    const section = element(document, "section", "source-entry source-entry-doc");
    section.append(
      element(document, "h4", undefined, `${label} documentation`),
      element(document, "p", "source-doc-text", entry.text),
    );
    if (target) section.append(sourceAction(document, target, stale));
    nodes.push(section);
  }
  for (const member of memberDocumentationForPreview(authoring, preview)) {
    const section = element(document, "section", "source-entry source-entry-doc source-entry-member-doc");
    section.append(element(document, "h4", undefined, `${member.target.label} documentation`));
    for (const entry of member.entries) {
      section.append(element(document, "p", "source-doc-text", entry.text));
    }
    section.append(sourceAction(document, member.target, stale));
    nodes.push(section);
  }
  for (const annotation of annotations) {
    const section = element(document, "section", "source-entry source-entry-annotation");
    const list = element(document, "ol", "annotation-entry-list");
    list.setAttribute("aria-label", "Annotations in source order");
    list.append(...annotation.entries.map((entry) => entryNode(document, entry)));
    section.append(
      element(document, "h4", undefined, annotation.target.label),
      list,
      sourceAction(document, annotation.target, stale),
    );
    nodes.push(section);
  }
  container.replaceChildren(...nodes);
  container.hidden = nodes.length === 0;
};

export const renderSourcePanel = (
  container: HTMLElement,
  authoring: PreparedAuthoring,
  stale: boolean,
  selectTarget?: (targetId: string) => void,
): void => {
  const document = container.ownerDocument;
  const sections: HTMLElement[] = [];
  for (const group of presentedSourceGroups(authoring)) {
    const groupSection = element(document, "section", "source-owner-group");
    const ownerHeading = element(document, "header", "source-owner-heading");
    ownerHeading.append(
      element(document, "span", "source-owner-kind", group.owner.kind),
      element(document, "h2", undefined, group.owner.name),
    );
    groupSection.append(ownerHeading);
    for (const target of group.targets) {
      const targetId = target.id;
      const entries = authoring.entriesByTarget.get(targetId) ?? [];
      const occurrences = authoring.occurrencesByTarget.get(targetId) ?? [];
      const section = element(document, "section", "source-entry");
      section.setAttribute("data-source-target-id", targetId);
      const heading = element(document, "div", "source-entry-heading");
      const actions = element(document, "div", "source-entry-actions");
      const annotations = entries.filter((entry) => entry.class === "annotation");
      if (annotations.length > 0) {
        actions.append(sourceTargetAction(document, target, occurrences, selectTarget));
      }
      actions.append(sourceAction(document, target, stale));
      heading.append(element(document, "h3", undefined, target.label), actions);
      section.append(heading);
      const docs = entries.filter((entry) => entry.class === "doc");
      for (const doc of docs) {
        section.append(element(document, "p", "source-doc-text", doc.text));
      }
      if (annotations.length > 0) {
        const list = element(document, "ol", "annotation-entry-list");
        list.append(...annotations.map((entry) => entryNode(document, entry)));
        section.append(list, element(
          document,
          "p",
          "source-render-status",
          annotationRenderStatus(occurrences),
        ));
      }
      groupSection.append(section);
    }
    sections.push(groupSection);
  }
  if (sections.length === 0) {
    sections.push(element(document, "p", "inspector-muted", "No authored documentation or annotations."));
  }
  container.replaceChildren(...sections);
  container.classList.toggle("is-stale", stale);
};

export class AnnotationOverlay {
  readonly #viewport: HTMLElement;
  readonly #root: HTMLElement;
  readonly #document: Document;
  readonly #window: Window;
  readonly #focusPreview: (
    previewId: string,
    anchors?: readonly HTMLElement[],
  ) => void;
  readonly #focusSourceTarget: ((targetId: string) => void) | undefined;
  readonly #chrome: readonly HTMLElement[];
  #canvasVisible = true;
  #activePreviewId: string | null = null;
  #activeMarkerId: string | null = null;
  #revealedTargetId: string | null = null;
  #pendingFocusTargetId: string | null = null;
  #install: AnnotationOverlayInstall = {
    render: null,
    authoring: {
      targetsById: new Map(),
      entriesById: new Map(),
      entriesByTarget: new Map(),
      occurrencesByTarget: new Map(),
      annotationTargets: [],
      documentedTargets: [],
    },
    resourcesByPreviewId: new Map(),
  };
  #records: OverlayRecord[] = [];
  #placementOrder: OverlayMarkerRecord[] = [];
  #placementsById = new Map<string, AnnotationPlacement>();
  #frame = 0;
  #disposed = false;
  readonly #onViewportScroll = (): void => {
    this.#pinToViewport();
    this.invalidate();
  };

  constructor(options: AnnotationOverlayOptions) {
    this.#viewport = options.viewport;
    this.#root = options.root;
    this.#document = this.#root.ownerDocument;
    const window = this.#document.defaultView;
    if (!window) throw new Error("annotation overlay requires a browser window");
    this.#window = window;
    this.#focusPreview = options.focusPreview;
    this.#focusSourceTarget = options.focusSourceTarget;
    this.#chrome = options.chrome ?? [];
    this.#pinToViewport();
    this.#viewport.addEventListener("scroll", this.#onViewportScroll, { passive: true });
  }

  install(install: AnnotationOverlayInstall): void {
    validateAnnotationRealizations(install);
    this.#install = install;
    this.#records = [];
    this.#placementOrder = [];
    this.#activeMarkerId = null;
    this.#revealedTargetId = null;
    this.#pendingFocusTargetId = null;
    this.#placementsById.clear();
    const svg = this.#document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.classList.add("annotation-leaders");
    svg.setAttribute("aria-hidden", "true");
    const controls = element(this.#document, "div", "annotation-controls");
    const stale = !sourceActionsEnabled(install.render);
    this.#root.classList.toggle("is-stale", stale);

    for (const [index, annotation] of install.authoring.annotationTargets.entries()) {
      const realized = renderedOccurrences(annotation);
      const realizationCount = realized.length;
      const label = String(index + 1);
      const cardId = `annotation-card-${index + 1}`;
      const card = element(this.#document, "article", "annotation-card");
      card.id = cardId;
      card.tabIndex = -1;
      card.hidden = true;
      card.setAttribute("data-source-target-id", annotation.target.id);
      const header = element(this.#document, "header", "annotation-card-heading");
      header.append(
        element(this.#document, "strong", undefined, annotation.target.label),
        sourceAction(this.#document, annotation.target, stale),
      );
      const list = element(this.#document, "ol", "annotation-entry-list");
      list.append(...annotation.entries.map((entry) => entryNode(this.#document, entry)));
      card.append(header, list);
      const markers: OverlayMarkerRecord[] = [];
      let realizationIndex = 0;
      for (const occurrence of realized) {
        realizationIndex += 1;
        const id = [
          annotation.target.id,
          occurrence.previewId,
          occurrence.occurrence.id,
        ].join("\u0000");
        const marker = element(this.#document, "button", "annotation-marker", label);
        marker.type = "button";
        marker.id = `annotation-marker-${index + 1}-${realizationIndex}`;
        marker.setAttribute("aria-controls", cardId);
        marker.setAttribute("aria-expanded", "false");
        marker.setAttribute(
          "aria-label",
          `${annotation.entries.map((entry) => `@${entry.kind}`).join(", ")} on ${annotation.target.label}; rendered instance ${realizationIndex} of ${realizationCount}`,
        );
        for (const [name, value] of [
          ["data-source-target-id", annotation.target.id],
          ["data-preview-id", occurrence.previewId],
          ["data-occurrence-id", occurrence.occurrence.id],
        ] as const) {
          marker.setAttribute(name, value);
        }
        const highlight = this.#document.createElementNS("http://www.w3.org/2000/svg", "rect");
        highlight.classList.add("annotation-highlight");
        highlight.setAttribute("rx", "4");
        const line = this.#document.createElementNS("http://www.w3.org/2000/svg", "line");
        for (const node of [highlight, line]) {
          node.setAttribute("data-source-target-id", annotation.target.id);
          node.setAttribute("data-preview-id", occurrence.previewId);
          node.setAttribute("data-occurrence-id", occurrence.occurrence.id);
        }
        const markerRecord: OverlayMarkerRecord = {
          id,
          annotation,
          occurrence,
          marker,
          line,
          highlight,
        };
        marker.addEventListener("focus", () => this.#focusMarker(markerRecord));
        marker.addEventListener("click", () => {
          this.#focusMarker(markerRecord);
          card.focus({ preventScroll: true });
        });
        svg.append(highlight, line);
        controls.append(marker);
        markers.push(markerRecord);
      }
      controls.append(card);
      this.#records.push({ annotation, markers, card });
    }
    this.#syncStateClasses();
    this.#stopViewportGestures(controls);
    this.#root.replaceChildren(svg, controls);
    this.invalidate();
  }

  /** Shows or hides all canvas annotation affordances. */
  setCanvasVisible(visible: boolean): void {
    if (this.#canvasVisible === visible) return;
    this.#canvasVisible = visible;
    if (!visible) {
      for (const record of this.#records) {
        record.card.hidden = true;
        record.card.classList.toggle("is-revealed", false);
        for (const marker of record.markers) this.#setMarkerVisibility(marker, false, false);
      }
    }
    this.#syncStateClasses();
    this.invalidate();
  }

  /** Toggles all canvas annotation affordances. They are on by default. */
  toggleCanvasVisibility(): void {
    this.setCanvasVisible(!this.#canvasVisible);
  }

  /** Dismisses annotation cards while retaining their canvas markers. */
  dismissCards(): void {
    this.#revealedTargetId = null;
    this.#activeMarkerId = null;
    this.#pendingFocusTargetId = null;
    for (const record of this.#records) {
      record.card.hidden = true;
      record.card.classList.toggle("is-revealed", false);
      for (const marker of record.markers) marker.line.style.display = "none";
    }
    this.#syncStateClasses();
    this.invalidate();
  }

  /** Selects a Source target and reveals its selected-preview or first realization. */
  selectSourceTarget(targetId: string): boolean {
    const record = this.#records.find((candidate) => candidate.annotation.target.id === targetId);
    if (!record || record.markers.length === 0) return false;
    this.setCanvasVisible(true);
    const selected = record.markers.filter((marker) =>
      marker.occurrence.previewId === this.#activePreviewId
    );
    const marker = selected.find((candidate) => !candidate.marker.hidden)
      ?? selected[0]
      ?? record.markers.find((candidate) => !candidate.marker.hidden)
      ?? record.markers[0];
    if (!marker) return false;
    for (const candidate of this.#records) {
      candidate.card.hidden = candidate !== record;
      candidate.card.classList.toggle("is-revealed", candidate === record);
      for (const realization of candidate.markers) realization.line.style.display = "none";
    }
    this.#activeMarkerId = marker.id;
    this.#revealedTargetId = targetId;
    this.#pendingFocusTargetId = targetId;
    record.card.hidden = false;
    record.card.classList.toggle("is-revealed", true);
    this.#syncStateClasses();
    this.#focusOccurrence(marker);
    this.invalidate();
    return true;
  }

  invalidate = (): void => {
    if (this.#disposed || this.#frame) return;
    this.#frame = this.#window.requestAnimationFrame(() => {
      this.#frame = 0;
      this.#layout();
    });
  };

  /** Marks selected-preview badges without changing which realizations are presented. */
  activatePreviewOccurrences(previewId: string | null): void {
    this.#activePreviewId = previewId;
    this.#syncStateClasses();
    this.invalidate();
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#viewport.removeEventListener("scroll", this.#onViewportScroll);
    if (this.#frame) this.#window.cancelAnimationFrame(this.#frame);
    this.#root.replaceChildren();
    this.#root.style.transform = "";
    this.#records = [];
    this.#placementOrder = [];
    this.#placementsById.clear();
  }

  #layout(): void {
    this.#pinToViewport();
    const viewportRect = this.#viewport.getBoundingClientRect();
    const visible: Array<{
      record: OverlayRecord;
      marker: OverlayMarkerRecord;
      anchor: AnnotationRect;
    }> = [];
    for (const record of this.#records) {
      const cardRequested = this.#canvasVisible
        && this.#revealedTargetId === record.annotation.target.id
        && record.markers.some((marker) => marker.id === this.#activeMarkerId);
      record.card.hidden = !cardRequested;
      record.card.classList.toggle("is-revealed", cardRequested);
      const visibleKinds = record.annotation.entries.map((entry) => `@${entry.kind}`);
      for (const [markerIndex, marker] of record.markers.entries()) {
        marker.marker.setAttribute(
          "aria-label",
          `${visibleKinds.join(", ")} on ${record.annotation.target.label}; rendered instance ${markerIndex + 1} of ${record.markers.length}`,
        );
        if (!this.#canvasVisible) {
          this.#setMarkerVisibility(marker, false, false);
          continue;
        }
        const resources = this.#install.resourcesByPreviewId.get(marker.occurrence.previewId);
        const rects = marker.occurrence.occurrence.anchors.flatMap((anchor, anchorIndex) => {
          const target = resources?.resolve(anchor);
          if (!target) {
            throw unresolvedAnchorError(
              record.annotation.target.id,
              marker.occurrence,
              anchorIndex,
              realizationKey(anchor),
              !resources,
            );
          }
          const rect = visibleElementRect(target, this.#viewport, this.#window);
          return rect ? [rect] : [];
        });
        const anchor = unionAnnotationRects(rects);
        const cardVisible = Boolean(anchor)
          && this.#revealedTargetId === record.annotation.target.id
          && this.#activeMarkerId === marker.id;
        this.#setMarkerVisibility(marker, Boolean(anchor), cardVisible);
        if (anchor) visible.push({ record, marker, anchor });
      }
    }
    const placements = placeAnnotations(
      visible.map(({ record, marker, anchor }) => ({
        id: marker.id,
        sourceOrder: record.annotation.sourceOrder,
        anchor,
        card: {
          width: record.card.offsetWidth || 260,
          height: record.card.offsetHeight || 132,
        },
        showCard: this.#revealedTargetId === record.annotation.target.id
          && this.#activeMarkerId === marker.id,
      })),
      { left: 0, top: 0, width: viewportRect.width, height: viewportRect.height },
      12,
      10,
      this.#chrome
        .flatMap((item) => {
          if (item.hidden || !item.isConnected) return [];
          const rect = item.getBoundingClientRect();
          return rect.width > 0 && rect.height > 0
            ? [localRect(clientRect(rect), viewportRect)]
            : [];
        }),
      this.#placementsById,
    );
    this.#placementsById = new Map(
      placements.map((placement) => [placement.id, placement]),
    );
    const byId = new Map(placements.map((placement) => [placement.id, placement]));
    const markersById = new Map(
      this.#records.flatMap((record) => record.markers.map((marker) => [marker.id, marker] as const)),
    );
    this.#placementOrder = placements.flatMap((placement) => {
      const marker = markersById.get(placement.id);
      return marker ? [marker] : [];
    });
    const shownCards = new Set<OverlayRecord>();
    for (const { record, marker, anchor } of visible) {
      const placement = byId.get(marker.id);
      if (!placement) continue;
      marker.marker.style.transform = `translate(${placement.marker.x}px, ${placement.marker.y}px)`;
      marker.highlight.setAttribute("x", String(anchor.left));
      marker.highlight.setAttribute("y", String(anchor.top));
      marker.highlight.setAttribute("width", String(anchor.width));
      marker.highlight.setAttribute("height", String(anchor.height));
      const showsCard = this.#revealedTargetId === record.annotation.target.id
        && this.#activeMarkerId === marker.id;
      if (showsCard) {
        shownCards.add(record);
        record.card.hidden = false;
        record.card.classList.toggle("is-revealed", true);
        record.card.style.transform = `translate(${placement.card.left}px, ${placement.card.top}px)`;
        record.card.classList.toggle("is-gutter", placement.gutter);
        marker.line.setAttribute("x1", String(placement.leaderFrom.x));
        marker.line.setAttribute("y1", String(placement.leaderFrom.y));
        marker.line.setAttribute("x2", String(placement.leaderTo.x));
        marker.line.setAttribute("y2", String(placement.leaderTo.y));
      }
    }
    for (const record of this.#records) {
      if (shownCards.has(record)) continue;
      record.card.hidden = true;
      record.card.classList.toggle("is-revealed", false);
    }
    if (this.#pendingFocusTargetId) {
      const pending = this.#records.find(
        (record) => record.annotation.target.id === this.#pendingFocusTargetId,
      );
      if (pending && !pending.card.hidden) {
        this.#pendingFocusTargetId = null;
        pending.card.focus({ preventScroll: true });
      }
    }
  }

  #setMarkerVisibility(
    record: OverlayMarkerRecord,
    markerVisible: boolean,
    cardVisible: boolean,
  ): void {
    const previewActive = this.#activePreviewId !== null
      && record.occurrence.previewId === this.#activePreviewId;
    record.marker.hidden = !markerVisible;
    record.line.style.display = cardVisible ? "" : "none";
    record.highlight.style.display = markerVisible && previewActive ? "" : "none";
  }

  #syncStateClasses(): void {
    for (const record of this.#records) {
      for (const marker of record.markers) {
        const previewActive = this.#activePreviewId !== null
          && marker.occurrence.previewId === this.#activePreviewId;
        const active = marker.id === this.#activeMarkerId;
        for (const node of [marker.marker, marker.highlight, marker.line]) {
          node.classList.toggle("is-preview-active", previewActive);
          node.classList.toggle("is-active", active);
        }
        marker.highlight.style.display = this.#canvasVisible
          && !marker.marker.hidden
          && previewActive
          ? ""
          : "none";
        marker.marker.setAttribute(
          "aria-expanded",
          String(
            this.#canvasVisible
            && active
            && this.#revealedTargetId === record.annotation.target.id,
          ),
        );
      }
    }
  }

  #focusMarker(marker: OverlayMarkerRecord): void {
    const changed = this.#activeMarkerId !== marker.id
      || this.#revealedTargetId !== marker.annotation.target.id;
    this.#activeMarkerId = marker.id;
    this.#revealedTargetId = marker.annotation.target.id;
    this.#pendingFocusTargetId = null;
    for (const record of this.#records) {
      const revealed = record.annotation.target.id === marker.annotation.target.id;
      record.card.hidden = !revealed;
      record.card.classList.toggle("is-revealed", revealed);
      for (const candidate of record.markers) candidate.line.style.display = "none";
    }
    this.#syncStateClasses();
    if (changed) this.#focusSourceTarget?.(marker.annotation.target.id);
    this.invalidate();
  }

  #focusOccurrence(record: OverlayMarkerRecord): void {
    const resources = this.#install.resourcesByPreviewId.get(record.occurrence.previewId);
    const anchors = record.occurrence.occurrence.anchors.flatMap((anchor) => {
      const realized = resources?.resolve(anchor);
      return realized ? [realized] : [];
    });
    this.#focusPreview(record.occurrence.previewId, anchors);
  }

  #pinToViewport(): void {
    this.#root.style.transform = `translate(${this.#viewport.scrollLeft}px, ${this.#viewport.scrollTop}px)`;
  }

  #stopViewportGestures(controls: HTMLElement): void {
    const stop = (event: Event): void => event.stopPropagation();
    controls.addEventListener("pointerdown", stop);
    controls.addEventListener("click", stop);
    controls.addEventListener("keydown", (rawEvent) => {
      const event = rawEvent as KeyboardEvent;
      const markers = this.#placementOrder.filter((record) => !record.marker.hidden);
      const active = this.#document.activeElement;
      const current = markers.findIndex((record) =>
        record.marker === active
        || (record.id === this.#activeMarkerId && this.#records.some((target) =>
          target.annotation.target.id === record.annotation.target.id
          && (target.card === active || (active ? target.card.contains(active) : false))
        ))
      );
      if ((event.key === "ArrowDown" || event.key === "ArrowRight") && markers.length > 0) {
        markers[(current + 1 + markers.length) % markers.length]?.marker.focus();
        event.preventDefault();
      } else if ((event.key === "ArrowUp" || event.key === "ArrowLeft") && markers.length > 0) {
        markers[(current - 1 + markers.length) % markers.length]?.marker.focus();
        event.preventDefault();
      } else if (event.key === "Escape" && current >= 0) {
        const record = markers[current];
        if (record) this.#focusOccurrence(record);
      }
    });
  }
}
