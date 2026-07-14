import assert from "node:assert/strict";

import { test, vi } from "vitest";

import {
  createDriver,
  type ProviderHost,
  type SpockDriver,
} from "./spock.js";

interface Decoded {
  [key: string]: unknown;
  kind?: string;
  port?: string;
  projection?: string;
  key?: unknown;
  value?: Decoded;
  outcome?: unknown;
  author: Decoded;
  user: Decoded;
  updates: Decoded[];
  posts: Decoded[];
  stories: Decoded[];
  people: Decoded[];
  saved: Decoded[];
  reels: Decoded[];
  progress: Decoded[];
}

interface RpcCall {
  url: string;
  init: RequestInit;
}

interface PublishedPayload {
  image: string;
  caption: string;
  alt: string;
}
type TestFetch = (
  input: RequestInfo | URL,
  init: RequestInit,
) => Promise<Response>;

const MIRA = "user-mira";
const LENA = "user-lena";
const THEO = "user-theo";

const USERS = [
  {
    id: LENA,
    username: "lena.holt",
    display_name: "Lena Holt",
    avatar: { id: "avatar-lena" },
    avatar_alt: "Lena Holt",
    bio: "Clay and slow mornings",
  },
  {
    id: MIRA,
    username: "mira.santos",
    display_name: "Mira Santos",
    avatar: { id: "avatar-mira" },
    avatar_alt: "Mira Santos",
    bio: "Designer",
  },
  {
    id: THEO,
    username: "theo.okafor",
    display_name: "Theo Okafor",
    avatar: { id: "avatar-theo" },
    avatar_alt: "Theo Okafor",
    bio: "Courts and murals",
  },
];

const BASE_SNAPSHOT = {
  users: USERS,
  stories: [
    {
      id: "story-mira-1",
      author: { id: MIRA },
      position: 1,
      media_file: { id: "story-media-mira" },
      media_alt: "Breakfast on a marble counter",
      caption: "Breakfast",
      published_at: "2026-07-13T15:30:00Z",
    },
    {
      id: "story-lena-1",
      author: { id: LENA },
      position: 1,
      media_file: { id: "story-media-lena-1" },
      media_alt: "Clay on a wheel",
      caption: "Centering",
      published_at: "2026-07-13T15:00:00Z",
    },
    {
      id: "story-lena-2",
      author: { id: LENA },
      position: 2,
      media_file: { id: "story-media-lena-2" },
      media_alt: "A tall clay cylinder",
      caption: "One pull",
      published_at: "2026-07-13T15:10:00Z",
    },
    {
      id: "story-lena-3",
      author: { id: LENA },
      position: 3,
      media_file: { id: "story-media-lena-3" },
      media_alt: "A clean ceramics bench",
      caption: "Reset",
      published_at: "2026-07-13T15:20:00Z",
    },
    {
      id: "story-theo-1",
      author: { id: THEO },
      position: 1,
      media_file: { id: "story-media-theo" },
      media_alt: "A newly painted court",
      caption: "Finished",
      published_at: "2026-07-13T15:25:00Z",
    },
  ],
  storyViews: [
    { viewer: { id: MIRA }, story: { id: "story-lena-1" }, at: "2026-07-13T16:00:00Z" },
  ],
  posts: [
    {
      id: "post-theo-image",
      author: { id: THEO },
      caption: "Court mural in cobalt and orange",
      published_at: "2026-07-13T14:00:00Z",
      show_in_feed: true,
      media_kind: "image",
      media_file: { id: "media-theo" },
      video_file: null,
      media_alt: "A geometric basketball court mural",
    },
    {
      id: "post-lena-video",
      author: { id: LENA },
      caption: "Kiln notes from Lena",
      published_at: "2026-07-13T13:00:00Z",
      show_in_feed: true,
      media_kind: "video",
      media_file: { id: "poster-lena" },
      video_file: { id: "video-lena" },
      media_alt: "Copper glaze moving through kiln light",
    },
    {
      id: "post-mira-image",
      author: { id: MIRA },
      caption: "First tram",
      published_at: "2026-07-13T12:00:00Z",
      show_in_feed: true,
      media_kind: "image",
      media_file: { id: "media-mira" },
      video_file: null,
      media_alt: "Tram rails at sunrise",
    },
    {
      id: "post-lena-archive",
      author: { id: LENA },
      caption: "Shelf of celadon tests",
      published_at: "2026-07-10T12:00:00Z",
      show_in_feed: false,
      media_kind: "image",
      media_file: { id: "media-lena-archive" },
      video_file: null,
      media_alt: "Celadon test cups on a shelf",
    },
    {
      id: "post-mira-video",
      author: { id: MIRA },
      caption: "Last ferry home",
      published_at: "2026-07-09T12:00:00Z",
      show_in_feed: false,
      media_kind: "video",
      media_file: { id: "poster-mira" },
      video_file: { id: "video-mira" },
      media_alt: "Ferry wake at golden hour",
    },
  ],
  slides: [],
  comments: [
    {
      id: "comment-1",
      post: { id: "post-lena-video" },
      author: { id: MIRA },
      body: "That light is perfect.",
      created_at: "2026-07-13T13:30:00Z",
    },
  ],
  likes: [
    { user: { id: MIRA }, post: { id: "post-lena-video" }, at: "2026-07-13T13:20:00Z" },
    { user: { id: THEO }, post: { id: "post-lena-video" }, at: "2026-07-13T13:21:00Z" },
  ],
  saves: [
    { user: { id: MIRA }, post: { id: "post-theo-image" }, at: "2026-07-13T14:10:00Z" },
    { user: { id: LENA }, post: { id: "post-mira-image" }, at: "2026-07-13T14:11:00Z" },
  ],
  follows: [
    { follower: { id: MIRA }, followed: { id: LENA }, at: "2026-07-01T00:00:00Z" },
    { follower: { id: THEO }, followed: { id: MIRA }, at: "2026-07-02T00:00:00Z" },
  ],
  postTags: [
    { post: { id: "post-theo-image" }, person: { id: LENA } },
  ],
};

function snapshot(): typeof BASE_SNAPSHOT {
  return structuredClone(BASE_SNAPSHOT);
}

function driver(
  actor = "mira.santos",
  host: ProviderHost = {
    signal: new AbortController().signal,
    pickFile: async () => null,
  },
): SpockDriver {
  return createDriver(
    {
      graphql_url: "http://spock.test/graphql/v1",
      rpc_url: "http://spock.test/rest/v1/rpc",
      storage_url: "http://spock.test/storage/v1",
      actor,
    },
    host,
  );
}

function graphql(data: unknown): Response {
  return new Response(JSON.stringify({ data }));
}

function frameworkEnvironment(
  authority: Record<string, unknown> = {
    graphql_path: "/framework/graphql",
    rpc_path: "/framework/rpc",
    storage_path: "/framework/storage",
  },
): Record<string, unknown> {
  return {
    protocol: "spock-host-environment/1",
    mode: "dev",
    project_generation_id: 7,
    backend_generation_id: 3,
    authority,
  };
}

function whoami(init: RequestInit): Response {
  const headers = new Headers(init?.headers);
  const actor = headers.get("x-spock-actor");
  return new Response(
    JSON.stringify({ actor, known: true, anonymous: actor === null }),
  );
}

function requestBody(init: RequestInit): string {
  if (typeof init.body !== "string") {
    throw new Error("expected a JSON request body");
  }
  return init.body;
}

async function withFetch<T>(
  fetcher: TestFetch,
  run: () => Promise<T>,
): Promise<T> {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = (input, init) => fetcher(input, init ?? {});
  try {
    return await run();
  } finally {
    globalThis.fetch = originalFetch;
  }
}

function bootMessages(remote: SpockDriver): Decoded[] {
  return remote.tick().map((message) => JSON.parse(message) as Decoded);
}

function projection(
  messages: Decoded[],
  port: string,
  name: string,
  key: unknown = null,
): Decoded {
  const found = messages.find(
    (message) =>
      message.kind === "projection" &&
      message.port === port &&
      message.projection === name &&
      message.key === key,
  );
  assert.ok(found, `missing ${port}.${name}(${JSON.stringify(key)})`);
  return found.value as Decoded;
}

function update(
  outcome: Decoded,
  port: string,
  name: string,
  key: unknown = null,
): Decoded {
  const found = outcome.updates.find(
    (candidate) =>
      candidate.port === port &&
      candidate.projection === name &&
      candidate.key === key,
  );
  assert.ok(found, `missing update ${port}.${name}(${JSON.stringify(key)})`);
  return found.value as Decoded;
}

function command(
  port: string,
  name: string,
  payload: Record<string, unknown>,
  correlation = `${port}-${name}`,
): string {
  return JSON.stringify({
    kind: "command",
    port,
    command: name,
    correlation,
    payload,
  });
}

async function settle(remote: SpockDriver): Promise<Decoded[]> {
  const messages: Decoded[] = [];
  for (let attempt = 0; attempt < 100; attempt += 1) {
    messages.push(...remote.tick().map((message) => JSON.parse(message)));
    if (remote.idle()) return messages;
    await new Promise((resolve) => setImmediate(resolve));
  }
  throw new Error("provider command did not settle");
}

function onlyOutcome(messages: Decoded[]): Decoded {
  const outcomes = messages.filter((message) => message.kind === "outcome");
  assert.equal(outcomes.length, 1);
  const [outcome] = outcomes;
  assert.ok(outcome);
  assert.deepEqual(outcome.outcome, { ok: {} });
  return outcome;
}

test("prefers one strictly typed framework environment before authority work", async () => {
  const data = snapshot();
  const calls: string[] = [];
  await withFetch(async (input, init) => {
    const url = String(input);
    calls.push(url);
    if (url === "/~project/environment") {
      assert.equal(init.method, "GET");
      assert.equal(new Headers(init.headers).get("accept"), "application/json");
      return new Response(JSON.stringify(frameworkEnvironment()));
    }
    if (url === "/framework/graphql") return graphql(data);
    if (url === "/~whoami") return whoami(init);
    if (url === "/framework/storage/object/sign/media-theo") {
      return new Response(
        JSON.stringify({
          url: "/framework/storage/object/media-theo?exp=9999999999&sig=test",
        }),
      );
    }
    if (url === "/framework/rpc/unlike_post") {
      return new Response(JSON.stringify({ user: MIRA, post: "post-lena-video" }));
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const remote = driver();
    await remote.assembleBoot();
    bootMessages(remote);

    assert.equal(
      await remote.resolveAsset("media-theo"),
      "/framework/storage/object/media-theo?exp=9999999999&sig=test",
    );
    remote.deliver(
      command("feed", "unlike-post", { post: "post-lena-video" }),
    );
    onlyOutcome(await settle(remote));
    remote.dispose();
  });

  assert.equal(calls[0], "/~project/environment");
  assert.equal(
    calls.filter((url) => url === "/~project/environment").length,
    1,
  );
  assert.ok(calls.includes("/framework/graphql"));
  assert.ok(calls.includes("/framework/rpc/unlike_post"));
  assert.ok(calls.includes("/framework/storage/object/sign/media-theo"));
  assert.equal(calls.some((url) => url.startsWith("http://spock.test")), false);
});

test("falls back for unavailable or invalid framework metadata", async () => {
  const cases: Array<{ name: string; response: () => Response }> = [
    {
      name: "unavailable",
      response: () => new Response(null, { status: 404 }),
    },
    {
      name: "wrong protocol",
      response: () =>
        new Response(
          JSON.stringify({
            ...frameworkEnvironment(),
            protocol: "spock-host-environment/0",
          }),
        ),
    },
    {
      name: "extra top-level provider data",
      response: () =>
        new Response(
          JSON.stringify({ ...frameworkEnvironment(), provider: { actor: THEO } }),
        ),
    },
    {
      name: "absolute authority URL",
      response: () =>
        new Response(
          JSON.stringify(
            frameworkEnvironment({
              graphql_path: "https://other.test/graphql",
              rpc_path: "/framework/rpc",
              storage_path: "/framework/storage",
            }),
          ),
        ),
    },
    {
      name: "invalid generation",
      response: () =>
        new Response(
          JSON.stringify({
            ...frameworkEnvironment(),
            backend_generation_id: 0,
          }),
        ),
    },
  ];

  for (const candidate of cases) {
    const calls: string[] = [];
    const data = snapshot();
    await withFetch(async (input, init) => {
      const url = String(input);
      calls.push(url);
      if (url === "/~project/environment") return candidate.response();
      if (url === "http://spock.test/graphql/v1") return graphql(data);
      if (url === "http://spock.test/~whoami") return whoami(init);
      throw new Error(`unexpected fetch ${url}`);
    }, async () => {
      const remote = driver();
      await remote.assembleBoot();
      remote.dispose();
    });
    assert.deepEqual(
      calls.slice(0, 2),
      ["/~project/environment", "http://spock.test/graphql/v1"],
      candidate.name,
    );
  }
});

test("bounds framework discovery before using standalone fallback endpoints", async () => {
  const calls: string[] = [];
  const data = snapshot();
  vi.useFakeTimers();
  try {
    await withFetch(async (input, init) => {
      const url = String(input);
      calls.push(url);
      if (url === "/~project/environment") {
        return await new Promise<Response>((_resolve, reject) => {
          const signal = init.signal;
          const abort = (): void =>
            reject(
              new DOMException("environment discovery timed out", "AbortError"),
            );
          if (signal?.aborted) abort();
          else signal?.addEventListener("abort", abort, { once: true });
        });
      }
      if (url === "http://spock.test/graphql/v1") return graphql(data);
      if (url === "http://spock.test/~whoami") return whoami(init);
      throw new Error(`unexpected fetch ${url}`);
    }, async () => {
      const remote = driver();
      const boot = remote.assembleBoot();
      await vi.advanceTimersByTimeAsync(2_001);
      await boot;
      remote.dispose();
    });
  } finally {
    vi.useRealTimers();
  }

  assert.deepEqual(calls.slice(0, 2), [
    "/~project/environment",
    "http://spock.test/graphql/v1",
  ]);
});

test("treats nullable integrated GraphQL as capability absence, not fallback", async () => {
  const calls: string[] = [];
  await withFetch(async (input) => {
    const url = String(input);
    calls.push(url);
    if (url === "/~project/environment") {
      return new Response(
        JSON.stringify(
          frameworkEnvironment({
            graphql_path: null,
            rpc_path: "/framework/rpc",
            storage_path: "/framework/storage",
          }),
        ),
      );
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const remote = driver();
    await assert.rejects(
      remote.assembleBoot(),
      /integrated Spock host does not advertise a GraphQL capability/,
    );
    remote.dispose();
  });

  assert.deepEqual(calls, ["/~project/environment"]);
  assert.equal(calls.some((url) => url.startsWith("http://spock.test")), false);
});

test("disposing during environment discovery aborts without authority fallback", async () => {
  const controller = new AbortController();
  let markStarted!: () => void;
  const started = new Promise<void>((resolve) => {
    markStarted = resolve;
  });
  let authorityCalls = 0;

  await withFetch(async (input, init) => {
    if (String(input) !== "/~project/environment") {
      authorityCalls += 1;
      throw new Error(`unexpected authority fetch ${String(input)}`);
    }
    markStarted();
    return await new Promise<Response>((_resolve, reject) => {
      init.signal?.addEventListener(
        "abort",
        () => reject(new DOMException("disposed", "AbortError")),
        { once: true },
      );
    });
  }, async () => {
    const remote = driver("mira.santos", {
      signal: controller.signal,
      pickFile: async () => null,
    });
    const boot = remote.assembleBoot();
    await started;
    controller.abort();
    await assert.rejects(
      boot,
      (error: unknown) =>
        error instanceof DOMException && error.name === "AbortError",
    );
  });

  assert.equal(authorityCalls, 0);
});

test("normalizes a configured username and exposes authority-owned actors", async () => {
  const data = snapshot();
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) return whoami(init);
    return graphql(data);
  }, async () => {
    const remote = driver();
    await remote.assembleBoot();
    assert.deepEqual(remote.systemInfo(), {
      actor: MIRA,
      actors: [
        { id: LENA, username: "lena.holt", label: "Lena Holt" },
        { id: MIRA, username: "mira.santos", label: "Mira Santos" },
        { id: THEO, username: "theo.okafor", label: "Theo Okafor" },
      ],
    });
  });
});

test("retains the actor catalog when the configured actor is invalid", async () => {
  const data = snapshot();
  await withFetch(async () => graphql(data), async () => {
    const remote = driver("typo");
    await assert.rejects(remote.assembleBoot(), /actor `typo` is not a seeded user/);
    assert.deepEqual(remote.systemInfo(), {
      actor: "typo",
      actors: [
        { id: LENA, username: "lena.holt", label: "Lena Holt" },
        { id: MIRA, username: "mira.santos", label: "Mira Santos" },
        { id: THEO, username: "theo.okafor", label: "Theo Okafor" },
      ],
    });
  });
});

test("boot projects actor-filtered home, playable video, sequences, profiles, and explore", async () => {
  const data = snapshot();
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) return whoami(init);
    return graphql(data);
  }, async () => {
    const remote = driver();
    const boot = JSON.parse(await remote.assembleBoot());
    assert.equal(boot.updates[0].value.id, MIRA);
    const messages = bootMessages(remote);

    const home = projection(messages, "feed", "feed-page");
    assert.deepEqual(home.posts.map((post) => post.id), [
      "post-lena-video",
      "post-mira-image",
    ]);
    assert.deepEqual(home.stories.map((ring) => ring.id), [
      "story-mira-1",
      "story-lena-2",
    ]);
    const [selfStory] = home.stories;
    const [firstPost] = home.posts;
    assert.ok(selfStory);
    assert.ok(firstPost);
    assert.equal(selfStory["is-self"], true);
    assert.equal(selfStory["has-unseen"], false);
    assert.deepEqual(firstPost.media, {
      video: {
        src: "video-lena",
        poster: {
          src: "poster-lena",
          alt: "Copper glaze moving through kiln light",
        },
      },
    });
    assert.equal(firstPost["viewer-has-liked"], true);
    assert.equal(firstPost["viewer-has-saved"], false);

    const middle = projection(messages, "feed", "story-by-id", "story-lena-2");
    assert.equal(middle.previous, "story-lena-1");
    assert.equal(middle.next, "story-lena-3");
    assert.deepEqual(middle.progress, [
      { id: "story-lena-1", "is-current": false, "is-viewed": true },
      { id: "story-lena-2", "is-current": true, "is-viewed": false },
      { id: "story-lena-3", "is-current": false, "is-viewed": false },
    ]);

    const self = projection(messages, "profile", "profile", MIRA);
    assert.equal(self["is-self"], true);
    assert.equal(self["viewer-follows"], false);
    assert.deepEqual(self.reels.map((post) => post.id), ["post-mira-video"]);
    assert.deepEqual(self.saved.map((post) => post.id), ["post-theo-image"]);

    const lena = projection(messages, "profile", "profile", LENA);
    assert.equal(lena["is-self"], false);
    assert.equal(lena["viewer-follows"], true);
    assert.deepEqual(lena.reels.map((post) => post.id), ["post-lena-video"]);
    assert.deepEqual(lena.saved, []);

    const explore = projection(messages, "profile", "search-results");
    assert.deepEqual(explore.people.map((person) => person.user.id), [LENA, THEO]);
    assert.deepEqual(explore.posts.map((post) => post.id), [
      "post-theo-image",
      "post-lena-video",
      "post-mira-image",
      "post-lena-archive",
      "post-mira-video",
    ]);
  });
});

test("follow and unfollow refresh the relationship-filtered feed and story tray", async () => {
  const data = snapshot();
  const rpcCalls: RpcCall[] = [];
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) return whoami(init);
    if (url.endsWith("/graphql/v1")) return graphql(data);
    if (url.endsWith("/follow_user")) {
      rpcCalls.push({ url, init });
      data.follows.push({
        follower: { id: MIRA },
        followed: { id: THEO },
        at: "2026-07-13T17:00:00Z",
      });
      return new Response(JSON.stringify({ follower: MIRA, followed: THEO }));
    }
    if (url.endsWith("/unfollow_user")) {
      rpcCalls.push({ url, init });
      data.follows = data.follows.filter(
        (edge) => !(edge.follower.id === MIRA && edge.followed.id === THEO),
      );
      return new Response(JSON.stringify({ follower: MIRA, followed: THEO }));
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const remote = driver();
    await remote.assembleBoot();
    bootMessages(remote);

    remote.deliver(command("profile", "follow-user", { user: THEO }));
    const followed = onlyOutcome(await settle(remote));
    const followedFeed = update(followed, "feed", "feed-page");
    assert.deepEqual(followedFeed.posts.map((post) => post.id), [
      "post-theo-image",
      "post-lena-video",
      "post-mira-image",
    ]);
    assert.deepEqual(followedFeed.stories.map((ring) => ring.user.id), [
      MIRA,
      THEO,
      LENA,
    ]);
    assert.equal(
      update(followed, "profile", "profile", THEO)["viewer-follows"],
      true,
    );
    assert.equal(
      update(followed, "profile", "search-results").people.find(
        (person) => person.user.id === THEO,
      )!["viewer-follows"],
      true,
    );

    remote.deliver(command("profile", "unfollow-user", { user: THEO }));
    const unfollowed = onlyOutcome(await settle(remote));
    const unfollowedFeed = update(unfollowed, "feed", "feed-page");
    assert.equal(
      unfollowedFeed.posts.some((post) => post.author.id === THEO),
      false,
    );
    assert.equal(
      unfollowedFeed.stories.some((ring) => ring.user.id === THEO),
      false,
    );

    assert.equal(rpcCalls.length, 2);
    for (const call of rpcCalls) {
      assert.equal(new Headers(call.init.headers).get("x-spock-actor"), MIRA);
      assert.deepEqual(JSON.parse(requestBody(call.init)), { target: THEO });
    }
  });
});

test("save and unsave settle every viewer-specific post surface and private grid", async () => {
  const data = snapshot();
  const rpcCalls: RpcCall[] = [];
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) return whoami(init);
    if (url.endsWith("/graphql/v1")) return graphql(data);
    if (url.endsWith("/save_post")) {
      rpcCalls.push({ url, init });
      data.saves.push({
        user: { id: MIRA },
        post: { id: "post-lena-video" },
        at: "2026-07-13T17:00:00Z",
      });
      return new Response(JSON.stringify({ user: MIRA, post: "post-lena-video" }));
    }
    if (url.endsWith("/unsave_post")) {
      rpcCalls.push({ url, init });
      data.saves = data.saves.filter(
        (save) => !(save.user.id === MIRA && save.post.id === "post-lena-video"),
      );
      return new Response(JSON.stringify({ user: MIRA, post: "post-lena-video" }));
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const remote = driver();
    await remote.assembleBoot();
    bootMessages(remote);

    remote.deliver(
      command("feed", "save-post", { post: "post-lena-video" }),
    );
    const saved = onlyOutcome(await settle(remote));
    assert.equal(
      update(saved, "feed", "post-by-id", "post-lena-video")["viewer-has-saved"],
      true,
    );
    assert.equal(
      update(saved, "feed", "feed-page").posts.find(
        (post) => post.id === "post-lena-video",
      )!["viewer-has-saved"],
      true,
    );
    assert.equal(
      update(saved, "feed", "reels").posts.find(
        (post) => post.id === "post-lena-video",
      )!["viewer-has-saved"],
      true,
    );
    assert.deepEqual(
      update(saved, "profile", "profile", MIRA).saved.map((post) => post.id),
      ["post-theo-image", "post-lena-video"],
    );

    remote.deliver(
      command("feed", "unsave-post", { post: "post-lena-video" }),
    );
    const unsaved = onlyOutcome(await settle(remote));
    assert.equal(
      update(unsaved, "feed", "post-by-id", "post-lena-video")["viewer-has-saved"],
      false,
    );
    assert.deepEqual(
      update(unsaved, "profile", "profile", MIRA).saved.map((post) => post.id),
      ["post-theo-image"],
    );

    assert.equal(rpcCalls.length, 2);
    for (const call of rpcCalls) {
      assert.equal(new Headers(call.init.headers).get("x-spock-actor"), MIRA);
      assert.deepEqual(JSON.parse(requestBody(call.init)), {
        post: "post-lena-video",
      });
    }
  });
});

test("viewing one frame advances the ring and refreshes sequence progress", async () => {
  const data = snapshot();
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) return whoami(init);
    if (url.endsWith("/graphql/v1")) return graphql(data);
    if (url.endsWith("/mark_story_viewed")) {
      const { story } = JSON.parse(requestBody(init)) as { story: string };
      data.storyViews.push({
        viewer: { id: MIRA },
        story: { id: story },
        at: "2026-07-13T17:00:00Z",
      });
      return new Response(JSON.stringify({ viewer: MIRA, story }));
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const remote = driver();
    await remote.assembleBoot();
    bootMessages(remote);

    remote.deliver(
      command("feed", "mark-story-seen", { story: "story-lena-2" }),
    );
    const viewed = onlyOutcome(await settle(remote));
    assert.equal(
      update(viewed, "feed", "feed-page").stories.find(
        (ring) => ring.user.id === LENA,
      )!.id,
      "story-lena-3",
    );
    assert.deepEqual(
      update(viewed, "feed", "story-by-id", "story-lena-3").progress,
      [
        { id: "story-lena-1", "is-current": false, "is-viewed": true },
        { id: "story-lena-2", "is-current": false, "is-viewed": true },
        { id: "story-lena-3", "is-current": true, "is-viewed": false },
      ],
    );
  });
});

test("search returns both matching people and authority post thumbnails", async () => {
  const data = snapshot();
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) return whoami(init);
    return graphql(data);
  }, async () => {
    const remote = driver();
    await remote.assembleBoot();
    bootMessages(remote);
    remote.deliver(
      command("profile", "search-people", { query: "lena" }),
    );
    const searched = onlyOutcome(await settle(remote));
    const results = update(searched, "profile", "search-results");
    assert.deepEqual(results.people.map((person) => person.user.id), [LENA]);
    assert.deepEqual(results.posts.map((post) => post.id), [
      "post-lena-video",
      "post-lena-archive",
    ]);
  });
});

test("empty create metadata publishes and uses a provenance-only fallback alt", async () => {
  const data = snapshot();
  const selected = new File(["jpeg bytes"], "sunrise.jpg", {
    type: "image/jpeg",
  });
  let publishedPayload: PublishedPayload | null = null;
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) return whoami(init);
    if (url.endsWith("/graphql/v1")) return graphql(data);
    if (url.endsWith("/object/upload/sign")) {
      return new Response(
        JSON.stringify({
          id: "upload-1",
          url: "/storage/v1/object/upload-1?exp=9999999999&sig=test",
        }),
      );
    }
    if (url.includes("/object/upload-1?") && init.method === "PUT") {
      assert.equal(new Headers(init.headers).get("content-type"), "image/jpeg");
      assert.equal(init.body, selected);
      return new Response(null, { status: 204 });
    }
    if (url.endsWith("/create_image_post")) {
      const payload = JSON.parse(requestBody(init)) as PublishedPayload;
      publishedPayload = payload;
      data.posts.push({
        id: "post-upload",
        author: { id: MIRA },
        caption: payload.caption,
        published_at: "2026-07-13T18:00:00Z",
        show_in_feed: true,
        media_kind: "image",
        media_file: { id: payload.image },
        video_file: null,
        media_alt: payload.alt,
      });
      return new Response(JSON.stringify({ id: "post-upload" }));
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const remote = driver("mira.santos", {
      signal: new AbortController().signal,
      pickFile: async () => selected,
    });
    await remote.assembleBoot();
    bootMessages(remote);

    remote.deliver(command("create", "choose-image", {}));
    const chosen = onlyOutcome(await settle(remote));
    assert.deepEqual(update(chosen, "create", "draft"), {
      uploaded: {
        object: "upload-1",
        preview: "upload-1",
        name: "sunrise.jpg",
      },
    });

    remote.deliver(
      command("create", "publish-image", {
        image: "upload-1",
        caption: "",
        alt: "",
      }),
    );
    const published = onlyOutcome(await settle(remote));
    assert.deepEqual(publishedPayload, {
      image: "upload-1",
      caption: "",
      alt: "Uploaded image “sunrise.jpg” by Mira Santos",
    });
    const post = update(published, "feed", "post-by-id", "post-upload");
    assert.equal(post.caption, "");
    assert.deepEqual(post.media, {
      image: {
        image: {
          src: "upload-1",
          alt: "Uploaded image “sunrise.jpg” by Mira Santos",
        },
      },
    });
    assert.equal(
      update(published, "profile", "profile", MIRA).posts[0]!.id,
      "post-upload",
    );
    assert.equal(
      update(published, "profile", "search-results").posts[0]!.id,
      "post-upload",
    );
    assert.deepEqual(update(published, "create", "draft"), { empty: {} });
  });
});

test("a remounted driver waits for an accepted retired mutation before boot", async () => {
  const data = snapshot();
  let graphqlCalls = 0;
  let abortedRetiredReads = 0;
  let markMutationStarted!: () => void;
  let finishMutation!: () => void;
  const mutationStarted = new Promise<void>((resolve) => {
    markMutationStarted = resolve;
  });
  const mutationFinished = new Promise<void>((resolve) => {
    finishMutation = resolve;
  });

  await withFetch(async (input, init) => {
    const url = String(input);
    if (init.signal?.aborted) {
      abortedRetiredReads += 1;
      throw new DOMException("retired", "AbortError");
    }
    if (url.endsWith("/~whoami")) return whoami(init);
    if (url.endsWith("/graphql/v1")) {
      graphqlCalls += 1;
      return graphql(data);
    }
    if (url.endsWith("/unlike_post")) {
      markMutationStarted();
      await mutationFinished;
      data.likes = data.likes.filter(
        (like) => !(like.user.id === MIRA && like.post.id === "post-lena-video"),
      );
      return new Response(JSON.stringify({ user: MIRA, post: "post-lena-video" }));
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const retired = driver();
    await retired.assembleBoot();
    bootMessages(retired);
    retired.deliver(
      command("feed", "unlike-post", { post: "post-lena-video" }),
    );
    await mutationStarted;
    retired.dispose();

    const fresh = driver();
    const freshBoot = fresh.assembleBoot();
    await Promise.resolve();
    assert.equal(graphqlCalls, 1, "fresh boot must wait behind accepted authority work");

    finishMutation();
    await freshBoot;
    assert.equal(graphqlCalls, 2);
    assert.equal(abortedRetiredReads, 1);
    assert.deepEqual(retired.tick(), []);
    assert.equal(
      projection(
        bootMessages(fresh),
        "feed",
        "post-by-id",
        "post-lena-video",
      )["viewer-has-liked"],
      false,
    );
    fresh.dispose();
  });
});

test("a retired hung upload cannot block the replacement driver boot", async () => {
  const data = snapshot();
  const controller = new AbortController();
  const selected = new File(["jpeg bytes"], "never-finishes.jpg", {
    type: "image/jpeg",
  });
  let markUploadStarted!: () => void;
  const uploadStarted = new Promise<void>((resolve) => {
    markUploadStarted = resolve;
  });
  let uploadSignal: AbortSignal | null = null;
  let graphqlCalls = 0;

  await withFetch(async (input, init) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) return whoami(init);
    if (url.endsWith("/graphql/v1")) {
      graphqlCalls += 1;
      return graphql(data);
    }
    if (url.endsWith("/object/upload/sign")) {
      uploadSignal = init.signal ?? null;
      markUploadStarted();
      // Model a transport that fails to settle even after cancellation. A
      // draft upload is not domain authority work, so it must not gate boot.
      return await new Promise<Response>(() => {});
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const retired = driver("mira.santos", {
      signal: controller.signal,
      pickFile: async () => selected,
    });
    await retired.assembleBoot();
    bootMessages(retired);
    retired.deliver(command("create", "choose-image", {}));
    await uploadStarted;
    retired.dispose();
    assert.equal(uploadSignal?.aborted, true);

    const fresh = driver();
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      await Promise.race([
        fresh.assembleBoot(),
        new Promise<never>((_resolve, reject) => {
          timeout = setTimeout(
            () => reject(new Error("replacement provider boot stayed blocked")),
            250,
          );
        }),
      ]);
    } finally {
      if (timeout !== undefined) clearTimeout(timeout);
    }
    assert.equal(graphqlCalls, 2);
    fresh.dispose();
  });
});
