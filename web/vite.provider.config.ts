import path from "node:path";

import { defineConfig, type Plugin } from "vite";

const providerRoot = path.resolve(
  import.meta.dirname,
  "../examples/instagram/client/providers",
);

const assertSingleProviderChunk = (): Plugin => ({
  name: "assert-single-provider-chunk",
  generateBundle(_options, bundle) {
    const files = Object.keys(bundle).sort();
    if (files.length !== 1 || files[0] !== "spock.js") {
      this.error(`unexpected provider bundle: ${files.join(", ")}`);
    }
  },
});

export default defineConfig({
  publicDir: false,
  plugins: [assertSingleProviderChunk()],
  build: {
    outDir: path.join(providerRoot, "dist"),
    emptyOutDir: true,
    copyPublicDir: false,
    target: "es2022",
    sourcemap: false,
    minify: false,
    lib: {
      entry: path.join(providerRoot, "spock.ts"),
      formats: ["es"],
      fileName: () => "spock.js",
    },
    rolldownOptions: {
      output: {
        codeSplitting: false,
        banner: "// Generated from providers/spock.ts; do not edit.",
      },
    },
  },
});
