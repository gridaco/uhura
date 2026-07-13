// uhura play (§8.4, §12.3): wires the wasm Session to the FixtureDriver
// by passing envelope JSON — the Spock seam stays visible in DevTools.
// Boot: fetch artifacts → assert protocols → boot deliveries → Init →
// tick cadence. `uhura dev` pushes recheck results over SSE: a good edit
// full-restart reloads onto the new IR; a broken one overlays diagnostics
// over the still-running last-good app.

import init, { FixtureDriver, Session, protocols } from "/wasm/uhura_wasm.js";
import * as focus from "./focus.js";
import { createOverlay } from "./overlay.js";
import { createPump, providerMsgToEvent } from "./pump.js";
import { createReconciler, findScope } from "./reconciler.js";
import { createScrolls } from "./scroll.js";
import { createSurfaces } from "./surfaces.js";
import { createTextFields } from "./textfield.js";
import { createTicks, tickMillis } from "./ticks.js";

/** @param {string} id */
function el(id) {
  const found = document.getElementById(id);
  if (!found) throw new Error(`index.html lost #${id}`);
  return found;
}

const overlay = createOverlay(el("uh-overlay"));
const pageHost = el("uh-page");
const surfaceHost = el("uh-surfaces");

/** @param {string} url */
async function fetchText(url) {
  const response = await fetch(url);
  const text = await response.text();
  if (!response.ok) throw new Error(`${url}: ${response.status}\n${text}`);
  return text;
}

/**
 * Fetches the five build artifacts and REFUSES a mixed-generation set (a
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

boot().catch((error) => overlay.showFatal(String(error)));

async function boot() {
  const [irText, bootText, fixtureText, scriptText, iconsText] = await fetchArtifacts([
    "/ir.json",
    "/boot.json",
    "/fixture.json",
    "/script.json",
    "/icons.json",
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

  const session = new Session(irText);
  session.boot(bootText);
  const driver = new FixtureDriver(fixtureText, scriptText);
  /** @type {Record<string, string>} */
  const glyphs = JSON.parse(iconsText);

  let currentRevision = 0;
  /** @type {string | null} */
  let currentNavKey = null;
  /** @type {HTMLElement | null} */
  let pageEl = null;
  // The machine owns nav (§7.4); its history intents mirror the stack
  // here so page instances get identity: route + depth + params
  // (register #17). Same key ⇒ same instance ⇒ scroll restores.
  /** @type {string[]} */
  const navParams = ["{}"];

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
  const reconciler = createReconciler({ emit, glyphs, textFields, scrolls });
  const surfaces = createSurfaces({
    host: surfaceHost,
    pageHost,
    emit,
    reconcileChildren: reconciler.reconcileChildren,
  });

  /** @param {import("./types.js").Snapshot} snapshot */
  function renderPage(snapshot) {
    const scope = findScope(snapshot.page.root) ?? "page";
    const navKey = `${snapshot.page.route}|${navParams.length}|${navParams.at(-1) ?? "{}"}`;
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
    // the mirrored stack (this step's push/back belongs to this view).
    for (const intent of result.i) {
      if (intent.intent === "history-push") {
        navParams.push(JSON.stringify(intent.params ?? {}));
      } else if (intent.intent === "history-back" && navParams.length > 1) {
        navParams.pop();
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
    intervalMs: tickMillis(location.search),
  });

  // Boot projections are already applied — bare reads are legal from the
  // first view (§9.2). Init mounts the entry route.
  const entry = String(JSON.parse(irText).entry);
  pump.enqueue(JSON.stringify({ kind: "init", route: entry, params: {} }));
  ticks.start();

  // Debug/verification handle (the M5 gate reads this).
  /** @type {any} */ (window).__uhura = { session, driver, steps, ticks };
}
