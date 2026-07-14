// Mount-owned Uhura Play runtime. Boot remains asynchronous, but every timer,
// stream, global handle, focus task, surface listener, and browser capability
// belongs to one route lifetime and is retired by dispose().

import type {
  Descriptor,
  DevEvent,
  Driver,
  InspectProgram,
  InspectSnapshot,
  InspectionHandle,
  PlayConfig,
  ProviderMode,
  ProviderModule,
  RemoteDriver,
  RemoteSystemInfo,
  RuntimeHandle,
  Snapshot,
  StepResult,
} from "../protocol/types.js";
import type { ResolveAsset } from "../renderer/play.js";
import type { AssetAppliers } from "../renderer/play.js";
import type { IconTable } from "../renderer/play.js";
import {
  createPlayAssets,
  createPlayRenderer,
  findScope,
} from "../renderer/play.js";
import { createFocusController } from "./focus.js";
import { PlayGenerationGate } from "./generation.js";
import type { GenerationAction } from "./generation.js";
import { createInspectionStore } from "./inspection-store.js";
import { createOverlay } from "./overlay.js";
import { selectPlayProvider } from "./play-provider-selection.js";
import { createPump, providerMsgToEvent } from "./pump.js";
import { createProviderHost } from "./provider-host.js";
import type { DisposableProviderHost } from "./provider-host.js";
import { createScrolls } from "./scroll.js";
import type { PlayShell } from "./shell.js";
import { createSurfaces } from "./surfaces.js";
import type { SurfaceController } from "./surfaces.js";
import {
  SYSTEM_ACTOR_STORAGE_KEY,
  SYSTEM_PROVIDER_STORAGE_KEY,
  createSystemControls,
} from "./system-controls.js";
import { createTextFields } from "./textfield.js";
import { createTicks, DEFAULT_TICK_MS } from "./ticks.js";

const WASM_MODULE_URL = "/api/play/wasm/uhura_wasm.js";
type WasmModule = typeof import("/api/play/wasm/uhura_wasm.js");

export const PLAY_ARTIFACT_URLS = [
  "/api/play/ir.json",
  "/api/play/inspect.json",
  "/api/play/boot.json",
  "/api/play/fixture.json",
  "/api/play/script.json",
  "/api/play/icons.json",
  "/api/play/config.json",
  "/api/play/stylesheet.css",
] as const;

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
  readonly inspection: InspectionHandle;
  dispose(): void;
}

class PlayArtifactsUnavailableError extends Error {}

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
  let ticks: ReturnType<typeof createTicks> | null = null;
  let surfaces: SurfaceController | null = null;
  let focus: ReturnType<typeof createFocusController> | null = null;
  let scrolls: ReturnType<typeof createScrolls> | null = null;
  let providerHost: DisposableProviderHost | null = null;
  let playAssets: AssetAppliers | null = null;

  const systemControls = createSystemControls({
    target: view,
    storage: view.sessionStorage,
    reload: () => view.location.reload(),
  });
  const runtime: RuntimeHandle = {
    session: null,
    driver: null,
    inspection: inspection.handle,
    get steps() {
      return inspection.handle.state.history.map((step) => step.trace);
    },
    ticks: null,
    get system() {
      return systemControls.state;
    },
    restart: () => systemControls.restart(),
    setActor: (actor: string) => systemControls.setActor(actor),
    setProvider: (provider: ProviderMode) => systemControls.setProvider(provider),
  };
  view.__uhura = runtime;

  function currentRemoteSystemInfo(): RemoteSystemInfo | undefined {
    const driver = runtime.driver as RemoteDriver | null;
    if (!driver || typeof driver.systemInfo !== "function") return undefined;
    try {
      return driver.systemInfo();
    } catch (error) {
      console.error("uhura provider system metadata failed", error);
      return undefined;
    }
  }

  async function importProvider(module: string): Promise<ProviderModule> {
    try {
      const loaded = (await import(/* @vite-ignore */ module)) as ProviderModule;
      view.sessionStorage.removeItem("uh-provider-retry");
      return loaded;
    } catch (error) {
      if (disposed) throw error;
      if (view.sessionStorage.getItem("uh-provider-retry") === null) {
        view.sessionStorage.setItem("uh-provider-retry", "1");
        view.location.reload();
        await new Promise<never>(() => {});
      }
      throw error;
    }
  }

  async function fetchArtifacts<const T extends readonly string[]>(
    urls: T,
  ): Promise<{ texts: { [K in keyof T]: string }; generation: number }> {
    const responses = await Promise.all(
      urls.map((url) => fetch(url, { signal: abort.signal })),
    );
    const texts = await Promise.all(responses.map((response) => response.text()));
    responses.forEach((response, index) => {
      if (!response.ok) {
        const message =
          `${urls[index] ?? "artifact"}: ${response.status}\n${texts[index] ?? ""}`;
        if (response.status === 503) throw new PlayArtifactsUnavailableError(message);
        throw new Error(message);
      }
    });
    const artifactGenerations = responses.map((response, index) => {
      const header = response.headers.get("x-uhura-generation");
      if (header === null || !/^\d+$/.test(header)) {
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
    const distinctGenerations = new Set(artifactGenerations);
    if (distinctGenerations.size > 1) {
      if (view.sessionStorage.getItem("uh-gen-retry") === null) {
        view.sessionStorage.setItem("uh-gen-retry", "1");
        view.location.reload();
        await new Promise<never>(() => {});
      }
      throw new Error("artifact fetches spanned a recheck twice — reload the page");
    }
    view.sessionStorage.removeItem("uh-gen-retry");
    const generation = artifactGenerations[0];
    if (generation === undefined) throw new Error("Play has no authoritative artifacts");
    return { texts: texts as { [K in keyof T]: string }, generation };
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
    const events = new EventSource("/api/play/events");
    eventSource = events;
    let everOpened = false;
    events.onopen = () => {
      if (disposed) return;
      if (everOpened) view.location.reload();
      everOpened = true;
    };
    events.onmessage = (message: MessageEvent<string>) => {
      if (disposed) return;
      const dev = JSON.parse(message.data) as DevEvent;
      applyGenerationAction(generations.event(dev));
    };
  }

  async function boot(): Promise<void> {
    const artifacts = await fetchArtifacts(PLAY_ARTIFACT_URLS);
    const generationAction = generations.artifacts(artifacts.generation);
    applyGenerationAction(generationAction);
    if (generationAction.kind === "reload") return;
    const [
      irText,
      inspectText,
      bootText,
      fixtureText,
      scriptText,
      iconsText,
      playText,
      styleText,
    ] = artifacts.texts;
    if (disposed) return;
    inspection.installArtifacts({
      generation: artifacts.generation,
      program: JSON.parse(inspectText) as InspectProgram,
    });
    applicationStyle.textContent = styleText;

    const wasm = await loadWasm();
    if (disposed) return;
    const { FixtureDriver, Session, protocols } = wasm;

    const spoken = JSON.parse(protocols()) as Record<string, unknown>;
    const expected: Record<string, string> = {
      inspect: "uhura-inspect/0",
      ir: "uhura-ir/0",
      view: "uhura-view/0",
      provider: "uhura-provider/0",
    };
    for (const [name, version] of Object.entries(expected)) {
      if (spoken[name] !== version) {
        throw new Error(
          `protocol mismatch: this shell speaks ${name} ${version}, the wasm build speaks ${spoken[name]} — rebuild with scripts/build-wasm.sh`,
        );
      }
    }

    const play = JSON.parse(playText) as PlayConfig;
    const storedProvider = view.sessionStorage.getItem(SYSTEM_PROVIDER_STORAGE_KEY);
    const selection = selectPlayProvider(play, storedProvider);
    if (selection.clearStoredProvider) {
      view.sessionStorage.removeItem(SYSTEM_PROVIDER_STORAGE_KEY);
    }
    const inferredProvider = selection.provider;
    const configuredActor =
      play.provider.kind === "module" ? play.provider.config.actor ?? null : null;
    const storedActor =
      view.sessionStorage.getItem(SYSTEM_ACTOR_STORAGE_KEY)?.trim() || null;
    const selectedActor = storedActor ?? configuredActor;
    systemControls.starting({
      provider: inferredProvider,
      providers: selection.providers,
      actor: inferredProvider === "remote" ? selectedActor : null,
      actors: [],
    });

    const session = new Session(irText);
    runtime.session = session;
    let driver: Driver;
    let resolveAsset: ResolveAsset | undefined;
    if (inferredProvider === "remote") {
      if (play.provider.kind !== "module") {
        throw new Error("remote play was selected without a provider module");
      }
      const providerModule = await importProvider(play.provider.module);
      if (disposed) return;
      if (typeof providerModule.createDriver !== "function") {
        throw new Error(`${play.provider.module} must export createDriver(config, host)`);
      }
      const config = { ...play.provider.config };
      if (selectedActor !== null) config.actor = selectedActor;
      providerHost = createProviderHost(abort.signal);
      const remote = providerModule.createDriver(config, providerHost);
      runtime.driver = remote;
      const remoteBoot = await remote.assembleBoot();
      if (disposed) return;
      session.boot(remoteBoot);
      driver = remote;
      if (typeof remote.resolveAsset === "function") {
        resolveAsset = remote.resolveAsset.bind(remote);
      }
    } else {
      session.boot(bootText);
      driver = new FixtureDriver(fixtureText, scriptText);
      runtime.driver = driver;
    }

    const icons = JSON.parse(iconsText) as IconTable;
    let currentRevision = 0;
    let currentNavKey: string | null = null;
    let pageElement: HTMLElement | null = null;
    let nextNavToken = 1;
    const navFrames: { params: string; token: number }[] = [
      { params: "{}", token: 0 },
    ];
    let pump: ReturnType<typeof createPump>;

    function emit(
      descriptor: Descriptor,
      data?: Record<string, unknown>,
      onApplied?: () => void,
    ): void {
      if (disposed) return;
      const event: Record<string, unknown> = {
        kind: "ui",
        descriptor,
        "view-rev": currentRevision,
      };
      if (data) event["data"] = data;
      pump.enqueue(JSON.stringify(event), onApplied);
    }

    const textFields = createTextFields({ emit });
    scrolls = createScrolls({ emit });
    const assets = createPlayAssets(resolveAsset);
    playAssets = assets;
    const renderer = createPlayRenderer({
      document: shell.document,
      emit,
      icons,
      assets,
      textFields,
      scrolls,
    });
    focus = createFocusController(shell.container);
    surfaces = createSurfaces({
      host: shell.surfaceHost,
      pageHost: shell.pageHost,
      emit,
      reconcileChildren: renderer.reconcileChildren,
      disposeSubtree: renderer.disposeSubtree,
      enterSurface: focus.enterSurface,
    });

    function renderPage(snapshot: Snapshot): void {
      const scope = findScope(snapshot.page.root) ?? "page";
      const topFrame = navFrames.at(-1) ?? { params: "{}", token: -1 };
      const navKey =
        `${snapshot.page.route}|${navFrames.length}|${topFrame.params}|${topFrame.token}`;
      if (!pageElement || currentNavKey !== navKey) {
        if (pageElement && currentNavKey !== null) {
          scrolls?.savePositions(currentNavKey, pageElement);
        }
        if (pageElement) renderer.disposeSubtree(pageElement);
        shell.pageHost.replaceChildren();
        pageElement = shell.document.createElement("div");
        pageElement.className = "uh-page-root";
        shell.pageHost.append(pageElement);
        renderer.reconcileChildren(pageElement, [snapshot.page.root], scope, false);
        scrolls?.restorePositions(navKey, pageElement);
        currentNavKey = navKey;
      } else {
        renderer.reconcileChildren(pageElement, [snapshot.page.root], scope, false);
      }
    }

    function onStep(result: StepResult): void {
      if (disposed) return;
      currentRevision = result.v.revision;
      for (const intent of result.i) {
        if (intent.intent === "history-push") {
          navFrames.push({
            params: JSON.stringify(intent.params ?? {}),
            token: nextNavToken++,
          });
        } else if (intent.intent === "history-replace") {
          navFrames[navFrames.length - 1] = {
            params: JSON.stringify(intent.params ?? {}),
            token: nextNavToken++,
          };
        } else if (intent.intent === "history-back" && navFrames.length > 1) {
          navFrames.pop();
        }
      }
      renderPage(result.v);
      surfaces?.render(result.v);
      focus?.handleIntents(result.i);
      for (const guard of result.g) {
        console.warn(`uhura ${guard.code} ${guard.rule}: ${guard.message}`);
      }
      try {
        const snapshot = JSON.parse(session.inspect()) as InspectSnapshot;
        inspection.record(result, snapshot);
      } catch (error) {
        // Inspection is observational. A tooling failure must not interrupt
        // the already-committed machine step or the renderer/provider pump.
        console.error("uhura inspection failed", error);
        inspection.dispose();
      }
      console.debug("uhura-step", JSON.stringify(result.t));
    }

    pump = createPump({
      dispatch: (eventJson) => session.dispatch(eventJson),
      deliver: (commandJson) => driver.deliver(commandJson),
      onStep,
      onError: (error, eventJson) => {
        if (disposed) return;
        console.error("uhura dispatch failed", error, eventJson);
        overlay.showFatal(`dispatch failed: ${String(error)}\n\nevent: ${eventJson}`);
      },
    });

    ticks = createTicks({
      tick: () => driver.tick(),
      idle: () => driver.idle(),
      enqueue: (eventJson) => pump.enqueue(eventJson),
      toEvent: providerMsgToEvent,
      intervalMs: DEFAULT_TICK_MS,
    });

    const entry = String((JSON.parse(irText) as { entry?: unknown }).entry);
    pump.enqueue(JSON.stringify({ kind: "init", route: entry, params: {} }));
    if (disposed) return;
    ticks.start();
    runtime.ticks = ticks;
    systemControls.ready(currentRemoteSystemInfo());
  }

  try {
    listenForDevEvents();
  } catch (error) {
    systemControls.failed(error);
    overlay.showFatal(`failed to open the Play event stream: ${String(error)}`);
  }

  void boot().catch((error: unknown) => {
    if (disposed || isAbort(error)) return;
    systemControls.failed(error, currentRemoteSystemInfo());
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
      ticks?.stop();
      ticks = null;
      surfaces?.dispose();
      surfaces = null;
      focus?.dispose();
      focus = null;
      scrolls?.dispose();
      scrolls = null;
      playAssets?.dispose?.();
      playAssets = null;
      inspection.dispose();
      release(runtime.driver);
      providerHost?.dispose();
      providerHost = null;
      release(runtime.session);
      runtime.driver = null;
      runtime.session = null;
      runtime.ticks = null;
      applicationStyle.textContent = "";
      overlay.hide();
      if (view.__uhura === runtime) delete view.__uhura;
    },
  };
}
