import assert from "node:assert/strict";
import test from "node:test";

import { selectPlayProvider } from "../play-provider-selection.js";

const remote = {
  provider: { kind: "module", module: "/provider.js", config: {} },
};

test("module play profiles keep the backwards-compatible fixture switch by default", () => {
  assert.deepEqual(selectPlayProvider(remote, "fixture"), {
    provider: "fixture",
    providers: ["remote", "fixture"],
    clearStoredProvider: false,
  });
});

test("remote-only play profiles reject and clear a stale Fixture selection", () => {
  assert.deepEqual(
    selectPlayProvider({ ...remote, allow_fixture: false }, "fixture"),
    {
      provider: "remote",
      providers: ["remote"],
      clearStoredProvider: true,
    },
  );
});

test("fixture-only profiles cannot select a stale Remote provider", () => {
  assert.deepEqual(
    selectPlayProvider({ provider: { kind: "fixture" } }, "remote"),
    {
      provider: "fixture",
      providers: ["fixture"],
      clearStoredProvider: true,
    },
  );
});

test("invalid stored provider values are cleared", () => {
  assert.equal(selectPlayProvider(remote, "other").clearStoredProvider, true);
});
