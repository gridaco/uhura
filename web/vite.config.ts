import path from "node:path";

import { defineConfig } from "vite";

const backend = process.env["UHURA_NATIVE_ORIGIN"] ?? "http://127.0.0.1:8787";

export default defineConfig({
  root: path.resolve(import.meta.dirname, "src/app"),
  base: "/",
  publicDir: false,
  build: {
    outDir: path.resolve(import.meta.dirname, "dist"),
    emptyOutDir: true,
    copyPublicDir: false,
    target: "es2022",
    sourcemap: false,
  },
  server: {
    proxy: {
      "/api": backend,
    },
  },
});
