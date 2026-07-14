// Generated from web/src/editor/canvas-chrome.ts; do not edit.
(function() {
	//#region src/editor/live-preview.ts
	var EDITOR_HOST_META_NAME = "uhura-editor-host";
	var EDITOR_BUILD_META_NAME = "uhura-editor-build";
	var EDITOR_EVENTS_PATH = "/editor/events";
	var EDITOR_CHECKPOINT_KEY = "uhura.editor.live-checkpoint.v2";
	var isRecord = (value) => typeof value === "object" && value !== null && !Array.isArray(value);
	var isGeneration = (value) => typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
	var isActiveGeneration = (value) => isGeneration(value) && value > 0;
	var isBuildId = (value) => typeof value === "string" && value.length > 0;
	var isFiniteNumber = (value) => typeof value === "number" && Number.isFinite(value);
	var optionalString = (value, fallback = "") => typeof value === "string" ? value : fallback;
	/**
	* Parses the hosted document's active generation. `0` is the cold-invalid
	* sentinel: the Editor is live, but there is no last-known-good Canvas yet.
	*/
	var parseHostedGeneration = (content) => {
		if (content === null || content.trim() === "") return void 0;
		const generation = Number(content);
		if (!Number.isSafeInteger(generation) || generation < 0) return void 0;
		return generation === 0 ? null : generation;
	};
	/**
	* Parses the content-derived identity of the hosted Canvas. An empty value is
	* the cold-invalid sentinel; a missing meta element is represented by the
	* caller as `undefined` so standalone exports remain entirely offline.
	*/
	var parseHostedBuildId = (content) => {
		if (content === null) return void 0;
		const buildId = content.trim();
		return buildId === "" ? null : buildId;
	};
	/** Runtime-checks the SSE boundary instead of trusting a TypeScript cast. */
	var parseEditorLiveEvent = (value) => {
		if (!isRecord(value)) return null;
		const candidateGeneration = value["candidateGeneration"];
		const activeGeneration = value["activeGeneration"];
		const activeBuildId = value["activeBuildId"];
		const status = value["status"];
		const diagnostics = value["diagnostics"];
		if (!isGeneration(candidateGeneration) || !(activeGeneration === null || isActiveGeneration(activeGeneration)) || !(activeBuildId === null || isBuildId(activeBuildId)) || activeGeneration === null !== (activeBuildId === null) || status !== "active" && status !== "rejected" || diagnostics !== void 0 && !isRecord(diagnostics) || status === "active" && activeGeneration !== candidateGeneration || status === "rejected" && activeGeneration !== null && activeGeneration >= candidateGeneration) return null;
		return {
			candidateGeneration,
			activeGeneration,
			activeBuildId,
			status,
			...diagnostics === void 0 ? {} : { diagnostics }
		};
	};
	/** Numeric generations are process-local labels; only the content identity
	* can decide whether a hosted document has converged after a restart. */
	var shouldReloadEditor = (documentBuildId, event) => event.activeBuildId !== documentBuildId;
	/** Process-local event ordering and exactly-once reload state. Transport and
	* DOM effects stay in the Editor controller; this state machine is pure. */
	var EditorLiveSession = class {
		#documentBuildId;
		#highestCandidate = -1;
		#reloading = false;
		constructor(documentBuildId) {
			this.#documentBuildId = documentBuildId;
		}
		get reloading() {
			return this.#reloading;
		}
		reconnect() {
			this.#highestCandidate = -1;
		}
		accept(event) {
			if (this.#reloading || event.candidateGeneration < this.#highestCandidate) return { kind: "ignored" };
			this.#highestCandidate = event.candidateGeneration;
			if (shouldReloadEditor(this.#documentBuildId, event)) {
				this.#reloading = true;
				return {
					kind: "reload",
					event
				};
			}
			return event.status === "rejected" ? {
				kind: "rejected",
				event
			} : {
				kind: "active",
				event
			};
		}
	};
	var parseSemanticKey = (value) => {
		if (value === null) return null;
		if (!isRecord(value)) return void 0;
		const kind = value["kind"];
		const subject = value["subject"];
		const example = value["example"];
		if (typeof kind !== "string" || typeof subject !== "string" || typeof example !== "string" || !kind || !subject || !example) return;
		return {
			kind,
			subject,
			example
		};
	};
	var parseCheckpoint = (value) => {
		if (!isRecord(value) || value["version"] !== 2) return null;
		const targetBuildId = value["targetBuildId"];
		const camera = value["camera"];
		const tool = value["tool"];
		const search = value["search"];
		const uiVisible = value["uiVisible"];
		const selection = parseSemanticKey(value["selection"]);
		if (!isBuildId(targetBuildId) || !isRecord(camera) || !isFiniteNumber(camera["x"]) || !isFiniteNumber(camera["y"]) || !isFiniteNumber(camera["scale"]) || camera["scale"] <= 0 || tool !== "cursor" && tool !== "hand" || typeof search !== "string" || typeof uiVisible !== "boolean" || selection === void 0) return null;
		return {
			version: 2,
			targetBuildId,
			camera: {
				x: camera["x"],
				y: camera["y"],
				scale: camera["scale"]
			},
			tool,
			search,
			uiVisible,
			selection
		};
	};
	/**
	* Reads and consumes a checkpoint for one reload handoff. Whichever active
	* build the navigation serves may consume it: another candidate or a rapid
	* source reversion can win after the reload has already started.
	*/
	var takeEditorCheckpoint = (storage, activeBuildId) => {
		let raw;
		try {
			raw = storage.getItem(EDITOR_CHECKPOINT_KEY);
			storage.removeItem(EDITOR_CHECKPOINT_KEY);
		} catch {
			return null;
		}
		if (raw === null || activeBuildId === null) return null;
		try {
			const checkpoint = parseCheckpoint(JSON.parse(raw));
			if (!checkpoint) return null;
			return {
				camera: checkpoint.camera,
				tool: checkpoint.tool,
				search: checkpoint.search,
				uiVisible: checkpoint.uiVisible,
				selection: checkpoint.selection
			};
		} catch {
			return null;
		}
	};
	var storeEditorCheckpoint = (storage, targetBuildId, state) => {
		const checkpoint = {
			version: 2,
			targetBuildId,
			...state
		};
		try {
			storage.setItem(EDITOR_CHECKPOINT_KEY, JSON.stringify(checkpoint));
			return true;
		} catch {
			return false;
		}
	};
	var diagnosticLocation = (diagnostic) => {
		const file = optionalString(diagnostic["file"]);
		const span = diagnostic["span"];
		if (!isRecord(span)) return file;
		const start = span["start"];
		if (!isRecord(start)) return file;
		const line = start["line"];
		const col = start["col"];
		if (typeof line !== "number") return file;
		return `${file || "source"}:${line}${typeof col === "number" ? `:${col}` : ""}`;
	};
	var summarizeDiagnostics = (envelope) => {
		const diagnostics = envelope?.["diagnostics"];
		if (!Array.isArray(diagnostics)) return [];
		return diagnostics.filter(isRecord).map((diagnostic) => ({
			code: optionalString(diagnostic["code"], "UH????"),
			rule: optionalString(diagnostic["rule"]),
			severity: optionalString(diagnostic["severity"], "error"),
			message: optionalString(diagnostic["message"], "The Canvas candidate was rejected."),
			location: diagnosticLocation(diagnostic)
		}));
	};
	//#endregion
	//#region src/editor/canvas-chrome.ts
	var createLiveStatus = () => {
		const style = document.createElement("style");
		style.textContent = `
    #uhura-editor-live-status {
      position: fixed;
      inset: 14px auto auto 50%;
      z-index: 1000;
      inline-size: min(540px, calc(100vw - 28px));
      max-block-size: min(430px, calc(100vh - 28px));
      overflow: auto;
      padding: 13px 14px;
      border: 1px solid #e5b45a;
      border-radius: 10px;
      color: #382a14;
      background: rgb(255 249 235 / 97%);
      box-shadow: 0 12px 32px rgb(51 38 14 / 18%);
      font: 12px/1.45 Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      transform: translateX(-50%);
      backdrop-filter: blur(12px);
    }
    #uhura-editor-live-status[hidden] { display: none; }
    .uhura-editor-live-heading { display: flex; align-items: flex-start; gap: 12px; }
    .uhura-editor-live-heading > div { min-inline-size: 0; flex: 1; }
    .uhura-editor-live-heading strong { display: block; font-size: 13px; }
    .uhura-editor-live-heading p { margin: 2px 0 0; color: #725d36; }
    .uhura-editor-live-dismiss {
      flex: none;
      inline-size: 26px;
      block-size: 26px;
      padding: 0;
      border: 0;
      border-radius: 6px;
      color: #725d36;
      background: transparent;
      font: 18px/1 sans-serif;
      cursor: pointer;
    }
    .uhura-editor-live-dismiss:hover { background: rgb(114 93 54 / 10%); }
    .uhura-editor-live-dismiss:focus-visible { outline: 2px solid #9a6500; outline-offset: 1px; }
    .uhura-editor-live-diagnostics { display: grid; gap: 8px; margin: 11px 0 0; padding: 0; list-style: none; }
    .uhura-editor-live-diagnostic { padding-block-start: 8px; border-block-start: 1px solid rgb(114 93 54 / 18%); }
    .uhura-editor-live-diagnostic p { margin: 2px 0 0; overflow-wrap: anywhere; }
    .uhura-editor-live-diagnostic small { display: block; color: #806b45; overflow-wrap: anywhere; }
    .uhura-editor-live-diagnostic code { color: #8a4b18; font: 10px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace; }
  `;
		document.head.append(style);
		const host = document.createElement("section");
		host.id = "uhura-editor-live-status";
		host.setAttribute("role", "status");
		host.setAttribute("aria-live", "polite");
		host.hidden = true;
		const heading = document.createElement("div");
		heading.className = "uhura-editor-live-heading";
		const copy = document.createElement("div");
		const title = document.createElement("strong");
		const detail = document.createElement("p");
		copy.append(title, detail);
		const dismiss = document.createElement("button");
		dismiss.className = "uhura-editor-live-dismiss";
		dismiss.type = "button";
		dismiss.setAttribute("aria-label", "Dismiss preview diagnostics");
		dismiss.title = "Dismiss";
		dismiss.textContent = "×";
		heading.append(copy, dismiss);
		const list = document.createElement("ol");
		list.className = "uhura-editor-live-diagnostics";
		host.append(heading, list);
		document.body.append(host);
		dismiss.addEventListener("click", () => {
			host.hidden = true;
		});
		const show = (headingText, detailText) => {
			title.textContent = headingText;
			detail.textContent = detailText;
			host.hidden = false;
		};
		const renderDiagnostics = (diagnostics, emptyMessage) => {
			const summaries = summarizeDiagnostics(diagnostics);
			const visible = summaries.slice(0, 8);
			list.replaceChildren(...visible.map((diagnostic) => {
				const item = document.createElement("li");
				item.className = "uhura-editor-live-diagnostic";
				const label = document.createElement("code");
				label.textContent = [
					diagnostic.severity.toUpperCase(),
					diagnostic.code,
					diagnostic.rule
				].filter(Boolean).join(" · ");
				const message = document.createElement("p");
				message.textContent = diagnostic.message;
				const location = document.createElement("small");
				location.textContent = diagnostic.location;
				item.append(label, message);
				if (diagnostic.location) item.append(location);
				return item;
			}));
			if (summaries.length > visible.length) {
				const remainder = document.createElement("li");
				remainder.className = "uhura-editor-live-diagnostic";
				remainder.textContent = `${summaries.length - visible.length} more diagnostics`;
				list.append(remainder);
			} else if (summaries.length === 0) {
				const unavailable = document.createElement("li");
				unavailable.className = "uhura-editor-live-diagnostic";
				unavailable.textContent = emptyMessage;
				list.append(unavailable);
			}
			return summaries.length;
		};
		return {
			hide() {
				host.hidden = true;
				list.replaceChildren();
			},
			showActiveWarnings(activeGeneration, diagnostics) {
				if (renderDiagnostics(diagnostics, "The active Canvas reported a warning without details.") === 0) {
					host.hidden = true;
					list.replaceChildren();
					return;
				}
				show("Preview updated with warnings", `Canvas ${activeGeneration} is active. Review the checker warnings below.`);
			},
			showRejected(candidateGeneration, activeGeneration, diagnostics) {
				renderDiagnostics(diagnostics, "The Canvas candidate was rejected without diagnostic details.");
				show(activeGeneration === null ? "No valid preview yet" : "Previewing the last valid version", activeGeneration === null ? `Canvas candidate ${candidateGeneration} has errors. The first valid save will open automatically.` : `Canvas candidate ${candidateGeneration} has errors. Canvas ${activeGeneration} remains active.`);
			},
			showReconnecting() {
				list.replaceChildren();
				show("Live preview disconnected", "Reconnecting to the Editor host…");
			},
			showWaiting() {
				list.replaceChildren();
				show("Waiting for a valid preview", "Fix the saved source errors; the first valid Canvas will open automatically.");
			}
		};
	};
	var startLiveUpdates = (documentGeneration, documentBuildId, checkpoint) => {
		const status = createLiveStatus();
		if (documentGeneration === null) status.showWaiting();
		const events = new EventSource(EDITOR_EVENTS_PATH);
		const live = new EditorLiveSession(documentBuildId);
		let opened = false;
		events.addEventListener("open", () => {
			if (opened) live.reconnect();
			opened = true;
		});
		events.addEventListener("error", () => {
			if (!live.reloading) status.showReconnecting();
		});
		events.addEventListener("message", (message) => {
			let decoded;
			try {
				decoded = JSON.parse(message.data);
			} catch {
				console.warn("ignored malformed Uhura Editor event");
				return;
			}
			const event = parseEditorLiveEvent(decoded);
			if (!event) {
				console.warn("ignored invalid Uhura Editor event", decoded);
				return;
			}
			const decision = live.accept(event);
			if (decision.kind === "ignored") return;
			if (decision.kind === "reload") {
				if (event.activeBuildId !== null) checkpoint?.(event.activeBuildId);
				events.close();
				window.location.reload();
				return;
			}
			if (decision.kind === "rejected") status.showRejected(event.candidateGeneration, event.activeGeneration, event.diagnostics);
			else if (event.diagnostics && event.activeGeneration !== null) status.showActiveWarnings(event.activeGeneration, event.diagnostics);
			else status.hide();
		});
	};
	(() => {
		const requiredElement = (id) => {
			const element = document.getElementById(id);
			if (!element) throw new Error(`missing required editor element #${id}`);
			return element;
		};
		const requiredQuery = (selector) => {
			const element = document.querySelector(selector);
			if (!element) throw new Error(`missing required editor element ${selector}`);
			return element;
		};
		const closestElement = (target, selector) => target instanceof Element ? target.closest(selector) : null;
		const hostMarker = document.querySelector(`meta[name="${EDITOR_HOST_META_NAME}"]`);
		const buildMarker = document.querySelector(`meta[name="${EDITOR_BUILD_META_NAME}"]`);
		const documentGeneration = hostMarker ? parseHostedGeneration(hostMarker.content) : void 0;
		const documentBuildId = hostMarker && buildMarker ? parseHostedBuildId(buildMarker.content) : void 0;
		const validHostedIdentity = documentGeneration !== void 0 && documentBuildId !== void 0 && documentGeneration === null === (documentBuildId === null);
		if (hostMarker && !validHostedIdentity) console.warn("ignored an invalid Uhura Editor host identity");
		if (!document.getElementById("viewport")) {
			if (validHostedIdentity) startLiveUpdates(documentGeneration, documentBuildId, null);
			return;
		}
		let restoredState = null;
		if (validHostedIdentity) try {
			restoredState = takeEditorCheckpoint(window.sessionStorage, documentBuildId);
		} catch {}
		const viewport = requiredElement("viewport");
		const board = requiredElement("board");
		const tools = requiredQuery(".canvas-tools");
		const cursorButton = requiredElement("tool-cursor");
		const handButton = requiredElement("tool-hand");
		const zoomOutput = requiredElement("canvas-zoom");
		const zoomOutButton = requiredElement("zoom-out");
		const zoomInButton = requiredElement("zoom-in");
		const focusSelectionButton = requiredElement("focus-selection");
		const navigator = requiredElement("canvas-navigator");
		const navigatorSearch = requiredElement("navigator-search");
		const navigatorEmpty = requiredElement("navigator-empty");
		const rulerX = requiredElement("ruler-x");
		const rulerY = requiredElement("ruler-y");
		const inspectorOverview = requiredElement("inspector-overview");
		const inspectorSelection = requiredElement("inspector-selection");
		const clearSelectionButton = requiredElement("clear-selection");
		const selectionKind = requiredElement("selection-kind");
		const selectionName = requiredElement("selection-name");
		const selectionSubject = requiredElement("selection-subject");
		const selectionExample = requiredElement("selection-example");
		const selectionSize = requiredElement("selection-size");
		const selectionOrigin = requiredElement("selection-origin");
		const selectionFromRow = requiredElement("selection-from-row");
		const selectionFrom = requiredElement("selection-from");
		const selectionStatus = requiredElement("selection-status");
		const selectionData = requiredElement("selection-data");
		const selectionNoData = requiredElement("selection-no-data");
		const selectionAnnouncement = requiredElement("selection-announcement");
		const selectionNoteBlock = requiredElement("selection-note-block");
		const selectionNote = requiredElement("selection-note");
		const selectionInteractions = requiredElement("selection-interactions");
		const selectionNoInteractions = requiredElement("selection-no-interactions");
		const MIN_SCALE = .02;
		const MAX_SCALE = 3;
		const ZOOM_STEP = 1.2;
		const WHEEL_ZOOM_SENSITIVITY = .01;
		const UI_VISIBLE_KEY = "uhura.editor.ui-visible";
		let x = restoredState?.camera.x ?? 0;
		let y = restoredState?.camera.y ?? 0;
		let scale = Math.min(Math.max(restoredState?.camera.scale ?? 1, MIN_SCALE), MAX_SCALE);
		let selectedTool = restoredState?.tool ?? "cursor";
		let selectedFrame = null;
		let spaceHeld = false;
		let pan = null;
		let pinch = null;
		let suppressClickUntil = 0;
		let rulerFrame = 0;
		const touches = /* @__PURE__ */ new Map();
		const storedBoolean = (key, fallback) => {
			try {
				const value = window.localStorage.getItem(key);
				return value === null ? fallback : value === "true";
			} catch {
				return fallback;
			}
		};
		const storeBoolean = (key, value) => {
			try {
				window.localStorage.setItem(key, String(value));
			} catch {}
		};
		const clampScale = (value) => Math.min(Math.max(value, MIN_SCALE), MAX_SCALE);
		const viewportCenter = () => ({
			x: viewport.clientWidth / 2,
			y: viewport.clientHeight / 2
		});
		const localPoint = (clientX, clientY) => {
			const rect = viewport.getBoundingClientRect();
			return {
				x: clientX - rect.left,
				y: clientY - rect.top
			};
		};
		const chooseRulerStep = () => {
			const desiredWorldUnits = 76 / scale;
			const magnitude = 10 ** Math.floor(Math.log10(desiredWorldUnits));
			const normalized = desiredWorldUnits / magnitude;
			return (normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10) * magnitude;
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
			if (!context) throw new Error("2D canvas rendering is unavailable");
			context.setTransform(ratio, 0, 0, ratio, 0, 0);
			context.clearRect(0, 0, width, height);
			context.strokeStyle = "#aeb5be";
			context.fillStyle = "#68717d";
			context.lineWidth = 1;
			context.font = "9px ui-monospace, SFMono-Regular, Menlo, monospace";
			return {
				context,
				width,
				height
			};
		};
		const drawRulers = () => {
			rulerFrame = 0;
			const horizontal = prepareCanvas(rulerX);
			const vertical = prepareCanvas(rulerY);
			const step = chooseRulerStep();
			const minor = step / 5;
			const firstX = Math.floor(-x / scale / minor) * minor;
			const lastX = (horizontal.width - x) / scale;
			horizontal.context.beginPath();
			for (let world = firstX; world <= lastX + minor; world += minor) {
				const screen = Math.round(x + world * scale) + .5;
				const major = Math.abs(world / step - Math.round(world / step)) < .001;
				horizontal.context.moveTo(screen, horizontal.height);
				horizontal.context.lineTo(screen, horizontal.height - (major ? 9 : 4));
				if (major) horizontal.context.fillText(String(Math.round(world)), screen + 3, 9);
			}
			horizontal.context.stroke();
			const firstY = Math.floor(-y / scale / minor) * minor;
			const lastY = (vertical.height - y) / scale;
			vertical.context.beginPath();
			for (let world = firstY; world <= lastY + minor; world += minor) {
				const screen = Math.round(y + world * scale) + .5;
				const major = Math.abs(world / step - Math.round(world / step)) < .001;
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
			if (!pan || pointerId !== void 0 && pan.pointerId !== pointerId) return;
			if (pan.moved) suppressClickUntil = performance.now() + 250;
			pan = null;
			viewport.classList.remove("panning");
			renderTools();
		};
		const beginPan = (pointerId, point) => {
			pan = {
				pointerId,
				pointerX: point.x,
				pointerY: point.y,
				x,
				y,
				moved: false
			};
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
				height: frameRect.height / scale
			};
		};
		const revealElement = (element) => {
			if (!element) return;
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
			selectionData.replaceChildren();
			selectionNoData.hidden = false;
			selectionAnnouncement.textContent = "";
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
			const origin = frame.dataset.pinned === "true" ? "Pinned example" : frame.dataset.derived === "true" ? "Replay-derived" : "Checked example";
			setInspectorText(selectionOrigin, origin);
			const from = frame.dataset.from.trim();
			selectionFromRow.hidden = !from;
			selectionFrom.textContent = from;
			const status = [];
			if (frame.dataset.default === "true") status.push("Default");
			const inFlight = Number(frame.dataset.inFlight || 0);
			status.push(inFlight > 0 ? `${inFlight} in flight` : "Settled");
			setInspectorText(selectionStatus, status.join(" · "));
			const dataTemplate = frame.querySelector(":scope > template[data-preview-data]");
			selectionData.replaceChildren(...dataTemplate ? [dataTemplate.content.cloneNode(true)] : []);
			selectionNoData.hidden = selectionData.childElementCount > 0;
			const note = frame.dataset.previewNote.trim();
			selectionNoteBlock.hidden = !note;
			selectionNote.textContent = note;
			const interactions = Array.from(frame.querySelectorAll(".shell [data-note]")).map((element) => element.dataset.note.trim()).filter(Boolean).filter((value, index, values) => values.indexOf(value) === index);
			selectionInteractions.replaceChildren(...interactions.map((interaction) => {
				const item = document.createElement("li");
				item.textContent = interaction;
				return item;
			}));
			selectionNoInteractions.hidden = interactions.length > 0;
			selectionAnnouncement.textContent = `${frame.dataset.subject} / ${frame.dataset.example} selected; details updated.`;
			if (reveal) revealElement(frame);
		};
		const semanticKeyForFrame = (frame) => ({
			kind: frame.dataset.kind,
			subject: frame.dataset.subject,
			example: frame.dataset.example
		});
		const findFrameBySemanticKey = (key) => Array.from(document.querySelectorAll("[data-frame]")).find((frame) => frame.dataset.kind === key.kind && frame.dataset.subject === key.subject && frame.dataset.example === key.example) ?? null;
		const setUiVisible = (visible, persist = true) => {
			document.body.classList.toggle("ui-hidden", !visible);
			if (persist) storeBoolean(UI_VISIBLE_KEY, visible);
			requestRulers();
		};
		setUiVisible(restoredState?.uiVisible ?? storedBoolean(UI_VISIBLE_KEY, true), false);
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
			const frameButton = closestElement(event.target, "[data-frame-target]");
			if (frameButton) {
				selectFrame(document.getElementById(frameButton.dataset.frameTarget), true);
				return;
			}
			const rowButton = closestElement(event.target, "[data-row-target]");
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
		navigatorSearch.value = restoredState?.search ?? "";
		navigatorSearch.dispatchEvent(new Event("input"));
		board.addEventListener("click", (event) => {
			if (effectiveTool() !== "cursor" || performance.now() < suppressClickUntil) return;
			const frame = closestElement(event.target, "[data-frame]");
			if (frame) selectFrame(frame);
		});
		board.addEventListener("keydown", (event) => {
			const frame = closestElement(event.target, "[data-frame]");
			if (!frame || event.key !== "Enter" && event.key !== " ") return;
			selectFrame(frame);
			event.preventDefault();
		});
		const twoTouches = () => Array.from(touches.values()).slice(0, 2);
		const beginPinch = () => {
			const [a, b] = twoTouches();
			if (!a || !b) return;
			finishPan();
			const midpoint = {
				x: (a.x + b.x) / 2,
				y: (a.y + b.y) / 2
			};
			const distance = Math.hypot(b.x - a.x, b.y - a.y);
			if (distance === 0) return;
			suppressClickUntil = performance.now() + 250;
			pinch = {
				distance,
				scale,
				worldX: (midpoint.x - x) / scale,
				worldY: (midpoint.y - y) / scale
			};
		};
		const updatePinch = () => {
			const [a, b] = twoTouches();
			if (!pinch || !a || !b) return;
			const distance = Math.hypot(b.x - a.x, b.y - a.y);
			const midpoint = {
				x: (a.x + b.x) / 2,
				y: (a.y + b.y) / 2
			};
			const nextScale = clampScale(pinch.scale * distance / pinch.distance);
			x = midpoint.x - pinch.worldX * nextScale;
			y = midpoint.y - pinch.worldY * nextScale;
			scale = nextScale;
			apply();
		};
		viewport.addEventListener("pointerdown", (event) => {
			if (event.pointerType === "touch") {
				const point = localPoint(event.clientX, event.clientY);
				touches.set(event.pointerId, point);
				viewport.setPointerCapture(event.pointerId);
				if (touches.size === 2) beginPinch();
				else if (touches.size === 1 && effectiveTool() === "hand") beginPan(event.pointerId, point);
				return;
			}
			if (!(event.button === 1 || event.button === 0 && effectiveTool() === "hand")) return;
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
			if (touches.size >= 2) beginPinch();
			else if (touches.size === 1 && effectiveTool() === "hand") {
				const remaining = touches.entries().next().value;
				if (remaining) beginPan(remaining[0], remaining[1]);
			}
		};
		viewport.addEventListener("pointerup", finishPointer);
		viewport.addEventListener("pointercancel", finishPointer);
		viewport.addEventListener("lostpointercapture", finishPointer);
		viewport.addEventListener("wheel", (event) => {
			event.preventDefault();
			const unit = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16 : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? viewport.clientHeight : 1;
			if (event.ctrlKey || event.metaKey) {
				const rawExponent = -event.deltaY * unit * WHEEL_ZOOM_SENSITIVITY;
				zoomAt(scale * Math.exp(Math.min(Math.max(rawExponent, -.25), .25)), localPoint(event.clientX, event.clientY));
				return;
			}
			if (event.shiftKey && event.deltaX === 0) x -= event.deltaY * unit;
			else {
				x -= event.deltaX * unit;
				y -= event.deltaY * unit;
			}
			apply();
		}, { passive: false });
		const ignoresShortcut = (target) => target instanceof Element && Boolean(target.closest("button, a, input, select, textarea, [contenteditable]"));
		window.addEventListener("keydown", (event) => {
			if ((event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey && event.code === "Backslash") {
				if (!event.repeat) setUiVisible(document.body.classList.contains("ui-hidden"));
				event.preventDefault();
				return;
			}
			if (ignoresShortcut(event.target) || event.metaKey || event.ctrlKey || event.altKey) return;
			if (event.code === "Space") {
				spaceHeld = true;
				renderTools();
				event.preventDefault();
			} else if (!event.repeat && event.code === "KeyH") selectTool("hand");
			else if (!event.repeat && event.code === "KeyV") selectTool("cursor");
			else if (!event.repeat && (event.key === "+" || event.key === "=")) zoomAt(scale * ZOOM_STEP, viewportCenter());
			else if (!event.repeat && event.key === "-") zoomAt(scale / ZOOM_STEP, viewportCenter());
			else if (!event.repeat && event.key === "Escape") clearSelection();
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
		if (window.ResizeObserver) new ResizeObserver(requestRulers).observe(viewport);
		else window.addEventListener("resize", requestRulers);
		if (restoredState?.selection) selectFrame(findFrameBySemanticKey(restoredState.selection));
		renderTools();
		apply();
		if (validHostedIdentity) {
			const liveHelp = document.querySelector(".inspector-callout p");
			if (liveHelp) liveHelp.textContent = "Save changes to rebuild these static previews automatically.";
			startLiveUpdates(documentGeneration, documentBuildId, (targetBuildId) => {
				const state = {
					camera: {
						x,
						y,
						scale
					},
					tool: selectedTool,
					search: navigatorSearch.value,
					uiVisible: !document.body.classList.contains("ui-hidden"),
					selection: selectedFrame ? semanticKeyForFrame(selectedFrame) : null
				};
				try {
					storeEditorCheckpoint(window.sessionStorage, targetBuildId, state);
				} catch {}
			});
		}
	})();
	//#endregion
})();
