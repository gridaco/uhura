export type SurfaceDispose = () => void;
export type SurfaceMount = (
  root: HTMLElement,
) => void | SurfaceDispose;
export type SurfaceLoader = () => Promise<SurfaceMount>;

export type AppSurface = "editor" | "play";
export type NavigationCause = "start" | "push" | "replace" | "pop";

export interface AppRoute {
  pathname: string;
  surface: AppSurface;
}

export interface BrowserLocation {
  pathname: string;
  search: string;
  hash: string;
}

export interface LocationChange {
  cause: NavigationCause;
  location: BrowserLocation;
  route: AppRoute;
}

export interface RouterOptions {
  root: HTMLElement;
  loadEditor: SurfaceLoader;
  loadPlay: SurfaceLoader;
  /**
   * Runs only after the matching surface owns the route. Play can use this
   * seam to deliver a real pathname/query change to its router port without
   * remounting the running machine.
   */
  locationChanged?(change: LocationChange): void;
}

export interface AppRouter {
  start(): void;
  navigate(destination: string | URL, replace?: boolean): Promise<void>;
}

interface RouteRendererOptions extends RouterOptions {
  committed?(route: AppRoute): void;
}

export interface RouteRenderer {
  /**
   * Returns false when a newer route superseded this asynchronous render.
   * A route within the already-mounted surface commits without remounting it.
   */
  render(pathname: string): Promise<boolean>;
}

export const EDITOR_PATH = "/_uhura/editor";

const editorPath = (pathname: string): boolean =>
  pathname === "/"
  || pathname === EDITOR_PATH
  || pathname === `${EDITOR_PATH}/`;

/**
 * `/` remains the friendly Editor entry. The explicit reserved route makes
 * Editor addressable after an Uhura application owns ordinary web paths.
 * `/play` is the compatibility Play entry; every other pathname is an actual
 * application location and therefore also belongs to Play.
 */
export const routeFor = (pathname: string): AppRoute => ({
  pathname,
  surface: editorPath(pathname) ? "editor" : "play",
});

interface RoutedAnchor {
  url: URL;
}

const routedAnchor = (target: EventTarget | null): RoutedAnchor | null => {
  if (!(target instanceof Element)) return null;
  const anchor = target.closest("a[href]");
  if (!(anchor instanceof HTMLAnchorElement)) return null;
  if (anchor.target && anchor.target !== "_self") return null;
  if (anchor.hasAttribute("download")) return null;
  const url = new URL(anchor.href, location.href);
  if (url.origin !== location.origin) return null;
  return { url };
};

/** Loads first and mounts only after ownership is rechecked. */
export function createRouteRenderer(options: RouteRendererOptions): RouteRenderer {
  let dispose: SurfaceDispose | undefined;
  let transition = 0;
  let activeSurface: AppSurface | null = null;

  const render = async (pathname: string): Promise<boolean> => {
    const token = ++transition;
    const route = routeFor(pathname);
    if (route.surface === activeSurface) {
      options.committed?.(route);
      return true;
    }

    const mount = await (
      route.surface === "play"
        ? options.loadPlay()
        : options.loadEditor()
    );
    if (token !== transition) return false;
    dispose?.();
    dispose = undefined;
    options.root.replaceChildren();
    const mounted = mount(options.root);
    activeSurface = route.surface;
    options.committed?.(route);
    if (typeof mounted === "function") dispose = mounted;
    return true;
  };

  return { render };
}

export function createRouter(options: RouterOptions): AppRouter {
  const renderer = createRouteRenderer({
    ...options,
    committed(route) {
      document.documentElement.dataset["uhuraRoute"] = route.surface;
      document.title = route.surface === "play" ? "Uhura Play" : "Uhura Editor";
    },
  });

  let locationSequence = 0;

  const browserLocation = (url: URL): BrowserLocation => ({
    pathname: url.pathname,
    search: url.search,
    hash: url.hash,
  });

  const renderLocation = async (
    url: URL,
    cause: NavigationCause,
  ): Promise<void> => {
    const sequence = ++locationSequence;
    const committed = await renderer.render(url.pathname);
    if (!committed || sequence !== locationSequence) return;
    options.locationChanged?.({
      cause,
      location: browserLocation(url),
      route: routeFor(url.pathname),
    });
  };

  const navigate = async (
    destination: string | URL,
    replace = false,
  ): Promise<void> => {
    const url = new URL(destination, location.href);
    if (url.origin !== location.origin) {
      throw new Error(`cannot route a different origin: ${url.origin}`);
    }
    const href = `${url.pathname}${url.search}${url.hash}`;
    const current = `${location.pathname}${location.search}${location.hash}`;
    if (replace) {
      history.replaceState(null, "", href);
    } else if (current !== href) {
      history.pushState(null, "", href);
    }
    await renderLocation(url, replace ? "replace" : "push");
  };

  return {
    start(): void {
      window.addEventListener("popstate", () => {
        void renderLocation(new URL(location.href), "pop");
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
        const routed = routedAnchor(event.target);
        if (!routed) return;
        event.preventDefault();
        void navigate(routed.url);
      });
      void renderLocation(new URL(location.href), "start");
    },
    navigate,
  };
}
