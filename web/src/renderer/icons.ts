export interface IconPaint {
  fill?: string;
  stroke?: string;
  strokeWidth?: string | number;
  lineCap?: "butt" | "round" | "square";
  lineJoin?: "miter" | "round" | "bevel";
  opacity?: string | number;
}

/**
 * Browser-contract command shape. The decoder guarantees JSON values and a
 * string kind; the renderer admits only its closed path/circle/rect subset.
 */
export interface IconCommand {
  kind: string;
  [property: string]: unknown;
}

export interface IconDefinition {
  viewBox: [number, number, number, number];
  commands: IconCommand[];
}

export type IconTable = Record<string, IconDefinition>;

const SVG_NAMESPACE = "http://www.w3.org/2000/svg";

function setOptionalAttribute(
  element: Element,
  name: string,
  value: string | number | undefined,
): void {
  if (value !== undefined) element.setAttribute(name, String(value));
}

function stringProperty(command: IconCommand, name: string): string | undefined {
  const value = command[name];
  return typeof value === "string" ? value : undefined;
}

function scalarProperty(
  command: IconCommand,
  name: string,
): string | number | undefined {
  const value = command[name];
  if (typeof value === "string") return value;
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function applyPaint(element: Element, command: IconCommand): void {
  setOptionalAttribute(element, "fill", stringProperty(command, "fill"));
  setOptionalAttribute(element, "stroke", stringProperty(command, "stroke"));
  setOptionalAttribute(element, "stroke-width", scalarProperty(command, "strokeWidth"));
  setOptionalAttribute(element, "stroke-linecap", stringProperty(command, "lineCap"));
  setOptionalAttribute(element, "stroke-linejoin", stringProperty(command, "lineJoin"));
  setOptionalAttribute(element, "opacity", scalarProperty(command, "opacity"));
}

function appendCommand(document: Document, svg: SVGSVGElement, command: IconCommand): void {
  switch (command.kind) {
    case "path": {
      const d = stringProperty(command, "d");
      if (d === undefined) break;
      const path = document.createElementNS(SVG_NAMESPACE, "path");
      path.setAttribute("d", d);
      applyPaint(path, command);
      svg.append(path);
      break;
    }
    case "circle": {
      const cx = scalarProperty(command, "cx");
      const cy = scalarProperty(command, "cy");
      const r = scalarProperty(command, "r");
      if (cx === undefined || cy === undefined || r === undefined) break;
      const circle = document.createElementNS(SVG_NAMESPACE, "circle");
      circle.setAttribute("cx", String(cx));
      circle.setAttribute("cy", String(cy));
      circle.setAttribute("r", String(r));
      applyPaint(circle, command);
      svg.append(circle);
      break;
    }
    case "rect": {
      const x = scalarProperty(command, "x");
      const y = scalarProperty(command, "y");
      const width = scalarProperty(command, "width");
      const height = scalarProperty(command, "height");
      if (x === undefined || y === undefined || width === undefined || height === undefined) {
        break;
      }
      const rect = document.createElementNS(SVG_NAMESPACE, "rect");
      rect.setAttribute("x", String(x));
      rect.setAttribute("y", String(y));
      rect.setAttribute("width", String(width));
      rect.setAttribute("height", String(height));
      setOptionalAttribute(rect, "rx", scalarProperty(command, "rx"));
      applyPaint(rect, command);
      svg.append(rect);
      break;
    }
  }
}

function fallbackIcon(): IconDefinition {
  return {
    viewBox: [0, 0, 24, 24],
    commands: [
      {
        kind: "circle",
        cx: 12,
        cy: 12,
        r: 8,
        fill: "none",
        stroke: "currentColor",
        strokeWidth: 1.8,
      },
    ],
  };
}

function isStructuredIcon(value: IconDefinition | undefined): value is IconDefinition {
  return value !== undefined && Array.isArray(value.commands);
}

/** Replaces an icon host's contents from a checked icon table. */
export function applyIcon(
  document: Document,
  host: HTMLElement,
  icon: IconDefinition | undefined,
): void {
  const svg = document.createElementNS(SVG_NAMESPACE, "svg");
  svg.setAttribute("width", "24");
  svg.setAttribute("height", "24");
  const definition = isStructuredIcon(icon) ? icon : fallbackIcon();
  svg.setAttribute("viewBox", definition.viewBox.join(" "));
  for (const command of definition.commands) appendCommand(document, svg, command);

  host.replaceChildren(svg);
}
