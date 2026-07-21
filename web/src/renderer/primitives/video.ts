import {
  booleanAttribute,
  physicalAttribute,
  presentBooleanAttribute,
  semanticAttributes,
  textAttribute,
} from "./common.js";
import type { PrimitiveAdapter } from "./types.js";

export const videoAdapter: PrimitiveAdapter = {
  id: "video",
  tag: "video",
  attributes(node, mode) {
    const play = mode === "play";
    return semanticAttributes(node, [
      physicalAttribute(
        "aria-label",
        textAttribute(node.attributes, "label"),
      ),
      presentBooleanAttribute(
        "autoplay",
        play && booleanAttribute(node.attributes, "autoplay") === true,
      ),
      presentBooleanAttribute(
        "muted",
        play && booleanAttribute(node.attributes, "muted") === true,
      ),
      presentBooleanAttribute(
        "loop",
        play && booleanAttribute(node.attributes, "loop") === true,
      ),
      presentBooleanAttribute(
        "controls",
        play && booleanAttribute(node.attributes, "controls") === true,
      ),
      presentBooleanAttribute(
        "playsinline",
        play && booleanAttribute(node.attributes, "playsinline") === true,
      ),
      mode === "editor"
        ? { name: "data-video-preview", value: "poster" }
        : null,
    ]);
  },
  hosts: (element) => ({ children: null, events: element }),
  sync(element, node, _hosts, context) {
    const video = element as HTMLVideoElement;
    const play = context.mode === "play";
    const autoplay =
      play && booleanAttribute(node.attributes, "autoplay") === true;
    const muted = play && booleanAttribute(node.attributes, "muted") === true;
    const loop = play && booleanAttribute(node.attributes, "loop") === true;
    const controls =
      play && booleanAttribute(node.attributes, "controls") === true;
    const playsInline =
      play && booleanAttribute(node.attributes, "playsinline") === true;
    if (video.autoplay !== autoplay) video.autoplay = autoplay;
    if (video.muted !== muted) video.muted = muted;
    if (video.loop !== loop) video.loop = loop;
    if (video.controls !== controls) video.controls = controls;
    if (video.playsInline !== playsInline) video.playsInline = playsInline;

    if (context.options.assets) {
      context.options.assets.applyVideoSource(
        video,
        play ? textAttribute(node.attributes, "src") : undefined,
      );
      context.options.assets.applyVideoPoster(
        video,
        textAttribute(node.attributes, "poster"),
      );
    } else {
      video.removeAttribute("src");
      video.removeAttribute("poster");
    }
  },
};
