import assert from "node:assert/strict";
import test from "node:test";

import { applyProps, tagFor } from "../appliers.js";

class FakeVideo {
  constructor() {
    this.attributes = new Map();
    this.autoplay = false;
    this.muted = false;
    this.loop = false;
    this.controls = false;
    this.playsInline = false;
  }

  getAttribute(name) {
    return this.attributes.get(name) ?? null;
  }

  hasAttribute(name) {
    return this.attributes.has(name);
  }

  setAttribute(name, value) {
    this.attributes.set(name, String(value));
  }

  removeAttribute(name) {
    this.attributes.delete(name);
  }
}

function node(props) {
  return {
    key: "demo-video",
    element: "video",
    props,
  };
}

test("video uses a native tag and applies assets, label, and playback flags", () => {
  const calls = [];
  const assets = {
    applyVideoPoster(el, ref) {
      calls.push(["poster", el, ref]);
    },
    applyVideoSource(el, ref) {
      calls.push(["source", el, ref]);
    },
  };
  const video = new FakeVideo();

  applyProps(
    video,
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
    { assets },
  );

  assert.equal(tagFor("video"), "video");
  assert.deepEqual(
    calls.map(([slot, , ref]) => [slot, ref]),
    [
      ["poster", "poster"],
      ["source", "clip"],
    ],
  );
  assert.equal(calls[0][1], video);
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
  const calls = [];
  const assets = {
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

  applyProps(video, node({}), { assets });

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
