// Shared semantic props -> DOM/ARIA mapping. Policy branches are restricted
// to effects: platform structure and safe property application stay common.

import type { VValue } from "../protocol/types.js";
import type { AssetAppliers } from "./assets.js";
import type {
  RenderPolicy,
  RendererNode,
  TextFieldHolder,
} from "./contracts.js";
import type { IconFontRegistry } from "./icons.js";

interface ApplyPropsContext {
  document: Document;
  icons: IconFontRegistry;
  assets: AssetAppliers;
  policy: RenderPolicy;
  holderOf(el: HTMLElement): TextFieldHolder;
}

type SemanticTag = "p" | "span" | "button" | "img" | "video" | "div";

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

function setAttr(el: Element, name: string, value: string | undefined): void {
  if (value === undefined) el.removeAttribute(name);
  else if (el.getAttribute(name) !== value) el.setAttribute(name, value);
}

function setBooleanAttr(el: Element, name: string, enabled: boolean): void {
  if (enabled) {
    if (!el.hasAttribute(name)) el.setAttribute(name, "");
  } else {
    el.removeAttribute(name);
  }
}

/** The native DOM tag used by a catalog element. */
export function tagFor(element: string): SemanticTag {
  switch (element) {
    case "text":
      return "p";
    case "icon":
      return "span";
    case "button":
      return "button";
    case "img":
      return "img";
    case "video":
      return "video";
    default:
      return "div";
  }
}

/**
 * Applies one node's semantic props. Structure-owning elements manage their
 * mechanic children here; the engine owns semantic keyed children.
 */
export function applyProps(
  el: HTMLElement,
  node: RendererNode,
  ctx: ApplyPropsContext,
): void {
  const props = node.props;
  switch (node.element) {
    case "view": {
      const role = textOf(props["role"]);
      setAttr(
        el,
        "role",
        role === "list" || role === "navigation" || role === "tablist"
          ? role
          : undefined,
      );
      break;
    }

    case "scroll":
      setAttr(el, "data-direction", textOf(props["direction"]) ?? "vertical");
      break;

    case "pager": {
      setAttr(el, "role", "group");
      setAttr(el, "aria-label", textOf(props["label"]));
      const track = ensureTrack(el, ctx.document);
      if (textOf(props["indicator"]) === "dots") {
        const dots = ensureDots(el, ctx.document);
        const count = node.children?.length ?? 0;
        while (dots.children.length > count) dots.lastElementChild?.remove();
        while (dots.children.length < count) {
          const dot = ctx.document.createElement("span");
          dot.className = "uh-dot";
          dots.append(dot);
        }
        updateDots(el, track);
      } else {
        el.querySelector(":scope > .uh-dots")?.remove();
      }
      break;
    }

    case "text": {
      const content = textOf(props["content"]) ?? "";
      if (el.textContent !== content) el.textContent = content;
      break;
    }

    case "img": {
      const img = el as HTMLImageElement;
      setAttr(img, "role", undefined);
      setAttr(img, "aria-label", undefined);
      setAttr(img, "aria-hidden", undefined);
      if (boolOf(props["decorative"])) {
        setAttr(img, "alt", "");
      } else {
        setAttr(img, "alt", textOf(props["alt"]) ?? "");
      }
      ctx.assets.applyImage(img, assetOf(props["src"]));
      break;
    }

    case "video": {
      const video = el as HTMLVideoElement;
      const isPlay = ctx.policy.kind === "play";
      const autoplay = isPlay && boolOf(props["autoplay"]);
      const muted = isPlay && boolOf(props["muted"]);
      const loop = isPlay && boolOf(props["loop"]);
      const controls = isPlay && boolOf(props["controls"]);
      const playsInline = isPlay && boolOf(props["playsinline"]);

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
      ctx.assets.applyVideoSource(video, isPlay ? assetOf(props["src"]) : undefined);
      if (isPlay) setAttr(video, "data-video-preview", undefined);
      else setAttr(video, "data-video-preview", "poster");
      break;
    }

    case "icon": {
      const name = textOf(props["name"]) ?? "";
      const requestedFamily = textOf(props["family"]);
      const family = requestedFamily ?? ctx.icons.defaultFamily;
      setAttr(el, "aria-hidden", "true");
      if (
        el.getAttribute("data-icon") !== name ||
        el.getAttribute("data-icon-family") !== family ||
        el.getAttribute("data-icon-resource") !== ctx.icons.fingerprint
      ) {
        ctx.icons.apply(el, requestedFamily, name);
        setAttr(el, "data-icon", name);
        setAttr(el, "data-icon-family", family);
        setAttr(el, "data-icon-resource", ctx.icons.fingerprint);
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

    case "textfield": {
      const existing = el.querySelector(":scope > input");
      let input: HTMLInputElement;
      if (isInput(existing)) {
        input = existing;
      } else {
        input = ctx.document.createElement("input");
        input.type = "text";
        input.setAttribute("data-uh-mechanic", "input");
        el.append(input);
        if (ctx.policy.kind === "play") {
          ctx.policy.textFields.wire(input, ctx.holderOf(el));
        }
      }
      setAttr(input, "placeholder", textOf(props["placeholder"]));
      setAttr(input, "aria-label", textOf(props["label"]));
      input.disabled = boolOf(props["disabled"]);
      const value = textOf(props["value"]) ?? "";
      if (ctx.policy.kind === "play") {
        input.readOnly = false;
        ctx.policy.textFields.applyValue(input, value);
      } else {
        input.readOnly = true;
        if (input.value !== value) input.value = value;
      }
      break;
    }

    case "region":
      setAttr(el, "role", "button");
      setAttr(el, "tabindex", ctx.policy.kind === "play" ? "0" : undefined);
      setAttr(el, "aria-label", textOf(props["label"]));
      break;

    default:
      setAttr(el, "data-unsupported", node.element);
  }
}

function isInput(value: Element | null): value is HTMLInputElement {
  return value !== null && value.tagName.toLowerCase() === "input";
}

function isElement(value: Element | null): value is HTMLElement {
  return value !== null && typeof (value as HTMLElement).setAttribute === "function";
}

function ensureTrack(el: HTMLElement, document: Document): HTMLElement {
  const found = el.querySelector(":scope > .uh-track");
  if (isElement(found)) return found;
  const track = document.createElement("div");
  track.className = "uh-track";
  track.setAttribute("data-uh-mechanic", "track");
  el.append(track);
  track.addEventListener("scroll", () => updateDots(el, track), { passive: true });
  return track;
}

function ensureDots(el: HTMLElement, document: Document): HTMLElement {
  const found = el.querySelector(":scope > .uh-dots");
  if (isElement(found)) return found;
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
  [...dots.children].forEach((dot, index) => dot.classList.toggle("on", index === active));
}
