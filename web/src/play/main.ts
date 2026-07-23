// Mount-owned Uhura Play runtime. The deterministic machine lives in Wasm;
// this browser layer owns artifacts, rendering, foreign adapters, developer
// inspection, and every effectful capability for one route lifetime.

import type {
  DevEvent,
  RuntimeHandle,
  RuntimeInspectionHandle,
  SystemInfo,
} from "../protocol/types.js";
import {
  UHURA_BROWSER_PROTOCOL,
  UHURA_RUNTIME_SNAPSHOT_PROTOCOL,
  decodeResolvedInput,
  type ResolvedInput,
  type Value,
} from "../protocol/machine.js";
import {
  decodeHostInspection,
  type HostInspection,
} from "../protocol/host-inspection.js";
import {
  createPlayAssets,
  type AssetAppliers,
} from "../renderer/assets.js";
import {
  decodeIconFontManifest,
  loadIconFontRegistry,
  type IconFontRegistry,
} from "../renderer/icons.js";
import {
  installLocationConsumer,
  publishLocation,
} from "../app/location.js";
import { routeFor } from "../app/router.js";
import {
  hostPath,
  rebasePlayAsset,
  rebaseHostResource,
  UHURA_STATIC_HOST,
} from "../app/host.js";
import { PlayGenerationGate } from "./generation.js";
import type { GenerationAction } from "./generation.js";
import { createInspectionStore } from "./inspection-store.js";
import { createOverlay } from "./overlay.js";
import {
  loadUhuraAdapterProvider,
  type UhuraAdapterProvider,
  type UhuraProviderHost,
} from "./provider.js";
import {
  partitionAdapterRequirements,
  type PortRequirement,
} from "./adapter-host.js";
import {
  admitConfiguredPorts,
  decodePortRequirements,
  decodePlayConfig,
  startPlay,
  type PlayConfig,
  type PlayController,
} from "./session.js";
import { createBrowserPortAdapters } from "./browser-adapters.js";
import {
  applicationPathForBrowser,
  browserUrlForApplication,
} from "./application-location.js";
import { createProviderHost } from "./provider-host.js";
import type { DisposableProviderHost } from "./provider-host.js";
import type { PlayShell } from "./shell.js";
import {
  SYSTEM_ACTOR_STORAGE_KEY,
  createSystemControls,
} from "./system-controls.js";

const WASM_MODULE_URL = hostPath("/api/play/wasm/uhura_wasm.js");
const STATIC_PLAY_METADATA_URL = hostPath("/api/play/static.json");
type WasmModule = typeof import("/api/play/wasm/uhura_wasm.js");
type WasmSession = InstanceType<WasmModule["Session"]>;

export const PLAY_ARTIFACT_URLS = [
  hostPath("/api/play/ir.json"),
  hostPath("/api/play/inspect.json"),
  hostPath("/api/play/config.json"),
  hostPath("/api/play/icon-fonts.json"),
  hostPath("/api/play/stylesheet.css"),
] as const;

const EXPECTED_PROTOCOLS: Readonly<Record<string, string>> = {
  browser: UHURA_BROWSER_PROTOCOL,
  checkpoint: "uhura-checkpoint/0",
  genesisReceipt: "uhura-genesis-receipt/0",
  ingressRecord: "uhura-ingress-record/0",
  ir: "uhura-ir/1",
  reactionReceipt: "uhura-reaction-receipt/0",
  runtimeSnapshot: UHURA_RUNTIME_SNAPSHOT_PROTOCOL,
  view: "uhura-view/1",
};

export function assertWasmProtocols(value: unknown): void {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new TypeError("Uhura Wasm protocols must be an object");
  }
  const spoken = value as Readonly<Record<string, unknown>>;
  const expectedKeys = Object.keys(EXPECTED_PROTOCOLS).sort();
  const spokenKeys = Object.keys(spoken).sort();
  if (
    expectedKeys.length !== spokenKeys.length
    || expectedKeys.some((key, index) => key !== spokenKeys[index])
  ) {
    throw new Error(
      `protocol set mismatch: this shell requires exactly [${expectedKeys.join(", ")}], the wasm build declares [${spokenKeys.join(", ")}] — rebuild with scripts/build-wasm.sh`,
    );
  }
  for (const [name, version] of Object.entries(EXPECTED_PROTOCOLS)) {
    if (spoken[name] !== version) {
      throw new Error(
        `protocol mismatch: this shell speaks ${name} ${version}, the wasm build speaks ${String(spoken[name])} — rebuild with scripts/build-wasm.sh`,
      );
    }
  }
}

let wasmReady: Promise<WasmModule> | undefined;

async function loadWasm(): Promise<WasmModule> {
  if (!wasmReady) {
    wasmReady = import(/* @vite-ignore */ WASM_MODULE_URL)
      .then(async (module: WasmModule) => {
        await module.default();
        return module;
      })
      .catch((error: unknown) => {
        wasmReady = undefined;
        throw error;
      });
  }
  return wasmReady;
}

export interface PlayRuntime {
  readonly inspection: RuntimeInspectionHandle;
  dispose(): void;
}

class PlayArtifactsUnavailableError extends Error {}

function assertDeploymentMatchesPlay(
  deployment: HostInspection,
  play: PlayConfig,
): void {
  if (
    deployment.identityProtocol !== play.identityProtocol
    || deployment.entry !== play.entry
    || deployment.machine !== play.machine
    || deployment.presentation !== play.presentation
    || deployment.machineProgramHash !== play.machineProgramHash
    || deployment.presentationHash !== play.presentationHash
    || deployment.evidenceHash !== play.evidenceHash
    || deployment.deploymentHash !== play.deploymentHash
  ) {
    throw new TypeError(
      "Uhura host inspection identity differs from the admitted Play deployment",
    );
  }
}

function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function release(value: unknown): void {
  if (typeof value !== "object" || value === null) return;
  const disposable = value as { dispose?: () => void; free?: () => void };
  try {
    if (typeof disposable.dispose === "function") disposable.dispose();
    else if (typeof disposable.free === "function") disposable.free();
  } catch (error) {
    console.error("uhura runtime cleanup failed", error);
  }
}

const parseJson = (source: string, context: string): unknown => {
  try {
    return JSON.parse(source) as unknown;
  } catch (error) {
    throw new TypeError(`${context} is not JSON: ${String(error)}`);
  }
};

const providerSystemInfo = (
  provider: UhuraAdapterProvider | null,
  hasProvider: boolean,
): SystemInfo => ({
  ...(provider?.systemInfo?.() ?? {}),
  hasProvider,
});

async function loadOptionalIcons(
  shell: PlayShell,
  source: string,
  generation: number,
): Promise<IconFontRegistry | undefined> {
  const value = parseJson(source, "Uhura icon-font manifest");
  if (
    typeof value !== "object"
    || value === null
    || (value as Record<string, unknown>)["protocol"] === undefined
  ) {
    return undefined;
  }
  const decoded = decodeIconFontManifest(value, "play");
  const manifest = {
    ...decoded,
    families: Object.fromEntries(
      Object.entries(decoded.families).map(([name, family]) => [
        name,
        { ...family, font: rebaseHostResource(family.font) },
      ]),
    ),
  };
  if (manifest.generation !== generation) {
    throw new Error(
      `Play icon fonts generation ${String(manifest.generation)} does not match artifact generation ${generation}`,
    );
  }
  return loadIconFontRegistry({
    document: shell.document,
    manifest,
  });
}

/** Starts Play without waiting for its network/provider boot to finish. */
export function startPlayRuntime(
  shell: PlayShell,
  applicationStyle: HTMLStyleElement,
): PlayRuntime {
  const view = shell.document.defaultView ?? window;
  const overlay = createOverlay(shell.overlayHost);
  const generations = new PlayGenerationGate();
  const inspection = createInspectionStore();
  const abort = new AbortController();
  let disposed = false;
  let eventSource: EventSource | null = null;
  let providerHost: DisposableProviderHost | null = null;
  let provider: UhuraAdapterProvider | null = null;
  let hasProvider = false;
  let playAssets: AssetAppliers | null = null;
  let play: PlayController | null = null;
  let pendingSession: WasmSession | null = null;
  let staticGeneration: Promise<number> | null = null;

  const systemControls = createSystemControls({
    target: view,
    storage: view.sessionStorage,
    reload: () => view.location.reload(),
  });
  const runtime: RuntimeHandle = {
    session: null,
    provider: null,
    inspection: inspection.handle,
    get steps() {
      return inspection.handle.state.history.map((step) => step.receipt);
    },
    get system() {
      return systemControls.state;
    },
    restart: () => systemControls.restart(),
    setActor: (actor: string) => systemControls.setActor(actor),
  };
  view.__uhura = runtime;

  const loadStaticGeneration = (): Promise<number> => {
    staticGeneration ??= fetch(STATIC_PLAY_METADATA_URL, { signal: abort.signal })
      .then(async (response) => {
        const value = await response.json() as Record<string, unknown>;
        const generation = value["playGeneration"];
        if (
          !response.ok
          || value["protocol"] !== "uhura-static-play/0"
          || typeof generation !== "number"
          || !Number.isSafeInteger(generation)
        ) {
          throw new TypeError("invalid Uhura static Play metadata");
        }
        return generation as number;
      });
    return staticGeneration;
  };

  async function fetchArtifacts<const T extends readonly string[]>(
    urls: T,
  ): Promise<{ texts: { [K in keyof T]: string }; generation: number }> {
    const pinnedGeneration = UHURA_STATIC_HOST
      ? await loadStaticGeneration()
      : null;
    const responses = await Promise.all(
      urls.map((url) => fetch(url, { signal: abort.signal })),
    );
    const texts = await Promise.all(responses.map((response) => response.text()));
    responses.forEach((response, index) => {
      if (!response.ok) {
        const message =
          `${urls[index] ?? "artifact"}: ${response.status}\n${texts[index] ?? ""}`;
        if (response.status === 503) {
          throw new PlayArtifactsUnavailableError(message);
        }
        throw new Error(message);
      }
    });
    const artifactGenerations = responses.map((response, index) => {
      const header = response.headers.get("x-uhura-generation");
      if (header === null && pinnedGeneration !== null) return pinnedGeneration;
      if (header === null || !/^\d+$/u.test(header)) {
        throw new Error(
          `${urls[index] ?? "artifact"}: missing or invalid x-uhura-generation`,
        );
      }
      const generation = Number(header);
      if (!Number.isSafeInteger(generation)) {
        throw new Error(`${urls[index] ?? "artifact"}: generation is out of range`);
      }
      return generation;
    });
    if (new Set(artifactGenerations).size > 1) {
      if (view.sessionStorage.getItem("uh-gen-retry") === null) {
        view.sessionStorage.setItem("uh-gen-retry", "1");
        view.location.reload();
        await new Promise<never>(() => {});
      }
      throw new Error("artifact fetches spanned a recheck twice — reload the page");
    }
    view.sessionStorage.removeItem("uh-gen-retry");
    const generation = artifactGenerations[0];
    if (generation === undefined) {
      throw new Error("Play has no authoritative artifacts");
    }
    return {
      texts: texts as { [K in keyof T]: string },
      generation,
    };
  }

  function applyGenerationAction(action: GenerationAction): void {
    if (disposed) return;
    switch (action.kind) {
      case "reload":
        view.location.reload();
        return;
      case "hide-diagnostics":
        overlay.hideDiagnostics();
        return;
      case "show-diagnostics":
        overlay.showDiagnostics(action.diagnostics);
        return;
      case "none":
        return;
    }
  }

  function listenForDevEvents(): void {
    const events = new EventSource(hostPath("/api/play/events"));
    eventSource = events;
    let everOpened = false;
    events.onopen = () => {
      if (disposed) return;
      if (everOpened) view.location.reload();
      everOpened = true;
    };
    events.onmessage = (message: MessageEvent<string>) => {
      if (disposed) return;
      applyGenerationAction(generations.event(JSON.parse(message.data) as DevEvent));
    };
  }

  const providerConfig = (
    config: Readonly<Record<string, unknown>>,
  ): Readonly<Record<string, unknown>> => {
    const actor =
      view.sessionStorage.getItem(SYSTEM_ACTOR_STORAGE_KEY)?.trim() || null;
    return actor === null ? config : { ...config, actor };
  };

  const adapterBoundary = (
    session: WasmSession,
    host: DisposableProviderHost,
    requirements: readonly PortRequirement[],
  ): UhuraProviderHost => {
    const ports = new Map(
      requirements.map((requirement) => [requirement.port, requirement]),
    );
    return {
      signal: host.signal,
      pickFile: (options) => host.pickFile(options),
      port(name): PortRequirement {
        const requirement = ports.get(name);
        if (!requirement) {
          throw new Error(`Uhura deployment has no admitted port \`${name}\``);
        }
        return requirement;
      },
      decodeRoute(port, url): ResolvedInput {
        if (!ports.has(port)) {
          throw new Error(
            `Uhura adapter boundary has no admitted port \`${port}\``,
          );
        }
        const input = decodeResolvedInput(
          parseJson(session.decode_route(port, url), "Uhura route input"),
          "Uhura route input",
        );
        if (input.source !== "port" || input.port !== port) {
          throw new TypeError(
            `Uhura route decoder did not produce an input for \`${port}\``,
          );
        }
        return input;
      },
      encodeRoute(port: string, location: Value): string {
        if (!ports.has(port)) {
          throw new Error(
            `Uhura adapter boundary has no admitted port \`${port}\``,
          );
        }
        return session.encode_route(port, JSON.stringify(location));
      },
      onLocation(listener): () => void {
        if (host.signal.aborted) return () => undefined;
        const stop = installLocationConsumer((change) => {
          if (change.route.surface !== "play") return;
          listener(applicationPathForBrowser(change.location));
        });
        let active = true;
        const dispose = (): void => {
          if (!active) return;
          active = false;
          host.signal.removeEventListener("abort", dispose);
          stop();
        };
        host.signal.addEventListener("abort", dispose, { once: true });
        return dispose;
      },
      navigate(mode, url): void {
        if (host.signal.aborted) {
          throw new Error(
            "cannot navigate through a disposed Uhura provider host",
          );
        }
        const destination = browserUrlForApplication(url, view.location.href);
        if (destination.origin !== view.location.origin) {
          throw new Error(
            `Uhura web history cannot navigate a different origin: ${destination.origin}`,
          );
        }
        const route = routeFor(destination.pathname);
        if (route.surface !== "play") {
          throw new Error(
            `Uhura application route ${JSON.stringify(destination.pathname)} is reserved by the host`,
          );
        }
        const href =
          `${destination.pathname}${destination.search}${destination.hash}`;
        if (mode === "replace") view.history.replaceState(null, "", href);
        else view.history.pushState(null, "", href);
        publishLocation({
          cause: mode,
          location: {
            pathname: destination.pathname,
            search: destination.search,
            hash: destination.hash,
          },
          route,
        });
      },
      back(): void {
        if (host.signal.aborted) {
          throw new Error(
            "cannot navigate through a disposed Uhura provider host",
          );
        }
        view.history.back();
      },
    };
  };

  async function boot(): Promise<void> {
    const artifacts = await fetchArtifacts(PLAY_ARTIFACT_URLS);
    const generationAction = generations.artifacts(artifacts.generation);
    applyGenerationAction(generationAction);
    if (generationAction.kind === "reload") return;
    const [
      irText,
      inspectText,
      playText,
      iconFontsText,
      styleText,
    ] = artifacts.texts;
    if (disposed) return;

    const config = decodePlayConfig(parseJson(playText, "Uhura Play config"));
    const deployment = decodeHostInspection(
      parseJson(inspectText, "Uhura host inspection"),
    );
    assertDeploymentMatchesPlay(deployment, config);

    const wasm = await loadWasm();
    if (disposed) return;
    const spoken = parseJson(wasm.protocols(), "Uhura Wasm protocols");
    assertWasmProtocols(spoken);

    hasProvider = config.provider !== null;
    systemControls.starting({
      hasProvider,
      actor: config.provider === null
        ? null
        : (view.sessionStorage.getItem(SYSTEM_ACTOR_STORAGE_KEY)?.trim() || null),
      actors: [],
    });

    const session = new wasm.Session(
      irText,
      config.machine,
      JSON.stringify(config.configuration),
      config.instance,
      config.presentation ?? undefined,
      JSON.stringify({
        identityProtocol: config.identityProtocol,
        machineProgramHash: config.machineProgramHash,
        presentationHash: config.presentationHash,
      }),
    );
    pendingSession = session;
    runtime.session = session;
    providerHost = createProviderHost(abort.signal);
    const portRequirements = decodePortRequirements(
      session.port_requirements(),
    );
    const admittedRequirements = admitConfiguredPorts(portRequirements, config);
    const requirements = partitionAdapterRequirements(admittedRequirements);
    const browserBoundary = adapterBoundary(
      session,
      providerHost,
      requirements.browser,
    );
    const browserAdapters = createBrowserPortAdapters(
      requirements.browser,
      browserBoundary,
    );
    if (config.provider !== null) {
      const boundary = adapterBoundary(
        session,
        providerHost,
        requirements.provider,
      );
      try {
        const loadedProvider = await loadUhuraAdapterProvider(
          rebaseHostResource(config.provider.module),
          providerConfig(config.provider.config),
          boundary,
          requirements.provider,
        );
        if (disposed) {
          release(loadedProvider);
          return;
        }
        provider = loadedProvider;
        view.sessionStorage.removeItem("uh-provider-retry");
      } catch (error) {
        if (
          !disposed
          && view.sessionStorage.getItem("uh-provider-retry") === null
        ) {
          view.sessionStorage.setItem("uh-provider-retry", "1");
          view.location.reload();
          await new Promise<never>(() => {});
        }
        throw error;
      }
    }
    if (disposed) return;

    const icons = await loadOptionalIcons(
      shell,
      iconFontsText,
      artifacts.generation,
    );
    if (disposed) return;
    applicationStyle.textContent = styleText;
    const providerAsset = provider?.resolveAsset?.bind(provider);
    const resolveAsset = providerAsset === undefined
      ? undefined
      : async (asset: string): Promise<string> =>
        rebasePlayAsset(await providerAsset(asset));
    playAssets = createPlayAssets(resolveAsset);
    inspection.installArtifacts({
      generation: artifacts.generation,
      deployment,
    });
    play = startPlay({
      shell,
      session,
      config,
      adapters: [...browserAdapters, ...(provider?.adapters ?? [])],
      assets: playAssets,
      icons,
      resolveLinkHref(href): string {
        const destination = browserUrlForApplication(href, view.location.href);
        return `${destination.pathname}${destination.search}${destination.hash}`;
      },
      publishRuntimeStep(snapshot, receipt): void {
        inspection.record(snapshot, receipt);
      },
      onProjectionError(error): void {
        if (disposed) return;
        console.error("Uhura presentation projection failed; machine continues", error);
      },
      onError(error): void {
        if (disposed) return;
        console.error("Uhura Play failed", error);
        overlay.showFatal(String(error));
      },
    });
    pendingSession = null;
    runtime.provider = provider;
    if (disposed) return;
    systemControls.ready(providerSystemInfo(provider, hasProvider));
  }

  if (!UHURA_STATIC_HOST) {
    try {
      listenForDevEvents();
    } catch (error) {
      systemControls.failed(error);
      overlay.showFatal(`failed to open the Play event stream: ${String(error)}`);
    }
  }

  void boot().catch((error: unknown) => {
    if (disposed || isAbort(error)) return;
    systemControls.failed(
      error,
      providerSystemInfo(provider, hasProvider),
    );
    if (error instanceof PlayArtifactsUnavailableError) {
      const action = generations.unavailable();
      applyGenerationAction(action);
      if (action.kind !== "none") return;
    }
    overlay.showFatal(String(error));
  });

  return {
    inspection: inspection.handle,
    dispose(): void {
      if (disposed) return;
      disposed = true;
      abort.abort();
      if (eventSource) {
        eventSource.onopen = null;
        eventSource.onmessage = null;
        eventSource.close();
        eventSource = null;
      }
      play?.dispose();
      play = null;
      if (pendingSession) release(pendingSession);
      pendingSession = null;
      playAssets?.dispose?.();
      playAssets = null;
      release(provider);
      provider = null;
      providerHost?.dispose();
      providerHost = null;
      inspection.dispose();
      runtime.provider = null;
      runtime.session = null;
      applicationStyle.textContent = "";
      overlay.hide();
      if (view.__uhura === runtime) delete view.__uhura;
    },
  };
}
