import { hash, type Hash } from "./machine.js";
import type { GraphEdgeKind, GraphNodeKind } from "./interaction-graph.js";

export const UHURA_PROVENANCE_PROTOCOL = "uhura-provenance/0" as const;
export const UHURA_AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL =
  "uhura-authored-interaction-topology/0" as const;

export interface SemanticProvenanceSource {
  readonly source: number;
  readonly package: string;
  readonly module: string;
  readonly path: string;
  readonly sha256: Hash;
  readonly bytes: number;
}

export interface SemanticProvenanceOccurrence {
  readonly node: Hash;
  readonly source: number;
  readonly start: number;
  readonly end: number;
  readonly role: string;
  readonly owner: string;
}

export interface SemanticProvenanceSelector {
  readonly node: Hash;
  readonly role: string;
  readonly owner: string;
}

export interface AuthoredInteractionNode {
  readonly id: string;
  readonly kind: GraphNodeKind;
  readonly machine: string;
  readonly label: string;
  readonly sources: readonly SemanticProvenanceSelector[];
}

export interface AuthoredInteractionEdge {
  readonly from: string;
  readonly to: string;
  readonly kind: GraphEdgeKind;
  readonly sources: readonly SemanticProvenanceSelector[];
}

export interface AuthoredInteractionTopology {
  readonly protocol: typeof UHURA_AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL;
  readonly nodes: readonly AuthoredInteractionNode[];
  readonly edges: readonly AuthoredInteractionEdge[];
}

export interface SemanticProvenance {
  readonly protocol: typeof UHURA_PROVENANCE_PROTOCOL;
  readonly sources: readonly SemanticProvenanceSource[];
  readonly occurrences: readonly SemanticProvenanceOccurrence[];
  readonly topology: AuthoredInteractionTopology;
}

const object = (
  value: unknown,
  context: string,
): Readonly<Record<string, unknown>> => {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError(`${context} must be an object`);
  }
  return value as Readonly<Record<string, unknown>>;
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

const list = (value: unknown, context: string): readonly unknown[] => {
  if (!Array.isArray(value)) throw new TypeError(`${context} must be a list`);
  return value;
};

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

const LOWER_SNAKE = /^[a-z][a-z0-9_]*$/u;
const LOWER_KEBAB = /^[a-z](?:[a-z0-9]|-(?!-))*[a-z0-9]$|^[a-z]$/u;

const validRole = (value: string): boolean => {
  if (value === "definition" || value === "reference" || value === "generated") {
    return true;
  }
  const slash = value.indexOf("/");
  const colon = value.indexOf(":", slash + 1);
  return slash > 0
    && colon > slash + 1
    && LOWER_KEBAB.test(value.slice(0, slash))
    && /^\d+$/u.test(value.slice(slash + 1, colon))
    && LOWER_KEBAB.test(value.slice(colon + 1));
};

const validOwner = (value: string): boolean =>
  value === "root" || value.split(".").every((part) => LOWER_SNAKE.test(part));

const validPath = (value: string): boolean =>
  !value.startsWith("/")
  && !value.includes("\\")
  && value.split("/").every((part) => part.length > 0 && part !== "." && part !== "..");

const compareText = (left: string, right: string): number =>
  left < right ? -1 : left > right ? 1 : 0;

/**
 * Decode source-layout-sensitive language provenance.
 */
export const decodeSemanticProvenance = (
  value: unknown,
  context = "Uhura semantic provenance",
): SemanticProvenance => {
  const root = object(value, context);
  exactKeys(root, ["protocol", "sources", "occurrences", "topology"], context);
  if (root["protocol"] !== UHURA_PROVENANCE_PROTOCOL) {
    throw new TypeError(
      `${context}.protocol must be ${JSON.stringify(UHURA_PROVENANCE_PROTOCOL)}`,
    );
  }

  const sources = list(root["sources"], `${context}.sources`).map(
    (value, index): SemanticProvenanceSource => {
      const item = object(value, `${context}.sources[${index}]`);
      exactKeys(
        item,
        ["source", "package", "module", "path", "sha256", "bytes"],
        `${context}.sources[${index}]`,
      );
      const source = integer(
        item["source"],
        `${context}.sources[${index}].source`,
      );
      if (source !== index) {
        throw new TypeError(
          `${context}.sources must use contiguous source indices from zero`,
        );
      }
      const path = text(item["path"], `${context}.sources[${index}].path`);
      if (!validPath(path)) {
        throw new TypeError(
          `${context}.sources[${index}].path must be project-relative`,
        );
      }
      return {
        source,
        package: text(item["package"], `${context}.sources[${index}].package`),
        module: text(item["module"], `${context}.sources[${index}].module`),
        path,
        sha256: hash(
          text(item["sha256"], `${context}.sources[${index}].sha256`),
        ),
        bytes: integer(item["bytes"], `${context}.sources[${index}].bytes`),
      };
    },
  );
  const paths = new Set<string>();
  for (const source of sources) {
    const packagePath = `${source.package}\u0000${source.path}`;
    if (paths.has(packagePath)) {
      throw new TypeError(
        `${context}.sources must use unique paths within each package`,
      );
    }
    paths.add(packagePath);
  }

  const occurrences = list(
    root["occurrences"],
    `${context}.occurrences`,
  ).map((value, index): SemanticProvenanceOccurrence => {
    const item = object(value, `${context}.occurrences[${index}]`);
    exactKeys(
      item,
      ["node", "source", "start", "end", "role", "owner"],
      `${context}.occurrences[${index}]`,
    );
    const source = integer(
      item["source"],
      `${context}.occurrences[${index}].source`,
    );
    const start = integer(
      item["start"],
      `${context}.occurrences[${index}].start`,
    );
    const end = integer(
      item["end"],
      `${context}.occurrences[${index}].end`,
    );
    const sourceEntry = sources[source];
    if (sourceEntry === undefined || start > end || end > sourceEntry.bytes) {
      throw new TypeError(
        `${context}.occurrences[${index}] must resolve within its source`,
      );
    }
    const role = text(
      item["role"],
      `${context}.occurrences[${index}].role`,
    );
    const owner = text(
      item["owner"],
      `${context}.occurrences[${index}].owner`,
    );
    if (!validRole(role)) {
      throw new TypeError(
        `${context}.occurrences[${index}].role is not admitted`,
      );
    }
    if (!validOwner(owner)) {
      throw new TypeError(
        `${context}.occurrences[${index}].owner is not admitted`,
      );
    }
    return {
      node: hash(text(item["node"], `${context}.occurrences[${index}].node`)),
      source,
      start,
      end,
      role,
      owner,
    };
  });
  for (let index = 1; index < occurrences.length; index += 1) {
    const previous = occurrences[index - 1]!;
    const current = occurrences[index]!;
    const order = compareText(previous.node, current.node)
      || compareText(
        sources[previous.source]!.package,
        sources[current.source]!.package,
      )
      || compareText(
        sources[previous.source]!.module,
        sources[current.source]!.module,
      )
      || compareText(
        sources[previous.source]!.path,
        sources[current.source]!.path,
      )
      || previous.start - current.start
      || previous.end - current.end
      || compareText(previous.role, current.role)
      || compareText(previous.owner, current.owner);
    if (order >= 0) {
      throw new TypeError(
        `${context}.occurrences must be unique and canonically ordered`,
      );
    }
  }

  const topologySource = object(root["topology"], `${context}.topology`);
  exactKeys(
    topologySource,
    ["protocol", "nodes", "edges"],
    `${context}.topology`,
  );
  if (
    topologySource["protocol"]
      !== UHURA_AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL
  ) {
    throw new TypeError(
      `${context}.topology.protocol must be ${JSON.stringify(UHURA_AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL)}`,
    );
  }
  const occurrenceKeys = new Set(occurrences.map((occurrence) =>
    `${occurrence.node}\u0000${occurrence.role}\u0000${occurrence.owner}`
  ));
  const decodeSelectors = (
    value: unknown,
    selectorContext: string,
  ): readonly SemanticProvenanceSelector[] => {
    const selectors = list(value, selectorContext).map((value, index) => {
      const itemContext = `${selectorContext}[${index}]`;
      const item = object(value, itemContext);
      exactKeys(item, ["node", "role", "owner"], itemContext);
      const selector = {
        node: hash(text(item["node"], `${itemContext}.node`)),
        role: text(item["role"], `${itemContext}.role`),
        owner: text(item["owner"], `${itemContext}.owner`),
      };
      if (!validRole(selector.role) || !validOwner(selector.owner)) {
        throw new TypeError(`${itemContext} is not an admitted provenance selector`);
      }
      if (
        !occurrenceKeys.has(
          `${selector.node}\u0000${selector.role}\u0000${selector.owner}`,
        )
      ) {
        throw new TypeError(`${itemContext} has no matching provenance occurrence`);
      }
      return selector;
    });
    if (selectors.length === 0) {
      throw new TypeError(`${selectorContext} must not be empty`);
    }
    return selectors;
  };

  const nodeIds = new Set<string>();
  const topologyNodes = list(
    topologySource["nodes"],
    `${context}.topology.nodes`,
  ).map((value, index): AuthoredInteractionNode => {
    const itemContext = `${context}.topology.nodes[${index}]`;
    const item = object(value, itemContext);
    exactKeys(item, ["id", "kind", "machine", "label", "sources"], itemContext);
    const id = text(item["id"], `${itemContext}.id`);
    if (nodeIds.has(id)) {
      throw new TypeError(`${context}.topology contains duplicate node identities`);
    }
    nodeIds.add(id);
    const kind = text(item["kind"], `${itemContext}.kind`);
    if (!NODE_KINDS.has(kind as GraphNodeKind)) {
      throw new TypeError(`${itemContext}.kind is not supported`);
    }
    return {
      id,
      kind: kind as GraphNodeKind,
      machine: text(item["machine"], `${itemContext}.machine`),
      label: text(item["label"], `${itemContext}.label`),
      sources: decodeSelectors(item["sources"], `${itemContext}.sources`),
    };
  });
  const edgeKeys = new Set<string>();
  const topologyEdges = list(
    topologySource["edges"],
    `${context}.topology.edges`,
  ).map((value, index): AuthoredInteractionEdge => {
    const itemContext = `${context}.topology.edges[${index}]`;
    const item = object(value, itemContext);
    exactKeys(item, ["from", "to", "kind", "sources"], itemContext);
    const from = text(item["from"], `${itemContext}.from`);
    const to = text(item["to"], `${itemContext}.to`);
    if (!nodeIds.has(from) || !nodeIds.has(to)) {
      throw new TypeError(`${itemContext} must reference declared topology nodes`);
    }
    const kind = text(item["kind"], `${itemContext}.kind`);
    if (!EDGE_KINDS.has(kind as GraphEdgeKind)) {
      throw new TypeError(`${itemContext}.kind is not supported`);
    }
    const edgeKey = `${kind}\u0000${from}\u0000${to}`;
    if (edgeKeys.has(edgeKey)) {
      throw new TypeError(`${context}.topology contains duplicate edges`);
    }
    edgeKeys.add(edgeKey);
    return {
      from,
      to,
      kind: kind as GraphEdgeKind,
      sources: decodeSelectors(item["sources"], `${itemContext}.sources`),
    };
  });

  return {
    protocol: UHURA_PROVENANCE_PROTOCOL,
    sources,
    occurrences,
    topology: {
      protocol: UHURA_AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL,
      nodes: topologyNodes,
      edges: topologyEdges,
    },
  };
};
