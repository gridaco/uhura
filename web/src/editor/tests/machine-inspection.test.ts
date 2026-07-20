import assert from "node:assert/strict";

import { test } from "vitest";

import type {
  EditorMachine,
  PreviewEvidence,
} from "../editor-state.js";
import {
  inspectMachine,
  machineMetricRows,
  previewEvidenceRows,
  renderInspectionRows,
} from "../machine-inspection.js";

const machine = (overrides: Partial<EditorMachine> = {}): EditorMachine => ({
  protocol: "uhura-machine-inspection/0",
  identityProtocol: "uhura-machine-identity/0",
  deployment: {
    entry: "return-desk",
    machine: "app.return_desk.machine@1::ReturnDesk",
    presentation: "app.return_desk.web@1::ReturnDeskWeb",
    deploymentHash: "sha256:deployment",
  },
  sources: [{ path: "machine.uhura" }, { path: "web.uhura" }],
  provenance: null,
  interactionGraph: {},
  graphSources: {},
  checkpoints: {
    empty: { protocol: "uhura-checkpoint/0" },
    reviewed: { protocol: "uhura-checkpoint/0" },
  },
  evidence: {
    passed: false,
    scenarios: [
      { scenario: "ready", status: "passed" },
      { scenario: "rejected", status: "failed" },
    ],
    failures: [{ code: "expectation-failed" }],
  },
  ...overrides,
});

test("summarizes deployment identity and bounded machine evidence counts", () => {
  const summary = inspectMachine(machine());

  assert.deepEqual(summary.identity, [
    { label: "Deployment", value: "return-desk" },
    { label: "Machine", value: "app.return_desk.machine@1::ReturnDesk" },
    { label: "Presentation", value: "app.return_desk.web@1::ReturnDeskWeb" },
  ]);
  assert.equal(summary.status, "failed");
  assert.deepEqual(machineMetricRows(summary), [
    { label: "Passes", value: "1" },
    { label: "Failures", value: "1" },
    { label: "Checkpoints", value: "2" },
    { label: "Sources", value: "2" },
  ]);
});

test("keeps absent deployment and unstable evidence payloads honest", () => {
  const summary = inspectMachine(machine({
    deployment: null,
    sources: [],
    checkpoints: {},
    evidence: {},
  }));

  assert.deepEqual(summary.identity, []);
  assert.equal(summary.status, "unknown");
  assert.equal(summary.passes, 0);
  assert.equal(summary.failures, 0);
  assert.equal(summary.checkpoints, 0);
  assert.equal(summary.sources, 0);
  assert.deepEqual(summary.ownership, []);
  assert.deepEqual(summary.outcomes, []);
  assert.deepEqual(summary.dependencies, []);
});

test("projects authored module, part ownership, and dependencies without replacing evidence UX", () => {
  const machineId = "app.return_desk.machine@1::ReturnDesk";
  const summary = inspectMachine(machine({
    interactionGraph: {
      protocol: "uhura-interaction-graph/0",
      outcome_policies: {
        accepted: "commit",
        refused: "abort",
      },
      nodes: [
        { id: "module:app", kind: "module", machine: machineId, label: "app" },
        { id: "module:parts", kind: "module", machine: machineId, label: "parts" },
        { id: "machine", kind: "machine", machine: machineId, label: machineId },
        { id: "producer", kind: "part", machine: machineId, label: "producer" },
        { id: "consumer", kind: "part", machine: machineId, label: "consumer" },
        { id: "value", kind: "state", machine: machineId, label: "producer.value" },
        { id: "current", kind: "computed", machine: machineId, label: "producer.current" },
        { id: "set", kind: "update", machine: machineId, label: "producer.set" },
        { id: "producer-invariant", kind: "invariant", machine: machineId, label: "producer.invariant 1" },
        { id: "input", kind: "input", machine: machineId, label: "consumer.Apply" },
        { id: "observed", kind: "observation", machine: machineId, label: "consumer.current" },
        { id: "root-invariant", kind: "invariant", machine: machineId, label: "invariant 1" },
        { id: "accepted", kind: "outcome", machine: machineId, label: "Accepted" },
        { id: "refused", kind: "outcome", machine: machineId, label: "Refused" },
      ],
      edges: [
        { from: "module:app", to: "machine", kind: "owns" },
        { from: "module:parts", to: "producer", kind: "owns" },
        { from: "module:parts", to: "consumer", kind: "owns" },
        { from: "machine", to: "producer", kind: "composes" },
        { from: "machine", to: "consumer", kind: "composes" },
        { from: "producer", to: "value", kind: "owns" },
        { from: "producer", to: "current", kind: "owns" },
        { from: "producer", to: "set", kind: "owns" },
        { from: "producer", to: "producer-invariant", kind: "owns" },
        { from: "consumer", to: "input", kind: "owns" },
        { from: "consumer", to: "observed", kind: "owns" },
        { from: "machine", to: "root-invariant", kind: "owns" },
        { from: "machine", to: "accepted", kind: "owns" },
        { from: "machine", to: "refused", kind: "owns" },
        { from: "current", to: "value", kind: "reads" },
        { from: "input", to: "set", kind: "calls" },
        { from: "observed", to: "current", kind: "observes" },
      ],
    },
  }));

  assert.deepEqual(summary.ownership, [
    { label: "Module", value: "app" },
    { label: "Module", value: "parts" },
    {
      label: "Machine-owned",
      value: "1 invariant",
    },
    {
      label: "Part consumer",
      value: "1 observation",
    },
    {
      label: "Part producer",
      value: "1 state · 1 computed · 1 invariant · 1 update",
    },
  ]);
  assert.deepEqual(summary.outcomes, [
    { label: "Outcome Accepted", value: "commit" },
    { label: "Outcome Refused", value: "abort" },
  ]);
  assert.deepEqual(summary.dependencies, [
    { label: "Reads", value: "1 · producer.current → producer.value" },
    { label: "Calls", value: "1 · consumer.Apply → producer.set" },
    { label: "Observes", value: "1 · consumer.current → producer.current" },
  ]);
});

test("exposes only the selected preview evidence identity", () => {
  const evidence: PreviewEvidence = {
    scenario: "return-approved",
    pin: "completed",
    sourceId: "conformance.uhura:44:3",
    sources: {
      registration: { path: "conformance.uhura" },
      pin: { path: "conformance.uhura" },
    },
    observation: { result: "accepted" },
    snapshot: { state: "complete" },
    scenarioReceiptLog: { receipts: [{ sequence: "1" }] },
  };

  assert.deepEqual(previewEvidenceRows(evidence), [
    { label: "Scenario", value: "return-approved" },
    { label: "Pin", value: "completed" },
    { label: "Source", value: "conformance.uhura:44:3" },
  ]);
});

class TestElement {
  readonly children: TestElement[] = [];
  readonly tagName: string;
  textContent = "";

  constructor(tagName: string) {
    this.tagName = tagName;
  }

  append(...children: TestElement[]): void {
    this.children.push(...children);
  }

  replaceChildren(...children: TestElement[]): void {
    this.children.splice(0, this.children.length, ...children);
  }
}

test("renders semantic definition-list rows without serializing raw evidence", () => {
  const document = {
    createElement: (tagName: string) => new TestElement(tagName),
  } as unknown as Document;
  const root = new TestElement("dl");

  renderInspectionRows(
    document,
    root as unknown as HTMLElement,
    previewEvidenceRows({
      scenario: "ready",
      pin: "loaded",
      sourceId: "evidence/ready/loaded",
      sources: { registration: {}, pin: {} },
      observation: { private: "large observation" },
      snapshot: { private: "large snapshot" },
      scenarioReceiptLog: { private: "large receipt log" },
    }),
  );

  assert.deepEqual(
    root.children.map((group) => ({
      tags: group.children.map((child) => child.tagName),
      text: group.children.map((child) => child.textContent),
    })),
    [
      { tags: ["dt", "dd"], text: ["Scenario", "ready"] },
      { tags: ["dt", "dd"], text: ["Pin", "loaded"] },
      { tags: ["dt", "dd"], text: ["Source", "evidence/ready/loaded"] },
    ],
  );
});
