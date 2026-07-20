import { describe, expect, it } from "vitest";

import {
  decodeSemanticProvenance,
  UHURA_AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL,
  UHURA_PROVENANCE_PROTOCOL,
} from "./provenance.js";

const hash = "a".repeat(64);
const artifact = {
  protocol: UHURA_PROVENANCE_PROTOCOL,
  sources: [{
    source: 0,
    package: "example@1",
    module: "machine",
    path: "machine.uhura",
    sha256: hash,
    bytes: 20,
  }],
  occurrences: [{
    node: hash,
    source: 0,
    start: 4,
    end: 11,
    role: "definition",
    owner: "root",
  }],
  topology: {
    protocol: UHURA_AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL,
    nodes: [{
      id: "module:example@1::Machine:machine",
      kind: "module",
      machine: "example@1::Machine",
      label: "machine",
      sources: [{
        node: hash,
        role: "definition",
        owner: "root",
      }],
    }, {
      id: "invariant:example@1::Machine:1",
      kind: "invariant",
      machine: "example@1::Machine",
      label: "invariant 1",
      sources: [{
        node: hash,
        role: "definition",
        owner: "root",
      }],
    }],
    edges: [],
  },
} as const;

describe("Uhura semantic provenance", () => {
  it("admits the exact source and occurrence contract", () => {
    const decoded = decodeSemanticProvenance(artifact);
    expect(decoded.sources[0]?.path).toBe("machine.uhura");
    expect(decoded.occurrences[0]?.node).toBe(hash);
    expect(decoded.topology.nodes[0]?.kind).toBe("module");
    expect(decoded.topology.nodes[1]?.kind).toBe("invariant");
    expect(() => decodeSemanticProvenance(null)).toThrow(/must be an object/u);
  });

  it("rejects non-contiguous sources and out-of-bounds occurrences", () => {
    expect(() =>
      decodeSemanticProvenance({
        ...artifact,
        sources: [{ ...artifact.sources[0], source: 1 }],
      })
    ).toThrow(/contiguous/u);
    expect(() =>
      decodeSemanticProvenance({
        ...artifact,
        occurrences: [{ ...artifact.occurrences[0], end: 21 }],
      })
    ).toThrow(/within its source/u);
    expect(() =>
      decodeSemanticProvenance({
        ...artifact,
        occurrences: [{ ...artifact.occurrences[0], role: "Definition" }],
      })
    ).toThrow(/role is not admitted/u);
  });

  it("qualifies physical path uniqueness and canonical ordering by package", () => {
    const secondHash = "b".repeat(64);
    const sharedPath = {
      ...artifact,
      sources: [
        artifact.sources[0],
        {
          ...artifact.sources[0],
          source: 1,
          package: "vendor@1",
          module: "machine",
          sha256: secondHash,
        },
      ],
      occurrences: [
        artifact.occurrences[0],
        {
          ...artifact.occurrences[0],
          node: secondHash,
          source: 1,
        },
      ],
      topology: {
        protocol: UHURA_AUTHORED_INTERACTION_TOPOLOGY_PROTOCOL,
        nodes: [],
        edges: [],
      },
    };
    expect(decodeSemanticProvenance(sharedPath).sources).toHaveLength(2);
    expect(() =>
      decodeSemanticProvenance({
        ...sharedPath,
        sources: [
          artifact.sources[0],
          {
            ...artifact.sources[0],
            source: 1,
            module: "other",
          },
        ],
      })
    ).toThrow(/unique paths within each package/u);
  });
});
