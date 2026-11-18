import test from "node:test";
import assert from "node:assert/strict";
import { ResourceCache } from "../src/resourceCache.ts";

function fixture() {
  const calls = [];
  const cache = new ResourceCache((path, signal) => new Promise((resolve, reject) => {
    calls.push({ path, signal, resolve, reject });
  }));
  return { cache, calls };
}

test("preloading, mounting twice, and navigating back share the same read", async () => {
  const { cache, calls } = fixture();
  const groups = cache.get("/groups");
  const preload = groups.load();
  groups.subscribe(() => {})();
  const unsubscribe = groups.subscribe(() => {});
  assert.equal(groups.load(true), preload);
  calls[0].resolve(["a"]);
  await preload;
  unsubscribe();
  cache.get("/groups").subscribe(() => {})();
  assert.equal(calls.length, 1);
  assert.deepEqual(groups.getSnapshot().data, ["a"]);
});

test("invalidate refreshes mounted variants only and discards late responses", async () => {
  const { cache, calls } = fixture();
  const stats = cache.get("/stats?from=1");
  stats.set("previous");
  stats.subscribe(() => {});
  const other = cache.get("/stats?from=2");
  other.set("other range");
  const first = stats.load(true);
  cache.invalidate("/stats");
  assert.equal(calls[0].signal.aborted, true);
  assert.deepEqual(calls.map(c => c.path), ["/stats?from=1", "/stats?from=1"]);
  assert.equal(stats.getSnapshot().data, "previous");
  const refresh = stats.load();
  calls[1].resolve("current"); calls[0].resolve("obsolete");
  await Promise.all([first, refresh]);
  assert.equal(stats.getSnapshot().data, "current");
  const next = other.load(); calls[2].resolve("new range"); await next;
  assert.equal(other.getSnapshot().data, "new range");
});

test("a failed background refresh retains the last good result and can be retried", async () => {
  const { cache, calls } = fixture();
  const logs = cache.get("/logs?page=1"); logs.set([1]);
  logs.subscribe(() => {});
  cache.get("/accounts").set([]);
  const refresh = cache.refreshActive();
  calls[0].reject(new Error("offline")); await refresh;
  assert.deepEqual(logs.getSnapshot(), { data: [1], loading: false, error: "offline" });
  const retry = logs.load(true); calls[1].resolve([2]); await retry;
  assert.equal(logs.getSnapshot().error, "");
  assert.equal(calls.length, 2);
});

test("a new session cannot consume a previous session's pending read", async () => {
  const { cache, calls } = fixture();
  const first = cache.get("/settings"); const old = first.load();
  cache.dispose();
  const next = fixture();
  const second = next.cache.get("/settings"); const fresh = second.load();
  assert.equal(calls[0].signal.aborted, true);
  next.calls[0].resolve("new session"); calls[0].resolve("old session"); await Promise.all([old, fresh]);
  assert.equal(first.getSnapshot().data, undefined);
  assert.equal(second.getSnapshot().data, "new session");
  await first.load(true);
  assert.equal(calls.length, 1);
});
