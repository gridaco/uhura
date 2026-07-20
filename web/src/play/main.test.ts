import { describe, expect, it } from "vitest";

import { assertWasmProtocols } from "./main.js";

const protocols = {
  browser: "uhura-browser/3",
  checkpoint: "uhura-checkpoint/0",
  genesisReceipt: "uhura-genesis-receipt/0",
  ingressRecord: "uhura-ingress-record/0",
  ir: "uhura-ir/1",
  reactionReceipt: "uhura-reaction-receipt/0",
  runtimeSnapshot: "uhura-runtime-snapshot/0",
  view: "uhura-view/1",
} as const;

describe("Uhura Wasm protocol admission", () => {
  it("accepts the one complete protocol set", () => {
    expect(() => assertWasmProtocols(protocols)).not.toThrow();
  });

  it("rejects missing, extra, and drifted protocol declarations", () => {
    const missing: Record<string, string> = { ...protocols };
    delete missing["view"];
    expect(() => assertWasmProtocols(missing)).toThrow(/protocol set mismatch/u);
    expect(() =>
      assertWasmProtocols({ ...protocols, experimental: "example/0" })
    ).toThrow(/protocol set mismatch/u);
    expect(() =>
      assertWasmProtocols({ ...protocols, browser: "uhura-browser/1" })
    ).toThrow(/protocol mismatch/u);
  });
});
