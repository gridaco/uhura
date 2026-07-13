import assert from "node:assert/strict";
import test from "node:test";

import { createAssets } from "../assets.js";

class FakeElement {
  constructor() {
    this.style = { backgroundImage: "" };
    this.attributes = new Map();
    this.loadCalls = 0;
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

  load() {
    this.loadCalls += 1;
  }
}

async function flushPromises() {
  await Promise.resolve();
  await Promise.resolve();
}

test("fixture assets select an encoded JPEG or MP4 rendition by semantic slot", () => {
  const assets = createAssets();
  const image = new FakeElement();
  const video = new FakeElement();

  assets.applyImage(image, "still one");
  assets.applyVideoPoster(video, "poster/one");
  assets.applyVideoSource(video, "clip/one");

  assert.equal(image.style.backgroundImage, 'url("/assets/still%20one.jpg")');
  assert.equal(video.getAttribute("poster"), "/assets/poster%2Fone.jpg");
  assert.equal(video.getAttribute("src"), "/assets/clip%2Fone.mp4");

  assets.applyVideoSource(video, "clip/two");
  assert.equal(video.loadCalls, 1, "replacing a loaded src resets old playback");
  assert.equal(video.getAttribute("src"), "/assets/clip%2Ftwo.mp4");

  assets.applyVideoSource(video, undefined);
  assets.applyVideoPoster(video, undefined);
  assets.applyImage(image, undefined);
  assert.equal(video.loadCalls, 2);
  assert.equal(video.hasAttribute("src"), false);
  assert.equal(video.hasAttribute("poster"), false);
  assert.equal(image.style.backgroundImage, "");
});

test("signed video source and poster resolutions are independent and stale-safe", async () => {
  const pending = new Map();
  const requested = [];
  const assets = createAssets(
    (ref) =>
      new Promise((resolve) => {
        requested.push(ref);
        pending.set(ref, resolve);
      }),
  );
  const video = new FakeElement();

  assets.applyVideoSource(video, "clip-old");
  assets.applyVideoSource(video, "clip-new");
  assets.applyVideoPoster(video, "poster-new");
  await flushPromises();

  pending.get("clip-new")("https://media.example/new.mp4?token=a&part=1");
  pending.get("poster-new")("https://media.example/new.jpg?token=b&part=2");
  await flushPromises();
  assert.equal(video.getAttribute("src"), "https://media.example/new.mp4?token=a&part=1");
  assert.equal(video.getAttribute("poster"), "https://media.example/new.jpg?token=b&part=2");

  pending.get("clip-old")("https://media.example/stale.mp4");
  await flushPromises();
  assert.equal(video.getAttribute("src"), "https://media.example/new.mp4?token=a&part=1");

  assets.applyVideoSource(video, "clip-new");
  await flushPromises();
  assert.deepEqual(requested, ["clip-old", "clip-new", "poster-new"]);
});
