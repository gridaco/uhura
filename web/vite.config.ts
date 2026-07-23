import path from "node:path";

import { defineConfig } from "vite";

const backend = process.env["UHURA_NATIVE_ORIGIN"] ?? "http://127.0.0.1:8787";
const HOST_CONFIG_PROTOCOL = "uhura-host-config/0";

export default defineConfig(({ mode }) => {
  const exportTemplate = mode === "export";
  const profile = exportTemplate ? "export-template" : "live";
  const assetBase = exportTemplate ? "./" : "/";

  return {
    root: path.resolve(import.meta.dirname, "src/app"),
    base: assetBase,
    plugins: [{
      name: "uhura-web-build-metadata",
      generateBundle() {
        this.emitFile({
          type: "asset",
          fileName: "uhura-web-build.json",
          source: `${JSON.stringify({
            protocol: "uhura-web-build/1",
            profile,
            assetBase,
            hostConfigProtocol: HOST_CONFIG_PROTOCOL,
          }, null, 2)}\n`,
        });
      },
    }],
    publicDir: false,
    build: {
      outDir: path.resolve(
        import.meta.dirname,
        exportTemplate ? "dist-export" : "dist",
      ),
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
  };
});
