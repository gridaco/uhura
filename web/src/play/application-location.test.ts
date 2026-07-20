import { describe, expect, it } from "vitest";

import {
  applicationPathForBrowser,
  browserUrlForApplication,
} from "./application-location.js";

describe("Play compatibility location", () => {
  it("presents /play as the application's root route", () => {
    expect(applicationPathForBrowser({
      pathname: "/play",
      search: "?tab=home",
      hash: "#top",
    })).toBe("/?tab=home#top");
    expect(applicationPathForBrowser({
      pathname: "/search",
      search: "?q=uhura",
      hash: "",
    })).toBe("/search?q=uhura");
  });

  it("keeps the application's root inside the mounted Play surface", () => {
    expect(
      browserUrlForApplication("/", "http://localhost/search").pathname,
    ).toBe("/play");
    expect(
      browserUrlForApplication(
        "/profile/mira?tab=posts",
        "http://localhost/play",
      ).pathname,
    ).toBe("/profile/mira");
  });
});
