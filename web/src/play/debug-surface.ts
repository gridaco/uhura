// DOM adapter for Play's optional runtime debugger. It consumes the immutable
// inspection handle through the framework-neutral controller and renders one
// focused definition at a time as a deterministic behavior graph.

import type {
  InspectionHandle,
  InspectionState,
} from "../protocol/types.js";
import {
  createDebugController,
  type DebugControllerUpdate,
} from "./debug-controller.js";
import {
  deriveDebugGraph,
  type DebugDefinitionOption,
  type DebugGraphModel,
  type DebugGraphNode,
} from "./debug-model.js";
import { layoutDebugGraph } from "./debug-layout.js";
import { createDebugResizeController } from "./debug-resize.js";
import { createDebugViewport } from "./debug-viewport.js";
import type { PlayShell } from "./shell.js";

const SVG_NAMESPACE = "http://www.w3.org/2000/svg";

export interface PlayDebugSurfaceOptions {
  window?: Window;
  onOpenChange?(open: boolean): void;
}

export interface PlayDebugSurface {
  readonly isOpen: boolean;
  dispose(): void;
}

function element(
  document: Document,
  tag: string,
  className?: string,
  text?: string,
): HTMLElement {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function svgElement(document: Document, tag: string): SVGElement {
  return document.createElementNS(SVG_NAMESPACE, tag);
}

function capitalized(value: string): string {
  return value.length === 0
    ? value
    : (value[0] ?? "").toUpperCase() + value.slice(1);
}

function definitionText(definition: DebugDefinitionOption): string {
  const markers: string[] = [];
  if (definition.top) markers.push("top");
  else if (definition.active) markers.push("mounted");
  if (definition.runtime) markers.push("running");
  if (definition.transitionTarget) markers.push("transition");
  if (definition.entry) markers.push("entry");
  const suffix = markers.length === 0 ? "" : " · " + markers.join(", ");
  return capitalized(definition.kind) + " · " + definition.label + suffix;
}

function traceEventLabel(state: InspectionState): string {
  const trace = state.latest?.trace;
  if (!trace) return "waiting for first step";
  if (trace.dispatch) return trace.dispatch.on;
  const kind = trace.event["kind"];
  return typeof kind === "string" ? kind : "runtime event";
}

function traceDisposition(state: InspectionState): string {
  const trace = state.latest?.trace;
  if (!trace) return "idle";
  if (trace.dispatch?.aborted) {
    return "aborted · " + trace.dispatch.aborted;
  }
  if (trace.drop) return "dropped · " + trace.drop;
  if (trace.dispatch?.selected !== null && trace.dispatch?.selected !== undefined) {
    return "handler " + String(trace.dispatch.selected + 1);
  }
  if (trace.reserved) return "reserved · " + trace.reserved.event;
  return "state updated";
}

function nodeStatus(node: DebugGraphNode): string {
  const runtime = node.runtime;
  if (runtime.selected) return "selected";
  if (runtime.consulted) return runtime.consulted;
  if (runtime.written) return "written";
  if (runtime.sent) return "sent";
  if (runtime.structural) return "transition";
  if (runtime.projectionApply) return runtime.projectionApply;
  if (runtime.pending > 0) return String(runtime.pending) + " pending";
  if (runtime.active) return "mounted";
  return node.kind;
}

function nodeClasses(node: DebugGraphNode, selected: boolean): string {
  const classes = [
    "uh-debug-node",
    "uh-debug-node-" + node.kind,
  ];
  const runtime = node.runtime;
  if (runtime.active) classes.push("is-active");
  if (runtime.current) classes.push("is-current");
  if (runtime.selected) classes.push("is-runtime-selected");
  if (runtime.written) classes.push("is-written");
  if (runtime.sent) classes.push("is-sent");
  if (runtime.structural) classes.push("is-structural");
  if (runtime.pending > 0) classes.push("is-pending");
  if (runtime.projectionFailures > 0 || runtime.projectionApply === "failed") {
    classes.push("has-failure");
  }
  if (runtime.consulted) classes.push("is-consulted-" + runtime.consulted);
  if (selected) classes.push("is-selected");
  return classes.join(" ");
}

function nodeRuntimeText(node: DebugGraphNode): string {
  const runtime = node.runtime;
  const states: string[] = [];
  if (runtime.active) states.push("mounted");
  if (runtime.selected) states.push("selected handler");
  else if (runtime.consulted) states.push("guard " + runtime.consulted);
  if (runtime.written) states.push("written this step");
  if (runtime.sent) states.push("sent this step");
  if (runtime.structural) states.push("structural target");
  if (runtime.pending > 0) states.push(String(runtime.pending) + " pending");
  if (runtime.projectionApply) {
    states.push("projection " + runtime.projectionApply);
  }
  if (runtime.projectionReady > 0) {
    states.push(String(runtime.projectionReady) + " projection snapshot");
  }
  if (runtime.projectionFailures > 0) {
    states.push(String(runtime.projectionFailures) + " projection failure");
  }
  return states.length === 0 ? "No activity in the latest step" : states.join(" · ");
}

function emptyMessage(reason: DebugGraphModel["emptyReason"]): string {
  switch (reason) {
    case "disposed":
      return "Runtime inspection is unavailable for this Play session.";
    case "no-definitions":
      return "The checked program contains no visualizable definitions.";
    case "loading":
      return "Waiting for the checked program and first runtime step.";
    case null:
      return "This definition has no inspectable behavior.";
  }
}

export function mountPlayDebugSurface(
  shell: PlayShell,
  inspection: InspectionHandle,
  options: PlayDebugSurfaceOptions = {},
): PlayDebugSurface {
  const view = options.window ?? shell.document.defaultView ?? window;
  const resizeController = createDebugResizeController({
    container: shell.container,
    panel: shell.debugPanel,
    panelSeparator: shell.debugPanelResize,
    details: shell.debugDetails,
    detailsSeparator: shell.debugDetailsResize,
    viewport: view,
  });
  const viewportController = createDebugViewport({
    viewport: shell.debugGraph,
    content: shell.debugGraphContent,
    zoomIn: shell.debugZoomIn,
    zoomOut: shell.debugZoomOut,
    zoomReset: shell.debugZoomReset,
    zoomOutput: shell.debugZoomLevel,
    window: view,
  });
  let disposed = false;
  let followLive = true;
  let pinnedDefinitionId: string | null = null;
  let selectedNodeId: string | null = null;
  let userSelectedNode = false;
  let lastState: InspectionState | null = null;
  let currentModel: DebugGraphModel | null = null;
  let definitionSignature = "";
  let lastRuntimeNodeId: string | null = null;

  function setDisclosure(open: boolean): void {
    shell.debugPanel.hidden = !open;
    shell.debugToggle.setAttribute("aria-expanded", String(open));
    const label = open ? "Close runtime debugger" : "Open runtime debugger";
    shell.debugToggle.setAttribute("aria-label", label);
    shell.debugToggle.setAttribute("title", label);
    if (open) shell.container.dataset["debugOpen"] = "true";
    else delete shell.container.dataset["debugOpen"];
    if (open) resizeController.syncLayout();
    options.onOpenChange?.(open);
  }

  function renderDetails(node: DebugGraphNode | null): void {
    const heading = element(
      shell.document,
      "h3",
      undefined,
      node?.label ?? "Selection",
    );
    heading.id = "uh-debug-details-title";
    if (!node) {
      shell.debugDetails.replaceChildren(
        heading,
        element(
          shell.document,
          "p",
          undefined,
          "Select a state, event, handler, or effect to inspect it.",
        ),
      );
      return;
    }

    const identity = element(shell.document, "code", "uh-debug-identity", node.id);
    const activity = element(
      shell.document,
      "p",
      "uh-debug-activity",
      nodeRuntimeText(node),
    );
    const children: Node[] = [heading, identity];
    if (node.detail) {
      children.push(
        element(shell.document, "p", "uh-debug-value", node.detail),
      );
    }
    children.push(activity);
    if (node.span) {
      children.push(
        element(
          shell.document,
          "p",
          "uh-debug-source",
          node.span.file
            + ":"
            + String(node.span.start)
            + "-"
            + String(node.span.end)
            + " · UTF-8 bytes",
        ),
      );
    }
    const nodeLabels = new Map(
      currentModel?.nodes.map((candidate) => [candidate.id, candidate.label]) ?? [],
    );
    const incoming = currentModel?.edges
      .filter((edge) => edge.to === node.id)
      .map((edge) => `${nodeLabels.get(edge.from) ?? edge.from}: ${edge.label}`)
      ?? [];
    const outgoing = currentModel?.edges
      .filter((edge) => edge.from === node.id)
      .map((edge) => `${edge.label}: ${nodeLabels.get(edge.to) ?? edge.to}`)
      ?? [];
    if (incoming.length > 0) {
      children.push(
        element(
          shell.document,
          "p",
          "uh-debug-topology",
          "Incoming · " + incoming.join(" · "),
        ),
      );
    }
    if (outgoing.length > 0) {
      children.push(
        element(
          shell.document,
          "p",
          "uh-debug-topology",
          "Outgoing · " + outgoing.join(" · "),
        ),
      );
    }
    shell.debugDetails.replaceChildren(...children);
  }

  function selectNode(nodeId: string | null, selectedByUser: boolean): void {
    selectedNodeId = nodeId;
    if (selectedByUser) userSelectedNode = true;
    for (const button of shell.debugGraph.querySelectorAll<HTMLButtonElement>(
      "[data-uh-debug-node]",
    )) {
      const selected = button.dataset["uhDebugNode"] === nodeId;
      button.setAttribute("aria-pressed", String(selected));
      button.classList.toggle("is-selected", selected);
      button.tabIndex = selected ? 0 : -1;
    }
    const node = currentModel?.nodes.find((candidate) => candidate.id === nodeId) ?? null;
    renderDetails(node);
  }

  function syncDefinitions(model: DebugGraphModel): void {
    const signature = model.definitions
      .map((definition) =>
        definition.id + "|" + definition.kind + "|" + String(definition.entry))
      .join(";");
    if (signature !== definitionSignature) {
      definitionSignature = signature;
      const options = model.definitions.map((definition) => {
        const option = shell.document.createElement("option");
        option.value = definition.id;
        return option;
      });
      shell.debugDefinition.replaceChildren(...options);
    }
    const optionsById = new Map(
      [...shell.debugDefinition.options].map((option) => [option.value, option]),
    );
    for (const definition of model.definitions) {
      const option = optionsById.get(definition.id);
      if (option) option.textContent = definitionText(definition);
    }
    shell.debugDefinition.disabled = model.definitions.length === 0;
    shell.debugFollowLive.disabled = model.definitions.length === 0;
    if (model.focusDefinitionId) {
      shell.debugDefinition.value = model.focusDefinitionId;
    }
    shell.debugFollowLive.setAttribute("aria-pressed", String(followLive));
  }

  function marker(
    id: string,
    className: string,
  ): SVGMarkerElement {
    const marker = svgElement(shell.document, "marker") as SVGMarkerElement;
    marker.id = id;
    marker.setAttribute("markerWidth", "7");
    marker.setAttribute("markerHeight", "7");
    marker.setAttribute("refX", "6");
    marker.setAttribute("refY", "3.5");
    marker.setAttribute("orient", "auto");
    marker.setAttribute("viewBox", "0 0 7 7");
    const arrow = svgElement(shell.document, "path");
    arrow.setAttribute("d", "M0 0 L7 3.5 L0 7 Z");
    arrow.setAttribute("class", className);
    marker.append(arrow);
    return marker;
  }

  function edgeMarker(activity: string): string {
    if (activity === "taken") return "url(#uh-debug-arrow-taken)";
    if (activity === "context") return "url(#uh-debug-arrow-context)";
    return "url(#uh-debug-arrow-idle)";
  }

  function renderGraph(model: DebugGraphModel): void {
    if (model.nodes.length === 0) {
      viewportController.clearLayout();
      shell.debugGraphContent.replaceChildren(
        element(
          shell.document,
          "p",
          "uh-debug-empty",
          emptyMessage(model.emptyReason),
        ),
      );
      selectedNodeId = null;
      userSelectedNode = false;
      lastRuntimeNodeId = null;
      renderDetails(null);
      return;
    }

    const focusedNodeId = shell.document.activeElement
      ?.getAttribute("data-uh-debug-node") ?? null;
    const layout = layoutDebugGraph(model);
    const canvas = element(shell.document, "div", "uh-debug-canvas");
    canvas.style.inlineSize = String(layout.width) + "px";
    canvas.style.blockSize = String(layout.height) + "px";

    const svg = svgElement(shell.document, "svg") as SVGSVGElement;
    svg.setAttribute("class", "uh-debug-edges");
    svg.setAttribute("viewBox", layout.viewBox);
    svg.setAttribute("width", String(layout.width));
    svg.setAttribute("height", String(layout.height));
    svg.style.inlineSize = String(layout.width) + "px";
    svg.style.blockSize = String(layout.height) + "px";
    svg.setAttribute("aria-hidden", "true");
    const definitions = svgElement(shell.document, "defs");
    definitions.append(
      marker("uh-debug-arrow-idle", "uh-debug-marker-idle"),
      marker("uh-debug-arrow-context", "uh-debug-marker-context"),
      marker("uh-debug-arrow-taken", "uh-debug-marker-taken"),
    );
    svg.append(definitions);

    for (const lane of layout.lanes) {
      const label = svgElement(shell.document, "text");
      label.setAttribute("class", "uh-debug-lane-label");
      label.setAttribute("x", String(lane.x));
      label.setAttribute("y", "18");
      label.textContent = lane.label;
      svg.append(label);
    }
    for (const layoutEdge of layout.edges) {
      const path = svgElement(shell.document, "path");
      path.setAttribute(
        "class",
        "uh-debug-edge uh-debug-edge-"
          + layoutEdge.edge.kind
          + " is-"
          + layoutEdge.edge.activity,
      );
      path.setAttribute("d", layoutEdge.path);
      path.setAttribute("marker-end", edgeMarker(layoutEdge.edge.activity));
      const title = svgElement(shell.document, "title");
      title.textContent = layoutEdge.edge.label;
      path.append(title);
      svg.append(path);
    }
    canvas.append(svg);

    const validIds = new Set(model.nodes.map((node) => node.id));
    if (selectedNodeId !== null && !validIds.has(selectedNodeId)) {
      selectedNodeId = null;
      userSelectedNode = false;
    }
    const runtimeNode = model.nodes.find((node) => node.runtime.selected)
      ?? model.nodes.find((node) => node.runtime.current)
      ?? null;
    if (!userSelectedNode && runtimeNode !== null) {
      selectedNodeId = runtimeNode.id;
    } else if (selectedNodeId === null) {
      selectedNodeId = runtimeNode?.id
        ?? model.nodes.find((node) => node.kind === "handler")?.id
        ?? model.nodes[0]?.id
        ?? null;
    }

    for (const layoutNode of layout.nodes) {
      const node = layoutNode.node;
      const button = shell.document.createElement("button");
      button.type = "button";
      button.className = nodeClasses(node, node.id === selectedNodeId);
      button.dataset["uhDebugNode"] = node.id;
      button.dataset["uhDebugLane"] = node.lane;
      button.style.left = String(layoutNode.x) + "px";
      button.style.top = String(layoutNode.y) + "px";
      button.style.inlineSize = String(layoutNode.width) + "px";
      button.style.blockSize = String(layoutNode.height) + "px";
      button.setAttribute("aria-pressed", String(node.id === selectedNodeId));
      button.tabIndex = node.id === selectedNodeId ? 0 : -1;
      button.setAttribute(
        "aria-label",
        node.label
          + ", "
          + node.kind
          + (node.detail ? ", " + node.detail : "")
          + ", "
          + nodeRuntimeText(node),
      );

      const meta = element(shell.document, "span", "uh-debug-node-meta");
      meta.append(
        element(shell.document, "span", "uh-debug-node-kind", node.kind),
        element(shell.document, "span", "uh-debug-node-status", nodeStatus(node)),
      );
      button.append(
        meta,
        element(shell.document, "strong", "uh-debug-node-label", node.label),
      );
      if (node.detail) {
        button.append(
          element(shell.document, "span", "uh-debug-node-detail", node.detail),
        );
      }
      button.addEventListener("click", () => selectNode(node.id, true));
      canvas.append(button);
    }

    shell.debugGraphContent.replaceChildren(canvas);
    viewportController.setLayout(canvas, layout.width, layout.height);
    currentModel = model;
    selectNode(selectedNodeId, false);

    if (focusedNodeId) {
      const nodeButtons = [...shell.debugGraph.querySelectorAll<HTMLButtonElement>(
        "[data-uh-debug-node]",
      )];
      const replacement = nodeButtons.find(
        (button) => button.dataset["uhDebugNode"] === focusedNodeId,
      ) ?? nodeButtons.find(
        (button) => button.dataset["uhDebugNode"] === selectedNodeId,
      );
      replacement?.focus({ preventScroll: true });
    }

    const runtimeNodeId = runtimeNode?.id ?? null;
    if (
      !userSelectedNode
      && runtimeNodeId !== null
      && runtimeNodeId !== lastRuntimeNodeId
    ) {
      const runtimeButton = [...shell.debugGraph.querySelectorAll<HTMLButtonElement>(
        "[data-uh-debug-node]",
      )].find((button) => button.dataset["uhDebugNode"] === runtimeNodeId);
      if (runtimeButton) {
        const zoom = viewportController.zoom;
        shell.debugGraph.scrollLeft = Math.max(
          0,
          runtimeButton.offsetLeft * zoom
            - (shell.debugGraph.clientWidth - runtimeButton.offsetWidth * zoom) / 2,
        );
        shell.debugGraph.scrollTop = Math.max(
          0,
          runtimeButton.offsetTop * zoom
            - (shell.debugGraph.clientHeight - runtimeButton.offsetHeight * zoom) / 2,
        );
      }
    }
    lastRuntimeNodeId = runtimeNodeId;
  }

  function renderInspection(state: InspectionState): void {
    lastState = state;
    const previousFocus = currentModel?.focusDefinitionId ?? null;
    const model = deriveDebugGraph(state, {
      focusDefinitionId: followLive ? null : pinnedDefinitionId,
    });
    if (followLive) pinnedDefinitionId = model.focusDefinitionId;
    if (previousFocus !== model.focusDefinitionId) {
      selectedNodeId = null;
      userSelectedNode = false;
      lastRuntimeNodeId = null;
    }
    currentModel = model;
    syncDefinitions(model);

    if (model.disposed) {
      shell.debugSummary.textContent = "Debugger unavailable · inspection retired";
    } else if (model.revision === null) {
      shell.debugSummary.textContent = model.generation === null
        ? "Waiting for checked program…"
        : "Program ready · waiting for first runtime step";
    } else {
      shell.debugSummary.textContent =
        (model.focusDefinitionId ?? "program")
        + " · revision "
        + String(model.revision)
        + " · "
        + traceEventLabel(state)
        + " · "
        + traceDisposition(state);
    }
    renderGraph(model);
  }

  function renderUpdate(update: DebugControllerUpdate): void {
    if (update.kind === "unavailable") {
      lastState = null;
      currentModel = null;
      definitionSignature = "";
      shell.debugDefinition.disabled = true;
      shell.debugFollowLive.disabled = true;
      shell.debugSummary.textContent = "Debugger unavailable";
      viewportController.clearLayout();
      shell.debugGraphContent.replaceChildren(
        element(
          shell.document,
          "p",
          "uh-debug-empty",
          "Runtime inspection could not be attached to this Play session.",
        ),
      );
      renderDetails(null);
      return;
    }
    renderInspection(update.state);
  }

  function clearTransientView(): void {
    lastState = null;
    currentModel = null;
    definitionSignature = "";
    lastRuntimeNodeId = null;
    const option = shell.document.createElement("option");
    option.value = "";
    option.textContent = "Waiting for program…";
    shell.debugDefinition.replaceChildren(option);
    shell.debugDefinition.disabled = true;
    shell.debugFollowLive.disabled = true;
    shell.debugSummary.textContent = "Waiting for runtime inspection…";
    viewportController.clearLayout();
    shell.debugGraphContent.replaceChildren(
      element(
        shell.document,
        "p",
        "uh-debug-empty",
        "The graph will appear after Play starts.",
      ),
    );
    renderDetails(null);
  }

  const controller = createDebugController({
    resolveInspection: () => inspection,
    requestFrame: (callback) => view.requestAnimationFrame(callback),
    cancelFrame: (handle) => view.cancelAnimationFrame(handle),
    render: renderUpdate,
  });

  function open(): void {
    if (disposed || controller.isOpen) return;
    setDisclosure(true);
    controller.open();
    shell.debugClose.focus();
  }

  function close(restoreFocus: boolean): void {
    if (disposed || !controller.isOpen) return;
    controller.close();
    clearTransientView();
    setDisclosure(false);
    if (restoreFocus) shell.debugToggle.focus();
  }

  const onToggle = (): void => {
    if (controller.isOpen) close(false);
    else open();
  };
  const onClose = (): void => close(true);
  const onDefinition = (): void => {
    if (!lastState || shell.debugDefinition.value.length === 0) return;
    followLive = false;
    pinnedDefinitionId = shell.debugDefinition.value;
    selectedNodeId = null;
    userSelectedNode = false;
    shell.debugFollowLive.setAttribute("aria-pressed", "false");
    renderInspection(lastState);
  };
  const onFollowLive = (): void => {
    if (!lastState) return;
    followLive = true;
    pinnedDefinitionId = null;
    selectedNodeId = null;
    userSelectedNode = false;
    shell.debugFollowLive.setAttribute("aria-pressed", "true");
    renderInspection(lastState);
  };
  const onPanelKeydown = (event: KeyboardEvent): void => {
    if (event.key !== "Escape" || !controller.isOpen) return;
    event.preventDefault();
    event.stopPropagation();
    close(true);
  };
  const onGraphKeydown = (event: KeyboardEvent): void => {
    if (
      event.key !== "ArrowUp"
      && event.key !== "ArrowDown"
      && event.key !== "ArrowLeft"
      && event.key !== "ArrowRight"
    ) {
      return;
    }
    const target = event.target;
    if (!target || !("getAttribute" in target)) return;
    const nodeId = (target as Element).getAttribute("data-uh-debug-node");
    const lane = (target as Element).getAttribute("data-uh-debug-lane");
    if (!nodeId || !lane) return;
    const buttons = [...shell.debugGraph.querySelectorAll<HTMLButtonElement>(
      "[data-uh-debug-node]",
    )];
    const current = buttons.find((button) => button.dataset["uhDebugNode"] === nodeId);
    if (!current) return;

    let replacement: HTMLButtonElement | undefined;
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      const peers = buttons.filter((button) => button.dataset["uhDebugLane"] === lane);
      const index = peers.indexOf(current);
      replacement = peers[event.key === "ArrowUp" ? index - 1 : index + 1];
    } else {
      const lanes = ["input", "handler", "effect"];
      const laneIndex = lanes.indexOf(lane);
      const nextLane = lanes[event.key === "ArrowLeft" ? laneIndex - 1 : laneIndex + 1];
      if (nextLane) {
        replacement = buttons
          .filter((button) => button.dataset["uhDebugLane"] === nextLane)
          .sort((left, right) =>
            Math.abs(left.offsetTop - current.offsetTop)
              - Math.abs(right.offsetTop - current.offsetTop))[0];
      }
    }
    if (!replacement) return;
    event.preventDefault();
    const replacementId = replacement.dataset["uhDebugNode"] ?? null;
    selectNode(replacementId, true);
    replacement.focus({ preventScroll: true });
    replacement.scrollIntoView({ block: "nearest", inline: "nearest" });
  };

  shell.debugToggle.addEventListener("click", onToggle);
  shell.debugClose.addEventListener("click", onClose);
  shell.debugDefinition.addEventListener("change", onDefinition);
  shell.debugFollowLive.addEventListener("click", onFollowLive);
  shell.debugPanel.addEventListener("keydown", onPanelKeydown);
  shell.debugGraph.addEventListener("keydown", onGraphKeydown);

  return Object.freeze({
    get isOpen() {
      return controller.isOpen;
    },
    dispose(): void {
      if (disposed) return;
      const wasOpen = controller.isOpen;
      disposed = true;
      controller.dispose();
      resizeController.dispose();
      viewportController.dispose();
      shell.debugToggle.removeEventListener("click", onToggle);
      shell.debugClose.removeEventListener("click", onClose);
      shell.debugDefinition.removeEventListener("change", onDefinition);
      shell.debugFollowLive.removeEventListener("click", onFollowLive);
      shell.debugPanel.removeEventListener("keydown", onPanelKeydown);
      shell.debugGraph.removeEventListener("keydown", onGraphKeydown);
      shell.debugPanel.hidden = true;
      shell.debugToggle.setAttribute("aria-expanded", "false");
      shell.debugToggle.setAttribute("aria-label", "Open runtime debugger");
      shell.debugToggle.setAttribute("title", "Open runtime debugger");
      delete shell.container.dataset["debugOpen"];
      if (wasOpen) options.onOpenChange?.(false);
      shell.debugGraphContent.replaceChildren();
      lastState = null;
      currentModel = null;
      selectedNodeId = null;
    },
  });
}
