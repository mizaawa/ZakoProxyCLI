import { gzipSync } from "node:zlib";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { licenseNotices } from "./license-notices.mjs";

export default defineConfig({
  plugins: [react(), licenseNotices(), {
    name: "precompress-console-assets",
    apply: "build",
    enforce: "post",
    // Vite rewrites import preloads late in generateBundle. Compress the final bytes,
    // otherwise a gzip-capable browser can receive different JS from the plain file.
    async writeBundle({ dir }, bundle) {
      if (!dir) return;
      await Promise.all(Object.keys(bundle).filter((name) => /\.(js|css)$/.test(name)).map(async (name) => {
        const path = resolve(dir, name);
        const source = await readFile(path);
        if (source.length >= 1024) await writeFile(`${path}.gz`, gzipSync(source, { level: 9 }));
      }));
    },
  }],
  server: {
    proxy: {
      "/api": "http://127.0.0.1:1990",
      "/health": "http://127.0.0.1:1990",
    },
  },
  build: {
    outDir: "dist",
    sourcemap: false,
  },
});
