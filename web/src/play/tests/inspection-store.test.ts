import assert from "node:assert/strict";
import { test } from "vitest";

import { hash } from "../../protocol/machine.js";
import { createInspectionStore } from "../inspection-store.js";
import {
  machineTestDeployment,
  machineTestInspection,
} from "./inspection-fixture.js";

test("retains a frozen bounded  history with exact receipt identity", () => {
  const store = createInspectionStore({ historyLimit: 2 });
  const publications: string[] = [];
  const stop = store.handle.subscribe((state) => {
    publications.push(
      state.latest?.receipt.sequence ?? (state.artifacts ? "artifacts" : "loading"),
    );
  });

  assert.equal(store.installArtifacts({
    generation: 7,
    deployment: machineTestDeployment(),
  }), true);
  for (let sequence = 0; sequence <= 2; sequence += 1) {
    const step = machineTestInspection(sequence);
    assert.equal(store.record(step.inspection, step.receipt), true);
  }

  assert.deepEqual(publications, ["loading", "artifacts", "0", "1", "2"]);
  assert.deepEqual(
    store.handle.state.history.map((step) => step.receipt.sequence),
    ["1", "2"],
  );
  assert.equal(store.handle.state.evictedSteps, 1);
  assert.equal(store.handle.state.latest?.inspection.nextSequence, "3");
  assert.equal(Object.isFrozen(store.handle.state), true);
  assert.equal(Object.isFrozen(store.handle.state.history), true);
  assert.equal(Object.isFrozen(store.handle.state.latest?.inspection), true);
  stop();
});

test("rejects deployment drift and noncontiguous runtime publications", () => {
  const store = createInspectionStore();
  store.installArtifacts({
    generation: 1,
    deployment: machineTestDeployment(),
  });
  const genesis = machineTestInspection(0);
  store.record(genesis.inspection, genesis.receipt);

  const next = machineTestInspection(1);
  assert.throws(
    () => store.record(
      {
        ...next.inspection,
        machineProgramHash: hash("e".repeat(64)),
      },
      next.receipt,
    ),
    /admitted deployment identity/u,
  );
  const skipped = machineTestInspection(2);
  assert.throws(
    () => store.record(skipped.inspection, skipped.receipt),
    /increase contiguously/u,
  );
  assert.throws(
    () => store.installArtifacts({
      generation: 2,
      deployment: machineTestDeployment(),
    }),
    /already installed/u,
  );
});

test("isolates listener failures and publishes one terminal disposed state", () => {
  const reported: unknown[] = [];
  const store = createInspectionStore({
    onListenerError: (error) => reported.push(error),
  });
  let healthyCalls = 0;
  store.handle.subscribe(() => {
    throw new Error("observer failed");
  });
  store.handle.subscribe(() => {
    healthyCalls += 1;
  });
  store.installArtifacts({
    generation: 1,
    deployment: machineTestDeployment(),
  });

  store.dispose();
  store.dispose();
  assert.equal(reported.length, 3);
  assert.equal(healthyCalls, 3);
  assert.equal(store.handle.state.disposed, true);
  assert.equal(store.handle.state.artifacts, null);
  assert.deepEqual(store.handle.state.history, []);
  let terminalCalls = 0;
  store.handle.subscribe((state) => {
    terminalCalls += 1;
    assert.equal(state.disposed, true);
  });
  assert.equal(terminalCalls, 1);
  const step = machineTestInspection(0);
  assert.equal(store.record(step.inspection, step.receipt), false);
});
