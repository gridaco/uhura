import { describe, expect, it } from "vitest";

import {
  decodeInteractionGraphArtifacts,
  UHURA_INTERACTION_GRAPH_PROTOCOL,
  UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL,
} from "./interaction-graph.js";
import {
  UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
} from "./machine.js";

const graph = {
  protocol: UHURA_INTERACTION_GRAPH_PROTOCOL,
  identity_protocol: UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
  machine_program_hashes: { "example@1::Counter": "a".repeat(64) },
  presentation_hashes: { "example@1::Web": "b".repeat(64) },
  outcome_policies: {
    "outcome:example@1::Counter:accepted": "commit",
  },
  nodes: [
    {
      id: "machine:example@1::Counter",
      kind: "machine",
      machine: "example@1::Counter",
      label: "example@1::Counter",
    },
    {
      id: "input:example@1::Counter:increment",
      kind: "input",
      machine: "example@1::Counter",
      label: "increment",
    },
    {
      id: "invariant:example@1::Counter:1",
      kind: "invariant",
      machine: "example@1::Counter",
      label: "invariant 1",
    },
    {
      id: "outcome:example@1::Counter:accepted",
      kind: "outcome",
      machine: "example@1::Counter",
      label: "accepted",
    },
    {
      id: "presentation:example@1::Counter:example@1::Web",
      kind: "presentation",
      machine: "example@1::Counter",
      label: "example@1::Web",
    },
  ],
  edges: [
    {
      from: "machine:example@1::Counter",
      to: "input:example@1::Counter:increment",
      kind: "owns",
    },
    {
      from: "machine:example@1::Counter",
      to: "invariant:example@1::Counter:1",
      kind: "owns",
    },
    {
      from: "machine:example@1::Counter",
      to: "outcome:example@1::Counter:accepted",
      kind: "owns",
    },
    {
      from: "presentation:example@1::Counter:example@1::Web",
      to: "machine:example@1::Counter",
      kind: "projects",
    },
  ],
} as const;

const source = (id: string, start: number) => ({
  id,
  path: "counter.uhura",
  start,
  end: start + 5,
});

const graphSources = {
  protocol: UHURA_INTERACTION_GRAPH_PROVENANCE_PROTOCOL,
  nodes: [
    {
      node: "machine:example@1::Counter",
      sources: [source("machine", 0)],
    },
    {
      node: "input:example@1::Counter:increment",
      sources: [source("handler", 20)],
    },
    {
      node: "invariant:example@1::Counter:1",
      sources: [source("invariant", 30)],
    },
    {
      node: "outcome:example@1::Counter:accepted",
      sources: [source("outcome", 35)],
    },
    {
      node: "presentation:example@1::Counter:example@1::Web",
      sources: [source("presentation", 40)],
    },
  ],
  edges: [
    {
      edge: graph.edges[0],
      sources: [source("handler", 20)],
    },
    {
      edge: graph.edges[1],
      sources: [source("invariant", 30)],
    },
    {
      edge: graph.edges[2],
      sources: [source("outcome", 35)],
    },
    {
      edge: graph.edges[3],
      sources: [source("presentation", 40)],
    },
  ],
} as const;

describe("Uhura interaction graph transport", () => {
  it("decodes a closed semantic graph and complete physical source sidecar", () => {
    const decoded = decodeInteractionGraphArtifacts(graph, graphSources);

    expect(decoded.graph.nodes).toHaveLength(5);
    expect(decoded.graph.edges).toHaveLength(4);
    expect(decoded.graph.outcomePolicies).toEqual({
      "outcome:example@1::Counter:accepted": "commit",
    });
    expect(decoded.sources.nodes[1]?.sources[0]).toEqual(
      source("handler", 20),
    );
    expect(decoded.graph.identityProtocol).toBe(
      UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
    );
    expect(() =>
      decodeInteractionGraphArtifacts({
        ...graph,
        identity_protocol: "uhura-semantic-ir-hash/0",
      }, graphSources)
    ).toThrow(/identity_protocol must be/u);
  });

  it("requires a closed commit-or-abort policy for every outcome node", () => {
    expect(() =>
      decodeInteractionGraphArtifacts({
        ...graph,
        outcome_policies: {},
      }, graphSources)
    ).toThrow(/cover every outcome node/u);
    expect(() =>
      decodeInteractionGraphArtifacts({
        ...graph,
        outcome_policies: {
          ...graph.outcome_policies,
          "input:example@1::Counter:increment": "abort",
        },
      }, graphSources)
    ).toThrow(/declared outcome node/u);
    expect(() =>
      decodeInteractionGraphArtifacts({
        ...graph,
        outcome_policies: {
          "outcome:example@1::Counter:accepted": "maybe",
        },
      }, graphSources)
    ).toThrow(/commit.*abort/u);
  });

  it("decodes authored composition vocabulary merged into the runtime graph", () => {
    const part = {
      id: "part:example@1::Counter:controls",
      kind: "part",
      machine: "example@1::Counter",
      label: "controls",
    } as const;
    const composes = {
      from: "machine:example@1::Counter",
      to: part.id,
      kind: "composes",
    } as const;
    const decoded = decodeInteractionGraphArtifacts({
      ...graph,
      nodes: [...graph.nodes, part],
      edges: [...graph.edges, composes],
    }, {
      ...graphSources,
      nodes: [
        ...graphSources.nodes,
        { node: part.id, sources: [source("part", 60)] },
      ],
      edges: [
        ...graphSources.edges,
        { edge: composes, sources: [source("part", 60)] },
      ],
    });

    expect(decoded.graph.nodes.at(-1)?.kind).toBe("part");
    expect(decoded.graph.edges.at(-1)?.kind).toBe("composes");
  });

  it("rejects semantic edges that reference absent nodes", () => {
    expect(() =>
      decodeInteractionGraphArtifacts({
        ...graph,
        edges: [{ ...graph.edges[0], to: "input:missing" }],
      }, graphSources)
    ).toThrow(/declared nodes/u);
  });

  it("rejects incomplete source coverage", () => {
    expect(() =>
      decodeInteractionGraphArtifacts(graph, {
        ...graphSources,
        nodes: graphSources.nodes.slice(0, 1),
      })
    ).toThrow(/cover every semantic node/u);
  });

  it("rejects reversed physical spans without affecting semantic decoding", () => {
    expect(() =>
      decodeInteractionGraphArtifacts(graph, {
        ...graphSources,
        nodes: [
          {
            ...graphSources.nodes[0],
            sources: [{ ...source("machine", 9), end: 3 }],
          },
          graphSources.nodes[1],
        ],
      })
    ).toThrow(/must not precede/u);
  });
});
