// Generated from web/src/editor/canvas-chrome.ts; do not edit.
(function() {
	//#region src/editor/canvas-chrome.ts
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
		const viewport = requiredElement("viewport");
		const board = requiredElement("board");
		const connectorLayer = requiredQuery("#workflow-connectors");
		const connectors = Array.from(connectorLayer.querySelectorAll("[data-connector]"));
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
		const selectionReplayRow = requiredElement("selection-replay-row");
		const selectionReplay = requiredElement("selection-replay");
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
		let connectorFrame = 0;
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
			board.style.setProperty("--connector-stroke", `${1.5 / scale}px`);
			zoomOutput.textContent = `${Math.round(scale * 100)}%`;
			requestRulers();
			requestConnectors();
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
		const boardLocalRect = (element) => {
			const elementRect = element.getBoundingClientRect();
			const boardRect = board.getBoundingClientRect();
			return {
				x: (elementRect.left - boardRect.left) / scale,
				y: (elementRect.top - boardRect.top) / scale,
				width: elementRect.width / scale,
				height: elementRect.height / scale
			};
		};
		const layoutConnectors = () => {
			connectorFrame = 0;
			const rows = Array.from(board.querySelectorAll("[data-preview-row]"));
			const rowConnectors = /* @__PURE__ */ new Map();
			for (const row of rows) row.style.removeProperty("--workflow-rail-height");
			for (const connector of connectors) {
				const row = document.getElementById(connector.dataset.targetFrame)?.closest("[data-preview-row]");
				if (!row) continue;
				const grouped = rowConnectors.get(row) || [];
				grouped.push(connector);
				rowConnectors.set(row, grouped);
			}
			for (const [row, grouped] of rowConnectors) {
				const frameOrder = Array.from(row.querySelectorAll("[data-frame]"));
				const indexById = new Map(frameOrder.map((frame, index) => [frame.id, index]));
				const lanes = [];
				for (const connector of grouped) {
					const sourceIndex = indexById.get(connector.dataset.sourceFrame);
					const targetIndex = indexById.get(connector.dataset.targetFrame);
					if (sourceIndex === void 0 || targetIndex === void 0) continue;
					const interval = {
						start: Math.min(sourceIndex, targetIndex),
						end: Math.max(sourceIndex, targetIndex)
					};
					let lane = lanes.findIndex((used) => used.every((other) => interval.end < other.start || interval.start > other.end));
					if (lane < 0) {
						lane = lanes.length;
						lanes.push([]);
					}
					const laneIntervals = lanes[lane];
					if (!laneIntervals) continue;
					laneIntervals.push(interval);
					connector.dataset.lane = String(lane);
				}
				row.style.setProperty("--workflow-rail-height", `${28 + lanes.length * 20}px`);
			}
			for (const connector of connectors) {
				const source = document.getElementById(connector.dataset.sourceFrame);
				const target = document.getElementById(connector.dataset.targetFrame);
				const sourceShell = source?.querySelector(":scope > .shell");
				const targetShell = target?.querySelector(":scope > .shell");
				const path = connector.querySelector("[data-connector-path]");
				const arrow = connector.querySelector("[data-connector-arrow]");
				const origin = connector.querySelector("[data-connector-origin]");
				const label = connector.querySelector("[data-connector-label]");
				if (!sourceShell || !targetShell || !path || !arrow || !origin || !label) continue;
				const sourceRect = boardLocalRect(sourceShell);
				const targetRect = boardLocalRect(targetShell);
				const startX = sourceRect.x + sourceRect.width / 2;
				const startY = sourceRect.y;
				const endX = targetRect.x + targetRect.width / 2;
				const endY = targetRect.y;
				const lane = Number(connector.dataset.lane || 0);
				const railY = Math.min(startY, endY) - 18 - lane * 20;
				path.setAttribute("d", `M ${startX} ${startY} L ${startX} ${railY} L ${endX} ${railY} L ${endX} ${endY}`);
				arrow.setAttribute("d", `M ${endX - 4} ${endY - 8} L ${endX} ${endY} L ${endX + 4} ${endY - 8} Z`);
				origin.setAttribute("cx", String(startX));
				origin.setAttribute("cy", String(startY));
				label.setAttribute("x", String((startX + endX) / 2));
				label.setAttribute("y", String(railY - 6));
			}
		};
		const requestConnectors = () => {
			if (!connectorFrame) connectorFrame = window.requestAnimationFrame(layoutConnectors);
		};
		const updateConnectorSelection = (frame) => {
			document.querySelectorAll("[data-frame].is-related").forEach((related) => {
				related.classList.remove("is-related");
			});
			connectorLayer.classList.toggle("has-selection", Boolean(frame));
			for (const connector of connectors) {
				const active = Boolean(frame) && (connector.dataset.sourceFrame === frame?.id || connector.dataset.targetFrame === frame?.id);
				connector.classList.toggle("is-active", active);
				if (!active || !frame) continue;
				for (const id of [connector.dataset.sourceFrame, connector.dataset.targetFrame]) if (id !== frame.id) document.getElementById(id)?.classList.add("is-related");
			}
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
			updateConnectorSelection(null);
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
			updateConnectorSelection(frame);
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
			const replaySteps = frame.dataset.replaySteps.trim();
			selectionReplayRow.hidden = !replaySteps;
			selectionReplay.textContent = replaySteps;
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
		if (window.ResizeObserver) {
			const observer = new ResizeObserver(() => {
				requestRulers();
				requestConnectors();
			});
			observer.observe(viewport);
			observer.observe(board);
		} else window.addEventListener("resize", () => {
			requestRulers();
			requestConnectors();
		});
		window.addEventListener("load", requestConnectors);
		renderTools();
		apply();
	})();
	//#endregion
})();
