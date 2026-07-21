import { buttonAdapter } from "./button.js";
import { iconAdapter } from "./icon.js";
import { imageAdapter } from "./image.js";
import { pagerAdapter } from "./pager.js";
import { regionAdapter } from "./region.js";
import { scrollAdapter } from "./scroll.js";
import { textAdapter } from "./text.js";
import { textfieldAdapter } from "./textfield.js";
import type { PrimitiveAdapter } from "./types.js";
import { videoAdapter } from "./video.js";
import { viewAdapter } from "./view.js";
import type { RenderNode } from "../projection.js";
import type { PrimitiveCapability } from "./types.js";

const ADAPTERS = {
  view: viewAdapter,
  scroll: scrollAdapter,
  pager: pagerAdapter,
  text: textAdapter,
  img: imageAdapter,
  video: videoAdapter,
  icon: iconAdapter,
  button: buttonAdapter,
  textfield: textfieldAdapter,
  region: regionAdapter,
} as const satisfies Readonly<Record<string, PrimitiveAdapter>>;

export type PrimitiveAdapterId = keyof typeof ADAPTERS;

/**
 * Browser-owned realization vocabulary. Its stable explicit shape is the
 * renderer side of checker/renderer catalogue parity tests.
 */
export const PRIMITIVE_ADAPTER_IDS: readonly PrimitiveAdapterId[] =
  Object.freeze(Object.keys(ADAPTERS) as PrimitiveAdapterId[]);

export const primitiveAdapter = (
  id: string,
): PrimitiveAdapter | undefined =>
  Object.hasOwn(ADAPTERS, id)
    ? ADAPTERS[id as PrimitiveAdapterId]
    : undefined;

export const projectionUsesPrimitiveCapability = (
  nodes: readonly RenderNode[],
  capability: PrimitiveCapability,
): boolean => nodes.some((node) =>
  node.kind === "element"
  && (
    primitiveAdapter(node.element)?.capabilities?.includes(capability) === true
    || projectionUsesPrimitiveCapability(node.children, capability)
  )
);
