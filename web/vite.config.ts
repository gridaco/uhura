import path from "node:path";

import { defineConfig, type Plugin } from "vite";

const backend = "http://127.0.0.1:8787";
const proxiedPaths = [
  "/assets",
  "/boot.json",
  "/events",
  "/fixture.json",
  "/icons.json",
  "/ir.json",
  "/play.json",
  "/provider.js",
  "/script.json",
  "/stylesheet.css",
  "/wasm",
];

const appStylesLast = (): Plugin => ({
  name: "uhura-app-styles-last",
  transformIndexHtml: {
    order: "post",
    handler: () => [
      {
        tag: "link",
        attrs: { rel: "stylesheet", href: "/stylesheet.css" },
        injectTo: "head",
      },
    ],
  },
});

export default defineConfig({
  root: path.resolve(import.meta.dirname, "src/play"),
  base: "/shell/",
  publicDir: false,
  plugins: [appStylesLast()],
  build: {
    outDir: path.resolve(import.meta.dirname, "dist/play"),
    emptyOutDir: true,
    copyPublicDir: false,
    target: "es2022",
    sourcemap: false,
  },
  server: {
    proxy: Object.fromEntries(proxiedPaths.map((pathname) => [pathname, backend])),
  },
});
