import {
  decodeIdentityProtocol,
  hash,
  type Hash,
  type UhuraIdentityProtocol,
} from "./machine.js";

export const UHURA_INTERACTION_GRAPH_PROTOCOL =
  "uhura-interaction-graph/0" as const;
export const UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL =
  "uhura-interaction-graph-provenance/0" as const;

export type GraphNodeKind =
  | "module"
  | "machine"
  | "part"
  | "port"
  | "input"
  | "transition"
  | "commit-hook"
  | "state"
  | "computed"
  | "invariant"
  | "update"
  | "observation"
  | "command"
  | "outcome"
  | "presentation"
  | "ui-event";

export type GraphEdgeKind =
  | "owns"
  | "composes"
  | "reads"
  | "calls"
  | "observes"
  | "delivers"
  | "writes"
  | "emits"
  | "finishes"
  | "triggers"
  | "delegates"
  | "sends-via"
  | "projects"
  | "exposes"
  | "dispatches";

export type OutcomePolicy = "commit" | "abort";

export interface GraphNode {
  readonly id: string;
  readonly kind: GraphNodeKind;
  readonly machine: string;
  readonly label: string;
}

export interface GraphEdge {
  readonly from: string;
  readonly to: string;
  readonly kind: GraphEdgeKind;
}

export interface InteractionGraph {
  readonly protocol: typeof UHURA_INTERACTION_GRAPH_PROTOCOL;
  readonly identityProtocol: UhuraIdentityProtocol;
  readonly machineProgramHashes: Readonly<Record<string, Hash>>;
  readonly presentationHashes: Readonly<Record<string, Hash>>;
  /** Closed map keyed by every outcome node ID, and by no other node. */
  readonly outcomePolicies: Readonly<Record<string, OutcomePolicy>>;
  readonly nodes: readonly GraphNode[];
  readonly edges: readonly GraphEdge[];
}

export interface GraphSourceRef {
  readonly id: string;
  readonly path: string;
  readonly start: number;
  readonly end: number;
}

export interface GraphNodeSources {
  readonly node: string;
  readonly sources: readonly GraphSourceRef[];
}

export interface GraphEdgeSources {
  readonly edge: GraphEdge;
  readonly sources: readonly GraphSourceRef[];
}

export interface InteractionGraphSources {
  readonly protocol: typeof UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL;
  readonly nodes: readonly GraphNodeSources[];
  readonly edges: readonly GraphEdgeSources[];
}

export interface InteractionGraphArtifacts {
  readonly graph: InteractionGraph;
  readonly sources: InteractionGraphSources;
}

const NODE_KINDS = new Set<GraphNodeKind>([
  "module",
  "machine",
  "part",
  "port",
  "input",
  "transition",
  "commit-hook",
  "state",
  "computed",
  "invariant",
  "update",
  "observation",
  "command",
  "outcome",
  "presentation",
  "ui-event",
]);

const EDGE_KINDS = new Set<GraphEdgeKind>([
  "owns",
  "composes",
  "reads",
  "calls",
  "observes",
  "delivers",
  "writes",
  "emits",
  "finishes",
  "triggers",
  "delegates",
  "sends-via",
  "projects",
  "exposes",
  "dispatches",
]);

const object = (
  value: unknown,
  context: string,
): Readonly<Record<string, unknown>> => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError(`${context} must be an object`);
  }
  return value as Readonly<Record<string, unknown>>;
};

const list = (value: unknown, context: string): readonly unknown[] => {
  if (!Array.isArray(value)) throw new TypeError(`${context} must be a list`);
  return value;
};

const exactKeys = (
  value: Readonly<Record<string, unknown>>,
  keys: readonly string[],
  context: string,
): void => {
  const expected = new Set(keys);
  const missing = keys.filter((key) => !Object.hasOwn(value, key));
  const extra = Object.keys(value).filter((key) => !expected.has(key));
  if (missing.length > 0 || extra.length > 0) {
    throw new TypeError(
      `${context} has the wrong fields; missing [${missing.join(", ")}], extra [${extra.join(", ")}]`,
    );
  }
};

const text = (value: unknown, context: string): string => {
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`${context} must be nonempty text`);
  }
  return value;
};

const integer = (value: unknown, context: string): number => {
  if (!Number.isSafeInteger(value) || (value as number) < 0) {
    throw new TypeError(`${context} must be a non-negative safe integer`);
  }
  return value as number;
};

const nodeKind = (value: unknown, context: string): GraphNodeKind => {
  if (typeof value !== "string" || !NODE_KINDS.has(value as GraphNodeKind)) {
    throw new TypeError(`${context} is not a supported interaction-graph node kind`);
  }
  return value as GraphNodeKind;
};

const edgeKind = (value: unknown, context: string): GraphEdgeKind => {
  if (typeof value !== "string" || !EDGE_KINDS.has(value as GraphEdgeKind)) {
    throw new TypeError(`${context} is not a supported interaction-graph edge kind`);
  }
  return value as GraphEdgeKind;
};

const outcomePolicy = (value: unknown, context: string): OutcomePolicy => {
  if (value !== "commit" && value !== "abort") {
    throw new TypeError(`${context} must be "commit" or "abort"`);
  }
  return value;
};

const decodeNode = (value: unknown, context: string): GraphNode => {
  const node = object(value, context);
  exactKeys(node, ["id", "kind", "machine", "label"], context);
  return {
    id: text(node["id"], `${context}.id`),
    kind: nodeKind(node["kind"], `${context}.kind`),
    machine: text(node["machine"], `${context}.machine`),
    label: text(node["label"], `${context}.label`),
  };
};

const decodeEdge = (value: unknown, context: string): GraphEdge => {
  const edge = object(value, context);
  exactKeys(edge, ["from", "to", "kind"], context);
  return {
    from: text(edge["from"], `${context}.from`),
    to: text(edge["to"], `${context}.to`),
    kind: edgeKind(edge["kind"], `${context}.kind`),
  };
};

const decodeHashes = (
  value: unknown,
  context: string,
): Readonly<Record<string, Hash>> => {
  const source = object(value, context);
  const result: Record<string, Hash> = {};
  for (const [identity, digest] of Object.entries(source)) {
    if (identity.length === 0) {
      throw new TypeError(`${context} identities must be nonempty`);
    }
    result[identity] = hash(text(digest, `${context}.${identity}`));
  }
  return result;
};

export const decodeSourceRef = (
  value: unknown,
  context: string,
): GraphSourceRef => {
  const source = object(value, context);
  exactKeys(source, ["id", "path", "start", "end"], context);
  const start = integer(source["start"], `${context}.start`);
  const end = integer(source["end"], `${context}.end`);
  if (end < start) {
    throw new TypeError(`${context}.end must not precede its start`);
  }
  return {
    id: text(source["id"], `${context}.id`),
    path: text(source["path"], `${context}.path`),
    start,
    end,
  };
};

const decodeSources = (
  value: unknown,
  context: string,
): readonly GraphSourceRef[] => {
  const sources = list(value, context).map((source, index) =>
    decodeSourceRef(source, `${context}[${index}]`)
  );
  if (sources.length === 0) {
    throw new TypeError(`${context} must contain at least one source span`);
  }
  return sources;
};

const edgeKey = (edge: GraphEdge): string =>
  `${edge.kind}\u0000${edge.from}\u0000${edge.to}`;

export const decodeInteractionGraphArtifacts = (
  graphValue: unknown,
  sourcesValue: unknown,
): InteractionGraphArtifacts => {
  const graphSource = object(graphValue, "interaction graph");
  exactKeys(
    graphSource,
    [
      "protocol",
      "identity_protocol",
      "machine_program_hashes",
      "presentation_hashes",
      "outcome_policies",
      "nodes",
      "edges",
    ],
    "interaction graph",
  );
  if (graphSource["protocol"] !== UHURA_INTERACTION_GRAPH_PROTOCOL) {
    throw new TypeError(
      `interaction graph.protocol must be ${JSON.stringify(UHURA_INTERACTION_GRAPH_PROTOCOL)}`,
    );
  }
  const identityProtocol = decodeIdentityProtocol(
    graphSource["identity_protocol"],
    "interaction graph.identity_protocol",
  );
  const nodes = list(graphSource["nodes"], "interaction graph.nodes")
    .map((node, index) =>
      decodeNode(node, `interaction graph.nodes[${index}]`)
    );
  const nodeIds = new Set<string>();
  for (const node of nodes) {
    if (nodeIds.has(node.id)) {
      throw new TypeError(
        `interaction graph contains duplicate node ${JSON.stringify(node.id)}`,
      );
    }
    nodeIds.add(node.id);
  }
  const outcomeNodeIds = new Set(
    nodes
      .filter((node) => node.kind === "outcome")
      .map((node) => node.id),
  );
  const rawOutcomePolicies = object(
    graphSource["outcome_policies"],
    "interaction graph.outcome_policies",
  );
  const outcomePolicies: Record<string, OutcomePolicy> = {};
  for (const [nodeId, policy] of Object.entries(rawOutcomePolicies)) {
    if (!outcomeNodeIds.has(nodeId)) {
      throw new TypeError(
        "interaction graph outcome policy must reference a declared outcome node",
      );
    }
    outcomePolicies[nodeId] = outcomePolicy(
      policy,
      `interaction graph.outcome_policies.${nodeId}`,
    );
  }
  if (
    outcomeNodeIds.size !== Object.keys(outcomePolicies).length
    || [...outcomeNodeIds].some((nodeId) =>
      !Object.hasOwn(outcomePolicies, nodeId))
  ) {
    throw new TypeError(
      "interaction graph outcome policies must cover every outcome node",
    );
  }
  const edges = list(graphSource["edges"], "interaction graph.edges")
    .map((edge, index) =>
      decodeEdge(edge, `interaction graph.edges[${index}]`)
    );
  const edgeKeys = new Set<string>();
  for (const edge of edges) {
    if (!nodeIds.has(edge.from) || !nodeIds.has(edge.to)) {
      throw new TypeError(
        "interaction graph edge must reference declared nodes",
      );
    }
    const key = edgeKey(edge);
    if (edgeKeys.has(key)) {
      throw new TypeError("interaction graph contains a duplicate edge");
    }
    edgeKeys.add(key);
  }
  const machineProgramHashes = decodeHashes(
    graphSource["machine_program_hashes"],
    "interaction graph.machine_program_hashes",
  );
  const presentationHashes = decodeHashes(
    graphSource["presentation_hashes"],
    "interaction graph.presentation_hashes",
  );
  for (const node of nodes) {
    if (!Object.hasOwn(machineProgramHashes, node.machine)) {
      throw new TypeError(
        "interaction graph node must reference a hashed machine",
      );
    }
    if (
      node.kind === "presentation"
      && !Object.hasOwn(presentationHashes, node.label)
    ) {
      throw new TypeError(
        "interaction graph presentation node must reference a hashed presentation",
      );
    }
  }
  for (const machine of Object.keys(machineProgramHashes)) {
    if (
      nodes.filter((node) =>
        node.kind === "machine" && node.machine === machine
      ).length !== 1
    ) {
      throw new TypeError(
        "interaction graph must contain one machine node per machine identity",
      );
    }
  }
  for (const presentation of Object.keys(presentationHashes)) {
    if (
      nodes.filter((node) =>
        node.kind === "presentation" && node.label === presentation
      ).length !== 1
    ) {
      throw new TypeError(
        "interaction graph must contain one presentation node per presentation identity",
      );
    }
  }
  const graph: InteractionGraph = {
    protocol: UHURA_INTERACTION_GRAPH_PROTOCOL,
    identityProtocol,
    machineProgramHashes,
    presentationHashes,
    outcomePolicies,
    nodes,
    edges,
  };

  const sourcesSource = object(
    sourcesValue,
    "interaction graph sources",
  );
  exactKeys(
    sourcesSource,
    ["protocol", "nodes", "edges"],
    "interaction graph sources",
  );
  if (
    sourcesSource["protocol"]
      !== UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL
  ) {
    throw new TypeError(
      `interaction graph sources.protocol must be ${JSON.stringify(UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL)}`,
    );
  }
  const sourceNodeIds = new Set<string>();
  const sourceNodes = list(
    sourcesSource["nodes"],
    "interaction graph sources.nodes",
  ).map((value, index): GraphNodeSources => {
    const entry = object(
      value,
      `interaction graph sources.nodes[${index}]`,
    );
    exactKeys(
      entry,
      ["node", "sources"],
      `interaction graph sources.nodes[${index}]`,
    );
    const node = text(
      entry["node"],
      `interaction graph sources.nodes[${index}].node`,
    );
    if (!nodeIds.has(node)) {
      throw new TypeError(
        "interaction graph source entry must reference a declared node",
      );
    }
    if (sourceNodeIds.has(node)) {
      throw new TypeError(
        "interaction graph contains duplicate node source entries",
      );
    }
    sourceNodeIds.add(node);
    return {
      node,
      sources: decodeSources(
        entry["sources"],
        `interaction graph sources.nodes[${index}].sources`,
      ),
    };
  });
  if (sourceNodeIds.size !== nodeIds.size) {
    throw new TypeError(
      "interaction graph sources must cover every semantic node",
    );
  }

  const sourceEdgeKeys = new Set<string>();
  const sourceEdges = list(
    sourcesSource["edges"],
    "interaction graph sources.edges",
  ).map((value, index): GraphEdgeSources => {
    const entry = object(
      value,
      `interaction graph sources.edges[${index}]`,
    );
    exactKeys(
      entry,
      ["edge", "sources"],
      `interaction graph sources.edges[${index}]`,
    );
    const edge = decodeEdge(
      entry["edge"],
      `interaction graph sources.edges[${index}].edge`,
    );
    const key = edgeKey(edge);
    if (!edgeKeys.has(key)) {
      throw new TypeError(
        "interaction graph edge source must reference a semantic edge",
      );
    }
    if (sourceEdgeKeys.has(key)) {
      throw new TypeError(
        "interaction graph contains duplicate edge source entries",
      );
    }
    sourceEdgeKeys.add(key);
    return {
      edge,
      sources: decodeSources(
        entry["sources"],
        `interaction graph sources.edges[${index}].sources`,
      ),
    };
  });
  if (sourceEdgeKeys.size !== edgeKeys.size) {
    throw new TypeError(
      "interaction graph sources must cover every semantic edge",
    );
  }

  return {
    graph,
    sources: {
      protocol: UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL,
      nodes: sourceNodes,
      edges: sourceEdges,
    },
  };
};
