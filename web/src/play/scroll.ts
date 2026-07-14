// scroll mechanics (§8.4): near-end observation via a sentinel +
// IntersectionObserver (rootMargin 100% — the catalog's stated
// threshold), with an EDGE LATCH: one emission per entry into the
// near-end zone; re-arms only after the sentinel leaves. Wiggle-scroll
// at the bottom emits nothing (the machine's guard is the backstop, the
// latch keeps the trace clean). Plus the per-route scroll cache
// (micro-decision #17).

import type { Descriptor } from "../protocol/types.js";

interface ScrollPosition {
  top: number;
  left: number;
}

export interface NearEndState {
  sentinel: HTMLElement;
  io: IntersectionObserver;
  armed: boolean;
  lastHeight: number;
}

export interface ScrollHolder {
  path: string;
  on: Record<string, Descriptor>;
  nearEnd?: NearEndState;
}

interface ScrollWiring {
  emit(descriptor: Descriptor): void;
}

export interface ScrollController {
  sync(el: HTMLElement, holder: ScrollHolder): void;
  savePositions(navKey: string, pageEl: HTMLElement): void;
  restorePositions(navKey: string, pageEl: HTMLElement): void;
}

export function createScrolls({ emit }: ScrollWiring): ScrollController {
  const routeCache = new Map<string, Map<string, ScrollPosition>>();

  /**
   * Keeps one scroll element's near-end observation in sync with its
   * CURRENT descriptors. `holder.on` rotates per step; descriptor
   * absence (exhausted feed) tears the sentinel down — descriptor
   * presence IS the subscription (§8.1).
   */
  function sync(el: HTMLElement, holder: ScrollHolder): void {
    const descriptor = holder.on["near-end"];
    if (!descriptor) {
      if (holder.nearEnd) {
        holder.nearEnd.io.disconnect();
        holder.nearEnd.sentinel.remove();
        holder.nearEnd = undefined;
      }
      return;
    }
    if (!holder.nearEnd) {
      const sentinel = document.createElement("div");
      sentinel.setAttribute("data-uh-mechanic", "sentinel");
      sentinel.style.cssText = "block-size:1px;flex:none;";
      el.append(sentinel);
      const nearEnd: NearEndState = {
        sentinel,
        armed: true,
        lastHeight: -1,
        io: new IntersectionObserver(
          (entries) => {
            for (const entry of entries) {
              if (entry.isIntersecting && nearEnd.armed) {
                nearEnd.armed = false;
                const d = holder.on["near-end"];
                if (d) emit(d);
              } else if (!entry.isIntersecting) {
                nearEnd.armed = true; // left the zone — re-arm the latch
              }
            }
          },
          { root: el, rootMargin: "100%" },
        ),
      };
      nearEnd.io.observe(sentinel);
      holder.nearEnd = nearEnd;
    }
    const nearEnd = holder.nearEnd;
    if (nearEnd.sentinel !== el.lastElementChild) {
      el.append(nearEnd.sentinel); // keep it after appended rows
    }
    if (el.scrollHeight !== nearEnd.lastHeight) {
      // Content changed. The catalog's near-end threshold is a STATE
      // (remaining extent below one viewport — §10), not an edge: re-arm
      // and take a fresh observation, so a feed still inside the zone
      // after a short append keeps paginating instead of deadlocking.
      // The machine's guard is the backstop against spam.
      nearEnd.lastHeight = el.scrollHeight;
      nearEnd.armed = true;
      nearEnd.io.unobserve(nearEnd.sentinel);
      nearEnd.io.observe(nearEnd.sentinel);
    }
  }

  /**
   * Saves every scroll position under the outgoing page instance before
   * the page subtree remounts. The key is main.ts's nav key
   * (route + depth + params — register #17), so two `profile/[user]`
   * instances never share positions.
   */
  function savePositions(navKey: string, pageEl: HTMLElement): void {
    const positions = new Map<string, ScrollPosition>();
    for (const candidate of pageEl.querySelectorAll(".uh-scroll")) {
      if (!(candidate instanceof HTMLElement)) continue;
      // Keyed by data-key, NOT data-path: node keys are stable source
      // ordinals, while paths embed the page serial, which is freshly
      // minted on every remount.
      const key = candidate.getAttribute("data-key");
      if (key) {
        positions.set(key, {
          top: candidate.scrollTop,
          left: candidate.scrollLeft,
        });
      }
    }
    routeCache.set(navKey, positions);
  }

  /**
   * Restores cached positions after a page remount (back → the feed
   * exactly where it was). Unknown keys stay at 0 — a freshly pushed
   * instance starts at the top.
   */
  function restorePositions(navKey: string, pageEl: HTMLElement): void {
    const positions = routeCache.get(navKey);
    if (!positions) return;
    for (const candidate of pageEl.querySelectorAll(".uh-scroll")) {
      if (!(candidate instanceof HTMLElement)) continue;
      const key = candidate.getAttribute("data-key");
      const saved = key ? positions.get(key) : undefined;
      if (saved) {
        candidate.scrollTop = saved.top;
        candidate.scrollLeft = saved.left;
      }
    }
  }

  return { sync, savePositions, restorePositions };
}
