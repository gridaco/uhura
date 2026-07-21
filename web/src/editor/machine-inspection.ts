import type {
  EditorMachine,
  JsonValue,
  PreviewEvidence,
} from "./editor-state.js";

export interface InspectionRow {
  label: string;
  value: string;
}

export interface MachineInspection {
  identity: InspectionRow[];
  status: "passed" | "failed" | "unknown";
  passes: number;
  failures: number;
  checkpoints: number;
  sources: number;
  ownership: InspectionRow[];
  outcomes: InspectionRow[];
  dependencies: InspectionRow[];
}

type JsonRecord = Record<string, JsonValue>;

interface InspectionGraphNode {
  id: string;
  kind: string;
  machine: string;
  label: string;
}

interface InspectionGraphEdge {
  from: string;
  to: string;
  kind: string;
}

const jsonRecord = (value: JsonValue | undefined): JsonRecord | null =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? value
    : null;

const nonEmptyString = (value: JsonValue | undefined): string | null =>
  typeof value === "string" && value.length > 0 ? value : null;

const collectionSize = (value: JsonValue): number => {
  if (Array.isArray(value)) return value.length;
  const object = jsonRecord(value);
  return object ? Object.keys(object).length : 0;
};

const graphNodes = (value: JsonValue): InspectionGraphNode[] => {
  const nodes = jsonRecord(value)?.["nodes"];
  if (!Array.isArray(nodes)) return [];
  return nodes.flatMap((value) => {
    const node = jsonRecord(value);
    const id = nonEmptyString(node?.["id"]);
    const kind = nonEmptyString(node?.["kind"]);
    const machine = nonEmptyString(node?.["machine"]);
    const label = nonEmptyString(node?.["label"]);
    return id && kind && machine && label
      ? [{ id, kind, machine, label }]
      : [];
  });
};

const graphEdges = (value: JsonValue): InspectionGraphEdge[] => {
  const edges = jsonRecord(value)?.["edges"];
  if (!Array.isArray(edges)) return [];
  return edges.flatMap((value) => {
    const edge = jsonRecord(value);
    const from = nonEmptyString(edge?.["from"]);
    const to = nonEmptyString(edge?.["to"]);
    const kind = nonEmptyString(edge?.["kind"]);
    return from && to && kind ? [{ from, to, kind }] : [];
  });
};

const memberSummary = (
  ids: ReadonlySet<string>,
  nodes: ReadonlyMap<string, InspectionGraphNode>,
): string => {
  const counts = new Map<string, number>();
  for (const id of ids) {
    const kind = nodes.get(id)?.kind;
    if (
      kind === "state"
      || kind === "computed"
      || kind === "invariant"
      || kind === "update"
      || kind === "observation"
    ) {
      counts.set(kind, (counts.get(kind) ?? 0) + 1);
    }
  }
  return ["state", "computed", "invariant", "update", "observation"]
    .flatMap((kind) => {
      const count = counts.get(kind) ?? 0;
      return count > 0 ? [`${count} ${kind}${count === 1 ? "" : "s"}`] : [];
    })
    .join(" · ");
};

const ownershipRows = (
  graph: JsonValue,
  machine: string | null,
): InspectionRow[] => {
  const allNodes = graphNodes(graph);
  const nodes = allNodes.filter((node) => machine === null || node.machine === machine);
  const nodeById = new Map(nodes.map((node) => [node.id, node]));
  const edges = graphEdges(graph).filter((edge) =>
    nodeById.has(edge.from) && nodeById.has(edge.to)
  );
  const modules = nodes
    .filter((node) => node.kind === "module")
    .map((node) => node.label)
    .filter((label, index, labels) => labels.indexOf(label) === index)
    .sort();
  const parts = nodes.filter((node) => node.kind === "part")
    .sort((left, right) => left.label.localeCompare(right.label));
  const partOwned = new Set(
    edges
      .filter((edge) =>
        edge.kind === "owns" && nodeById.get(edge.from)?.kind === "part"
      )
      .map((edge) => edge.to),
  );
  const machineOwned = new Set(
    nodes
      .filter((node) =>
        ["state", "computed", "invariant", "update", "observation"].includes(node.kind)
        && !partOwned.has(node.id)
      )
      .map((node) => node.id),
  );

  return [
    ...modules.map((value) => ({ label: "Module", value })),
    ...(machineOwned.size > 0
      ? [{ label: "Machine-owned", value: memberSummary(machineOwned, nodeById) }]
      : []),
    ...parts.map((part) => {
      const owned = new Set(
        edges
          .filter((edge) => edge.kind === "owns" && edge.from === part.id)
          .map((edge) => edge.to),
      );
      return {
        label: `Part ${part.label}`,
        value: memberSummary(owned, nodeById) || "No stateful members",
      };
    }),
  ];
};

const outcomeRows = (
  graph: JsonValue,
  machine: string | null,
): InspectionRow[] => {
  const policies = jsonRecord(jsonRecord(graph)?.["outcome_policies"]);
  if (policies === null) return [];
  return graphNodes(graph)
    .filter((node) =>
      node.kind === "outcome"
      && (machine === null || node.machine === machine)
    )
    .flatMap((node) => {
      const policy = nonEmptyString(policies[node.id]);
      return policy === "commit" || policy === "abort"
        ? [{ label: `Outcome ${node.label}`, value: policy }]
        : [];
    })
    .sort((left, right) => left.label.localeCompare(right.label));
};

const dependencyRows = (
  graph: JsonValue,
  machine: string | null,
): InspectionRow[] => {
  const nodes = graphNodes(graph)
    .filter((node) => machine === null || node.machine === machine);
  const nodeById = new Map(nodes.map((node) => [node.id, node]));
  const edges = graphEdges(graph)
    .filter((edge) =>
      ["reads", "calls", "observes"].includes(edge.kind)
      && nodeById.has(edge.from)
      && nodeById.has(edge.to)
    );
  return [
    { kind: "reads", label: "Reads" },
    { kind: "calls", label: "Calls" },
    { kind: "observes", label: "Observes" },
  ].flatMap(({ kind, label }) => {
    const matches = edges.filter((edge) => edge.kind === kind);
    if (matches.length === 0) return [];
    const examples = matches.slice(0, 2).map((edge) =>
      `${nodeById.get(edge.from)!.label} → ${nodeById.get(edge.to)!.label}`
    );
    return [{
      label,
      value: `${matches.length} · ${examples.join(", ")}${
        matches.length > examples.length ? ", …" : ""
      }`,
    }];
  });
};

export const inspectMachine = (machine: EditorMachine): MachineInspection => {
  const deployment = jsonRecord(machine.deployment);
  const entry = nonEmptyString(deployment?.["entry"]);
  const machineName = nonEmptyString(deployment?.["machine"]);
  const presentation = nonEmptyString(deployment?.["presentation"]);
  const evidence = machine.evidence;

  return {
    identity: [
      entry ? { label: "Deployment", value: entry } : null,
      machineName ? { label: "Machine", value: machineName } : null,
      presentation ? { label: "Presentation", value: presentation } : null,
    ].filter((row): row is InspectionRow => row !== null),
    status: evidence.passed ? "passed" : "failed",
    passes: evidence.scenarios.passed,
    failures: evidence.failureCount,
    checkpoints: evidence.artifacts.checkpoints,
    sources: collectionSize(machine.sources),
    ownership: ownershipRows(machine.interactionGraph, machineName),
    outcomes: outcomeRows(machine.interactionGraph, machineName),
    dependencies: dependencyRows(machine.interactionGraph, machineName),
  };
};

export const machineMetricRows = (
  inspection: MachineInspection,
): InspectionRow[] => [
  { label: "Passes", value: String(inspection.passes) },
  { label: "Failures", value: String(inspection.failures) },
  { label: "Checkpoints", value: String(inspection.checkpoints) },
  { label: "Sources", value: String(inspection.sources) },
];

export const previewEvidenceRows = (
  evidence: PreviewEvidence,
): InspectionRow[] => [
  { label: "Scenario", value: evidence.scenario },
  { label: "Pin", value: evidence.pin },
  { label: "Source", value: evidence.sourceId },
];

export const renderInspectionRows = (
  document: Document,
  root: HTMLElement,
  rows: readonly InspectionRow[],
): void => {
  root.replaceChildren(...rows.map((row) => {
    const group = document.createElement("div");
    const term = document.createElement("dt");
    term.textContent = row.label;
    const description = document.createElement("dd");
    description.textContent = row.value;
    group.append(term, description);
    return group;
  }));
};
