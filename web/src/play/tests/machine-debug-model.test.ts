import assert from "node:assert/strict";
import { test } from "vitest";

import { decodeValue } from "../../protocol/machine.js";
import {
  deriveDebugGraph,
  formatDebugValue,
} from "../debug-model.js";
import { createInspectionStore } from "../inspection-store.js";
import {
  UHURA_TEST_MACHINE,
  UHURA_TEST_MACHINE_HASH,
  machineTestDeployment,
  machineTestRuntimeStep,
} from "./inspection-fixture.js";

function byId<T extends { id: string }>(
  values: readonly T[],
  fragment: string,
): T {
  const value = values.find((candidate) => candidate.id.includes(fragment));
  assert.ok(value, `missing ${fragment}`);
  return value;
}

test("projects admitted machine topology with conservative receipt activity", () => {
  const store = createInspectionStore();
  store.installArtifacts({
    generation: 11,
    deployment: machineTestDeployment(),
  });
  const genesis = machineTestRuntimeStep(0);
  store.record(genesis.snapshot, genesis.receipt);
  const before = deriveDebugGraph(store.handle.state);
  const reaction = machineTestRuntimeStep(1);
  store.record(reaction.snapshot, reaction.receipt);
  const model = deriveDebugGraph(store.handle.state);

  assert.equal(model.generation, 11);
  assert.equal(model.programHash, UHURA_TEST_MACHINE_HASH);
  assert.equal(model.exactSequence, "2");
  assert.equal(model.focusDefinitionId, `machine:${UHURA_TEST_MACHINE}`);
  assert.equal(model.runtimeDefinitionId, `machine:${UHURA_TEST_MACHINE}`);
  assert.deepEqual(model.definitions.map((definition) => ({
    kind: definition.kind,
    label: definition.label,
    entry: definition.entry,
    active: definition.active,
    runtime: definition.runtime,
  })), [{
    kind: "machine",
    label: UHURA_TEST_MACHINE,
    entry: true,
    active: true,
    runtime: true,
  }]);
  assert.deepEqual(
    model.nodes.map((node) => node.id).sort(),
    before.nodes.map((node) => node.id).sort(),
    "runtime steps decorate but never reshape admitted topology",
  );
  assert.deepEqual(
    model.edges.map((edge) => edge.id).sort(),
    before.edges.map((edge) => edge.id).sort(),
  );

  const input = byId(model.nodes, "input:");
  assert.equal(input.lane, "handler");
  assert.equal(input.runtime.selected, true);
  assert.equal(input.runtime.current, true);
  assert.equal(input.detail, "Input · increment");
  const state = byId(model.nodes, "state:");
  assert.equal(state.runtime.written, true);
  assert.equal(state.detail, "1");
  const command = byId(model.nodes, "command:");
  assert.equal(command.runtime.sent, true);
  assert.equal(command.detail, "tracked");
  assert.equal(byId(model.nodes, "outcome:").runtime.current, true);
  assert.equal(byId(model.nodes, "commit-hook:").runtime.current, true);
  assert.equal(byId(model.nodes, "presentation:").runtime.current, true);
  assert.equal(input.sourceSpans?.length, 1);
  assert.match(input.sourceSpans?.[0]?.id ?? "", /^node-/u);
  assert.deepEqual(input.span, {
    file: "counter.uhura",
    start: 10,
    end: 14,
  });

  const activity = (kind: string) =>
    model.edges.find((edge) => edge.kind === kind)?.activity;
  assert.equal(activity("writes"), "taken");
  assert.equal(activity("emits"), "taken");
  assert.equal(activity("finishes"), "taken");
  assert.equal(activity("triggers"), "taken");
  assert.equal(activity("sends-via"), "taken");
  assert.equal(activity("dispatches"), "context");
});

test("keeps exact machine numeric text and reports disposed state explicitly", () => {
  const exact = "900719925474099312345678901234567890";
  assert.equal(
    formatDebugValue(decodeValue({ $: "Nat", value: exact })),
    exact,
  );

  const store = createInspectionStore();
  store.installArtifacts({
    generation: 1,
    deployment: machineTestDeployment(),
  });
  store.dispose();
  const model = deriveDebugGraph(store.handle.state);
  assert.equal(model.disposed, true);
  assert.equal(model.emptyReason, "disposed");
  assert.deepEqual(model.nodes, []);
});
