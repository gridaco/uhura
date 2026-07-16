import assert from "node:assert/strict";
import { test } from "vitest";

import type { VNode, VValue } from "../../protocol/types.js";
import {
  applyProps,
  tagFor,
} from "../../renderer/appliers.js";
import type { AssetAppliers } from "../../renderer/assets.js";
import type { IconFontRegistry } from "../../renderer/icons.js";

const UNUSED_ICONS: IconFontRegistry = {
  defaultFamily: "lucide",
  fingerprint: "unused",
  apply: () => {
    throw new Error("video test unexpectedly applied an icon");
  },
};

class FakeVideo {
  readonly attributes = new Map<string, string>();
  autoplay = false;
  muted = false;
  loop = false;
  controls = false;
  playsInline = false;

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  hasAttribute(name: string): boolean {
    return this.attributes.has(name);
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, String(value));
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }
}

function node(props: Record<string, VValue>): VNode {
  return {
    key: "demo-video",
    element: "video",
    props,
  };
}

const asVideo = (video: FakeVideo): HTMLVideoElement =>
  video as unknown as HTMLVideoElement;

function context(assets: AssetAppliers): Parameters<typeof applyProps>[2] {
  return {
    document: {} as Document,
    icons: UNUSED_ICONS,
    assets,
    policy: {
      kind: "play",
      emit: () => {},
      textFields: {
        wire: () => {},
        applyValue: () => {},
      },
      scrolls: {
        sync: () => {},
        disposeSubtree: () => {},
        savePositions: () => {},
        restorePositions: () => {},
      },
      disposeSubtree: () => {},
    },
    holderOf: () => ({ path: "", on: {} }),
  };
}

test("video uses a native tag and applies assets, label, and playback flags", () => {
  const calls: ["poster" | "source", HTMLVideoElement, string | undefined][] = [];
  const assets: AssetAppliers = {
    applyImage: () => {},
    applyVideoPoster(el, ref) {
      calls.push(["poster", el, ref]);
    },
    applyVideoSource(el, ref) {
      calls.push(["source", el, ref]);
    },
  };
  const video = new FakeVideo();

  applyProps(
    asVideo(video),
    node({
      src: { t: "image", asset: "clip" },
      poster: { t: "image", asset: "poster" },
      label: { t: "plain", v: "Aurora above the fjord" },
      autoplay: true,
      muted: true,
      loop: true,
      controls: true,
      playsinline: true,
    }),
    context(assets),
  );

  assert.equal(tagFor("video"), "video");
  assert.deepEqual(
    calls.map(([slot, , ref]) => [slot, ref]),
    [
      ["poster", "poster"],
      ["source", "clip"],
    ],
  );
  assert.equal(calls[0]?.[1], video);
  assert.equal(video.getAttribute("aria-label"), "Aurora above the fjord");
  for (const name of ["autoplay", "muted", "loop", "controls", "playsinline"]) {
    assert.equal(video.hasAttribute(name), true, `${name} attribute`);
  }
  assert.equal(video.autoplay, true);
  assert.equal(video.muted, true);
  assert.equal(video.loop, true);
  assert.equal(video.controls, true);
  assert.equal(video.playsInline, true);
});

test("absent video flags are false and remove prior DOM state", () => {
  const calls: ["poster" | "source", string | undefined][] = [];
  const assets: AssetAppliers = {
    applyImage: () => {},
    applyVideoPoster(_el, ref) {
      calls.push(["poster", ref]);
    },
    applyVideoSource(_el, ref) {
      calls.push(["source", ref]);
    },
  };
  const video = new FakeVideo();
  for (const name of ["autoplay", "muted", "loop", "controls", "playsinline"]) {
    video.setAttribute(name, "");
  }
  video.setAttribute("aria-label", "old label");
  video.autoplay = video.muted = video.loop = video.controls = video.playsInline = true;

  applyProps(asVideo(video), node({}), context(assets));

  for (const name of ["autoplay", "muted", "loop", "controls", "playsinline"]) {
    assert.equal(video.hasAttribute(name), false, `${name} attribute`);
  }
  assert.equal(video.autoplay, false);
  assert.equal(video.muted, false);
  assert.equal(video.loop, false);
  assert.equal(video.controls, false);
  assert.equal(video.playsInline, false);
  assert.equal(video.hasAttribute("aria-label"), false);
  assert.deepEqual(calls, [
    ["poster", undefined],
    ["source", undefined],
  ]);
});
