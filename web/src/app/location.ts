import type { LocationChange } from "../app/router.js";

export type LocationConsumer = (change: LocationChange) => void;

const consumers = new Set<LocationConsumer>();
let latest: LocationChange | null = null;

/** Publishes the browser router's committed location to the mounted Play runtime. */
export const publishLocation = (change: LocationChange): void => {
  latest = change;
  for (const consumer of [...consumers]) consumer(change);
};

/** Subscribes one app-owned route adapter until its provider is disposed. */
export const installLocationConsumer = (
  next: LocationConsumer,
): (() => void) => {
  consumers.add(next);
  try {
    if (latest !== null) next(latest);
  } catch (error) {
    consumers.delete(next);
    throw error;
  }
  return () => {
    consumers.delete(next);
  };
};
