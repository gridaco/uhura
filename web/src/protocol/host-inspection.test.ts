import { describe, expect, it } from "vitest";

import {
  decodeHostInspection,
  UHURA_HOST_INSPECTION_PROTOCOL,
} from "./host-inspection.js";
import {
  UHURA_INTERACTION_GRAPH_PROTOCOL,
  UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL,
} from "./interaction-graph.js";
import {
  UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
} from "./machine.js";

const machine = "example@1::Counter";
const hash = "a".repeat(64);
const node = {
  id: `machine:${machine}`,
  kind: "machine",
  machine,
  label: machine,
} as const;
const provenance = {
  protocol: "uhura-provenance/0",
  sources: [{
    source: 0,
    package: "example@1",
    module: "app",
    path: "app.uhura",
    sha256: "c".repeat(64),
    bytes: 100,
  }],
  occurrences: [{
    node: "d".repeat(64),
    source: 0,
    start: 0,
    end: 10,
    role: "definition",
    owner: "root",
  }],
  topology: {
    protocol: "uhura-authored-interaction-topology/0",
    nodes: [],
    edges: [],
  },
} as const;
const artifact = {
  protocol: UHURA_HOST_INSPECTION_PROTOCOL,
  identityProtocol: UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
  entry: "app",
  machine,
  presentation: null,
  machineProgramHash: hash,
  presentationHash: null,
  evidenceHash: null,
  deploymentHash: "b".repeat(64),
  sources: [{
    file: 0,
    path: "app.uhura",
    sha256: "c".repeat(64),
    bytes: 100,
  }],
  provenance,
  interactionGraph: {
    protocol: UHURA_INTERACTION_GRAPH_PROTOCOL,
    identity_protocol: UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
    machine_program_hashes: { [machine]: hash },
    presentation_hashes: {},
    outcome_policies: {},
    nodes: [node],
    edges: [],
  },
  graphSources: {
    protocol: UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL,
    nodes: [{
      node: node.id,
      sources: [{
        id: "machine",
        path: "app.uhura",
        start: 0,
        end: 10,
      }],
    }],
    edges: [],
  },
  evidence: { passed: true },
} as const;

describe("Uhura host inspection", () => {
  it("strictly decodes identity, topology, and resolvable physical spans", () => {
    const decoded = decodeHostInspection(artifact);
    expect(decoded.machineProgramHash).toBe(hash);
    expect(decoded.interactionGraph.nodes[0]?.kind).toBe("machine");
    expect(decoded.graphSources.nodes[0]?.sources[0]?.path).toBe("app.uhura");

    expect(decoded.identityProtocol).toBe(UHURA_MACHINE_PROGRAM_ID_PROTOCOL);
    expect(() =>
      decodeHostInspection({
        ...artifact,
        identityProtocol: "uhura-semantic-ir-hash/0",
      })
    ).toThrow(/identityProtocol must be/u);
  });

  it("joins semantic provenance to the accepted physical source inventory", () => {
    expect(decodeHostInspection(artifact).provenance).toEqual(provenance);
    expect(() =>
      decodeHostInspection({
        ...artifact,
        provenance: {
          ...provenance,
          sources: [{
            ...provenance.sources[0],
            sha256: "e".repeat(64),
          }],
        },
      })
    ).toThrow(/accepted source inventory/u);
  });

  it("rejects identity and source-inventory drift", () => {
    expect(() =>
      decodeHostInspection({
        ...artifact,
        machineProgramHash: "d".repeat(64),
      })
    ).toThrow(/machine identity/u);
    expect(() =>
      decodeHostInspection({
        ...artifact,
        graphSources: {
          ...artifact.graphSources,
          nodes: [{
            ...artifact.graphSources.nodes[0],
            sources: [{
              ...artifact.graphSources.nodes[0].sources[0],
              end: 101,
            }],
          }],
        },
      })
    ).toThrow(/source inventory/u);
  });
});
