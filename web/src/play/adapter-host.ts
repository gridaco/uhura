import type {
  Hash,
  Inspection,
  ReactionStep,
  ResolvedCommand,
  ResolvedInput,
  Value,
} from "../protocol/machine.js";

export interface RuntimeSession {
  /**
   * Runs exactly one admitted reaction. The native/Wasm implementation owns
   * FIFO admission, transactional publication, and receipt construction.
   */
  submit(input: ResolvedInput): ReactionStep;
  inspect(): Inspection;
}

export const WEB_HISTORY_ADAPTER = "web.history" as const;
export const APPLICATION_PROVIDER_ADAPTER = "app.provider" as const;
export const WEB_ROUTER_CONTRACT = "uhura.web_router@1::Router" as const;

export type AdapterIdentity =
  | typeof WEB_HISTORY_ADAPTER
  | typeof APPLICATION_PROVIDER_ADAPTER;

export interface PortRequirement {
  readonly port: string;
  readonly adapter: AdapterIdentity;
  readonly contractHash: Hash;
  readonly contractInstanceHash: Hash;
}

export interface AdmittedPortRequirement extends PortRequirement {
  readonly contract: string;
}

/** The browser-visible mirror of the host's sealed adapter table. */
export const assertSupportedAdapterBinding = (
  requirement: AdmittedPortRequirement,
): void => {
  const adapter: string = requirement.adapter;
  switch (adapter) {
    case WEB_HISTORY_ADAPTER:
      if (requirement.contract !== WEB_ROUTER_CONTRACT) {
        throw new TypeError(
          `Uhura adapter ${JSON.stringify(WEB_HISTORY_ADAPTER)} cannot implement ${JSON.stringify(requirement.contract)}`,
        );
      }
      return;
    case APPLICATION_PROVIDER_ADAPTER:
      return;
    default:
      throw new TypeError(
        `unknown sealed Uhura adapter ${JSON.stringify(adapter)}`,
      );
  }
};

export interface AdapterRequirementPartition {
  readonly browser: readonly AdmittedPortRequirement[];
  readonly provider: readonly AdmittedPortRequirement[];
}

/**
 * Partitions admitted ownership without guessing from a contract family.
 * `web.history` is deliberately singular in the current sealed table.
 */
export const partitionAdapterRequirements = (
  requirements: readonly AdmittedPortRequirement[],
): AdapterRequirementPartition => {
  const browser: AdmittedPortRequirement[] = [];
  const provider: AdmittedPortRequirement[] = [];
  for (const requirement of requirements) {
    assertSupportedAdapterBinding(requirement);
    if (requirement.adapter === WEB_HISTORY_ADAPTER) browser.push(requirement);
    else provider.push(requirement);
  }
  if (browser.length > 1) {
    throw new TypeError(
      `Uhura adapter ${JSON.stringify(WEB_HISTORY_ADAPTER)} may own at most one port`,
    );
  }
  return { browser, provider };
};

export interface PortAdapterContext {
  readonly signal: AbortSignal;
  /**
   * Reports one later port input. The bridge always schedules delivery; this
   * callback can never synchronously reenter a machine reaction.
   */
  deliver(value: Value): void;
}

export interface PortAdapter {
  readonly port: string;
  readonly adapter: AdapterIdentity;
  readonly contractHash: Hash;
  readonly contractInstanceHash: Hash;
  /**
   * Starts an observation or browser-capability adapter after the complete
   * admitted set exists. Deliveries are always deferred by the host queue.
   */
  start?(context: PortAdapterContext): void | Promise<void>;
  accept(
    command: Value,
    context: PortAdapterContext,
  ): void | Promise<void>;
  dispose?(): void;
}

export interface DeliveryQueue {
  enqueue(input: ResolvedInput): void;
  close(): void;
}

export type Schedule = (task: () => void) => void;

const defaultSchedule: Schedule = (task) => {
  queueMicrotask(task);
};

/**
 * A small host-boundary queue. Each drain uses a snapshot, so inputs reported
 * while a reaction publishes new commands are deferred to a later turn.
 */
export function createDeliveryQueue(
  deliver: (input: ResolvedInput) => void,
  schedule: Schedule = defaultSchedule,
): DeliveryQueue {
  let pending: ResolvedInput[] = [];
  let scheduled = false;
  let closed = false;

  const requestDrain = (): void => {
    if (scheduled || closed) return;
    scheduled = true;
    schedule(() => {
      scheduled = false;
      if (closed) return;
      const batch = pending;
      pending = [];
      for (const input of batch) deliver(input);
      if (pending.length > 0) requestDrain();
    });
  };

  return {
    enqueue(input): void {
      if (closed) {
        throw new Error("cannot deliver to a disposed Uhura adapter host");
      }
      pending.push(input);
      requestDrain();
    },
    close(): void {
      closed = true;
      pending = [];
    },
  };
}

export interface AdapterHostOptions {
  readonly requirements: readonly PortRequirement[];
  readonly adapters: readonly PortAdapter[];
  readonly deliver: (input: ResolvedInput) => void;
  readonly localCommand?: (command: Value) => void;
  readonly adapterError?: (
    error: unknown,
    port: string,
    command?: ResolvedCommand,
  ) => void;
  readonly schedule?: Schedule;
}

export interface AdapterHost {
  /** Starts every admitted adapter exactly once. */
  start(): void;
  /**
   * Offers committed commands in semantic order. Adapters may complete in any
   * order; promises are observed only for operational error reporting.
   */
  publish(commands: readonly ResolvedCommand[]): void;
  dispose(): void;
}

const portTable = (
  adapters: readonly PortAdapter[],
): ReadonlyMap<string, PortAdapter> => {
  const table = new Map<string, PortAdapter>();
  for (const adapter of adapters) {
    if (table.has(adapter.port)) {
      throw new Error(`duplicate Uhura adapter for port \`${adapter.port}\``);
    }
    table.set(adapter.port, adapter);
  }
  return table;
};

const admitAdapters = (
  requirements: readonly PortRequirement[],
  adapters: ReadonlyMap<string, PortAdapter>,
): void => {
  const required = new Set<string>();
  for (const requirement of requirements) {
    if (required.has(requirement.port)) {
      throw new Error(`duplicate Uhura port requirement \`${requirement.port}\``);
    }
    required.add(requirement.port);
    const adapter = adapters.get(requirement.port);
    if (!adapter) {
      throw new Error(`missing Uhura adapter for port \`${requirement.port}\``);
    }
    if (
      adapter.adapter !== requirement.adapter
      || adapter.contractHash !== requirement.contractHash
      || adapter.contractInstanceHash !== requirement.contractInstanceHash
    ) {
      throw new Error(
        `Uhura adapter for \`${requirement.port}\` has an incompatible admitted identity`,
      );
    }
  }
  for (const port of adapters.keys()) {
    if (!required.has(port)) {
      throw new Error(`undeclared Uhura adapter for port \`${port}\``);
    }
  }
};

/**
 * Admits a complete adapter set and creates the only bridge from committed
 * commands to foreign work. This object owns no machine semantics.
 */
export function createAdapterHost(
  options: AdapterHostOptions,
): AdapterHost {
  const adapters = portTable(options.adapters);
  admitAdapters(options.requirements, adapters);
  const abort = new AbortController();
  const deliveries = createDeliveryQueue(
    options.deliver,
    options.schedule,
  );
  let disposed = false;
  let started = false;

  const reportError = (
    error: unknown,
    port: string,
    command?: ResolvedCommand,
  ): void => {
    options.adapterError?.(error, port, command);
  };

  const contextFor = (port: string): PortAdapterContext => ({
    signal: abort.signal,
    deliver(value): void {
      deliveries.enqueue({
        source: "port",
        port,
        value,
      });
    },
  });

  return {
    start(): void {
      if (disposed) {
        throw new Error("cannot start a disposed Uhura adapter host");
      }
      if (started) return;
      started = true;
      for (const adapter of adapters.values()) {
        if (!adapter.start) continue;
        try {
          const result = adapter.start(contextFor(adapter.port));
          if (result) {
            void Promise.resolve(result).catch((error: unknown) => {
              reportError(error, adapter.port);
            });
          }
        } catch (error) {
          reportError(error, adapter.port);
        }
      }
    },
    publish(commands): void {
      if (disposed) {
        throw new Error("cannot publish through a disposed Uhura adapter host");
      }
      for (const command of commands) {
        if (command.target === "local") {
          options.localCommand?.(command.value);
          continue;
        }
        const adapter = adapters.get(command.port);
        if (!adapter) {
          throw new Error(
            `admitted Uhura adapter for \`${command.port}\` disappeared`,
          );
        }
        const context = contextFor(command.port);
        try {
          const accepted = adapter.accept(command.value, context);
          if (accepted) {
            void Promise.resolve(accepted).catch((error: unknown) => {
              reportError(error, command.port, command);
            });
          }
        } catch (error) {
          reportError(error, command.port, command);
        }
      }
    },
    dispose(): void {
      if (disposed) return;
      disposed = true;
      abort.abort();
      deliveries.close();
      for (const adapter of adapters.values()) adapter.dispose?.();
    },
  };
}
