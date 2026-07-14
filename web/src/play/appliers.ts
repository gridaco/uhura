// Semantic props → DOM/ARIA, one applier per catalog element (§8.2).
// Mirrors the static renderer in crates/uhura-project (same `uh-<element>`
// bases, same ARIA mapping) so both renderers stay two consumers of one
// protocol. Human text lands via TEXT NODES / attribute strings only —
// never markup.

import type {
  Descriptor,
  VNode,
  VValue,
} from "../protocol/types.js";
import type { TextFieldController } from "./textfield.js";

export interface AssetAppliers {
  apply(el: HTMLElement, assetRef: string | undefined): void;
  applyVideoSource(el: HTMLVideoElement, assetRef: string | undefined): void;
  applyVideoPoster(el: HTMLVideoElement, assetRef: string | undefined): void;
}

interface PropsHolder {
  path: string;
  on: Record<string, Descriptor>;
}

interface ApplyPropsContext {
  glyphs: Record<string, string>;
  assets: AssetAppliers;
  textFields: TextFieldController;
  holderOf(el: HTMLElement): PropsHolder;
}

type SemanticTag = "p" | "span" | "button" | "video" | "div";

export function textOf(v: VValue | undefined): string | undefined {
  if (typeof v === "string") return v;
  if (typeof v === "object" && v !== null && v.t === "plain") return v.v;
  return undefined;
}

function boolOf(v: VValue | undefined): boolean {
  return v === true;
}

function assetOf(v: VValue | undefined): string | undefined {
  if (typeof v === "object" && v !== null && v.t === "image") return v.asset;
  return undefined;
}

/**
 * Set when defined, remove when not.
 */
function setAttr(
  el: HTMLElement,
  name: string,
  value: string | undefined,
): void {
  if (value === undefined) el.removeAttribute(name);
  else if (el.getAttribute(name) !== value) el.setAttribute(name, value);
}

function setBooleanAttr(el: HTMLElement, name: string, enabled: boolean): void {
  if (enabled) {
    if (!el.hasAttribute(name)) el.setAttribute(name, "");
  } else {
    el.removeAttribute(name);
  }
}

/**
 * The DOM tag an element renders as (everything else is a div).
 */
export function tagFor(element: string): SemanticTag {
  switch (element) {
    case "text":
      return "p";
    case "icon":
      return "span";
    case "button":
      return "button";
    case "video":
      return "video";
    default:
      return "div";
  }
}

/**
 * Applies one node's semantic props. Structure-owning elements
 * (text-field's input, pager's track/dots, icon's svg) manage their
 * mechanic children here; the reconciler owns keyed children.
 */
export function applyProps(
  el: HTMLElement,
  node: VNode,
  ctx: ApplyPropsContext,
): void {
  const props = node.props;
  switch (node.element) {
    case "view": {
      const role = textOf(props["role"]);
      setAttr(
        el,
        "role",
        role === "list" || role === "navigation" || role === "tablist" ? role : undefined,
      );
      break;
    }

    case "scroll":
      setAttr(el, "data-direction", textOf(props["direction"]) ?? "vertical");
      break;

    case "pager": {
      const label = textOf(props["label"]);
      setAttr(el, "role", "group");
      setAttr(el, "aria-label", label);
      // Keyed children live inside .uh-track (the reconciler asks for it
      // via childHost); dots are pure renderer mechanics.
      const track = ensureTrack(el);
      if (textOf(props["indicator"]) === "dots") {
        const dots = ensureDots(el);
        const count = node.children?.length ?? 0;
        while (dots.children.length > count) dots.lastElementChild?.remove();
        while (dots.children.length < count) {
          const dot = document.createElement("span");
          dot.className = "uh-dot";
          dots.append(dot);
        }
        updateDots(el, track);
      } else {
        el.querySelector(":scope > .uh-dots")?.remove();
      }
      break;
    }

    case "text":
      // Interpolations were evaluated by core into one plain `content`
      // prop; a text node is the only carrier (§8.2).
      if (el.textContent !== (textOf(props["content"]) ?? "")) {
        el.textContent = textOf(props["content"]) ?? "";
      }
      break;

    case "image": {
      const asset = assetOf(props["src"]);
      ctx.assets.apply(el, asset);
      if (boolOf(props["decorative"])) {
        setAttr(el, "aria-hidden", "true");
        setAttr(el, "role", undefined);
        setAttr(el, "aria-label", undefined);
      } else {
        setAttr(el, "role", "img");
        setAttr(el, "aria-label", textOf(props["alt"]));
        setAttr(el, "aria-hidden", undefined);
      }
      break;
    }

    case "video": {
      const video = el as HTMLVideoElement;
      const autoplay = boolOf(props["autoplay"]);
      const muted = boolOf(props["muted"]);
      const loop = boolOf(props["loop"]);
      const controls = boolOf(props["controls"]);
      const playsInline = boolOf(props["playsinline"]);

      // Apply policy flags before `src`: autoplay eligibility is evaluated as
      // media selection starts, and browsers require autoplaying video to be
      // muted in the common case. Set attributes for faithful DOM semantics
      // and properties for the current playback state (`muted` in particular
      // is not merely a reflected content attribute).
      setBooleanAttr(video, "autoplay", autoplay);
      setBooleanAttr(video, "muted", muted);
      setBooleanAttr(video, "loop", loop);
      setBooleanAttr(video, "controls", controls);
      setBooleanAttr(video, "playsinline", playsInline);
      video.autoplay = autoplay;
      video.muted = muted;
      video.loop = loop;
      video.controls = controls;
      video.playsInline = playsInline;
      setAttr(video, "aria-label", textOf(props["label"]));

      ctx.assets.applyVideoPoster(video, assetOf(props["poster"]));
      ctx.assets.applyVideoSource(video, assetOf(props["src"]));
      break;
    }

    case "icon": {
      const name = textOf(props["name"]) ?? "";
      setAttr(el, "aria-hidden", "true");
      if (el.getAttribute("data-icon") !== name) {
        setAttr(el, "data-icon", name);
        const glyph =
          ctx.glyphs[name] ??
          '<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.8"/>';
        // Trusted markup: the glyph table is the checked catalog's icon
        // set served by `uhura dev` — never author or wire data.
        el.innerHTML = `<svg viewBox="0 0 24 24" width="24" height="24">${glyph}</svg>`;
      }
      break;
    }

    case "button": {
      const button = el as HTMLButtonElement;
      button.disabled = boolOf(props["disabled"]);
      setAttr(el, "aria-busy", boolOf(props["busy"]) ? "true" : undefined);
      setAttr(
        el,
        "aria-pressed",
        typeof props["pressed"] === "boolean" ? String(props["pressed"]) : undefined,
      );
      setAttr(el, "aria-current", boolOf(props["current"]) ? "true" : undefined);
      setAttr(el, "aria-label", textOf(props["label"]));
      break;
    }

    case "text-field": {
      const existing = el.querySelector(":scope > input");
      let input: HTMLInputElement;
      if (existing instanceof HTMLInputElement) {
        input = existing;
      } else {
        input = document.createElement("input");
        input.type = "text";
        input.setAttribute("data-uh-mechanic", "input");
        el.append(input);
        ctx.textFields.wire(input, ctx.holderOf(el));
      }
      setAttr(input, "placeholder", textOf(props["placeholder"]));
      setAttr(input, "aria-label", textOf(props["label"]));
      input.disabled = boolOf(props["disabled"]);
      // Core's draft applies through the in-flight gate — never directly.
      ctx.textFields.applyValue(input, textOf(props["value"]) ?? "");
      break;
    }

    case "region":
      setAttr(el, "role", "button");
      setAttr(el, "tabindex", "0");
      setAttr(el, "aria-label", textOf(props["label"]));
      break;

    default:
      // Honest labeled placeholder for anything unsupported (§8.2).
      setAttr(el, "data-unsupported", node.element);
  }
}

function ensureTrack(el: HTMLElement): HTMLElement {
  const found = el.querySelector(":scope > .uh-track");
  if (found instanceof HTMLElement) return found;
  const track = document.createElement("div");
  track.className = "uh-track";
  track.setAttribute("data-uh-mechanic", "track");
  el.append(track);
  track.addEventListener("scroll", () => updateDots(el, track), { passive: true });
  return track;
}

function ensureDots(el: HTMLElement): HTMLElement {
  const found = el.querySelector(":scope > .uh-dots");
  if (found instanceof HTMLElement) return found;
  const dots = document.createElement("div");
  dots.className = "uh-dots";
  dots.setAttribute("data-uh-mechanic", "dots");
  dots.setAttribute("aria-hidden", "true");
  el.append(dots);
  return dots;
}

function updateDots(pager: HTMLElement, track: HTMLElement): void {
  const dots = pager.querySelector(":scope > .uh-dots");
  if (!dots) return;
  const width = track.clientWidth || 1;
  const active = Math.min(
    dots.children.length - 1,
    Math.max(0, Math.round(track.scrollLeft / width)),
  );
  [...dots.children].forEach((dot, i) => dot.classList.toggle("on", i === active));
}
