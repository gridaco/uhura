import assert from "node:assert/strict";
import { test } from "vitest";

import { createPlayAssets } from "../../renderer/assets.js";

class FakeElement {
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

const asImage = (element: FakeElement): HTMLImageElement =>
  element as unknown as HTMLImageElement;
const asVideo = (element: FakeElement): HTMLVideoElement =>
  element as unknown as HTMLVideoElement;

async function flushPromises(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

test("local assets select an encoded JPEG or MP4 rendition by semantic slot", () => {
  const assets = createPlayAssets();
  const image = new FakeElement();
  const video = new FakeElement();

  assets.applyImage(asImage(image), "still one");
  assets.applyVideoPoster(asVideo(video), "poster/one");
  assets.applyVideoSource(asVideo(video), "clip/one");

  assert.equal(image.getAttribute("src"), "/api/play/assets/still%20one.jpg");
  assert.equal(video.getAttribute("poster"), "/api/play/assets/poster%2Fone.jpg");
  assert.equal(video.getAttribute("src"), "/api/play/assets/clip%2Fone.mp4");

  assets.applyVideoSource(asVideo(video), "clip/two");
  assert.equal(video.loadCalls, 1, "replacing a loaded src resets old playback");
  assert.equal(video.getAttribute("src"), "/api/play/assets/clip%2Ftwo.mp4");

  assets.applyVideoSource(asVideo(video), undefined);
  assets.applyVideoPoster(asVideo(video), undefined);
  assets.applyImage(asImage(image), undefined);
  assert.equal(video.loadCalls, 2);
  assert.equal(video.hasAttribute("src"), false);
  assert.equal(video.hasAttribute("poster"), false);
  assert.equal(image.hasAttribute("src"), false);
});

test("signed image resolution clears eagerly and ignores stale results", async () => {
  const pending = new Map<string, (url: string) => void>();
  const requested: string[] = [];
  const assets = createPlayAssets(
    (ref) =>
      new Promise((resolve) => {
        requested.push(ref);
        pending.set(ref, resolve);
      }),
  );
  const image = new FakeElement();

  assets.applyImage(asImage(image), "old");
  assets.applyImage(asImage(image), "new");
  assert.equal(image.hasAttribute("src"), false);
  await flushPromises();

  pending.get("new")?.("https://media.example/new.jpg?token=a&part=1");
  await flushPromises();
  assert.equal(image.getAttribute("src"), "https://media.example/new.jpg?token=a&part=1");

  pending.get("old")?.("https://media.example/stale.jpg");
  await flushPromises();
  assert.equal(image.getAttribute("src"), "https://media.example/new.jpg?token=a&part=1");
  assert.deepEqual(requested, ["old", "new"]);
});

test("signed video source and poster resolutions are independent and stale-safe", async () => {
  const pending = new Map<string, (url: string) => void>();
  const requested: string[] = [];
  const assets = createPlayAssets(
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
