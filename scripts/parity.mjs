// Native ↔ wasm parity (design §12.5, §13): replays a script through the
// REAL wasm32 binary (pkg/node, built by scripts/build-wasm.sh) with the
// same JSON-only pump the play shell and the native ABI-contract test
// use, and diffs the per-step trace lines byte-for-byte against the
// native harness's output.
//
// Inputs (a directory of prepared artifacts):
//   ir.json      — canonical uhura-ir/0 (uhura check --emit-ir)
//   fixture.json — resolved slices (what `uhura play` serves)
//   script.json  — the script as JSON
//   boot.json    — {"updates": […]} boot deliveries
//   native.jsonl — `uhura trace --script=<name>` output
//
// Usage: node scripts/parity.mjs <artifact-dir>
// M6 automates artifact preparation per canonical script; until then the
// quickest source is a running `uhura play` (curl `/api/play/ir.json`,
// `/api/play/fixture.json`, `/api/play/script.json`, and `/api/play/boot.json`)
// plus `uhura trace` for native.jsonl.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const require = createRequire(import.meta.url);
const { Session, FixtureDriver, protocols } = require(
  join(here, "../crates/uhura-wasm/pkg/node/uhura_wasm.js"),
);

const dir = process.argv[2];
if (!dir) {
  console.error("usage: node scripts/parity.mjs <artifact-dir>");
  process.exit(2);
}
const read = (name) => readFileSync(join(dir, name), "utf8");

const spoken = JSON.parse(protocols());
if (spoken.ir !== "uhura-ir/0" || spoken.view !== "uhura-view/0" || spoken.provider !== "uhura-provider/0") {
  console.error(`protocol mismatch: ${JSON.stringify(spoken)}`);
  process.exit(1);
}

const irText = read("ir.json");
const script = JSON.parse(read("script.json"));
const native = read("native.jsonl").trim().split("\n");

// ── the same pump as web/src/play/main.ts and abi_contract.rs ──────────
const session = new Session(irText);
session.boot(read("boot.json"));
const driver = new FixtureDriver(read("fixture.json"), JSON.stringify(script));

const stimuli = (script.ui ?? []).map((entry) => ({
  atTick: entry["at-tick"],
  emit: entry.emit,
  where: entry.where ?? {},
  data: entry.data ?? {},
}));

const lines = [];
function dispatch(event) {
  const raw = session.dispatch(JSON.stringify(event));
  // The compared artifacts are extracted from the RAW canonical envelope
  // bytes — never round-tripped through JS numbers, so the byte-parity
  // verdict is faithful for the whole i64 domain.
  lines.push(extractTop(raw, "t"));
  for (const c of arrayElements(extractTop(raw, "c"))) driver.deliver(c);
  return JSON.parse(raw).v;
}

/**
 * The raw value substring of a top-level key in one canonical JSON
 * object (canonical ⇒ object/array/string/number/bool/null, no floats).
 */
function extractTop(raw, key) {
  const needle = `"${key}":`;
  let depth = 0;
  let inStr = false;
  let esc = false;
  for (let i = 0; i < raw.length; i += 1) {
    const ch = raw[i];
    if (inStr) {
      if (esc) esc = false;
      else if (ch === "\\") esc = true;
      else if (ch === '"') inStr = false;
      continue;
    }
    if (ch === '"') {
      if (depth === 1 && raw.startsWith(needle, i)) {
        return sliceValue(raw, i + needle.length);
      }
      inStr = true;
    } else if (ch === "{" || ch === "[") depth += 1;
    else if (ch === "}" || ch === "]") depth -= 1;
  }
  throw new Error(`no top-level "${key}" in the step result`);
}

function sliceValue(raw, start) {
  let depth = 0;
  let inStr = false;
  let esc = false;
  for (let i = start; i < raw.length; i += 1) {
    const ch = raw[i];
    if (inStr) {
      if (esc) esc = false;
      else if (ch === "\\") esc = true;
      else if (ch === '"') inStr = false;
    } else if (ch === '"') inStr = true;
    else if (ch === "{" || ch === "[") depth += 1;
    else if (ch === "}" || ch === "]") {
      depth -= 1;
      if (depth === 0) return raw.slice(start, i + 1);
    } else if (depth === 0 && (ch === "," || ch === "}")) {
      return raw.slice(start, i); // bare scalar value
    }
  }
  throw new Error("unbalanced JSON value");
}

/** Splits a raw canonical JSON array into raw element substrings. */
function arrayElements(rawArray) {
  const out = [];
  let depth = 0;
  let inStr = false;
  let esc = false;
  let start = -1;
  for (let i = 0; i < rawArray.length; i += 1) {
    const ch = rawArray[i];
    if (inStr) {
      if (esc) esc = false;
      else if (ch === "\\") esc = true;
      else if (ch === '"') inStr = false;
      continue;
    }
    if (ch === '"') inStr = true;
    else if (ch === "{" || ch === "[") {
      depth += 1;
      if (depth === 2) start = i;
    } else if (ch === "}" || ch === "]") {
      if (depth === 2 && start >= 0) {
        out.push(rawArray.slice(start, i + 1));
        start = -1;
      }
      depth -= 1;
    }
  }
  return out;
}

const matches = (d, stim) =>
  d.emit === stim.emit &&
  Object.entries(stim.where).every(([k, v]) => JSON.stringify(d.payload?.[k]) === JSON.stringify(v));

function findDescriptor(view, stim) {
  const found = [];
  const walk = (node) => {
    for (const d of node.on ?? []) if (matches(d, stim)) found.push(d);
    for (const child of node.children ?? []) walk(child);
  };
  walk(view.page.root);
  for (const surface of view.surfaces) {
    walk(surface.root);
    if (matches(surface.dismiss, stim)) found.push(surface.dismiss);
  }
  const distinct = found.filter(
    (d, i) =>
      found.findIndex(
        (o) => o.emit === d.emit && o.scope === d.scope && canonical(o.payload) === canonical(d.payload),
      ) === i,
  );
  if (distinct.length !== 1) {
    throw new Error(`stimulus \`${stim.emit}\` matched ${distinct.length} descriptors`);
  }
  return distinct[0];
}

let view = dispatch({ kind: "init", route: JSON.parse(irText).entry, params: {} });
let tick = 0;
let next = 0;
while (!(driver.idle() && next >= stimuli.length)) {
  tick += 1;
  if (tick > 10_000) throw new Error("the script did not quiesce");
  for (const msgJson of driver.tick()) {
    const msg = JSON.parse(msgJson);
    view = dispatch(msg.kind === "projection" ? { kind: "projection", updates: [msg] } : msg);
  }
  while (next < stimuli.length && stimuli[next].atTick === tick) {
    const stim = stimuli[next++];
    const event = { kind: "ui", descriptor: findDescriptor(view, stim), "view-rev": view.revision };
    if (Object.keys(stim.data).length > 0) event.data = stim.data;
    view = dispatch(event);
  }
}

// ── canonical JSON (stimulus matching only — trace lines never pass
// through here) ─────────────────────────────────────────────────────────
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value).sort();
    return `{${keys.map((k) => `${JSON.stringify(k)}:${canonical(value[k])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

// ── diff ─────────────────────────────────────────────────────────────────
if (lines.length !== native.length) {
  console.error(`step count diverged: wasm ${lines.length}, native ${native.length}`);
  process.exit(1);
}
for (let i = 0; i < lines.length; i += 1) {
  if (lines[i] !== native[i]) {
    console.error(`step ${i} diverged:\n  wasm:   ${lines[i]}\n  native: ${native[i]}`);
    process.exit(1);
  }
}
console.log(`parity: ${lines.length} steps byte-identical (native ↔ wasm32)`);
