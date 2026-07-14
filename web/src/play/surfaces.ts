// The surface stack (§8.1, §8.4): overlays keyed by `"definition:serial"`,
// scrim + Escape wired to the surface's FIRST-CLASS dismiss descriptor —
// the shell never invents a dismiss event, it presses the one core
// published. The page (and every non-top surface) is `inert` while
// anything is stacked above it.

import { findScope } from "../renderer/play.js";
import type {
  Descriptor,
  Snapshot,
  SurfaceView,
  VNode,
} from "../protocol/types.js";

interface SurfaceContext {
  host: HTMLElement;
  pageHost: HTMLElement;
  emit(descriptor: Descriptor): void;
  reconcileChildren(
    host: HTMLElement,
    nodes: VNode[],
    parentPath: string,
    parentIsList: boolean,
  ): void;
  disposeSubtree(root: HTMLElement): void;
  enterSurface(surface: HTMLElement): void;
}

export interface SurfaceController {
  render(snapshot: Snapshot): void;
  dispose(): void;
}

export function createSurfaces(ctx: SurfaceContext): SurfaceController {
  let stack: SurfaceView[] = [];
  const document = ctx.host.ownerDocument;

  const onKeyDown = (event: KeyboardEvent): void => {
    if (event.key !== "Escape" || stack.length === 0) return;
    // An Escape that cancels an IME composition is not a dismiss gesture
    // (WebKit stamps keyCode 229 even after compositionend).
    if (event.isComposing || event.keyCode === 229) return;
    const top = stack[stack.length - 1];
    if (top) {
      event.preventDefault();
      ctx.emit(top.dismiss);
    }
  };
  document.addEventListener("keydown", onKeyDown);

  function render(snapshot: Snapshot): void {
    stack = snapshot.surfaces;
    const wanted = new Set(stack.map((s) => s.key));
    for (const el of [...ctx.host.children]) {
      if (el instanceof HTMLElement && !wanted.has(el.dataset["surfaceKey"] ?? "")) {
        ctx.disposeSubtree(el);
        el.remove();
      }
    }

    stack.forEach((surface, index) => {
      let overlay = ctx.host.querySelector<HTMLElement>(
        `:scope > [data-surface-key="${CSS.escape(surface.key)}"]`,
      );
      let mounted = false;
      if (!overlay) {
        mounted = true;
        const createdOverlay = document.createElement("div");
        createdOverlay.className = "uh-surface-overlay";
        createdOverlay.dataset["surfaceKey"] = surface.key;

        const scrim = document.createElement("div");
        scrim.className = "uh-scrim";
        scrim.addEventListener("click", () => {
          const current = stack.find(
            (s) => s.key === createdOverlay.dataset["surfaceKey"],
          );
          if (current) ctx.emit(current.dismiss);
        });

        const panel = document.createElement("div");
        panel.className = `uh-surface uh-modality-${surface.modality}`;
        panel.setAttribute("role", "dialog");
        panel.setAttribute("aria-modal", "true");
        panel.setAttribute("tabindex", "-1");

        createdOverlay.append(scrim, panel);
        ctx.host.append(createdOverlay);
        overlay = createdOverlay;
      }

      // Stack order: bottom → top in host order.
      const atIndex = ctx.host.children[index];
      if (atIndex !== overlay) ctx.host.insertBefore(overlay, atIndex ?? null);

      const panel = overlay.lastElementChild;
      if (!(panel instanceof HTMLElement)) {
        throw new Error(`surface ${surface.key} has no panel`);
      }
      // The dismiss descriptor names the surface scope even when the
      // content has no interactive nodes.
      const scope = findScope(surface.root) ?? surface.dismiss.scope;
      ctx.reconcileChildren(panel, [surface.root], scope, false);

      // Only the top surface is interactive; everything below waits.
      overlay.inert = index !== stack.length - 1;
      if (mounted && index === stack.length - 1) ctx.enterSurface(panel);
    });

    ctx.pageHost.inert = stack.length > 0;
  }

  function dispose(): void {
    stack = [];
    document.removeEventListener("keydown", onKeyDown);
    ctx.disposeSubtree(ctx.host);
    ctx.host.replaceChildren();
    ctx.pageHost.inert = false;
  }

  return { render, dispose };
}
