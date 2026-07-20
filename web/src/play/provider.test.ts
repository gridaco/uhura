import { describe, expect, it } from "vitest";

import { hash } from "../protocol/machine.js";
import {
  APPLICATION_PROVIDER_ADAPTER,
  WEB_HISTORY_ADAPTER,
  type PortAdapter,
  type PortRequirement,
} from "./adapter-host.js";
import { admitProviderAdapterSet } from "./provider.js";

const requirement: PortRequirement = {
  port: "authority",
  adapter: APPLICATION_PROVIDER_ADAPTER,
  contractHash: hash("1".repeat(64)),
  contractInstanceHash: hash("2".repeat(64)),
};

const adapter = (
  fields: Partial<PortAdapter> = {},
): PortAdapter => ({
  ...requirement,
  accept() {},
  ...fields,
});

describe("application adapter provider admission", () => {
  it("admits exactly the configured app.provider set", () => {
    expect(() => admitProviderAdapterSet(
      { adapters: [adapter()] },
      [requirement],
      "provider.js",
    )).not.toThrow();
  });

  it("rejects ownership substitution, missing, extra, and duplicate adapters", () => {
    expect(() => admitProviderAdapterSet(
      { adapters: [adapter({ adapter: WEB_HISTORY_ADAPTER })] },
      [requirement],
      "provider.js",
    )).toThrow(/only "app\.provider" adapters/u);
    expect(() => admitProviderAdapterSet(
      { adapters: [] },
      [requirement],
      "provider.js",
    )).toThrow(/omitted provider adapter/u);
    expect(() => admitProviderAdapterSet(
      { adapters: [adapter({ port: "extra" })] },
      [requirement],
      "provider.js",
    )).toThrow(/undeclared provider adapter/u);
    expect(() => admitProviderAdapterSet(
      { adapters: [adapter(), adapter()] },
      [requirement],
      "provider.js",
    )).toThrow(/duplicate adapter/u);
  });

  it("rejects contract or instance substitution", () => {
    expect(() => admitProviderAdapterSet(
      { adapters: [adapter({ contractHash: hash("3".repeat(64)) })] },
      [requirement],
      "provider.js",
    )).toThrow(/incompatible provider adapter/u);
    expect(() => admitProviderAdapterSet(
      {
        adapters: [adapter({
          contractInstanceHash: hash("4".repeat(64)),
        })],
      },
      [requirement],
      "provider.js",
    )).toThrow(/incompatible provider adapter/u);
  });
});
