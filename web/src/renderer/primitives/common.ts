import type { Value } from "../../protocol/machine.js";
import type {
  RenderAttribute,
  RenderAttributeValue,
} from "../projection.js";
import type { ElementNode, PrimitiveHosts } from "./types.js";

export const UNIT_EVENT: Value = {
  $: "record",
  fields: [],
};

export const textEvent = (value: string): Value => ({
  $: "record",
  fields: [{
    name: "text",
    value: { $: "Text", value },
  }],
});

export const attribute = (
  attributes: readonly RenderAttribute[],
  name: string,
): RenderAttributeValue | undefined =>
  attributes.find((candidate) => candidate.name === name)?.value;

export const booleanAttribute = (
  attributes: readonly RenderAttribute[],
  name: string,
): boolean | undefined => {
  const value = attribute(attributes, name);
  return typeof value === "boolean" ? value : undefined;
};

export const textAttribute = (
  attributes: readonly RenderAttribute[],
  name: string,
): string | undefined => {
  const value = attribute(attributes, name);
  return typeof value === "string" ? value : undefined;
};

export const physicalAttribute = (
  name: string,
  value: RenderAttributeValue | undefined,
): RenderAttribute | null =>
  value === undefined ? null : { name, value };

export const presentBooleanAttribute = (
  name: string,
  value: boolean | undefined,
): RenderAttribute | null =>
  value === true ? { name, value: true } : null;

export const primitiveClass = (node: ElementNode): string => {
  const authored = textAttribute(node.attributes, "class")?.trim();
  return `uh-${node.element}${authored ? ` ${authored}` : ""}`;
};

export const semanticAttributes = (
  node: ElementNode,
  extras: readonly (RenderAttribute | null)[] = [],
): readonly RenderAttribute[] => [
  { name: "class", value: primitiveClass(node) },
  ...extras.filter(
    (candidate): candidate is RenderAttribute => candidate !== null,
  ),
];

export const defaultHosts = (element: HTMLElement): PrimitiveHosts => ({
  children: element,
  events: element,
});

export const directMechanic = (
  element: HTMLElement,
  mechanic: string,
): HTMLElement | undefined =>
  Array.from(element.children).find(
    (child): child is HTMLElement =>
      (child as HTMLElement).dataset?.["uhMechanic"] === mechanic,
  );
