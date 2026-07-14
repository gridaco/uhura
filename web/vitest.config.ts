import path from "node:path";

import { defineConfig } from "vitest/config";

export default defineConfig({
  root: path.resolve(import.meta.dirname, ".."),
  cacheDir: path.resolve(import.meta.dirname, "node_modules/.vite"),
  test: {
    environment: "node",
    include: [
      "web/src/**/*.test.ts",
      "examples/instagram-uhura/providers/*.test.ts",
    ],
  },
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "src"),
    },
  },
});
