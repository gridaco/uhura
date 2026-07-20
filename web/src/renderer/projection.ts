import type {
  DecimalText,
  NaturalText,
  Value,
} from "../protocol/machine.js";
import { decimal, natural } from "../protocol/machine.js";
import type { AssetAppliers } from "./assets.js";
import type { IconFontRegistry } from "./icons.js";

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

const UNIT_EVENT: Value = {
  $: "record",
  fields: [],
};

const textEvent = (value: string): Value => ({
  $: "record",
  fields: [{
    name: "text",
    value: { $: "Text", value },
  }],
});

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

type ElementNode = Extract<RenderNode, { readonly kind: "element" }>;
type RendererMode = NonNullable<ProjectionRendererOptions["mode"]>;

const appliedAttributes = new WeakMap<Element, Set<string>>();
const listeners = new WeakMap<
  Element,
  readonly { readonly type: string; readonly listener: EventListener }[]
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

const SEMANTIC_PRIMITIVES = new Set([
  "view",
  "scroll",
  "pager",
  "text",
  "img",
  "video",
  "icon",
  "button",
  "textfield",
  "region",
]);

const isSemanticPrimitive = (element: string): boolean =>
  SEMANTIC_PRIMITIVES.has(element);

const attribute = (
  attributes: readonly RenderAttribute[],
  name: string,
): RenderAttributeValue | undefined =>
  attributes.find((candidate) => candidate.name === name)?.value;

const booleanAttribute = (
  attributes: readonly RenderAttribute[],
  name: string,
): boolean | undefined => {
  const value = attribute(attributes, name);
  return typeof value === "boolean" ? value : undefined;
};

const textAttribute = (
  attributes: readonly RenderAttribute[],
  name: string,
): string | undefined => {
  const value = attribute(attributes, name);
  return typeof value === "string" ? value : undefined;
};

const physicalAttribute = (
  name: string,
  value: RenderAttributeValue | undefined,
): RenderAttribute | null =>
  value === undefined ? null : { name, value };

const presentBooleanAttribute = (
  name: string,
  value: boolean | undefined,
): RenderAttribute | null =>
  value === true ? { name, value: true } : null;

const primitiveClass = (node: ElementNode): string => {
  const authored = textAttribute(node.attributes, "class")?.trim();
  return `uh-${node.element}${authored ? ` ${authored}` : ""}`;
};

/**
 * Converts the closed Uhura primitive vocabulary into physical DOM
 * attributes. Non-primitive HTML nodes deliberately bypass this translation.
 */
const physicalAttributes = (
  node: ElementNode,
  mode: RendererMode,
  listItem: boolean,
): readonly RenderAttribute[] => {
  if (!isSemanticPrimitive(node.element)) {
    const authoredSurfaceClass =
      textAttribute(node.attributes, "class")?.trim();
    const passthrough = node.surface
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
    if (!listItem) return passthrough;
    return [
      ...passthrough.filter((candidate) => candidate.name !== "role"),
      { name: "role", value: "listitem" },
    ];
  }

  const projected: Array<RenderAttribute | null> = [
    { name: "class", value: primitiveClass(node) },
  ];
  switch (node.element) {
    case "view": {
      const role = textAttribute(node.attributes, "role");
      projected.push(
        role === "list" || role === "navigation" || role === "tablist"
          ? { name: "role", value: role }
          : null,
      );
      break;
    }
    case "scroll":
      projected.push({
        name: "data-direction",
        value: textAttribute(node.attributes, "direction") ?? "vertical",
      });
      break;
    case "pager":
      projected.push(
        { name: "role", value: "group" },
        physicalAttribute(
          "aria-label",
          textAttribute(node.attributes, "label"),
        ),
      );
      break;
    case "img":
      projected.push({
        name: "alt",
        value: booleanAttribute(node.attributes, "decorative") === true
          ? ""
          : textAttribute(node.attributes, "alt") ?? "",
      });
      break;
    case "video": {
      const play = mode === "play";
      projected.push(
        physicalAttribute(
          "aria-label",
          textAttribute(node.attributes, "label"),
        ),
        presentBooleanAttribute(
          "autoplay",
          play && booleanAttribute(node.attributes, "autoplay") === true,
        ),
        presentBooleanAttribute(
          "muted",
          play && booleanAttribute(node.attributes, "muted") === true,
        ),
        presentBooleanAttribute(
          "loop",
          play && booleanAttribute(node.attributes, "loop") === true,
        ),
        presentBooleanAttribute(
          "controls",
          play && booleanAttribute(node.attributes, "controls") === true,
        ),
        presentBooleanAttribute(
          "playsinline",
          play && booleanAttribute(node.attributes, "playsinline") === true,
        ),
        mode === "editor"
          ? { name: "data-video-preview", value: "poster" }
          : null,
      );
      break;
    }
    case "icon":
      projected.push({ name: "aria-hidden", value: "true" });
      break;
    case "button": {
      const pressed =
        booleanAttribute(node.attributes, "pressed")
        ?? booleanAttribute(node.attributes, "aria-pressed");
      projected.push(
        { name: "type", value: "button" },
        physicalAttribute(
          "aria-label",
          textAttribute(node.attributes, "label")
            ?? textAttribute(node.attributes, "aria-label"),
        ),
        booleanAttribute(node.attributes, "busy") === true
          ? { name: "aria-busy", value: "true" }
          : null,
        pressed === undefined
          ? null
          : { name: "aria-pressed", value: String(pressed) },
        booleanAttribute(node.attributes, "current") === true
          ? { name: "aria-current", value: "true" }
          : null,
        presentBooleanAttribute(
          "disabled",
          booleanAttribute(node.attributes, "disabled"),
        ),
      );
      break;
    }
    case "region":
      projected.push(
        { name: "role", value: "button" },
        mode === "play" ? { name: "tabindex", value: "0" } : null,
        physicalAttribute(
          "aria-label",
          textAttribute(node.attributes, "label"),
        ),
      );
      break;
    case "textfield":
    case "text":
      break;
  }
  if (listItem) {
    const roleIndex = projected.findIndex(
      (candidate) => candidate?.name === "role",
    );
    if (roleIndex >= 0) {
      projected[roleIndex] = { name: "role", value: "listitem" };
    } else {
      projected.push({ name: "role", value: "listitem" });
    }
  }
  return projected.filter(
    (candidate): candidate is RenderAttribute => candidate !== null,
  );
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

const attributeValue = (
  attributes: readonly RenderAttribute[],
  name: string,
): string | undefined => textAttribute(attributes, name);

const applyCapabilities = (
  element: HTMLElement,
  node: ElementNode,
  options: ProjectionRendererOptions,
  mode: RendererMode,
): void => {
  if (node.element === "img") {
    const image = element as HTMLImageElement;
    if (options.assets) {
      options.assets.applyImage(
        image,
        attributeValue(node.attributes, "src"),
      );
    } else {
      image.removeAttribute("src");
    }
  }
  if (node.element === "video") {
    const video = element as HTMLVideoElement;
    const play = mode === "play";
    const autoplay =
      play && booleanAttribute(node.attributes, "autoplay") === true;
    const muted = play && booleanAttribute(node.attributes, "muted") === true;
    const loop = play && booleanAttribute(node.attributes, "loop") === true;
    const controls =
      play && booleanAttribute(node.attributes, "controls") === true;
    const playsInline =
      play && booleanAttribute(node.attributes, "playsinline") === true;
    if (video.autoplay !== autoplay) video.autoplay = autoplay;
    if (video.muted !== muted) video.muted = muted;
    if (video.loop !== loop) video.loop = loop;
    if (video.controls !== controls) video.controls = controls;
    if (video.playsInline !== playsInline) video.playsInline = playsInline;
    if (options.assets) {
      options.assets.applyVideoSource(
        video,
        play ? attributeValue(node.attributes, "src") : undefined,
      );
      options.assets.applyVideoPoster(
        video,
        attributeValue(node.attributes, "poster"),
      );
    } else {
      video.removeAttribute("src");
      video.removeAttribute("poster");
    }
  }
  if (node.element === "icon") {
    const name = attributeValue(node.attributes, "name") ?? "";
    const family = attributeValue(node.attributes, "family");
    if (options.icons && name.length > 0) {
      const realizedFamily = family ?? options.icons.defaultFamily;
      if (
        element.dataset["icon"] !== name
        || element.dataset["iconFamily"] !== realizedFamily
        || element.dataset["iconResource"] !== options.icons.fingerprint
      ) {
        options.icons.apply(element, family, name);
        element.dataset["icon"] = name;
        element.dataset["iconFamily"] = realizedFamily;
        element.dataset["iconResource"] = options.icons.fingerprint;
      }
    } else {
      element.textContent = "";
      delete element.dataset["icon"];
      delete element.dataset["iconFamily"];
      delete element.dataset["iconResource"];
    }
  }
};

interface TextFieldState {
  composing: boolean;
  inFlight: number;
  stash: string | undefined;
}

const textFieldStates = new WeakMap<HTMLInputElement, TextFieldState>();

const textFieldState = (input: HTMLInputElement): TextFieldState => {
  let state = textFieldStates.get(input);
  if (!state) {
    state = { composing: false, inFlight: 0, stash: undefined };
    textFieldStates.set(input, state);
  }
  return state;
};

const dispatchTextFieldChange = (
  input: HTMLInputElement,
  binding: string,
  projectionRevision: NaturalText | undefined,
  dispatch: ProjectionRendererOptions["dispatch"],
): void => {
  const state = textFieldState(input);
  state.inFlight += 1;
  try {
    dispatch(binding, projectionRevision, textEvent(input.value));
  } finally {
    state.inFlight -= 1;
    if (state.inFlight === 0 && state.stash !== undefined) {
      if (state.stash !== input.value) input.value = state.stash;
      state.stash = undefined;
    }
  }
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

const applyEvents = (
  element: HTMLElement,
  node: ElementNode,
  events: readonly RenderEvent[],
  projectionRevision: NaturalText | undefined,
  options: ProjectionRendererOptions,
  mode: RendererMode,
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

  const next: Array<{
    readonly type: string;
    readonly listener: EventListener;
  }> = [];
  for (const [eventIndex, { event }] of events.entries()) {
    if (event === "near-end") {
      continue;
    }
    if (node.element === "textfield" && event === "change") {
      const input = element as HTMLInputElement;
      const onInput: EventListener = (domEvent) => {
        const composing =
          textFieldState(input).composing
          || ("isComposing" in domEvent && domEvent.isComposing === true);
        if (!composing && eventAllowed(input)) {
          const configuration = eventConfigurations.get(element);
          const current = configuration?.events[eventIndex];
          if (configuration && current?.event === event) {
            dispatchTextFieldChange(
              input,
              current.binding,
              configuration.projectionRevision,
              configuration.dispatch,
            );
          }
        }
      };
      const onCompositionStart: EventListener = () => {
        textFieldState(input).composing = true;
      };
      const onCompositionEnd: EventListener = () => {
        const state = textFieldState(input);
        state.composing = false;
        if (eventAllowed(input)) {
          const configuration = eventConfigurations.get(element);
          const current = configuration?.events[eventIndex];
          if (configuration && current?.event === event) {
            dispatchTextFieldChange(
              input,
              current.binding,
              configuration.projectionRevision,
              configuration.dispatch,
            );
          }
        }
      };
      element.addEventListener("input", onInput);
      element.addEventListener("compositionstart", onCompositionStart);
      element.addEventListener("compositionend", onCompositionEnd);
      next.push(
        { type: "input", listener: onInput },
        { type: "compositionstart", listener: onCompositionStart },
        { type: "compositionend", listener: onCompositionEnd },
      );
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
      const configuration = eventConfigurations.get(element);
      const current = configuration?.events[eventIndex];
      if (configuration && current?.event === event) {
        configuration.dispatch(
          current.binding,
          configuration.projectionRevision,
          eventValue(element, event),
        );
      }
    };
    element.addEventListener(type, listener);
    next.push({ type, listener });
  }
  if (node.element === "region") {
    let activationIndex = events.findIndex(
      (candidate) => candidate.event === "activate",
    );
    if (activationIndex < 0) {
      activationIndex = events.findIndex(
        (candidate) => candidate.event === "press",
      );
    }
    if (activationIndex < 0) {
      activationIndex = events.findIndex(
        (candidate) => candidate.event === "activate-double",
      );
    }
    const activation = events[activationIndex];
    if (activation) {
      const listener: EventListener = (domEvent) => {
        if (
          !(domEvent instanceof KeyboardEvent)
          || (domEvent.key !== "Enter" && domEvent.key !== " ")
          || !eventAllowed(element)
        ) {
          return;
        }
        domEvent.preventDefault();
        const configuration = eventConfigurations.get(element);
        const current = configuration?.events[activationIndex];
        if (configuration && current?.event === activation.event) {
          configuration.dispatch(
            current.binding,
            configuration.projectionRevision,
            UNIT_EVENT,
          );
        }
      };
      element.addEventListener("keydown", listener);
      next.push({ type: "keydown", listener });
    }
  }
  listeners.set(element, next);
};

interface NearEndObservation {
  readonly observer: IntersectionObserver;
  readonly sentinel: HTMLElement;
  binding: string;
  projectionRevision: NaturalText | undefined;
  dispatch: ProjectionRendererOptions["dispatch"];
  armed: boolean;
}

const nearEndObservations = new WeakMap<HTMLElement, NearEndObservation>();
const activeNearEndElements = new Set<HTMLElement>();

const disposeNearEnd = (element: HTMLElement): void => {
  const observation = nearEndObservations.get(element);
  if (!observation) return;
  observation.observer.disconnect();
  observation.sentinel.remove();
  nearEndObservations.delete(element);
  activeNearEndElements.delete(element);
};

const disposeMechanics = (root: HTMLElement): void => {
  for (const element of [...activeNearEndElements]) {
    if (element === root || root.contains(element)) disposeNearEnd(element);
  }
};

const syncNearEnd = (
  element: HTMLElement,
  events: readonly RenderEvent[],
  projectionRevision: NaturalText | undefined,
  dispatch: ProjectionRendererOptions["dispatch"],
): void => {
  const binding = events.find((event) => event.event === "near-end")?.binding;
  const current = nearEndObservations.get(element);
  if (!binding) {
    disposeNearEnd(element);
    return;
  }
  if (current) {
    current.binding = binding;
    current.projectionRevision = projectionRevision;
    current.dispatch = dispatch;
    if (current.sentinel !== element.lastElementChild) {
      element.append(current.sentinel);
    }
    return;
  }
  const Observer = element.ownerDocument.defaultView?.IntersectionObserver;
  if (!Observer) return;
  const sentinel = element.ownerDocument.createElement("div");
  sentinel.dataset["uhMechanic"] = "near-end";
  sentinel.style.cssText = "block-size:1px;flex:none;";
  const observation: NearEndObservation = {
    sentinel,
    binding,
    projectionRevision,
    dispatch,
    armed: true,
    observer: new Observer((entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting && observation.armed) {
          observation.armed = false;
          observation.dispatch(
            observation.binding,
            observation.projectionRevision,
            UNIT_EVENT,
          );
        } else if (!entry.isIntersecting) {
          observation.armed = true;
        }
      }
    }, { root: element, rootMargin: "100%" }),
  };
  nearEndObservations.set(element, observation);
  activeNearEndElements.add(element);
  element.append(sentinel);
  observation.observer.observe(sentinel);
};

const isInputElement = (element: Element): element is HTMLInputElement =>
  element.localName === "input";

const directMechanic = (
  element: HTMLElement,
  mechanic: string,
): HTMLElement | undefined =>
  Array.from(element.children).find(
    (child): child is HTMLElement =>
      isHtmlElement(child) && child.dataset["uhMechanic"] === mechanic,
  );

const ensureTextFieldInput = (
  wrapper: HTMLElement,
  node: ElementNode,
  mode: RendererMode,
): HTMLInputElement => {
  const existing = directMechanic(wrapper, "input");
  const input = existing && isInputElement(existing)
    ? existing
    : wrapper.ownerDocument.createElement("input");
  if (input !== existing) {
    input.type = "text";
    input.dataset["uhMechanic"] = "input";
    wrapper.append(input);
  }
  applyAttributes(input, [
    ...(textAttribute(node.attributes, "placeholder") === undefined
      ? []
      : [{
        name: "placeholder",
        value: textAttribute(node.attributes, "placeholder") ?? "",
      }]),
    ...(textAttribute(node.attributes, "label") === undefined
      ? []
      : [{
        name: "aria-label",
        value: textAttribute(node.attributes, "label") ?? "",
      }]),
    ...(booleanAttribute(node.attributes, "disabled") === true
      ? [{ name: "disabled", value: true }]
      : []),
  ]);
  const disabled = booleanAttribute(node.attributes, "disabled") === true;
  const readOnly = mode === "editor";
  if (input.disabled !== disabled) input.disabled = disabled;
  if (input.readOnly !== readOnly) input.readOnly = readOnly;

  const value = textAttribute(node.attributes, "value") ?? "";
  const state = textFieldState(input);
  if (mode === "editor") {
    state.stash = undefined;
    if (input.value !== value) input.value = value;
  } else if (state.composing || state.inFlight > 0) {
    state.stash = value;
  } else {
    state.stash = undefined;
    if (input.value !== value) input.value = value;
  }
  return input;
};

const wiredPagerTracks = new WeakSet<HTMLElement>();

const updatePagerDots = (
  pager: HTMLElement,
  track: HTMLElement,
): void => {
  const dots = directMechanic(pager, "dots");
  if (!dots) return;
  const width = track.clientWidth || 1;
  const active = Math.min(
    dots.children.length - 1,
    Math.max(0, Math.round(track.scrollLeft / width)),
  );
  Array.from(dots.children).forEach((dot, index) => {
    dot.classList.toggle("on", index === active);
  });
};

const ensurePagerTrack = (
  pager: HTMLElement,
  node: ElementNode,
): HTMLElement => {
  let track = directMechanic(pager, "track");
  if (!track) {
    track = pager.ownerDocument.createElement("div");
    track.className = "uh-track";
    track.dataset["uhMechanic"] = "track";
    pager.append(track);
  }
  if (!wiredPagerTracks.has(track)) {
    track.addEventListener(
      "scroll",
      () => updatePagerDots(pager, track),
      { passive: true },
    );
    wiredPagerTracks.add(track);
  }

  if (textAttribute(node.attributes, "indicator") === "dots") {
    let dots = directMechanic(pager, "dots");
    if (!dots) {
      dots = pager.ownerDocument.createElement("div");
      dots.className = "uh-dots";
      dots.dataset["uhMechanic"] = "dots";
      dots.setAttribute("aria-hidden", "true");
      pager.append(dots);
    }
    while (dots.children.length > node.children.length) {
      dots.lastElementChild?.remove();
    }
    while (dots.children.length < node.children.length) {
      const dot = pager.ownerDocument.createElement("span");
      dot.className = "uh-dot";
      dots.append(dot);
    }
  } else {
    directMechanic(pager, "dots")?.remove();
  }
  return track;
};

const htmlTag = (element: string): string => {
  switch (element.toLowerCase()) {
    case "view":
    case "scroll":
    case "pager":
    case "region":
    case "textfield":
      return "div";
    case "text":
      return "p";
    case "icon":
      return "span";
    default:
      return element.toLowerCase();
  }
};

const applyStaticScrollPosition = (
  element: HTMLElement,
  node: Extract<RenderNode, { readonly kind: "element" }>,
  options: ProjectionRendererOptions,
): void => {
  if (options.mode !== "editor" || node.element !== "scroll") return;
  const raw = attributeValue(node.attributes, "position");
  if (raw === undefined) return;
  const position = Number(raw);
  if (!Number.isFinite(position) || position < 0 || position > 1) return;
  const direction = attributeValue(node.attributes, "direction") ?? "vertical";
  if (direction === "horizontal") {
    element.scrollLeft = Math.round(
      Math.max(0, element.scrollWidth - element.clientWidth) * position,
    );
  } else {
    element.scrollTop = Math.round(
      Math.max(0, element.scrollHeight - element.clientHeight) * position,
    );
  }
};

const elementFor = (
  document: Document,
  node: ElementNode,
): HTMLElement => {
  const existingName = htmlTag(node.element);
  const element =
    existingName === "dialog"
      ? document.createElement("dialog")
      : document.createElement(existingName);
  element.dataset["uhuraKey"] = node.key;
  semanticElements.set(element, node.element);
  if (element.localName === "button") (element as HTMLButtonElement).type = "button";
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

const childHost = (
  element: HTMLElement,
  node: ElementNode,
  mode: RendererMode,
): {
  readonly children: HTMLElement | null;
  readonly events: HTMLElement;
} => {
  switch (node.element) {
    case "textfield": {
      const input = ensureTextFieldInput(element, node, mode);
      return { children: null, events: input };
    }
    case "pager":
      return {
        children: ensurePagerTrack(element, node),
        events: element,
      };
    case "img":
    case "video":
    case "icon":
    case "input":
      return { children: null, events: element };
    default:
      return { children: element, events: element };
  }
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
        child.localName !== htmlTag(node.element)
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
      if (node.element === "button") {
        const button = child as HTMLButtonElement;
        const disabled =
          booleanAttribute(node.attributes, "disabled") === true;
        if (button.disabled !== disabled) button.disabled = disabled;
      }
      applyCapabilities(child, node, options, mode);
      const hosts = childHost(child, node, mode);
      applyEvents(
        hosts.events,
        node,
        node.events,
        projectionRevision,
        options,
        mode,
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
          node.element === "view"
            && textAttribute(node.attributes, "role") === "list",
        );
      }
      if (node.element === "pager") {
        updatePagerDots(child, hosts.children ?? child);
      }
      if (node.element === "scroll" && mode === "play") {
        syncNearEnd(
          child,
          node.events,
          projectionRevision,
          options.dispatch,
        );
      } else {
        disposeNearEnd(child);
      }
      applyStaticScrollPosition(child, node, options);
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
      if (isHtmlElement(child)) disposeMechanics(child);
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
      disposeMechanics(options.root);
      options.root.replaceChildren();
    },
  };
};
