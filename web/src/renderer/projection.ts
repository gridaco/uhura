import type {
  DecimalText,
  NaturalText,
  Value,
} from "../protocol/machine.js";
import { decimal, natural } from "../protocol/machine.js";
import type { AssetAppliers } from "./assets.js";
import type { IconFontRegistry } from "./icons.js";
import {
  textEvent,
  textAttribute,
  UNIT_EVENT,
} from "./primitives/common.js";
import {
  primitiveAdapter,
} from "./primitives/registry.js";
import type {
  ElementNode,
  PrimitiveAdapter,
  PrimitiveContext,
  PrimitiveEventListener,
  PrimitiveHosts,
  RendererMode,
} from "./primitives/types.js";

export const UHURA_VIEW_PROTOCOL = "uhura-view/1" as const;
export const UHURA_PROJECTION_SOURCES_PROTOCOL =
  "uhura-projection-sources/0" as const;

export type RenderAttributeValue = boolean | string;

export interface RenderAttribute {
  readonly name: string;
  readonly value: RenderAttributeValue;
}

export interface RenderEvent {
  readonly event: string;
  readonly binding: string;
}

export type RenderNode =
  | {
      readonly kind: "text";
      readonly key: string;
      readonly text: string;
    }
  | {
      readonly kind: "element";
      readonly key: string;
      readonly element: string;
      readonly attributes: readonly RenderAttribute[];
      readonly events: readonly RenderEvent[];
      readonly children: readonly RenderNode[];
      readonly surface: boolean;
    };

export interface RenderDocument {
  readonly protocol: typeof UHURA_VIEW_PROTOCOL;
  readonly presentation: string;
  readonly machine: string;
  readonly instance: string;
  readonly sequence: NaturalText;
  readonly nodes: readonly RenderNode[];
}

export interface ProjectionSource {
  readonly id: string;
  readonly path: string;
  readonly start: number;
  readonly end: number;
}

export interface ProjectionSources {
  readonly protocol: typeof UHURA_PROJECTION_SOURCES_PROTOCOL;
  readonly presentation: string;
  readonly nodes: Readonly<Record<string, ProjectionSource>>;
}

export interface ProjectionRenderer {
  /**
   * Reconciles one pure view. Live Play supplies its correlated browser
   * projection revision; static Editor projections omit it.
   */
  render(
    document: RenderDocument,
    projectionRevision?: NaturalText,
  ): void;
  dispose(): void;
}

export interface ProjectionRendererOptions {
  readonly root: HTMLElement;
  readonly dispatch: (
    binding: string,
    projectionRevision: NaturalText | undefined,
    event: Value,
  ) => void;
  /** Editor is inert and may realize authored static scroll positions. */
  readonly mode?: "editor" | "play";
  /** Optional host-owned logical-asset resolver for img/video primitives. */
  readonly assets?: AssetAppliers;
  /** Optional checked icon-font vocabulary for the icon primitive. */
  readonly icons?: IconFontRegistry;
  /**
   * Receives the stable semantic key and realized element after a complete
   * projection has reconciled. Static Editor previews use this direct handle
   * for source annotations without querying across a ShadowRoot.
   */
  readonly observeElement?: (key: string, element: HTMLElement) => void;
  /**
   * Live Play projects keyed surfaces into the browser top layer. Static
   * Editor examples keep them open in-card without acquiring document-wide
   * modality.
   */
  readonly modalSurfaces?: boolean;
}

const record = (
  value: unknown,
  context: string,
): Readonly<Record<string, unknown>> => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError(`${context} must be an object`);
  }
  return value as Readonly<Record<string, unknown>>;
};

const exactFields = (
  object: Readonly<Record<string, unknown>>,
  fields: readonly string[],
  context: string,
): void => {
  const expected = new Set(fields);
  const missing = fields.filter((field) => !Object.hasOwn(object, field));
  const extra = Object.keys(object).filter((field) => !expected.has(field));
  if (missing.length > 0 || extra.length > 0) {
    throw new TypeError(
      `${context} has the wrong fields; missing [${missing.join(", ")}], extra [${extra.join(", ")}]`,
    );
  }
};

const text = (
  value: unknown,
  context: string,
): string => {
  if (typeof value !== "string") {
    throw new TypeError(`${context} must be text`);
  }
  return value;
};

const list = (
  value: unknown,
  context: string,
): readonly unknown[] => {
  if (!Array.isArray(value)) {
    throw new TypeError(`${context} must be a list`);
  }
  return value;
};

const nonNegativeInteger = (value: unknown, context: string): number => {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new TypeError(`${context} must be a non-negative safe integer`);
  }
  return value as number;
};

const decodeNode = (value: unknown, context: string): RenderNode => {
  const node = record(value, context);
  const kind = text(node["kind"], `${context}.kind`);
  if (kind === "text") {
    exactFields(node, ["kind", "key", "text"], context);
    return {
      kind,
      key: text(node["key"], `${context}.key`),
      text: text(node["text"], `${context}.text`),
    };
  }
  if (kind !== "element") {
    throw new TypeError(`${context}.kind is not an Uhura render-node kind`);
  }
  exactFields(
    node,
    [
      "kind",
      "key",
      "element",
      "attributes",
      "events",
      "children",
      "surface",
    ],
    context,
  );
  const attributes = list(node["attributes"], `${context}.attributes`).map(
    (value, index): RenderAttribute => {
      const attributeContext = `${context}.attributes[${index}]`;
      const attribute = record(value, attributeContext);
      exactFields(attribute, ["name", "value"], attributeContext);
      const decoded = attribute["value"];
      if (typeof decoded !== "boolean" && typeof decoded !== "string") {
        throw new TypeError(
          `${context}.attributes[${index}].value must be boolean or text`,
        );
      }
      return {
        name: text(attribute["name"], `${attributeContext}.name`),
        value: decoded,
      };
    },
  );
  const events = list(node["events"], `${context}.events`).map(
    (value, index): RenderEvent => {
      const eventContext = `${context}.events[${index}]`;
      const event = record(value, eventContext);
      exactFields(event, ["event", "binding"], eventContext);
      return {
        event: text(event["event"], `${eventContext}.event`),
        binding: text(event["binding"], `${eventContext}.binding`),
      };
    },
  );
  if (typeof node["surface"] !== "boolean") {
    throw new TypeError(`${context}.surface must be boolean`);
  }
  return {
    kind,
    key: text(node["key"], `${context}.key`),
    element: text(node["element"], `${context}.element`),
    attributes,
    events,
    children: list(node["children"], `${context}.children`).map((child, index) =>
      decodeNode(child, `${context}.children[${index}]`)
    ),
    surface: node["surface"],
  };
};

/** Validates a host/Wasm view before any foreign value reaches the DOM. */
export const decodeRenderDocument = (
  value: unknown,
  context = "Uhura render document",
): RenderDocument => {
  const document = record(value, context);
  exactFields(
    document,
    [
      "protocol",
      "presentation",
      "machine",
      "instance",
      "sequence",
      "nodes",
    ],
    context,
  );
  if (document["protocol"] !== UHURA_VIEW_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_VIEW_PROTOCOL)}`,
    );
  }
  return {
    protocol: UHURA_VIEW_PROTOCOL,
    presentation: text(document["presentation"], `${context}.presentation`),
    machine: text(document["machine"], `${context}.machine`),
    instance: text(document["instance"], `${context}.instance`),
    sequence: natural(text(document["sequence"], `${context}.sequence`)),
    nodes: list(document["nodes"], `${context}.nodes`).map((node, index) =>
      decodeNode(node, `${context}.nodes[${index}]`)
    ),
  };
};

const renderKeys = (
  nodes: readonly RenderNode[],
): readonly string[] => nodes.flatMap((node) => [
  node.key,
  ...(node.kind === "element" ? renderKeys(node.children) : []),
]);

export const decodeProjectionSources = (
  value: unknown,
  document: RenderDocument,
  context = "Uhura projection sources",
): ProjectionSources => {
  const source = record(value, context);
  exactFields(source, ["protocol", "presentation", "nodes"], context);
  if (source["protocol"] !== UHURA_PROJECTION_SOURCES_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_PROJECTION_SOURCES_PROTOCOL)}`,
    );
  }
  const presentation = text(source["presentation"], `${context}.presentation`);
  if (presentation !== document.presentation) {
    throw new TypeError(`${context}.presentation must match its render document`);
  }
  const sourceNodes = record(source["nodes"], `${context}.nodes`);
  const nodes = Object.fromEntries(
    Object.entries(sourceNodes).map(([key, value]) => {
      const sourceContext = `${context}.nodes[${JSON.stringify(key)}]`;
      const reference = record(value, sourceContext);
      exactFields(reference, ["id", "path", "start", "end"], sourceContext);
      const start = nonNegativeInteger(reference["start"], `${sourceContext}.start`);
      const end = nonNegativeInteger(reference["end"], `${sourceContext}.end`);
      if (end < start) {
        throw new TypeError(`${sourceContext}.end must not precede start`);
      }
      return [key, {
        id: text(reference["id"], `${sourceContext}.id`),
        path: text(reference["path"], `${sourceContext}.path`),
        start,
        end,
      }] as const;
    }),
  );
  const expected = renderKeys(document.nodes);
  if (new Set(expected).size !== expected.length) {
    throw new TypeError(`${context} cannot address duplicate rendered keys`);
  }
  const expectedSet = new Set(expected);
  const missing = expected.find((key) => !Object.hasOwn(nodes, key));
  const extra = Object.keys(nodes).find((key) => !expectedSet.has(key));
  if (missing !== undefined || extra !== undefined) {
    throw new TypeError(
      `${context}.nodes must address every rendered key exactly; missing ${JSON.stringify(missing ?? null)}, extra ${JSON.stringify(extra ?? null)}`,
    );
  }
  return {
    protocol: UHURA_PROJECTION_SOURCES_PROTOCOL,
    presentation,
    nodes,
  };
};

const inputValueEvent = (value: string): Value => ({
  $: "record",
  fields: [{
    name: "value",
    value: { $: "Text", value },
  }],
});

const canonicalDecimal = (source: string): DecimalText | null => {
  const match = /^(-?)(\d+)(?:\.(\d+))?$/u.exec(source);
  if (!match) return null;
  const sign = match[1] === "-" ? "-" : "";
  let whole = (match[2] ?? "0").replace(/^0+(?=\d)/u, "");
  let fraction = (match[3] ?? "").replace(/0+$/u, "");
  if (whole.length === 0) whole = "0";
  if (whole === "0" && fraction.length === 0) return decimal("0");
  return decimal(`${sign}${whole}${fraction.length > 0 ? `.${fraction}` : ""}`);
};

const isHtmlElement = (value: unknown): value is HTMLElement =>
  typeof value === "object"
  && value !== null
  && (value as { nodeType?: unknown }).nodeType === 1
  && "dataset" in value;

const isTextNode = (value: unknown): value is Text =>
  typeof value === "object"
  && value !== null
  && (value as { nodeType?: unknown }).nodeType === 3
  && "data" in value;

const eventValue = (element: Element, event: string): Value => {
  if (event === "input" && element.localName === "input") {
    return inputValueEvent((element as HTMLInputElement).value);
  }
  if (event !== "change") {
    return UNIT_EVENT;
  }
  if (element.localName === "textarea" || element.localName === "select") {
    return textEvent((element as HTMLTextAreaElement | HTMLSelectElement).value);
  }
  if (element.localName !== "input") return UNIT_EVENT;
  const input = element as HTMLInputElement;
  if (input.type !== "number" && input.type !== "range") {
    return textEvent(input.value);
  }
  const decimal = canonicalDecimal(input.value);
  const number: Value =
    decimal === null
      ? { $: "BoundaryNumber", case: "nan" }
      : { $: "BoundaryNumber", case: "finite", value: decimal };
  return {
    $: "record",
    fields: [{ name: "number", value: number }],
  };
};

const DOM_EVENT: Readonly<Record<string, string>> = {
  press: "click",
  activate: "click",
  "activate-double": "dblclick",
  follow: "click",
  change: "input",
  submit: "keydown",
};

const appliedAttributes = new WeakMap<Element, Set<string>>();
const listeners = new WeakMap<
  Element,
  readonly PrimitiveEventListener[]
>();
interface AppliedEventConfiguration {
  readonly mode: RendererMode;
  readonly semanticElement: string;
  readonly dispatch: ProjectionRendererOptions["dispatch"];
  readonly projectionRevision: NaturalText | undefined;
  readonly events: readonly RenderEvent[];
}

const eventConfigurations = new WeakMap<Element, AppliedEventConfiguration>();
const semanticElements = new WeakMap<Element, string>();

/**
 * Delegates the closed primitive vocabulary to its adapters. Non-primitive
 * HTML nodes deliberately retain transparent attribute passthrough.
 */
const physicalAttributes = (
  node: ElementNode,
  mode: RendererMode,
  listItem: boolean,
): readonly RenderAttribute[] => {
  const adapter = primitiveAdapter(node.element);
  let projected: readonly RenderAttribute[];
  if (!adapter) {
    const authoredSurfaceClass =
      textAttribute(node.attributes, "class")?.trim();
    projected = node.surface
      ? [
        ...node.attributes.filter((candidate) => candidate.name !== "class"),
        {
          name: "class",
          value: `uhura-surface${
            authoredSurfaceClass ? ` ${authoredSurfaceClass}` : ""
          }`,
        },
      ]
      : [...node.attributes];
  } else {
    projected = adapter.attributes(node, mode);
  }
  if (!listItem) return projected;
  const semanticRole = projected.find((candidate) => candidate.name === "role");
  if (semanticRole !== undefined && semanticRole.value !== "none") {
    throw new Error(
      `checked Uhura lists require a neutral direct child; <${node.element}> projected role ${JSON.stringify(semanticRole.value)}`,
    );
  }
  return [
    ...projected.filter((candidate) => candidate.name !== "role"),
    { name: "role", value: "listitem" },
  ];
};

const applyAttributes = (
  element: HTMLElement,
  attributes: readonly RenderAttribute[],
): void => {
  const next = new Set(attributes.map((attribute) => attribute.name));
  for (const name of appliedAttributes.get(element) ?? []) {
    if (!next.has(name)) element.removeAttribute(name);
  }
  for (const { name, value } of attributes) {
    if (typeof value === "boolean") {
      if (name.startsWith("aria-")) {
        const text = String(value);
        if (element.getAttribute(name) !== text) {
          element.setAttribute(name, text);
        }
      } else if (element.hasAttribute(name) !== value) {
        element.toggleAttribute(name, value);
      }
      continue;
    }
    if (element.getAttribute(name) !== value) {
      element.setAttribute(name, value);
    }
    if (name === "value" && element.localName === "input") {
      const input = element as HTMLInputElement;
      if (input.value !== value) input.value = value;
    }
  }
  appliedAttributes.set(element, next);
};

const eventAllowed = (element: HTMLElement): boolean =>
  !element.hasAttribute("disabled")
  && element.getAttribute("aria-disabled") !== "true";

const sameEventShape = (
  left: readonly RenderEvent[],
  right: readonly RenderEvent[],
): boolean =>
  left.length === right.length
  && left.every((event, index) => {
    const candidate = right[index];
    return candidate?.event === event.event;
  });

const dispatchConfiguredEvent = (
  element: HTMLElement,
  eventIndex: number,
  expectedEvent: string,
  value: Value,
): void => {
  const configuration = eventConfigurations.get(element);
  const current = configuration?.events[eventIndex];
  if (configuration && current?.event === expectedEvent) {
    configuration.dispatch(
      current.binding,
      configuration.projectionRevision,
      value,
    );
  }
};

const applyEvents = (
  element: HTMLElement,
  node: ElementNode,
  events: readonly RenderEvent[],
  projectionRevision: NaturalText | undefined,
  options: ProjectionRendererOptions,
  mode: RendererMode,
  adapter: PrimitiveAdapter | undefined,
): void => {
  const previous = eventConfigurations.get(element);
  const retainListeners =
    previous?.mode === mode
    && previous.semanticElement === node.element
    && sameEventShape(previous.events, events);

  eventConfigurations.set(element, {
    mode,
    semanticElement: node.element,
    dispatch: options.dispatch,
    projectionRevision,
    events: events.map(({ event, binding }) => ({ event, binding })),
  });
  if (retainListeners) return;

  for (const { type, listener } of listeners.get(element) ?? []) {
    element.removeEventListener(type, listener);
  }
  if (mode === "editor") {
    listeners.set(element, []);
    return;
  }

  const next: PrimitiveEventListener[] = [];
  const attach = (
    registrations: readonly PrimitiveEventListener[],
  ): void => {
    for (const registration of registrations) {
      element.addEventListener(registration.type, registration.listener);
      next.push(registration);
    }
  };
  for (const [eventIndex, binding] of events.entries()) {
    const { event } = binding;
    if (adapter?.managedEvents?.includes(event)) continue;
    const specialized = adapter?.bindEvent?.({
      target: element,
      node,
      event: binding,
      eventIndex,
      dispatch: (value) =>
        dispatchConfiguredEvent(element, eventIndex, event, value),
      eventAllowed,
    });
    if (specialized) {
      attach(specialized);
      continue;
    }
    const type =
      event === "change" && element.localName === "select"
        ? "change"
        : DOM_EVENT[event] ?? event;
    const listener: EventListener = (domEvent) => {
      if (event === "follow") domEvent.preventDefault();
      if (
        event === "submit"
        && (!(domEvent instanceof KeyboardEvent)
          || domEvent.key !== "Enter"
          || domEvent.isComposing
          || domEvent.keyCode === 229)
      ) {
        return;
      }
      if (event === "activate-double") domEvent.preventDefault();
      if (!eventAllowed(element)) return;
      dispatchConfiguredEvent(
        element,
        eventIndex,
        event,
        eventValue(element, event),
      );
    };
    attach([{ type, listener }]);
  }
  attach(adapter?.bindSupplementalEvents?.({
    target: element,
    node,
    events,
    dispatchAt: (eventIndex, value) => {
      const expected = events[eventIndex]?.event;
      if (expected !== undefined) {
        dispatchConfiguredEvent(element, eventIndex, expected, value);
      }
    },
    eventAllowed,
  }) ?? []);
  listeners.set(element, next);
};

const physicalTag = (element: string): string =>
  primitiveAdapter(element)?.tag ?? element.toLowerCase();

const primitiveContext = (
  options: ProjectionRendererOptions,
  mode: RendererMode,
  projectionRevision: NaturalText | undefined,
): PrimitiveContext => ({
  mode,
  options,
  projectionRevision,
  applyAttributes,
});

const defaultHosts = (element: HTMLElement): PrimitiveHosts => ({
  children: element.localName === "input" ? null : element,
  events: element,
});

const disposeRealizationTree = (root: HTMLElement): void => {
  for (const child of Array.from(root.children)) {
    if (isHtmlElement(child)) disposeRealizationTree(child);
  }
  const semantic = semanticElements.get(root);
  if (semantic !== undefined) primitiveAdapter(semantic)?.dispose?.(root);
};

const elementFor = (
  document: Document,
  node: ElementNode,
): HTMLElement => {
  const existingName = physicalTag(node.element);
  const element =
    existingName === "dialog"
      ? document.createElement("dialog")
      : document.createElement(existingName);
  element.dataset["uhuraKey"] = node.key;
  semanticElements.set(element, node.element);
  if (node.surface && element.localName === "dialog") {
    element.classList.add("uhura-surface");
    element.setAttribute("aria-modal", "true");
    element.addEventListener("cancel", (event) => {
      // Surface lifetime belongs to the machine. Browser Escape cannot close
      // it behind the Uhura machine's committed observation.
      event.preventDefault();
    });
  }
  return element;
};

const reconcile = (
  parent: HTMLElement,
  nodes: readonly RenderNode[],
  modalSurfaces: boolean,
  observed: Array<{ readonly key: string; readonly element: HTMLElement }>,
  options: ProjectionRendererOptions,
  mode: RendererMode,
  projectionRevision: NaturalText | undefined,
  parentIsList = false,
): void => {
  const document = parent.ownerDocument;
  const existing = new Map<string, ChildNode>();
  for (const node of Array.from(parent.childNodes)) {
    if (isHtmlElement(node) && node.dataset["uhuraKey"]) {
      existing.set(node.dataset["uhuraKey"], node);
    } else if (
      isTextNode(node)
      && (node as Text & { __uhuraKey?: string }).__uhuraKey
    ) {
      existing.set(
        (node as Text & { __uhuraKey: string }).__uhuraKey,
        node,
      );
    }
  }
  let cursor: ChildNode | null = parent.firstChild;
  const retained = new Set<ChildNode>();
  for (const node of nodes) {
    let child = existing.get(node.key);
    if (node.kind === "text") {
      if (!isTextNode(child)) {
        child = document.createTextNode(node.text);
        (child as Text & { __uhuraKey: string }).__uhuraKey = node.key;
      } else if (child.data !== node.text) {
        child.data = node.text;
      }
      // Text nodes have no box/style contract of their own. Keyed source
      // annotations resolve them to the closest stable measurable element.
      observed.push({ key: node.key, element: parent });
    } else {
      if (
        !isHtmlElement(child) ||
        child.localName !== physicalTag(node.element)
        || semanticElements.get(child) !== node.element
      ) {
        child = elementFor(document, node);
      }
      if (!isHtmlElement(child)) {
        throw new Error("Uhura element reconciliation lost its HTMLElement");
      }
      applyAttributes(
        child,
        physicalAttributes(node, mode, parentIsList),
      );
      const adapter = primitiveAdapter(node.element);
      const context = primitiveContext(options, mode, projectionRevision);
      const hosts =
        adapter?.hosts?.(child, node, context) ?? defaultHosts(child);
      applyEvents(
        hosts.events,
        node,
        node.events,
        projectionRevision,
        options,
        mode,
        adapter,
      );
      observed.push({ key: node.key, element: child });
      if (hosts.children) {
        reconcile(
          hosts.children,
          node.children,
          modalSurfaces,
          observed,
          options,
          mode,
          projectionRevision,
          adapter?.childrenAreList?.(node) ?? false,
        );
      }
      adapter?.sync?.(child, node, hosts, context);
    }
    retained.add(child);
    if (child !== cursor) parent.insertBefore(child, cursor);
    if (
      node.kind === "element"
      && node.surface
      && isHtmlElement(child)
      && child.localName === "dialog"
      && !(child as HTMLDialogElement).open
    ) {
      const dialog = child as HTMLDialogElement;
      if (modalSurfaces && typeof dialog.showModal === "function") {
        try {
          dialog.showModal();
        } catch {
          // DOM implementations without a top layer still preserve the
          // visible, semantically owned surface.
          child.setAttribute("open", "");
        }
      } else {
        child.setAttribute("open", "");
      }
    }
    cursor = child.nextSibling;
  }
  for (const child of Array.from(parent.childNodes)) {
    if (
      isHtmlElement(child)
      && child.dataset["uhMechanic"] !== undefined
    ) {
      continue;
    }
    if (!retained.has(child)) {
      if (isHtmlElement(child)) disposeRealizationTree(child);
      child.remove();
    }
  }
};

export const createProjectionRenderer = (
  options: ProjectionRendererOptions,
): ProjectionRenderer => {
  let disposed = false;
  const mode = options.mode ?? "play";
  options.root.inert = mode === "editor";
  return {
    render(document, projectionRevision): void {
      if (disposed) throw new Error("Uhura renderer is disposed");
      if (document.protocol !== UHURA_VIEW_PROTOCOL) {
        throw new Error(`unsupported Uhura render protocol: ${document.protocol}`);
      }
      const observed: Array<{
        readonly key: string;
        readonly element: HTMLElement;
      }> = [];
      reconcile(
        options.root,
        document.nodes,
        options.modalSurfaces ?? true,
        observed,
        options,
        mode,
        projectionRevision,
      );
      for (const realization of observed) {
        options.observeElement?.(realization.key, realization.element);
      }
    },
    dispose(): void {
      if (disposed) return;
      disposed = true;
      disposeRealizationTree(options.root);
      options.root.replaceChildren();
    },
  };
};
