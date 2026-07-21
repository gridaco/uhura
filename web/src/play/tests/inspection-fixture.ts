import type { HostInspection } from "../../protocol/host-inspection.js";
import {
  decodeHostInspection,
  UHURA_EVIDENCE_SUMMARY_PROTOCOL,
} from "../../protocol/host-inspection.js";
import {
  UHURA_INTERACTION_GRAPH_PROTOCOL,
  UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL,
} from "../../protocol/interaction-graph.js";
import {
  UHURA_GENESIS_RECEIPT_PROTOCOL,
  UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
  UHURA_REACTION_RECEIPT_PROTOCOL,
  UHURA_RUNTIME_SNAPSHOT_PROTOCOL,
  decodeReceipt,
  decodeRuntimeSnapshot,
  type Receipt,
  type RuntimeSnapshot,
} from "../../protocol/machine.js";

export const UHURA_TEST_MACHINE = "example@1::Counter";
export const UHURA_TEST_PRESENTATION = "counter-ui";
export const UHURA_TEST_MACHINE_HASH = "a".repeat(64);
export const UHURA_TEST_PRESENTATION_HASH = "b".repeat(64);
export const UHURA_TEST_CONFIGURATION_HASH = "c".repeat(64);
const HASH = "d".repeat(64);

const node = (
  id: string,
  kind: string,
  label: string,
) => ({
  id,
  kind,
  machine: UHURA_TEST_MACHINE,
  label,
});

const nodes = [
  node(`machine:${UHURA_TEST_MACHINE}`, "machine", UHURA_TEST_MACHINE),
  node(`port:${UHURA_TEST_MACHINE}:analytics`, "port", "analytics"),
  node(`input:${UHURA_TEST_MACHINE}:increment`, "input", "increment"),
  node(`state:${UHURA_TEST_MACHINE}:count`, "state", "count"),
  node(
    `command:${UHURA_TEST_MACHINE}:analytics`,
    "command",
    "analytics.tracked",
  ),
  node(`outcome:${UHURA_TEST_MACHINE}:accepted`, "outcome", "accepted"),
  node(
    `commit-hook:${UHURA_TEST_MACHINE}:before-commit`,
    "commit-hook",
    "before commit",
  ),
  node(
    `presentation:${UHURA_TEST_MACHINE}:${UHURA_TEST_PRESENTATION}`,
    "presentation",
    UHURA_TEST_PRESENTATION,
  ),
  node(`ui-event:${UHURA_TEST_MACHINE}:0000`, "ui-event", "button.click"),
] as const;

const machineId = nodes[0].id;
const portId = nodes[1].id;
const inputId = nodes[2].id;
const stateId = nodes[3].id;
const commandId = nodes[4].id;
const outcomeId = nodes[5].id;
const hookId = nodes[6].id;
const presentationId = nodes[7].id;
const uiEventId = nodes[8].id;

const edges = [
  { kind: "owns", from: machineId, to: portId },
  { kind: "owns", from: machineId, to: inputId },
  { kind: "owns", from: machineId, to: stateId },
  { kind: "owns", from: machineId, to: commandId },
  { kind: "owns", from: machineId, to: outcomeId },
  { kind: "owns", from: machineId, to: hookId },
  { kind: "writes", from: inputId, to: stateId },
  { kind: "emits", from: inputId, to: commandId },
  { kind: "sends-via", from: commandId, to: portId },
  { kind: "finishes", from: inputId, to: outcomeId },
  { kind: "triggers", from: outcomeId, to: hookId },
  { kind: "projects", from: presentationId, to: machineId },
  { kind: "exposes", from: presentationId, to: uiEventId },
  { kind: "dispatches", from: uiEventId, to: inputId },
] as const;

const source = (id: string, index: number) => ({
  id,
  path: "counter.uhura",
  start: index * 5,
  end: index * 5 + 4,
});

export function machineTestDeployment(): HostInspection {
  return decodeHostInspection({
    protocol: "uhura-inspection/1",
    identityProtocol: UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
    entry: "counter",
    machine: UHURA_TEST_MACHINE,
    presentation: UHURA_TEST_PRESENTATION,
    machineProgramHash: UHURA_TEST_MACHINE_HASH,
    presentationHash: UHURA_TEST_PRESENTATION_HASH,
    evidenceHash: null,
    deploymentHash: HASH,
    sources: [{
      file: 0,
      path: "counter.uhura",
      sha256: HASH,
      bytes: 1000,
    }],
    provenance: {
      protocol: "uhura-provenance/0",
      sources: [{
        source: 0,
        package: "example@1",
        module: "counter",
        path: "counter.uhura",
        sha256: HASH,
        bytes: 1000,
      }],
      occurrences: [],
      topology: {
        protocol: "uhura-authored-interaction-topology/0",
        nodes: [],
        edges: [],
      },
    },
    interactionGraph: {
      protocol: UHURA_INTERACTION_GRAPH_PROTOCOL,
      identity_protocol: UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
      machine_program_hashes: {
        [UHURA_TEST_MACHINE]: UHURA_TEST_MACHINE_HASH,
      },
      presentation_hashes: {
        [UHURA_TEST_PRESENTATION]: UHURA_TEST_PRESENTATION_HASH,
      },
      outcome_policies: {
        [outcomeId]: "commit",
      },
      nodes,
      edges,
    },
    graphSources: {
      protocol: UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL,
      nodes: nodes.map((entry, index) => ({
        node: entry.id,
        sources: [source(`node-${String(index)}`, index)],
      })),
      edges: edges.map((edge, index) => ({
        edge,
        sources: [source(`edge-${String(index)}`, nodes.length + index)],
      })),
    },
    evidence: {
      protocol: UHURA_EVIDENCE_SUMMARY_PROTOCOL,
      passed: true,
      scenarios: { total: 0, passed: 0, failed: 0 },
      artifacts: { pins: 0, examples: 0, checkpoints: 0 },
      failureCount: 0,
    },
  });
}

const count = (value: number) => ({
  $: "record",
  fields: [{
    name: "count",
    value: { $: "Nat", value: String(value) },
  }],
});

const outcome = {
  $: "variant",
  type: `${UHURA_TEST_MACHINE}::Outcome`,
  case: "accepted",
  fields: [],
};

const genesis = {
  protocol: UHURA_GENESIS_RECEIPT_PROTOCOL,
  kind: "genesis",
  instance: "counter-1",
  machineProgramHash: UHURA_TEST_MACHINE_HASH,
  configurationHash: UHURA_TEST_CONFIGURATION_HASH,
  sequence: "0",
  initialObservation: count(0),
  initialStateHash: HASH,
};

const reaction = (sequence: number) => ({
  protocol: UHURA_REACTION_RECEIPT_PROTOCOL,
  kind: "reaction",
  instance: "counter-1",
  machineProgramHash: UHURA_TEST_MACHINE_HASH,
  configurationHash: UHURA_TEST_CONFIGURATION_HASH,
  sequence: String(sequence),
  input: {
    source: "local",
    value: {
      $: "variant",
      type: `${UHURA_TEST_MACHINE}::Input`,
      case: "increment",
      fields: [],
    },
  },
  resolution: {
    kind: "completed",
    outcome,
    disposition: "commit",
  },
  orderedCommands: [{
    target: "port",
    port: "analytics",
    value: {
      $: "variant",
      type: `${UHURA_TEST_MACHINE}::port.analytics.Send`,
      case: "tracked",
      fields: [],
    },
  }],
  postObservation: count(sequence),
  preStateHash: HASH,
  postStateHash: HASH,
});

export function machineTestRuntimeStep(sequence: number): {
  readonly snapshot: RuntimeSnapshot;
  readonly receipt: Receipt;
} {
  if (!Number.isSafeInteger(sequence) || sequence < 0) {
    throw new RangeError("test sequence must be a non-negative safe integer");
  }
  const snapshot = decodeRuntimeSnapshot({
    protocol: UHURA_RUNTIME_SNAPSHOT_PROTOCOL,
    instance: "counter-1",
    machineProgramHash: UHURA_TEST_MACHINE_HASH,
    presentation: UHURA_TEST_PRESENTATION,
    presentationHash: UHURA_TEST_PRESENTATION_HASH,
    configurationHash: UHURA_TEST_CONFIGURATION_HASH,
    state: count(sequence),
    stateHash: HASH,
    lifecycle: "running",
    nextSequence: String(sequence + 1),
    tracePrefixHash: HASH,
    ingressPrefixHash: HASH,
    nextIngressOrdinal: "1",
  });
  const receipt = decodeReceipt(sequence === 0 ? genesis : reaction(sequence));
  return { snapshot, receipt };
}
