export type RealizationOwner = object;

export const realizationKey = (key: string): string => `key|${key}`;

/**
 * Direct semantic-node handles and their geometry subscriptions for one
 * realized preview frame. Ownership is explicit because the same ShadowRoot
 * can move from one prepared model to the next without being recreated.
 */
export class RealizationResources {
  readonly #elements = new Map<string, HTMLElement>();
  #owner: RealizationOwner | null = null;
  #watchDisposers: Array<() => void> = [];
  #disposed = false;

  get disposed(): boolean {
    return this.#disposed;
  }

  claim(owner: RealizationOwner): void {
    if (this.#disposed) throw new Error("cannot claim disposed realization resources");
    if (this.#owner !== null) throw new Error("realization resources already have an owner");
    this.#owner = owner;
  }

  canTransfer(from: RealizationOwner): boolean {
    return !this.#disposed && this.#owner === from;
  }

  transfer(from: RealizationOwner, to: RealizationOwner): void {
    if (!this.canTransfer(from)) {
      throw new Error("only the current owner can transfer realization resources");
    }
    this.#owner = to;
  }

  registerKey(key: string, element: HTMLElement): void {
    if (this.#disposed) throw new Error("cannot register into disposed realization resources");
    const realization = realizationKey(key);
    if (this.#elements.has(realization)) {
      throw new Error(`duplicate semantic realization ${realization}`);
    }
    this.#elements.set(realization, element);
  }

  resolve(key: string): HTMLElement | null {
    if (this.#disposed) return null;
    return this.#elements.get(realizationKey(key)) ?? null;
  }

  realizedElements(): readonly HTMLElement[] {
    return [...this.#elements.values()];
  }

  /**
   * Rebinds layout invalidation to the current prepared model. Captured scroll
   * and load events cross open ShadowRoots through the direct roots supplied by
   * registered elements; no selector lookup is involved.
   */
  watch(
    owner: RealizationOwner,
    frame: HTMLElement,
    window: Window,
    invalidate: () => void,
  ): void {
    if (this.#disposed || this.#owner !== owner) {
      throw new Error("only the current owner can watch realization resources");
    }
    this.#clearWatchers();

    const ResizeObserverType = (window as Window & {
      ResizeObserver?: typeof ResizeObserver;
    }).ResizeObserver;
    if (ResizeObserverType) {
      const observer = new ResizeObserverType(invalidate);
      observer.observe(frame);
      for (const element of this.#elements.values()) observer.observe(element);
      this.#watchDisposers.push(() => observer.disconnect());
    }

    const roots = new Set<EventTarget>([frame]);
    for (const element of this.#elements.values()) roots.add(element.getRootNode());
    const onGeometryEvent = (): void => invalidate();
    for (const root of roots) {
      root.addEventListener("scroll", onGeometryEvent, { capture: true, passive: true });
      root.addEventListener("load", onGeometryEvent, { capture: true, passive: true });
      this.#watchDisposers.push(() => {
        root.removeEventListener("scroll", onGeometryEvent, { capture: true });
        root.removeEventListener("load", onGeometryEvent, { capture: true });
      });
    }
  }

  release(owner: RealizationOwner): void {
    if (this.#owner !== owner) return;
    this.#dispose();
  }

  #clearWatchers(): void {
    for (const dispose of this.#watchDisposers.splice(0)) dispose();
  }

  #dispose(): void {
    if (this.#disposed) return;
    this.#clearWatchers();
    this.#elements.clear();
    this.#owner = null;
    this.#disposed = true;
  }
}
