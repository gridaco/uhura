import type { RuntimeHandle } from "../protocol/types.js";

declare global {
  interface Window {
    __uhura?: RuntimeHandle;
  }
}

export {};
