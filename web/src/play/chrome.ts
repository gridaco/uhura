// Route-owned Play controls. Frame size, provider, actor, and restart remain
// host state rather than Uhura application state.

import type { SystemState } from "../protocol/types.js";
import {
  readFramePreference,
  writeFramePreference,
} from "./frame-preference.js";
import type { FramePreferenceStorage } from "./frame-preference.js";
import type { PlayShell } from "./shell.js";

const FRAME_SPECS = {
  mobile: { label: "Mobile", width: 390, height: 844 },
  desktop: { label: "Desktop", width: 1280, height: 800 },
} as const;

// Keep the centered prototype clear of the 52px toolbar and its frame label.
// Reserving the same space below keeps the prototype centered in the viewport.
const FRAME_VERTICAL_SAFE_INSET = 84;

type FrameName = keyof typeof FRAME_SPECS;

interface ResizeObserverHandle {
  observe(target: Element): void;
  disconnect(): void;
}

export interface PlayChromeOptions {
  window?: Window;
  storage?: FramePreferenceStorage;
  createResizeObserver?: (
    callback: ResizeObserverCallback,
  ) => ResizeObserverHandle;
}

export interface PlayChrome {
  setDebugOpen(open: boolean): void;
  dispose(): void;
}

function windowStorage(view: Window): FramePreferenceStorage | undefined {
  try {
    return view.localStorage;
  } catch {
    return undefined;
  }
}

function clearOptions(select: HTMLSelectElement): void {
  while (select.firstChild) select.firstChild.remove();
}

export function mountPlayChrome(
  shell: PlayShell,
  options: PlayChromeOptions = {},
): PlayChrome {
  const AUTO_HIDE_DELAY_MS = 3_000;
  const view = options.window ?? shell.document.defaultView ?? window;
  const storage = options.storage ?? windowStorage(view);
  let frameName: FrameName = "mobile";
  let disposed = false;
  let autoHideTimer: number | undefined;
  const toolbar = shell.container.querySelector<HTMLElement>("#uh-shell-toolbar");

  function chromeContains(target: EventTarget | null): boolean {
    if (target === null) return false;
    const node = target as Node;
    return toolbar?.contains(node) === true || shell.restart.contains(node);
  }

  function chromeHasFocus(): boolean {
    return chromeContains(shell.document.activeElement);
  }

  function revealShellUi(): void {
    if (autoHideTimer !== undefined) view.clearTimeout(autoHideTimer);
    autoHideTimer = undefined;
    delete shell.container.dataset["uiHidden"];
  }

  function hideShellUi(): void {
    revealShellUi();
    if (
      shell.container.dataset["debugOpen"] === "true"
      || chromeHasFocus()
    ) {
      return;
    }
    shell.container.dataset["uiHidden"] = "true";
  }

  function scheduleAutoHide(focusIsLeavingChrome = false): void {
    revealShellUi();
    if (
      shell.container.dataset["debugOpen"] === "true"
      || (!focusIsLeavingChrome && chromeHasFocus())
    ) {
      return;
    }
    autoHideTimer = view.setTimeout(hideShellUi, AUTO_HIDE_DELAY_MS);
  }

  function toggleShellUi(): void {
    if (shell.container.dataset["uiHidden"] === "true") scheduleAutoHide();
    else hideShellUi();
  }

  function fitFrame(): void {
    if (disposed) return;
    const spec = FRAME_SPECS[frameName];
    const availableWidth = Math.max(1, shell.stage.clientWidth - 48);
    const availableHeight = Math.max(
      1,
      shell.stage.clientHeight - FRAME_VERTICAL_SAFE_INSET * 2,
    );
    const scale = Math.min(
      1,
      availableWidth / spec.width,
      availableHeight / spec.height,
    );
    shell.frame.style.inlineSize = `${spec.width}px`;
    shell.frame.style.blockSize = `${spec.height}px`;
    shell.frame.style.transform = `scale(${scale})`;
    shell.frameSizer.style.inlineSize = `${Math.round(spec.width * scale)}px`;
    shell.frameSizer.style.blockSize = `${Math.round(spec.height * scale)}px`;
  }

  function selectFrame(next: FrameName, persist: boolean): void {
    frameName = next;
    const spec = FRAME_SPECS[next];
    shell.frame.dataset.frame = next;
    shell.frameLabel.replaceChildren(
      shell.document.createTextNode(`${spec.label} `),
      Object.assign(shell.document.createElement("span"), {
        textContent: `${spec.width} × ${spec.height}`,
      }),
    );
    for (const button of shell.frameButtons) {
      button.setAttribute(
        "aria-pressed",
        String(button.getAttribute("data-uh-frame") === next),
      );
    }
    if (persist) writeFramePreference(next, storage);
    fitFrame();
  }

  function renderStatus(state: SystemState["status"], message?: string): void {
    shell.runtimeStatus.dataset.status = state;
    const label = state === "ready" ? "Running" : state === "error" ? "Error" : "Starting";
    const dot = shell.document.createElement("span");
    dot.setAttribute("aria-hidden", "true");
    shell.runtimeStatus.replaceChildren(dot);
    if (state !== "ready") {
      shell.runtimeStatus.append(shell.document.createTextNode(label));
    }
    shell.runtimeStatus.setAttribute("aria-label", label);
    shell.runtimeStatus.title = message ?? label;
  }

  function renderSystem(system: SystemState): void {
    if (disposed) return;
    renderStatus(system.status, system.error);
    shell.restart.disabled = system.status === "starting";
    shell.providerControl.hidden = system.providers.length < 2;

    const priorProvider = shell.providerSelect.value;
    clearOptions(shell.providerSelect);
    for (const provider of system.providers) {
      const option = shell.document.createElement("option");
      option.value = provider;
      option.textContent = provider === "remote" ? "Remote" : "Fixture";
      shell.providerSelect.append(option);
    }
    if (system.provider) shell.providerSelect.value = system.provider;
    else if (priorProvider) shell.providerSelect.value = priorProvider;
    shell.providerSelect.disabled =
      system.status === "starting" || system.providers.length < 2;

    clearOptions(shell.actorSelect);
    if (system.actors.length === 0) {
      const option = shell.document.createElement("option");
      option.textContent =
        system.provider === "fixture" ? "Fixture identity" : "Unavailable";
      shell.actorSelect.append(option);
    } else {
      const hasCurrent = system.actors.some((actor) => actor.id === system.actor);
      if (!hasCurrent) {
        const prompt = shell.document.createElement("option");
        prompt.value = "";
        prompt.textContent = "Choose actor…";
        prompt.disabled = true;
        prompt.selected = true;
        shell.actorSelect.append(prompt);
      }
      for (const actor of system.actors) {
        const option = shell.document.createElement("option");
        option.value = actor.id;
        option.textContent = `${actor.label} (@${actor.username})`;
        shell.actorSelect.append(option);
      }
      if (hasCurrent && system.actor) shell.actorSelect.value = system.actor;
    }
    shell.actorSelect.disabled =
      system.status === "starting" || !system.canSwitchActor;
  }

  const frameListeners = new Map<HTMLButtonElement, () => void>();
  for (const button of shell.frameButtons) {
    const listener = (): void => {
      const next = button.getAttribute("data-uh-frame");
      if (next === "mobile" || next === "desktop") selectFrame(next, true);
    };
    frameListeners.set(button, listener);
    button.addEventListener("click", listener);
  }

  const onSystemState = (event: Event): void => {
    const detail = (event as CustomEvent<unknown>).detail;
    if (typeof detail === "object" && detail !== null) {
      renderSystem(detail as SystemState);
    }
  };
  const onProviderChange = (): void => {
    const provider = shell.providerSelect.value;
    if (provider === "remote" || provider === "fixture") {
      view.__uhura?.setProvider(provider);
    }
  };
  const onActorChange = (): void => {
    view.__uhura?.setActor(shell.actorSelect.value);
  };
  const renderFullscreen = (): void => {
    const active = shell.document.fullscreenElement !== null;
    const label = active ? "Exit fullscreen" : "Enter fullscreen";
    shell.fullscreen.setAttribute("aria-label", label);
    shell.fullscreen.setAttribute("title", label);
    shell.fullscreen.setAttribute("aria-pressed", String(active));
  };
  const onFullscreen = (): void => {
    const action = shell.document.fullscreenElement
      ? shell.document.exitFullscreen()
      : shell.document.documentElement.requestFullscreen();
    void action.catch((error: unknown) => {
      console.error("uhura fullscreen request failed", error);
    });
  };
  const onRestart = (): void => view.__uhura?.restart();
  const onStageClick = (event: MouseEvent): void => {
    const target = event.target;
    if (target instanceof Node && !shell.frame.contains(target)) toggleShellUi();
  };
  const onShellInteraction = (): void => scheduleAutoHide();
  const onChromeFocusIn = (): void => revealShellUi();
  const onChromeFocusOut = (event: FocusEvent): void => {
    if (chromeContains(event.relatedTarget)) return;
    scheduleAutoHide(true);
  };

  view.addEventListener("uhura:system-state", onSystemState);
  shell.providerSelect.addEventListener("change", onProviderChange);
  shell.actorSelect.addEventListener("change", onActorChange);
  shell.fullscreen.addEventListener("click", onFullscreen);
  shell.document.addEventListener("fullscreenchange", renderFullscreen);
  shell.restart.addEventListener("click", onRestart);
  shell.stage.addEventListener("click", onStageClick);
  toolbar?.addEventListener("pointerdown", onShellInteraction);
  toolbar?.addEventListener("focusin", onChromeFocusIn);
  toolbar?.addEventListener("focusout", onChromeFocusOut);
  shell.restart.addEventListener("pointerdown", onShellInteraction);
  shell.restart.addEventListener("focusin", onChromeFocusIn);
  shell.restart.addEventListener("focusout", onChromeFocusOut);

  const observer = options.createResizeObserver
    ? options.createResizeObserver(fitFrame)
    : new ResizeObserver(fitFrame);
  observer.observe(shell.stage);
  selectFrame(readFramePreference(storage), false);
  renderFullscreen();
  scheduleAutoHide();
  if (view.__uhura?.system) renderSystem(view.__uhura.system);

  return {
    setDebugOpen(open: boolean): void {
      if (disposed) return;
      revealShellUi();
      if (!open) scheduleAutoHide();
      fitFrame();
    },
    dispose(): void {
      if (disposed) return;
      disposed = true;
      if (autoHideTimer !== undefined) view.clearTimeout(autoHideTimer);
      autoHideTimer = undefined;
      observer.disconnect();
      view.removeEventListener("uhura:system-state", onSystemState);
      shell.providerSelect.removeEventListener("change", onProviderChange);
      shell.actorSelect.removeEventListener("change", onActorChange);
      shell.fullscreen.removeEventListener("click", onFullscreen);
      shell.document.removeEventListener("fullscreenchange", renderFullscreen);
      shell.restart.removeEventListener("click", onRestart);
      shell.stage.removeEventListener("click", onStageClick);
      toolbar?.removeEventListener("pointerdown", onShellInteraction);
      toolbar?.removeEventListener("focusin", onChromeFocusIn);
      toolbar?.removeEventListener("focusout", onChromeFocusOut);
      shell.restart.removeEventListener("pointerdown", onShellInteraction);
      shell.restart.removeEventListener("focusin", onChromeFocusIn);
      shell.restart.removeEventListener("focusout", onChromeFocusOut);
      for (const [button, listener] of frameListeners) {
        button.removeEventListener("click", listener);
      }
      frameListeners.clear();
    },
  };
}
