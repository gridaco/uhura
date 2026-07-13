// The surface stack (§8.1, §8.4): overlays keyed by `"definition:serial"`,
// scrim + Escape wired to the surface's FIRST-CLASS dismiss descriptor —
// the shell never invents a dismiss event, it presses the one core
// published. The page (and every non-top surface) is `inert` while
// anything is stacked above it.

import * as focus from "./focus.js";
import { findScope } from "./reconciler.js";

/**
 * @param {{
 *   host: HTMLElement,
 *   pageHost: HTMLElement,
 *   emit: (descriptor: import("./types.js").Descriptor) => void,
 *   reconcileChildren: (host: HTMLElement,
 *     nodes: import("./types.js").VNode[],
 *     parentPath: string, parentIsList: boolean) => void,
 * }} ctx
 */
export function createSurfaces(ctx) {
  /** @type {import("./types.js").SurfaceView[]} */
  let stack = [];

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || stack.length === 0) return;
    // An Escape that cancels an IME composition is not a dismiss gesture
    // (WebKit stamps keyCode 229 even after compositionend).
    if (event.isComposing || event.keyCode === 229) return;
    const top = stack[stack.length - 1];
    if (top) {
      event.preventDefault();
      ctx.emit(top.dismiss);
    }
  });

  /** @param {import("./types.js").Snapshot} snapshot */
  function render(snapshot) {
    stack = snapshot.surfaces;
    const wanted = new Set(stack.map((s) => s.key));
    for (const el of [...ctx.host.children]) {
      if (el instanceof HTMLElement && !wanted.has(el.dataset["surfaceKey"] ?? "")) {
        el.remove();
      }
    }

    stack.forEach((surface, index) => {
      let overlay = /** @type {HTMLElement | null} */ (
        ctx.host.querySelector(
          `:scope > [data-surface-key="${CSS.escape(surface.key)}"]`,
        )
      );
      let mounted = false;
      if (!overlay) {
        mounted = true;
        overlay = document.createElement("div");
        overlay.className = "uh-surface-overlay";
        overlay.dataset["surfaceKey"] = surface.key;

        const scrim = document.createElement("div");
        scrim.className = "uh-scrim";
        scrim.addEventListener("click", () => {
          const current = stack.find((s) => s.key === overlay?.dataset["surfaceKey"]);
          if (current) ctx.emit(current.dismiss);
        });

        const panel = document.createElement("div");
        panel.className = `uh-surface uh-modality-${surface.modality}`;
        panel.setAttribute("role", "dialog");
        panel.setAttribute("aria-modal", "true");
        panel.setAttribute("tabindex", "-1");

        overlay.append(scrim, panel);
        ctx.host.append(overlay);
      }

      // Stack order: bottom → top in host order.
      const atIndex = ctx.host.children[index];
      if (atIndex !== overlay) ctx.host.insertBefore(overlay, atIndex ?? null);

      const panel = /** @type {HTMLElement} */ (overlay.lastElementChild);
      // The dismiss descriptor names the surface scope even when the
      // content has no interactive nodes.
      const scope = findScope(surface.root) ?? surface.dismiss.scope;
      ctx.reconcileChildren(panel, [surface.root], scope, false);

      // Only the top surface is interactive; everything below waits.
      overlay.inert = index !== stack.length - 1;
      if (mounted && index === stack.length - 1) focus.enterSurface(panel);
    });

    ctx.pageHost.inert = stack.length > 0;
  }

  return { render };
}
