import { createEditorRenderer } from "../renderer/editor.js";
import type { Snapshot, VNode } from "../protocol/types.js";
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

export interface PreparedWorkflowConnector extends WorkflowConnector {
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
  render: EditorRender | null;
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
  if (connector.openedSurfaces.length > 0) {
    group.classList.add("opens-surface");
    group.dataset.openedSurfaces = connector.openedSurfaces
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
  label.textContent = workflowConnectorLabel(connector.steps, connector.openedSurfaces);
  group.append(title, path, arrow, origin, label);
  return { ...connector, element: group };
};

const isSnapshot = (content: Snapshot | VNode): content is Snapshot =>
  "protocol" in content && content.protocol === "uhura-view/0";

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
): void => {
  const shadow = host.attachShadow({ mode: "open" });
  shadow.adoptedStyleSheets = [stylesheet];
  const application = element(document, "div", "preview-application");
  application.id = "uh-app";
  const wrapper = element(document, "div", isSnapshot(preview.content)
    ? "screen-root"
    : "fragment-root");
  wrapper.inert = true;
  const renderer = createEditorRenderer({
    document,
    icons: render.icons,
    assets: render.assets,
  });
  if (isSnapshot(preview.content)) {
    renderer.realizeRoot(wrapper, preview.content.page.root, {
      root: { kind: "page" },
      scope: `${preview.id}:page`,
      parentIsList: false,
      observe: (realization) => resources.register(realization),
    });
    for (const [index, surface] of preview.content.surfaces.entries()) {
      const overlay = element(document, "div", "uh-surface-overlay");
      overlay.dataset.surfaceDefinition = surface.definition;
      overlay.dataset.surfaceModality = surface.modality;
      overlay.dataset.surfaceStackIndex = String(index);
      overlay.style.zIndex = String(index + 1);
      const scrim = element(document, "div", "uh-scrim");
      const surfaceHost = element(
        document,
        "div",
        `uh-surface uh-modality-${surface.modality}`,
      );
      surfaceHost.setAttribute("role", "dialog");
      surfaceHost.setAttribute("aria-modal", "true");
      surfaceHost.dataset.surfaceDefinition = surface.definition;
      renderer.realizeRoot(surfaceHost, surface.root, {
        root: { kind: "surface", key: surface.key },
        scope: `${preview.id}:surface:${surface.key}`,
        parentIsList: false,
        observe: (realization) => resources.register(realization),
      });
      overlay.append(scrim, surfaceHost);
      wrapper.append(overlay);
    }
  } else {
    renderer.realizeRoot(wrapper, preview.content, {
      root: { kind: "fragment" },
      scope: `${preview.id}:fragment`,
      parentIsList: false,
      observe: (realization) => resources.register(realization),
    });
  }
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
  render: EditorRender,
  stylesheet: CSSStyleSheet,
  resources: RealizationResources,
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
  if (realize) realizePreview(document, preview, render, stylesheet, shadowHost, resources);

  const caption = element(document, "figcaption");
  const captionId = `caption-${preview.id}`;
  caption.id = captionId;
  figure.setAttribute("aria-labelledby", captionId);
  caption.append(element(
    document,
    "span",
    "caption-title",
    `${preview.identity.subject} / ${preview.identity.example}`,
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
        `${surface.modality} ${surface.definition}`,
      );
      surfaceBadge.dataset.relation = surface.relation;
      surfaceBadge.title = {
        direct: "Child surface opened by this replay edge",
        inherited: "Child surface inherited from replay ancestry",
        mounted: "Child surface mounted in this snapshot",
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
  const section = element(document, "section", "navigator-group");
  section.dataset.navigatorGroup = "";
  section.dataset.search = `${group.kind} ${group.subject}`.toLocaleLowerCase();

  const row = element(document, "button", "navigator-row");
  row.type = "button";
  row.dataset.groupId = group.id;
  row.append(
    element(document, "span", "navigator-kind"),
    element(document, "span", "navigator-row-title", group.subject),
    element(document, "span", "navigator-count", String(previews.length)),
  );
  (row.firstElementChild as HTMLElement).dataset.kind = group.kind;

  const list = element(document, "div", "navigator-frames");
  for (const preview of previews) {
    const button = element(document, "button", "navigator-frame");
    button.type = "button";
    button.dataset.previewId = preview.id;
    button.dataset.search = [
      preview.identity.kind,
      preview.identity.subject,
      preview.identity.example,
    ].join(" ").toLocaleLowerCase();
    button.setAttribute("aria-pressed", "false");
    button.append(
      element(document, "span", "navigator-frame-icon"),
      element(document, "span", "navigator-frame-title", preview.identity.example),
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

export const prepareEditorModel = (
  document: Document,
  render: EditorRender | null,
  previous: PreparedEditorModel | null = null,
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
      render,
      stylesheet: null,
      reusableRealizationIds: new Set(),
      reusableFrameIds: new Set(),
    };
  }

  const stylesheet = previous?.render?.stylesheet === render.stylesheet
    ? previous.stylesheet ?? preparePreviewStylesheet(document, render.stylesheet)
    : preparePreviewStylesheet(document, render.stylesheet);
  const reusableRealizationIds = new Set(
    [...reusablePreviewIds(previous?.render ?? null, render)].filter((id) =>
      previous?.frameById.has(id) ?? false),
  );
  const reusableFrameIds = new Set(
    [...reusablePreviewFrameIds(previous?.render ?? null, render)].filter((id) =>
      previous?.frameById.has(id) ?? false),
  );

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
        `${group.kind} ${group.subject}`,
      ));
      const frames = element(document, "div", "row-frames");
      const laneCount = groupConnectors.reduce(
        (count, connector) => Math.max(count, connector.lane + 1),
        0,
      );
      if (laneCount > 0) {
        frames.style.setProperty("--workflow-rail-height", `${workflowRailHeight(laneCount)}px`);
      }
      for (const preview of typedPreviews) {
        const resources = new RealizationResources();
        resources.claim(resourceOwner);
        resourcesByPreviewId.set(preview.id, resources);
        const prepared = frame(
          document,
          preview,
          render,
          stylesheet,
          resources,
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
    render,
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
