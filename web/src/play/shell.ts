/** Static, host-owned Play chrome created when the `/play` route mounts. */
export const PLAY_SHELL_MARKUP = `
  <header id="uh-shell-toolbar">
    <a class="uh-editor-link" href="/" aria-label="Back to Editor">
      <svg aria-hidden="true" viewBox="0 0 16 16"><path d="m10.5 3.5-4.5 4.5 4.5 4.5"></path></svg>
      Editor
    </a>

    <div class="uh-frame-switch" role="group" aria-label="Prototype viewport">
      <button type="button" data-uh-frame="mobile" aria-pressed="true">
        <svg aria-hidden="true" viewBox="0 0 16 16">
          <rect x="4.25" y="1.5" width="7.5" height="13" rx="1.5"></rect>
          <path d="M6.75 3h2.5M7.25 12.75h1.5"></path>
        </svg>
        Mobile
      </button>
      <button type="button" data-uh-frame="desktop" aria-pressed="false">
        <svg aria-hidden="true" viewBox="0 0 16 16">
          <rect x="1.5" y="2.25" width="13" height="9" rx="1.25"></rect>
          <path d="M5.25 14h5.5M8 11.25V14"></path>
        </svg>
        Desktop
      </button>
    </div>

    <div class="uh-shell-controls">
      <output id="uh-runtime-status" class="uh-runtime-status" aria-live="polite">
        <span aria-hidden="true"></span>Starting
      </output>
      <label id="uh-provider-control" class="uh-system-select" for="uh-provider-select" hidden>
        <span>Provider</span>
        <select id="uh-provider-select" disabled><option>Starting…</option></select>
      </label>
      <label class="uh-system-select uh-actor-select" for="uh-actor-select">
        <span>Actor</span>
        <select id="uh-actor-select" disabled><option>Starting…</option></select>
      </label>
      <button id="uh-fullscreen" class="uh-fullscreen" type="button" aria-label="Enter fullscreen" title="Enter fullscreen">
        <svg aria-hidden="true" viewBox="0 0 16 16">
          <path d="M6 2.25H2.25V6M10 2.25h3.75V6M6 13.75H2.25V10M10 13.75h3.75V10"></path>
        </svg>
      </button>
    </div>
  </header>

  <button id="uh-restart" class="uh-restart" type="button" disabled>
    <svg aria-hidden="true" viewBox="0 0 16 16">
      <path d="M13.25 6A5.5 5.5 0 1 0 13 10.5"></path>
      <path d="M10.5 3.25h3v3"></path>
    </svg>
    Restart
  </button>

  <main id="uh-stage">
    <div id="uh-frame-stack">
      <div id="uh-frame-label">Mobile <span>390 × 844</span></div>
      <div id="uh-frame-sizer">
        <div id="uh-frame" data-frame="mobile">
          <div id="uh-app">
            <div id="uh-page"></div>
            <div id="uh-surfaces"></div>
          </div>
          <div id="uh-overlay" hidden></div>
        </div>
      </div>
    </div>
  </main>
`;

export interface PlayShell {
  document: Document;
  container: HTMLElement;
  stage: HTMLElement;
  frame: HTMLElement;
  frameSizer: HTMLElement;
  frameLabel: HTMLElement;
  frameButtons: HTMLButtonElement[];
  runtimeStatus: HTMLElement;
  providerControl: HTMLElement;
  providerSelect: HTMLSelectElement;
  actorSelect: HTMLSelectElement;
  fullscreen: HTMLButtonElement;
  restart: HTMLButtonElement;
  pageHost: HTMLElement;
  surfaceHost: HTMLElement;
  overlayHost: HTMLElement;
}

function required<T extends Element>(
  container: HTMLElement,
  selector: string,
): T {
  const found = container.querySelector(selector);
  if (!found) throw new Error(`Play shell lost ${selector}`);
  return found as T;
}

export function createPlayShell(document: Document): PlayShell {
  const container = document.createElement("div");
  container.className = "uh-play-route";
  container.innerHTML = PLAY_SHELL_MARKUP;
  return {
    document,
    container,
    stage: required(container, "#uh-stage"),
    frame: required(container, "#uh-frame"),
    frameSizer: required(container, "#uh-frame-sizer"),
    frameLabel: required(container, "#uh-frame-label"),
    frameButtons: [...container.querySelectorAll<HTMLButtonElement>("[data-uh-frame]")],
    runtimeStatus: required(container, "#uh-runtime-status"),
    providerControl: required(container, "#uh-provider-control"),
    providerSelect: required(container, "#uh-provider-select"),
    actorSelect: required(container, "#uh-actor-select"),
    fullscreen: required(container, "#uh-fullscreen"),
    restart: required(container, "#uh-restart"),
    pageHost: required(container, "#uh-page"),
    surfaceHost: required(container, "#uh-surfaces"),
    overlayHost: required(container, "#uh-overlay"),
  };
}
