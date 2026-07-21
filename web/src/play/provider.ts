import type {
  ProviderHost,
  SystemInfo,
} from "../protocol/types.js";
import type {
  ResolvedInput,
  Value,
} from "../protocol/machine.js";
import type { ResolveAsset } from "../renderer/assets.js";
import type {
  PortAdapter,
  PortRequirement,
} from "./adapter-host.js";
import { APPLICATION_PROVIDER_ADAPTER } from "./adapter-host.js";

/** Browser module ABI implemented by application-owned adapter providers. */
export const UHURA_ADAPTER_PROVIDER_PROTOCOL =
  "uhura-adapter-provider/0" as const;

export interface UhuraProviderHost extends ProviderHost {
  /** Exact admitted identity for one machine port owned by this deployment. */
  port(name: string): PortRequirement;
  /**
   * Decodes one browser URL through the checked route contract attached to a
   * machine port. The returned input retains the admitted port identity.
   */
  decodeRoute(port: string, url: string): ResolvedInput;
  /** Encodes a checked route-contract Location value for browser history. */
  encodeRoute(port: string, location: Value): string;
  /** Subscribes to committed Play locations owned by the application router. */
  onLocation(listener: (url: string) => void): () => void;
  /** Applies browser history without inventing a machine input. */
  navigate(mode: "push" | "replace", url: string): void;
  /** Requests one browser-history back traversal. */
  back(): void;
}

/**
 * One app-owned foreign-capability boundary. The browser admits the complete
 * adapter set against Wasm-issued contract hashes before any command leaves
 * the deterministic machine.
 */
export interface UhuraAdapterProvider {
  readonly adapters: readonly PortAdapter[];
  readonly resolveAsset?: ResolveAsset;
  systemInfo?(): SystemInfo;
  dispose?(): void;
}

export interface UhuraAdapterProviderModule {
  createUhuraAdapters(
    config: Readonly<Record<string, unknown>>,
    host: UhuraProviderHost,
  ): UhuraAdapterProvider | Promise<UhuraAdapterProvider>;
}

const providerModule = (
  value: unknown,
  module: string,
): UhuraAdapterProviderModule => {
  if (typeof value !== "object" || value === null) {
    throw new TypeError(`${module} must export createUhuraAdapters(config, host)`);
  }
  const candidate = value as Partial<UhuraAdapterProviderModule>;
  if (typeof candidate.createUhuraAdapters !== "function") {
    throw new TypeError(`${module} must export createUhuraAdapters(config, host)`);
  }
  return candidate as UhuraAdapterProviderModule;
};

const providerInstance = (
  value: unknown,
  module: string,
): UhuraAdapterProvider => {
  if (typeof value !== "object" || value === null) {
    throw new TypeError(`${module} createUhuraAdapters() must return an object`);
  }
  const candidate = value as Partial<UhuraAdapterProvider>;
  if (!Array.isArray(candidate.adapters)) {
    throw new TypeError(
      `${module} createUhuraAdapters() must return an adapters array`,
    );
  }
  return candidate as UhuraAdapterProvider;
};

const providerAbort = (): DOMException =>
  new DOMException("Uhura adapter provider loading was aborted", "AbortError");

const disposeProvider = (provider: UhuraAdapterProvider): void => {
  try {
    provider.dispose?.();
  } catch (error) {
    console.error("uhura provider cleanup failed", error);
  }
};

export const admitProviderAdapterSet = (
  provider: UhuraAdapterProvider,
  requirements: readonly PortRequirement[],
  module: string,
): void => {
  const expected = new Map<string, PortRequirement>();
  for (const requirement of requirements) {
    if (requirement.adapter !== APPLICATION_PROVIDER_ADAPTER) {
      throw new TypeError(
        `${module} was offered non-provider port \`${requirement.port}\``,
      );
    }
    if (expected.has(requirement.port)) {
      throw new TypeError(
        `${module} received duplicate port requirement \`${requirement.port}\``,
      );
    }
    expected.set(requirement.port, requirement);
  }

  const supplied = new Set<string>();
  for (const adapter of provider.adapters) {
    if (typeof adapter !== "object" || adapter === null) {
      throw new TypeError(`${module} returned a non-object Uhura adapter`);
    }
    if (adapter.adapter !== APPLICATION_PROVIDER_ADAPTER) {
      throw new TypeError(
        `${module} may return only ${JSON.stringify(APPLICATION_PROVIDER_ADAPTER)} adapters`,
      );
    }
    if (supplied.has(adapter.port)) {
      throw new TypeError(
        `${module} returned duplicate adapter for port \`${adapter.port}\``,
      );
    }
    supplied.add(adapter.port);
    const requirement = expected.get(adapter.port);
    if (!requirement) {
      throw new TypeError(
        `${module} returned undeclared provider adapter for port \`${adapter.port}\``,
      );
    }
    if (
      adapter.contractHash !== requirement.contractHash
      || adapter.contractInstanceHash !== requirement.contractInstanceHash
    ) {
      throw new TypeError(
        `${module} returned an incompatible provider adapter for port \`${adapter.port}\``,
      );
    }
    if (typeof adapter.accept !== "function") {
      throw new TypeError(
        `${module} adapter for \`${adapter.port}\` must implement accept()`,
      );
    }
  }

  for (const port of expected.keys()) {
    if (!supplied.has(port)) {
      throw new TypeError(
        `${module} omitted provider adapter for port \`${port}\``,
      );
    }
  }
};

export async function loadUhuraAdapterProvider(
  module: string,
  config: Readonly<Record<string, unknown>>,
  host: UhuraProviderHost,
  requirements: readonly PortRequirement[],
): Promise<UhuraAdapterProvider> {
  if (host.signal.aborted) throw providerAbort();
  const loaded = providerModule(
    await import(/* @vite-ignore */ module) as unknown,
    module,
  );
  if (host.signal.aborted) throw providerAbort();
  const provider = providerInstance(
    await loaded.createUhuraAdapters(config, host),
    module,
  );
  if (host.signal.aborted) {
    disposeProvider(provider);
    throw providerAbort();
  }
  try {
    admitProviderAdapterSet(provider, requirements, module);
  } catch (error) {
    disposeProvider(provider);
    throw error;
  }
  return provider;
}
