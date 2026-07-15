import type {
  Descriptor,
  InteractionGraph,
  Snapshot,
  SurfaceView,
  VNode,
  VValue,
} from "../protocol/types.js";

export const EDITOR_STATE_PROTOCOL = "uhura-editor-state/1" as const;
export const EDITOR_EVENT_PROTOCOL = "uhura-editor-event/0" as const;
export const INTERACTION_GRAPH_PROTOCOL = "uhura-interaction-graph/0" as const;

export type PreviewKind = "page" | "surface" | "component";
export type PreviewFreshness = "current" | "stale";
export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface PreviewIdentity {
  kind: PreviewKind;
  subject: string;
  example: string;
}

export interface PreviewGroup {
  id: string;
  kind: PreviewKind;
  subject: string;
  previews: string[];
}

export type PreviewDataSource =
  | {
    kind: "inline";
    declaredIn: string | null;
    timeline: boolean;
  }
  | {
    kind: "fixture" | "automatic-fixture";
    declaredIn: string | null;
    timeline: boolean;
    fixture: string;
    path: string[];
  };

export interface PreviewDataField {
  group: "properties" | "page-address" | "provided-data";
  name: string;
  key: JsonValue;
  status: "ready" | "waiting" | "failed";
  value?: JsonValue;
  reason?: string;
  source: PreviewDataSource | null;
}

export interface PreviewInteraction {
  nodeKey: string;
  element: string;
  kind: "input" | "observe";
  event: string;
  emit: string;
  scope: string;
  payload: JsonValue;
  carries: Record<string, string>;
}

export interface EditorSourcePosition {
  line: number;
  col: number;
}

export interface EditorSourceSpan {
  offset: number;
  len: number;
  start: EditorSourcePosition;
  end: EditorSourcePosition;
}

export type SourceTargetClass =
  | "source-module"
  | "component-declaration"
  | "page-declaration"
  | "surface-declaration"
  | "prop-declaration"
  | "emitted-event-declaration"
  | "emitted-event-parameter"
  | "route-parameter"
  | "store-scope"
  | "state-field"
  | "event-handler"
  | "outcome-handler"
  | "handler-parameter"
  | "example-declaration"
  | "catalog-element"
  | "component-invocation"
  | "if-block"
  | "each-block"
  | "match-block";

export type SourceTargetOwnerKind =
  | "module"
  | "examples"
  | "component"
  | "page"
  | "surface";

export interface SourceTargetOwner {
  kind: SourceTargetOwnerKind;
  name: string;
}

export interface SourceTarget {
  id: string;
  class: SourceTargetClass;
  file: string;
  span: EditorSourceSpan;
  label: string;
  owner: SourceTargetOwner;
}

export type SourceMetadataClass = "doc" | "annotation";

export interface SourceMetadataEntry {
  id: string;
  class: SourceMetadataClass;
  kind: string;
  text: string;
  span: EditorSourceSpan;
  targetId: string;
  order: number;
}

export interface AuthoringMetadata {
  targets: SourceTarget[];
  entries: SourceMetadataEntry[];
}

export interface PreviewDocumentation {
  declarationDocId: string | null;
  exampleDocId: string | null;
}

export type RenderRoot =
  | { kind: "page" }
  | { kind: "fragment" }
  | { kind: "surface"; key: string };

export interface RenderNodeRef {
  root: RenderRoot;
  path: number[];
}

export interface TargetOccurrence {
  id: string;
  targetId: string;
  anchors: RenderNodeRef[];
}

export interface PreviewProvenance {
  occurrences: TargetOccurrence[];
}

export interface ReplayGuard {
  handler: number;
  result: "satisfied" | "unsatisfied" | "not-ready";
}

export interface ReplayDispatch {
  scope: string;
  definition: string;
  on: string;
  guards: ReplayGuard[];
  selected: number | null;
  aborted: string | null;
}

export interface ReplayEffects {
  writes: JsonValue[];
  commands: JsonValue[];
  intents: JsonValue[];
  structural: JsonValue[];
  projections: JsonValue[];
}

export interface ReplayStep {
  label: string;
  kind: "semantic" | "outcome" | "projection";
  payload: JsonValue;
  dispatch: ReplayDispatch | null;
  effects: ReplayEffects;
}

export interface EditorPreview {
  id: string;
  identity: PreviewIdentity;
  sourceFile: string;
  default: boolean;
  pinned: boolean;
  derived: boolean;
  inFlight: number;
  from: string | null;
  replaySteps: string[];
  replay: ReplayStep[];
  note: string | null;
  data: PreviewDataField[];
  interactions: PreviewInteraction[];
  documentation: PreviewDocumentation;
  provenance: PreviewProvenance;
  content: Snapshot | VNode;
}

export interface EditorAsset {
  dataUri: string;
  alt: string;
}

interface EditorIconPaint {
  [property: string]: unknown;
  fill?: string;
  stroke?: string;
  strokeWidth?: string;
  lineCap?: "butt" | "round" | "square";
  lineJoin?: "miter" | "round" | "bevel";
  opacity?: string;
}

export type EditorIconCommand = EditorIconPaint & (
  | { kind: "path"; d: string }
  | { kind: "circle"; cx: string; cy: string; r: string }
  | {
    kind: "rect";
    x: string;
    y: string;
    width: string;
    height: string;
    rx?: string;
  }
);

export interface EditorIcon {
  viewBox: [number, number, number, number];
  commands: EditorIconCommand[];
}

export interface EditorRender {
  revision: number;
  freshness: PreviewFreshness;
  application: { name: string };
  authoring: AuthoringMetadata;
  groups: PreviewGroup[];
  previews: EditorPreview[];
  stylesheet: string;
  icons: Record<string, EditorIcon>;
  assets: Record<string, EditorAsset>;
  interactionGraph: InteractionGraph;
}

export interface EditorState {
  protocol: typeof EDITOR_STATE_PROTOCOL;
  sourceRevision: number;
  diagnostics: Record<string, JsonValue> | null;
  render: EditorRender | null;
}

export interface EditorRevisionEvent {
  protocol: typeof EDITOR_EVENT_PROTOCOL;
  sourceRevision: number;
}

export class EditorContractError extends Error {
  constructor(path: string, expectation: string) {
    super(`${path}: expected ${expectation}`);
    this.name = "EditorContractError";
  }
}

type UnknownRecord = Record<string, unknown>;

const record = (value: unknown, path: string): UnknownRecord => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new EditorContractError(path, "an object");
  }
  return value as UnknownRecord;
};

const exact = (value: UnknownRecord, path: string, allowed: readonly string[]): void => {
  const keys = Object.keys(value);
  const unexpected = keys.find((key) => !allowed.includes(key));
  if (unexpected !== undefined) {
    throw new EditorContractError(`${path}.${unexpected}`, "no unknown property");
  }
};

const array = (value: unknown, path: string): unknown[] => {
  if (!Array.isArray(value)) throw new EditorContractError(path, "an array");
  return value;
};

const string = (value: unknown, path: string, allowEmpty = false): string => {
  if (typeof value !== "string" || (!allowEmpty && value.length === 0)) {
    throw new EditorContractError(path, allowEmpty ? "a string" : "a non-empty string");
  }
  return value;
};

const optionalString = (value: unknown, path: string): string | undefined =>
  value === undefined ? undefined : string(value, path, true);

const nullableString = (value: unknown, path: string): string | null =>
  value === null ? null : string(value, path, true);

const boolean = (value: unknown, path: string): boolean => {
  if (typeof value !== "boolean") throw new EditorContractError(path, "a boolean");
  return value;
};

const finiteNumber = (value: unknown, path: string): number => {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new EditorContractError(path, "a finite number");
  }
  return value;
};

const positiveRevision = (value: unknown, path: string): number => {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 1) {
    throw new EditorContractError(path, "an integer at least 1");
  }
  return value;
};

const positiveInteger = (value: unknown, path: string): number => {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 1) {
    throw new EditorContractError(path, "a positive integer");
  }
  return value;
};

const nonNegativeInteger = (value: unknown, path: string): number => {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new EditorContractError(path, "a non-negative integer");
  }
  return value;
};

const nullableNonNegativeInteger = (value: unknown, path: string): number | null =>
  value === null ? null : nonNegativeInteger(value, path);

const oneOf = <T extends string>(
  value: unknown,
  path: string,
  choices: readonly T[],
): T => {
  if (typeof value !== "string" || !choices.includes(value as T)) {
    throw new EditorContractError(path, choices.map((choice) => JSON.stringify(choice)).join(" or "));
  }
  return value as T;
};

const jsonValue = (value: unknown, path: string): JsonValue => {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return finiteNumber(value, path);
  if (Array.isArray(value)) {
    return value.map((item, index) => jsonValue(item, `${path}[${index}]`));
  }
  const object = record(value, path);
  return Object.fromEntries(
    Object.entries(object).map(([key, item]) => [key, jsonValue(item, `${path}.${key}`)]),
  );
};

const jsonRecord = (value: unknown, path: string): Record<string, JsonValue> => {
  const object = record(value, path);
  return Object.fromEntries(
    Object.entries(object).map(([key, item]) => [key, jsonValue(item, `${path}.${key}`)]),
  );
};

const previewKind = (value: unknown, path: string): PreviewKind =>
  oneOf(value, path, ["page", "surface", "component"]);

const sourceTargetClasses = [
  "source-module",
  "component-declaration",
  "page-declaration",
  "surface-declaration",
  "prop-declaration",
  "emitted-event-declaration",
  "emitted-event-parameter",
  "route-parameter",
  "store-scope",
  "state-field",
  "event-handler",
  "outcome-handler",
  "handler-parameter",
  "example-declaration",
  "catalog-element",
  "component-invocation",
  "if-block",
  "each-block",
  "match-block",
] as const satisfies readonly SourceTargetClass[];

const annotatableTargetClasses = new Set<SourceTargetClass>([
  "catalog-element",
  "component-invocation",
  "if-block",
  "each-block",
  "match-block",
]);

const sourcePosition = (value: unknown, path: string): EditorSourcePosition => {
  const object = record(value, path);
  exact(object, path, ["line", "col"]);
  return {
    line: positiveInteger(object["line"], `${path}.line`),
    col: positiveInteger(object["col"], `${path}.col`),
  };
};

const sourceSpan = (value: unknown, path: string): EditorSourceSpan => {
  const object = record(value, path);
  exact(object, path, ["offset", "len", "start", "end"]);
  const offset = nonNegativeInteger(object["offset"], `${path}.offset`);
  const len = nonNegativeInteger(object["len"], `${path}.len`);
  if (!Number.isSafeInteger(offset + len)) {
    throw new EditorContractError(path, "a byte range within safe integer bounds");
  }
  const start = sourcePosition(object["start"], `${path}.start`);
  const end = sourcePosition(object["end"], `${path}.end`);
  const endBeforeStart = end.line < start.line
    || (end.line === start.line && end.col < start.col);
  const samePosition = start.line === end.line && start.col === end.col;
  if (endBeforeStart || ((len === 0) !== samePosition)) {
    throw new EditorContractError(path, "a consistent half-open source range");
  }
  return { offset, len, start, end };
};

const canonicalSourcePath = (value: unknown, path: string): string => {
  const sourcePath = string(value, path);
  if (
    sourcePath.startsWith("/")
    || sourcePath.includes("\\")
    || sourcePath.split("/").some((part) => part === "" || part === "." || part === "..")
  ) {
    throw new EditorContractError(path, "a canonical project-relative source path");
  }
  return sourcePath;
};

const sourceTarget = (value: unknown, path: string): SourceTarget => {
  const object = record(value, path);
  exact(object, path, ["id", "class", "file", "span", "label", "owner"]);
  const file = canonicalSourcePath(object["file"], `${path}.file`);
  const owner = record(object["owner"], `${path}.owner`);
  exact(owner, `${path}.owner`, ["kind", "name"]);
  const ownerKind = oneOf(
    owner["kind"],
    `${path}.owner.kind`,
    ["module", "examples", "component", "page", "surface"],
  );
  const ownerName = string(owner["name"], `${path}.owner.name`);
  if ((ownerKind === "module" || ownerKind === "examples") && ownerName !== file) {
    throw new EditorContractError(`${path}.owner.name`, `the source file ${JSON.stringify(file)}`);
  }
  return {
    id: string(object["id"], `${path}.id`),
    class: oneOf(object["class"], `${path}.class`, sourceTargetClasses),
    file,
    span: sourceSpan(object["span"], `${path}.span`),
    label: string(object["label"], `${path}.label`),
    owner: { kind: ownerKind, name: ownerName },
  };
};

const sourceMetadataEntry = (value: unknown, path: string): SourceMetadataEntry => {
  const object = record(value, path);
  exact(object, path, ["id", "class", "kind", "text", "span", "targetId", "order"]);
  const span = sourceSpan(object["span"], `${path}.span`);
  if (span.len === 0) throw new EditorContractError(`${path}.span`, "a non-empty source range");
  return {
    id: string(object["id"], `${path}.id`),
    class: oneOf(object["class"], `${path}.class`, ["doc", "annotation"]),
    kind: string(object["kind"], `${path}.kind`),
    text: string(object["text"], `${path}.text`),
    span,
    targetId: string(object["targetId"], `${path}.targetId`),
    order: nonNegativeInteger(object["order"], `${path}.order`),
  };
};

const authoringMetadata = (value: unknown, path: string): AuthoringMetadata => {
  const object = record(value, path);
  exact(object, path, ["targets", "entries"]);
  return {
    targets: array(object["targets"], `${path}.targets`).map((item, index) =>
      sourceTarget(item, `${path}.targets[${index}]`)),
    entries: array(object["entries"], `${path}.entries`).map((item, index) =>
      sourceMetadataEntry(item, `${path}.entries[${index}]`)),
  };
};

const previewDocumentation = (value: unknown, path: string): PreviewDocumentation => {
  const object = record(value, path);
  exact(object, path, ["declarationDocId", "exampleDocId"]);
  return {
    declarationDocId: nullableString(object["declarationDocId"], `${path}.declarationDocId`),
    exampleDocId: nullableString(object["exampleDocId"], `${path}.exampleDocId`),
  };
};

const renderRoot = (value: unknown, path: string): RenderRoot => {
  const object = record(value, path);
  const kind = oneOf(object["kind"], `${path}.kind`, ["page", "fragment", "surface"]);
  if (kind === "surface") {
    exact(object, path, ["kind", "key"]);
    return { kind, key: string(object["key"], `${path}.key`) };
  }
  exact(object, path, ["kind"]);
  return { kind };
};

const renderNodeRef = (value: unknown, path: string): RenderNodeRef => {
  const object = record(value, path);
  exact(object, path, ["root", "path"]);
  return {
    root: renderRoot(object["root"], `${path}.root`),
    path: array(object["path"], `${path}.path`).map((item, index) =>
      nonNegativeInteger(item, `${path}.path[${index}]`)),
  };
};

const targetOccurrence = (value: unknown, path: string): TargetOccurrence => {
  const object = record(value, path);
  exact(object, path, ["id", "targetId", "anchors"]);
  return {
    id: string(object["id"], `${path}.id`),
    targetId: string(object["targetId"], `${path}.targetId`),
    anchors: array(object["anchors"], `${path}.anchors`).map((item, index) =>
      renderNodeRef(item, `${path}.anchors[${index}]`)),
  };
};

const previewProvenance = (value: unknown, path: string): PreviewProvenance => {
  const object = record(value, path);
  exact(object, path, ["occurrences"]);
  return {
    occurrences: array(object["occurrences"], `${path}.occurrences`).map((item, index) =>
      targetOccurrence(item, `${path}.occurrences[${index}]`)),
  };
};

const descriptor = (value: unknown, path: string): Descriptor => {
  const object = record(value, path);
  exact(object, path, ["kind", "event", "emit", "scope", "payload", "carries"]);
  const carriesValue = object["carries"];
  let carries: Record<string, "text" | "bool" | "int"> | undefined;
  if (carriesValue !== undefined) {
    carries = Object.fromEntries(Object.entries(record(carriesValue, `${path}.carries`)).map(
      ([key, item]) => [key, oneOf(item, `${path}.carries.${key}`, ["text", "bool", "int"])],
    ));
  }
  return {
    kind: oneOf(object["kind"], `${path}.kind`, ["input", "observe"]),
    event: string(object["event"], `${path}.event`),
    emit: string(object["emit"], `${path}.emit`),
    scope: string(object["scope"], `${path}.scope`),
    payload: jsonValue(object["payload"], `${path}.payload`),
    ...(carries === undefined ? {} : { carries }),
  };
};

const vValue = (value: unknown, path: string): VValue => {
  if (typeof value === "boolean" || typeof value === "string") return value;
  if (typeof value === "number") return finiteNumber(value, path);
  const object = record(value, path);
  const tag = object["t"];
  if (tag === "plain") {
    exact(object, path, ["t", "v"]);
    return { t: "plain", v: string(object["v"], `${path}.v`, true) };
  }
  if (tag === "image") {
    exact(object, path, ["t", "asset"]);
    return { t: "image", asset: string(object["asset"], `${path}.asset`) };
  }
  throw new EditorContractError(path, "a valid Uhura property value");
};

const vnode = (value: unknown, path: string): VNode => {
  const object = record(value, path);
  exact(object, path, ["key", "element", "class", "props", "children", "on"]);
  const props = Object.fromEntries(Object.entries(record(object["props"], `${path}.props`)).map(
    ([key, item]) => [key, vValue(item, `${path}.props.${key}`)],
  ));
  const childrenValue = object["children"];
  const onValue = object["on"];
  const children = childrenValue === undefined
    ? undefined
    : array(childrenValue, `${path}.children`).map((item, index) =>
      vnode(item, `${path}.children[${index}]`));
  const on = onValue === undefined
    ? undefined
    : array(onValue, `${path}.on`).map((item, index) =>
      descriptor(item, `${path}.on[${index}]`));
  return {
    key: string(object["key"], `${path}.key`),
    element: string(object["element"], `${path}.element`),
    props,
    ...(object["class"] === undefined
      ? {}
      : { class: string(object["class"], `${path}.class`, true) }),
    ...(children === undefined ? {} : { children }),
    ...(on === undefined ? {} : { on }),
  };
};

const surface = (value: unknown, path: string): SurfaceView => {
  const object = record(value, path);
  exact(object, path, ["key", "definition", "modality", "restore-focus", "dismiss", "root"]);
  return {
    key: string(object["key"], `${path}.key`),
    definition: string(object["definition"], `${path}.definition`),
    modality: string(object["modality"], `${path}.modality`),
    ...(object["restore-focus"] === undefined
      ? {}
      : { "restore-focus": string(object["restore-focus"], `${path}.restore-focus`) }),
    dismiss: descriptor(object["dismiss"], `${path}.dismiss`),
    root: vnode(object["root"], `${path}.root`),
  };
};

const snapshot = (value: UnknownRecord, path: string): Snapshot => {
  exact(value, path, ["protocol", "revision", "page", "surfaces"]);
  if (value["protocol"] !== "uhura-view/0") {
    throw new EditorContractError(`${path}.protocol`, JSON.stringify("uhura-view/0"));
  }
  const page = record(value["page"], `${path}.page`);
  exact(page, `${path}.page`, ["route", "root"]);
  return {
    protocol: "uhura-view/0",
    revision: nonNegativeInteger(value["revision"], `${path}.revision`),
    page: {
      route: string(page["route"], `${path}.page.route`, true),
      root: vnode(page["root"], `${path}.page.root`),
    },
    surfaces: array(value["surfaces"], `${path}.surfaces`).map((item, index) =>
      surface(item, `${path}.surfaces[${index}]`)),
  };
};

const content = (value: unknown, path: string): Snapshot | VNode => {
  const object = record(value, path);
  return object["protocol"] === "uhura-view/0"
    ? snapshot(object, path)
    : vnode(object, path);
};

const isSnapshotContent = (value: Snapshot | VNode): value is Snapshot =>
  "protocol" in value && value.protocol === "uhura-view/0";

const dataSource = (value: unknown, path: string): PreviewDataSource | null => {
  if (value === null) return null;
  const object = record(value, path);
  const kind = oneOf(object["kind"], `${path}.kind`, ["inline", "fixture", "automatic-fixture"]);
  if (kind === "inline") {
    exact(object, path, ["kind", "declaredIn", "timeline"]);
    return {
      kind,
      declaredIn: nullableString(object["declaredIn"], `${path}.declaredIn`),
      timeline: boolean(object["timeline"], `${path}.timeline`),
    };
  }
  exact(object, path, ["kind", "declaredIn", "timeline", "fixture", "path"]);
  return {
    kind,
    declaredIn: nullableString(object["declaredIn"], `${path}.declaredIn`),
    timeline: boolean(object["timeline"], `${path}.timeline`),
    fixture: string(object["fixture"], `${path}.fixture`),
    path: array(object["path"], `${path}.path`).map((item, index) =>
      string(item, `${path}.path[${index}]`, true)),
  };
};

const dataField = (value: unknown, path: string): PreviewDataField => {
  const object = record(value, path);
  exact(object, path, ["group", "name", "key", "status", "value", "reason", "source"]);
  const status = oneOf(object["status"], `${path}.status`, ["ready", "waiting", "failed"]);
  const valueValue = object["value"];
  const reason = optionalString(object["reason"], `${path}.reason`);
  if (status === "ready" && valueValue === undefined) {
    throw new EditorContractError(`${path}.value`, "a JSON value for ready data");
  }
  if (status !== "ready" && valueValue !== undefined) {
    throw new EditorContractError(`${path}.value`, "no value unless status is ready");
  }
  if (status === "failed" && reason === undefined) {
    throw new EditorContractError(`${path}.reason`, "a failure reason");
  }
  if (status !== "failed" && reason !== undefined) {
    throw new EditorContractError(`${path}.reason`, "no reason unless status is failed");
  }
  return {
    group: oneOf(object["group"], `${path}.group`, ["properties", "page-address", "provided-data"]),
    name: string(object["name"], `${path}.name`),
    key: jsonValue(object["key"], `${path}.key`),
    status,
    ...(valueValue === undefined ? {} : { value: jsonValue(valueValue, `${path}.value`) }),
    ...(reason === undefined ? {} : { reason }),
    source: dataSource(object["source"], `${path}.source`),
  };
};

const interaction = (value: unknown, path: string): PreviewInteraction => {
  const object = record(value, path);
  exact(object, path, [
    "nodeKey", "element", "kind", "event", "emit", "scope", "payload", "carries",
  ]);
  const carries = Object.fromEntries(Object.entries(record(object["carries"], `${path}.carries`)).map(
    ([key, item]) => [key, string(item, `${path}.carries.${key}`)],
  ));
  return {
    nodeKey: string(object["nodeKey"], `${path}.nodeKey`),
    element: string(object["element"], `${path}.element`),
    kind: oneOf(object["kind"], `${path}.kind`, ["input", "observe"]),
    event: string(object["event"], `${path}.event`),
    emit: string(object["emit"], `${path}.emit`),
    scope: string(object["scope"], `${path}.scope`),
    payload: jsonValue(object["payload"], `${path}.payload`),
    carries,
  };
};

const replayDispatch = (value: unknown, path: string): ReplayDispatch | null => {
  if (value === null) return null;
  const object = record(value, path);
  exact(object, path, ["scope", "definition", "on", "guards", "selected", "aborted"]);
  return {
    scope: string(object["scope"], `${path}.scope`),
    definition: string(object["definition"], `${path}.definition`),
    on: string(object["on"], `${path}.on`),
    guards: array(object["guards"], `${path}.guards`).map((item, index) => {
      const guardPath = `${path}.guards[${index}]`;
      const guard = record(item, guardPath);
      exact(guard, guardPath, ["handler", "result"]);
      return {
        handler: nonNegativeInteger(guard["handler"], `${guardPath}.handler`),
        result: oneOf(guard["result"], `${guardPath}.result`, [
          "satisfied", "unsatisfied", "not-ready",
        ]),
      };
    }),
    selected: nullableNonNegativeInteger(object["selected"], `${path}.selected`),
    aborted: nullableString(object["aborted"], `${path}.aborted`),
  };
};

const replayStep = (value: unknown, path: string): ReplayStep => {
  const object = record(value, path);
  exact(object, path, ["label", "kind", "payload", "dispatch", "effects"]);
  const effectsPath = `${path}.effects`;
  const effects = record(object["effects"], effectsPath);
  exact(effects, effectsPath, ["writes", "commands", "intents", "structural", "projections"]);
  const effectList = (name: keyof ReplayEffects): JsonValue[] =>
    array(effects[name], `${effectsPath}.${name}`).map((item, index) =>
      jsonValue(item, `${effectsPath}.${name}[${index}]`));
  return {
    label: string(object["label"], `${path}.label`),
    kind: oneOf(object["kind"], `${path}.kind`, ["semantic", "outcome", "projection"]),
    payload: jsonValue(object["payload"], `${path}.payload`),
    dispatch: replayDispatch(object["dispatch"], `${path}.dispatch`),
    effects: {
      writes: effectList("writes"),
      commands: effectList("commands"),
      intents: effectList("intents"),
      structural: effectList("structural"),
      projections: effectList("projections"),
    },
  };
};

const identity = (value: unknown, path: string): PreviewIdentity => {
  const object = record(value, path);
  exact(object, path, ["kind", "subject", "example"]);
  return {
    kind: previewKind(object["kind"], `${path}.kind`),
    subject: string(object["subject"], `${path}.subject`),
    example: string(object["example"], `${path}.example`),
  };
};

const preview = (value: unknown, path: string): EditorPreview => {
  const object = record(value, path);
  exact(object, path, [
    "id", "identity", "sourceFile", "default", "pinned", "derived", "inFlight", "from", "note",
    "replaySteps", "replay", "data", "interactions", "documentation", "provenance", "content",
  ]);
  const previewIdentity = identity(object["identity"], `${path}.identity`);
  const previewContent = content(object["content"], `${path}.content`);
  if ((previewIdentity.kind === "page") !== isSnapshotContent(previewContent)) {
    throw new EditorContractError(
      `${path}.content`,
      previewIdentity.kind === "page" ? "an uhura-view/0 snapshot" : "a fragment VNode",
    );
  }
  const replaySteps = array(object["replaySteps"], `${path}.replaySteps`).map((item, index) =>
    string(item, `${path}.replaySteps[${index}]`));
  const replay = array(object["replay"], `${path}.replay`).map((item, index) =>
    replayStep(item, `${path}.replay[${index}]`));
  if (replay.length !== replaySteps.length
      || replay.some((step, index) => step.label !== replaySteps[index])) {
    throw new EditorContractError(`${path}.replay`, "details matching replaySteps in order");
  }
  return {
    id: string(object["id"], `${path}.id`),
    identity: previewIdentity,
    sourceFile: canonicalSourcePath(object["sourceFile"], `${path}.sourceFile`),
    default: boolean(object["default"], `${path}.default`),
    pinned: boolean(object["pinned"], `${path}.pinned`),
    derived: boolean(object["derived"], `${path}.derived`),
    inFlight: nonNegativeInteger(object["inFlight"], `${path}.inFlight`),
    from: nullableString(object["from"], `${path}.from`),
    replaySteps,
    replay,
    note: nullableString(object["note"], `${path}.note`),
    data: array(object["data"], `${path}.data`).map((item, index) =>
      dataField(item, `${path}.data[${index}]`)),
    interactions: array(object["interactions"], `${path}.interactions`).map((item, index) =>
      interaction(item, `${path}.interactions[${index}]`)),
    documentation: previewDocumentation(object["documentation"], `${path}.documentation`),
    provenance: previewProvenance(object["provenance"], `${path}.provenance`),
    content: previewContent,
  };
};

const group = (value: unknown, path: string): PreviewGroup => {
  const object = record(value, path);
  exact(object, path, ["id", "kind", "subject", "previews"]);
  return {
    id: string(object["id"], `${path}.id`),
    kind: previewKind(object["kind"], `${path}.kind`),
    subject: string(object["subject"], `${path}.subject`),
    previews: array(object["previews"], `${path}.previews`).map((item, index) =>
      string(item, `${path}.previews[${index}]`)),
  };
};

const icon = (value: unknown, path: string): EditorIcon => {
  const object = record(value, path);
  exact(object, path, ["viewBox", "commands"]);
  const viewBox = array(object["viewBox"], `${path}.viewBox`);
  if (viewBox.length !== 4) throw new EditorContractError(`${path}.viewBox`, "four numbers");
  const commands = array(object["commands"], `${path}.commands`).map((item, index): EditorIconCommand => {
    const commandPath = `${path}.commands[${index}]`;
    const command = record(item, commandPath);
    const kind = oneOf(command["kind"], `${commandPath}.kind`, ["path", "circle", "rect"]);
    const paintKeys = ["fill", "stroke", "strokeWidth", "lineCap", "lineJoin", "opacity"] as const;
    const paint: EditorIconPaint = {
      ...(command["fill"] === undefined ? {} : { fill: string(command["fill"], `${commandPath}.fill`) }),
      ...(command["stroke"] === undefined ? {} : { stroke: string(command["stroke"], `${commandPath}.stroke`) }),
      ...(command["strokeWidth"] === undefined
        ? {}
        : { strokeWidth: string(command["strokeWidth"], `${commandPath}.strokeWidth`) }),
      ...(command["lineCap"] === undefined
        ? {}
        : { lineCap: oneOf(
          command["lineCap"],
          `${commandPath}.lineCap`,
          ["butt", "round", "square"] as const,
        ) }),
      ...(command["lineJoin"] === undefined
        ? {}
        : { lineJoin: oneOf(
          command["lineJoin"],
          `${commandPath}.lineJoin`,
          ["miter", "round", "bevel"] as const,
        ) }),
      ...(command["opacity"] === undefined
        ? {}
        : { opacity: string(command["opacity"], `${commandPath}.opacity`) }),
    };
    if (kind === "path") {
      exact(command, commandPath, ["kind", "d", ...paintKeys]);
      return { kind, d: string(command["d"], `${commandPath}.d`), ...paint };
    }
    if (kind === "circle") {
      exact(command, commandPath, ["kind", "cx", "cy", "r", ...paintKeys]);
      return {
        kind,
        cx: string(command["cx"], `${commandPath}.cx`),
        cy: string(command["cy"], `${commandPath}.cy`),
        r: string(command["r"], `${commandPath}.r`),
        ...paint,
      };
    }
    exact(command, commandPath, ["kind", "x", "y", "width", "height", "rx", ...paintKeys]);
    return {
      kind,
      x: string(command["x"], `${commandPath}.x`),
      y: string(command["y"], `${commandPath}.y`),
      width: string(command["width"], `${commandPath}.width`),
      height: string(command["height"], `${commandPath}.height`),
      ...(command["rx"] === undefined ? {} : { rx: string(command["rx"], `${commandPath}.rx`) }),
      ...paint,
    };
  });
  const viewBoxNumber = (item: unknown, itemPath: string): number => {
    const number = finiteNumber(item, itemPath);
    if (!Number.isSafeInteger(number)) throw new EditorContractError(itemPath, "an integer");
    return number;
  };
  return {
    viewBox: [
      viewBoxNumber(viewBox[0], `${path}.viewBox[0]`),
      viewBoxNumber(viewBox[1], `${path}.viewBox[1]`),
      viewBoxNumber(viewBox[2], `${path}.viewBox[2]`),
      viewBoxNumber(viewBox[3], `${path}.viewBox[3]`),
    ],
    commands,
  };
};

const asset = (value: unknown, path: string): EditorAsset => {
  const object = record(value, path);
  exact(object, path, ["dataUri", "alt"]);
  return {
    dataUri: string(object["dataUri"], `${path}.dataUri`),
    alt: string(object["alt"], `${path}.alt`, true),
  };
};

const unique = (values: string[], path: string): void => {
  const seen = new Set<string>();
  for (const value of values) {
    if (seen.has(value)) throw new EditorContractError(path, `unique values (duplicate ${JSON.stringify(value)})`);
    seen.add(value);
  }
};

const validateAuthoring = (
  authoring: AuthoringMetadata,
  previews: EditorPreview[],
): void => {
  unique(authoring.targets.map((target) => target.id), "$.render.authoring.targets[].id");
  unique(authoring.entries.map((entry) => entry.id), "$.render.authoring.entries[].id");
  const targets = new Map(authoring.targets.map((target) => [target.id, target]));
  const entries = new Map(authoring.entries.map((entry) => [entry.id, entry]));
  const orders = new Map<string, number[]>();
  const annotationTargets = new Set<string>();

  for (const [index, entry] of authoring.entries.entries()) {
    const entryPath = `$.render.authoring.entries[${index}]`;
    const target = targets.get(entry.targetId);
    if (!target) {
      throw new EditorContractError(`${entryPath}.targetId`, "an existing source target id");
    }
    const annotatable = annotatableTargetClasses.has(target.class);
    if (entry.class === "doc") {
      if (entry.kind !== "doc" || entry.order !== 0 || annotatable) {
        throw new EditorContractError(entryPath, "doc metadata on a documentable target");
      }
    } else {
      if (
        !annotatable
        || entry.kind.length > 64
        || !/^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$/.test(entry.kind)
      ) {
        throw new EditorContractError(entryPath, "annotation metadata on an annotatable target");
      }
      annotationTargets.add(entry.targetId);
    }
    const targetOrders = orders.get(entry.targetId) ?? [];
    targetOrders.push(entry.order);
    orders.set(entry.targetId, targetOrders);
  }
  for (const [targetId, values] of orders) {
    values.sort((left, right) => left - right);
    values.forEach((value, index) => {
      if (value !== index) {
        throw new EditorContractError(
          "$.render.authoring.entries[].order",
          `contiguous target-local order for ${JSON.stringify(targetId)}`,
        );
      }
    });
  }
  const unusedTarget = authoring.targets.find((target) => !orders.has(target.id));
  if (unusedTarget) {
    throw new EditorContractError(
      "$.render.authoring.targets",
      `only metadata-referenced targets (unused ${JSON.stringify(unusedTarget.id)})`,
    );
  }

  const validateDocumentation = (
    preview: EditorPreview,
    entryId: string | null,
    expectedClass: SourceTargetClass,
    path: string,
  ): void => {
    if (entryId === null) return;
    const entry = entries.get(entryId);
    const target = entry ? targets.get(entry.targetId) : undefined;
    const contextMatches = expectedClass === "example-declaration"
      ? target?.label === preview.identity.example
      : target?.owner.kind === preview.identity.kind
        && target.owner.name === preview.identity.subject;
    if (
      !entry
      || entry.class !== "doc"
      || target?.class !== expectedClass
      || !contextMatches
    ) {
      throw new EditorContractError(path, `a doc entry for ${expectedClass}`);
    }
  };

  for (const [previewIndex, preview] of previews.entries()) {
    const previewPath = `$.render.previews[${previewIndex}]`;
    const declarationClass: SourceTargetClass = preview.identity.kind === "page"
      ? "page-declaration"
      : preview.identity.kind === "surface"
      ? "surface-declaration"
      : "component-declaration";
    validateDocumentation(
      preview,
      preview.documentation.declarationDocId,
      declarationClass,
      `${previewPath}.documentation.declarationDocId`,
    );
    validateDocumentation(
      preview,
      preview.documentation.exampleDocId,
      "example-declaration",
      `${previewPath}.documentation.exampleDocId`,
    );
    unique(
      preview.provenance.occurrences.map((occurrence) => occurrence.id),
      `${previewPath}.provenance.occurrences[].id`,
    );
    for (const [occurrenceIndex, occurrence] of preview.provenance.occurrences.entries()) {
      const occurrencePath = `${previewPath}.provenance.occurrences[${occurrenceIndex}]`;
      const target = targets.get(occurrence.targetId);
      if (
        !target
        || !annotatableTargetClasses.has(target.class)
        || !annotationTargets.has(occurrence.targetId)
      ) {
        throw new EditorContractError(
          `${occurrencePath}.targetId`,
          "an annotation-bearing annotatable source target id",
        );
      }
      unique(
        occurrence.anchors.map((anchor) => JSON.stringify(anchor)),
        `${occurrencePath}.anchors`,
      );
      for (const [anchorIndex, anchor] of occurrence.anchors.entries()) {
        if (!anchorResolves(preview.content, anchor)) {
          throw new EditorContractError(
            `${occurrencePath}.anchors[${anchorIndex}]`,
            "a semantic node path in this preview",
          );
        }
      }
    }
  }
};

const anchorResolves = (contentValue: Snapshot | VNode, anchor: RenderNodeRef): boolean => {
  let node: VNode | undefined;
  if (isSnapshotContent(contentValue)) {
    if (anchor.root.kind === "page") node = contentValue.page.root;
    else if (anchor.root.kind === "surface") {
      const key = anchor.root.key;
      const matching = contentValue.surfaces.filter((surfaceValue) => surfaceValue.key === key);
      if (matching.length === 1) node = matching[0]?.root;
    }
  } else if (anchor.root.kind === "fragment") {
    node = contentValue;
  }
  if (!node) return false;
  for (const index of anchor.path) {
    node = node.children?.[index];
    if (!node) return false;
  }
  return true;
};

const validateReferences = (groups: PreviewGroup[], previews: EditorPreview[]): void => {
  unique(groups.map((item) => item.id), "$.render.groups[].id");
  unique(previews.map((item) => item.id), "$.render.previews[].id");
  unique(previews.map((item) => JSON.stringify([
    item.identity.kind,
    item.identity.subject,
    item.identity.example,
  ])),
  "$.render.previews[].identity");

  const byId = new Map(previews.map((item) => [item.id, item]));
  const identities = new Set(previews.map((item) => semanticPreviewKey(item.identity)));
  for (const previewValue of previews) {
    if (previewValue.from === null) continue;
    const parentKey = semanticPreviewKey({
      kind: previewValue.identity.kind,
      subject: previewValue.identity.subject,
      example: previewValue.from,
    });
    if (!identities.has(parentKey)) {
      throw new EditorContractError(
        `$.render.previews[${JSON.stringify(previewValue.id)}].from`,
        `an existing example in the same subject (missing ${JSON.stringify(previewValue.from)})`,
      );
    }
  }
  const referenced: string[] = [];
  for (const groupValue of groups) {
    unique(groupValue.previews, `$.render.groups[${JSON.stringify(groupValue.id)}].previews`);
    for (const previewId of groupValue.previews) {
      const previewValue = byId.get(previewId);
      if (!previewValue) {
        throw new EditorContractError(
          `$.render.groups[${JSON.stringify(groupValue.id)}].previews`,
          `an existing preview id (missing ${JSON.stringify(previewId)})`,
        );
      }
      if (
        previewValue.identity.kind !== groupValue.kind
        || previewValue.identity.subject !== groupValue.subject
      ) {
        throw new EditorContractError(
          `$.render.groups[${JSON.stringify(groupValue.id)}]`,
          `kind and subject matching preview ${JSON.stringify(previewId)}`,
        );
      }
      referenced.push(previewId);
    }
  }
  unique(referenced, "$.render.groups[].previews");
  if (referenced.length !== previews.length) {
    const referencedSet = new Set(referenced);
    const missing = previews.find((item) => !referencedSet.has(item.id));
    throw new EditorContractError(
      "$.render.groups[].previews",
      `every preview exactly once${missing ? ` (missing ${JSON.stringify(missing.id)})` : ""}`,
    );
  }
};

const interactionGraphNodeKinds = ["page", "surface", "command", "dynamic"] as const;

const interactionGraphEdgeKinds = [
  "navigate", "navigate-back", "present", "dismiss", "state-change", "send-command",
  "receive-outcome",
] as const;

/**
 * Decodes the fields the board draws. Unlike the render envelope this is
 * deliberately lenient about extra keys: the native graph carries analysis
 * detail (guards, commands, source spans) the editor does not mirror.
 */
const interactionGraph = (value: unknown, path: string): InteractionGraph => {
  const object = record(value, path);
  if (object["protocol"] !== INTERACTION_GRAPH_PROTOCOL) {
    throw new EditorContractError(`${path}.protocol`, JSON.stringify(INTERACTION_GRAPH_PROTOCOL));
  }
  return {
    protocol: INTERACTION_GRAPH_PROTOCOL,
    nodes: array(object["nodes"], `${path}.nodes`).map((item, index) => {
      const nodePath = `${path}.nodes[${index}]`;
      const node = record(item, nodePath);
      return {
        id: string(node["id"], `${nodePath}.id`),
        kind: oneOf(node["kind"], `${nodePath}.kind`, interactionGraphNodeKinds),
        label: string(node["label"], `${nodePath}.label`),
      };
    }),
    edges: array(object["edges"], `${path}.edges`).map((item, index) => {
      const edgePath = `${path}.edges[${index}]`;
      const edge = record(item, edgePath);
      return {
        kind: oneOf(edge["kind"], `${edgePath}.kind`, interactionGraphEdgeKinds),
        from: string(edge["from"], `${edgePath}.from`),
        to: string(edge["to"], `${edgePath}.to`),
        event: string(edge["event"], `${edgePath}.event`),
      };
    }),
  };
};

const render = (value: unknown, path: string, sourceRevision: number): EditorRender | null => {
  if (value === null) return null;
  const object = record(value, path);
  exact(object, path, [
    "revision", "freshness", "application", "authoring", "groups", "previews", "stylesheet",
    "icons", "assets", "interactionGraph",
  ]);
  const revision = positiveRevision(object["revision"], `${path}.revision`);
  const freshness = oneOf(object["freshness"], `${path}.freshness`, ["current", "stale"]);
  if (freshness === "current" && revision !== sourceRevision) {
    throw new EditorContractError(`${path}.revision`, `sourceRevision ${sourceRevision} for current render`);
  }
  if (freshness === "stale" && revision >= sourceRevision) {
    throw new EditorContractError(`${path}.revision`, `less than sourceRevision ${sourceRevision} for stale render`);
  }
  const application = record(object["application"], `${path}.application`);
  exact(application, `${path}.application`, ["name"]);
  const groups = array(object["groups"], `${path}.groups`).map((item, index) =>
    group(item, `${path}.groups[${index}]`));
  const previews = array(object["previews"], `${path}.previews`).map((item, index) =>
    preview(item, `${path}.previews[${index}]`));
  validateReferences(groups, previews);
  const authoring = authoringMetadata(object["authoring"], `${path}.authoring`);
  validateAuthoring(authoring, previews);
  const icons = Object.fromEntries(Object.entries(record(object["icons"], `${path}.icons`)).map(
    ([key, item]) => [key, icon(item, `${path}.icons.${key}`)],
  ));
  const assets = Object.fromEntries(Object.entries(record(object["assets"], `${path}.assets`)).map(
    ([key, item]) => [key, asset(item, `${path}.assets.${key}`)],
  ));
  return {
    revision,
    freshness,
    application: { name: string(application["name"], `${path}.application.name`) },
    authoring,
    groups,
    previews,
    stylesheet: string(object["stylesheet"], `${path}.stylesheet`, true),
    icons,
    assets,
    interactionGraph: interactionGraph(object["interactionGraph"], `${path}.interactionGraph`),
  };
};

const diagnosticsEnvelope = (
  value: unknown,
  path: string,
): Record<string, JsonValue> | null => {
  if (value === null) return null;
  const envelope = record(value, path);
  exact(envelope, path, ["format", "version", "summary", "diagnostics"]);
  if (envelope["format"] !== "uhura-diagnostics") {
    throw new EditorContractError(`${path}.format`, JSON.stringify("uhura-diagnostics"));
  }
  if (envelope["version"] !== 0) {
    throw new EditorContractError(`${path}.version`, "0");
  }
  const summary = record(envelope["summary"], `${path}.summary`);
  exact(summary, `${path}.summary`, ["errors", "warnings"]);
  const expectedErrors = nonNegativeInteger(summary["errors"], `${path}.summary.errors`);
  const expectedWarnings = nonNegativeInteger(summary["warnings"], `${path}.summary.warnings`);
  let errors = 0;
  let warnings = 0;
  array(envelope["diagnostics"], `${path}.diagnostics`).forEach((item, index) => {
    const diagnosticPath = `${path}.diagnostics[${index}]`;
    const diagnostic = record(item, diagnosticPath);
    string(diagnostic["code"], `${diagnosticPath}.code`);
    string(diagnostic["rule"], `${diagnosticPath}.rule`);
    string(diagnostic["message"], `${diagnosticPath}.message`, true);
    const severity = oneOf(
      diagnostic["severity"],
      `${diagnosticPath}.severity`,
      ["error", "warning", "info"],
    );
    if (severity === "error") errors += 1;
    else if (severity === "warning") warnings += 1;
  });
  if (errors !== expectedErrors || warnings !== expectedWarnings) {
    throw new EditorContractError(
      `${path}.summary`,
      `counts matching diagnostics (${errors} errors, ${warnings} warnings)`,
    );
  }
  return jsonRecord(value, path);
};

export const decodeEditorState = (value: unknown): EditorState => {
  const object = record(value, "$");
  exact(object, "$", ["protocol", "sourceRevision", "diagnostics", "render"]);
  if (object["protocol"] !== EDITOR_STATE_PROTOCOL) {
    throw new EditorContractError("$.protocol", JSON.stringify(EDITOR_STATE_PROTOCOL));
  }
  const sourceRevision = positiveRevision(object["sourceRevision"], "$.sourceRevision");
  return {
    protocol: EDITOR_STATE_PROTOCOL,
    sourceRevision,
    diagnostics: diagnosticsEnvelope(object["diagnostics"], "$.diagnostics"),
    render: render(object["render"], "$.render", sourceRevision),
  };
};

export const decodeEditorRevisionEvent = (value: unknown): EditorRevisionEvent => {
  const object = record(value, "$");
  exact(object, "$", ["protocol", "sourceRevision"]);
  if (object["protocol"] !== EDITOR_EVENT_PROTOCOL) {
    throw new EditorContractError("$.protocol", JSON.stringify(EDITOR_EVENT_PROTOCOL));
  }
  return {
    protocol: EDITOR_EVENT_PROTOCOL,
    sourceRevision: positiveRevision(object["sourceRevision"], "$.sourceRevision"),
  };
};

export const semanticPreviewKey = (identity: PreviewIdentity): string =>
  JSON.stringify([identity.kind, identity.subject, identity.example]);
