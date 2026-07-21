import { natural } from "../../../protocol/machine.js";
import type { RenderNode } from "../../../renderer/projection.js";
import type { PreviewContent } from "../../editor-state.js";

export const elementNode = (
  key: string,
  children: readonly RenderNode[] = [],
  options: {
    element?: string;
    surface?: boolean;
    attributes?: readonly { name: string; value: boolean | string }[];
  } = {},
): RenderNode => ({
  kind: "element",
  key,
  element: options.element ?? "div",
  attributes: options.attributes ?? [],
  events: [],
  children,
  surface: options.surface ?? false,
});

export const textNode = (key: string, text: string): RenderNode => ({
  kind: "text",
  key,
  text,
});

const keys = (nodes: readonly RenderNode[]): string[] =>
  nodes.flatMap((node) => [
    node.key,
    ...(node.kind === "element" ? keys(node.children) : []),
  ]);

export const projectionContent = (
  nodes: readonly RenderNode[] = [elementNode("root")],
  presentation = "test@1::Web",
): PreviewContent => ({
  kind: "projection",
  value: {
    document: {
      protocol: "uhura-view/1",
      presentation,
      machine: "test@1::Machine",
      instance: "editor/test",
      sequence: natural("0"),
      nodes,
    },
    sources: {
      protocol: "uhura-projection-sources/0",
      presentation,
      nodes: Object.fromEntries(keys(nodes).map((key) => [
        key,
        { id: `ui/${key}`, path: "web.uhura", start: 0, end: 1 },
      ])),
    },
  },
});
