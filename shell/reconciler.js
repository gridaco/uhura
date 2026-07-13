// Keyed V→DOM reconciliation (§8.2, §8.4): one insertBefore sweep per
// sibling list — element change replaces, key match patches in place,
// appends never move existing nodes. Every node carries `data-key`
// (sibling identity) and `data-path` (`<scope>/<keys…>` — global identity,
// what focus-restore key-paths resolve against).
//
// Renderer state lives on the DOM elements themselves (`el.__uh` holder):
// listeners are attached once and read the CURRENT descriptors at fire
// time, so re-renders never re-bind.

import { applyProps, tagFor, textOf } from "./appliers.js";

/**
 * @typedef {Object} Holder
 * @property {string} path
 * @property {import("./types.js").VNode} node
 * @property {Record<string, import("./types.js").Descriptor>} on
 * @property {boolean} wiredPress
 * @property {{ sentinel: HTMLElement, io: IntersectionObserver, armed: boolean,
 *              lastHeight: number }} [nearEnd]
 */

/** @param {HTMLElement} el @returns {Holder} */
function holderOf(el) {
  const anyEl = /** @type {any} */ (el);
  if (!anyEl.__uh) {
    anyEl.__uh = { path: "", node: undefined, on: {}, wiredPress: false };
  }
  return anyEl.__uh;
}

/**
 * The first descriptor scope under a root — a page/surface root's scope
 * prefix for key-paths. V itself never names the page serial; its
 * descriptors do (§8.1).
 * @param {import("./types.js").VNode} node
 * @returns {string | undefined}
 */
export function findScope(node) {
  const first = node.on?.[0];
  if (first) return first.scope;
  for (const child of node.children ?? []) {
    const scope = findScope(child);
    if (scope) return scope;
  }
  return undefined;
}

/**
 * @param {{
 *   emit: (descriptor: import("./types.js").Descriptor,
 *          data?: Record<string, unknown>,
 *          onApplied?: () => void) => void,
 *   glyphs: Record<string, string>,
 *   textFields: ReturnType<typeof import("./textfield.js").createTextFields>,
 *   scrolls: ReturnType<typeof import("./scroll.js").createScrolls>,
 * }} ctx
 */
export function createReconciler(ctx) {
  /** @param {HTMLElement} el */
  function wireInput(el) {
    const holder = holderOf(el);
    if (holder.wiredPress) return;
    holder.wiredPress = true;
    el.addEventListener("click", () => {
      const d = holder.on["press"] ?? holder.on["activate"];
      if (d) ctx.emit(d);
    });
    el.addEventListener("dblclick", (event) => {
      const d = holder.on["activate-double"];
      if (d) {
        event.preventDefault();
        ctx.emit(d);
      }
    });
    if (holder.node.element === "region") {
      // Keyboard path for non-native interactives (§11.4 step 4):
      // focus + Enter/Space activates — double-only regions included.
      el.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        const d =
          holder.on["activate"] ?? holder.on["press"] ?? holder.on["activate-double"];
        if (d) {
          event.preventDefault();
          ctx.emit(d);
        }
      });
    }
  }

  /**
   * Where a node's KEYED children live (pager wraps them in its track).
   * @param {HTMLElement} el @param {string} element
   */
  function childHost(el, element) {
    if (element === "pager") {
      const track = el.querySelector(":scope > .uh-track");
      if (track instanceof HTMLElement) return track;
    }
    return el;
  }

  /**
   * Patches one element in place: identity attributes, semantic props,
   * event wiring, keyed children.
   * @param {HTMLElement} el
   * @param {import("./types.js").VNode} node
   * @param {string} parentPath
   * @param {boolean} listItem
   */
  function applyNode(el, node, parentPath, listItem) {
    const holder = holderOf(el);
    holder.path = `${parentPath}/${node.key}`;
    holder.node = node;
    holder.on = {};
    for (const d of node.on ?? []) holder.on[d.event] = d;

    const className = `uh-${node.element}${node.class ? ` ${node.class}` : ""}`;
    if (el.className !== className) el.className = className;
    if (el.getAttribute("data-key") !== node.key) el.setAttribute("data-key", node.key);
    if (el.getAttribute("data-path") !== holder.path) {
      el.setAttribute("data-path", holder.path);
    }

    applyProps(el, node, {
      glyphs: ctx.glyphs,
      textFields: ctx.textFields,
      holderOf,
    });

    // A list's children are its items — set AFTER element roles so the
    // list semantics win (mirrors the static renderer); a role the list
    // stamped earlier is withdrawn when the parent stops being a list.
    if (listItem) el.setAttribute("role", "listitem");
    else if (el.getAttribute("role") === "listitem") el.removeAttribute("role");

    if (
      holder.on["press"] ||
      holder.on["activate"] ||
      holder.on["activate-double"] ||
      node.element === "region"
    ) {
      wireInput(el);
    }

    if (node.element !== "text") {
      const isList = node.element === "view" && textOf(node.props["role"]) === "list";
      reconcileChildren(
        childHost(el, node.element),
        node.children ?? [],
        holder.path,
        isList,
      );
    }
    if (node.element === "scroll") {
      // After children: the sentinel must trail appended rows, and the
      // observation tears down when the descriptor is gone.
      ctx.scrolls.sync(el, holder);
    }
  }

  /**
   * The keyed insertBefore sweep. Mechanic children (`data-uh-mechanic`:
   * text-field inputs, pager track/dots, scroll sentinels) are invisible
   * to it.
   * @param {HTMLElement} host
   * @param {import("./types.js").VNode[]} nodes
   * @param {string} parentPath
   * @param {boolean} parentIsList
   */
  function reconcileChildren(host, nodes, parentPath, parentIsList) {
    /** @type {Map<string, HTMLElement>} */
    const existing = new Map();
    for (const child of [...host.children]) {
      if (!(child instanceof HTMLElement)) continue;
      if (child.hasAttribute("data-uh-mechanic")) continue;
      const key = child.getAttribute("data-key");
      if (key !== null) existing.set(key, child);
    }

    /** @type {HTMLElement | null} */
    let prev = null;
    for (const node of nodes) {
      let el = existing.get(node.key);
      if (el && holderOf(el).node?.element !== node.element) {
        // Element change ⇒ replace: identity is (key, element) — a key
        // that changes element is a different thing (§8.2).
        existing.delete(node.key);
        el.remove();
        el = undefined;
      }
      if (el) {
        existing.delete(node.key);
      } else {
        el = document.createElement(tagFor(node.element));
        if (node.element === "button") {
          /** @type {HTMLButtonElement} */ (el).type = "button";
        }
      }
      applyNode(el, node, parentPath, parentIsList);

      /** @type {ChildNode | null} */
      const desired = prev ? prev.nextSibling : host.firstChild;
      if (el !== desired) {
        // insertBefore MOVES the node, and moving blurs any focus inside
        // it — put focus back where the user had it.
        const active = document.activeElement;
        const hadFocus =
          active instanceof HTMLElement && (el === active || el.contains(active));
        host.insertBefore(el, desired);
        if (hadFocus && document.activeElement !== active) active.focus();
      }
      prev = el;
    }
    for (const leftover of existing.values()) leftover.remove();
  }

  return { reconcileChildren, applyNode };
}
