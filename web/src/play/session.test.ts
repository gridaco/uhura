import { describe, expect, it } from "vitest";

import {
  UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
  UHURA_SEMANTIC_IR_HASH_PROTOCOL,
  decodeValue,
} from "../protocol/machine.js";
import {
  admitConfiguredPorts,
  decodePlayConfig,
  decodePlayStep,
  startPlay,
} from "./session.js";
import { UHURA_ADAPTER_PROVIDER_PROTOCOL } from "./provider.js";
import {
  APPLICATION_PROVIDER_ADAPTER,
  WEB_HISTORY_ADAPTER,
  WEB_ROUTER_CONTRACT,
  type PortAdapter,
} from "./adapter-host.js";

const hash = "11".repeat(32);
const configurationHash = "22".repeat(32);
const stateHash = "33".repeat(32);
const nextStateHash = "44".repeat(32);

const config = {
  protocol: "uhura-play-config/1",
  identityProtocol: UHURA_SEMANTIC_IR_HASH_PROTOCOL,
  entry: "app",
  machine: "example.app@1::App",
  presentation: "example.app@1::Web",
  machineProgramHash: hash,
  presentationHash: hash,
  evidenceHash: null,
  deploymentHash: hash,
  lifetime: "application-session",
  instance: "entry/app",
  configuration: { $: "unit" },
  ports: [],
} as const;

const observation = (count: string) => ({
  $: "record",
  fields: [{ name: "count", value: { $: "Int", value: count } }],
});

const command = {
  target: "local",
  value: {
    $: "variant",
    type: "example.app@1::App.Command",
    case: "reported",
    fields: [],
  },
};

const reaction = {
  protocol: "uhura-reaction-receipt/0",
  kind: "reaction",
  instance: config.instance,
  machineProgramHash: hash,
  configurationHash,
  sequence: "1",
  input: {
    source: "local",
    value: {
      $: "variant",
      type: "example.app@1::App.Input",
      case: "increment",
      fields: [],
    },
  },
  resolution: {
    kind: "completed",
    outcome: {
      $: "variant",
      type: "example.app@1::App.Outcome",
      case: "accepted",
      fields: [],
    },
    disposition: "commit",
  },
  orderedCommands: [command],
  postObservation: observation("1"),
  preStateHash: stateHash,
  postStateHash: nextStateHash,
};

const step = {
  protocol: "uhura-browser/3",
  receipt: reaction,
  snapshot: {
    protocol: "uhura-runtime-snapshot/0",
    instance: config.instance,
    machineProgramHash: hash,
    presentation: config.presentation,
    presentationHash: hash,
    configurationHash,
    state: observation("1"),
    stateHash: nextStateHash,
    lifecycle: "running",
    nextSequence: "2",
    tracePrefixHash: "66".repeat(32),
    ingressPrefixHash: "77".repeat(32),
    nextIngressOrdinal: "1",
  },
  presentation: {
    kind: "view",
    projectionRevision: "1",
    view: {
      protocol: "uhura-view/1",
      presentation: config.presentation,
      machine: config.machine,
      instance: config.instance,
      sequence: "1",
      nodes: [],
    },
  },
};

const clone = <T>(value: T): T => structuredClone(value);

describe("Uhura Play config", () => {
  it("admits only the two language-owned identity protocols", () => {
    expect(decodePlayConfig(config).identityProtocol)
      .toBe(UHURA_SEMANTIC_IR_HASH_PROTOCOL);
    expect(
      decodePlayConfig({
        ...config,
        identityProtocol: UHURA_MACHINE_PROGRAM_ID_PROTOCOL,
      }).identityProtocol,
    ).toBe(UHURA_MACHINE_PROGRAM_ID_PROTOCOL);
    expect(() =>
      decodePlayConfig({
        ...config,
        identityProtocol: "uhura-unrecognized-identity/9",
      })
    ).toThrow(/identityProtocol must be/u);
  });

  it("admits generic provider metadata and exact port identities", () => {
    const decoded = decodePlayConfig({
      ...config,
      ports: [{
        port: "authority",
        adapter: APPLICATION_PROVIDER_ADAPTER,
        contractHash: hash,
        contractInstanceHash: hash,
      }],
      provider: {
        protocol: UHURA_ADAPTER_PROVIDER_PROTOCOL,
        module: "/api/play/provider.js",
        config: { actor: "demo" },
      },
    });
    expect(decoded.ports[0]?.contractInstanceHash).toBe(hash);
    expect(decoded.ports[0]?.adapter).toBe(APPLICATION_PROVIDER_ADAPTER);
    expect(decoded.provider?.protocol).toBe(UHURA_ADAPTER_PROVIDER_PROTOCOL);
    expect(decoded.provider?.module).toBe("/api/play/provider.js");
    expect(decoded.provider?.config).toEqual({ actor: "demo" });
  });

  it("rejects a provider module with an unknown adapter ABI", () => {
    expect(() =>
      decodePlayConfig({
        ...config,
        provider: {
          protocol: "uhura-adapter-provider/9",
          module: "/api/play/provider.js",
          config: {},
        },
      })
    ).toThrow(/provider\.protocol/u);
  });

  it("has no runtime discriminator and rejects unsealed adapter names", () => {
    expect(() =>
      decodePlayConfig({
        ...config,
        runtime: "other",
      })
    ).toThrow(/wrong fields/u);
    expect(() =>
      decodePlayConfig({
        ...config,
        ports: [{
          port: "orders",
          adapter: "return-desk.orders",
          contractHash: hash,
          contractInstanceHash: hash,
        }],
      })
    ).toThrow(/sealed Uhura adapter table/u);
  });

  it("requires provider metadata exactly when app.provider owns a port", () => {
    expect(() => decodePlayConfig({
      ...config,
      ports: [{
        port: "authority",
        adapter: APPLICATION_PROVIDER_ADAPTER,
        contractHash: hash,
        contractInstanceHash: hash,
      }],
    })).toThrow(/has no provider module/u);
    expect(() => decodePlayConfig({
      ...config,
      provider: {
        protocol: UHURA_ADAPTER_PROVIDER_PROTOCOL,
        module: "/api/play/provider.js",
        config: {},
      },
    })).toThrow(/binds no app\.provider ports/u);
  });

  it("merges core contracts with exact host adapter ownership", () => {
    const play = decodePlayConfig({
      ...config,
      ports: [{
        port: "router",
        adapter: WEB_HISTORY_ADAPTER,
        contractHash: hash,
        contractInstanceHash: hash,
      }],
    });
    const admitted = admitConfiguredPorts([{
      port: "router",
      contract: WEB_ROUTER_CONTRACT,
      contractHash: play.ports[0]!.contractHash,
      contractInstanceHash: play.ports[0]!.contractInstanceHash,
    }], play);
    expect(admitted).toEqual([{
      port: "router",
      adapter: WEB_HISTORY_ADAPTER,
      contract: WEB_ROUTER_CONTRACT,
      contractHash: hash,
      contractInstanceHash: hash,
    }]);
  });

  it("requires a presentation and its identity together", () => {
    expect(() =>
      decodePlayConfig({
        ...config,
        presentationHash: null,
      })
    ).toThrow(/must either both be null or both be present/u);
  });
});

describe("Uhura browser-step admission", () => {
  const play = decodePlayConfig(config);

  it("correlates receipt, bounded snapshot, derived values, and view", () => {
    const decoded = decodePlayStep(
      JSON.stringify(step),
      play,
    );
    expect(decoded.receipt.sequence).toBe("1");
    expect(decoded.commands).toEqual(decoded.receipt.orderedCommands);
    expect(decoded.observation).toEqual(decoded.receipt.postObservation);
    expect(decoded.snapshot.nextSequence).toBe("2");
    expect(decoded.presentation.kind).toBe("view");
    if (decoded.presentation.kind !== "view") throw new Error("expected view");
    expect(decoded.presentation.projectionRevision).toBe("1");
    expect(decoded.presentation.view.sequence).toBe(decoded.receipt.sequence);
  });

  it("rejects a stale or unrelated view", () => {
    const invalid = clone(step);
    invalid.presentation.view.sequence = "0";
    expect(() =>
      decodePlayStep(
        JSON.stringify(invalid),
        play,
      )
    ).toThrow(/view identity or sequence/u);
  });

  it("rejects a live view without a projection revision before dispatch", () => {
    const invalid = clone(step);
    delete (invalid.presentation as {
      projectionRevision?: string;
    }).projectionRevision;
    expect(() =>
      decodePlayStep(
        JSON.stringify(invalid),
        play,
      )
    ).toThrow(/wrong fields/u);
  });

  it("requires the live projection revision to be canonical natural text", () => {
    for (const projectionRevision of [undefined, 1, "01", "-1"]) {
      const invalid = clone(step) as {
        presentation: {
          projectionRevision: unknown;
        };
      };
      invalid.presentation.projectionRevision = projectionRevision;
      expect(() =>
        decodePlayStep(
          JSON.stringify(invalid),
          play,
        )
      ).toThrow(
        projectionRevision === undefined
          ? /wrong fields/u
          : projectionRevision === 1
            ? /projectionRevision must be nonempty text/u
            : /Uhura Nat must use canonical exact text/u,
      );
    }
  });

  it("admits a correlated projection error without losing committed commands", () => {
    const failed = clone(step) as Record<string, unknown>;
    failed["presentation"] = {
      kind: "error",
      error: {
        code: "projection-failed",
        message: "one projection contains duplicate Surface keys",
        machine: config.machine,
        presentation: config.presentation,
        instance: config.instance,
        sequence: reaction.sequence,
      },
    };
    const decoded = decodePlayStep(
      JSON.stringify(failed),
      play,
    );
    expect(decoded.presentation.kind).toBe("error");
    expect(decoded.commands).toEqual(decoded.receipt.orderedCommands);
  });

  it("rejects an uncorrelated projection error", () => {
    const failed = clone(step) as Record<string, unknown>;
    failed["presentation"] = {
      kind: "error",
      error: {
        code: "projection-failed",
        message: "one projection contains duplicate Surface keys",
        machine: config.machine,
        presentation: config.presentation,
        instance: config.instance,
        sequence: "0",
      },
    };
    expect(() =>
      decodePlayStep(
        JSON.stringify(failed),
        play,
      )
    ).toThrow(/projection error identity or sequence/u);
  });

  it("rejects receipt protocol drift", () => {
    const invalid = clone(step);
    invalid.receipt.protocol = "uhura-reaction-receipt/9";
    expect(() =>
      decodePlayStep(
        JSON.stringify(invalid),
        play,
      )
    ).toThrow(/reaction-receipt\/0/u);
  });

  it("rejects duplicate command transport outside the committed receipt", () => {
    const invalid = clone(step);
    Object.assign(invalid, { commands: [] });
    expect(() =>
      decodePlayStep(
        JSON.stringify(invalid),
        play,
      )
    ).toThrow(/wrong fields/u);
  });

  it("rejects a snapshot that does not follow the committed receipt", () => {
    const invalid = clone(step);
    invalid.snapshot.nextSequence = "1";
    expect(() =>
      decodePlayStep(
        JSON.stringify(invalid),
        play,
      )
    ).toThrow(/nextSequence/u);
  });

  it("rejects a snapshot with an unrelated committed state identity", () => {
    const invalid = clone(step);
    invalid.snapshot.stateHash = stateHash;
    expect(() =>
      decodePlayStep(
        JSON.stringify(invalid),
        play,
      )
    ).toThrow(/state identity/u);
  });
});

describe("Uhura Play reaction inspection", () => {
  it("does not request cumulative inspection on the reaction hot path", async () => {
    const deliveryCount = 6;
    const port = "events";
    const play = decodePlayConfig({
      ...config,
      presentation: null,
      presentationHash: null,
      ports: [{
        port,
        adapter: APPLICATION_PROVIDER_ADAPTER,
        contractHash: hash,
        contractInstanceHash: hash,
      }],
      provider: {
        protocol: UHURA_ADAPTER_PROVIDER_PROTOCOL,
        module: "/api/play/provider.js",
        config: {},
      },
    });
    const genesis = {
      protocol: "uhura-genesis-receipt/0",
      kind: "genesis",
      instance: play.instance,
      machineProgramHash: hash,
      configurationHash,
      sequence: "0",
      initialObservation: observation("0"),
      initialStateHash: stateHash,
    };
    const inspection = {
      protocol: "uhura-browser/3",
      identityProtocol: play.identityProtocol,
      instance: play.instance,
      machineProgramHash: hash,
      presentation: null,
      presentationHash: null,
      configurationHash,
      configuration: play.configuration,
      state: observation("0"),
      observation: observation("0"),
      inbox: [],
      lifecycle: "running",
      nextSequence: "1",
      tracePrefixHash: "66".repeat(32),
      receipts: [genesis],
      ingressPrefixHash: "77".repeat(32),
      nextIngressOrdinal: "1",
      ingressRecords: [],
    };
    let inspectCalls = 0;
    let submitCalls = 0;
    let freed = false;
    const session = {
      inspect(): string {
        inspectCalls += 1;
        return JSON.stringify(inspection);
      },
      port_requirements(): string {
        return JSON.stringify([{
          port,
          contract: "example.events@1::Events",
          contractHash: hash,
          contractInstanceHash: hash,
        }]);
      },
      presentation(): string {
        return JSON.stringify({ kind: "none" });
      },
      submit(inputSource: string): string {
        submitCalls += 1;
        const sequence = String(submitCalls);
        const input = JSON.parse(inputSource) as unknown;
        const receipt = {
          protocol: "uhura-reaction-receipt/0",
          kind: "reaction",
          instance: play.instance,
          machineProgramHash: hash,
          configurationHash,
          sequence,
          input,
          resolution: {
            kind: "completed",
            outcome: {
              $: "variant",
              type: "example.app@1::App.Outcome",
              case: "accepted",
              fields: [],
            },
            disposition: "commit",
          },
          orderedCommands: [],
          postObservation: observation(sequence),
          preStateHash: nextStateHash,
          postStateHash: nextStateHash,
        };
        return JSON.stringify({
          protocol: "uhura-browser/3",
          receipt,
          snapshot: {
            protocol: "uhura-runtime-snapshot/0",
            instance: play.instance,
            machineProgramHash: hash,
            presentation: null,
            presentationHash: null,
            configurationHash,
            state: observation(sequence),
            stateHash: nextStateHash,
            lifecycle: "running",
            nextSequence: String(submitCalls + 1),
            tracePrefixHash: "66".repeat(32),
            ingressPrefixHash: "77".repeat(32),
            nextIngressOrdinal: "1",
          },
          presentation: { kind: "none" },
        });
      },
      free(): void {
        freed = true;
      },
    } as unknown as Parameters<typeof startPlay>[0]["session"];
    const adapter = {
      port,
      adapter: APPLICATION_PROVIDER_ADAPTER,
      contractHash: play.ports[0]!.contractHash,
      contractInstanceHash: play.ports[0]!.contractInstanceHash,
      start(context): void {
        for (let index = 1; index <= deliveryCount; index += 1) {
          context.deliver(decodeValue({
            $: "variant",
            type: "example.events@1::Events.Input",
            case: "tick",
            fields: [{
              name: "count",
              value: { $: "Nat", value: String(index) },
            }],
          }, "test deferred provider input"));
        }
      },
      accept(): void {},
    } satisfies PortAdapter;
    const published: string[] = [];
    const errors: unknown[] = [];
    const controller = startPlay({
      shell: {} as Parameters<typeof startPlay>[0]["shell"],
      session,
      config: play,
      adapters: [adapter],
      publishRuntimeStep(_snapshot, receipt): void {
        published.push(receipt.sequence);
      },
      onError(error): void {
        errors.push(error);
      },
    });

    await new Promise<void>((resolve) => queueMicrotask(resolve));

    expect(errors).toEqual([]);
    expect(submitCalls).toBe(deliveryCount);
    expect(published).toEqual(["0", "1", "2", "3", "4", "5", "6"]);
    expect(inspectCalls).toBe(1);

    controller.dispose();
    expect(freed).toBe(true);
  });
});
