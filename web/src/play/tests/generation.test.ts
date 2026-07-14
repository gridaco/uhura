import assert from "node:assert/strict";
import { test } from "vitest";

import { PlayGenerationGate } from "../generation.js";

test("a newer hello received before artifacts forces a reload", () => {
  const gate = new PlayGenerationGate();
  assert.deepEqual(gate.event({ generation: 12, ok: true }), { kind: "none" });
  assert.deepEqual(gate.artifacts(11), { kind: "reload" });
});

test("newer invalid source keeps the last good artifacts and shows diagnostics", () => {
  const gate = new PlayGenerationGate();
  assert.deepEqual(gate.artifacts(4), { kind: "none" });
  assert.deepEqual(
    gate.event({ generation: 5, ok: false, diagnostics: { summary: "broken" } }),
    {
      kind: "show-diagnostics",
      diagnostics: { summary: "broken" },
    },
  );
  assert.deepEqual(gate.event({ generation: 6, ok: true }), { kind: "reload" });
});

test("the artifact generation is the baseline rather than the first event", () => {
  const gate = new PlayGenerationGate();
  assert.deepEqual(gate.artifacts(8), { kind: "none" });
  assert.deepEqual(gate.event({ generation: 8, ok: true }), {
    kind: "hide-diagnostics",
  });
  assert.deepEqual(gate.event({ generation: 9, ok: true }), { kind: "reload" });
});

test("an equal failed generation exposes diagnostics for retained last-good bytes", () => {
  const gate = new PlayGenerationGate();
  assert.deepEqual(
    gate.event({ generation: 21, ok: false, diagnostics: { errors: 1 } }),
    { kind: "none" },
  );
  assert.deepEqual(gate.artifacts(21), {
    kind: "show-diagnostics",
    diagnostics: { errors: 1 },
  });
});

test("cold-invalid Play reloads after its first successful build", () => {
  const gate = new PlayGenerationGate();
  assert.deepEqual(
    gate.event({ generation: 1, ok: false, diagnostics: { errors: 1 } }),
    { kind: "none" },
  );
  assert.deepEqual(gate.unavailable(), {
    kind: "show-diagnostics",
    diagnostics: { errors: 1 },
  });
  assert.deepEqual(gate.event({ generation: 2, ok: true }), { kind: "reload" });
});
