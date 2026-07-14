import { createEditorAssets } from "./assets.js";
import type { EditorAssetTable } from "./assets.js";
import type { EditorNode, RendererNode } from "./contracts.js";
import type { IconTable } from "./icons.js";
import { createSemanticRenderer } from "./reconciler.js";

export interface EditorRendererOptions {
  document?: Document;
  icons: IconTable;
  assets: EditorAssetTable;
}

export interface EditorRealizeOptions {
  scope?: string;
  parentIsList?: boolean;
}

export interface EditorRenderer {
  /** Replaces the host with a fresh, inert realization of semantic nodes. */
  realize(
    host: HTMLElement,
    nodes: EditorNode[],
    options?: EditorRealizeOptions,
  ): void;
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
 * text-field controller to accidentally wire into a preview.
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
  };
}

export type { EditorAsset, EditorAssetTable } from "./assets.js";
export type {
  IconDefinition,
  IconTable,
  IconCommand,
  IconPaint,
} from "./icons.js";
export type { EditorNode } from "./contracts.js";
