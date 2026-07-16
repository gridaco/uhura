// One semantic VNode -> DOM engine. Play selects keyed reconciliation and
// runtime effects; Editor selects fresh realization and has no effect channel.

import { applyProps, tagFor, textOf } from "./appliers.js";
import type { AssetAppliers } from "./assets.js";
import type {
  RenderPolicy,
  RendererNode,
  ScrollHolder,
  TextFieldHolder,
} from "./contracts.js";
import type { IconFontRegistry } from "./icons.js";

interface Holder extends ScrollHolder, TextFieldHolder {
  node?: RendererNode;
  wiredPress: boolean;
}

type HeldElement = HTMLElement & { __uh?: Holder };

interface SemanticRendererContext {
  document: Document;
  icons: IconFontRegistry;
  assets: AssetAppliers;
  policy: RenderPolicy;
}

interface PendingRealization {
  path: readonly number[];
  element: HTMLElement;
}

type RealizationCollector = (realization: PendingRealization) => void;

export interface SemanticRenderer {
  reconcileChildren(
    host: HTMLElement,
    nodes: RendererNode[],
    parentPath: string,
    parentIsList: boolean,
  ): void;
  realizeChildren(
    host: HTMLElement,
    nodes: RendererNode[],
    parentPath: string,
    parentIsList: boolean,
  ): void;
  realizeRoot(
    host: HTMLElement,
    node: RendererNode,
    parentPath: string,
    listItem: boolean,
    collect?: RealizationCollector,
  ): void;
  applyNode(
    el: HTMLElement,
    node: RendererNode,
    parentPath: string,
    listItem: boolean,
  ): void;
  /** Releases runtime effects owned anywhere under a subtree before removal. */
  disposeSubtree(root: HTMLElement): void;
}

function isHTMLElement(value: unknown): value is HTMLElement {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as HTMLElement).getAttribute === "function" &&
    typeof (value as HTMLElement).setAttribute === "function"
  );
}

function holderOf(el: HTMLElement): Holder {
  const held = el as HeldElement;
  if (!held.__uh) held.__uh = { path: "", on: {}, wiredPress: false };
  return held.__uh;
}

/** First runtime descriptor scope under a root. Editor never needs this. */
export function findScope(node: RendererNode): string | undefined {
  const first = node.on?.[0];
  if (first) return first.scope;
  for (const child of node.children ?? []) {
    const scope = findScope(child);
    if (scope) return scope;
  }
  return undefined;
}

export function createSemanticRenderer(ctx: SemanticRendererContext): SemanticRenderer {
  function disposeSubtree(root: HTMLElement): void {
    if (ctx.policy.kind === "play") ctx.policy.disposeSubtree(root);
  }

  function wireInput(el: HTMLElement): void {
    if (ctx.policy.kind !== "play") return;
    const policy = ctx.policy;
    const holder = holderOf(el);
    if (holder.wiredPress) return;
    holder.wiredPress = true;
    el.addEventListener("click", () => {
      const descriptor = holder.on["press"] ?? holder.on["activate"];
      if (descriptor) policy.emit(descriptor);
    });
    el.addEventListener("dblclick", (event) => {
      const descriptor = holder.on["activate-double"];
      if (descriptor) {
        event.preventDefault();
        policy.emit(descriptor);
      }
    });
    if (holder.node?.element === "region") {
      el.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        const descriptor =
          holder.on["activate"] ??
          holder.on["press"] ??
          holder.on["activate-double"];
        if (descriptor) {
          event.preventDefault();
          policy.emit(descriptor);
        }
      });
    }
  }

  function childHost(el: HTMLElement, element: string): HTMLElement {
    if (element === "pager") {
      const track = el.querySelector(":scope > .uh-track");
      if (isHTMLElement(track)) return track;
    }
    return el;
  }

  function applyNode(
    el: HTMLElement,
    node: RendererNode,
    parentPath: string,
    listItem: boolean,
    semanticPath?: readonly number[],
    collect?: RealizationCollector,
  ): void {
    const holder = holderOf(el);
    holder.path = `${parentPath}/${node.key}`;
    holder.node = node;
    holder.on = {};
    if (ctx.policy.kind === "play") {
      for (const descriptor of node.on ?? []) holder.on[descriptor.event] = descriptor;
    }

    const className = `uh-${node.element}${node.class ? ` ${node.class}` : ""}`;
    if (el.className !== className) el.className = className;
    if (el.getAttribute("data-key") !== node.key) el.setAttribute("data-key", node.key);
    if (el.getAttribute("data-path") !== holder.path) {
      el.setAttribute("data-path", holder.path);
    }

    applyProps(el, node, {
      document: ctx.document,
      icons: ctx.icons,
      assets: ctx.assets,
      policy: ctx.policy,
      holderOf,
    });

    if (listItem) el.setAttribute("role", "listitem");
    else if (el.getAttribute("role") === "listitem") el.removeAttribute("role");

    if (
      ctx.policy.kind === "play" &&
      (holder.on["press"] !== undefined ||
        holder.on["activate"] !== undefined ||
        holder.on["activate-double"] !== undefined ||
        node.element === "region")
    ) {
      wireInput(el);
    }

    if (node.element !== "text") {
      const isList = node.element === "view" && textOf(node.props["role"]) === "list";
      const host = childHost(el, node.element);
      if (ctx.policy.kind === "play") {
        reconcileChildren(host, node.children ?? [], holder.path, isList);
      } else {
        realizeChildrenAtPath(
          host,
          node.children ?? [],
          holder.path,
          isList,
          semanticPath,
          collect,
        );
      }
    }
    if (ctx.policy.kind === "play" && node.element === "scroll") {
      ctx.policy.scrolls.sync(el, holder);
    }
  }

  function createElement(node: RendererNode): HTMLElement {
    const element = ctx.document.createElement(tagFor(node.element));
    if (node.element === "button") (element as HTMLButtonElement).type = "button";
    return element;
  }

  function reconcileChildren(
    host: HTMLElement,
    nodes: RendererNode[],
    parentPath: string,
    parentIsList: boolean,
  ): void {
    const existing = new Map<string, HTMLElement>();
    for (const child of [...host.children]) {
      if (!isHTMLElement(child) || child.hasAttribute("data-uh-mechanic")) continue;
      const key = child.getAttribute("data-key");
      if (key !== null) existing.set(key, child);
    }

    let previous: HTMLElement | null = null;
    for (const node of nodes) {
      let element = existing.get(node.key);
      if (element && holderOf(element).node?.element !== node.element) {
        existing.delete(node.key);
        disposeSubtree(element);
        element.remove();
        element = undefined;
      }
      if (element) existing.delete(node.key);
      else element = createElement(node);

      applyNode(element, node, parentPath, parentIsList);

      const desired: ChildNode | null = previous ? previous.nextSibling : host.firstChild;
      if (element !== desired) {
        const active = ctx.document.activeElement;
        const hadFocus =
          isHTMLElement(active) && (element === active || element.contains(active));
        host.insertBefore(element, desired);
        if (hadFocus && ctx.document.activeElement !== active) active.focus();
      }
      previous = element;
    }
    for (const leftover of existing.values()) {
      disposeSubtree(leftover);
      leftover.remove();
    }
  }

  function realizeChildren(
    host: HTMLElement,
    nodes: RendererNode[],
    parentPath: string,
    parentIsList: boolean,
  ): void {
    realizeChildrenAtPath(host, nodes, parentPath, parentIsList);
  }

  function realizeChildrenAtPath(
    host: HTMLElement,
    nodes: RendererNode[],
    parentPath: string,
    parentIsList: boolean,
    semanticParentPath?: readonly number[],
    collect?: RealizationCollector,
  ): void {
    // Remove only semantic children. A prop applier may already have created
    // mechanic children (textfield input, pager track/dots).
    for (const child of [...host.children]) {
      if (isHTMLElement(child) && child.hasAttribute("data-key")) {
        disposeSubtree(child);
        child.remove();
      }
    }
    for (const [index, node] of nodes.entries()) {
      const element = createElement(node);
      const semanticPath = semanticParentPath
        ? [...semanticParentPath, index]
        : undefined;
      if (semanticPath && collect) collect({ path: semanticPath, element });
      applyNode(
        element,
        node,
        parentPath,
        parentIsList,
        semanticPath,
        collect,
      );
      host.append(element);
    }
  }

  function realizeRoot(
    host: HTMLElement,
    node: RendererNode,
    parentPath: string,
    listItem: boolean,
    collect?: RealizationCollector,
  ): void {
    const element = createElement(node);
    const path: readonly number[] = [];
    if (collect) collect({ path, element });
    applyNode(element, node, parentPath, listItem, path, collect);
    host.append(element);
  }

  return {
    reconcileChildren,
    realizeChildren,
    realizeRoot,
    applyNode,
    disposeSubtree,
  };
}
