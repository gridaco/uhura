import path from "node:path";

import { defineConfig, type Plugin } from "vite";

const assertSingleCanvasChunk = (): Plugin => ({
  name: "assert-single-canvas-chunk",
  generateBundle(_options, bundle) {
    const files = Object.keys(bundle).sort();
    if (files.length !== 1 || files[0] !== "canvas-chrome.js") {
      this.error(`unexpected Canvas bundle: ${files.join(", ")}`);
    }
  },
});

export default defineConfig({
  publicDir: false,
  plugins: [assertSingleCanvasChunk()],
  build: {
    outDir: path.resolve(import.meta.dirname, "dist/editor"),
    emptyOutDir: true,
    copyPublicDir: false,
    target: "es2022",
    sourcemap: false,
    minify: false,
    rolldownOptions: {
      input: path.resolve(import.meta.dirname, "src/editor/canvas-chrome.ts"),
      output: {
        format: "iife",
        exports: "none",
        codeSplitting: false,
        entryFileNames: "canvas-chrome.js",
        banner: "// Generated from web/src/editor/canvas-chrome.ts; do not edit.",
      },
    },
  },
});
