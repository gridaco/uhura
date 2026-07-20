import type { NaturalText, Value } from "../../protocol/machine.js";
import type {
  ProjectionRendererOptions,
  RenderAttribute,
  RenderEvent,
  RenderNode,
} from "../projection.js";

export type ElementNode = Extract<RenderNode, { readonly kind: "element" }>;
export type RendererMode = NonNullable<ProjectionRendererOptions["mode"]>;
export type PrimitiveCapability = "icon-fonts";

export interface PrimitiveHosts {
  /** The host reconciled with the authored children, or null for void widgets. */
  readonly children: HTMLElement | null;
  /** The physical element that receives authored event bindings. */
  readonly events: HTMLElement;
}

export interface PrimitiveContext {
  readonly mode: RendererMode;
  readonly options: ProjectionRendererOptions;
  readonly projectionRevision: NaturalText | undefined;
  readonly applyAttributes: (
    element: HTMLElement,
    attributes: readonly RenderAttribute[],
  ) => void;
}

export interface PrimitiveEventContext {
  readonly target: HTMLElement;
  readonly node: ElementNode;
  readonly event: RenderEvent;
  readonly eventIndex: number;
  readonly dispatch: (value: Value) => void;
  readonly eventAllowed: (element: HTMLElement) => boolean;
}

export interface PrimitiveSupplementalEventsContext {
  readonly target: HTMLElement;
  readonly node: ElementNode;
  readonly events: readonly RenderEvent[];
  readonly dispatchAt: (eventIndex: number, value: Value) => void;
  readonly eventAllowed: (element: HTMLElement) => boolean;
}

export interface PrimitiveEventListener {
  readonly type: string;
  readonly listener: EventListener;
}

/**
 * One browser realization boundary for one checked Uhura primitive.
 *
 * The reconciler knows only this lifecycle. Attribute projection, physical
 * hosts, browser capabilities, event specialization, and mechanics remain
 * owned by the adapter named in the registry.
 */
export interface PrimitiveAdapter {
  readonly id: string;
  readonly tag: string;
  readonly capabilities?: readonly PrimitiveCapability[];
  readonly managedEvents?: readonly string[];
  attributes(
    node: ElementNode,
    mode: RendererMode,
  ): readonly RenderAttribute[];
  hosts?(
    element: HTMLElement,
    node: ElementNode,
    context: PrimitiveContext,
  ): PrimitiveHosts;
  sync?(
    element: HTMLElement,
    node: ElementNode,
    hosts: PrimitiveHosts,
    context: PrimitiveContext,
  ): void;
  bindEvent?(
    context: PrimitiveEventContext,
  ): readonly PrimitiveEventListener[] | undefined;
  bindSupplementalEvents?(
    context: PrimitiveSupplementalEventsContext,
  ): readonly PrimitiveEventListener[];
  childrenAreList?(node: ElementNode): boolean;
  dispose?(element: HTMLElement): void;
}
