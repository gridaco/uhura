import { createEditorRenderer } from "../renderer/editor.js";
import type { Snapshot, VNode } from "../protocol/types.js";
import {
  type EditorPreview,
  type EditorRender,
  type PreviewIdentity,
  semanticPreviewKey,
} from "./editor-state.js";
import { preparePreviewStylesheet } from "./editor-styles.js";
import { reusablePreviewIds } from "./editor-updates.js";

export interface PreparedEditorModel {
  board: HTMLElement;
  navigator: DocumentFragment;
  frameById: Map<string, HTMLElement>;
  previewById: Map<string, EditorPreview>;
  previewIdByIdentity: Map<string, string>;
  render: EditorRender | null;
  stylesheet: CSSStyleSheet | null;
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
    renderer.realize(wrapper, [preview.content.page.root], {
      scope: `${preview.id}:page`,
      parentIsList: false,
    });
    for (const [index, surface] of preview.content.surfaces.entries()) {
      const overlay = element(document, "div", "uh-surface-overlay");
      const scrim = element(document, "div", "uh-scrim");
      const surfaceHost = element(
        document,
        "div",
        `uh-surface uh-modality-${surface.modality}`,
      );
      surfaceHost.setAttribute("role", "dialog");
      surfaceHost.setAttribute("aria-modal", "true");
      renderer.realize(surfaceHost, [surface.root], {
        scope: `${preview.id}:surface:${index}`,
        parentIsList: false,
      });
      overlay.append(scrim, surfaceHost);
      wrapper.append(overlay);
    }
  } else {
    renderer.realize(wrapper, [preview.content], {
      scope: `${preview.id}:fragment`,
      parentIsList: false,
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

const frame = (
  document: Document,
  preview: EditorPreview,
  render: EditorRender,
  stylesheet: CSSStyleSheet,
): HTMLElement => {
  const figure = element(document, "figure", "editor-frame");
  figure.dataset.previewId = preview.id;
  figure.tabIndex = 0;
  figure.setAttribute("role", "button");
  figure.setAttribute("aria-pressed", "false");

  const shellClass = preview.identity.kind === "page"
    ? "device"
    : preview.identity.kind === "surface"
      ? "sheet"
      : "component";
  const shell = element(document, "div", `preview-shell ${shellClass}`);
  const shadowHost = element(document, "div", "preview-shadow-host");
  shell.append(shadowHost);
  realizePreview(document, preview, render, stylesheet, shadowHost);

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
  caption.append(element(document, "span", "caption-prov", provenance(preview)));
  if (preview.note) caption.append(element(document, "p", "caption-note", preview.note));
  figure.append(shell, caption);
  return figure;
};

const framePlaceholder = (document: Document, preview: EditorPreview): HTMLElement => {
  const placeholder = element(document, "figure", "editor-frame-placeholder");
  placeholder.dataset.previewId = preview.id;
  return placeholder;
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
  const previewById = new Map<string, EditorPreview>();
  const previewIdByIdentity = new Map<string, string>();

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
      previewById,
      previewIdByIdentity,
      render,
      stylesheet: null,
      reusableFrameIds: new Set(),
    };
  }

  const stylesheet = previous?.render?.stylesheet === render.stylesheet
    ? previous.stylesheet ?? preparePreviewStylesheet(document, render.stylesheet)
    : preparePreviewStylesheet(document, render.stylesheet);
  const reusableFrameIds = new Set(
    [...reusablePreviewIds(previous?.render ?? null, render)].filter((id) =>
      previous?.frameById.has(id) ?? false),
  );

  for (const preview of render.previews) {
    previewById.set(preview.id, preview);
    previewIdByIdentity.set(semanticPreviewKey(preview.identity), preview.id);
  }
  for (const group of render.groups) {
    const previews = group.previews.map((id) => previewById.get(id));
    if (previews.some((preview) => preview === undefined)) {
      // The decoder already enforces this. Keep the preparation boundary
      // independently total in case a typed caller bypasses JSON decoding.
      throw new Error(`Editor group ${group.id} refers to an unknown preview`);
    }
    const typedPreviews = previews as EditorPreview[];
    const row = element(document, "section", "preview-row");
    row.dataset.groupId = group.id;
    row.append(element(
      document,
      "h2",
      "row-title",
      `${group.kind} ${group.subject}`,
    ));
    const frames = element(document, "div", "row-frames");
    for (const preview of typedPreviews) {
      const previewFrame = reusableFrameIds.has(preview.id)
        ? framePlaceholder(document, preview)
        : frame(document, preview, render, stylesheet);
      frameById.set(preview.id, previewFrame);
      frames.append(previewFrame);
    }
    row.append(frames);
    board.append(row);
    navigator.append(navigatorGroup(document, group, typedPreviews));
  }

  return {
    board,
    navigator,
    frameById,
    previewById,
    previewIdByIdentity,
    render,
    stylesheet,
    reusableFrameIds,
  };
};

interface FrameReplacement {
  id: string;
  previous: HTMLElement;
  candidate: HTMLElement;
  shadow: ShadowRoot;
  parent: ParentNode;
  nextSibling: ChildNode | null;
  stylesheets: CSSStyleSheet[];
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
  if (next.reusableFrameIds.size === 0) return;
  if (!next.stylesheet) {
    throw new Error("A renderable Editor model must have a preview stylesheet");
  }

  // Validate the entire transplant before moving the first connected frame.
  const replacements: FrameReplacement[] = [];
  for (const id of next.reusableFrameIds) {
    const previousFrame = previous.frameById.get(id);
    const candidate = next.frameById.get(id);
    const shadow = previousFrame
      ?.querySelector<HTMLElement>(".preview-shadow-host")
      ?.shadowRoot;
    const parent = previousFrame?.parentNode;
    if (!previousFrame || !candidate || !shadow || !parent || !candidate.parentNode) {
      throw new Error(`Reusable Editor preview ${id} has no realized frame`);
    }
    replacements.push({
      id,
      previous: previousFrame,
      candidate,
      shadow,
      parent,
      nextSibling: previousFrame.nextSibling,
      stylesheets: [...shadow.adoptedStyleSheets],
    });
  }

  const moved: FrameReplacement[] = [];
  try {
    for (const replacement of replacements) {
      moved.push(replacement);
      replacement.candidate.replaceWith(replacement.previous);
      if (
        replacement.shadow.adoptedStyleSheets.length !== 1
        || replacement.shadow.adoptedStyleSheets[0] !== next.stylesheet
      ) {
        replacement.shadow.adoptedStyleSheets = [next.stylesheet];
      }
      next.frameById.set(replacement.id, replacement.previous);
    }
  } catch (error) {
    for (const replacement of moved.reverse()) {
      replacement.previous.replaceWith(replacement.candidate);
      const anchor = replacement.nextSibling?.parentNode === replacement.parent
        ? replacement.nextSibling
        : null;
      replacement.parent.insertBefore(replacement.previous, anchor);
      replacement.shadow.adoptedStyleSheets = replacement.stylesheets;
      next.frameById.set(replacement.id, replacement.candidate);
    }
    throw error;
  }
};

export const previewIdForIdentity = (
  model: PreparedEditorModel,
  identity: PreviewIdentity | null,
): string | null => identity
  ? model.previewIdByIdentity.get(semanticPreviewKey(identity)) ?? null
  : null;
