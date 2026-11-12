export class ApiError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

export async function api<T = unknown>(
  path: string,
  method = "GET",
  body?: unknown,
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method,
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      "X-Console-Path": window.location.pathname,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await response.text();
  let value: unknown;
  try {
    value = text ? JSON.parse(text) : {};
  } catch {
    value = {};
  }
  if (!response.ok) {
    const failure = value as {
      error?: { message?: string } | string;
      message?: string;
    };
    const message =
      typeof failure.error === "string"
        ? failure.error
        : failure.error?.message;
    if (
      response.status === 401 &&
      !["/login", "/setup", "/bootstrap"].includes(path)
    )
      window.dispatchEvent(new Event("session-expired"));
    throw new ApiError(
      message || failure.message || `请求失败 (${response.status})`,
      response.status,
    );
  }
  return value as T;
}

export type Group = {
  id: string;
  name: string;
  description: string;
};
export type Account = {
  id: string;
  name: string;
  endpoint: string;
  protocols: string[];
  models: string[];
  model_mappings: Record<string, string>;
  group_ids: string[];
  priority: number;
  rpm: number;
  concurrency: number;
  enabled: boolean;
  has_api_key: boolean;
  has_proxy: boolean;
  proxy_display: string;
  runtime: { active: number; failures: number; cooldown_until: number };
  provider_binding?: ProviderBinding | null;
};
export type ProviderBinding = {
  provider_id: string;
  key_id: string;
  key_name: string;
  group_name: string;
  group_multiplier: number | null;
  status: string;
  last_synced_at: number | null;
};
export type ProviderSummary = {
  id: string;
  name: string;
  kind: "sub2api" | "newapi";
  base_url: string;
  status: string;
  official?: boolean;
  readonly?: boolean;
  pinned?: boolean;
};
export type ProviderAuthRequirements = {
  turnstile_required: boolean;
  turnstile_site_key: string;
};
export type ProviderKeyGroup = { id: string; name: string; multiplier: number | null };
export type ProviderKeyGroups = { provider_id: string; groups: ProviderKeyGroup[] };
export type ProviderKeyCreated = { provider_id: string; created: true; name: string; group_id: string };
export type ProviderAccountKey = {
  id: string;
  name: string;
  group_name: string;
  group_multiplier: number | null;
  status: string;
  available: boolean;
};
export type ProviderAccountKeys = {
  provider_id: string;
  endpoint: string;
  keys: ProviderAccountKey[];
  synced_at: number | null;
};
export type AccountProviderBinding = {
  account_id: string;
  endpoint: string;
  provider_binding: ProviderBinding | null;
  available: boolean;
  synced_at: number | null;
};
export type ProxyTestResult = {
  connected: boolean;
  ok: boolean;
  latency_ms: number;
  upstream_status: number | null;
  error: string | null;
};
export type Usage = {
  input_tokens: number;
  output_tokens: number;
  cached_tokens: number;
  cache_creation_input_tokens?: number;
  cache_read_input_tokens?: number;
};
export type RequestLog = {
  id: string;
  timestamp: number;
  key_id: string;
  account_id: string;
  key_name: string | null;
  account_name: string | null;
  model: string;
  protocol: string;
  request_path?: string;
  status: number;
  latency_ms: number;
  first_token_ms?: number | null;
  usage: Usage;
  cyber: boolean | number;
  attempts: number;
  error: string | null;
};
export type RequestLogPage = {
  items: RequestLog[];
  page: number;
  page_size: 20;
  total: number;
  total_pages: number;
  retention_days: number;
};
export type RequestLogExport = {
  exported_at: number | string;
  retention_days: number;
  logs: RequestLog[];
};
export type RelayKey = {
  id: string;
  name: string;
  group_id: string;
  enabled: boolean;
  profile_ids: string[];
  default_strategy_enabled: boolean;
  prefix: string;
  created_at: number;
  usage: Usage;
};
export type Settings = {
  security_path: string;
  turnstile_site_key: string;
  turnstile_enabled: boolean;
  max_retries: number;
  failure_threshold: number;
  cooldown_seconds: number;
  request_timeout_seconds: number;
  retention_days: number;
};
export type Release = {
  tag: string;
  version: string;
  url: string;
  published_at: string;
  notes: string;
};
export type UpdateJob = {
  id: string;
  status: string;
  version: string;
  started_at: number;
  finished_at: number | null;
  message: string;
  logs: string[];
};
export type UpdateStatus = {
  current_version: string;
  configured: boolean;
  latest: Release | null;
  update_available: boolean;
  job: UpdateJob | null;
  error?: string;
};
export type Profile = {
  id: string;
  name: string;
  model_family: string;
  models: string[];
  instruction: string;
  metadata?: {
    source_url: string;
    source_revision: string;
    variant: string;
    validation: string;
  };
};
export type ProfileTemplate = {
  id: string;
  name: string;
  source_url: string;
  source_revision: string;
  validation: string;
};
export type State = {
  groups: Group[];
  accounts: Account[];
  keys: RelayKey[];
  settings: Settings;
  profiles: Profile[];
  username: string;
};
export type Summary = Usage & {
  uncached_tokens: number;
  total_tokens: number;
  total_requests: number;
  success_rate: number;
  cache_hit_rate: number;
  avg_latency_ms: number;
  rpm: number;
  tpm: number;
  cyber_count: number;
};
export type Daily = Usage & {
  date: string;
  total_requests: number;
  success_count: number;
  cyber_count: number;
};
export type Recent = {
  id: string;
  created_at: number;
  model: string;
  account_name?: string;
  key_name?: string;
  status: number;
  latency_ms: number;
  input_tokens: number;
  output_tokens: number;
  cached_tokens: number;
  cyber_count?: number;
};
export type Stats = {
  summary: Summary;
  daily: Daily[];
  recent: Recent[];
  accounts: { total: number; healthy: number; cooling: number };
  keys: number;
};

export const zeroSummary: Summary = {
  input_tokens: 0,
  output_tokens: 0,
  cached_tokens: 0,
  uncached_tokens: 0,
  total_tokens: 0,
  total_requests: 0,
  success_rate: 0,
  cache_hit_rate: 0,
  avg_latency_ms: 0,
  rpm: 0,
  tpm: 0,
  cyber_count: 0,
};
export const emptyStats: Stats = {
  summary: zeroSummary,
  daily: [],
  recent: [],
  accounts: { total: 0, healthy: 0, cooling: 0 },
  keys: 0,
};
export const number = (value = 0) =>
  new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 }).format(value);
export const compact = (value = 0) =>
  new Intl.NumberFormat("en-US", {
    notation: "compact",
    maximumFractionDigits: 2,
  }).format(value);
export const compactCount = (value = 0) => {
  const magnitude = Math.abs(value);
  if (magnitude >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}m`;
  if (magnitude >= 1_000) return `${(value / 1_000).toFixed(2)}k`;
  return number(value);
};
export const tokenCount = (value = 0) => {
  const magnitude = Math.abs(value);
  const unit = ([
    [1e12, "T"],
    [1e9, "B"],
    [1e6, "M"],
    [1e3, "K"],
  ] as const).find(([threshold]) => magnitude >= threshold);
  return unit ? `${(value / unit[0]).toFixed(2)}${unit[1]}` : number(value);
};
export const percent = (value = 0) => `${value.toFixed(1)}%`;
export const time = (value: number) =>
  new Date(value * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
export const day = (date: Date) =>
  `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
export const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : "操作失败，请重试";

export async function copy(value: string) {
  try {
    await navigator.clipboard.writeText(value);
  } catch {
    const element = document.createElement("textarea");
    element.value = value;
    element.style.position = "fixed";
    element.style.opacity = "0";
    document.body.append(element);
    element.select();
    const successful = document.execCommand("copy");
    element.remove();
    if (!successful) throw new Error("复制失败，请手动选择并复制");
  }
}
