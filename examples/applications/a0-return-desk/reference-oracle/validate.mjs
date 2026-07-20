import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import {
  ReturnDeskOracle,
  acceptance,
  assertSame,
  expectStep,
  nav,
  order7,
  order8,
  order9WithoutLamp,
  other,
  refusal,
  submitCommand,
  unavailable,
  urls,
} from "./model.mjs";

const checks = [];

function compareText(left, right) {
  return Buffer.compare(
    Buffer.from(left, "utf8"),
    Buffer.from(right, "utf8"),
  );
}

function canonical(value) {
  if (value === null) return ["null"];
  if (typeof value === "boolean") return ["boolean", value];
  if (typeof value === "string") return ["string", value];
  if (typeof value === "number") return ["number", value];
  if (typeof value === "bigint") return ["bigint", value.toString()];
  if (Array.isArray(value)) {
    return ["array", value.map(canonical)];
  }
  if (value && typeof value === "object") {
    return [
      "record",
      Object.keys(value)
        .sort(compareText)
        .map((key) => [key, canonical(value[key])]),
    ];
  }
  throw new TypeError("unsupported canonical value");
}

function digest(value) {
  return createHash("sha256")
    .update(JSON.stringify(canonical(value)))
    .digest("hex");
}

function check(name, body) {
  body();
  checks.push(name);
}

function pendingSnapshot(
  requestId,
  revision,
  lineId,
  quantity,
  reason,
  method,
) {
  return {
    requestId,
    orderId: "order-100",
    expectedRevision: revision,
    selections: [{ lineId, quantity, reason }],
    method,
  };
}

function assertNoMutation(before, step, message) {
  assert.deepEqual(step.state, before, message);
  assert.deepEqual(step.consequences, [], message);
}

function readyReview({
  order = order7,
  lineId = "lamp",
  reason = "damaged",
  method = "drop-off",
} = {}) {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order), "accepted");
  expectStep(machine.chooseQuantity(lineId, 1), "accepted");
  expectStep(machine.chooseReason(lineId, reason), "accepted");
  expectStep(
    machine.goToStep("method"),
    "accepted",
    [nav("push", urls.method)],
  );
  expectStep(machine.deliverLocation(urls.method), "accepted");
  expectStep(machine.chooseMethod(method), "accepted");
  expectStep(
    machine.goToStep("review"),
    "accepted",
    [nav("push", urls.review)],
  );
  expectStep(machine.deliverLocation(urls.review), "accepted");
  return machine;
}

const canonicalPins = {};
let canonicalCheckpoint;
let canonicalPrefixDigest;

check("canonical trace 0..31", () => {
  const machine = new ReturnDeskOracle();
  const trace = [];

  trace.push({
    step: 0,
    result: "initialized",
    consequences: [],
    state: machine.snapshot(),
    presentation: machine.presentation(),
  });
  assert.equal(machine.state.location, null);
  assert.equal(machine.presentation().orderStatus.kind, "waiting");
  assert.equal(machine.state.draft, null);

  let step = expectStep(
    machine.deliverLocation(urls.review),
    "accepted",
    [nav("replace", urls.items)],
  );
  trace.push({ step: 1, ...step });
  assert.equal(step.presentation.deliveredLocation, urls.review);
  assert.equal(step.presentation.normalizing, true);
  assert.equal(machine.state.routeScope, null);

  step = expectStep(machine.deliverLocation(urls.items), "accepted");
  trace.push({ step: 2, ...step });
  assert.equal(step.presentation.activeStep, "items");
  assert.equal(step.presentation.orderStatus.kind, "waiting");
  canonicalPins.waitingAtItems = machine.snapshot();

  step = expectStep(machine.deliverOrder(order7), "accepted");
  trace.push({ step: 3, ...step });
  assert.equal(machine.state.draft.baseRevision, 7);
  assert.equal(Object.keys(machine.state.draft.selections).length, 0);

  step = expectStep(machine.chooseQuantity("lamp", 1), "accepted");
  trace.push({ step: 4, ...step });
  assert.equal(step.presentation.itemsComplete, false);
  canonicalPins.incompleteItems = machine.snapshot();

  step = expectStep(machine.chooseReason("lamp", "damaged"), "accepted");
  trace.push({ step: 5, ...step });
  assert.equal(step.presentation.itemsComplete, true);
  assert.equal(step.presentation.actions.chooseQuantity.lamp, true);
  assert.equal(step.presentation.actions.chooseReason.lamp, true);
  assert.equal(step.presentation.actions.goToStep.method, true);
  assert.equal(step.presentation.actions.submit, false);
  canonicalPins.completeItems = machine.snapshot();

  step = expectStep(machine.openPolicy("lamp"), "accepted");
  trace.push({ step: 6, ...step });
  assert.equal(machine.state.surface.id, "surface-1");
  assert.equal(
    step.presentation.policySurface.policySummary,
    "Return the lamp in protective packaging.",
  );
  canonicalPins.openPolicySurface = machine.snapshot();

  const draftBeforeDismiss = machine.snapshot().draft;
  step = expectStep(machine.dismissPolicy("surface-1"), "accepted");
  trace.push({ step: 7, ...step });
  assert.equal(machine.state.surface, null);
  assert.deepEqual(machine.state.draft, draftBeforeDismiss);

  step = expectStep(
    machine.goToStep("method"),
    "accepted",
    [nav("push", urls.method)],
  );
  trace.push({ step: 8, ...step });
  assert.equal(step.presentation.deliveredLocation, urls.items);

  const itemsScope = machine.state.routeScope;
  step = expectStep(machine.deliverLocation(urls.method), "accepted");
  trace.push({ step: 9, ...step });
  assert.equal(step.presentation.activeStep, "method");
  assert.notEqual(machine.state.routeScope, itemsScope);

  step = expectStep(machine.chooseMethod("drop-off"), "accepted");
  trace.push({ step: 10, ...step });
  assert.equal(step.presentation.methodComplete, true);
  canonicalPins.methodSelection = machine.snapshot();

  step = expectStep(
    machine.goToStep("review"),
    "accepted",
    [nav("push", urls.review)],
  );
  trace.push({ step: 11, ...step });

  step = expectStep(machine.deliverLocation(urls.review), "accepted");
  trace.push({ step: 12, ...step });
  assert.equal(step.presentation.activeStep, "review");
  assert.equal(step.presentation.actions.submit, true);
  canonicalPins.review = machine.snapshot();

  const request1 = pendingSnapshot(
    "request-1",
    7,
    "lamp",
    1,
    "damaged",
    "drop-off",
  );
  step = expectStep(
    machine.submit(),
    "accepted",
    [submitCommand(request1)],
  );
  trace.push({ step: 13, ...step });
  assert.deepEqual(machine.state.phase.snapshot, request1);
  assert.equal(step.presentation.actions.submit, false);

  const beforeDuplicateSubmit = machine.snapshot();
  step = expectStep(machine.submit(), "duplicate");
  trace.push({ step: 14, ...step });
  assertNoMutation(beforeDuplicateSubmit, step, "duplicate Submit");

  step = expectStep(machine.deliverLocation(urls.method), "accepted");
  trace.push({ step: 15, ...step });
  assert.equal(step.presentation.activeStep, "method");
  assert.deepEqual(machine.state.phase.snapshot, request1);

  const draftBeforeRefusal = machine.snapshot().draft;
  const orderBeforeRefusal = machine.snapshot().order;
  const scopeBeforeRefusal = machine.state.routeScope;
  step = expectStep(
    machine.settle("request-1", refusal(8)),
    "accepted",
  );
  trace.push({ step: 16, ...step });
  assert.equal(machine.state.revisionFence, 8);
  assert.deepEqual(machine.state.phase, {
    kind: "refused",
    request: request1,
    currentRevision: 8,
  });
  assert.deepEqual(machine.state.draft, draftBeforeRefusal);
  assert.deepEqual(machine.state.order, orderBeforeRefusal);
  assert.equal(machine.state.routeScope, scopeBeforeRefusal);
  assert.equal(step.presentation.activeStep, "method");
  assert.equal(step.consequences.length, 0);
  canonicalPins.domainRefusal = machine.snapshot();

  step = expectStep(
    machine.deliverOrder(order8),
    "accepted",
    [nav("replace", urls.items)],
  );
  trace.push({ step: 17, ...step });
  assert.deepEqual(machine.state.draft.selections.lamp, {
    quantity: 1,
    reason: "damaged",
  });
  assert.equal(step.presentation.conflict.kind, "source-changed");
  assert.equal(step.presentation.deliveredLocation, urls.method);

  step = expectStep(machine.deliverLocation(urls.items), "accepted");
  trace.push({ step: 18, ...step });
  assert.equal(step.presentation.activeStep, "items");
  assert.equal(step.presentation.conflict.kind, "source-changed");
  canonicalPins.sourceChangedConflict = machine.snapshot();

  step = expectStep(machine.restartFromCurrentOrder(), "accepted");
  trace.push({ step: 19, ...step });
  assert.equal(machine.state.draft.baseRevision, 8);
  assert.equal(Object.keys(machine.state.draft.selections).length, 0);
  assert.equal(machine.state.revisionFence, null);
  assert.equal(machine.state.phase.kind, "idle");

  step = expectStep(machine.chooseQuantity("mug", 1), "accepted");
  trace.push({ step: 20, ...step });
  assert.equal(step.presentation.itemsComplete, false);

  step = expectStep(machine.chooseReason("mug", "not-needed"), "accepted");
  trace.push({ step: 21, ...step });
  assert.equal(step.presentation.itemsComplete, true);

  step = expectStep(
    machine.goToStep("method"),
    "accepted",
    [nav("push", urls.method)],
  );
  trace.push({ step: 22, ...step });

  step = expectStep(machine.deliverLocation(urls.method), "accepted");
  trace.push({ step: 23, ...step });
  assert.equal(step.presentation.activeStep, "method");

  step = expectStep(machine.chooseMethod("pickup"), "accepted");
  trace.push({ step: 24, ...step });

  step = expectStep(
    machine.goToStep("review"),
    "accepted",
    [nav("push", urls.review)],
  );
  trace.push({ step: 25, ...step });

  step = expectStep(machine.deliverLocation(urls.review), "accepted");
  trace.push({ step: 26, ...step });

  const request2 = pendingSnapshot(
    "request-2",
    8,
    "mug",
    1,
    "not-needed",
    "pickup",
  );
  step = expectStep(
    machine.submit(),
    "accepted",
    [submitCommand(request2)],
  );
  trace.push({ step: 27, ...step });

  step = expectStep(machine.deliverLocation(urls.method), "accepted");
  trace.push({ step: 28, ...step });
  assert.deepEqual(machine.state.phase.snapshot, request2);
  assert.equal(machine.state.navigationIntent, null);
  canonicalPins.pendingAfterBack = machine.snapshot();
  canonicalPrefixDigest = digest(trace);
  canonicalCheckpoint = machine.checkpoint({
    scenario: "canonical",
    afterStep: 28,
    prefixDigest: canonicalPrefixDigest,
  });

  step = expectStep(
    machine.settle("request-2", acceptance("return-900")),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  trace.push({ step: 29, ...step });
  assert.equal(machine.state.completedReturn, "return-900");
  assert.equal(machine.state.draft, null);
  assert.equal(step.presentation.deliveredLocation, urls.method);

  step = expectStep(
    machine.deliverLocation(urls.receipt("return-900")),
    "accepted",
  );
  trace.push({ step: 30, ...step });
  assert.deepEqual(step.presentation.receipt, {
    returnId: "return-900",
    status: "completed",
  });
  assert.equal(step.presentation.completionNotice, null);
  canonicalPins.receipt = machine.snapshot();

  const beforeDuplicateAcceptance = machine.snapshot();
  step = expectStep(
    machine.settle("request-2", acceptance("return-900")),
    "stale",
  );
  trace.push({ step: 31, ...step });
  assertNoMutation(
    beforeDuplicateAcceptance,
    step,
    "duplicate acceptance must be stale and inert",
  );

  assert.equal(trace.length, 32);
  assert.deepEqual(
    trace.map((row) => row.step),
    Array.from({ length: 32 }, (_, index) => index),
  );
});

check("scenario 1: complete items without method normalizes review to method", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order7), "accepted");
  expectStep(machine.chooseQuantity("lamp", 1), "accepted");
  expectStep(machine.chooseReason("lamp", "damaged"), "accepted");
  const step = expectStep(
    machine.deliverLocation(urls.review),
    "accepted",
    [nav("replace", urls.method)],
  );
  assert.equal(step.presentation.normalizing, true);
  assert.equal(machine.state.routeScope, null);
});

check("scenario 2: missing and unknown step normalize to items", () => {
  for (const location of [urls.missingStep, urls.unknownStep]) {
    const machine = new ReturnDeskOracle();
    const step = expectStep(
      machine.deliverLocation(location),
      "accepted",
      [nav("replace", urls.items)],
    );
    assert.equal(step.presentation.normalizing, true);
    assert.equal(step.presentation.deliveredLocation, location);
  }
});

check("scenario 3: Forward redelivery after restart uses normal admissibility", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order7), "accepted");
  expectStep(machine.chooseQuantity("lamp", 1), "accepted");
  expectStep(machine.chooseReason("lamp", "damaged"), "accepted");
  expectStep(machine.deliverOrder(order8), "accepted");
  expectStep(machine.restartFromCurrentOrder(), "accepted");
  const step = expectStep(
    machine.deliverLocation(urls.method),
    "accepted",
    [nav("replace", urls.items)],
  );
  assert.equal(step.presentation.normalizing, true);
});

check("scenario 4: older order revision is stale", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverOrder(order8), "accepted");
  const before = machine.snapshot();
  const step = expectStep(machine.deliverOrder(order7), "stale");
  assertNoMutation(before, step, "older order observation");
  assert.equal(machine.state.order.revision, 8);
});

check("scenario 5: removed selected line remains conflicted and closes surface", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order7), "accepted");
  expectStep(machine.chooseQuantity("lamp", 1), "accepted");
  expectStep(machine.chooseReason("lamp", "damaged"), "accepted");
  expectStep(machine.openPolicy("lamp"), "accepted");
  const allocationBefore = machine.state.nextSurface;

  const step = expectStep(machine.deliverOrder(order9WithoutLamp), "accepted");
  assert.equal(machine.state.surface, null);
  assert.equal(machine.state.nextSurface, allocationBefore);
  assert.deepEqual(machine.state.draft.selections.lamp, {
    quantity: 1,
    reason: "damaged",
  });
  assert.equal(step.presentation.conflict.kind, "source-changed");
});

check("scenario 6: stale surface cannot close a newer surface", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order7), "accepted");
  expectStep(machine.openPolicy("lamp"), "accepted");
  const firstScope = machine.state.routeScope;

  expectStep(
    machine.followOrderLink(),
    "accepted",
    [nav("push", urls.order)],
  );
  expectStep(machine.deliverLocation(urls.order), "accepted");
  assert.equal(machine.state.surface, null);
  expectStep(machine.deliverLocation(urls.items), "accepted");
  assert.notEqual(machine.state.routeScope, firstScope);
  expectStep(machine.openPolicy("mug"), "accepted");
  assert.equal(machine.state.surface.id, "surface-2");

  const before = machine.snapshot();
  const stale = expectStep(machine.dismissPolicy("surface-1"), "stale");
  assertNoMutation(before, stale, "stale surface dismissal");
  assert.equal(machine.state.surface.id, "surface-2");
});

let retryUnavailablePin;
check("scenario 7: unavailable preserves draft and retry uses fresh identity", () => {
  const machine = readyReview();
  const request1 = pendingSnapshot(
    "request-1",
    7,
    "lamp",
    1,
    "damaged",
    "drop-off",
  );
  expectStep(machine.submit(), "accepted", [submitCommand(request1)]);
  const authoredDraft = machine.snapshot().draft;
  expectStep(machine.settle("request-1", unavailable), "accepted");
  assert.deepEqual(machine.state.draft, authoredDraft);
  assert.equal(machine.state.phase.kind, "unavailable");
  retryUnavailablePin = machine.snapshot();

  const request2 = { ...request1, requestId: "request-2" };
  expectStep(machine.submit(), "accepted", [submitCommand(request2)]);
  assert.equal(machine.state.phase.snapshot.requestId, "request-2");
  assert.deepEqual(machine.state.phase.snapshot.selections, request1.selections);
});

check("scenario 8: non-newer refusal is invalid and request stays pending", () => {
  const machine = readyReview();
  expectStep(
    machine.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  const before = machine.snapshot();
  const step = expectStep(
    machine.settle("request-1", refusal(7)),
    "invalid(refusal-revision)",
  );
  assertNoMutation(before, step, "invalid refusal");
  assert.equal(machine.state.phase.kind, "pending");
  assert.equal(machine.state.phase.snapshot.requestId, "request-1");
});

check("scenario 9: fence blocks edits and Submit until observation plus restart", () => {
  const machine = readyReview();
  expectStep(
    machine.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  expectStep(machine.deliverLocation(urls.method), "accepted");
  expectStep(machine.settle("request-1", refusal(8)), "accepted");
  expectStep(machine.deliverLocation(urls.items), "accepted");

  expectStep(
    machine.chooseQuantity("lamp", 2),
    "blocked(source-changed)",
  );
  expectStep(machine.deliverLocation(urls.review), "accepted");
  expectStep(machine.submit(), "blocked(source-changed)");
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(
    machine.restartFromCurrentOrder(),
    "blocked(revision-fence)",
  );

  expectStep(machine.deliverOrder(order8), "accepted");
  expectStep(machine.restartFromCurrentOrder(), "accepted");
  assert.equal(machine.state.revisionFence, null);
  assert.equal(machine.state.draft.baseRevision, 8);
});

let offeredReceiptPin;
check("scenario 10: late outside-flow acceptance creates notice and semantic link", () => {
  const machine = readyReview();
  expectStep(
    machine.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  expectStep(
    machine.followOrderLink(),
    "accepted",
    [nav("push", urls.order)],
  );
  expectStep(machine.deliverLocation(urls.order), "accepted");

  const acceptedOutside = expectStep(
    machine.settle("request-1", acceptance("return-900")),
    "accepted",
  );
  assert.deepEqual(acceptedOutside.consequences, []);
  assert.equal(acceptedOutside.presentation.deliveredLocation, urls.order);
  assert.deepEqual(acceptedOutside.presentation.completionNotice, {
    returnId: "return-900",
    receipt: urls.receipt("return-900"),
  });
  offeredReceiptPin = machine.snapshot();

  const oldFlowDetour = new ReturnDeskOracle(offeredReceiptPin);
  expectStep(
    oldFlowDetour.deliverLocation(urls.items),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  assert.equal(oldFlowDetour.state.receiptAccess, "redirecting");
  assert.equal(oldFlowDetour.presentation().completionNotice, null);
  expectStep(
    oldFlowDetour.deliverLocation(urls.receipt("return-900")),
    "accepted",
  );
  assert.equal(oldFlowDetour.state.receiptAccess, "acknowledged");
  assert.equal(oldFlowDetour.presentation().completionNotice, null);

  expectStep(
    machine.followReceiptLink(),
    "accepted",
    [nav("push", urls.receipt("return-900"))],
  );
  expectStep(
    machine.deliverLocation(urls.receipt("return-900")),
    "accepted",
  );
  assert.equal(machine.state.receiptAccess, "acknowledged");
  assert.equal(machine.presentation().completionNotice, null);

  const supersededRedirect = readyReview();
  const supersededRequest = pendingSnapshot(
    "request-1",
    7,
    "lamp",
    1,
    "damaged",
    "drop-off",
  );
  expectStep(
    supersededRedirect.submit(),
    "accepted",
    [submitCommand(supersededRequest)],
  );
  expectStep(
    supersededRedirect.settle(
      "request-1",
      acceptance("return-redirected"),
    ),
    "accepted",
    [nav("replace", urls.receipt("return-redirected"))],
  );
  assert.equal(supersededRedirect.state.receiptAccess, "redirecting");
  expectStep(
    supersededRedirect.deliverLocation(urls.order),
    "accepted",
  );
  assert.equal(supersededRedirect.state.receiptAccess, "offered");
  assert.deepEqual(
    supersededRedirect.presentation().completionNotice,
    {
      returnId: "return-redirected",
      receipt: urls.receipt("return-redirected"),
    },
  );
  expectStep(
    supersededRedirect.followReceiptLink(),
    "accepted",
    [nav("push", urls.receipt("return-redirected"))],
  );
});

check("scenario 11: unknown or conflicting settlement is inert", () => {
  const machine = readyReview();
  expectStep(
    machine.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  const before = machine.snapshot();
  const unknown = expectStep(
    machine.settle("request-999", acceptance("return-evil")),
    "stale",
  );
  assertNoMutation(before, unknown, "unknown settlement");
  assert.equal(machine.state.completedReturn, null);
  assert.equal(machine.state.phase.snapshot.requestId, "request-1");

  expectStep(machine.settle("request-1", unavailable), "accepted");
  const settled = machine.snapshot();
  const contradictory = expectStep(
    machine.settle("request-1", acceptance("return-evil")),
    "stale",
  );
  assertNoMutation(settled, contradictory, "contradictory settlement");
});

function completedAtReceipt() {
  const machine = readyReview();
  expectStep(
    machine.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  expectStep(
    machine.settle("request-1", acceptance("return-900")),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  expectStep(
    machine.deliverLocation(urls.receipt("return-900")),
    "accepted",
  );
  return machine;
}

check("scenario 12: old flow after completion replaces to retained receipt", () => {
  const machine = completedAtReceipt();
  const step = expectStep(
    machine.deliverLocation(urls.items),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  assert.equal(step.presentation.normalizing, true);
  assert.equal(machine.state.draft, null);
  assert.equal(machine.state.routeScope, null);
  assert.equal(machine.state.receiptAccess, "acknowledged");
  assert.equal(machine.presentation().completionNotice, null);

  const acknowledged = completedAtReceipt();
  expectStep(
    acknowledged.followOrderLink(),
    "accepted",
    [nav("push", urls.order)],
  );
  expectStep(acknowledged.deliverLocation(urls.order), "accepted");
  assert.equal(acknowledged.state.receiptAccess, "acknowledged");
  assert.equal(acknowledged.presentation().completionNotice, null);
  expectStep(
    acknowledged.followReceiptLink(),
    "blocked(no-completion-notice)",
  );
  expectStep(
    acknowledged.deliverLocation(urls.items),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  assert.equal(acknowledged.state.receiptAccess, "acknowledged");
  assert.equal(acknowledged.presentation().completionNotice, null);
});

check("scenario 13: repeated normalization delivery emits no second navigation", () => {
  const incomplete = new ReturnDeskOracle();
  expectStep(
    incomplete.deliverLocation(urls.review),
    "accepted",
    [nav("replace", urls.items)],
  );
  const beforeIncompleteRepeat = incomplete.snapshot();
  const repeatedIncomplete = expectStep(
    incomplete.deliverLocation(urls.review),
    "duplicate",
  );
  assertNoMutation(
    beforeIncompleteRepeat,
    repeatedIncomplete,
    "repeated inadmissible location",
  );

  const completed = completedAtReceipt();
  expectStep(
    completed.deliverLocation(urls.items),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  const beforeCompletedRepeat = completed.snapshot();
  const repeatedCompleted = expectStep(
    completed.deliverLocation(urls.items),
    "duplicate",
  );
  assertNoMutation(
    beforeCompletedRepeat,
    repeatedCompleted,
    "repeated completed-flow location",
  );
});

check("scenario 14: one navigation intent deduplicates, blocks, then clears", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order7), "accepted");
  expectStep(machine.chooseQuantity("lamp", 1), "accepted");
  expectStep(machine.chooseReason("lamp", "damaged"), "accepted");

  expectStep(
    machine.goToStep("method"),
    "accepted",
    [nav("push", urls.method)],
  );
  expectStep(machine.goToStep("method"), "duplicate");
  expectStep(
    machine.followOrderLink(),
    "blocked(navigation-pending)",
  );
  expectStep(machine.deliverLocation(urls.method), "accepted");
  assert.equal(machine.state.navigationIntent, null);
});

check("audit regressions: invalid delivery precedence and link ownership", () => {
  const pending = readyReview();
  expectStep(
    pending.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  expectStep(
    pending.settle("request-999", {
      kind: "refused",
      currentRevision: "8",
    }),
    "invalid(refusal-revision)",
  );
  expectStep(
    pending.settle("request-999", {
      kind: "refused",
      currentRevision: 0,
    }),
    "invalid(refusal-revision)",
  );
  expectStep(
    pending.settle("request-999", {
      kind: "refused",
      currentRevision: -1,
    }),
    "invalid(refusal-revision)",
  );
  expectStep(
    pending.settle("request-999", acceptance("\ud800")),
    "invalid(return-id)",
  );
  expectStep(
    pending.settle({ malformed: true }, unavailable),
    "invalid(request-id)",
  );
  expectStep(
    pending.settle("", unavailable),
    "invalid(request-id)",
  );
  expectStep(
    pending.settle("valid-but-never-emitted", unavailable),
    "invalid(request-id)",
  );
  expectStep(
    pending.settle("request-999", unavailable),
    "stale",
  );
  expectStep(
    pending.settle(
      "request-1",
      refusal(Number.MAX_SAFE_INTEGER + 1),
    ),
    "invalid(refusal-revision)",
  );
  assert.equal(pending.state.phase.snapshot.requestId, "request-1");

  const unsafeOrder = {
    ...order7,
    revision: Number.MAX_SAFE_INTEGER + 1,
    lines: [...order7.lines],
    allowedMethods: [...order7.allowedMethods],
  };
  const unsafeQuantityOrder = {
    ...order7,
    lines: [
      {
        ...order7.lines[0],
        purchasedQuantity: Number.MAX_SAFE_INTEGER + 1,
      },
    ],
    allowedMethods: [...order7.allowedMethods],
  };
  const safeDomain = new ReturnDeskOracle();
  expectStep(safeDomain.deliverOrder(unsafeOrder), "invalid(order)");
  expectStep(
    safeDomain.deliverOrder(unsafeQuantityOrder),
    "invalid(order)",
  );
  expectStep(safeDomain.deliverLocation(urls.items), "accepted");
  expectStep(safeDomain.deliverOrder(order7), "accepted");
  expectStep(
    safeDomain.chooseQuantity(
      "lamp",
      Number.MAX_SAFE_INTEGER + 1,
    ),
    "invalid(quantity)",
  );

  const maximumSafeOrder = {
    ...order7,
    revision: Number.MAX_SAFE_INTEGER,
    lines: [...order7.lines],
    allowedMethods: [...order7.allowedMethods],
  };
  const maximumSafe = readyReview({ order: maximumSafeOrder });
  const maximumSafeRequest = pendingSnapshot(
    "request-1",
    Number.MAX_SAFE_INTEGER,
    "lamp",
    1,
    "damaged",
    "drop-off",
  );
  expectStep(
    maximumSafe.submit(),
    "accepted",
    [submitCommand(maximumSafeRequest)],
  );

  const surfaceAllocator = new ReturnDeskOracle();
  expectStep(surfaceAllocator.deliverLocation(urls.items), "accepted");
  expectStep(surfaceAllocator.deliverOrder(order7), "accepted");
  surfaceAllocator.state.nextSurface = Number.MAX_SAFE_INTEGER;
  expectStep(surfaceAllocator.openPolicy("lamp"), "accepted");
  assert.equal(
    surfaceAllocator.state.surface.id,
    `surface-${Number.MAX_SAFE_INTEGER}`,
  );
  assert.equal(
    surfaceAllocator.state.nextSurface,
    BigInt(Number.MAX_SAFE_INTEGER) + 1n,
  );
  expectStep(surfaceAllocator.openPolicy("mug"), "accepted");
  assert.equal(
    surfaceAllocator.state.surface.id,
    `surface-${BigInt(Number.MAX_SAFE_INTEGER) + 1n}`,
  );
  const promotedCheckpoint = surfaceAllocator.checkpoint({
    scenario: "exact-counter-promotion",
  });
  assert.deepEqual(
    ReturnDeskOracle.restore(promotedCheckpoint).snapshot(),
    surfaceAllocator.snapshot(),
  );

  const routeAllocator = new ReturnDeskOracle();
  routeAllocator.state.nextRouteScope = Number.MAX_SAFE_INTEGER;
  expectStep(routeAllocator.deliverLocation(urls.items), "accepted");
  assert.equal(
    routeAllocator.state.routeScope,
    `scope-${Number.MAX_SAFE_INTEGER}`,
  );
  assert.equal(
    routeAllocator.state.nextRouteScope,
    BigInt(Number.MAX_SAFE_INTEGER) + 1n,
  );
  expectStep(routeAllocator.deliverOrder(order7), "accepted");
  expectStep(routeAllocator.chooseQuantity("lamp", 1), "accepted");
  expectStep(routeAllocator.chooseReason("lamp", "damaged"), "accepted");
  expectStep(
    routeAllocator.goToStep("method"),
    "accepted",
    [nav("push", urls.method)],
  );
  expectStep(routeAllocator.deliverLocation(urls.method), "accepted");
  assert.equal(
    routeAllocator.state.routeScope,
    `scope-${BigInt(Number.MAX_SAFE_INTEGER) + 1n}`,
  );

  const outside = new ReturnDeskOracle();
  expectStep(outside.deliverLocation(urls.order), "accepted");
  expectStep(
    outside.followOrderLink(),
    "blocked(wrong-location)",
  );
});

check("audit regressions: removed current-domain line edits are invalid", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order7), "accepted");
  expectStep(machine.chooseQuantity("lamp", 1), "accepted");
  expectStep(machine.chooseReason("lamp", "damaged"), "accepted");
  expectStep(machine.deliverOrder(order9WithoutLamp), "accepted");

  expectStep(
    machine.chooseQuantity("lamp", 1),
    "invalid(unknown-line)",
  );
  expectStep(
    machine.chooseReason("lamp", "not-needed"),
    "invalid(unknown-line)",
  );

  const noObservation = new ReturnDeskOracle();
  expectStep(
    noObservation.chooseQuantity("lamp", 1),
    "invalid(unknown-line)",
  );
  expectStep(
    noObservation.chooseReason("mug", "damaged"),
    "invalid(unknown-line)",
  );
});

check("audit regressions: opaque IDs are own keys and canonically ordered", () => {
  const composed = "é";
  const decomposed = "e\u0301";
  const line = (id) => ({
    id,
    title: id,
    purchasedQuantity: 1,
    returnableQuantity: 1,
    policySummary: id,
  });
  const opaqueOrder = (ids) => ({
    id: "order-100",
    revision: 1,
    lines: ids.map(line),
    allowedMethods: ["drop-off", "pickup"],
  });

  const duplicate = new ReturnDeskOracle();
  expectStep(
    duplicate.deliverOrder(
      opaqueOrder([composed, decomposed]),
    ),
    "accepted",
  );
  expectStep(
    duplicate.deliverOrder(
      opaqueOrder([decomposed, composed]),
    ),
    "duplicate",
  );

  const emptyOpaqueId = new ReturnDeskOracle();
  expectStep(
    emptyOpaqueId.deliverOrder(opaqueOrder([""])),
    "accepted",
  );
  assert.equal(emptyOpaqueId.state.order.lines[0].id, "");

  const requestFor = (ids) => {
    const machine = new ReturnDeskOracle();
    expectStep(machine.deliverLocation(urls.items), "accepted");
    expectStep(machine.deliverOrder(opaqueOrder(ids)), "accepted");
    for (const id of ids) {
      expectStep(machine.chooseQuantity(id, 1), "accepted");
      expectStep(machine.chooseReason(id, "damaged"), "accepted");
    }
    expectStep(
      machine.goToStep("method"),
      "accepted",
      [nav("push", urls.method)],
    );
    expectStep(machine.deliverLocation(urls.method), "accepted");
    expectStep(machine.chooseMethod("drop-off"), "accepted");
    expectStep(
      machine.goToStep("review"),
      "accepted",
      [nav("push", urls.review)],
    );
    expectStep(machine.deliverLocation(urls.review), "accepted");
    const submitted = machine.submit();
    assert.equal(submitted.result, "accepted");
    return submitted.consequences;
  };

  assert.equal(requestFor([""])[0].snapshot.selections[0].lineId, "");

  assert.deepEqual(
    requestFor([composed, decomposed]),
    requestFor([decomposed, composed]),
  );

  const malformedText = new ReturnDeskOracle();
  expectStep(
    malformedText.deliverOrder(opaqueOrder(["\ud800"])),
    "invalid(order)",
  );

  const prototypeIds = new ReturnDeskOracle();
  expectStep(prototypeIds.deliverLocation(urls.items), "accepted");
  expectStep(
    prototypeIds.deliverOrder(
      opaqueOrder(["constructor", "__proto__"]),
    ),
    "accepted",
  );
  expectStep(
    prototypeIds.chooseReason("constructor", "damaged"),
    "invalid(unselected-line)",
  );
  for (const id of ["constructor", "__proto__"]) {
    expectStep(prototypeIds.chooseQuantity(id, 1), "accepted");
    expectStep(prototypeIds.chooseReason(id, "damaged"), "accepted");
  }
  assert.deepEqual(
    Object.keys(prototypeIds.state.draft.selections).sort(),
    ["__proto__", "constructor"],
  );
  const prototypePresentation = prototypeIds.presentation();
  assert.equal(prototypePresentation.itemsComplete, true);
  for (const action of [
    prototypePresentation.actions.chooseQuantity,
    prototypePresentation.actions.chooseReason,
    prototypePresentation.actions.openPolicy,
  ]) {
    assert.equal(Object.hasOwn(action, "__proto__"), true);
    assert.equal(typeof action.__proto__, "boolean");
  }
});

check("audit regressions: boundary records are closed own data", () => {
  const inheritedLine = Object.create({
    id: "inherited",
    title: "Inherited",
    purchasedQuantity: 1,
    returnableQuantity: 1,
    policySummary: "Inherited",
  });
  const inheritedOrder = {
    id: "order-100",
    revision: 1,
    lines: [inheritedLine],
    allowedMethods: ["drop-off"],
  };
  const orderMachine = new ReturnDeskOracle();
  expectStep(orderMachine.deliverOrder(inheritedOrder), "invalid(order)");
  assert.equal(orderMachine.state.order, null);

  const extraLineOrder = {
    id: "order-100",
    revision: 1,
    lines: [
      {
        id: "extra",
        title: "Extra",
        purchasedQuantity: 1,
        returnableQuantity: 1,
        policySummary: "Extra",
        helper: () => "not data",
      },
    ],
    allowedMethods: ["drop-off"],
  };
  expectStep(orderMachine.deliverOrder(extraLineOrder), "invalid(order)");
  assert.equal(orderMachine.state.order, null);

  let revisionReads = 0;
  const accessorOrder = {
    id: "order-100",
    lines: [...order7.lines],
    allowedMethods: [...order7.allowedMethods],
  };
  Object.defineProperty(accessorOrder, "revision", {
    get() {
      revisionReads += 1;
      return 7;
    },
    enumerable: true,
  });
  expectStep(orderMachine.deliverOrder(accessorOrder), "invalid(order)");
  assert.equal(revisionReads, 0, "accessor is rejected before reading");

  const symbolicOrder = {
    id: "order-100",
    revision: 7,
    lines: [...order7.lines],
    allowedMethods: [...order7.allowedMethods],
    [Symbol("hidden")]: true,
  };
  expectStep(orderMachine.deliverOrder(symbolicOrder), "invalid(order)");

  let lineReads = 0;
  const accessorLines = [];
  Object.defineProperty(accessorLines, "0", {
    get() {
      lineReads += 1;
      return order7.lines[0];
    },
    enumerable: true,
  });
  accessorLines.length = 1;
  expectStep(
    orderMachine.deliverOrder({
      id: "order-100",
      revision: 7,
      lines: accessorLines,
      allowedMethods: ["drop-off"],
    }),
    "invalid(order)",
  );
  assert.equal(lineReads, 0, "array accessor is rejected before reading");

  const reasonMachine = new ReturnDeskOracle();
  expectStep(reasonMachine.deliverLocation(urls.items), "accepted");
  expectStep(reasonMachine.deliverOrder(order7), "accepted");
  expectStep(reasonMachine.chooseQuantity("lamp", 1), "accepted");
  const inheritedReason = Object.create({ note: "inherited" });
  inheritedReason.kind = "other";
  inheritedReason.extra = true;
  expectStep(
    reasonMachine.chooseReason("lamp", inheritedReason),
    "invalid(reason)",
  );
  assert.equal(
    reasonMachine.state.draft.selections.lamp.reason,
    null,
  );

  let noteReads = 0;
  const accessorReason = { kind: "other" };
  Object.defineProperty(accessorReason, "note", {
    get() {
      noteReads += 1;
      return "hidden";
    },
    enumerable: true,
  });
  expectStep(
    reasonMachine.chooseReason("lamp", accessorReason),
    "invalid(reason)",
  );
  assert.equal(noteReads, 0, "reason accessor is rejected before reading");
});

check("audit regressions: invalid restored states are rejected", () => {
  const unavailableMachine = readyReview();
  expectStep(
    unavailableMachine.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  expectStep(
    unavailableMachine.settle("request-1", unavailable),
    "accepted",
  );

  const reused = unavailableMachine.snapshot();
  reused.settledRequests = [];
  reused.nextRequest = 1;
  assert.throws(() => new ReturnDeskOracle(reused));

  const unknownPhase = unavailableMachine.snapshot();
  unknownPhase.phase = { kind: "nonsense" };
  assert.throws(() => new ReturnDeskOracle(unknownPhase));

  const forgedReceiptAccess = new ReturnDeskOracle().snapshot();
  forgedReceiptAccess.receiptAccess = "offered";
  assert.throws(() => new ReturnDeskOracle(forgedReceiptAccess));

  const extraState = new ReturnDeskOracle().snapshot();
  extraState.hidden = true;
  assert.throws(() => new ReturnDeskOracle(extraState));

  const arbitraryFreshNavigation = new ReturnDeskOracle().snapshot();
  arbitraryFreshNavigation.navigationIntent = {
    kind: "user-push",
    target: urls.order,
  };
  assert.throws(() => new ReturnDeskOracle(arbitraryFreshNavigation));

  const mismatchedRoute = new ReturnDeskOracle();
  expectStep(
    mismatchedRoute.deliverLocation(urls.review),
    "accepted",
    [nav("replace", urls.items)],
  );
  const contradictory = mismatchedRoute.snapshot();
  contradictory.location.url = urls.items;
  assert.throws(() => new ReturnDeskOracle(contradictory));

  const normalizing = new ReturnDeskOracle();
  expectStep(
    normalizing.deliverLocation(urls.review),
    "accepted",
    [nav("replace", urls.items)],
  );
  const missingRequiredIntent = normalizing.snapshot();
  missingRequiredIntent.navigationIntent = null;
  assert.throws(() => new ReturnDeskOracle(missingRequiredIntent));

  const activeFlow = new ReturnDeskOracle();
  expectStep(activeFlow.deliverLocation(urls.items), "accepted");
  const missingScope = activeFlow.snapshot();
  missingScope.routeScope = null;
  missingScope.navigationIntent = {
    kind: "required-replace",
    target: urls.order,
  };
  assert.throws(() => new ReturnDeskOracle(missingScope));

  const pendingMachine = readyReview();
  expectStep(
    pendingMachine.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  const inconsistentPending = pendingMachine.snapshot();
  inconsistentPending.phase.snapshot.method = "pickup";
  assert.throws(() => new ReturnDeskOracle(inconsistentPending));

  const zeroRequest = pendingMachine.snapshot();
  zeroRequest.phase.snapshot.requestId = "request-0";
  zeroRequest.nextRequest = 1;
  assert.throws(() => new ReturnDeskOracle(zeroRequest));

  const invalidDraftMethod = readyReview().snapshot();
  invalidDraftMethod.draft.method = "drone";
  assert.throws(() => new ReturnDeskOracle(invalidDraftMethod));

  const extraDraftField = readyReview().snapshot();
  extraDraftField.draft.hidden = true;
  assert.throws(() => new ReturnDeskOracle(extraDraftField));

  const extraSelectionField = readyReview().snapshot();
  extraSelectionField.draft.selections.lamp.hidden = true;
  assert.throws(() => new ReturnDeskOracle(extraSelectionField));

  const ghostSelection = readyReview().snapshot();
  ghostSelection.draft.selections.ghost = {
    quantity: 1,
    reason: "damaged",
  };
  assert.throws(() => new ReturnDeskOracle(ghostSelection));

  const noncanonicalOrder = readyReview().snapshot();
  noncanonicalOrder.order.lines.reverse();
  assert.throws(() => new ReturnDeskOracle(noncanonicalOrder));

  const observedFlowWithoutDraft = readyReview().snapshot();
  observedFlowWithoutDraft.draft = null;
  assert.throws(() => new ReturnDeskOracle(observedFlowWithoutDraft));

  const draftWithoutOrder = readyReview().snapshot();
  draftWithoutOrder.order = null;
  assert.throws(() => new ReturnDeskOracle(draftWithoutOrder));

  const surfaceMachine = new ReturnDeskOracle();
  expectStep(surfaceMachine.deliverLocation(urls.items), "accepted");
  expectStep(surfaceMachine.deliverOrder(order7), "accepted");
  expectStep(surfaceMachine.chooseQuantity("lamp", 1), "accepted");
  expectStep(surfaceMachine.chooseReason("lamp", "damaged"), "accepted");
  expectStep(surfaceMachine.openPolicy("lamp"), "accepted");

  const surfaceOnMethod = surfaceMachine.snapshot();
  surfaceOnMethod.location = {
    url: urls.method,
    route: { kind: "flow", step: "method", rawStep: "method" },
  };
  assert.throws(() => new ReturnDeskOracle(surfaceOnMethod));

  const malformedSurface = surfaceMachine.snapshot();
  malformedSurface.surface.id = "surface-garbage";
  assert.throws(() => new ReturnDeskOracle(malformedSurface));

  const malformedScope = surfaceMachine.snapshot();
  malformedScope.routeScope = "scope-garbage";
  malformedScope.surface.ownerScope = "scope-garbage";
  assert.throws(() => new ReturnDeskOracle(malformedScope));

  const skippedSurfaceAllocation = surfaceMachine.snapshot();
  skippedSurfaceAllocation.nextSurface = 99;
  assert.throws(() => new ReturnDeskOracle(skippedSurfaceAllocation));

  const skippedScopeAllocation = surfaceMachine.snapshot();
  skippedScopeAllocation.nextRouteScope = 99;
  assert.throws(() => new ReturnDeskOracle(skippedScopeAllocation));

  const refusedMachine = readyReview();
  expectStep(
    refusedMachine.submit(),
    "accepted",
    [
      submitCommand(
        pendingSnapshot(
          "request-1",
          7,
          "lamp",
          1,
          "damaged",
          "drop-off",
        ),
      ),
    ],
  );
  expectStep(
    refusedMachine.settle("request-1", refusal(8)),
    "accepted",
  );
  const nonNewerFence = refusedMachine.snapshot();
  nonNewerFence.phase.currentRevision = 7;
  nonNewerFence.revisionFence = 7;
  assert.throws(() => new ReturnDeskOracle(nonNewerFence));

  const changedRefusedDraft = refusedMachine.snapshot();
  changedRefusedDraft.draft.selections = {};
  changedRefusedDraft.draft.method = null;
  assert.throws(() => new ReturnDeskOracle(changedRefusedDraft));

  const unavailableWithoutDraft = unavailableMachine.snapshot();
  unavailableWithoutDraft.draft = null;
  assert.throws(() => new ReturnDeskOracle(unavailableWithoutDraft));

  const changedUnavailableDraft = unavailableMachine.snapshot();
  changedUnavailableDraft.draft.selections.lamp.quantity = 2;
  assert.throws(() => new ReturnDeskOracle(changedUnavailableDraft));

  const twoSettlements = readyReview();
  const ledgerRequest1 = pendingSnapshot(
    "request-1",
    7,
    "lamp",
    1,
    "damaged",
    "drop-off",
  );
  expectStep(
    twoSettlements.submit(),
    "accepted",
    [submitCommand(ledgerRequest1)],
  );
  expectStep(
    twoSettlements.settle("request-1", unavailable),
    "accepted",
  );
  const ledgerRequest2 = {
    ...ledgerRequest1,
    requestId: "request-2",
  };
  expectStep(
    twoSettlements.submit(),
    "accepted",
    [submitCommand(ledgerRequest2)],
  );
  expectStep(
    twoSettlements.settle("request-2", unavailable),
    "accepted",
  );

  const reversedLedger = twoSettlements.snapshot();
  reversedLedger.settledRequests.reverse();
  assert.throws(() => new ReturnDeskOracle(reversedLedger));

  const gappedLedger = twoSettlements.snapshot();
  gappedLedger.settledRequests[1] = "request-3";
  gappedLedger.phase.request.requestId = "request-3";
  gappedLedger.nextRequest = 4;
  assert.throws(() => new ReturnDeskOracle(gappedLedger));

  const nonCurrentSettlement = twoSettlements.snapshot();
  nonCurrentSettlement.phase.request =
    unavailableMachine.snapshot().phase.request;
  assert.throws(() => new ReturnDeskOracle(nonCurrentSettlement));

  const incoherentRequestAllocator = new ReturnDeskOracle().snapshot();
  incoherentRequestAllocator.nextRequest = Number.MAX_SAFE_INTEGER;
  assert.throws(() => new ReturnDeskOracle(incoherentRequestAllocator));

  const completedWithoutOffer = completedAtReceipt().snapshot();
  completedWithoutOffer.location = {
    url: urls.order,
    route: { kind: "order" },
  };
  completedWithoutOffer.navigationIntent = null;
  completedWithoutOffer.routeScope = null;
  completedWithoutOffer.receiptAccess = "redirecting";
  assert.throws(() => new ReturnDeskOracle(completedWithoutOffer));

  const checkpoint = unavailableMachine.checkpoint({
    scenario: "tamper-regression",
    prefixDigest: "fixture",
  });
  checkpoint.state.nextRequest = 1;
  checkpoint.stateHash = digest(checkpoint.state);
  assert.throws(() => ReturnDeskOracle.restore(checkpoint));
});

check("audit regressions: opaque return IDs round-trip through receipt routes", () => {
  const fresh = new ReturnDeskOracle();
  expectStep(
    fresh.deliverLocation(urls.receipt("stranger")),
    "invalid(location)",
  );

  const completed = completedAtReceipt();
  expectStep(
    completed.deliverLocation(urls.receipt("stranger")),
    "invalid(location)",
  );

  for (const opaqueReturnId of [
    "return/with/slash",
    ".",
    "..",
    "~reserved-prefix",
  ]) {
    const machine = readyReview();
    const request = pendingSnapshot(
      "request-1",
      7,
      "lamp",
      1,
      "damaged",
      "drop-off",
    );
    expectStep(machine.submit(), "accepted", [submitCommand(request)]);

    const receiptUrl = urls.receipt(opaqueReturnId);
    assert.equal(
      new URL(receiptUrl, "https://example.test").pathname,
      receiptUrl,
      `${opaqueReturnId}: browser path normalization is inert`,
    );
    expectStep(
      machine.settle("request-1", acceptance(opaqueReturnId)),
      "accepted",
      [nav("replace", receiptUrl)],
    );
    const delivered = expectStep(
      machine.deliverLocation(receiptUrl),
      "accepted",
    );
    assert.deepEqual(delivered.presentation.receipt, {
      returnId: opaqueReturnId,
      status: "completed",
    });
    assert.equal(delivered.presentation.completionNotice, null);
  }
  assert.equal(
    urls.receipt("return/with/slash"),
    "/returns/return%2Fwith%2Fslash",
  );
  assert.equal(urls.receipt(""), "/returns/~");
  assert.equal(urls.receipt("."), "/returns/~Lg");
  assert.equal(urls.receipt(".."), "/returns/~Li4");

  const automaticRedirect = new ReturnDeskOracle(offeredReceiptPin);
  expectStep(
    automaticRedirect.deliverLocation(urls.items),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  expectStep(
    automaticRedirect.followReceiptLink(),
    "blocked(no-completion-notice)",
  );
});

check("static pins contain full semantic and lifecycle context", () => {
  const requiredPins = {
    waitingAtItems: canonicalPins.waitingAtItems,
    incompleteItems: canonicalPins.incompleteItems,
    completeItems: canonicalPins.completeItems,
    methodSelection: canonicalPins.methodSelection,
    review: canonicalPins.review,
    openPolicySurface: canonicalPins.openPolicySurface,
    pendingAfterBack: canonicalPins.pendingAfterBack,
    sourceChangedConflict: canonicalPins.sourceChangedConflict,
    domainRefusal: canonicalPins.domainRefusal,
    retryableUnavailability: retryUnavailablePin,
    completionNoticeOutsideFlow: offeredReceiptPin,
    receipt: canonicalPins.receipt,
  };

  assert.equal(Object.keys(requiredPins).length, 12);
  for (const [name, pin] of Object.entries(requiredPins)) {
    assert.notEqual(pin, undefined, `${name}: pin exists`);
    for (const key of [
      "location",
      "order",
      "draft",
      "phase",
      "revisionFence",
      "completedReturn",
      "receiptAccess",
      "routeScope",
      "nextRouteScope",
      "surface",
      "nextSurface",
      "navigationIntent",
      "nextRequest",
      "settledRequests",
    ]) {
      assert.equal(
        Object.hasOwn(pin, key),
        true,
        `${name}: complete context contains ${key}`,
      );
    }
  }

  assert.equal(
    requiredPins.waitingAtItems.location.url,
    urls.items,
  );
  assert.equal(requiredPins.waitingAtItems.order, null);
  assert.equal(requiredPins.waitingAtItems.draft, null);
  assert.notEqual(requiredPins.waitingAtItems.routeScope, null);

  assert.deepEqual(
    requiredPins.incompleteItems.draft.selections.lamp,
    { quantity: 1, reason: null },
  );
  assert.deepEqual(
    requiredPins.completeItems.draft.selections.lamp,
    { quantity: 1, reason: "damaged" },
  );
  assert.equal(
    requiredPins.methodSelection.draft.method,
    "drop-off",
  );
  assert.equal(requiredPins.methodSelection.location.url, urls.method);
  assert.equal(requiredPins.review.location.url, urls.review);

  assert.equal(
    requiredPins.openPolicySurface.surface.id,
    "surface-1",
  );
  assert.equal(
    requiredPins.openPolicySurface.surface.ownerScope,
    requiredPins.openPolicySurface.routeScope,
  );

  assert.equal(
    requiredPins.pendingAfterBack.phase.snapshot.requestId,
    "request-2",
  );
  assert.equal(requiredPins.pendingAfterBack.location.url, urls.method);
  assert.deepEqual(
    requiredPins.pendingAfterBack.settledRequests,
    ["request-1"],
  );
  assert.equal(requiredPins.pendingAfterBack.navigationIntent, null);

  assert.equal(
    requiredPins.sourceChangedConflict.order.revision,
    8,
  );
  assert.equal(
    requiredPins.sourceChangedConflict.draft.baseRevision,
    7,
  );
  assert.deepEqual(
    requiredPins.sourceChangedConflict.draft.selections.lamp,
    { quantity: 1, reason: "damaged" },
  );

  assert.equal(requiredPins.domainRefusal.order.revision, 7);
  assert.equal(requiredPins.domainRefusal.revisionFence, 8);
  assert.equal(requiredPins.domainRefusal.phase.kind, "refused");

  assert.equal(
    requiredPins.retryableUnavailability.phase.kind,
    "unavailable",
  );
  assert.equal(
    requiredPins.retryableUnavailability.phase.request.requestId,
    "request-1",
  );

  assert.equal(
    requiredPins.completionNoticeOutsideFlow.location.url,
    urls.order,
  );
  assert.equal(
    requiredPins.completionNoticeOutsideFlow.receiptAccess,
    "offered",
  );
  assert.deepEqual(
    new ReturnDeskOracle(
      requiredPins.completionNoticeOutsideFlow,
    ).presentation().completionNotice,
    {
      returnId: "return-900",
      receipt: urls.receipt("return-900"),
    },
  );
  assert.equal(
    requiredPins.completionNoticeOutsideFlow.draft,
    null,
  );

  assert.equal(
    requiredPins.receipt.location.url,
    urls.receipt("return-900"),
  );
  assert.equal(requiredPins.receipt.receiptAccess, "acknowledged");
  assert.equal(requiredPins.receipt.completedReturn, "return-900");
});

check("checkpoint restore is silent and replay-equivalent", () => {
  assert.notEqual(canonicalCheckpoint, undefined);
  assert.deepEqual(canonicalCheckpoint.provenance, {
    scenario: "canonical",
    afterStep: 28,
    prefixDigest: canonicalPrefixDigest,
  });
  assert.match(canonicalCheckpoint.provenance.prefixDigest, /^[0-9a-f]{64}$/);
  assert.match(canonicalCheckpoint.stateHash, /^[0-9a-f]{64}$/);
  assert.equal(
    canonicalCheckpoint.state.phase.snapshot.requestId,
    "request-2",
  );
  assert.deepEqual(canonicalCheckpoint.state.settledRequests, ["request-1"]);
  assert.equal(canonicalCheckpoint.state.nextRequest, 3);
  assert.equal(canonicalCheckpoint.state.nextSurface, 2);
  assert.equal(canonicalCheckpoint.state.navigationIntent, null);
  assert.equal(canonicalCheckpoint.state.surface, null);

  const first = ReturnDeskOracle.restore(canonicalCheckpoint);
  const second = ReturnDeskOracle.restore(canonicalCheckpoint);
  assert.deepEqual(first.snapshot(), canonicalCheckpoint.state);
  assert.deepEqual(second.snapshot(), canonicalCheckpoint.state);

  const firstAcceptance = expectStep(
    first.settle("request-2", acceptance("return-900")),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  const firstLocation = expectStep(
    first.deliverLocation(urls.receipt("return-900")),
    "accepted",
  );
  const secondAcceptance = expectStep(
    second.settle("request-2", acceptance("return-900")),
    "accepted",
    [nav("replace", urls.receipt("return-900"))],
  );
  const secondLocation = expectStep(
    second.deliverLocation(urls.receipt("return-900")),
    "accepted",
  );

  assertSame(firstAcceptance, secondAcceptance, "acceptance replay");
  assertSame(firstLocation, secondLocation, "location replay");
  assertSame(first.snapshot(), second.snapshot(), "restored final state");
  assertSame(first.presentation(), second.presentation(), "restored presentation");
});

let c1IncompletePin;
check("C1: other(note) has valid empty intermediate and tagged snapshot", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order7), "accepted");
  expectStep(machine.chooseQuantity("mug", 1), "accepted");
  expectStep(machine.chooseReason("mug", other("")), "accepted");
  assert.equal(machine.presentation().itemsComplete, false);
  c1IncompletePin = machine.snapshot();

  expectStep(
    machine.chooseReason("mug", other("Does not fit the room")),
    "accepted",
  );
  assert.equal(machine.presentation().itemsComplete, true);
  expectStep(
    machine.goToStep("method"),
    "accepted",
    [nav("push", urls.method)],
  );
  expectStep(machine.deliverLocation(urls.method), "accepted");
  expectStep(machine.chooseMethod("pickup"), "accepted");
  expectStep(
    machine.goToStep("review"),
    "accepted",
    [nav("push", urls.review)],
  );
  expectStep(machine.deliverLocation(urls.review), "accepted");

  const expected = pendingSnapshot(
    "request-1",
    7,
    "mug",
    1,
    other("Does not fit the room"),
    "pickup",
  );
  expectStep(machine.submit(), "accepted", [submitCommand(expected)]);
  assert.deepEqual(
    machine.state.phase.snapshot.selections[0].reason,
    other("Does not fit the room"),
  );
});

check("C1: replacing other(note) with a base reason retains no hidden note", () => {
  const machine = new ReturnDeskOracle();
  expectStep(machine.deliverLocation(urls.items), "accepted");
  expectStep(machine.deliverOrder(order7), "accepted");
  expectStep(machine.chooseQuantity("mug", 1), "accepted");
  expectStep(machine.chooseReason("mug", other("private note")), "accepted");
  expectStep(machine.chooseReason("mug", "damaged"), "accepted");
  assert.equal(machine.state.draft.selections.mug.reason, "damaged");
  assert.equal(
    JSON.stringify(machine.state.draft.selections.mug).includes("private note"),
    false,
  );
  assert.notEqual(c1IncompletePin, undefined);
});

console.log(
  `PASS ${checks.length} validation groups: canonical 0..31, 15 adversarial scenarios, 12 static pins, checkpoint replay, and C1.`,
);
