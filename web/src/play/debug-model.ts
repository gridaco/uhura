// Pure projection from admitted machine topology and immutable runtime
// inspection into the focused behavior graph consumed by Play's developer UI.
// Receipts decorate the checked graph; they never invent topology or claim
// execution details that the machine boundary does not expose.

import type {
  RuntimeInspectionState,
} from "../protocol/types.js";
import type {
  GraphEdgeKind,
  GraphNodeKind,
  GraphSourceRef,
  OutcomePolicy,
} from "../protocol/interaction-graph.js";
import type {
  ReactionReceipt,
  ResolvedCommand,
  ResolvedInput,
  Value,
} from "../protocol/machine.js";

export type DebugLane = "input" | "handler" | "effect";
export type DebugDefinitionKind = "machine";
export type DebugEdgeActivity = "idle" | "context" | "taken";
export type DebugGraphNodeKind = GraphNodeKind;
export type DebugGraphEdgeKind = GraphEdgeKind;

export interface DebugSourceSpan {
  /** Stable source inventory identity from the admitted inspection artifact. */
  readonly id: string | null;
  readonly file: string;
  /** Inclusive UTF-8 byte offset; this is not a JavaScript string index. */
  readonly start: number;
  /** Exclusive UTF-8 byte offset; this is not a JavaScript string index. */
  readonly end: number;
}

export interface DebugDefinitionOption {
  readonly id: string;
  readonly kind: DebugDefinitionKind;
  readonly label: string;
  readonly entry: boolean;
  readonly active: boolean;
  readonly runtime: boolean;
}

export interface DebugNodeRuntime {
  /** The node belongs to the admitted machine instance. */
  readonly active: boolean;
  /** The node participated in the latest retained receipt. */
  readonly current: boolean;
  readonly selected: boolean;
  readonly written: boolean;
  readonly sent: boolean;
}

export interface DebugGraphNode {
  readonly id: string;
  readonly kind: DebugGraphNodeKind;
  readonly lane: DebugLane;
  readonly definitionId: string;
  readonly label: string;
  readonly detail: string | null;
  /** Stable source-order hint within a lane. */
  readonly order: number;
  readonly span: Omit<DebugSourceSpan, "id"> | null;
  readonly sourceSpans: readonly DebugSourceSpan[];
  readonly runtime: DebugNodeRuntime;
}

export interface DebugGraphEdge {
  readonly id: string;
  readonly kind: DebugGraphEdgeKind;
  readonly from: string;
  readonly to: string;
  readonly label: string;
  readonly order: number;
  readonly activity: DebugEdgeActivity;
  readonly sourceSpans: readonly DebugSourceSpan[];
}

export type DebugEmptyReason = "loading" | "disposed" | "no-machines";

export interface DebugGraphModel {
  readonly disposed: boolean;
  readonly emptyReason: DebugEmptyReason | null;
  readonly generation: number | null;
  readonly programHash: string | null;
  /** Exact machine sequence text. Never projected through a JavaScript number. */
  readonly exactSequence: string | null;
  readonly focusDefinitionId: string | null;
  readonly runtimeDefinitionId: string | null;
  readonly definitions: readonly DebugDefinitionOption[];
  readonly nodes: readonly DebugGraphNode[];
  readonly edges: readonly DebugGraphEdge[];
}

export interface DeriveDebugGraphOptions {
  /** A valid machine ID pins focus; absent/invalid focus follows the runtime. */
  readonly focusDefinitionId?: string | null;
}

const NODE_KIND_ORDER: Readonly<Record<GraphNodeKind, number>> = {
  module: -2,
  part: -1,
  port: 0,
  "ui-event": 1,
  input: 2,
  transition: 3,
  "commit-hook": 4,
  computed: 4.5,
  invariant: 4.55,
  observation: 4.6,
  update: 4.7,
  state: 5,
  command: 6,
  outcome: 7,
  presentation: 8,
  machine: 9,
};

const LANE_ORDER: Readonly<Record<DebugLane, number>> = {
  input: 0,
  handler: 1,
  effect: 2,
};

function compareText(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function stableJson(value: unknown): string | undefined {
  if (value === undefined) return undefined;
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) {
    return `[${value.map((item) => stableJson(item) ?? "null").join(",")}]`;
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

function renderValue(value: Value): string {
  switch (value.$) {
    case "unit":
      return "unit";
    case "bool":
      return String(value.value);
    case "Int":
    case "Nat":
    case "PositiveInt":
    case "Decimal":
    case "Ratio":
      return value.value;
    case "BoundaryNumber":
      return value.case === "finite" ? value.value : value.case;
    case "Text":
      return JSON.stringify(value.value);
    case "key":
      return `${value.type}(${renderValue(value.value)})`;
    case "tuple":
      return `(${value.items.map(renderValue).join(", ")})`;
    case "record":
      return `{ ${
        value.fields
          .map((field) => `${field.name}: ${renderValue(field.value)}`)
          .join(", ")
      } }`;
    case "variant": {
      const fields = value.fields.map((field) => {
        const rendered = renderValue(field.value);
        return field.name === null ? rendered : `${field.name}: ${rendered}`;
      });
      return fields.length === 0
        ? value.case
        : `${value.case}(${fields.join(", ")})`;
    }
    case "seq":
      return `[${value.items.map(renderValue).join(", ")}]`;
    case "nonempty":
      return `NonEmpty[${value.items.map(renderValue).join(", ")}]`;
    case "set":
      return `Set{${value.items.map(renderValue).join(", ")}}`;
    case "map":
      return `Map{ ${
        value.entries
          .map(([key, entry]) =>
            `${renderValue(key)}: ${renderValue(entry)}`)
          .join(", ")
      } }`;
    case "table":
      return `${value.keyType}{ ${
        value.entries
          .map(([key, entry]) =>
            `${JSON.stringify(key)}: ${renderValue(entry)}`)
          .join(", ")
      } }`;
  }
}

/**
 * Human-facing Uhura value text. Exact numerics remain canonical text and are
 * never translated through JavaScript's lossy number domain.
 */
export function formatDebugValue(
  value: Value,
  maxLength = 120,
): string {
  const rendered = renderValue(value);
  if (
    value.$ === "Int"
    || value.$ === "Nat"
    || value.$ === "PositiveInt"
    || value.$ === "Decimal"
    || value.$ === "Ratio"
    || (value.$ === "BoundaryNumber" && value.case === "finite")
  ) {
    return rendered;
  }
  if (rendered.length <= maxLength) return rendered;
  if (maxLength <= 1) return "…".slice(0, maxLength);
  return `${rendered.slice(0, maxLength - 1)}…`;
}

function constructor(value: Value): string | null {
  return value.$ === "variant" ? value.case : null;
}

function inputLabel(input: ResolvedInput): string | null {
  const name = constructor(input.value);
  if (name === null) return null;
  return input.source === "port" ? `${input.port}.${name}` : name;
}

function commandLabel(command: ResolvedCommand): string | null {
  const name = constructor(command.value);
  if (name === null) return null;
  return command.target === "port" ? `${command.port}.${name}` : name;
}

function labelMatches(graphLabel: string, runtimeLabel: string | null): boolean {
  return runtimeLabel !== null && graphLabel === runtimeLabel;
}

function recordFields(
  value: Value | null,
): ReadonlyMap<string, Value> {
  if (value?.$ !== "record") return new Map();
  return new Map(value.fields.map((field) => [field.name, field.value]));
}

function valueEqual(
  left: Value | undefined,
  right: Value | undefined,
): boolean {
  if (left === undefined || right === undefined) return left === right;
  return stableJson(left) === stableJson(right);
}

function sourceSpan(source: GraphSourceRef): DebugSourceSpan {
  return {
    id: source.id,
    file: source.path,
    start: source.start,
    end: source.end,
  };
}

function lane(kind: GraphNodeKind): DebugLane {
  switch (kind) {
    case "module":
    case "part":
    case "computed":
    case "invariant":
    case "observation":
    case "port":
    case "presentation":
    case "ui-event":
      return "input";
    case "input":
    case "transition":
    case "commit-hook":
    case "update":
      return "handler";
    case "machine":
    case "state":
    case "command":
    case "outcome":
      return "effect";
  }
}

interface RuntimeFacts {
  readonly reaction: ReactionReceipt | null;
  readonly inputLabel: string | null;
  readonly inputPort: string | null;
  readonly commandLabels: readonly string[];
  readonly commandPorts: ReadonlySet<string>;
  readonly outcomeLabel: string | null;
  readonly commit: boolean;
  readonly stateFields: ReadonlyMap<string, Value>;
  readonly writtenFields: ReadonlySet<string>;
}

function runtimeFacts(state: RuntimeInspectionState): RuntimeFacts {
  const latest = state.latest;
  const receipt = latest?.receipt;
  const reaction = receipt?.kind === "reaction" ? receipt : null;
  const stateFields = recordFields(latest?.inspection.state ?? null);
  const prior = state.history.length > 1
    ? state.history.at(-2)?.inspection.state ?? null
    : null;
  const priorFields = recordFields(prior);
  const writtenFields = new Set<string>();
  if (prior !== null) {
    for (const [field, value] of stateFields) {
      if (!valueEqual(priorFields.get(field), value)) {
        writtenFields.add(field);
      }
    }
    for (const field of priorFields.keys()) {
      if (!stateFields.has(field)) writtenFields.add(field);
    }
  }
  const commandLabels = reaction?.orderedCommands
    .map(commandLabel)
    .filter((label): label is string => label !== null) ?? [];
  const commandPorts = new Set(
    reaction?.orderedCommands.flatMap((command) =>
      command.target === "port" ? [command.port] : []) ?? [],
  );
  const completed = reaction?.resolution.kind === "completed"
    ? reaction.resolution
    : null;
  return {
    reaction,
    inputLabel: reaction ? inputLabel(reaction.input) : null,
    inputPort: reaction?.input.source === "port"
      ? reaction.input.port
      : null,
    commandLabels,
    commandPorts,
    outcomeLabel: completed === null
      ? null
      : constructor(completed.outcome),
    commit: completed?.disposition === "commit",
    stateFields,
    writtenFields,
  };
}

function emptyModel(
  state: RuntimeInspectionState,
  reason: DebugEmptyReason,
): DebugGraphModel {
  return {
    disposed: state.disposed,
    emptyReason: reason,
    generation: state.artifacts?.generation ?? null,
    programHash: state.artifacts?.deployment.machineProgramHash ?? null,
    exactSequence: state.latest?.inspection.nextSequence ?? null,
    focusDefinitionId: null,
    runtimeDefinitionId: null,
    definitions: [],
    nodes: [],
    edges: [],
  };
}

function nodeDetail(
  kind: GraphNodeKind,
  label: string,
  state: RuntimeInspectionState,
  facts: RuntimeFacts,
  runtimeMachine: boolean,
  policy: OutcomePolicy | null,
): string | null {
  const inspection = state.latest?.inspection;
  switch (kind) {
    case "module":
      return "Source module";
    case "machine":
      return runtimeMachine && inspection
        ? `Machine · ${inspection.lifecycle}`
        : "Machine";
    case "part":
      return "Composed part";
    case "port":
      return facts.inputPort === label
        ? "Inbound port"
        : facts.commandPorts.has(label)
          ? "Outbound port"
          : "Port";
    case "input":
      return facts.reaction && labelMatches(label, facts.inputLabel)
        ? `Input · ${formatDebugValue(facts.reaction.input.value)}`
        : "Input handler";
    case "transition":
      return "Named transition";
    case "commit-hook":
      return "Atomic commit hook";
    case "state": {
      const value = facts.stateFields.get(label);
      return value === undefined ? "State" : formatDebugValue(value);
    }
    case "computed":
      return "Computed read";
    case "invariant":
      return "Invariant";
    case "update":
      return "Callable update";
    case "observation":
      return "Committed observation";
    case "command": {
      const commands = facts.reaction?.orderedCommands.filter((command) =>
        labelMatches(label, commandLabel(command))) ?? [];
      if (commands.length === 0) return "Command";
      return commands
        .map((command) => formatDebugValue(command.value))
        .join(" · ");
    }
    case "outcome": {
      const resolution = facts.reaction?.resolution;
      return resolution?.kind === "completed"
        && labelMatches(label, facts.outcomeLabel)
        ? `${resolution.disposition} · ${
          formatDebugValue(resolution.outcome)
        }`
        : policy === null
          ? "Outcome"
          : `Outcome · ${policy}`;
    }
    case "presentation":
      return runtimeMachine && inspection
        ? `Observation · ${formatDebugValue(inspection.observation)}`
        : "Presentation";
    case "ui-event":
      return "Checked UI event binding";
  }
}

function edgeKey(
  edge: { readonly kind: GraphEdgeKind; readonly from: string; readonly to: string },
): string {
  return `${edge.kind}\u0000${edge.from}\u0000${edge.to}`;
}

/**
 * Projects one inspection publication into a stable, machine-sized graph.
 * Runtime receipts decorate admitted nodes and edges conservatively.
 */
export function deriveDebugGraph(
  state: RuntimeInspectionState,
  options: DeriveDebugGraphOptions = {},
): DebugGraphModel {
  const artifacts = state.artifacts;
  if (artifacts === null) {
    return emptyModel(state, state.disposed ? "disposed" : "loading");
  }
  const deployment = artifacts.deployment;
  const graph = deployment.interactionGraph;
  const machineNodes = graph.nodes.filter((node) => node.kind === "machine");
  if (machineNodes.length === 0) {
    return emptyModel(state, "no-machines");
  }
  const deployedMachineNode = machineNodes.find(
    (node) => node.machine === deployment.machine,
  ) ?? null;
  const validDefinitionIds = new Set(machineNodes.map((node) => node.id));
  const requested = options.focusDefinitionId;
  const focusDefinitionId = requested && validDefinitionIds.has(requested)
    ? requested
    : deployedMachineNode?.id
      ?? [...validDefinitionIds].sort(compareText)[0]
      ?? null;
  const runtimeDefinitionId = state.latest === null
    ? null
    : deployedMachineNode?.id ?? null;
  const definitions = machineNodes
    .map((node): DebugDefinitionOption => ({
      id: node.id,
      kind: "machine",
      label: node.label,
      entry: node.machine === deployment.machine,
      active: node.machine === deployment.machine
        && state.latest?.inspection.lifecycle !== "stopped",
      runtime: node.id === runtimeDefinitionId,
    }))
    .sort((left, right) =>
      compareText(left.label, right.label) || compareText(left.id, right.id));
  if (focusDefinitionId === null) {
    return {
      ...emptyModel(state, "no-machines"),
      definitions,
    };
  }
  const focusedMachine = machineNodes.find(
    (node) => node.id === focusDefinitionId,
  )?.machine;
  if (focusedMachine === undefined) {
    return {
      ...emptyModel(state, "no-machines"),
      definitions,
    };
  }

  const runtimeMachine = focusedMachine === deployment.machine;
  const facts: RuntimeFacts = runtimeMachine
    ? runtimeFacts(state)
    : {
        reaction: null,
        inputLabel: null,
        inputPort: null,
        commandLabels: [],
        commandPorts: new Set(),
        outcomeLabel: null,
        commit: false,
        stateFields: new Map(),
        writtenFields: new Set(),
      };
  const nodeSources = new Map(
    deployment.graphSources.nodes.map((entry) => [entry.node, entry.sources]),
  );
  const included = graph.nodes.filter((node) => node.machine === focusedMachine);
  const includedIds = new Set(included.map((node) => node.id));
  const activeMachine = focusedMachine === deployment.machine
    && state.latest?.inspection.lifecycle !== "stopped";
  const nodes = included
    .map((node, order): DebugGraphNode => {
      const inputCurrent = node.kind === "input"
        && labelMatches(node.label, facts.inputLabel);
      const commandCurrent = node.kind === "command"
        && facts.commandLabels.some((label) =>
          labelMatches(node.label, label));
      const outcomeCurrent = node.kind === "outcome"
        && labelMatches(node.label, facts.outcomeLabel);
      const hookCurrent = node.kind === "commit-hook" && facts.commit;
      const stateWritten = node.kind === "state"
        && facts.writtenFields.has(node.label);
      const portCurrent = node.kind === "port"
        && (facts.inputPort === node.label || facts.commandPorts.has(node.label));
      const machineCurrent = runtimeMachine
        && node.kind === "machine"
        && state.latest !== null;
      const presentationCurrent = node.kind === "presentation"
        && runtimeMachine
        && state.latest !== null
        && node.label === deployment.presentation;
      const current = inputCurrent
        || commandCurrent
        || outcomeCurrent
        || hookCurrent
        || stateWritten
        || portCurrent
        || machineCurrent
        || presentationCurrent;
      const sources = (nodeSources.get(node.id) ?? []).map(sourceSpan);
      const first = sources[0];
      return {
        id: node.id,
        kind: node.kind,
        lane: lane(node.kind),
        definitionId: focusDefinitionId,
        label: node.label,
        detail: nodeDetail(
          node.kind,
          node.label,
          state,
          facts,
          runtimeMachine,
          graph.outcomePolicies[node.id] ?? null,
        ),
        order,
        span: first
          ? { file: first.file, start: first.start, end: first.end }
          : null,
        sourceSpans: sources,
        runtime: {
          active: activeMachine,
          current,
          selected: inputCurrent || hookCurrent,
          written: stateWritten,
          sent: commandCurrent,
        },
      };
    })
    .sort((left, right) =>
      LANE_ORDER[left.lane] - LANE_ORDER[right.lane]
      || NODE_KIND_ORDER[left.kind] - NODE_KIND_ORDER[right.kind]
      || left.order - right.order
      || compareText(left.id, right.id));
  const runtimeById = new Map(nodes.map((node) => [node.id, node.runtime]));
  const edgeSources = new Map(
    deployment.graphSources.edges.map((entry) => [
      edgeKey(entry.edge),
      entry.sources,
    ]),
  );
  const edges = graph.edges
    .filter((edge) => includedIds.has(edge.from) && includedIds.has(edge.to))
    .map((edge, order): DebugGraphEdge => {
      const from = runtimeById.get(edge.from);
      const to = runtimeById.get(edge.to);
      let activity: DebugEdgeActivity = "idle";
      switch (edge.kind) {
        case "delivers":
          if (from?.current && to?.selected) activity = "taken";
          break;
        case "writes":
          if (from?.current && to?.written) activity = "taken";
          break;
        case "emits":
          if (from?.current && to?.sent) activity = "taken";
          break;
        case "finishes":
        case "triggers":
          if (from?.current && to?.current) activity = "taken";
          break;
        case "sends-via":
          if (from?.sent && to?.current) activity = "taken";
          break;
        case "dispatches":
          if (to?.selected) activity = "context";
          break;
        case "projects":
        case "exposes":
          if (from?.current || to?.current) activity = "context";
          break;
        case "owns":
        case "composes":
        case "reads":
        case "calls":
        case "observes":
          if (to?.current) activity = "context";
          break;
        case "delegates":
          // Receipts do not expose internal transition paths.
          break;
      }
      const sources = (edgeSources.get(edgeKey(edge)) ?? []).map(sourceSpan);
      return {
        id: `edge/${edge.kind}/${edge.from}/${edge.to}`,
        kind: edge.kind,
        from: edge.from,
        to: edge.to,
        label: edge.kind,
        order,
        activity,
        sourceSpans: sources,
      };
    });

  return {
    disposed: false,
    emptyReason: null,
    generation: artifacts.generation,
    programHash: graph.machineProgramHashes[focusedMachine]
      ?? deployment.machineProgramHash,
    exactSequence: state.latest?.inspection.nextSequence ?? null,
    focusDefinitionId,
    runtimeDefinitionId,
    definitions,
    nodes,
    edges,
  };
}
