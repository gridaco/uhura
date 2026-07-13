(() => {
  const viewport = document.getElementById("viewport");
  const board = document.getElementById("board");
  const tools = document.querySelector(".canvas-tools");
  const cursorButton = document.getElementById("tool-cursor");
  const handButton = document.getElementById("tool-hand");
  const zoomOutput = document.getElementById("canvas-zoom");
  const zoomOutButton = document.getElementById("zoom-out");
  const zoomInButton = document.getElementById("zoom-in");
  const focusSelectionButton = document.getElementById("focus-selection");
  const navigator = document.getElementById("canvas-navigator");
  const navigatorSearch = document.getElementById("navigator-search");
  const navigatorEmpty = document.getElementById("navigator-empty");
  const rulerX = document.getElementById("ruler-x");
  const rulerY = document.getElementById("ruler-y");
  const inspectorOverview = document.getElementById("inspector-overview");
  const inspectorSelection = document.getElementById("inspector-selection");
  const clearSelectionButton = document.getElementById("clear-selection");
  const selectionKind = document.getElementById("selection-kind");
  const selectionName = document.getElementById("selection-name");
  const selectionSubject = document.getElementById("selection-subject");
  const selectionExample = document.getElementById("selection-example");
  const selectionSize = document.getElementById("selection-size");
  const selectionOrigin = document.getElementById("selection-origin");
  const selectionFromRow = document.getElementById("selection-from-row");
  const selectionFrom = document.getElementById("selection-from");
  const selectionStatus = document.getElementById("selection-status");
  const selectionNoteBlock = document.getElementById("selection-note-block");
  const selectionNote = document.getElementById("selection-note");
  const selectionInteractions = document.getElementById("selection-interactions");
  const selectionNoInteractions = document.getElementById("selection-no-interactions");

  const MIN_SCALE = 0.02;
  const MAX_SCALE = 3;
  const ZOOM_STEP = 1.2;
  const WHEEL_ZOOM_SENSITIVITY = 0.01;
  const UI_VISIBLE_KEY = "uhura.editor.ui-visible";
  let x = 0;
  let y = 0;
  let scale = 1;
  let selectedTool = "cursor";
  let selectedFrame = null;
  let spaceHeld = false;
  let pan = null;
  let pinch = null;
  let suppressClickUntil = 0;
  let rulerFrame = 0;
  const touches = new Map();

  const storedBoolean = (key, fallback) => {
    try {
      const value = window.localStorage.getItem(key);
      return value === null ? fallback : value === "true";
    } catch (_) {
      return fallback;
    }
  };
  const storeBoolean = (key, value) => {
    try {
      window.localStorage.setItem(key, String(value));
    } catch (_) {
      // Storage is only a convenience; the editor remains usable without it.
    }
  };
  const clampScale = (value) => Math.min(Math.max(value, MIN_SCALE), MAX_SCALE);
  const viewportCenter = () => ({ x: viewport.clientWidth / 2, y: viewport.clientHeight / 2 });
  const localPoint = (clientX, clientY) => {
    const rect = viewport.getBoundingClientRect();
    return { x: clientX - rect.left, y: clientY - rect.top };
  };

  const chooseRulerStep = () => {
    const desiredWorldUnits = 76 / scale;
    const magnitude = 10 ** Math.floor(Math.log10(desiredWorldUnits));
    const normalized = desiredWorldUnits / magnitude;
    const factor = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
    return factor * magnitude;
  };
  const prepareCanvas = (canvas) => {
    const rect = canvas.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    const width = Math.max(1, Math.round(rect.width));
    const height = Math.max(1, Math.round(rect.height));
    const pixelWidth = Math.round(width * ratio);
    const pixelHeight = Math.round(height * ratio);
    if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
      canvas.width = pixelWidth;
      canvas.height = pixelHeight;
    }
    const context = canvas.getContext("2d");
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.strokeStyle = "#aeb5be";
    context.fillStyle = "#68717d";
    context.lineWidth = 1;
    context.font = "9px ui-monospace, SFMono-Regular, Menlo, monospace";
    return { context, width, height };
  };
  const drawRulers = () => {
    rulerFrame = 0;
    const horizontal = prepareCanvas(rulerX);
    const vertical = prepareCanvas(rulerY);
    const step = chooseRulerStep();
    const minor = step / 5;

    const firstX = Math.floor((-x / scale) / minor) * minor;
    const lastX = (horizontal.width - x) / scale;
    horizontal.context.beginPath();
    for (let world = firstX; world <= lastX + minor; world += minor) {
      const screen = Math.round(x + world * scale) + 0.5;
      const major = Math.abs(world / step - Math.round(world / step)) < 0.001;
      horizontal.context.moveTo(screen, horizontal.height);
      horizontal.context.lineTo(screen, horizontal.height - (major ? 9 : 4));
      if (major) horizontal.context.fillText(String(Math.round(world)), screen + 3, 9);
    }
    horizontal.context.stroke();

    const firstY = Math.floor((-y / scale) / minor) * minor;
    const lastY = (vertical.height - y) / scale;
    vertical.context.beginPath();
    for (let world = firstY; world <= lastY + minor; world += minor) {
      const screen = Math.round(y + world * scale) + 0.5;
      const major = Math.abs(world / step - Math.round(world / step)) < 0.001;
      vertical.context.moveTo(vertical.width, screen);
      vertical.context.lineTo(vertical.width - (major ? 9 : 4), screen);
      if (major) {
        vertical.context.save();
        vertical.context.translate(9, screen - 3);
        vertical.context.rotate(-Math.PI / 2);
        vertical.context.fillText(String(Math.round(world)), 0, 0);
        vertical.context.restore();
      }
    }
    vertical.context.stroke();
  };
  const requestRulers = () => {
    if (!rulerFrame) rulerFrame = window.requestAnimationFrame(drawRulers);
  };
  const apply = () => {
    board.style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
    board.style.setProperty("--selection-stroke", `${2 / scale}px`);
    board.style.setProperty("--selection-offset", `${4 / scale}px`);
    zoomOutput.textContent = `${Math.round(scale * 100)}%`;
    requestRulers();
  };
  const zoomAt = (nextScale, point) => {
    nextScale = clampScale(nextScale);
    const ratio = nextScale / scale;
    x = point.x - (point.x - x) * ratio;
    y = point.y - (point.y - y) * ratio;
    scale = nextScale;
    apply();
  };

  const effectiveTool = () => selectedTool === "hand" || spaceHeld || pan ? "hand" : "cursor";
  const renderTools = () => {
    const activeTool = effectiveTool();
    cursorButton.setAttribute("aria-pressed", String(activeTool === "cursor"));
    handButton.setAttribute("aria-pressed", String(activeTool === "hand"));
    viewport.dataset.tool = activeTool;
  };
  const selectTool = (tool) => {
    selectedTool = tool;
    renderTools();
  };
  const finishPan = (pointerId) => {
    if (!pan || (pointerId !== undefined && pan.pointerId !== pointerId)) return;
    if (pan.moved) suppressClickUntil = performance.now() + 250;
    pan = null;
    viewport.classList.remove("panning");
    renderTools();
  };
  const beginPan = (pointerId, point) => {
    pan = { pointerId, pointerX: point.x, pointerY: point.y, x, y, moved: false };
    viewport.classList.add("panning");
    renderTools();
  };
  const updatePan = (pointerId, point) => {
    if (!pan || pan.pointerId !== pointerId) return;
    const deltaX = point.x - pan.pointerX;
    const deltaY = point.y - pan.pointerY;
    if (Math.hypot(deltaX, deltaY) > 3) pan.moved = true;
    x = pan.x + deltaX;
    y = pan.y + deltaY;
    apply();
  };

  const frameWorldRect = (element) => {
    const frameRect = element.getBoundingClientRect();
    const viewportRect = viewport.getBoundingClientRect();
    return {
      x: (frameRect.left - viewportRect.left - x) / scale,
      y: (frameRect.top - viewportRect.top - y) / scale,
      width: frameRect.width / scale,
      height: frameRect.height / scale,
    };
  };
  const revealElement = (element) => {
    const rect = frameWorldRect(element);
    x = viewport.clientWidth / 2 - (rect.x + rect.width / 2) * scale;
    y = viewport.clientHeight / 2 - (rect.y + rect.height / 2) * scale;
    apply();
  };
  const setInspectorText = (element, value) => {
    element.textContent = value || "—";
  };
  const clearSelection = () => {
    if (selectedFrame) {
      selectedFrame.classList.remove("is-selected");
      selectedFrame.setAttribute("aria-pressed", "false");
    }
    selectedFrame = null;
    document.querySelectorAll("[data-frame-target]").forEach((button) => {
      button.setAttribute("aria-pressed", "false");
      button.removeAttribute("aria-current");
    });
    inspectorOverview.hidden = false;
    inspectorSelection.hidden = true;
    focusSelectionButton.disabled = true;
  };
  const selectFrame = (frame, reveal = false) => {
    if (!frame) return;
    clearSelection();
    selectedFrame = frame;
    frame.classList.add("is-selected");
    frame.setAttribute("aria-pressed", "true");
    const navigatorButton = document.querySelector(`[data-frame-target="${CSS.escape(frame.id)}"]`);
    if (navigatorButton) {
      navigatorButton.setAttribute("aria-pressed", "true");
      navigatorButton.setAttribute("aria-current", "true");
      navigatorButton.scrollIntoView({ block: "nearest" });
    }

    inspectorOverview.hidden = true;
    inspectorSelection.hidden = false;
    focusSelectionButton.disabled = false;
    setInspectorText(selectionKind, frame.dataset.kind);
    setInspectorText(selectionName, `${frame.dataset.subject} / ${frame.dataset.example}`);
    setInspectorText(selectionSubject, frame.dataset.subject);
    setInspectorText(selectionExample, frame.dataset.example);
    const shell = frame.querySelector(":scope > .shell");
    setInspectorText(selectionSize, shell ? `${Math.round(shell.offsetWidth)} × ${Math.round(shell.offsetHeight)}` : "—");
    const origin = frame.dataset.pinned === "true"
      ? "Pinned example"
      : frame.dataset.derived === "true"
        ? "Replay-derived"
        : "Checked example";
    setInspectorText(selectionOrigin, origin);
    const from = frame.dataset.from.trim();
    selectionFromRow.hidden = !from;
    selectionFrom.textContent = from;
    const status = [];
    if (frame.dataset.default === "true") status.push("Default");
    const inFlight = Number(frame.dataset.inFlight || 0);
    status.push(inFlight > 0 ? `${inFlight} in flight` : "Settled");
    setInspectorText(selectionStatus, status.join(" · "));

    const note = frame.dataset.previewNote.trim();
    selectionNoteBlock.hidden = !note;
    selectionNote.textContent = note;
    const interactions = Array.from(frame.querySelectorAll(".shell [data-note]"))
      .map((element) => element.dataset.note.trim())
      .filter(Boolean)
      .filter((value, index, values) => values.indexOf(value) === index);
    selectionInteractions.replaceChildren(...interactions.map((interaction) => {
      const item = document.createElement("li");
      item.textContent = interaction;
      return item;
    }));
    selectionNoInteractions.hidden = interactions.length > 0;
    if (reveal) revealElement(frame);
  };

  const setUiVisible = (visible, persist = true) => {
    document.body.classList.toggle("ui-hidden", !visible);
    if (persist) storeBoolean(UI_VISIBLE_KEY, visible);
    requestRulers();
  };
  setUiVisible(storedBoolean(UI_VISIBLE_KEY, true), false);

  cursorButton.addEventListener("click", () => selectTool("cursor"));
  handButton.addEventListener("click", () => selectTool("hand"));
  zoomOutButton.addEventListener("click", () => zoomAt(scale / ZOOM_STEP, viewportCenter()));
  zoomInButton.addEventListener("click", () => zoomAt(scale * ZOOM_STEP, viewportCenter()));
  zoomOutput.addEventListener("click", () => zoomAt(1, viewportCenter()));
  focusSelectionButton.addEventListener("click", () => {
    if (selectedFrame) revealElement(selectedFrame);
  });
  clearSelectionButton.addEventListener("click", clearSelection);
  tools.addEventListener("pointerdown", (event) => event.stopPropagation());

  navigator.addEventListener("click", (event) => {
    const frameButton = event.target.closest("[data-frame-target]");
    if (frameButton) {
      selectFrame(document.getElementById(frameButton.dataset.frameTarget), true);
      return;
    }
    const rowButton = event.target.closest("[data-row-target]");
    if (rowButton) revealElement(document.getElementById(rowButton.dataset.rowTarget));
  });
  navigatorSearch.addEventListener("input", () => {
    const query = navigatorSearch.value.trim().toLocaleLowerCase();
    let visibleGroups = 0;
    document.querySelectorAll("[data-navigator-group]").forEach((group) => {
      const groupMatches = group.dataset.search.toLocaleLowerCase().includes(query);
      let visibleFrames = 0;
      group.querySelectorAll("[data-frame-target]").forEach((button) => {
        const matches = !query || groupMatches || button.dataset.search.toLocaleLowerCase().includes(query);
        button.hidden = !matches;
        if (matches) visibleFrames += 1;
      });
      const visible = !query || groupMatches || visibleFrames > 0;
      group.hidden = !visible;
      if (visible) visibleGroups += 1;
    });
    navigatorEmpty.hidden = visibleGroups > 0;
  });
  navigatorSearch.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !navigatorSearch.value) return;
    navigatorSearch.value = "";
    navigatorSearch.dispatchEvent(new Event("input"));
  });
  navigatorSearch.dispatchEvent(new Event("input"));

  board.addEventListener("click", (event) => {
    if (effectiveTool() !== "cursor" || performance.now() < suppressClickUntil) return;
    const frame = event.target.closest("[data-frame]");
    if (frame) selectFrame(frame);
  });
  board.addEventListener("keydown", (event) => {
    const frame = event.target.closest("[data-frame]");
    if (!frame || (event.key !== "Enter" && event.key !== " ")) return;
    selectFrame(frame);
    event.preventDefault();
  });

  const twoTouches = () => Array.from(touches.values()).slice(0, 2);
  const beginPinch = () => {
    const [a, b] = twoTouches();
    if (!a || !b) return;
    finishPan();
    const midpoint = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const distance = Math.hypot(b.x - a.x, b.y - a.y);
    if (distance === 0) return;
    suppressClickUntil = performance.now() + 250;
    pinch = {
      distance,
      scale,
      worldX: (midpoint.x - x) / scale,
      worldY: (midpoint.y - y) / scale,
    };
  };
  const updatePinch = () => {
    const [a, b] = twoTouches();
    if (!pinch || !a || !b) return;
    const distance = Math.hypot(b.x - a.x, b.y - a.y);
    const midpoint = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    const nextScale = clampScale(pinch.scale * distance / pinch.distance);
    x = midpoint.x - pinch.worldX * nextScale;
    y = midpoint.y - pinch.worldY * nextScale;
    scale = nextScale;
    apply();
  };

  viewport.addEventListener("pointerdown", (event) => {
    if (event.pointerType === "touch") {
      touches.set(event.pointerId, localPoint(event.clientX, event.clientY));
      viewport.setPointerCapture(event.pointerId);
      if (touches.size === 2) {
        beginPinch();
      } else if (touches.size === 1 && effectiveTool() === "hand") {
        beginPan(event.pointerId, touches.get(event.pointerId));
      }
      return;
    }
    const shouldPan = event.button === 1 || (event.button === 0 && effectiveTool() === "hand");
    if (!shouldPan) return;
    beginPan(event.pointerId, localPoint(event.clientX, event.clientY));
    viewport.setPointerCapture(event.pointerId);
    event.preventDefault();
  });
  viewport.addEventListener("pointermove", (event) => {
    if (event.pointerType === "touch" && touches.has(event.pointerId)) {
      const point = localPoint(event.clientX, event.clientY);
      touches.set(event.pointerId, point);
      if (!pinch && touches.size >= 2) beginPinch();
      if (pinch) updatePinch();
      else updatePan(event.pointerId, point);
      return;
    }
    updatePan(event.pointerId, localPoint(event.clientX, event.clientY));
  });
  const finishPointer = (event) => {
    finishPan(event.pointerId);
    if (!touches.delete(event.pointerId)) return;
    pinch = null;
    if (touches.size >= 2) {
      beginPinch();
    } else if (touches.size === 1 && effectiveTool() === "hand") {
      const [remaining] = touches.entries();
      beginPan(remaining[0], remaining[1]);
    }
  };
  viewport.addEventListener("pointerup", finishPointer);
  viewport.addEventListener("pointercancel", finishPointer);
  viewport.addEventListener("lostpointercapture", finishPointer);

  viewport.addEventListener("wheel", (event) => {
    event.preventDefault();
    const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE
      ? 16
      : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
        ? viewport.clientHeight
        : 1;
    if (event.ctrlKey || event.metaKey) {
      const rawExponent = -event.deltaY * unit * WHEEL_ZOOM_SENSITIVITY;
      const exponent = Math.min(Math.max(rawExponent, -0.25), 0.25);
      const factor = Math.exp(exponent);
      zoomAt(scale * factor, localPoint(event.clientX, event.clientY));
      return;
    }
    if (event.shiftKey && event.deltaX === 0) {
      x -= event.deltaY * unit;
    } else {
      x -= event.deltaX * unit;
      y -= event.deltaY * unit;
    }
    apply();
  }, { passive: false });

  const ignoresShortcut = (target) => target instanceof Element
    && Boolean(target.closest("button, a, input, select, textarea, [contenteditable]"));
  window.addEventListener("keydown", (event) => {
    const togglesUi = (event.metaKey || event.ctrlKey)
      && !event.altKey
      && !event.shiftKey
      && event.code === "Backslash";
    if (togglesUi) {
      if (!event.repeat) setUiVisible(document.body.classList.contains("ui-hidden"));
      event.preventDefault();
      return;
    }
    if (ignoresShortcut(event.target) || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.code === "Space") {
      spaceHeld = true;
      renderTools();
      event.preventDefault();
    } else if (!event.repeat && event.code === "KeyH") {
      selectTool("hand");
    } else if (!event.repeat && event.code === "KeyV") {
      selectTool("cursor");
    } else if (!event.repeat && (event.key === "+" || event.key === "=")) {
      zoomAt(scale * ZOOM_STEP, viewportCenter());
    } else if (!event.repeat && event.key === "-") {
      zoomAt(scale / ZOOM_STEP, viewportCenter());
    } else if (!event.repeat && event.key === "Escape") {
      clearSelection();
    }
  });
  window.addEventListener("keyup", (event) => {
    if (event.code !== "Space") return;
    spaceHeld = false;
    renderTools();
  });
  window.addEventListener("blur", () => {
    spaceHeld = false;
    finishPan();
    touches.clear();
    pinch = null;
    renderTools();
  });
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) return;
    spaceHeld = false;
    finishPan();
    touches.clear();
    pinch = null;
    renderTools();
  });

  if (window.ResizeObserver) {
    new ResizeObserver(requestRulers).observe(viewport);
  } else {
    window.addEventListener("resize", requestRulers);
  }
  renderTools();
  apply();
})();
