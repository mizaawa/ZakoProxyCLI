import { createContext, useContext, useMemo, useSyncExternalStore } from "react";
import { LoaderCircle, RefreshCw } from "lucide-react";
import { Button, ErrorBox } from "./components";
import { ResourceCache } from "./resourceCache";

export const Resources = createContext<ResourceCache | null>(null);
export const keyListPath = "/keys?usage=false";
export function useResources() {
  const cache = useContext(Resources);
  if (!cache) throw new Error("需要登录工作空间");
  return cache;
}
const idle = { data: undefined, loading: false, error: "" };
const idleSnapshot = () => idle;
const unsubscribe = () => {};
const idleSubscribe = () => unsubscribe;
const idleLoad = () => Promise.resolve();
export function useResource<T>(path: string | null) {
  const cache = useResources();
  const entry = useMemo(() => path ? cache.get<T>(path) : undefined, [cache, path]);
  const snapshot = useSyncExternalStore(entry?.subscribe ?? idleSubscribe, entry?.getSnapshot ?? idleSnapshot);
  return { ...snapshot, load: entry?.load ?? idleLoad };
}
export function Loading({ label }: { label: string }) {
  return <div className="resource-status" role="status"><LoaderCircle className="spin" size={18} />正在加载{label}…</div>;
}
export function ResourceStatus({ queries, label }: {
  queries: { data?: unknown; error: string; load: (force: boolean) => Promise<void> }[];
  label: string;
}) {
  const failed = queries.filter((q) => q.error);
  if (failed.length) return <div className="page-load-error" role="alert"><ErrorBox message={`${label}加载失败：${failed[0].error}`} />
    <Button className="small-button" onClick={() => failed.forEach((q) => void q.load(true))}><RefreshCw size={15} />重试</Button></div>;
  return queries.some((q) => q.data === undefined) ? <Loading label={label} /> : null;
}
