export type SurfaceDispose = () => void;
export type SurfaceMount = (
  root: HTMLElement,
) => void | SurfaceDispose;
export type SurfaceLoader = () => Promise<SurfaceMount>;

interface RouterOptions {
  root: HTMLElement;
  loadEditor: SurfaceLoader;
  loadPlay: SurfaceLoader;
}

export interface AppRouter {
  start(): void;
  navigate(path: "/" | "/play", replace?: boolean): Promise<void>;
}

interface RouteRendererOptions extends RouterOptions {
  committed?(path: "/" | "/play"): void;
}

export interface RouteRenderer {
  render(path: "/" | "/play"): Promise<void>;
}

const routeFor = (pathname: string): "/" | "/play" =>
  pathname === "/play" || pathname === "/play/" ? "/play" : "/";

const routedAnchor = (target: EventTarget | null): HTMLAnchorElement | null => {
  if (!(target instanceof Element)) return null;
  const anchor = target.closest("a[href]");
  if (!(anchor instanceof HTMLAnchorElement)) return null;
  if (anchor.target && anchor.target !== "_self") return null;
  const url = new URL(anchor.href, location.href);
  if (url.origin !== location.origin || (url.pathname !== "/" && url.pathname !== "/play")) {
    return null;
  }
  return anchor;
};

/** Loads first and mounts only after ownership is rechecked. */
export function createRouteRenderer(options: RouteRendererOptions): RouteRenderer {
  let dispose: SurfaceDispose | undefined;
  let transition = 0;

  const render = async (path: "/" | "/play"): Promise<void> => {
    const token = ++transition;
    const mount = await (path === "/play" ? options.loadPlay() : options.loadEditor());
    if (token !== transition) return;
    dispose?.();
    dispose = undefined;
    options.root.replaceChildren();
    options.committed?.(path);
    const mounted = mount(options.root);
    if (typeof mounted === "function") dispose = mounted;
  };

  return { render };
}

export function createRouter(options: RouterOptions): AppRouter {
  const renderer = createRouteRenderer({
    ...options,
    committed(path) {
      document.documentElement.dataset["uhuraRoute"] = path === "/play" ? "play" : "editor";
      document.title = path === "/play" ? "Uhura Play" : "Uhura Editor";
    },
  });

  const navigate = async (path: "/" | "/play", replace = false): Promise<void> => {
    const normalized = routeFor(path);
    if (replace) history.replaceState(null, "", normalized);
    else if (routeFor(location.pathname) !== normalized) history.pushState(null, "", normalized);
    await renderer.render(normalized);
  };

  return {
    start(): void {
      window.addEventListener("popstate", () => {
        void renderer.render(routeFor(location.pathname));
      });
      document.addEventListener("click", (event) => {
        if (
          event.defaultPrevented ||
          event.button !== 0 ||
          event.metaKey ||
          event.ctrlKey ||
          event.shiftKey ||
          event.altKey
        ) {
          return;
        }
        const anchor = routedAnchor(event.target);
        if (!anchor) return;
        event.preventDefault();
        const path = new URL(anchor.href, location.href).pathname as "/" | "/play";
        void navigate(path);
      });
      void renderer.render(routeFor(location.pathname));
    },
    navigate,
  };
}
