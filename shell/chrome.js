// Host-owned Play controls. These values are deliberately outside the Uhura
// program: frame size, provider selection, actor identity, and restart are
// properties of the prototype runner rather than application session state.

import { readFramePreference, writeFramePreference } from "./frame-preference.js";

const FRAME_SPECS = {
  mobile: { label: "Mobile", width: 390, height: 844 },
  desktop: { label: "Desktop", width: 1280, height: 800 },
};

/** @param {string} id */
function required(id) {
  const node = document.getElementById(id);
  if (!node) throw new Error(`index.html lost #${id}`);
  return node;
}

const stage = required("uh-stage");
const frame = required("uh-frame");
const frameSizer = required("uh-frame-sizer");
const frameLabel = required("uh-frame-label");
const frameButtons = [...document.querySelectorAll("[data-uh-frame]")];
const runtimeStatus = required("uh-runtime-status");
const providerControl = required("uh-provider-control");
/** @type {HTMLSelectElement} */
const providerSelect = /** @type {HTMLSelectElement} */ (required("uh-provider-select"));
/** @type {HTMLSelectElement} */
const actorSelect = /** @type {HTMLSelectElement} */ (required("uh-actor-select"));
/** @type {HTMLButtonElement} */
const restart = /** @type {HTMLButtonElement} */ (required("uh-restart"));
const playWindow = /** @type {any} */ (window);

/** @type {keyof typeof FRAME_SPECS} */
let frameName = "mobile";

function fitFrame() {
  const spec = FRAME_SPECS[frameName];
  const availableWidth = Math.max(1, stage.clientWidth - 48);
  const availableHeight = Math.max(1, stage.clientHeight - 76);
  const scale = Math.min(1, availableWidth / spec.width, availableHeight / spec.height);
  frame.style.inlineSize = `${spec.width}px`;
  frame.style.blockSize = `${spec.height}px`;
  frame.style.transform = `scale(${scale})`;
  frameSizer.style.inlineSize = `${Math.round(spec.width * scale)}px`;
  frameSizer.style.blockSize = `${Math.round(spec.height * scale)}px`;
}

/**
 * @param {keyof typeof FRAME_SPECS} next
 * @param {boolean} persist
 */
function selectFrame(next, persist) {
  frameName = next;
  const spec = FRAME_SPECS[next];
  frame.dataset.frame = next;
  frameLabel.replaceChildren(
    document.createTextNode(`${spec.label} `),
    Object.assign(document.createElement("span"), {
      textContent: `${spec.width} × ${spec.height}`,
    }),
  );
  for (const button of frameButtons) {
    button.setAttribute("aria-pressed", String(button.getAttribute("data-uh-frame") === next));
  }
  if (persist) writeFramePreference(next);
  fitFrame();
}

for (const button of frameButtons) {
  button.addEventListener("click", () => {
    const next = button.getAttribute("data-uh-frame");
    if (next === "mobile" || next === "desktop") selectFrame(next, true);
  });
}

new ResizeObserver(fitFrame).observe(stage);
selectFrame(readFramePreference(), false);

/** @param {HTMLSelectElement} select */
function clearOptions(select) {
  while (select.firstChild) select.firstChild.remove();
}

/**
 * @param {"starting" | "ready" | "error"} state
 * @param {string} [message]
 */
function renderStatus(state, message) {
  runtimeStatus.dataset.status = state;
  const label = state === "ready" ? "Running" : state === "error" ? "Error" : "Starting";
  runtimeStatus.replaceChildren(document.createElement("span"), document.createTextNode(` ${label}`));
  runtimeStatus.title = message ?? "";
}

/** @param {import("./system-controls.js").SystemState} system */
function renderSystem(system) {
  renderStatus(system.status, system.error);
  restart.disabled = system.status === "starting";
  providerControl.hidden = system.providers.length < 2;

  const priorProvider = providerSelect.value;
  clearOptions(providerSelect);
  for (const provider of system.providers) {
    const option = document.createElement("option");
    option.value = provider;
    option.textContent = provider === "remote" ? "Remote" : "Fixture";
    providerSelect.append(option);
  }
  if (system.provider) providerSelect.value = system.provider;
  else if (priorProvider) providerSelect.value = priorProvider;
  providerSelect.disabled = system.status === "starting" || system.providers.length < 2;

  clearOptions(actorSelect);
  if (system.actors.length === 0) {
    const option = document.createElement("option");
    option.textContent = system.provider === "fixture" ? "Fixture identity" : "Unavailable";
    actorSelect.append(option);
  } else {
    const hasCurrent = system.actors.some((actor) => actor.id === system.actor);
    if (!hasCurrent) {
      const prompt = document.createElement("option");
      prompt.value = "";
      prompt.textContent = "Choose actor…";
      prompt.disabled = true;
      prompt.selected = true;
      actorSelect.append(prompt);
    }
    for (const actor of system.actors) {
      const option = document.createElement("option");
      option.value = actor.id;
      option.textContent = `${actor.label} (@${actor.username})`;
      actorSelect.append(option);
    }
    if (hasCurrent && system.actor) actorSelect.value = system.actor;
  }
  actorSelect.disabled = system.status === "starting" || !system.canSwitchActor;
}

window.addEventListener("uhura:system-state", (event) => {
  if (event instanceof CustomEvent) renderSystem(event.detail);
});

providerSelect.addEventListener("change", () => {
  const provider = providerSelect.value;
  if (provider === "remote" || provider === "fixture") {
    playWindow.__uhura?.setProvider(provider);
  }
});

actorSelect.addEventListener("change", () => {
  playWindow.__uhura?.setActor(actorSelect.value);
});

restart.addEventListener("click", () => {
  playWindow.__uhura?.restart();
});

if (playWindow.__uhura?.system) renderSystem(playWindow.__uhura.system);

export {};
