export interface IconPaint {
  fill?: string;
  stroke?: string;
  strokeWidth?: string | number;
  lineCap?: "butt" | "round" | "square";
  lineJoin?: "miter" | "round" | "bevel";
  opacity?: string | number;
}

/**
 * Renderer-local command shape. Definitions may come from the bundled
 * provisional table or a future renderer resource; realization admits only
 * this closed path/circle/rect subset.
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

const icon = (...commands: IconCommand[]): IconDefinition => ({
  viewBox: [0, 0, 24, 24],
  commands,
});

const path = (d: string, paint: IconPaint): IconCommand => ({
  kind: "path",
  d,
  ...paint,
});

const circle = (
  cx: number,
  cy: number,
  r: number,
  paint: IconPaint,
): IconCommand => ({ kind: "circle", cx, cy, r, ...paint });

const rect = (
  x: number,
  y: number,
  width: number,
  height: number,
  rx: number | undefined,
  paint: IconPaint,
): IconCommand => ({
  kind: "rect",
  x,
  y,
  width,
  height,
  ...(rx === undefined ? {} : { rx }),
  ...paint,
});

const outline = (
  strokeWidth: number,
  extra: Partial<IconPaint> = {},
): IconPaint => ({
  fill: "none",
  stroke: "currentColor",
  strokeWidth,
  ...extra,
});

const stroke = (
  strokeWidth: number,
  extra: Partial<IconPaint> = {},
): IconPaint => ({
  stroke: "currentColor",
  strokeWidth,
  ...extra,
});

const filled = (): IconPaint => ({ fill: "currentColor" });

/**
 * Provisional browser-renderer glyphs for the current base-catalog names.
 *
 * This table preserves the spike's visual defaults, but it is renderer data:
 * it is not part of Uhura Core, EditorState, Play artifacts, or the language's
 * icon-family contract.
 *
 * FIXME(icon-font): Moving this table out of native engine/editor code fixes
 * the ownership violation only. Replace it with the font-only pre-v1 resource
 * pipeline documented in `docs/widgets/integrations/icon-font.md`; do not
 * expand this table or treat its names as Uhura's canonical icon system.
 */
export const PROVISIONAL_BROWSER_ICON_TABLE: IconTable = {
  home: icon(path(
    "M4 11 12 4l8 7v8a1 1 0 0 1-1 1h-4v-6h-6v6H5a1 1 0 0 1-1-1z",
    outline(1.8, { lineJoin: "round" }),
  )),
  search: icon(
    circle(10.5, 10.5, 6, outline(1.8)),
    path("m15.5 15.5 5 5", stroke(1.8, { lineCap: "round" })),
  ),
  plus: icon(
    rect(3.5, 3.5, 17, 17, 4, outline(1.8)),
    path("M12 8v8M8 12h8", stroke(1.8, { lineCap: "round" })),
  ),
  reels: icon(
    rect(3.5, 3.5, 17, 17, 4, outline(1.8)),
    path("M3.5 8.5h17M8.5 3.5l3 5M14 3.5l3 5", stroke(1.6)),
    path("m10.5 12.2 4.4 2.6-4.4 2.6z", filled()),
  ),
  profile: icon(
    circle(12, 8.6, 3.6, outline(1.8)),
    path("M4.8 20a7.4 7.4 0 0 1 14.4 0", outline(1.8, { lineCap: "round" })),
  ),
  heart: icon(path(
    "M12 20.3 5 13.6a4.6 4.6 0 0 1 6.5-6.5l.5.5.5-.5a4.6 4.6 0 0 1 6.5 6.5z",
    outline(1.8, { lineJoin: "round" }),
  )),
  "heart-filled": icon(path(
    "M12 20.3 5 13.6a4.6 4.6 0 0 1 6.5-6.5l.5.5.5-.5a4.6 4.6 0 0 1 6.5 6.5z",
    filled(),
  )),
  comment: icon(path(
    "M20 11.6A8 8 0 1 0 7 17.9L4.5 20l.6-3.2A8 8 0 0 0 20 11.6z",
    outline(1.8, { lineJoin: "round" }),
  )),
  close: icon(path(
    "m6 6 12 12M18 6 6 18",
    stroke(1.8, { lineCap: "round" }),
  )),
  back: icon(path(
    "M14.5 5 8 12l6.5 7",
    outline(1.8, { lineCap: "round", lineJoin: "round" }),
  )),
  grid: icon(path(
    "M4 4h16v16H4zM4 10.7h16M4 17.3h16M10.7 4v16M17.3 4v16",
    outline(1.5),
  )),
  layers: icon(
    path("m12 4 8 4.5-8 4.5-8-4.5z", outline(1.7, { lineJoin: "round" })),
    path("m5.2 12.8 6.8 3.8 6.8-3.8", outline(1.7, { lineJoin: "round" })),
    path("m5.2 16.3 6.8 3.8 6.8-3.8", outline(1.7, { lineJoin: "round" })),
  ),
  "video-off": icon(
    path(
      "M4 7.5A1.5 1.5 0 0 1 5.5 6h8A1.5 1.5 0 0 1 15 7.5v9a1.5 1.5 0 0 1-1.5 1.5h-8A1.5 1.5 0 0 1 4 16.5zM15 10.5l5-2.5v8l-5-2.5",
      outline(1.7, { lineJoin: "round" }),
    ),
    path("m3.5 3.5 17 17", stroke(1.7, { lineCap: "round" })),
  ),
  progress: icon(
    circle(12, 12, 7.5, outline(1.8, { opacity: 0.25 })),
    path("M12 4.5a7.5 7.5 0 0 1 7.5 7.5", outline(1.8, { lineCap: "round" })),
  ),
  bookmark: icon(path(
    "M6.5 4.5h11a1 1 0 0 1 1 1v15L12 16.2l-6.5 4.3v-15a1 1 0 0 1 1-1z",
    outline(1.8, { lineJoin: "round" }),
  )),
  "bookmark-filled": icon(path(
    "M6.5 4.5h11a1 1 0 0 1 1 1v15L12 16.2l-6.5 4.3v-15a1 1 0 0 1 1-1z",
    filled(),
  )),
  "chevron-left": icon(path(
    "m14.5 5-6.5 7 6.5 7",
    outline(1.8, { lineCap: "round", lineJoin: "round" }),
  )),
  "chevron-right": icon(path(
    "m9.5 5 6.5 7-6.5 7",
    outline(1.8, { lineCap: "round", lineJoin: "round" }),
  )),
};

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

/** Replaces an icon host's contents from renderer-owned glyph data. */
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
