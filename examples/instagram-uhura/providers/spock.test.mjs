import assert from "node:assert/strict";
import test from "node:test";

import { createDriver } from "./spock.js";

const USERS = [
  {
    id: "user-lena",
    username: "lena.holt",
    display_name: "Lena Holt",
    avatar: { id: "avatar-lena" },
    avatar_alt: "Lena Holt",
    bio: null,
  },
  {
    id: "user-mira",
    username: "mira.santos",
    display_name: "Mira Santos",
    avatar: { id: "avatar-mira" },
    avatar_alt: "Mira Santos",
    bio: "Designer",
  },
];

const SNAPSHOT = {
  users: USERS,
  stories: [],
  storyViews: [],
  posts: [],
  slides: [],
  comments: [],
  likes: [],
  follows: [],
  postTags: [],
};

function driver(actor) {
  return createDriver(
    {
      graphql_url: "http://spock.test/graphql/v1",
      rpc_url: "http://spock.test/rest/v1/rpc",
      storage_url: "http://spock.test/storage/v1",
      actor,
    },
    { pickFile: async () => null },
  );
}

test("normalizes a configured username and exposes authority-owned actors", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (input) => {
    const url = String(input);
    if (url.endsWith("/~whoami")) {
      return new Response(
        JSON.stringify({ actor: "user-mira", known: true, anonymous: false }),
      );
    }
    return new Response(JSON.stringify({ data: SNAPSHOT }));
  };
  try {
    const remote = driver("mira.santos");
    await remote.assembleBoot();
    assert.deepEqual(remote.systemInfo(), {
      actor: "user-mira",
      actors: [
        { id: "user-lena", username: "lena.holt", label: "Lena Holt" },
        { id: "user-mira", username: "mira.santos", label: "Mira Santos" },
      ],
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("retains the actor catalog when the configured actor is invalid", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () =>
    new Response(JSON.stringify({ data: SNAPSHOT }));
  try {
    const remote = driver("typo");
    await assert.rejects(remote.assembleBoot(), /actor `typo` is not a seeded user/);
    assert.deepEqual(remote.systemInfo(), {
      actor: "typo",
      actors: [
        { id: "user-lena", username: "lena.holt", label: "Lena Holt" },
        { id: "user-mira", username: "mira.santos", label: "Mira Santos" },
      ],
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
});
