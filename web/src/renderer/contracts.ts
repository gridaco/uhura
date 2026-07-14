import type { Descriptor, VValue } from "../protocol/types.js";

/**
 * The descriptor-free node shape accepted by the read-only Editor renderer.
 * Runtime VNodes are structurally assignable to this type, but the Editor API
 * does not advertise or consume their `on` descriptors.
 */
export interface EditorNode {
  key: string;
  element: string;
  class?: string;
  props: Record<string, VValue>;
  children?: EditorNode[];
}

/** The independently rendered semantic tree within one Editor preview. */
export type EditorRenderRoot =
  | { readonly kind: "page" }
  | { readonly kind: "fragment" }
  | { readonly kind: "surface"; readonly key: string };

/**
 * Child indexes in the semantic node tree. The root node has the empty path.
 * Renderer-created mechanic DOM (for example a pager track) is never counted.
 */
export type SemanticNodePath = readonly number[];

export interface EditorRenderNodeRef {
  readonly root: EditorRenderRoot;
  readonly path: SemanticNodePath;
}

/** A semantic node and the exact DOM element created to realize it. */
export interface EditorNodeRealization extends EditorRenderNodeRef {
  readonly element: HTMLElement;
}

export type EditorRealizationObserver = (
  realization: EditorNodeRealization,
) => void;

/** Internal superset used by the shared engine when Play enables effects. */
export interface RendererNode extends EditorNode {
  children?: RendererNode[];
  on?: Descriptor[];
}

export interface TextFieldHolder {
  on: Record<string, Descriptor>;
}

export interface TextFieldController {
  wire(input: HTMLInputElement, holder: TextFieldHolder): void;
  applyValue(input: HTMLInputElement, value: string): void;
}

export interface NearEndState {
  sentinel: HTMLElement;
  io: IntersectionObserver;
  armed: boolean;
  lastHeight: number;
}

export interface ScrollHolder {
  path: string;
  on: Record<string, Descriptor>;
  nearEnd?: NearEndState;
}

export interface ScrollController {
  sync(el: HTMLElement, holder: ScrollHolder): void;
  disposeSubtree(root: HTMLElement): void;
  savePositions(navKey: string, pageEl: HTMLElement): void;
  restorePositions(navKey: string, pageEl: HTMLElement): void;
}

export interface EditorRenderPolicy {
  readonly kind: "editor";
}

export interface PlayRenderPolicy {
  readonly kind: "play";
  emit(
    descriptor: Descriptor,
    data?: Record<string, unknown>,
    onApplied?: () => void,
  ): void;
  textFields: TextFieldController;
  scrolls: ScrollController;
  disposeSubtree(root: HTMLElement): void;
}

export type RenderPolicy = EditorRenderPolicy | PlayRenderPolicy;
