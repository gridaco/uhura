import { createEditorAssets } from "./assets.js";
import type { EditorAssetTable } from "./assets.js";
import type {
  EditorNode,
  EditorNodeRealization,
  EditorRealizationObserver,
  EditorRenderRoot,
  RendererNode,
} from "./contracts.js";
import type { IconFontRegistry } from "./icons.js";
import { createSemanticRenderer } from "./reconciler.js";

export interface EditorRendererOptions {
  document?: Document;
  /** Host-loaded icon-font resource for the same render revision. */
  icons: IconFontRegistry;
  assets: EditorAssetTable;
}

export interface EditorRealizeOptions {
  scope?: string;
  parentIsList?: boolean;
}

export interface EditorRealizeRootOptions extends EditorRealizeOptions {
  root: EditorRenderRoot;
  observe?: EditorRealizationObserver;
}

export interface EditorRenderer {
  /** Replaces the host with a fresh, inert realization of semantic nodes. */
  realize(
    host: HTMLElement,
    nodes: EditorNode[],
    options?: EditorRealizeOptions,
  ): void;
  /**
   * Replaces the host with one semantic root and reports every realized node.
   * Results and callbacks are pre-order and are published only after the full
   * tree has been mounted. The renderer retains no element references.
   */
  realizeRoot(
    host: HTMLElement,
    node: EditorNode,
    options: EditorRealizeRootOptions,
  ): readonly EditorNodeRealization[];
}

function browserDocument(injected: Document | undefined): Document {
  if (injected) return injected;
  if (typeof document === "undefined") {
    throw new Error("createEditorRenderer requires a Document outside a browser");
  }
  return document;
}

/**
 * Creates the read-only renderer facade. Its construction surface has no
 * emitter, descriptor delivery, provider resolver, scroll controller, or
 * textfield controller to accidentally wire into a preview.
 */
export function createEditorRenderer(options: EditorRendererOptions): EditorRenderer {
  const dom = browserDocument(options.document);
  const renderer = createSemanticRenderer({
    document: dom,
    icons: options.icons,
    assets: createEditorAssets(options.assets),
    policy: { kind: "editor" },
  });

  return {
    realize(host, nodes, realizeOptions = {}) {
      // One-shot means no holder/listener identity crosses preview revisions.
      host.replaceChildren();
      host.inert = true;
      renderer.realizeChildren(
        host,
        nodes as RendererNode[],
        realizeOptions.scope ?? "editor",
        realizeOptions.parentIsList ?? false,
      );
    },
    realizeRoot(host, node, realizeOptions) {
      // Collect while building, but do not publish partially applied or
      // detached elements to Editor registry owners.
      const pending: { path: readonly number[]; element: HTMLElement }[] = [];
      host.replaceChildren();
      host.inert = true;
      renderer.realizeRoot(
        host,
        node as RendererNode,
        realizeOptions.scope ?? "editor",
        realizeOptions.parentIsList ?? false,
        (realization) => pending.push(realization),
      );

      const realizations = pending.map<EditorNodeRealization>(({ path, element }) => ({
        root: realizeOptions.root,
        path,
        element,
      }));
      for (const realization of realizations) realizeOptions.observe?.(realization);
      return realizations;
    },
  };
}

export type { EditorAsset, EditorAssetTable } from "./assets.js";
export type { IconFontRegistry } from "./icons.js";
export type {
  EditorNode,
  EditorNodeRealization,
  EditorRealizationObserver,
  EditorRenderNodeRef,
  EditorRenderRoot,
  SemanticNodePath,
} from "./contracts.js";
