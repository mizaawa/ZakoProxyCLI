// Only idempotent console reads belong here. Each login owns a new, memory-only cache.
type Loader = (path: string, signal: AbortSignal) => Promise<unknown>;
type Snapshot<T> = { data?: T; loading: boolean; error: string };

export class Resource<T> {
  private snapshot: Snapshot<T> = { loading: true, error: "" };
  private listeners = new Set<() => void>();
  private updated = 0;
  private pending?: { controller: AbortController; promise: Promise<void> };
  private fetchData: (signal: AbortSignal) => Promise<unknown>;
  constructor(fetchData: (signal: AbortSignal) => Promise<unknown>) { this.fetchData = fetchData; }

  getSnapshot = () => this.snapshot;
  get active() { return this.listeners.size > 0; }
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    void this.load();
    return () => { this.listeners.delete(listener); };
  };
  private publish(snapshot: Snapshot<T>) {
    this.snapshot = snapshot;
    this.listeners.forEach((listener) => listener());
  }
  load = (force = false): Promise<void> => {
    if (this.pending) return this.pending.promise;
    if (!force && this.updated && Date.now() - this.updated < 30_000) return Promise.resolve();
    const controller = new AbortController();
    const promise = this.fetchData(controller.signal).then((data) => {
      if (controller.signal.aborted) return;
      this.updated = Date.now();
      this.publish({ data: data as T, loading: false, error: "" });
    }, (failure) => {
      if (controller.signal.aborted) return;
      this.updated = 0;
      this.publish({ ...this.snapshot, loading: false, error: failure instanceof Error ? failure.message : "加载失败" });
    }).finally(() => {
      if (this.pending?.controller === controller) this.pending = undefined;
    });
    this.pending = { controller, promise };
    this.publish({ ...this.snapshot, loading: true, error: "" });
    return promise;
  };
  invalidate() {
    this.updated = 0;
    this.pending?.controller.abort();
    this.pending = undefined;
  }
  set(data: T) {
    this.invalidate();
    this.updated = Date.now();
    this.publish({ data, loading: false, error: "" });
  }
}

export class ResourceCache {
  private entries = new Map<string, Resource<unknown>>();
  private lifetime = new AbortController();
  get signal() { return this.lifetime.signal; }
  private fetchData: Loader;
  constructor(fetchData: Loader) { this.fetchData = fetchData; }
  get<T>(path: string): Resource<T> {
    let entry = this.entries.get(path);
    if (!entry) {
      // Bound dated queries and log pages without evicting subscribed views.
      if (this.entries.size >= 48) {
        for (const [key, candidate] of this.entries) {
          if (!candidate.active) { candidate.invalidate(); this.entries.delete(key); break; }
        }
      }
      entry = new Resource((signal) => this.signal.aborted
        ? Promise.reject(new Error("登录会话已结束")) : this.fetchData(path, signal));
    }
    this.entries.delete(path);
    this.entries.set(path, entry);
    return entry as Resource<T>;
  }
  // Invalidate all query variants; only mounted consumers issue a new request.
  invalidate(...paths: string[]) {
    for (const [path, entry] of this.entries) {
      if (!paths.includes(path.split("?")[0])) continue;
      entry.invalidate();
      if (entry.active) void entry.load();
    }
  }
  async refreshActive() {
    await Promise.all([...this.entries.values()].filter((entry) => entry.active).map((entry) => entry.load(true)));
  }
  dispose() {
    this.lifetime.abort();
    this.entries.forEach((entry) => entry.invalidate());
    this.entries.clear();
  }
}
