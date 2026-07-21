import { describe, expect, it, vi } from "vitest";

import { hash } from "../protocol/machine.js";
import {
  APPLICATION_PROVIDER_ADAPTER,
  WEB_HISTORY_ADAPTER,
  type PortAdapter,
  type PortRequirement,
} from "./adapter-host.js";
import {
  admitProviderAdapterSet,
  loadUhuraAdapterProvider,
  type UhuraAdapterProvider,
  type UhuraProviderHost,
} from "./provider.js";

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

  it("disposes a provider that resolves after its Play route is aborted", async () => {
    let beginFactory = (): void => undefined;
    const factoryStarted = new Promise<void>((resolve) => {
      beginFactory = resolve;
    });
    let resolveProvider = (_provider: UhuraAdapterProvider): void => undefined;
    const providerReady = new Promise<UhuraAdapterProvider>((resolve) => {
      resolveProvider = resolve;
    });
    const globals = globalThis as typeof globalThis & {
      __uhuraDeferredProvider?: () => Promise<UhuraAdapterProvider>;
    };
    globals.__uhuraDeferredProvider = () => {
      beginFactory();
      return providerReady;
    };
    const module = `data:text/javascript,${
      encodeURIComponent(
        "export const createUhuraAdapters = () => globalThis.__uhuraDeferredProvider();",
      )
    }`;
    const abort = new AbortController();
    const disposed = vi.fn<() => void>();

    try {
      const pending = loadUhuraAdapterProvider(
        module,
        {},
        { signal: abort.signal } as UhuraProviderHost,
        [],
      );
      await factoryStarted;
      abort.abort();
      resolveProvider({ adapters: [], dispose: disposed });

      await expect(pending).rejects.toMatchObject({ name: "AbortError" });
      expect(disposed).toHaveBeenCalledOnce();
    } finally {
      delete globals.__uhuraDeferredProvider;
    }
  });
});
