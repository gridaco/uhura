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
      <label class="uh-system-select uh-actor-select" for="uh-actor-select">
        <span>Actor</span>
        <select id="uh-actor-select" disabled><option>Starting…</option></select>
      </label>
      <button
        id="uh-debug-toggle"
        class="uh-debug-toggle"
        type="button"
        aria-label="Open runtime debugger"
        aria-controls="uh-debug-panel"
        aria-expanded="false"
        title="Open runtime debugger"
      >
        <svg aria-hidden="true" viewBox="0 0 16 16">
          <circle cx="4" cy="4" r="1.5"></circle>
          <circle cx="12" cy="4" r="1.5"></circle>
          <circle cx="8" cy="12" r="1.5"></circle>
          <path d="m5.25 4.75 1.9 5.8M10.75 4.75l-1.9 5.8M5.5 4h5"></path>
        </svg>
        <span>Debug</span>
      </button>
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

  <aside id="uh-debug-panel" hidden aria-labelledby="uh-debug-title">
    <div
      id="uh-debug-panel-resize"
      role="separator"
      aria-label="Resize runtime debugger"
      aria-orientation="vertical"
      tabindex="0"
    ></div>
    <header class="uh-debug-header">
      <h2 id="uh-debug-title">Runtime state machine</h2>
      <button
        id="uh-debug-close"
        type="button"
        aria-label="Close runtime debugger"
        title="Close runtime debugger"
      >
        <svg aria-hidden="true" viewBox="0 0 16 16">
          <path d="m3.5 3.5 9 9M12.5 3.5l-9 9"></path>
        </svg>
      </button>
    </header>

    <div class="uh-debug-controls">
      <label for="uh-debug-definition">
        <span>Machine</span>
        <select id="uh-debug-definition" disabled>
          <option value="">Waiting for program…</option>
        </select>
      </label>
      <button id="uh-debug-follow-live" type="button" aria-pressed="true">
        <svg aria-hidden="true" viewBox="0 0 16 16">
          <circle cx="8" cy="8" r="5.25"></circle>
          <circle cx="8" cy="8" r="1.5"></circle>
        </svg>
        Follow live
      </button>
    </div>

    <div class="uh-debug-graph-region">
      <div
        id="uh-debug-graph"
        role="region"
        aria-label="Runtime state machine graph"
        tabindex="0"
      >
        <div id="uh-debug-graph-content">
          <p class="uh-debug-empty">The graph will appear after Play starts.</p>
        </div>
      </div>
      <div class="uh-debug-zoom" role="group" aria-label="Graph zoom">
        <button
          id="uh-debug-zoom-out"
          type="button"
          aria-label="Zoom graph out"
          title="Zoom out"
        >
          <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3.5 8h9"></path></svg>
        </button>
        <button
          id="uh-debug-zoom-reset"
          type="button"
          aria-label="Reset graph zoom to 100%"
          title="Reset zoom to 100%"
        ><span id="uh-debug-zoom-level">100%</span></button>
        <button
          id="uh-debug-zoom-in"
          type="button"
          aria-label="Zoom graph in"
          title="Zoom in"
        >
          <svg aria-hidden="true" viewBox="0 0 16 16"><path d="M3.5 8h9M8 3.5v9"></path></svg>
        </button>
      </div>
    </div>

    <div
      id="uh-debug-details-resize"
      role="separator"
      aria-label="Resize selection details"
      aria-orientation="horizontal"
      tabindex="0"
    ></div>
    <section id="uh-debug-details" aria-labelledby="uh-debug-details-title">
      <h3 id="uh-debug-details-title">Selection</h3>
      <p>Select a state, event, or transition to inspect it.</p>
    </section>

    <output id="uh-debug-summary" aria-live="off" aria-atomic="true">
      Waiting for runtime inspection…
    </output>
  </aside>
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
  actorSelect: HTMLSelectElement;
  debugToggle: HTMLButtonElement;
  debugPanel: HTMLElement;
  debugPanelResize: HTMLElement;
  debugTitle: HTMLHeadingElement;
  debugClose: HTMLButtonElement;
  debugDefinition: HTMLSelectElement;
  debugFollowLive: HTMLButtonElement;
  debugSummary: HTMLOutputElement;
  debugGraph: HTMLElement;
  debugGraphContent: HTMLElement;
  debugZoomOut: HTMLButtonElement;
  debugZoomReset: HTMLButtonElement;
  debugZoomLevel: HTMLSpanElement;
  debugZoomIn: HTMLButtonElement;
  debugDetailsResize: HTMLElement;
  debugDetails: HTMLElement;
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
    actorSelect: required(container, "#uh-actor-select"),
    debugToggle: required(container, "#uh-debug-toggle"),
    debugPanel: required(container, "#uh-debug-panel"),
    debugPanelResize: required(container, "#uh-debug-panel-resize"),
    debugTitle: required(container, "#uh-debug-title"),
    debugClose: required(container, "#uh-debug-close"),
    debugDefinition: required(container, "#uh-debug-definition"),
    debugFollowLive: required(container, "#uh-debug-follow-live"),
    debugSummary: required(container, "#uh-debug-summary"),
    debugGraph: required(container, "#uh-debug-graph"),
    debugGraphContent: required(container, "#uh-debug-graph-content"),
    debugZoomOut: required(container, "#uh-debug-zoom-out"),
    debugZoomReset: required(container, "#uh-debug-zoom-reset"),
    debugZoomLevel: required(container, "#uh-debug-zoom-level"),
    debugZoomIn: required(container, "#uh-debug-zoom-in"),
    debugDetailsResize: required(container, "#uh-debug-details-resize"),
    debugDetails: required(container, "#uh-debug-details"),
    fullscreen: required(container, "#uh-fullscreen"),
    restart: required(container, "#uh-restart"),
    pageHost: required(container, "#uh-page"),
    surfaceHost: required(container, "#uh-surfaces"),
    overlayHost: required(container, "#uh-overlay"),
  };
}
