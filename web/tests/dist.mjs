import test from "node:test";
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { gunzipSync } from "node:zlib";

const dist = new URL("../dist/", import.meta.url);

test("gzip and identity serve exactly the same final JS and CSS", async () => {
  const assets = new URL("assets/", dist);
  const names = await readdir(assets);
  for (const name of names.filter((name) => /\.(js|css)$/.test(name))) {
    const source = await readFile(new URL(name, assets));
    if (source.length < 1024) continue;
    const gz = await readFile(new URL(`${name}.gz`, assets));
    assert.ok(gunzipSync(gz).equals(source), `${name}: compressed bytes are not the shipped bundle`);
  }
});

test("the entry HTML keeps page-only code outside the cold-start JS budget", async () => {
  const html = await readFile(new URL("index.html", dist), "utf8");
  const entryScripts = [...html.matchAll(/(?:src|href)="\/([^"?]+\.js)"/g)].map((match) => match[1]);
  assert.ok(entryScripts.length > 0);
  const files = await Promise.all(entryScripts.map((name) => readFile(new URL(name, dist))));
  assert.ok(files.reduce((bytes, file) => bytes + file.length, 0) < 400_000, "initial JavaScript exceeds 400 kB uncompressed");
});
