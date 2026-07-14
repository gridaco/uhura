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
