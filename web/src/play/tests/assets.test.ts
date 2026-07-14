import assert from "node:assert/strict";
import { test } from "vitest";

import { createAssets } from "../assets.js";

class FakeElement {
  readonly style = { backgroundImage: "" };
  readonly attributes = new Map<string, string>();
  loadCalls = 0;

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

  load(): void {
    this.loadCalls += 1;
  }
}

const asElement = (element: FakeElement): HTMLElement =>
  element as unknown as HTMLElement;
const asVideo = (element: FakeElement): HTMLVideoElement =>
  element as unknown as HTMLVideoElement;

async function flushPromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

test("fixture assets select an encoded JPEG or MP4 rendition by semantic slot", () => {
  const assets = createAssets();
  const image = new FakeElement();
  const video = new FakeElement();

  assets.applyImage(asElement(image), "still one");
  assets.applyVideoPoster(asVideo(video), "poster/one");
  assets.applyVideoSource(asVideo(video), "clip/one");

  assert.equal(image.style.backgroundImage, 'url("/assets/still%20one.jpg")');
  assert.equal(video.getAttribute("poster"), "/assets/poster%2Fone.jpg");
  assert.equal(video.getAttribute("src"), "/assets/clip%2Fone.mp4");

  assets.applyVideoSource(asVideo(video), "clip/two");
  assert.equal(video.loadCalls, 1, "replacing a loaded src resets old playback");
  assert.equal(video.getAttribute("src"), "/assets/clip%2Ftwo.mp4");

  assets.applyVideoSource(asVideo(video), undefined);
  assets.applyVideoPoster(asVideo(video), undefined);
  assets.applyImage(asElement(image), undefined);
  assert.equal(video.loadCalls, 2);
  assert.equal(video.hasAttribute("src"), false);
  assert.equal(video.hasAttribute("poster"), false);
  assert.equal(image.style.backgroundImage, "");
});

test("signed video source and poster resolutions are independent and stale-safe", async () => {
  const pending = new Map<string, (url: string) => void>();
  const requested: string[] = [];
  const assets = createAssets(
    (ref) =>
      new Promise((resolve) => {
        requested.push(ref);
        pending.set(ref, resolve);
      }),
  );
  const video = new FakeElement();

  assets.applyVideoSource(asVideo(video), "clip-old");
  assets.applyVideoSource(asVideo(video), "clip-new");
  assets.applyVideoPoster(asVideo(video), "poster-new");
  await flushPromises();

  pending.get("clip-new")?.("https://media.example/new.mp4?token=a&part=1");
  pending.get("poster-new")?.("https://media.example/new.jpg?token=b&part=2");
  await flushPromises();
  assert.equal(video.getAttribute("src"), "https://media.example/new.mp4?token=a&part=1");
  assert.equal(video.getAttribute("poster"), "https://media.example/new.jpg?token=b&part=2");

  pending.get("clip-old")?.("https://media.example/stale.mp4");
  await flushPromises();
  assert.equal(video.getAttribute("src"), "https://media.example/new.mp4?token=a&part=1");

  assets.applyVideoSource(asVideo(video), "clip-new");
  await flushPromises();
  assert.deepEqual(requested, ["clip-old", "clip-new", "poster-new"]);
});
