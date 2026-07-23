import type { BrowserLocation } from "../app/router.js";
import {
  hostPath,
  stripHostPath,
  UHURA_HOST_BASE,
} from "../app/host.js";

export const PLAY_COMPATIBILITY_PATH = hostPath("/play");

/**
 * The host keeps `/` as the friendly Editor entry, while an application's
 * checked route table is still allowed to own `/`. `/play` is therefore a
 * browser-shell alias for the application's root location, never a second
 * route in the machine.
 */
export const applicationPathForBrowser = (
  location: BrowserLocation,
): string => {
  const hosted = stripHostPath(UHURA_HOST_BASE, location.pathname);
  if (hosted === null) {
    throw new Error(
      `browser location ${JSON.stringify(location.pathname)} is outside the Uhura host`,
    );
  }
  const pathname =
    hosted === "/play"
      || hosted === "/play/"
      ? "/"
      : hosted;
  return `${pathname}${location.search}${location.hash}`;
};

/** Maps a checked application URL back into the host-owned browser topology. */
export const browserUrlForApplication = (
  applicationUrl: string,
  baseUrl: string,
): URL => {
  const destination = new URL(applicationUrl, baseUrl);
  destination.pathname = hostPath(
    destination.pathname === "/" ? "/play" : destination.pathname,
  );
  return destination;
};
