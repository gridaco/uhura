import { createEditorAssets } from "../renderer/assets.js";
import type { IconFontRegistry } from "../renderer/icons.js";
import { createProjectionRenderer } from "../renderer/projection.js";
import {
  type EditorPreview,
  type EditorRender,
  type PreviewIdentity,
  semanticPreviewKey,
} from "./editor-state.js";
import { prepareAuthoring, type PreparedAuthoring } from "./editor-authoring.js";
import {
  RealizationResources,
  type RealizationOwner,
} from "./editor-realization.js";
import { preparePreviewStylesheet } from "./editor-styles.js";
import { surfaceHierarchy } from "./surface-hierarchy.js";
import {
  reusablePreviewFrameIds,
  reusablePreviewIds,
} from "./editor-updates.js";
import {
  buildWorkflowConnectors,
  type WorkflowConnector,
  workflowRailHeight,
  workflowConnectorDescription,
  workflowConnectorLabel,
} from "./workflow-connectors.js";
import {
  buildStructureConnectors,
  type StructureConnector,
  type StructureConnectorDirection,
  structureConnectorDescription,
} from "./structure-connectors.js";
import { structureConnectorLabelSegments } from "./structure-presentation.js";
import {
  editorIdentifierLabel,
  editorPreviewLabels,
  editorSubjectLabel,
  type PreviewDisplayLabels,
} from "./display-labels.js";

export interface PreparedWorkflowConnector extends WorkflowConnector {
  element: SVGGElement;
}

export interface PreparedStructureConnector extends StructureConnector {
  element: SVGGElement;
}

export interface PreparedEditorModel {
  board: HTMLElement;
  navigator: DocumentFragment;
  frameById: Map<string, HTMLElement>;
  shadowHostById: Map<string, HTMLElement>;
  resourcesByPreviewId: Map<string, RealizationResources>;
  resourceOwner: RealizationOwner;
  previewById: Map<string, EditorPreview>;
  previewIdByIdentity: Map<string, string>;
  authoring: PreparedAuthoring;
  connectorLayer: SVGSVGElement;
  connectors: PreparedWorkflowConnector[];
  structureConnectors: PreparedStructureConnector[];
  render: EditorRender | null;
  iconFingerprint: string | null;
  stylesheet: CSSStyleSheet | null;
  reusableRealizationIds: ReadonlySet<string>;
  reusableFrameIds: ReadonlySet<string>;
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

const svgElement = <K extends keyof SVGElementTagNameMap>(
  document: Document,
  tag: K,
  className?: string,
): SVGElementTagNameMap[K] => {
  const node = document.createElementNS("http://www.w3.org/2000/svg", tag);
  if (className) node.setAttribute("class", className);
  return node;
};

const prepareWorkflowConnector = (
  document: Document,
  connector: WorkflowConnector,
): PreparedWorkflowConnector => {
  const group = svgElement(document, "g", "workflow-connector");
  group.dataset.sourcePreviewId = connector.sourceId;
  group.dataset.targetPreviewId = connector.targetId;
  group.dataset.lane = String(connector.lane);
  group.dataset.sourcePort = `${connector.sourcePort.slot + 1}/${connector.sourcePort.count}`;
  group.dataset.targetPort = `${connector.targetPort.slot + 1}/${connector.targetPort.count}`;
  if (connector.introducedSurfaces.length > 0) {
    group.classList.add("opens-surface");
    group.dataset.introducedSurfaces = connector.introducedSurfaces
      .map((surface) => surface.definition)
      .join(" ");
  }

  const title = svgElement(document, "title");
  title.textContent = `Checked replay provenance via ${workflowConnectorDescription(connector)}`;
  const path = svgElement(document, "path", "workflow-connector-path");
  const arrow = svgElement(document, "path", "workflow-connector-arrow");
  const origin = svgElement(document, "circle", "workflow-connector-origin");
  origin.setAttribute("r", "3");
  const label = svgElement(document, "text", "workflow-connector-label");
  label.textContent = workflowConnectorLabel(connector.steps, connector.introducedSurfaces);
  group.append(title, path, arrow, origin, label);
  return { ...connector, element: group };
};

/**
 * Renders the pill text as segments: kind glyph + event at normal weight and
 * the far endpoint's name as a slightly bolder tspan. The text element keeps
 * its single-anchor counter-scaling behavior — only its children change.
 */
export const renderStructureConnectorLabel = (
  label: SVGTextElement,
  connector: Pick<
    StructureConnector,
    "kind" | "event" | "extraCount" | "sourceNode" | "targetNode"
  >,
  direction: StructureConnectorDirection = "outgoing",
): void => {
  const document = label.ownerDocument;
  const segments = structureConnectorLabelSegments(connector, direction);
  const lead = svgElement(document, "tspan");
  lead.textContent = segments.lead;
  const farName = svgElement(document, "tspan", "structure-label-far");
  farName.textContent = segments.farName;
  const children: SVGTSpanElement[] = [lead, farName];
  if (segments.suffix) {
    const suffix = svgElement(document, "tspan");
    suffix.textContent = segments.suffix;
    children.push(suffix);
  }
  label.replaceChildren(...children);
};

const prepareStructureConnector = (
  document: Document,
  connector: StructureConnector,
): PreparedStructureConnector => {
  const group = svgElement(
    document,
    "g",
    `structure-connector structure-${connector.kind}`,
  );
  group.dataset.sourcePreviewId = connector.sourceId;
  group.dataset.targetPreviewId = connector.targetId;
  group.dataset.sourceNode = connector.sourceNode;
  group.dataset.targetNode = connector.targetNode;
  group.dataset.structureKind = connector.kind;
  group.dataset.event = connector.event;

  const title = svgElement(document, "title");
  title.textContent = `App structure: ${structureConnectorDescription(connector)}`;
  const path = svgElement(document, "path", "workflow-connector-path");
  const arrow = svgElement(document, "path", "workflow-connector-arrow");
  const origin = svgElement(document, "circle", "workflow-connector-origin");
  origin.setAttribute("r", "3");
  // The label pill: a rounded rect sized to the text at layout time so the
  // event name stays readable over frames and connectors at any zoom. Rect
  // and text share one group so hover scaling lifts them together.
  const pill = svgElement(document, "g", "structure-connector-pill");
  const labelBackground = svgElement(document, "rect", "structure-connector-label-bg");
  const label = svgElement(document, "text", "workflow-connector-label");
  renderStructureConnectorLabel(label, connector);
  pill.append(labelBackground, label);
  group.append(title, path, arrow, origin, pill);
  return { ...connector, element: group };
};

const provenance = (preview: EditorPreview): string => {
  if (preview.pinned) return "Pinned example";
  if (preview.derived) return "Replay-derived";
  return "Checked example";
};

const realizePreview = (
  document: Document,
  preview: EditorPreview,
  render: EditorRender,
  stylesheet: CSSStyleSheet,
  host: HTMLElement,
  resources: RealizationResources,
  icons: IconFontRegistry | undefined,
): void => {
  const shadow = host.attachShadow({ mode: "open" });
  shadow.adoptedStyleSheets = [stylesheet];
  const application = element(document, "div", "preview-application");
  application.id = "uh-app";
  const wrapper = element(document, "div", preview.identity.kind === "page"
    ? "screen-root"
    : "preview-root");
  wrapper.inert = true;
  const renderer = createProjectionRenderer({
    root: wrapper,
    dispatch: () => undefined,
    mode: "editor",
    assets: createEditorAssets(render.assets),
    icons,
    modalSurfaces: false,
    observeElement: (key, realized) => resources.registerKey(key, realized),
  });
  renderer.render(preview.content.value.document);
  application.append(wrapper);
  shadow.append(application);
};

const badge = (
  document: Document,
  className: string,
  text: string,
): HTMLSpanElement => element(document, "span", `badge ${className}`, text);

interface PreparedFrame {
  frame: HTMLElement;
  shadowHost: HTMLElement;
}

const frame = (
  document: Document,
  preview: EditorPreview,
  labels: PreviewDisplayLabels,
  render: EditorRender,
  stylesheet: CSSStyleSheet,
  resources: RealizationResources,
  icons: IconFontRegistry | undefined,
  realize: boolean,
): PreparedFrame => {
  const figure = element(document, "figure", "editor-frame");
  figure.dataset.previewId = preview.id;
  figure.tabIndex = 0;
  figure.setAttribute("role", "button");
  figure.setAttribute("aria-pressed", "false");
  figure.setAttribute("aria-keyshortcuts", "Enter");

  const shellClass = preview.identity.kind === "page"
    ? "device"
    : preview.identity.kind === "surface"
      ? "sheet"
      : "component";
  const shell = element(document, "div", `preview-shell ${shellClass}`);
  const shadowHost = element(document, "div", "preview-shadow-host");
  shell.append(shadowHost);
  if (realize) {
    realizePreview(document, preview, render, stylesheet, shadowHost, resources, icons);
  }

  const caption = element(document, "figcaption");
  const captionId = `caption-${preview.id}`;
  caption.id = captionId;
  figure.setAttribute("aria-labelledby", captionId);
  caption.append(element(
    document,
    "span",
    "caption-title",
    labels.combined,
  ));
  if (preview.default) caption.append(badge(document, "badge-default", "default"));
  if (preview.pinned) caption.append(badge(document, "badge-pinned", "pinned"));
  if (preview.inFlight > 0) {
    caption.append(badge(document, "badge-in-flight", `${preview.inFlight} in flight`));
  }
  const hierarchy = surfaceHierarchy(preview, render.previews);
  if (hierarchy && hierarchy.surfaces.length > 0) {
    figure.dataset.surfaceCount = String(hierarchy.surfaces.length);
    for (const surface of hierarchy.surfaces) {
      const surfaceBadge = badge(
        document,
        "badge-surface",
        `${surface.modality} ${editorIdentifierLabel(surface.definition)}`,
      );
      surfaceBadge.dataset.relation = surface.relation;
      surfaceBadge.title = {
        introduced: "Present in this projection but absent from its evidence parent",
        retained: "Present in this projection and its evidence parent",
        present: "Present in this standalone projection",
      }[surface.relation];
      caption.append(surfaceBadge);
    }
  }
  caption.append(element(document, "span", "caption-prov", provenance(preview)));
  if (preview.note) caption.append(element(document, "p", "caption-note", preview.note));
  figure.append(shell, caption);
  return { frame: figure, shadowHost };
};

const navigatorGroup = (
  document: Document,
  group: EditorRender["groups"][number],
  previews: EditorPreview[],
): HTMLElement => {
  const peerIdentities = previews.map((preview) => preview.identity);
  const subjectLabel = editorSubjectLabel(group);
  const section = element(document, "section", "navigator-group");
  section.dataset.navigatorGroup = "";
  section.dataset.search = [
    group.kind,
    group.subject,
    subjectLabel,
  ].join(" ").toLocaleLowerCase();

  const row = element(document, "button", "navigator-row");
  row.type = "button";
  row.dataset.groupId = group.id;
  row.append(
    element(document, "span", "navigator-kind"),
    element(document, "span", "navigator-row-title", subjectLabel),
    element(document, "span", "navigator-count", String(previews.length)),
  );
  (row.firstElementChild as HTMLElement).dataset.kind = group.kind;

  const list = element(document, "div", "navigator-frames");
  for (const preview of previews) {
    const labels = editorPreviewLabels(preview.identity, peerIdentities);
    const button = element(document, "button", "navigator-frame");
    button.type = "button";
    button.dataset.previewId = preview.id;
    button.dataset.search = [
      preview.identity.kind,
      preview.identity.subject,
      preview.identity.example,
      labels.subject,
      labels.example,
    ].join(" ").toLocaleLowerCase();
    button.setAttribute("aria-pressed", "false");
    button.append(
      element(document, "span", "navigator-frame-icon"),
      element(document, "span", "navigator-frame-title", labels.example),
    );
    if (preview.derived) {
      const marker = element(document, "span", "navigator-derived", "D");
      marker.title = "Replay-derived";
      button.append(marker);
    } else if (preview.default) {
      const marker = element(document, "span", "navigator-default");
      marker.title = "Default preview";
      button.append(marker);
    }
    list.append(button);
  }
  section.append(row, list);
  return section;
};

// Shift+Y hides authored annotations plus replay (workflow) connectors, but
// structural arrows are selection state rather than annotation content. The
// connector layer therefore never leaves the render tree: CSS keyed off this
// class hides only `.workflow-connector` groups, so `.structure-connector
// .is-active` keeps drawing (and `getBBox` keeps measuring label pills) while
// annotations are hidden.
export const setAnnotationConnectorsHidden = (
  connectorLayer: SVGSVGElement,
  hidden: boolean,
): void => {
  connectorLayer.classList.toggle("annotations-hidden", hidden);
};

export const prepareEditorModel = (
  document: Document,
  render: EditorRender | null,
  previous: PreparedEditorModel | null = null,
  icons?: IconFontRegistry,
): PreparedEditorModel => {
  const board = element(document, "div", "editor-board");
  const navigator = document.createDocumentFragment();
  const frameById = new Map<string, HTMLElement>();
  const shadowHostById = new Map<string, HTMLElement>();
  const resourcesByPreviewId = new Map<string, RealizationResources>();
  const resourceOwner = {};
  const previewById = new Map<string, EditorPreview>();
  const previewIdByIdentity = new Map<string, string>();
  const authoring = prepareAuthoring(render);
  const connectorLayer = svgElement(document, "svg", "workflow-connectors");
  connectorLayer.setAttribute("aria-hidden", "true");
  const connectors: PreparedWorkflowConnector[] = [];
  const structureConnectors: PreparedStructureConnector[] = [];
  board.append(connectorLayer);

  if (!render) {
    const empty = element(document, "section", "empty-board");
    empty.append(
      element(document, "h2", undefined, "Waiting for a valid preview"),
      element(document, "p", undefined, "Fix the saved source errors. Editor will recover automatically."),
    );
    board.append(empty);
    return {
      board,
      navigator,
      frameById,
      shadowHostById,
      resourcesByPreviewId,
      resourceOwner,
      previewById,
      previewIdByIdentity,
      authoring,
      connectorLayer,
      connectors,
      structureConnectors,
      render,
      iconFingerprint: null,
      stylesheet: null,
      reusableRealizationIds: new Set(),
      reusableFrameIds: new Set(),
    };
  }

  const stylesheet = previous?.render?.stylesheet === render.stylesheet
    ? previous.stylesheet ?? preparePreviewStylesheet(document, render.stylesheet)
    : preparePreviewStylesheet(document, render.stylesheet);
  const iconFingerprint = icons?.fingerprint ?? null;
  const resourcesMatch = previous?.iconFingerprint === iconFingerprint;
  const reusableRealizationIds = new Set(resourcesMatch
    ? [...reusablePreviewIds(previous?.render ?? null, render)].filter((id) =>
      previous?.frameById.has(id) ?? false)
    : []);
  const reusableFrameIds = new Set(resourcesMatch
    ? [...reusablePreviewFrameIds(previous?.render ?? null, render)].filter((id) =>
      previous?.frameById.has(id) ?? false)
    : []);

  for (const preview of render.previews) {
    previewById.set(preview.id, preview);
    previewIdByIdentity.set(semanticPreviewKey(preview.identity), preview.id);
  }
  try {
    for (const group of render.groups) {
      const previews = group.previews.map((id) => previewById.get(id));
      if (previews.some((preview) => preview === undefined)) {
        // The decoder already enforces this. Keep the preparation boundary
        // independently total in case a typed caller bypasses JSON decoding.
        throw new Error(`Editor group ${group.id} refers to an unknown preview`);
      }
      const typedPreviews = previews as EditorPreview[];
      const groupConnectors = buildWorkflowConnectors(group.id, typedPreviews).map((connector) =>
        prepareWorkflowConnector(document, connector));
      connectors.push(...groupConnectors);
      connectorLayer.append(...groupConnectors.map((connector) => connector.element));
      const row = element(document, "section", "preview-row");
      row.dataset.groupId = group.id;
      row.append(element(
        document,
        "h2",
        "row-title",
        `${group.kind} ${editorSubjectLabel(group)}`,
      ));
      const frames = element(document, "div", "row-frames");
      const laneCount = groupConnectors.reduce(
        (count, connector) => Math.max(count, connector.lane + 1),
        0,
      );
      if (laneCount > 0) {
        frames.style.setProperty("--workflow-rail-height", `${workflowRailHeight(laneCount)}px`);
      }
      const peerIdentities = typedPreviews.map((preview) => preview.identity);
      for (const preview of typedPreviews) {
        const resources = new RealizationResources();
        resources.claim(resourceOwner);
        resourcesByPreviewId.set(preview.id, resources);
        const prepared = frame(
          document,
          preview,
          editorPreviewLabels(preview.identity, peerIdentities),
          render,
          stylesheet,
          resources,
          icons,
          !reusableRealizationIds.has(preview.id),
        );
        frameById.set(preview.id, prepared.frame);
        shadowHostById.set(preview.id, prepared.shadowHost);
        frames.append(prepared.frame);
      }
      row.append(frames);
      board.append(row);
      navigator.append(navigatorGroup(document, group, typedPreviews));
    }
    // Every structural candidate is prepared once but stays hidden: the
    // editor scopes them to the current selection (Figma prototype-arrow
    // behavior) and packs lanes over that visible subset only.
    structureConnectors.push(...buildStructureConnectors(
      render.interactionGraph,
      render.previews,
    ).map((connector) => prepareStructureConnector(document, connector)));
    connectorLayer.append(...structureConnectors.map((connector) => connector.element));
  } catch (error) {
    for (const resources of resourcesByPreviewId.values()) resources.release(resourceOwner);
    throw error;
  }

  return {
    board,
    navigator,
    frameById,
    shadowHostById,
    resourcesByPreviewId,
    resourceOwner,
    previewById,
    previewIdByIdentity,
    authoring,
    connectorLayer,
    connectors,
    structureConnectors,
    render,
    iconFingerprint,
    stylesheet,
    reusableRealizationIds,
    reusableFrameIds,
  };
};

interface FrameReplacement {
  id: string;
  previousFrame: HTMLElement;
  candidateFrame: HTMLElement;
  previousHost: HTMLElement;
  candidateHost: HTMLElement;
  shadow: ShadowRoot;
  moveWholeFrame: boolean;
  previousNode: HTMLElement;
  candidateNode: HTMLElement;
  previousParent: ParentNode;
  nextSibling: ChildNode | null;
  stylesheets: CSSStyleSheet[];
  previousResources: RealizationResources;
  candidateResources: RealizationResources;
}

/**
 * Moves safe, already-realized preview frames into a detached replacement.
 * Call only inside the update session's commit callback: preparation must not
 * detach anything from the currently visible board.
 */
export const reconcilePreparedEditorModel = (
  previous: PreparedEditorModel,
  next: PreparedEditorModel,
): void => {
  if (next.reusableRealizationIds.size === 0) return;
  if (!next.stylesheet) {
    throw new Error("A renderable Editor model must have a preview stylesheet");
  }

  // Validate the entire transplant before moving the first connected frame.
  const replacements: FrameReplacement[] = [];
  for (const id of next.reusableRealizationIds) {
    const previousFrame = previous.frameById.get(id);
    const candidateFrame = next.frameById.get(id);
    const previousHost = previous.shadowHostById.get(id);
    const candidateHost = next.shadowHostById.get(id);
    const shadow = previousHost?.shadowRoot;
    const moveWholeFrame = next.reusableFrameIds.has(id);
    const previousNode = moveWholeFrame ? previousFrame : previousHost;
    const candidateNode = moveWholeFrame ? candidateFrame : candidateHost;
    const previousParent = previousNode?.parentNode;
    const previousResources = previous.resourcesByPreviewId.get(id);
    const candidateResources = next.resourcesByPreviewId.get(id);
    if (
      !previousFrame
      || !candidateFrame
      || !previousHost
      || !candidateHost
      || !previousNode
      || !candidateNode
      || !shadow
      || !previousParent
      || !candidateNode.parentNode
      || !previousResources
      || !candidateResources
      || !previousResources.canTransfer(previous.resourceOwner)
    ) {
      throw new Error(`Reusable Editor preview ${id} has no realized frame`);
    }
    replacements.push({
      id,
      previousFrame,
      candidateFrame,
      previousHost,
      candidateHost,
      shadow,
      moveWholeFrame,
      previousNode,
      candidateNode,
      previousParent,
      nextSibling: previousNode.nextSibling,
      stylesheets: [...shadow.adoptedStyleSheets],
      previousResources,
      candidateResources,
    });
  }

  const moved: FrameReplacement[] = [];
  try {
    for (const replacement of replacements) {
      moved.push(replacement);
      replacement.candidateNode.replaceWith(replacement.previousNode);
      if (
        replacement.shadow.adoptedStyleSheets.length !== 1
        || replacement.shadow.adoptedStyleSheets[0] !== next.stylesheet
      ) {
        replacement.shadow.adoptedStyleSheets = [next.stylesheet];
      }
      if (replacement.moveWholeFrame) {
        next.frameById.set(replacement.id, replacement.previousFrame);
      }
      next.shadowHostById.set(replacement.id, replacement.previousHost);
    }
  } catch (error) {
    for (const replacement of moved.reverse()) {
      replacement.previousNode.replaceWith(replacement.candidateNode);
      const anchor = replacement.nextSibling?.parentNode === replacement.previousParent
        ? replacement.nextSibling
        : null;
      replacement.previousParent.insertBefore(replacement.previousNode, anchor);
      replacement.shadow.adoptedStyleSheets = replacement.stylesheets;
      if (replacement.moveWholeFrame) {
        next.frameById.set(replacement.id, replacement.candidateFrame);
      }
      next.shadowHostById.set(replacement.id, replacement.candidateHost);
    }
    throw error;
  }

  // Resource transfer is infallible after the validation above and happens
  // only after every DOM move succeeds, so rollback never revives a disposed
  // candidate registry.
  for (const replacement of replacements) {
    replacement.previousResources.transfer(previous.resourceOwner, next.resourceOwner);
    replacement.candidateResources.release(next.resourceOwner);
    next.resourcesByPreviewId.set(replacement.id, replacement.previousResources);
  }
};

export const watchPreparedEditorModel = (
  model: PreparedEditorModel,
  window: Window,
  invalidate: () => void,
): void => {
  for (const [previewId, resources] of model.resourcesByPreviewId) {
    const frame = model.frameById.get(previewId);
    if (!frame) throw new Error(`Realized preview ${previewId} has no frame`);
    resources.watch(model.resourceOwner, frame, window, invalidate);
  }
};

export const disposePreparedEditorModel = (model: PreparedEditorModel): void => {
  for (const resources of model.resourcesByPreviewId.values()) {
    resources.release(model.resourceOwner);
  }
};

export const previewIdForIdentity = (
  model: PreparedEditorModel,
  identity: PreviewIdentity | null,
): string | null => identity
  ? model.previewIdByIdentity.get(semanticPreviewKey(identity)) ?? null
  : null;
