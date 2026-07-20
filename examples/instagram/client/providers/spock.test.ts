import assert from "node:assert/strict";

import { test } from "vitest";

import { createUhuraAdapters } from "./spock.js";

interface WireValue {
  readonly $: string;
  readonly [field: string]: unknown;
}

type TestFetch = (
  input: RequestInfo | URL,
  init: RequestInit,
) => Promise<Response>;

const MODULE = "app.instagram@1";
const MACHINE = `${MODULE}::Instagram`;
const POST_ID = `${MODULE}::PostId`;
const REQUEST_ID = `${MODULE}::RequestId`;
const MUTATION = `${MODULE}::Mutation`;
const MUTATIONS_SEND = `${MACHINE}::port.mutations.Send`;

const MIRA = "user-mira";
const LENA = "user-lena";

function snapshot() {
  return {
    users: [
      {
        id: MIRA,
        username: "mira.santos",
        display_name: "Mira Santos",
        avatar: { id: "avatar-mira" },
        avatar_alt: "Mira Santos",
        bio: "Designer",
      },
      {
        id: LENA,
        username: "lena.holt",
        display_name: "Lena Holt",
        avatar: { id: "avatar-lena" },
        avatar_alt: "Lena Holt",
        bio: "Clay and slow mornings",
      },
    ],
    stories: [
      {
        id: "story-lena-1",
        author: { id: LENA },
        position: 1,
        media_file: { id: "story-media-lena" },
        media_alt: "Clay on a wheel",
        caption: "Centering",
        published_at: "2026-07-13T15:00:00Z",
      },
    ],
    storyViews: [],
    posts: [
      {
        id: "post-lena-image",
        author: { id: LENA },
        caption: "Kiln notes from Lena",
        published_at: "2026-07-13T13:00:00Z",
        show_in_feed: true,
        media_kind: "image",
        media_file: { id: "media-lena" },
        video_file: null,
        media_alt: "Copper glaze moving through kiln light",
      },
    ],
    slides: [],
    comments: [],
    likes: [],
    saves: [],
    follows: [
      {
        follower: { id: MIRA },
        followed: { id: LENA },
        at: "2026-07-01T00:00:00Z",
      },
    ],
    postTags: [],
  };
}

function frameworkEnvironment(): Record<string, unknown> {
  return {
    protocol: "spock-host-environment/1",
    mode: "dev",
    project_generation_id: 7,
    backend_generation_id: 3,
    authority: {
      graphql_path: "/framework/graphql",
      rpc_path: "/framework/rpc",
      storage_path: "/framework/storage",
    },
  };
}

function whoami(init: RequestInit): Response {
  const actor = new Headers(init.headers).get("x-spock-actor");
  return new Response(
    JSON.stringify({ actor, known: true, anonymous: actor === null }),
  );
}

async function withFetch<T>(
  fetcher: TestFetch,
  run: () => Promise<T>,
): Promise<T> {
  const original = globalThis.fetch;
  globalThis.fetch = (input, init) => fetcher(input, init ?? {});
  try {
    return await run();
  } finally {
    globalThis.fetch = original;
  }
}

function variant(
  type: string,
  caseName: string,
  fields: ReadonlyArray<readonly [string | null, WireValue]> = [],
): WireValue {
  return {
    $: "variant",
    type,
    case: caseName,
    fields: fields.map(([name, value]) => ({ name, value })),
  };
}

function text(value: string): WireValue {
  return { $: "Text", value };
}

function bool(value: boolean): WireValue {
  return { $: "bool", value };
}

function key(type: string, value: WireValue): WireValue {
  return { $: "key", type, value };
}

function textKeyMapKeys(value: WireValue): string[] {
  assert.equal(value.$, "map");
  assert.ok(Array.isArray(value.entries));
  return (value.entries as WireValue[][]).map(([entryKey]) => {
    assert.ok(entryKey);
    assert.equal(entryKey.$, "key");
    assert.equal(typeof entryKey.value, "object");
    assert.notEqual(entryKey.value, null);
    const underlying = entryKey.value as WireValue;
    assert.equal(underlying.$, "Text");
    assert.equal(typeof underlying.value, "string");
    return underlying.value as string;
  });
}

function request(
  id: number,
  mutation: string,
  fields: ReadonlyArray<readonly [string | null, WireValue]> = [],
): WireValue {
  return variant(MUTATIONS_SEND, "request", [
    [
      "id",
      key(REQUEST_ID, { $: "PositiveInt", value: String(id) }),
    ],
    ["payload", variant(MUTATION, mutation, fields)],
  ]);
}

function caseOf(value: unknown): string {
  assert.equal(typeof value, "object");
  assert.notEqual(value, null);
  assert.equal((value as Record<string, unknown>).$, "variant");
  const caseName = (value as Record<string, unknown>).case;
  assert.equal(typeof caseName, "string");
  return caseName as string;
}

function namedField(value: unknown, name: string): WireValue {
  assert.equal(typeof value, "object");
  assert.notEqual(value, null);
  const fields = (value as Record<string, unknown>).fields;
  assert.ok(Array.isArray(fields));
  const found = fields.find(
    (field) =>
      typeof field === "object"
      && field !== null
      && (field as Record<string, unknown>).name === name,
  );
  assert.ok(found, `missing wire field ${name}`);
  return (found as Record<string, unknown>).value as WireValue;
}

function onlyField(value: unknown): WireValue {
  assert.equal(typeof value, "object");
  assert.notEqual(value, null);
  const fields = (value as Record<string, unknown>).fields;
  assert.ok(Array.isArray(fields));
  assert.equal(fields.length, 1);
  return (fields[0] as Record<string, unknown>).value as WireValue;
}

function makeHarness(
  pickFile: () => Promise<File | null> = async () => null,
) {
  const abort = new AbortController();
  const requirements = {
    authority: {
      port: "authority",
      adapter: "app.provider",
      contractHash: "authority-contract",
      contractInstanceHash: "authority-instance",
    },
    mutations: {
      port: "mutations",
      adapter: "app.provider",
      contractHash: "mutations-contract",
      contractInstanceHash: "mutations-instance",
    },
  } as const;
  const provider = createUhuraAdapters(
    {
      graphql_url: "http://standalone.test/graphql/v1",
      rpc_url: "http://standalone.test/rest/v1/rpc",
      storage_url: "http://standalone.test/storage/v1",
      actor: "mira.santos",
    },
    {
      signal: abort.signal,
      pickFile: async () => pickFile(),
      port(name: string) {
        if (name !== "authority" && name !== "mutations") {
          throw new Error(`unexpected port ${name}`);
        }
        return requirements[name];
      },
    },
  );
  const authority = provider.adapters.find(
    (adapter) => adapter.port === "authority",
  );
  const mutations = provider.adapters.find(
    (adapter) => adapter.port === "mutations",
  );
  assert.ok(authority);
  assert.ok(mutations);
  assert.equal(authority.adapter, "app.provider");
  assert.equal(mutations.adapter, "app.provider");
  const authorityValues: WireValue[] = [];
  const mutationValues: WireValue[] = [];
  const authorityContext = {
    signal: abort.signal,
    deliver(value: WireValue): void {
      authorityValues.push(value);
    },
  };
  const mutationsContext = {
    signal: abort.signal,
    deliver(value: WireValue): void {
      mutationValues.push(value);
    },
  };
  return {
    abort,
    provider,
    authority,
    mutations,
    authorityContext,
    mutationsContext,
    authorityValues,
    mutationValues,
  };
}

test("boots through admitted authority and mutation port identities", async () => {
  const data = snapshot();
  data.stories.push({
    id: "s",
    author: { id: LENA },
    position: 2,
    media_file: { id: "story-media-lena-2" },
    media_alt: "Glaze buckets beside the kiln",
    caption: "Firing day",
    published_at: "2026-07-13T15:30:00Z",
  });
  const calls: string[] = [];
  await withFetch(async (input, init) => {
    const url = String(input);
    calls.push(url);
    if (url === "/~project/environment") {
      return new Response(JSON.stringify(frameworkEnvironment()));
    }
    if (url === "/framework/graphql") {
      return new Response(JSON.stringify({ data }));
    }
    if (url === "/~whoami") return whoami(init);
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const harness = makeHarness();
    await harness.authority.start?.(harness.authorityContext);

    assert.equal(harness.authority.contractHash, "authority-contract");
    assert.equal(harness.authority.contractInstanceHash, "authority-instance");
    assert.equal(harness.mutations.contractHash, "mutations-contract");
    assert.equal(harness.mutations.contractInstanceHash, "mutations-instance");
    assert.equal(harness.authorityValues.length, 1);
    const observed = harness.authorityValues[0];
    assert.equal(caseOf(observed), "authority.observed");
    const authority = namedField(observed, "value");
    assert.equal(caseOf(authority), "Ready");
    const authorityData = namedField(authority, "data");
    const storyDetails = namedField(authorityData, "story_details");
    assert.equal(storyDetails.$, "map");
    assert.ok(Array.isArray(storyDetails.entries));
    assert.equal(storyDetails.entries.length, 2);
    assert.deepEqual(textKeyMapKeys(storyDetails), ["s", "story-lena-1"]);
    const storyDetailValues = (storyDetails.entries as WireValue[][]).map(
      (entry) => entry[1],
    );
    const previousOptions = storyDetailValues.map((detail) =>
      namedField(detail, "previous")
    );
    const nextOptions = storyDetailValues.map((detail) =>
      namedField(detail, "next")
    );
    const presentPrevious = previousOptions.filter(
      (option) => caseOf(option) === "some",
    );
    const presentNext = nextOptions.filter(
      (option) => caseOf(option) === "some",
    );
    assert.equal(presentPrevious.length, 1);
    assert.equal(presentNext.length, 1);
    for (const option of [...presentPrevious, ...presentNext]) {
      assert.equal(namedField(option, "value").$, "key");
    }
    assert.deepEqual(
      textKeyMapKeys(namedField(authorityData, "profiles")),
      [LENA, MIRA],
    );
    assert.deepEqual(harness.provider.systemInfo(), {
      actor: MIRA,
      actors: [
        { id: LENA, username: "lena.holt", label: "Lena Holt" },
        { id: MIRA, username: "mira.santos", label: "Mira Santos" },
      ],
    });
    harness.provider.dispose();
  });

  assert.deepEqual(calls.slice(0, 3), [
    "/~project/environment",
    "/framework/graphql",
    "/~whoami",
  ]);
});

test("settles a machine mutation and publishes refreshed authority", async () => {
  const data = snapshot();
  const rpcBodies: unknown[] = [];
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url === "/~project/environment") {
      return new Response(JSON.stringify(frameworkEnvironment()));
    }
    if (url === "/framework/graphql") {
      return new Response(JSON.stringify({ data }));
    }
    if (url === "/~whoami") return whoami(init);
    if (url === "/framework/rpc/like_post") {
      assert.equal(init.method, "POST");
      assert.equal(new Headers(init.headers).get("x-spock-actor"), MIRA);
      assert.equal(typeof init.body, "string");
      rpcBodies.push(JSON.parse(init.body as string));
      (data.likes as Array<{
        user: { id: string };
        post: { id: string };
        at: string;
      }>).push({
        user: { id: MIRA },
        post: { id: "post-lena-image" },
        at: "2026-07-13T16:00:00Z",
      });
      return new Response(JSON.stringify({ user: MIRA, post: "post-lena-image" }));
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const harness = makeHarness();
    await harness.authority.start?.(harness.authorityContext);
    await harness.mutations.accept(
      request(1, "SetLike", [
        ["post", key(POST_ID, text("post-lena-image"))],
        ["liked", bool(true)],
      ]),
      harness.mutationsContext,
    );

    assert.equal(harness.mutationValues.length, 1);
    const settled = harness.mutationValues[0];
    assert.equal(caseOf(settled), "mutations.settled");
    const result = namedField(settled, "result");
    assert.equal(caseOf(result), "Accepted", JSON.stringify(result));
    assert.deepEqual(rpcBodies, [{ post: "post-lena-image" }]);
    assert.equal(harness.authorityValues.length, 2);
    harness.provider.dispose();
  });
});

test("accepts the browser-unqualified request case for search", async () => {
  const data = snapshot();
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url === "/~project/environment") {
      return new Response(JSON.stringify(frameworkEnvironment()));
    }
    if (url === "/framework/graphql") {
      return new Response(JSON.stringify({ data }));
    }
    if (url === "/~whoami") return whoami(init);
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const harness = makeHarness();
    await harness.authority.start?.(harness.authorityContext);
    await harness.mutations.accept(
      request(1, "SearchPeople", [["query", text("nils")]]),
      harness.mutationsContext,
    );

    assert.equal(harness.mutationValues.length, 1);
    const settled = harness.mutationValues[0];
    assert.equal(caseOf(settled), "mutations.settled");
    assert.equal(caseOf(namedField(settled, "result")), "Accepted");
    assert.equal(harness.authorityValues.length, 2);
    harness.provider.dispose();
  });
});

test("returns ImageReady directly from the current mutation contract", async () => {
  const data = snapshot();
  const selected = new File(["jpeg bytes"], "sunrise.jpg", {
    type: "image/jpeg",
  });
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url === "/~project/environment") {
      return new Response(JSON.stringify(frameworkEnvironment()));
    }
    if (url === "/framework/graphql") {
      return new Response(JSON.stringify({ data }));
    }
    if (url === "/~whoami") return whoami(init);
    if (url === "/framework/storage/object/upload/sign") {
      return new Response(
        JSON.stringify({
          id: "upload-1",
          url: "/framework/storage/object/upload-1?exp=9999999999&sig=test",
        }),
      );
    }
    if (
      url === "/framework/storage/object/upload-1?exp=9999999999&sig=test"
      && init.method === "PUT"
    ) {
      assert.equal(init.body, selected);
      return new Response(null, { status: 204 });
    }
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const harness = makeHarness(async () => selected);
    await harness.authority.start?.(harness.authorityContext);
    await harness.mutations.accept(
      request(2, "ChooseImage"),
      harness.mutationsContext,
    );

    assert.equal(harness.authorityValues.length, 1);
    const settled = harness.mutationValues[0];
    const result = namedField(settled, "result");
    assert.equal(caseOf(result), "ImageReady");
    assert.deepEqual(
      [
        namedField(result, "object").value,
        namedField(result, "preview").value,
        namedField(result, "name").value,
      ],
      ["upload-1", "upload-1", "sunrise.jpg"],
    );
    harness.provider.dispose();
  });
});

test("resolves checked local assets without contacting Spock storage", async () => {
  let fetched = false;
  await withFetch(async () => {
    fetched = true;
    throw new Error("local fixture asset must not use fetch");
  }, async () => {
    const harness = makeHarness();
    assert.equal(
      await harness.provider.resolveAsset("video-nils-aurora"),
      "/api/play/assets/media-nils-aurora.mp4",
    );
    harness.provider.dispose();
  });
  assert.equal(fetched, false);
});

test("cancelling the picker settles as an explicit refusal", async () => {
  const data = snapshot();
  await withFetch(async (input, init) => {
    const url = String(input);
    if (url === "/~project/environment") {
      return new Response(JSON.stringify(frameworkEnvironment()));
    }
    if (url === "/framework/graphql") {
      return new Response(JSON.stringify({ data }));
    }
    if (url === "/~whoami") return whoami(init);
    throw new Error(`unexpected fetch ${url}`);
  }, async () => {
    const harness = makeHarness();
    await harness.authority.start?.(harness.authorityContext);
    await harness.mutations.accept(
      request(3, "ChooseImage"),
      harness.mutationsContext,
    );
    const result = namedField(harness.mutationValues[0], "result");
    assert.equal(caseOf(result), "Refused");
    assert.equal(onlyField(result).value, "selection-cancelled");
    harness.provider.dispose();
  });
});
