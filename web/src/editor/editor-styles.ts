export const EDITOR_STYLES = `
  .uhura-editor {
    --navigator-width: 240px;
    --inspector-width: 272px;
    --focus-header-height: 0px;
    --ruler-size: 20px;
    --panel: #fff;
    --stage: #d3d8dd;
    --border: #e3e5e8;
    --ink: #20242a;
    --muted: #69717d;
    --faint: #98a1ad;
    --hover: #f2f4f6;
    --accent: #0d99ff;
    --accent-soft: #e9f4ff;
    position: fixed;
    inset: 0;
    overflow: hidden;
    color: var(--ink);
    background: var(--panel);
    font: 13px/1.45 Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }
  .uhura-editor.is-focus-mode {
    --navigator-width: 0px;
    --inspector-width: 340px;
    --focus-header-height: 42px;
  }
  .uhura-editor, .uhura-editor * { box-sizing: border-box; }
  .uhura-editor button, .uhura-editor input { color: inherit; font: inherit; }
  .uhura-editor [hidden] { display: none !important; }
  .uhura-editor button:focus-visible,
  .uhura-editor input:focus-visible,
  .uhura-editor a:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .editor-navigator,
  .editor-inspector {
    position: absolute;
    inset-block: 0;
    z-index: 60;
    display: flex;
    flex-direction: column;
    min-block-size: 0;
    background: var(--panel);
  }
  .editor-navigator {
    inset-inline-start: 0;
    inline-size: var(--navigator-width);
    border-inline-end: 1px solid var(--border);
  }
  .uhura-editor.is-focus-mode .editor-navigator { display: none; }
  .editor-inspector {
    inset-inline-end: 0;
    inline-size: var(--inspector-width);
    border-inline-start: 1px solid var(--border);
    overflow-y: auto;
  }
  .editor-inspector > .panel-heading {
    position: sticky;
    inset-block-start: 0;
    z-index: 2;
    flex: none;
    justify-content: flex-end;
    background: var(--panel);
  }
  .editor-navigator > .panel-heading > strong {
    min-inline-size: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }
  .panel-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-block-size: 62px;
    padding: 12px 14px;
    border-block-end: 1px solid var(--border);
  }
  .panel-heading h1, .panel-heading h2 { margin: 0; font-size: 14px; line-height: 1.25; }
  .panel-heading > span { color: var(--faint); font-size: 10px; }
  .play-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-block-size: 30px;
    padding: 0 10px;
    border-radius: 7px;
    color: #fff;
    background: #111827;
    text-decoration: none;
    font-size: 11px;
    font-weight: 650;
  }
  .play-link svg { inline-size: 12px; block-size: 12px; fill: currentColor; }
  .focus-header {
    position: absolute;
    inset: 0 0 auto;
    z-index: 55;
    display: flex;
    align-items: center;
    gap: 10px;
    block-size: 42px;
    padding: 0 12px;
    border-block-end: 1px solid var(--border);
    background: rgb(255 255 255 / 96%);
    backdrop-filter: blur(12px);
  }
  .focus-exit {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-block-size: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: 7px;
    color: #4f5864;
    background: transparent;
    font-size: 11px;
    font-weight: 650;
    cursor: pointer;
  }
  .focus-exit:hover { color: #20242a; background: var(--hover); }
  .focus-exit svg { inline-size: 14px; block-size: 14px; fill: none; stroke: currentColor; stroke-width: 1.4; stroke-linecap: round; stroke-linejoin: round; }
  .focus-breadcrumb {
    display: flex;
    align-items: center;
    gap: 7px;
    min-inline-size: 0;
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    white-space: nowrap;
  }
  .focus-breadcrumb-kind { color: var(--faint); text-transform: capitalize; }
  .focus-breadcrumb-separator { color: #b2b8c0; }
  .focus-breadcrumb-subject,
  .focus-breadcrumb-example { overflow: hidden; text-overflow: ellipsis; }
  .focus-breadcrumb-subject { color: var(--ink); font-weight: 650; }

  .navigator-search {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 10px 12px 8px;
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: #f8f9fa;
  }
  .navigator-search:focus-within { border-color: #8ccaff; box-shadow: 0 0 0 2px var(--accent-soft); }
  .navigator-search svg { flex: none; inline-size: 14px; block-size: 14px; fill: none; stroke: var(--faint); stroke-width: 1.35; }
  .navigator-search input { min-inline-size: 0; inline-size: 100%; padding: 0; border: 0; outline: 0; background: transparent; font-size: 12px; }
  .navigator-search input::placeholder { color: var(--faint); }
  .navigator-results { flex: 1; min-block-size: 0; overflow-y: auto; padding: 4px 8px 16px; scrollbar-width: thin; }
  .navigator-group { margin-block-end: 10px; }
  .navigator-row, .navigator-frame {
    display: flex;
    align-items: center;
    inline-size: 100%;
    border: 0;
    border-radius: 6px;
    background: transparent;
    text-align: start;
    cursor: pointer;
  }
  .navigator-row { gap: 7px; min-block-size: 30px; padding: 5px 6px; font-weight: 650; }
  .navigator-row:hover, .navigator-frame:hover { background: var(--hover); }
  .navigator-kind { inline-size: 13px; block-size: 13px; border: 1.2px solid #89929e; border-radius: 3px; }
  .navigator-kind[data-kind="surface"] { border-radius: 6px 6px 3px 3px; }
  .navigator-kind[data-kind="component"] { transform: rotate(45deg) scale(.72); border-radius: 2px; }
  .navigator-row-title, .navigator-frame-title { min-inline-size: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .navigator-count { color: var(--faint); font-size: 10px; font-weight: 500; }
  .navigator-frames { padding-inline-start: 13px; }
  .navigator-frame { gap: 7px; min-block-size: 27px; padding: 4px 7px; color: #4f5864; font-size: 12px; }
  .navigator-frame[aria-pressed="true"] { color: #075f9d; background: var(--accent-soft); }
  .navigator-frame.is-focus-target { box-shadow: inset 2px 0 #6554c0; }
  .navigator-frame-icon { inline-size: 9px; block-size: 11px; border: 1px solid #a4acb6; border-radius: 2px; background: #fff; }
  .navigator-derived { color: #6a5acd; font-size: 9px; font-weight: 750; }
  .navigator-default { inline-size: 5px; block-size: 5px; border-radius: 999px; background: var(--accent); }
  .navigator-empty { padding: 24px 10px; color: var(--faint); text-align: center; font-size: 12px; }
  .editor-stage {
    position: absolute;
    inset-block: 0;
    inset-inline: var(--navigator-width) var(--inspector-width);
    min-inline-size: 0;
    min-block-size: 0;
    background: var(--stage);
    transition: inset-inline-start .18s ease, inset-inline-end .18s ease;
  }
  .ruler-corner, .canvas-ruler { position: absolute; z-index: 45; background: #f7f8f9; pointer-events: none; }
  .ruler-corner { inset: var(--focus-header-height) auto auto 0; inline-size: var(--ruler-size); block-size: var(--ruler-size); border-inline-end: 1px solid #cbd0d6; border-block-end: 1px solid #cbd0d6; }
  .ruler-x { inset: var(--focus-header-height) 0 auto var(--ruler-size); inline-size: calc(100% - var(--ruler-size)); block-size: var(--ruler-size); border-block-end: 1px solid #cbd0d6; }
  .ruler-y { inset: calc(var(--focus-header-height) + var(--ruler-size)) auto 0 0; inline-size: var(--ruler-size); block-size: calc(100% - var(--focus-header-height) - var(--ruler-size)); border-inline-end: 1px solid #cbd0d6; }
  .editor-viewport {
    position: absolute;
    inset: calc(var(--focus-header-height) + var(--ruler-size)) 0 0 var(--ruler-size);
    overflow: hidden;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    background: var(--stage);
    cursor: default;
  }
  .editor-viewport[data-tool="hand"] { cursor: grab; }
  .editor-viewport.panning { cursor: grabbing; user-select: none; }
  .editor-viewport:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .editor-board { position: absolute; transform-origin: 0 0; padding: 46px 40px 120px; }
  .editor-board.is-focus-mode > .preview-row:not(.is-focus-row),
  .editor-board.is-focus-mode .editor-frame:not(.is-focus-target) { display: none; }
  .editor-board.is-focus-mode > .preview-row.is-focus-row { margin-block-end: 0; }
  .editor-board.is-focus-mode > .preview-row.is-focus-row > .row-title { display: none; }

  .annotation-overlay {
    position: absolute;
    inset: 0;
    z-index: 40;
    overflow: hidden;
    pointer-events: none;
  }
  .annotation-leaders, .annotation-controls { position: absolute; inset: 0; inline-size: 100%; block-size: 100%; pointer-events: none; }
  .annotation-leaders { overflow: visible; }
  .annotation-leaders line { stroke: rgb(77 61 145 / 65%); stroke-width: 1.25; stroke-dasharray: 3 3; vector-effect: non-scaling-stroke; }
  .annotation-leaders line.is-active { stroke: #4c3cb3; stroke-width: 1.75; }
  .annotation-highlight { fill: rgb(101 84 192 / 10%); stroke: #6554c0; stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  .annotation-highlight.is-preview-active { fill: rgb(101 84 192 / 15%); stroke-width: 2; }
  .annotation-highlight.is-active { fill: rgb(76 60 179 / 20%); stroke: #4c3cb3; stroke-width: 2.5; }
  .annotation-marker, .annotation-card { position: absolute; inset: 0 auto auto 0; }
  .annotation-marker {
    inline-size: 22px;
    block-size: 22px;
    margin: -11px 0 0 -11px;
    padding: 0;
    border: 2px solid #fff;
    border-radius: 999px;
    color: #fff;
    background: #6554c0;
    box-shadow: 0 2px 8px rgb(30 27 75 / 32%);
    font-size: 10px;
    font-weight: 750;
    cursor: pointer;
    pointer-events: auto;
  }
  .annotation-marker:focus-visible { outline: 2px solid #312e81; outline-offset: 2px; }
  .annotation-marker.is-preview-active { box-shadow: 0 0 0 3px rgb(101 84 192 / 28%), 0 2px 8px rgb(30 27 75 / 38%); }
  .annotation-marker.is-active { background: #4c3cb3; box-shadow: 0 0 0 4px rgb(101 84 192 / 38%), 0 3px 10px rgb(30 27 75 / 44%); }
  .annotation-card {
    inline-size: 260px;
    max-block-size: min(320px, calc(100% - 24px));
    overflow: auto;
    padding: 11px;
    border: 1px solid #d8d2f2;
    border-radius: 9px;
    color: #312e46;
    background: rgb(255 255 255 / 98%);
    box-shadow: 0 10px 28px rgb(30 27 75 / 18%);
    pointer-events: none;
    user-select: none;
  }
  .annotation-card button { pointer-events: auto; }
  .annotation-card.is-gutter { border-color: #bbb2e7; }
  .annotation-card.is-revealed { border-color: #a99ee0; }
  .annotation-card-heading, .source-entry-heading, .source-drawer-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; }
  .annotation-card-heading strong { flex: 1; min-inline-size: 0; color: #706985; font-size: 9px; font-weight: 650; overflow-wrap: anywhere; }
  .annotation-card-heading .source-location { flex: 0 1 150px; max-inline-size: 150px; }
  .annotation-entry-list { display: grid; gap: 8px; margin: 9px 0 0; padding: 0; list-style: none; }
  .annotation-entry { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: start; gap: 7px; }
  .annotation-kind { padding: 2px 5px; border-radius: 999px; color: #5543a5; background: #eeebff; font: 8px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; }
  .annotation-text { margin: 0 !important; color: inherit !important; font-size: 13px !important; line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; }
  .source-entry-heading { display: grid; justify-content: stretch; }
  .source-entry-actions { display: flex; align-items: center; justify-content: flex-start; flex-wrap: wrap; gap: 5px; min-inline-size: 0; inline-size: 100%; }
  .source-entry-actions .source-location { flex: 1 1 180px; }
  .source-target-select, .source-location { padding: 3px 6px; border: 1px solid #ddd9ee; border-radius: 5px; color: #5543a5; background: #fff; font-size: 8px; cursor: pointer; }
  .source-target-select { font-weight: 700; }
  .source-location { position: relative; max-inline-size: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: copy; }
  .source-location::after { content: "Copy"; position: absolute; inset: 0; display: grid; place-items: center; border-radius: inherit; color: #fff; background: #6554c0; opacity: 0; transition: opacity .12s ease; }
  .source-location:not(:disabled):is(:hover, :focus-visible)::after { opacity: 1; }
  .source-location:disabled { color: #9a93a8; background: #f4f2f6; cursor: not-allowed; }
  .source-target-select:disabled { color: #9a93a8; background: #f4f2f6; cursor: not-allowed; }
  .annotation-overlay.is-stale .annotation-marker { background: #756c82; }
  .annotation-overlay.is-stale .annotation-card { border-style: dashed; filter: saturate(.65); }

  .workflow-connectors {
    position: absolute;
    inset: 0;
    z-index: 1;
    overflow: visible;
    color: var(--accent);
    pointer-events: none;
  }
  .workflow-connector { transition: opacity .12s ease; }
  .workflow-connector.opens-surface { color: #6d4fc2; }
  .workflow-connectors.has-selection .workflow-connector { opacity: .16; }
  .workflow-connectors.has-selection .workflow-connector.is-active { opacity: 1; }
  .workflow-connector-path {
    fill: none;
    stroke: currentcolor;
    stroke-width: var(--connector-stroke, 1.5px);
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .workflow-connector-arrow { fill: currentcolor; }
  .workflow-connector-origin {
    fill: var(--stage);
    stroke: currentcolor;
    stroke-width: var(--connector-stroke, 1.5px);
  }
  .workflow-connector-label {
    fill: currentcolor;
    stroke: var(--stage);
    stroke-width: 4px;
    paint-order: stroke;
    text-anchor: middle;
    font: 10px/1 ui-monospace, SFMono-Regular, Menlo, monospace;
  }

  .canvas-tools {
    position: absolute;
    inset: auto auto 18px 50%;
    z-index: 50;
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 5px;
    border: 1px solid rgb(27 35 46 / 12%);
    border-radius: 12px;
    background: rgb(255 255 255 / 96%);
    box-shadow: 0 8px 26px rgb(31 41 55 / 18%);
    transform: translateX(-50%);
    backdrop-filter: blur(12px);
  }
  .canvas-tool, .canvas-zoom {
    display: inline-grid;
    place-items: center;
    block-size: 30px;
    padding: 0;
    border: 0;
    border-radius: 7px;
    background: transparent;
    cursor: pointer;
  }
  .canvas-tool { inline-size: 30px; color: #555e69; }
  .canvas-tool:hover:not(:disabled), .canvas-zoom:hover { color: #111827; background: var(--hover); }
  .canvas-tool[aria-pressed="true"] { color: #fff; background: var(--accent); }
  .canvas-tool:disabled { opacity: .3; cursor: default; }
  .canvas-tool svg { inline-size: 16px; block-size: 16px; fill: currentColor; }
  .canvas-tool svg path[fill="none"], .canvas-tool.stroke svg { fill: none; stroke: currentColor; stroke-width: 1.4; stroke-linecap: round; stroke-linejoin: round; }
  .canvas-zoom { min-inline-size: 46px; padding-inline: 6px; color: #555e69; font: 11px/1 ui-monospace, SFMono-Regular, Menlo, monospace; }
  .tool-divider { inline-size: 1px; block-size: 20px; margin-inline: 3px; background: var(--border); }

  .preview-row { position: relative; z-index: 2; margin-block-end: 62px; }
  .row-title { margin: 0 0 14px; color: #626b76; font-size: 11px; font-weight: 650; letter-spacing: .08em; text-transform: uppercase; }
  .row-frames { display: flex; align-items: flex-start; gap: 32px; padding-block-start: var(--workflow-rail-height, 0); }
  .editor-frame { flex: none; margin: 0; color: var(--ink); }
  .editor-frame.is-selected > .preview-shell,
  .editor-frame:focus-visible > .preview-shell { outline: var(--selection-stroke, 2px) solid var(--accent); outline-offset: var(--selection-offset, 4px); }
  .editor-frame:focus-visible { outline: none; }
  .editor-frame.is-related > .preview-shell {
    outline: var(--selection-stroke, 2px) dashed color-mix(in srgb, var(--accent) 72%, transparent);
    outline-offset: var(--selection-offset, 4px);
  }
  .preview-shell { position: relative; overflow: hidden; border: 1px solid rgb(26 35 47 / 16%); border-radius: 8px; color: #16181c; background: #fff; box-shadow: none; }
  .preview-shell.device { inline-size: 390px; block-size: 844px; }
  .preview-shell.sheet { inline-size: 390px; block-size: 560px; }
  .preview-shell.component { inline-size: 390px; min-block-size: 48px; background: #fff radial-gradient(circle, #d8d8de 1px, transparent 1px); background-size: 16px 16px; }
  .preview-shadow-host { display: block; inline-size: 100%; block-size: 100%; }
  .editor-frame figcaption { margin-block-start: 10px; max-inline-size: 390px; color: #343b44; }
  .caption-title { margin-inline-end: 6px; font-size: 12px; font-weight: 650; }
  .caption-prov { display: block; color: #747d89; font-size: 11px; }
  .caption-note { margin: 2px 0 0; color: #68717d; font-size: 11px; }
  .badge { margin-inline-start: 6px; padding: 2px 6px; border-radius: 999px; font-size: 9px; font-weight: 650; }
  .badge-default { color: #075f9d; background: var(--accent-soft); }
  .badge-pinned { color: #715d13; background: #fff7d6; }
  .badge-in-flight { color: #73510d; background: #fff0cc; }
  .badge-surface { color: #5b3fa7; background: #eee9ff; }
  .badge-surface[data-relation="direct"]::before { content: "↳ "; }
  .badge-surface[data-relation="inherited"]::before { content: "↪ "; }
  .badge-surface[data-relation="mounted"]::before { content: "◇ "; }

  .inspector-section { padding: 16px 14px 24px; }
  .inspector-hero { display: flex; align-items: center; gap: 10px; padding-block-end: 16px; border-block-end: 1px solid var(--border); }
  .inspector-hero-icon { display: inline-grid; place-items: center; inline-size: 34px; block-size: 34px; border-radius: 9px; color: #fff; background: #111827; font-weight: 760; }
  .inspector-hero div { display: flex; flex-direction: column; }
  .inspector-hero strong { font-size: 13px; }
  .inspector-hero span { color: var(--faint); font-size: 10px; }
  .inspector-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; margin-block: 16px; }
  .inspector-grid > div { padding: 10px; border: 1px solid var(--border); border-radius: 8px; background: #fafbfc; }
  .inspector-grid dt { color: var(--faint); font-size: 9px; text-transform: uppercase; letter-spacing: .06em; }
  .inspector-grid dd { margin: 2px 0 0; font-size: 16px; font-weight: 680; }
  .inspector-callout { padding: 12px; border-radius: 8px; color: #425466; background: #f3f7fa; }
  .inspector-callout strong { font-size: 11px; }
  .inspector-callout p { margin: 4px 0 0; color: #657181; font-size: 11px; }
  .inspector-callout code { font: 10px ui-monospace, SFMono-Regular, Menlo, monospace; }
  .selection-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; padding-block-end: 14px; border-block-end: 1px solid var(--border); }
  .selection-heading h2 { margin: 3px 0 0; font-size: 14px; overflow-wrap: anywhere; }
  .selection-kind { color: #0878c9; font-size: 9px; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; }
  .icon-button { display: inline-grid; place-items: center; inline-size: 30px; block-size: 30px; padding: 0; border: 0; border-radius: 7px; background: transparent; cursor: pointer; }
  .icon-button:hover { background: var(--hover); }
  .icon-button svg { inline-size: 16px; block-size: 16px; fill: none; stroke: currentColor; stroke-width: 1.35; }
  .property-list > div { display: grid; grid-template-columns: 74px minmax(0, 1fr); gap: 8px; padding-block: 9px; border-block-end: 1px solid #eef0f2; }
  .property-list dt { color: var(--faint); font-size: 11px; }
  .property-list dd { min-inline-size: 0; margin: 0; color: #343b44; font-size: 11px; overflow-wrap: anywhere; }
  .property-list .selection-source { display: block; inline-size: 100%; padding: 4px 6px; text-align: start; font: 9px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; }
  .inspector-block { margin-block-start: 18px; }
  .inspector-block h3 { margin: 0 0 8px; color: #59616c; font-size: 10px; letter-spacing: .08em; text-transform: uppercase; }
  .inspector-block p { color: #626c78; font-size: 11px; overflow-wrap: anywhere; }
  .selection-documentation:has(> .selection-documentation-content[hidden]) { display: none; }
  .selection-documentation-content { display: grid; gap: 10px; }
  .source-entry { padding: 10px; border: 1px solid #e6e3ef; border-radius: 8px; background: #fcfbff; }
  .source-entry + .source-entry { margin-block-start: 9px; }
  .source-entry h3, .source-entry h4 { margin: 0; color: #6b6576; font-size: 9px; font-weight: 650; overflow-wrap: anywhere; }
  .source-doc-text { margin: 7px 0 0 !important; color: #37323f !important; font-size: 13px !important; line-height: 1.55; white-space: pre-wrap; }
  .source-render-status { margin: 8px 0 0 !important; color: #81798d !important; font-size: 8px !important; }
  .inspector-block-intro { margin: -3px 0 10px; color: #68717d; }
  .preview-data-group + .preview-data-group { margin-block-start: 14px; }
  .preview-data-group h4 { margin: 0 0 4px; color: #68717d; font-size: 11px; font-weight: 650; }
  .preview-data-list { display: grid; margin: 0; }
  .preview-data-row { padding-block: 9px; border-block-end: 1px solid #eef0f2; }
  .preview-data-row:first-child { border-block-start: 1px solid #eef0f2; }
  .preview-data-row > dt { color: #59616c; font-size: 10px; }
  .preview-data-row > dd { min-inline-size: 0; margin: 3px 0 0; color: #2f3742; font-size: 11px; overflow-wrap: anywhere; }
  .preview-data-value, .preview-data-state { display: block; font-weight: 600; line-height: 1.45; white-space: pre-wrap; }
  .preview-data-reason { display: block; margin-block-start: 2px; color: #7b4651; line-height: 1.4; }
  .preview-data-source { margin: 3px 0 0 !important; color: #68717d !important; line-height: 1.4; }
  .workflow-step-list { display: grid; gap: 8px; margin: 0; padding: 0; list-style: none; }
  .workflow-step { padding: 9px; border: 1px solid #d9e4ec; border-radius: 8px; background: #f8fbfd; }
  .workflow-step-heading { display: grid; grid-template-columns: 18px minmax(0, 1fr) auto; align-items: center; gap: 6px; color: #253343; font: 10px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; }
  .workflow-step-heading strong { min-inline-size: 0; overflow-wrap: anywhere; }
  .workflow-step-ordinal { display: grid; place-items: center; inline-size: 18px; block-size: 18px; border-radius: 999px; color: #fff; background: var(--accent); font-size: 9px; font-weight: 700; }
  .workflow-step-kind { padding: 2px 5px; border-radius: 999px; color: #65717e; background: #e9eff4; font-size: 8px; text-transform: uppercase; }
  .workflow-dispatch { margin-block-start: 8px; color: #34485b; font: 9px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; overflow-wrap: anywhere; }
  .workflow-guards { display: flex; flex-wrap: wrap; gap: 4px; margin: 6px 0 0; padding: 0; list-style: none; }
  .workflow-guards li { padding: 2px 5px; border-radius: 999px; color: #6f4b14; background: #fff1c9; font: 8px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; }
  .workflow-guards li[data-result="satisfied"] { color: #24623c; background: #dff4e7; }
  .workflow-guards li[data-result="not-ready"] { color: #7b4651; background: #fbe5e9; }
  .workflow-detail { margin-block-start: 7px; border-block-start: 1px solid #e5ebef; padding-block-start: 6px; }
  .workflow-detail summary { cursor: pointer; color: #596a79; font-size: 9px; font-weight: 650; }
  .workflow-detail pre { max-block-size: 180px; margin: 6px 0 0; padding: 7px; overflow: auto; border-radius: 5px; color: #2e3a47; background: #eef3f6; font: 8px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
  .workflow-no-effects { margin: 7px 0 0 !important; color: #84909b !important; font-size: 9px !important; }
  .surface-hierarchy, .surface-hierarchy ul { margin: 0; padding: 0; list-style: none; }
  .surface-hierarchy-root { position: relative; padding: 7px 8px; border: 1px solid #dbe1e7; border-radius: 7px; color: #34404c; background: #fafbfc; font: 10px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; }
  .surface-hierarchy-root > ul { display: grid; gap: 5px; margin-block-start: 7px; padding-inline-start: 17px; }
  .surface-hierarchy-child { position: relative; display: flex; flex-direction: column; gap: 1px; padding: 6px 7px; border: 1px solid #ddd5f7; border-radius: 6px; color: #4c378c; background: #f7f4ff; }
  .surface-hierarchy-child::before { content: ""; position: absolute; inset: 50% 100% auto auto; inline-size: 13px; border-block-start: 1px solid #9a86d2; }
  .surface-hierarchy-child > ul { display: grid; gap: 5px; margin-block-start: 5px; padding-inline-start: 17px; }
  .surface-hierarchy-child strong { font-size: 9px; }
  .surface-hierarchy-child span { color: #7866aa; font-size: 8px; }
  .interaction-list { display: grid; gap: 6px; padding: 0; list-style: none; }
  .interaction-list li { padding: 8px 9px; border: 1px solid #d9e8f4; border-radius: 7px; color: #315a75; background: #f4faff; font: 10px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; overflow-wrap: anywhere; }
  .inspector-muted { color: var(--faint) !important; font-style: italic; }
  .visually-hidden { position: absolute !important; inline-size: 1px !important; block-size: 1px !important; padding: 0 !important; margin: -1px !important; overflow: hidden !important; clip: rect(0 0 0 0) !important; white-space: nowrap !important; border: 0 !important; }

  .editor-source-drawer {
    position: absolute;
    inset: 12px 12px 12px auto;
    z-index: 90;
    display: flex;
    flex-direction: column;
    inline-size: min(360px, calc(100% - 24px));
    min-block-size: 0;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: rgb(255 255 255 / 98%);
    box-shadow: 0 16px 44px rgb(31 41 55 / 24%);
    backdrop-filter: blur(14px);
  }
  .source-drawer-heading { flex: none; padding: 12px 12px 10px; border-block-end: 1px solid var(--border); }
  .source-drawer-heading > div { display: flex; flex-direction: column; }
  .source-drawer-heading strong { font-size: 13px; }
  .source-drawer-heading span { color: var(--faint); font-size: 10px; }
  .source-panel { min-block-size: 0; overflow-y: auto; padding: 12px; user-select: text; }
  .source-owner-group + .source-owner-group { margin-block-start: 18px; }
  .source-owner-heading { margin-block-end: 8px; padding-block-end: 7px; border-block-end: 1px solid var(--border); }
  .source-owner-heading h2 { margin: 2px 0 0; color: #514b59; font-size: 11px; overflow-wrap: anywhere; }
  .source-owner-kind { color: #786ea0; font-size: 8px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
  .source-panel.is-stale::before { content: "Stale render — source actions are unavailable"; display: block; margin-block-end: 10px; padding: 7px 8px; border-radius: 6px; color: #6e5a2d; background: #fff7df; font-size: 10px; }

  .editor-status {
    position: absolute;
    inset: 14px auto auto 50%;
    z-index: 100;
    inline-size: min(540px, calc(100% - 28px));
    max-block-size: min(430px, calc(100% - 28px));
    overflow: auto;
    padding: 12px 14px;
    border: 1px solid #e5b45a;
    border-radius: 10px;
    color: #382a14;
    background: rgb(255 249 235 / 97%);
    box-shadow: 0 12px 32px rgb(51 38 14 / 18%);
    transform: translateX(-50%);
    backdrop-filter: blur(12px);
  }
  .editor-status[data-tone="error"] { border-color: #e2a0a0; color: #4b2020; background: rgb(255 245 245 / 97%); }
  .editor-status[data-tone="neutral"] { border-color: #cbd3dc; color: #35404c; background: rgb(248 250 252 / 97%); }
  .status-heading { display: flex; align-items: flex-start; gap: 12px; }
  .status-copy { min-inline-size: 0; flex: 1; }
  .status-copy strong { display: block; font-size: 13px; }
  .status-copy p { margin: 2px 0 0; color: inherit; opacity: .72; }
  .status-dismiss { flex: none; inline-size: 26px; block-size: 26px; padding: 0; border: 0; border-radius: 6px; background: transparent; font: 18px/1 sans-serif; cursor: pointer; }
  .status-dismiss:hover { background: rgb(0 0 0 / 7%); }
  .diagnostic-list { display: grid; gap: 8px; margin: 11px 0 0; padding: 0; list-style: none; }
  .diagnostic-list li { padding-block-start: 8px; border-block-start: 1px solid rgb(0 0 0 / 12%); overflow-wrap: anywhere; }

  .empty-board { min-inline-size: 500px; padding: 80px 60px; color: #69717d; }
  .empty-board h2 { margin: 0 0 6px; color: #343b44; font-size: 18px; }
  .empty-board p { margin: 0; }

  .uhura-editor.ui-hidden .editor-navigator,
  .uhura-editor.ui-hidden .editor-inspector,
  .uhura-editor.ui-hidden .editor-source-drawer,
  .uhura-editor.ui-hidden .focus-header,
  .uhura-editor.ui-hidden .canvas-tools,
  .uhura-editor.ui-hidden .annotation-overlay,
  .uhura-editor.ui-hidden .ruler-corner,
  .uhura-editor.ui-hidden .canvas-ruler { display: none; }
  .uhura-editor.ui-hidden .editor-stage { inset-inline: 0; }
  .uhura-editor.ui-hidden .editor-viewport { inset: 0; }

  @media (max-width: 1199px) {
    .editor-inspector { display: none; }
    .editor-stage { inset-inline-end: 0; }
    .uhura-editor.is-focus-mode {
      --inspector-width: min(320px, 40vw);
    }
    .uhura-editor.is-focus-mode .editor-inspector { display: flex; }
    .uhura-editor.is-focus-mode .editor-stage { inset-inline-end: var(--inspector-width); }
  }
  @media (max-width: 799px) {
    .editor-navigator { display: none; }
    .editor-stage { inset-inline-start: 0; }
  }
`;

export const PREVIEW_BASE_STYLES = `
  :host, #uh-app { display: block; inline-size: 100%; block-size: 100%; color: #16181c; }
  *, *::before, *::after { box-sizing: border-box; }
  .screen-root, .fragment-root { position: relative; inline-size: 100%; block-size: 100%; overflow: hidden; }
  .screen-root { isolation: isolate; }
  .screen-root > * { block-size: 100%; }
  .fragment-root > * { min-inline-size: 0; }
  .uh-view { display: block; min-inline-size: 0; }
  .uh-scroll { overflow-y: auto; overflow-x: hidden; min-block-size: 0; }
  .uh-scroll[data-direction="horizontal"] { overflow-x: auto; overflow-y: hidden; }
  .uh-text { margin: 0; overflow-wrap: anywhere; }
  .uh-image { display: block; background-size: cover; background-position: center; background-color: #d9d9de; }
  .uh-video { display: block; inline-size: 100%; background: #111 center / cover no-repeat; object-fit: cover; }
  .uh-icon { display: inline-flex; }
  .uh-icon svg { display: block; }
  button.uh-button { appearance: none; display: inline-flex; align-items: center; gap: 6px; padding: 6px; border: 0; border-radius: 8px; color: inherit; background: none; font: inherit; }
  button.uh-button[disabled] { opacity: .35; }
  button.uh-button[aria-busy="true"] { opacity: .6; }
  .uh-text-field input { inline-size: 100%; padding: 8px 14px; border: 1px solid #d5d5da; border-radius: 999px; color: #222; background: #fff; font: inherit; }
  .uh-region { display: block; }
  .uh-pager { position: relative; }
  .uh-pager .uh-track { display: flex; overflow-x: auto; scroll-snap-type: x mandatory; }
  .uh-pager .uh-track > * { flex: 0 0 100%; scroll-snap-align: center; }
  .uh-dots { position: absolute; inset-block-end: 10px; inset-inline: 0; display: flex; justify-content: center; gap: 5px; }
  .uh-dot { inline-size: 6px; block-size: 6px; border-radius: 999px; background: rgb(255 255 255 / 55%); }
  .uh-dot.on { background: #fff; }
  .uh-surface-overlay { position: absolute; inset: 0; display: flex; flex-direction: column; justify-content: flex-end; isolation: isolate; }
  .uh-scrim { position: absolute; inset: 0; z-index: 0; background: rgb(0 0 0 / 40%); }
  .uh-surface { position: relative; z-index: 1; block-size: 72%; max-block-size: 72%; overflow: hidden; border-radius: 16px 16px 0 0; background: #fff; box-shadow: 0 -8px 32px rgb(0 0 0 / 35%); }
`;

/**
 * Maps document-root selectors onto the explicit application boundary inside
 * each preview shadow root. The stylesheet is parsed by the browser first;
 * callers apply this only to CSSStyleRule.selectorText, never declaration
 * values, strings, comments, keyframe names, or at-rule preludes.
 */
export const scopePreviewSelector = (selector: string): string =>
  selector.replace(
    /(^|[\s>+~,(])(:root|html|body)(?=$|[\s>+~,.#:[\])])/g,
    (_match, boundary: string, root: string) =>
      `${boundary}${root === "body" ? "#uh-app" : ":host"}`,
  );

type RuleWithSelector = CSSRule & { selectorText: string };
type RuleWithChildren = CSSRule & { cssRules: CSSRuleList };

const hasSelector = (rule: CSSRule): rule is RuleWithSelector =>
  "selectorText" in rule && typeof rule.selectorText === "string";

const hasChildren = (rule: CSSRule): rule is RuleWithChildren =>
  "cssRules" in rule && rule.cssRules !== undefined;

/** Scopes every style rule, including rules nested in media/supports/layers. */
export const scopePreviewStylesheet = (sheet: CSSStyleSheet): void => {
  const visit = (rules: CSSRuleList): void => {
    for (const rule of rules) {
      if (hasSelector(rule)) rule.selectorText = scopePreviewSelector(rule.selectorText);
      if (hasChildren(rule)) visit(rule.cssRules);
    }
  };
  visit(sheet.cssRules);
};

/**
 * Parses and adapts one immutable application stylesheet per Editor model.
 * Constructing it in the target document's realm keeps it shareable by every
 * preview ShadowRoot in that document and avoids detached <style> lifecycle
 * behavior entirely.
 */
export const preparePreviewStylesheet = (
  document: Document,
  applicationStylesheet: string,
): CSSStyleSheet => {
  const StyleSheet = document.defaultView?.CSSStyleSheet;
  if (!StyleSheet) throw new Error("Editor preview styles require a browser document");
  const sheet = new StyleSheet();
  sheet.replaceSync(`${PREVIEW_BASE_STYLES}\n${applicationStylesheet}`);
  scopePreviewStylesheet(sheet);
  return sheet;
};
