// Pure projection from the versioned inspection protocol into the small,
// focused behavior graph consumed by Play's developer UI. Runtime values only
// decorate static nodes: they never decide which nodes exist, so a focused
// graph keeps the same geometry while the machine advances.

import type {
  DeepReadonly,
  InspectProgramEdge,
  InspectProgramNode,
  InspectSourceSpan,
  InspectionState,
  StepTrace,
  TraceGuardNote,
} from "../protocol/types.js";

export type DebugLane = "input" | "handler" | "effect";
export type DebugDefinitionKind = "page" | "surface" | "component";
export type DebugProjectionApply = "applied" | "dropped-stale" | "failed";
export type DebugEdgeActivity = "idle" | "context" | "taken";

export interface DebugDefinitionOption {
  readonly id: string;
  readonly kind: DebugDefinitionKind;
  readonly label: string;
  readonly entry: boolean;
  readonly active: boolean;
  readonly top: boolean;
  readonly runtime: boolean;
  readonly transitionTarget: boolean;
}

export interface DebugNodeRuntime {
  /** The owning definition is mounted, or this definition target is mounted. */
  readonly active: boolean;
  /** The node participated in the latest retained step. */
  readonly current: boolean;
  readonly selected: boolean;
  readonly consulted: TraceGuardNote["guard"] | null;
  readonly written: boolean;
  readonly sent: boolean;
  readonly pending: number;
  readonly projectionApply: DebugProjectionApply | null;
  readonly projectionReady: number;
  readonly projectionFailures: number;
  readonly structural: boolean;
}

export interface DebugGraphNode {
  readonly id: string;
  readonly kind: InspectProgramNode["kind"];
  readonly lane: DebugLane;
  readonly definitionId: string | null;
  readonly label: string;
  readonly detail: string | null;
  /** Source-order hint. Handler nodes use their absolute handler index. */
  readonly order: number;
  readonly span: InspectSourceSpan | null;
  readonly runtime: DebugNodeRuntime;
}

export interface DebugGraphEdge {
  readonly id: string;
  readonly kind: InspectProgramEdge["kind"];
  readonly from: string;
  readonly to: string;
  readonly label: string;
  readonly order: number | null;
  readonly mode: "push" | "replace" | null;
  readonly activity: DebugEdgeActivity;
}

export type DebugEmptyReason = "loading" | "disposed" | "no-definitions";

export interface DebugGraphModel {
  readonly disposed: boolean;
  readonly emptyReason: DebugEmptyReason | null;
  readonly generation: number | null;
  readonly programHash: string | null;
  readonly revision: number | null;
  readonly focusDefinitionId: string | null;
  readonly runtimeDefinitionId: string | null;
  readonly definitions: readonly DebugDefinitionOption[];
  readonly nodes: readonly DebugGraphNode[];
  readonly edges: readonly DebugGraphEdge[];
}

export interface DeriveDebugGraphOptions {
  /** A valid definition pins focus; absent/invalid focus follows the runtime. */
  readonly focusDefinitionId?: string | null;
}

type ProgramNode = DeepReadonly<InspectProgramNode>;
type ProgramEdge = DeepReadonly<InspectProgramEdge>;

const DEFINITION_KIND_ORDER: Readonly<Record<DebugDefinitionKind, number>> = {
  page: 0,
  surface: 1,
  component: 2,
};

const NODE_KIND_ORDER: Readonly<Record<InspectProgramNode["kind"], number>> = {
  event: 0,
  projection: 1,
  state: 2,
  handler: 3,
  command: 4,
  definition: 5,
};

const LANE_ORDER: Readonly<Record<DebugLane, number>> = {
  input: 0,
  handler: 1,
  effect: 2,
};

function compareText(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function definitionIdForNode(node: ProgramNode): string | null {
  switch (node.kind) {
    case "definition":
      return node.id;
    case "event":
    case "handler":
    case "state":
      return node.definition;
    case "command":
    case "projection":
      return null;
  }
}

/** Maps a canonical dispatch record to the same definition namespace as IR. */
export function runtimeDefinitionIdForTrace(
  trace: DeepReadonly<StepTrace> | null,
): string | null {
  const dispatch = trace?.dispatch;
  if (!dispatch) return null;
  if (dispatch.scope.startsWith("page:")) return `pages.${dispatch.definition}`;
  if (dispatch.scope.startsWith("surface:")) {
    return `surfaces.${dispatch.definition}`;
  }
  return null;
}

function stableJson(value: unknown): string | undefined {
  if (value === undefined) return undefined;
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) {
    const items = value.map((item) => stableJson(item) ?? "null");
    return `[${items.join(",")}]`;
  }
  const record = value as Record<string, unknown>;
  const fields = Object.keys(record)
    .sort(compareText)
    .flatMap((key) => {
      const encoded = stableJson(record[key]);
      return encoded === undefined ? [] : [`${JSON.stringify(key)}:${encoded}`];
    });
  return `{${fields.join(",")}}`;
}

/** Compact deterministic value text; layout never measures this string. */
export function formatDebugValue(value: unknown, maxLength = 88): string {
  const encoded = stableJson(value) ?? "unset";
  if (encoded.length <= maxLength) return encoded;
  if (maxLength <= 1) return "…".slice(0, maxLength);
  return `${encoded.slice(0, maxLength - 1)}…`;
}

function activeDefinitions(
  state: InspectionState,
): { ids: Set<string>; top: string | null } {
  const snapshot = state.latest?.inspection;
  if (!snapshot) return { ids: new Set(), top: null };
  const ids = new Set<string>();
  for (const entry of snapshot.u.nav) ids.add(`pages.${entry.route}`);
  for (const surface of snapshot.u.surfaces) {
    ids.add(`surfaces.${surface.definition}`);
  }
  const topSurface = snapshot.u.surfaces.at(-1);
  if (topSurface) return { ids, top: `surfaces.${topSurface.definition}` };
  const topPage = snapshot.u.nav.at(-1);
  return { ids, top: topPage ? `pages.${topPage.route}` : null };
}

function structuralTargets(trace: DeepReadonly<StepTrace> | null): Set<string> {
  const targets = new Set<string>();
  const surfaceDefinition = (instance: string): string =>
    instance.replace(/:\d+$/, "");
  for (const operation of trace?.structural ?? []) {
    switch (operation.op) {
      case "init":
      case "navigate":
      case "replace":
        targets.add(`pages.${operation.route}`);
        break;
      case "back":
        if (operation.to !== null) targets.add(`pages.${operation.to}`);
        break;
      case "open-surface":
      case "already-open":
      case "force-close":
      case "dismiss":
        targets.add(`surfaces.${surfaceDefinition(operation.surface)}`);
        break;
      case "nav-underflow":
        break;
    }
  }
  return targets;
}

interface DefinitionInstance {
  readonly definitionId: string;
  readonly scope: string;
  readonly state: Readonly<Record<string, unknown>>;
}

function definitionInstance(
  state: InspectionState,
  definitionId: string,
  exactScope?: string,
): DefinitionInstance | null {
  const snapshot = state.latest?.inspection;
  if (!snapshot) return null;
  if (definitionId.startsWith("pages.")) {
    const route = definitionId.slice("pages.".length);
    const candidates = snapshot.u.nav.filter((item) => item.route === route);
    const entry = exactScope === undefined
      ? candidates.at(-1)
      : candidates.find((item) => `page:${item.serial}` === exactScope);
    return entry
      ? { definitionId, scope: `page:${entry.serial}`, state: entry.state }
      : null;
  }
  if (definitionId.startsWith("surfaces.")) {
    const definition = definitionId.slice("surfaces.".length);
    const candidates = snapshot.u.surfaces.filter(
      (item) => item.definition === definition,
    );
    const surface = exactScope === undefined
      ? candidates.at(-1)
      : candidates.find((item) => `surface:${item.serial}` === exactScope);
    return surface
      ? {
          definitionId,
          scope: `surface:${surface.serial}`,
          state: surface.state,
        }
      : null;
  }
  return null;
}

function staticEdgeOrder(edge: ProgramEdge): number | null {
  switch (edge.kind) {
    case "writes":
    case "sends":
    case "opens":
    case "navigates":
      return edge.order;
    case "handles":
    case "guard-reads":
    case "body-reads":
    case "settles":
      return null;
  }
}

function staticEdgeMode(edge: ProgramEdge): "push" | "replace" | null {
  return edge.kind === "navigates" ? edge.mode : null;
}

function edgeSignature(edge: ProgramEdge): string {
  return [
    edge.kind,
    edge.from,
    edge.to,
    String(staticEdgeOrder(edge) ?? -1),
    staticEdgeMode(edge) ?? "",
  ].join("|");
}

function edgeLabel(edge: ProgramEdge): string {
  switch (edge.kind) {
    case "handles":
      return "handles";
    case "guard-reads":
      return "guard";
    case "body-reads":
      return "reads";
    case "writes":
      return "writes";
    case "sends":
      return "sends";
    case "opens":
      return "opens";
    case "navigates":
      return edge.mode;
    case "settles":
      return "settles";
  }
}

function laneForNode(
  node: ProgramNode,
  writtenStateIds: ReadonlySet<string>,
): DebugLane {
  switch (node.kind) {
    case "handler":
      return "handler";
    case "event":
    case "projection":
      return "input";
    case "state":
      return writtenStateIds.has(node.id) ? "effect" : "input";
    case "command":
    case "definition":
      return "effect";
  }
}

function definitionDetail(node: Extract<ProgramNode, { kind: "definition" }>): string {
  const kind = node["definition-kind"];
  const label = `${kind[0]?.toUpperCase() ?? ""}${kind.slice(1)}`;
  return node.entry ? `${label} · entry` : label;
}

function projectionDetail(
  node: Extract<ProgramNode, { kind: "projection" }>,
  state: InspectionState,
): string {
  const snapshot = state.latest?.inspection;
  if (!snapshot) return `Projection · ${node.port}`;
  const ready = snapshot.x.snapshots.filter(
    (item) => item.projection === node.name,
  );
  const failed = snapshot.x.failed.filter(
    (item) => item.projection === node.name,
  );
  if (ready.length === 0 && failed.length === 0) return "Waiting";
  if (ready.length === 1 && failed.length === 0) {
    return `Ready · ${formatDebugValue(ready[0]?.value)}`;
  }
  const parts: string[] = [];
  if (ready.length > 0) parts.push(`${ready.length} ready`);
  if (failed.length > 0) parts.push(`${failed.length} failed`);
  return parts.join(" · ");
}

function presentation(
  node: ProgramNode,
  state: InspectionState,
  instance: DefinitionInstance | null,
  pendingByCommand: ReadonlyMap<string, number>,
): { label: string; detail: string | null; order: number } {
  switch (node.kind) {
    case "definition":
      return { label: node.name, detail: definitionDetail(node), order: 0 };
    case "event": {
      const detail = node["event-kind"] === "outcome"
        ? `${node.command ?? "command"}.${node.outcome ?? "outcome"}`
        : "Semantic event";
      return { label: node.name, detail, order: 0 };
    }
    case "handler":
      return {
        label: `Handler ${node.index + 1}`,
        detail: `on ${node.on}${node.guarded ? " · guarded" : ""}`,
        order: node.index,
      };
    case "state": {
      const values = instance?.definitionId === node.definition
        ? instance.state
        : null;
      const value = values && Object.hasOwn(values, node.name)
        ? values[node.name]
        : node.initial;
      const prefix = values ? "" : "Initial · ";
      return {
        label: node.name,
        detail: `${prefix}${formatDebugValue(value)}`,
        order: 0,
      };
    }
    case "projection":
      return { label: node.name, detail: projectionDetail(node, state), order: 0 };
    case "command": {
      const pending = pendingByCommand.get(node.name) ?? 0;
      const detail = pending > 0
        ? `${pending} pending`
        : node.port
          ? `Command · ${node.port}`
          : "Command";
      return { label: node.name, detail, order: 0 };
    }
  }
}

function programSpan(
  spans: DeepReadonly<Record<string, InspectSourceSpan>>,
  id: string,
): InspectSourceSpan | null {
  const span = spans[id];
  return span ? { file: span.file, start: span.start, end: span.end } : null;
}

function projectionRuntime(
  node: ProgramNode,
  state: InspectionState,
  applies: ReadonlyMap<string, DebugProjectionApply>,
): {
  apply: DebugProjectionApply | null;
  ready: number;
  failures: number;
} {
  if (node.kind !== "projection") return { apply: null, ready: 0, failures: 0 };
  const snapshot = state.latest?.inspection;
  return {
    apply: applies.get(node.name) ?? null,
    ready: snapshot?.x.snapshots.filter(
      (item) => item.projection === node.name,
    ).length ?? 0,
    failures: snapshot?.x.failed.filter(
      (item) => item.projection === node.name,
    ).length ?? 0,
  };
}

interface EdgeActivityContext {
  readonly currentEventId: string | null;
  readonly selectedHandlerId: string | null;
  readonly consultedHandlers: ReadonlyMap<string, TraceGuardNote["guard"]>;
  readonly runtimeWrittenIds: ReadonlySet<string>;
  readonly sentCommandIds: ReadonlySet<string>;
  readonly transitionTargets: ReadonlySet<string>;
}

interface DebugNodeContext extends EdgeActivityContext {
  readonly state: InspectionState;
  readonly spans: DeepReadonly<Record<string, InspectSourceSpan>>;
  readonly writtenStateIds: ReadonlySet<string>;
  readonly activeIds: ReadonlySet<string>;
  readonly instance: DefinitionInstance | null;
  readonly pendingByCommand: ReadonlyMap<string, number>;
  readonly applies: ReadonlyMap<string, DebugProjectionApply>;
}

function debugNode(node: ProgramNode, context: DebugNodeContext): DebugGraphNode {
  const {
    state,
    spans,
    writtenStateIds,
    activeIds,
    currentEventId,
    selectedHandlerId,
    consultedHandlers,
    runtimeWrittenIds,
    sentCommandIds,
    instance,
    pendingByCommand,
    applies,
    transitionTargets,
  } = context;
  const definitionId = definitionIdForNode(node);
  const view = presentation(node, state, instance, pendingByCommand);
  const consulted = consultedHandlers.get(node.id) ?? null;
  const written = runtimeWrittenIds.has(node.id);
  const sent = sentCommandIds.has(node.id);
  const projection = projectionRuntime(node, state, applies);
  const structural = transitionTargets.has(node.id);
  const selected = node.id === selectedHandlerId;
  const active = node.kind === "definition"
    ? activeIds.has(node.id)
    : definitionId !== null && activeIds.has(definitionId);
  const current = node.id === currentEventId
    || consulted !== null
    || selected
    || written
    || sent
    || projection.apply !== null
    || structural;
  return {
    id: node.id,
    kind: node.kind,
    lane: laneForNode(node, writtenStateIds),
    definitionId,
    label: view.label,
    detail: view.detail,
    order: view.order,
    span: programSpan(spans, node.id),
    runtime: {
      active,
      current,
      selected,
      consulted,
      written,
      sent,
      pending: node.kind === "command"
        ? pendingByCommand.get(node.name) ?? 0
        : 0,
      projectionApply: projection.apply,
      projectionReady: projection.ready,
      projectionFailures: projection.failures,
      structural,
    },
  };
}

function edgeActivity(
  edge: ProgramEdge,
  context: EdgeActivityContext,
): DebugEdgeActivity {
  const {
    currentEventId,
    selectedHandlerId,
    consultedHandlers,
    runtimeWrittenIds,
    sentCommandIds,
    transitionTargets,
  } = context;
  switch (edge.kind) {
    case "handles":
      if (edge.from === currentEventId && edge.to === selectedHandlerId) return "taken";
      if (edge.from === currentEventId && consultedHandlers.has(edge.to)) return "context";
      return "idle";
    case "guard-reads":
      return consultedHandlers.has(edge.to) ? "context" : "idle";
    case "body-reads":
      return edge.to === selectedHandlerId ? "context" : "idle";
    case "writes":
      return edge.from === selectedHandlerId && runtimeWrittenIds.has(edge.to)
        ? "taken"
        : "idle";
    case "sends":
      return edge.from === selectedHandlerId && sentCommandIds.has(edge.to)
        ? "taken"
        : "idle";
    case "opens":
    case "navigates":
      return edge.from === selectedHandlerId && transitionTargets.has(edge.to)
        ? "taken"
        : "idle";
    case "settles":
      return edge.to === currentEventId ? "taken" : "idle";
  }
}

function emptyModel(
  state: InspectionState,
  reason: DebugEmptyReason,
): DebugGraphModel {
  return {
    disposed: state.disposed,
    emptyReason: reason,
    generation: null,
    programHash: null,
    revision: null,
    focusDefinitionId: null,
    runtimeDefinitionId: null,
    definitions: [],
    nodes: [],
    edges: [],
  };
}

/**
 * Produces one definition-sized behavior graph. The returned node and edge set
 * depends only on `(program, focusDefinitionId)`; live state changes labels and
 * runtime marks without moving or adding graph structure.
 */
export function deriveDebugGraph(
  state: InspectionState,
  options: DeriveDebugGraphOptions = {},
): DebugGraphModel {
  const artifacts = state.artifacts;
  if (!artifacts) return emptyModel(state, state.disposed ? "disposed" : "loading");

  const program = artifacts.program;
  const nodesById = new Map(program.nodes.map((node) => [node.id, node]));
  const definitionNodes = program.nodes.filter(
    (node): node is Extract<ProgramNode, { kind: "definition" }> =>
      node.kind === "definition",
  );
  if (definitionNodes.length === 0) {
    return {
      ...emptyModel(state, "no-definitions"),
      generation: artifacts.generation,
      programHash: program.ir.hash,
      revision: state.latest?.inspection.revision ?? null,
    };
  }

  const trace = state.latest?.trace ?? null;
  const runtimeDefinitionId = runtimeDefinitionIdForTrace(trace);
  const active = activeDefinitions(state);
  const transitionTargets = structuralTargets(trace);
  const validDefinitionIds = new Set(definitionNodes.map((node) => node.id));
  const requested = options.focusDefinitionId;
  const entryId = `pages.${program.ir.entry}`;
  const focusDefinitionId = requested && validDefinitionIds.has(requested)
    ? requested
    : runtimeDefinitionId && validDefinitionIds.has(runtimeDefinitionId)
      ? runtimeDefinitionId
      : active.top && validDefinitionIds.has(active.top)
        ? active.top
        : validDefinitionIds.has(entryId)
          ? entryId
          : definitionNodes
              .map((node) => node.id)
              .sort(compareText)[0] ?? null;

  const definitions = definitionNodes
    .map((node): DebugDefinitionOption => ({
      id: node.id,
      kind: node["definition-kind"],
      label: node.name,
      entry: node.entry === true,
      active: active.ids.has(node.id),
      top: active.top === node.id,
      runtime: runtimeDefinitionId === node.id,
      transitionTarget: transitionTargets.has(node.id),
    }))
    .sort((left, right) =>
      DEFINITION_KIND_ORDER[left.kind] - DEFINITION_KIND_ORDER[right.kind]
      || compareText(left.label, right.label)
      || compareText(left.id, right.id));

  if (focusDefinitionId === null) {
    return {
      disposed: false,
      emptyReason: "no-definitions",
      generation: artifacts.generation,
      programHash: program.ir.hash,
      revision: state.latest?.inspection.revision ?? null,
      focusDefinitionId: null,
      runtimeDefinitionId,
      definitions,
      nodes: [],
      edges: [],
    };
  }

  const localNodeIds = new Set(
    program.nodes
      .filter((node) =>
        node.kind !== "definition"
        && definitionIdForNode(node) === focusDefinitionId)
      .map((node) => node.id),
  );
  const localHandlerIds = new Set(
    program.nodes
      .filter(
        (node) => node.kind === "handler" && node.definition === focusDefinitionId,
      )
      .map((node) => node.id),
  );

  const focusedEdges = program.edges.filter((edge) =>
    localHandlerIds.has(edge.from) || localHandlerIds.has(edge.to));
  const includedNodeIds = new Set(localNodeIds);
  for (const edge of focusedEdges) {
    includedNodeIds.add(edge.from);
    includedNodeIds.add(edge.to);
  }
  // Commands sent by this definition can settle into its outcome events.
  const settleEdges = program.edges.filter((edge) =>
    edge.kind === "settles"
    && includedNodeIds.has(edge.from)
    && localNodeIds.has(edge.to));
  const includedEdges = [...focusedEdges, ...settleEdges]
    .filter((edge, index, all) => all.indexOf(edge) === index);

  const writtenStateIds = new Set(
    focusedEdges.filter((edge) => edge.kind === "writes").map((edge) => edge.to),
  );
  const dispatch = trace?.dispatch;
  const traceMatchesFocus = runtimeDefinitionId === focusDefinitionId;
  // A dispatch identifies one concrete mounted instance. When there is no
  // dispatch (for example a projection delivery or a user-pinned definition),
  // the topmost mounted instance of that definition is the observable one.
  // If the dispatched instance was structurally removed by this step, do not
  // fall through to a different duplicate instance with the same definition.
  const exactScope = traceMatchesFocus ? dispatch?.scope : undefined;
  const instance = definitionInstance(
    state,
    focusDefinitionId,
    exactScope,
  );
  const focusScope = exactScope ?? instance?.scope ?? null;
  const currentEventId = traceMatchesFocus && dispatch
    ? `${focusDefinitionId}/event/${dispatch.on}`
    : null;
  const selectedHandlerId = traceMatchesFocus && dispatch?.selected !== null
    && dispatch?.selected !== undefined
    ? `${focusDefinitionId}/handler/${dispatch.selected}`
    : null;
  const consultedHandlers = new Map<string, TraceGuardNote["guard"]>();
  if (traceMatchesFocus && dispatch) {
    for (const guard of dispatch.guards) {
      consultedHandlers.set(
        `${focusDefinitionId}/handler/${guard.handler}`,
        guard.guard,
      );
    }
  }
  const runtimeWrittenIds = new Set<string>();
  if (traceMatchesFocus && dispatch) {
    for (const write of dispatch.writes ?? []) {
      runtimeWrittenIds.add(`${focusDefinitionId}/state/${write.field}`);
    }
  }
  const sentCommandIds = new Set<string>();
  if (traceMatchesFocus) {
    for (const message of trace?.c ?? []) {
      if (message.kind === "command" && message.command) {
        sentCommandIds.add(`commands.${message.command}`);
      }
    }
  }
  const pendingByCommand = new Map<string, number>();
  for (const pending of Object.values(state.latest?.inspection.u.pending ?? {})) {
    if (focusScope === null || pending.origin !== focusScope) continue;
    pendingByCommand.set(
      pending.command,
      (pendingByCommand.get(pending.command) ?? 0) + 1,
    );
  }
  const applies = new Map<string, DebugProjectionApply>();
  for (const apply of trace?.applies ?? []) applies.set(apply.projection, apply.apply);
  const focusedTransitionTargets: ReadonlySet<string> = traceMatchesFocus
    ? transitionTargets
    : new Set();
  const debugContext: DebugNodeContext = {
    state,
    spans: program.spans,
    writtenStateIds,
    activeIds: active.ids,
    currentEventId,
    selectedHandlerId,
    consultedHandlers,
    runtimeWrittenIds,
    sentCommandIds,
    instance,
    pendingByCommand,
    applies,
    transitionTargets: focusedTransitionTargets,
  };

  const nodes = [...includedNodeIds]
    .map((id) => nodesById.get(id))
    .filter((node): node is ProgramNode => node !== undefined)
    .map((node) => debugNode(node, debugContext))
    .sort((left, right) =>
      LANE_ORDER[left.lane] - LANE_ORDER[right.lane]
      || NODE_KIND_ORDER[left.kind] - NODE_KIND_ORDER[right.kind]
      || left.order - right.order
      || compareText(left.id, right.id));

  const sortedProgramEdges = includedEdges
    .map((edge, sourceIndex) => ({ edge, sourceIndex, signature: edgeSignature(edge) }))
    .sort((left, right) =>
      compareText(left.signature, right.signature)
      || left.sourceIndex - right.sourceIndex);
  const duplicateCounts = new Map<string, number>();
  const edges = sortedProgramEdges.map(({ edge, signature }): DebugGraphEdge => {
    const duplicate = duplicateCounts.get(signature) ?? 0;
    duplicateCounts.set(signature, duplicate + 1);
    return {
      id: `edge/${signature}/${duplicate}`,
      kind: edge.kind,
      from: edge.from,
      to: edge.to,
      label: edgeLabel(edge),
      order: staticEdgeOrder(edge),
      mode: staticEdgeMode(edge),
      activity: edgeActivity(edge, debugContext),
    };
  });

  return {
    disposed: false,
    emptyReason: null,
    generation: artifacts.generation,
    programHash: program.ir.hash,
    revision: state.latest?.inspection.revision ?? null,
    focusDefinitionId,
    runtimeDefinitionId,
    definitions,
    nodes,
    edges,
  };
}
