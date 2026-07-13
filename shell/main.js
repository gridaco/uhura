// uhura play (§8.4, §12.3): wires the wasm Session to the selected provider
// through envelope JSON — the provider seam stays visible in DevTools.
// Boot: fetch artifacts → assert protocols → boot deliveries → Init →
// tick cadence. The Play server pushes recheck results over SSE: a good edit
// full-restart reloads onto the new IR; a broken one overlays diagnostics
// over the still-running last-good app.

import init, { FixtureDriver, Session, protocols } from "/wasm/uhura_wasm.js";
import { createAssets } from "./assets.js";
import * as focus from "./focus.js";
import { createOverlay } from "./overlay.js";
import { selectPlayProvider } from "./play-provider-selection.js";
import { createPump, providerMsgToEvent } from "./pump.js";
import { createProviderHost } from "./provider-host.js";
import { createReconciler, findScope } from "./reconciler.js";
import { createScrolls } from "./scroll.js";
import { createSurfaces } from "./surfaces.js";
import {
  SYSTEM_ACTOR_STORAGE_KEY,
  SYSTEM_PROVIDER_STORAGE_KEY,
  createSystemControls,
} from "./system-controls.js";
import { createTextFields } from "./textfield.js";
import { createTicks, DEFAULT_TICK_MS } from "./ticks.js";

/** @param {string} id */
function el(id) {
  const found = document.getElementById(id);
  if (!found) throw new Error(`index.html lost #${id}`);
  return found;
}

const overlay = createOverlay(el("uh-overlay"));
const pageHost = el("uh-page");
const surfaceHost = el("uh-surfaces");

// Install one stable handle before the first await. Prototype chrome can read
// the starting state immediately, and even a failed remote boot keeps enough
// host control to choose another provider/actor and navigate to a clean run.
const systemControls = createSystemControls({
  target: window,
  storage: sessionStorage,
  reload: () => location.reload(),
});
const runtime = /** @type {any} */ ({
  session: null,
  driver: null,
  steps: [],
  ticks: null,
  get system() {
    return systemControls.state;
  },
  restart: () => systemControls.restart(),
  setActor: (/** @type {string} */ actor) => systemControls.setActor(actor),
  setProvider: (/** @type {"remote" | "fixture"} */ provider) =>
    systemControls.setProvider(provider),
});
/** @type {any} */ (window).__uhura = runtime;

/** @returns {import("./types.js").RemoteSystemInfo | undefined} */
function currentRemoteSystemInfo() {
  const driver = runtime.driver;
  if (!driver || typeof driver.systemInfo !== "function") return undefined;
  try {
    return driver.systemInfo();
  } catch (error) {
    console.error("uhura provider system metadata failed", error);
    return undefined;
  }
}

/** @param {string} url */
async function fetchText(url) {
  const response = await fetch(url);
  const text = await response.text();
  if (!response.ok) throw new Error(`${url}: ${response.status}\n${text}`);
  return text;
}

/**
 * Imports the content-addressed app provider. A recheck can retire that exact
 * hash after play.json was fetched, so mirror the artifact loader's one silent
 * retry before surfacing a real module error.
 * @param {string} module
 */
async function importProvider(module) {
  try {
    const loaded = await import(module);
    sessionStorage.removeItem("uh-provider-retry");
    return loaded;
  } catch (error) {
    if (sessionStorage.getItem("uh-provider-retry") === null) {
      sessionStorage.setItem("uh-provider-retry", "1");
      location.reload();
      await new Promise(() => {}); // reloading — never resolve
    }
    throw error;
  }
}

/**
 * Fetches the build artifacts and REFUSES a mixed-generation set (a
 * recheck can land between fetches): one silent retry via reload, then
 * fatal. The dev server stamps `X-Uhura-Generation` on each.
 * @param {string[]} urls
 */
async function fetchArtifacts(urls) {
  const responses = await Promise.all(urls.map((url) => fetch(url)));
  const texts = await Promise.all(responses.map((r) => r.text()));
  responses.forEach((r, i) => {
    if (!r.ok) throw new Error(`${urls[i]}: ${r.status}\n${texts[i]}`);
  });
  const generations = new Set(
    responses.map((r) => r.headers.get("x-uhura-generation")).filter((g) => g !== null),
  );
  if (generations.size > 1) {
    if (sessionStorage.getItem("uh-gen-retry") === null) {
      sessionStorage.setItem("uh-gen-retry", "1");
      location.reload();
      await new Promise(() => {}); // reloading — never resolve
    }
    throw new Error("artifact fetches spanned a recheck twice — reload the page");
  }
  sessionStorage.removeItem("uh-gen-retry");
  return texts;
}

// ── dev events first: even a failed boot hears the fix and reloads ─────────
/** @type {number | null} */
let baseGeneration = null;
let everOpened = false;
const events = new EventSource("/events");
events.onopen = () => {
  // A RECONNECT means the server restarted (fresh generation counter) or
  // this page slept through events — reload onto whatever is current.
  if (everOpened) location.reload();
  everOpened = true;
};
events.onmessage = (/** @type {MessageEvent<string>} */ message) => {
  /** @type {import("./types.js").DevEvent} */
  const dev = JSON.parse(message.data);
  if (baseGeneration === null) baseGeneration = dev.generation;
  if (dev.ok) {
    if (dev.generation > baseGeneration) {
      location.reload(); // full-restart hot reload (§12.4) — never faked
    } else {
      overlay.hideDiagnostics(); // never un-covers a dead (fatal) page
    }
  } else if (dev.diagnostics) {
    overlay.showDiagnostics(dev.diagnostics);
  }
};

boot().catch((error) => {
  systemControls.failed(error, currentRemoteSystemInfo());
  overlay.showFatal(String(error));
});

async function boot() {
  const [irText, bootText, fixtureText, scriptText, iconsText, playText] = await fetchArtifacts([
    "/ir.json",
    "/boot.json",
    "/fixture.json",
    "/script.json",
    "/icons.json",
    "/play.json",
  ]);
  await init();

  // ── the §12.3 boot-protocol assertion: hard fail, no guessing ──────────
  const spoken = JSON.parse(protocols());
  const expected = { ir: "uhura-ir/0", view: "uhura-view/0", provider: "uhura-provider/0" };
  for (const [name, version] of Object.entries(expected)) {
    if (spoken[name] !== version) {
      throw new Error(
        `protocol mismatch: this shell speaks ${name} ${version}, the wasm build speaks ${spoken[name]} — rebuild with scripts/build-wasm.sh`,
      );
    }
  }

  // Play alone may select an app-owned remote provider. Static canvas,
  // checking, examples, and trace keep using the fixture. Play's provider and
  // auth actor are tab-local shell state and never enter the app's URL.
  /** @type {import("./types.js").PlayConfig} */
  const play = JSON.parse(playText);
  const storedProvider = sessionStorage.getItem(SYSTEM_PROVIDER_STORAGE_KEY);
  const selection = selectPlayProvider(play, storedProvider);
  if (selection.clearStoredProvider) {
    sessionStorage.removeItem(SYSTEM_PROVIDER_STORAGE_KEY);
  }
  const inferredProvider = selection.provider;
  const configuredActor =
    play.provider.kind === "module" ? play.provider.config.actor ?? null : null;
  const storedActor = sessionStorage.getItem(SYSTEM_ACTOR_STORAGE_KEY)?.trim() || null;
  const selectedActor = storedActor ?? configuredActor;
  systemControls.starting({
    provider: inferredProvider,
    providers: selection.providers,
    actor: inferredProvider === "remote" ? selectedActor : null,
    actors: [],
  });
  const useRemote = inferredProvider === "remote";
  const session = new Session(irText);
  runtime.session = session;
  /** @type {import("./types.js").Driver} */
  let driver;
  /** @type {((assetRef: string) => Promise<string>) | undefined} */
  let resolveAsset;
  if (useRemote) {
    if (play.provider.kind !== "module") {
      throw new Error("remote play was selected without a provider module");
    }
    /** @type {{ createDriver?: (config: Record<string, string>, host?: import("./types.js").ProviderHost) => import("./types.js").RemoteDriver }} */
    const providerModule = await importProvider(play.provider.module);
    if (typeof providerModule.createDriver !== "function") {
      throw new Error(`${play.provider.module} must export createDriver(config, host)`);
    }
    const config = { ...play.provider.config };
    if (selectedActor !== null) config.actor = selectedActor;
    const remote = providerModule.createDriver(config, createProviderHost());
    runtime.driver = remote;
    session.boot(await remote.assembleBoot());
    driver = remote;
    if (typeof remote.resolveAsset === "function") {
      resolveAsset = remote.resolveAsset.bind(remote);
    }
  } else {
    session.boot(bootText);
    driver = new FixtureDriver(fixtureText, scriptText);
    runtime.driver = driver;
  }
  /** @type {Record<string, string>} */
  const glyphs = JSON.parse(iconsText);

  let currentRevision = 0;
  /** @type {string | null} */
  let currentNavKey = null;
  /** @type {HTMLElement | null} */
  let pageEl = null;
  // The machine owns nav (§7.4); the shell only mirrors its stack closely
  // enough to key page instances. The minted local token matters for
  // `replace`: replacing a route with the same route and params must still
  // remount a fresh page subtree, while `back` must reveal the prior key.
  let nextNavToken = 1;
  /** @type {{ params: string, token: number }[]} */
  const navFrames = [{ params: "{}", token: 0 }];

  /**
   * @param {import("./types.js").Descriptor} descriptor
   * @param {Record<string, unknown>} [data]
   * @param {() => void} [onApplied]
   */
  function emit(descriptor, data, onApplied) {
    /** @type {Record<string, unknown>} */
    const event = { kind: "ui", descriptor, "view-rev": currentRevision };
    if (data) event["data"] = data;
    pump.enqueue(JSON.stringify(event), onApplied);
  }

  const textFields = createTextFields({ emit });
  const scrolls = createScrolls({ emit });
  const assets = createAssets(resolveAsset);
  const reconciler = createReconciler({ emit, glyphs, assets, textFields, scrolls });
  const surfaces = createSurfaces({
    host: surfaceHost,
    pageHost,
    emit,
    reconcileChildren: reconciler.reconcileChildren,
  });

  /** @param {import("./types.js").Snapshot} snapshot */
  function renderPage(snapshot) {
    const scope = findScope(snapshot.page.root) ?? "page";
    const topFrame = navFrames.at(-1) ?? { params: "{}", token: -1 };
    const navKey = `${snapshot.page.route}|${navFrames.length}|${topFrame.params}|${topFrame.token}`;
    if (!pageEl || currentNavKey !== navKey) {
      // A page-instance change remounts the subtree (§8.4); scroll
      // positions round-trip through the per-instance cache.
      if (pageEl && currentNavKey !== null) scrolls.savePositions(currentNavKey, pageEl);
      pageHost.replaceChildren();
      pageEl = document.createElement("div");
      pageEl.className = "uh-page-root";
      pageHost.append(pageEl);
      reconciler.reconcileChildren(pageEl, [snapshot.page.root], scope, false);
      scrolls.restorePositions(navKey, pageEl);
      currentNavKey = navKey;
    } else {
      reconciler.reconcileChildren(pageEl, [snapshot.page.root], scope, false);
    }
  }

  /** @type {Record<string, unknown>[]} */
  const steps = [];

  /** @param {import("./types.js").StepResult} result */
  function onStep(result) {
    currentRevision = result.v.revision;
    // Nav intents first: renderPage keys the incoming page instance off
    // the mirrored stack (this step's push/replace/back belongs to this view).
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
    surfaces.render(result.v);
    focus.handleIntents(result.i);
    for (const g of result.g) console.warn(`uhura ${g.code} ${g.rule}: ${g.message}`);
    steps.push(result.t);
    console.debug("uhura-step", JSON.stringify(result.t));
  }

  const pump = createPump({
    dispatch: (eventJson) => session.dispatch(eventJson),
    deliver: (cmdJson) => driver.deliver(cmdJson),
    onStep,
    onError: (error, eventJson) => {
      console.error("uhura dispatch failed", error, eventJson);
      overlay.showFatal(`dispatch failed: ${String(error)}\n\nevent: ${eventJson}`);
    },
  });

  const ticks = createTicks({
    tick: () => driver.tick(),
    idle: () => driver.idle(),
    enqueue: (eventJson) => pump.enqueue(eventJson),
    toEvent: providerMsgToEvent,
    intervalMs: DEFAULT_TICK_MS,
  });

  // Boot projections are already applied — bare reads are legal from the
  // first view (§9.2). Init mounts the entry route.
  const entry = String(JSON.parse(irText).entry);
  pump.enqueue(JSON.stringify({ kind: "init", route: entry, params: {} }));
  ticks.start();

  // Debug/verification handle (the M5 gate reads this). Keep the object
  // identity installed before boot so system chrome never loses its methods.
  Object.assign(runtime, { session, driver, steps, ticks });
  systemControls.ready(currentRemoteSystemInfo());
}
