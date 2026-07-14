import type { Descriptor, VNode } from "../protocol/types.js";
import type { AssetAppliers } from "./assets.js";
import { createPlayAssets } from "./assets.js";
import type {
  RendererNode,
  ScrollController,
  TextFieldController,
} from "./contracts.js";
import type { IconTable } from "./icons.js";
import { createSemanticRenderer, findScope as findRendererScope } from "./reconciler.js";

export interface PlayRendererOptions {
  document?: Document;
  emit(
    descriptor: Descriptor,
    data?: Record<string, unknown>,
    onApplied?: () => void,
  ): void;
  icons: IconTable;
  assets: AssetAppliers;
  textFields: TextFieldController;
  scrolls: ScrollController;
}

export interface PlayRenderer {
  reconcileChildren(
    host: HTMLElement,
    nodes: VNode[],
    parentPath: string,
    parentIsList: boolean,
  ): void;
  applyNode(
    el: HTMLElement,
    node: VNode,
    parentPath: string,
    listItem: boolean,
  ): void;
  /** Releases runtime effects owned anywhere under a subtree before removal. */
  disposeSubtree(root: HTMLElement): void;
}

function browserDocument(injected: Document | undefined): Document {
  if (injected) return injected;
  if (typeof document === "undefined") {
    throw new Error("createPlayRenderer requires a Document outside a browser");
  }
  return document;
}

/** Play facade over the same semantic engine, with runtime effects enabled. */
export function createPlayRenderer(options: PlayRendererOptions): PlayRenderer {
  const renderer = createSemanticRenderer({
    document: browserDocument(options.document),
    icons: options.icons,
    assets: options.assets,
    policy: {
      kind: "play",
      emit: options.emit,
      textFields: options.textFields,
      scrolls: options.scrolls,
      disposeSubtree: (root) => options.scrolls.disposeSubtree(root),
    },
  });

  return {
    reconcileChildren(host, nodes, parentPath, parentIsList) {
      renderer.reconcileChildren(
        host,
        nodes as RendererNode[],
        parentPath,
        parentIsList,
      );
    },
    applyNode(el, node, parentPath, listItem) {
      renderer.applyNode(el, node as RendererNode, parentPath, listItem);
    },
    disposeSubtree(root) {
      renderer.disposeSubtree(root);
    },
  };
}

export function findScope(node: VNode): string | undefined {
  return findRendererScope(node as RendererNode);
}

export { createPlayAssets };
export type { AssetAppliers, ResolveAsset } from "./assets.js";
export type { IconDefinition, IconTable } from "./icons.js";
export type { ScrollController, TextFieldController } from "./contracts.js";
