import type { BrowserLocation } from "../app/router.js";

export const PLAY_COMPATIBILITY_PATH = "/play" as const;

/**
 * The host keeps `/` as the friendly Editor entry, while an application's
 * checked route table is still allowed to own `/`. `/play` is therefore a
 * browser-shell alias for the application's root location, never a second
 * route in the machine.
 */
export const applicationPathForBrowser = (
  location: BrowserLocation,
): string => {
  const pathname =
    location.pathname === PLAY_COMPATIBILITY_PATH
      || location.pathname === `${PLAY_COMPATIBILITY_PATH}/`
      ? "/"
      : location.pathname;
  return `${pathname}${location.search}${location.hash}`;
};

/** Maps a checked application URL back into the host-owned browser topology. */
export const browserUrlForApplication = (
  applicationUrl: string,
  baseUrl: string,
): URL => {
  const destination = new URL(applicationUrl, baseUrl);
  if (destination.pathname === "/") {
    destination.pathname = PLAY_COMPATIBILITY_PATH;
  }
  return destination;
};
