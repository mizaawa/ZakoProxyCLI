import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import { AlertCircle, ArrowUpRight, Check, CheckCircle2, ChevronDown, ChevronRight, CircleDollarSign, CreditCard, ExternalLink, Eye, EyeOff, KeyRound, LoaderCircle, Pencil, Plus, RefreshCw, Server, ShieldCheck, Trash2, Wallet } from "lucide-react";
import { api, errorMessage, time } from "./api";
import type { ProviderAuthRequirements } from "./api";
import { Badge, Button, Confirm, Empty, ErrorBox, Field, IconButton, Modal, ModalForm, SearchInput } from "./components";
import type { Notice } from "./components";
import "./providers.css";

type ProviderKind = "sub2api" | "newapi";
type AuthMode = "password" | "token";
type Provider = {
  id: string; name: string; kind: ProviderKind; base_url: string;
  auth_mode: AuthMode; username: string; user_id: string;
  has_password: boolean; has_access_token: boolean; has_refresh_token: boolean;
  token_type: string; price_multiplier: number; notes: string;
  balance: number | null; today_usage: number | null; total_usage: number | null;
  total_recharged: number | null; total_recharged_estimated: boolean;
  currency: "USD"; status: "connected" | "error" | "unverified" | "not_authenticated";
  last_error: string | null; last_synced_at: number | null;
  official?: boolean; readonly?: boolean; pinned?: boolean;
};
type TestResult = Pick<Provider, "balance" | "today_usage" | "total_usage" | "total_recharged" | "total_recharged_estimated" | "currency" | "user_id"> & { ok: boolean };
// Leave a short margin before the server's 600-second preflight cache expires.
const providerPreflightTtlMs = 590_000;
type FormValues = {
  name: string; kind: ProviderKind; base_url: string; auth_mode: AuthMode;
  username: string; password: string; access_token: string; refresh_token: string;
  user_id: string; token_type: string; price_multiplier: string; notes: string;
};
const labels: Record<ProviderKind, string> = { sub2api: "Sub2API", newapi: "New API" };
function money(value: number | null | undefined, currency = "USD") {
  if (value == null || !Number.isFinite(value)) return "—";
  return `${currency === "CNY" ? "¥" : "$"}${value.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: value !== 0 && Math.abs(value) < 0.01 ? 4 : 2 })}`;
}
function defaults(provider?: Provider): FormValues {
  return { name: provider?.name || "", kind: provider?.official ? "sub2api" : provider?.kind || "sub2api", base_url: provider?.base_url || "", auth_mode: provider?.auth_mode || "password", username: provider?.username || "", password: "", access_token: "", refresh_token: "", user_id: provider?.user_id || "", token_type: provider?.token_type || "Bearer", price_multiplier: String(provider?.price_multiplier ?? 1), notes: provider?.notes || "" };
}
function payload(values: FormValues, id?: string) {
  return { ...values, id, name: values.name.trim(), base_url: values.base_url.trim().replace(/\/+$/, ""), username: values.username.trim(), user_id: values.user_id.trim(), access_token: values.access_token.trim(), refresh_token: values.refresh_token.trim(), token_type: values.token_type.trim() || "Bearer", price_multiplier: Number(values.price_multiplier), notes: values.notes.trim() };
}

function normalizedSite(value: string) {
  try { return new URL(value.trim()).toString().replace(/\/+$/, ""); }
  catch { return value.trim().replace(/\/+$/, ""); }
}

let providerTurnstileScript: Promise<void> | null = null;
function loadProviderTurnstile() {
  if (window.turnstile) return Promise.resolve();
  if (providerTurnstileScript) return providerTurnstileScript;
  providerTurnstileScript = new Promise<void>((resolve, reject) => {
    const existing = document.querySelector<HTMLScriptElement>("script[data-provider-turnstile], script[src*='challenges.cloudflare.com/turnstile/v0/api.js']");
    const script = existing || document.createElement("script");
    const cleanup = () => {
      script.removeEventListener("load", loaded);
      script.removeEventListener("error", failed);
    };
    const fail = (message: string) => {
      cleanup();
      script.remove();
      providerTurnstileScript = null;
      reject(new Error(message));
    };
    const loaded = () => {
      if (window.turnstile) { cleanup(); resolve(); }
      else fail("Cloudflare Turnstile 脚本未能启动，请重试");
    };
    const failed = () => fail("Cloudflare Turnstile 脚本加载失败，请检查网络后重试");
    script.addEventListener("load", loaded, { once: true });
    script.addEventListener("error", failed, { once: true });
    if (!existing) {
      script.dataset.providerTurnstile = "true";
      script.src = "https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit";
      script.async = true;
      script.defer = true;
      document.head.appendChild(script);
    }
  });
  return providerTurnstileScript;
}

function hasCredentials(provider: Provider) {
  if (provider.auth_mode === "password") return Boolean(provider.username.trim() && provider.has_password);
  return Boolean(provider.has_access_token || (provider.kind === "sub2api" && provider.has_refresh_token));
}

const platformFilters = [
  { value: "all", label: "全部平台" },
  { value: "sub2api", label: "Sub2API" },
  { value: "newapi", label: "New API" },
] as const;

function ProviderPlatformFilter({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const selectedIndex = Math.max(0, platformFilters.findIndex((option) => option.value === value));

  function positionMenu() {
    if (!trigger.current || !menu.current) return;
    const rect = trigger.current.getBoundingClientRect();
    const viewport = window.visualViewport;
    const left = viewport?.offsetLeft ?? 0;
    const top = viewport?.offsetTop ?? 0;
    const width = viewport?.width ?? window.innerWidth;
    const height = viewport?.height ?? window.innerHeight;
    const below = top + height - rect.bottom - 12;
    const above = rect.top - top - 12;
    const desiredHeight = platformFilters.length * 44 + 52;
    const upward = below < desiredHeight && above > below;
    const popupHeight = Math.min(desiredHeight, Math.max(60, upward ? above : below));
    const popupWidth = Math.min(Math.max(rect.width, 180), width - 24);
    menu.current.style.width = `${popupWidth}px`;
    menu.current.style.maxHeight = `${popupHeight}px`;
    menu.current.style.left = `${Math.max(left + 12, Math.min(rect.left, left + width - popupWidth - 12))}px`;
    menu.current.style.top = `${upward ? Math.max(top + 12, rect.top - popupHeight - 6) : rect.bottom + 6}px`;
  }
  function close() { menu.current?.hidePopover(); setOpen(false); }
  function show(index = selectedIndex) { setActive(index); positionMenu(); menu.current?.showPopover(); setOpen(true); }
  function choose(index: number) { onChange(platformFilters[index].value); close(); trigger.current?.focus(); }
  function handleKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) show();
      else setActive((index) => (index + (event.key === "ArrowDown" ? 1 : -1) + platformFilters.length) % platformFilters.length);
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault(); const index = event.key === "Home" ? 0 : platformFilters.length - 1;
      if (open) setActive(index); else show(index);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault(); if (open) choose(active); else show();
    } else if (event.key === "Escape" && open) {
      event.preventDefault(); event.stopPropagation(); close();
    } else if (event.key === "Tab") close();
  }
  useEffect(() => {
    if (!open) return;
    const reposition = () => positionMenu();
    window.addEventListener("resize", reposition);
    window.addEventListener("scroll", reposition, true);
    window.visualViewport?.addEventListener("resize", reposition);
    window.visualViewport?.addEventListener("scroll", reposition);
    return () => {
      window.removeEventListener("resize", reposition);
      window.removeEventListener("scroll", reposition, true);
      window.visualViewport?.removeEventListener("resize", reposition);
      window.visualViewport?.removeEventListener("scroll", reposition);
    };
  }, [open]);
  useEffect(() => { if (open) menu.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" }); }, [active, open]);
  return <span className="provider-platform-filter">
    <button ref={trigger} id={`${id}-trigger`} type="button" role="combobox" aria-label="筛选服务提供商平台" aria-haspopup="listbox" aria-expanded={open} aria-controls={`${id}-list`} aria-activedescendant={open ? `${id}-option-${active}` : undefined} className={`provider-filter-trigger${open ? " is-open" : ""}`} onClick={() => open ? close() : show()} onKeyDown={handleKeyDown}>
      <span>{platformFilters[selectedIndex].label}</span><ChevronDown size={15} aria-hidden="true" />
    </button>
    <div ref={menu} id={`${id}-list`} popover="auto" role="listbox" aria-label="服务提供商平台" className="provider-filter-menu" onToggle={(event) => setOpen(event.currentTarget.matches(":popover-open"))}>
      <div className="provider-filter-heading" role="presentation">选择平台</div>
      {platformFilters.map((option, index) => <div key={option.value} id={`${id}-option-${index}`} role="option" aria-selected={option.value === value} data-index={index} className={`provider-filter-option${index === active ? " is-active" : ""}`} onPointerMove={() => setActive(index)} onMouseDown={(event) => event.preventDefault()} onClick={() => choose(index)}><span>{option.label}</span>{option.value === value && <Check size={16} aria-hidden="true" />}</div>)}
    </div>
  </span>;
}

function SecretInput({ value, onChange, placeholder, required = false, disabled = false }: { value: string; onChange: (value: string) => void; placeholder: string; required?: boolean; disabled?: boolean }) {
  const [visible, setVisible] = useState(false);
  return <span className="provider-secret"><input type={visible ? "text" : "password"} value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} required={required} disabled={disabled} autoComplete="new-password" spellCheck={false} /><IconButton title={visible ? "隐藏凭据" : "显示凭据"} onClick={() => setVisible(!visible)} disabled={disabled}>{visible ? <EyeOff size={16} /> : <Eye size={16} />}</IconButton></span>;
}

function ProviderForm({ provider, onClose, onSaved, notify }: { provider?: Provider; onClose: () => void; onSaved: () => Promise<void>; notify: Notice }) {
  const [values, setValues] = useState(() => defaults(provider));
  const [busy, setBusy] = useState<"test" | "save" | null>(null);
  const [error, setError] = useState("");
  const [tested, setTested] = useState<(TestResult & { testedAt: number }) | null>(null);
  const [turnstileToken, setTurnstileToken] = useState("");
  const [requirements, setRequirements] = useState<{ key: string; status: "loading" | "loaded" | "error"; required?: boolean; siteKey?: string; error?: string } | null>(null);
  const [requirementsRetry, setRequirementsRetry] = useState(0);
  const [widgetLoading, setWidgetLoading] = useState(false);
  const [widgetError, setWidgetError] = useState("");
  const [widgetRetry, setWidgetRetry] = useState(0);
  const turnstileRef = useRef<HTMLDivElement>(null);
  const turnstileWidget = useRef<string | null>(null);
  const widgetGeneration = useRef(0);
  const turnstileLabelId = useId();
  const authRequirementsKey = (() => {
    if (values.kind !== "sub2api" || values.auth_mode !== "password") return "";
    try {
      const url = new URL(values.base_url.trim());
      if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.search || url.hash) return "";
      return normalizedSite(values.base_url);
    } catch { return ""; }
  })();
  useEffect(() => {
    if (!authRequirementsKey) { setRequirements(null); return; }
    let active = true;
    setRequirements({ key: authRequirementsKey, status: "loading" });
    void api<ProviderAuthRequirements>("/providers/auth-requirements", "POST", { kind: values.kind, base_url: values.base_url.trim() })
      .then((result) => { if (active) setRequirements({ key: authRequirementsKey, status: "loaded", required: result.turnstile_required, siteKey: result.turnstile_site_key || "" }); })
      .catch((failure) => { if (active) setRequirements({ key: authRequirementsKey, status: "error", error: errorMessage(failure) }); });
    return () => { active = false; };
  }, [authRequirementsKey, requirementsRetry, values.base_url, values.kind]);
  const currentRequirements = authRequirementsKey && requirements?.key === authRequirementsKey ? requirements : null;
  const requirementsLoading = Boolean(authRequirementsKey && (!currentRequirements || currentRequirements.status === "loading"));
  const turnstileRequired = Boolean(currentRequirements?.status === "loaded" && currentRequirements.required);
  const turnstileSiteKey = turnstileRequired ? currentRequirements?.siteKey || "" : "";
  function resetChallenge() {
    setTurnstileToken("");
    if (turnstileWidget.current && window.turnstile) {
      try { window.turnstile.reset(turnstileWidget.current); } catch { /* Widget may have been removed during a form transition. */ }
    }
  }
  const update = <K extends keyof FormValues>(key: K, value: FormValues[K]) => {
    setValues((current) => ({ ...current, [key]: value }));
    setError("");
    if (["kind", "base_url", "auth_mode", "username", "password", "access_token", "refresh_token", "user_id", "token_type"].includes(key)) { setTested(null); resetChallenge(); }
  };
  useEffect(() => {
    const generation = ++widgetGeneration.current;
    setWidgetError("");
    setWidgetLoading(Boolean(turnstileSiteKey));
    if (!turnstileSiteKey || !turnstileRef.current) {
      if (turnstileWidget.current && window.turnstile && typeof window.turnstile.remove === "function") window.turnstile.remove(turnstileWidget.current);
      turnstileWidget.current = null;
      setTurnstileToken("");
      return;
    }
    setTurnstileToken("");
    void loadProviderTurnstile().then(() => {
      if (generation !== widgetGeneration.current || !turnstileRef.current || !window.turnstile) return;
      turnstileWidget.current = window.turnstile.render(turnstileRef.current, {
        sitekey: turnstileSiteKey,
        callback: (token) => { setTurnstileToken(token); setWidgetError(""); },
        "expired-callback": () => { resetChallenge(); setWidgetError("验证已过期，请重新完成安全验证。"); },
        "error-callback": () => { resetChallenge(); setWidgetError("验证遇到问题，请重新完成安全验证。"); },
      });
      setWidgetLoading(false);
    }).catch((failure: unknown) => {
      if (generation !== widgetGeneration.current) return;
      setWidgetLoading(false);
      setWidgetError(errorMessage(failure));
    });
    return () => {
      widgetGeneration.current += 1;
      if (turnstileWidget.current && window.turnstile && typeof window.turnstile.remove === "function") window.turnstile.remove(turnstileWidget.current);
      turnstileWidget.current = null;
    };
  }, [turnstileSiteKey, widgetRetry]);
  const identityChanges = provider ? [
    normalizedSite(provider.base_url) !== normalizedSite(values.base_url) ? "站点地址" : "",
    provider.kind !== values.kind ? "平台" : "",
    provider.auth_mode !== values.auth_mode ? "认证方式" : "",
    provider.username !== values.username.trim() ? "登录账号" : "",
    values.auth_mode === "token" && provider.user_id !== values.user_id.trim() ? "用户 ID" : "",
  ].filter(Boolean) : [];
  const canReuse = Boolean(provider && identityChanges.length === 0);
  const hasNewCredentials = values.auth_mode === "password" ? Boolean(values.password) : Boolean(values.access_token.trim() || (values.kind === "sub2api" && values.refresh_token.trim()));
  function validate() {
    if (!values.name.trim()) throw new Error("请填写站点名称");
    let url: URL;
    try { url = new URL(values.base_url.trim()); } catch { throw new Error("请输入完整的站点地址，例如 https://api.zayuapi.com"); }
    if (!["https:", "http:"].includes(url.protocol) || url.username || url.password) throw new Error("站点地址仅支持 HTTP / HTTPS，且不能包含账号或密码");
    if (!Number.isFinite(Number(values.price_multiplier)) || Number(values.price_multiplier) <= 0) throw new Error("充值倍率必须大于 0");
    if (values.auth_mode === "password") {
      if (!values.username.trim()) throw new Error(values.kind === "sub2api" ? "请填写登录邮箱" : "请填写登录账号");
      if (!values.password && !(canReuse && provider?.has_password)) throw new Error("请填写登录密码");
    } else {
      if (values.kind === "newapi" && values.access_token.trim() && !values.access_token.trim().startsWith("nap_") && !values.user_id.trim()) throw new Error("旧版 New API 令牌需要用户 ID；新版 nap_ 令牌可留空");
      if (!values.access_token.trim() && !(canReuse && provider?.has_access_token) && !(values.kind === "sub2api" && (values.refresh_token.trim() || (canReuse && provider?.has_refresh_token)))) throw new Error("请填写访问令牌");
    }
  }
  function validateRequirements() {
    if (!authRequirementsKey) return;
    if (!currentRequirements || currentRequirements.status === "loading") throw new Error("正在读取站点安全验证要求，请稍候");
    if (currentRequirements.status === "error") throw new Error(`读取站点安全验证要求失败：${currentRequirements.error || "请重试"}`);
    if (currentRequirements.required && !currentRequirements.siteKey) throw new Error("此站点要求 Cloudflare Turnstile 验证，但未返回站点密钥");
  }
  async function test() {
    setError(""); setBusy("test"); setTested(null);
    try {
      validate(); validateRequirements();
      if (turnstileRequired && !turnstileToken) throw new Error("请先完成 Cloudflare Turnstile 安全验证");
      const testedAt = Date.now();
      const result = await api<TestResult>("/providers/test", "POST", { ...payload(values, provider?.id), ...(turnstileRequired ? { turnstile_token: turnstileToken } : {}) });
      if (!result.ok) throw new Error("登录验证失败，请检查站点和凭据");
      setTested({ ...result, testedAt });
    }
    catch (failure) { setError(errorMessage(failure)); }
    finally { if (turnstileRequired) resetChallenge(); setBusy(null); }
  }
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setError(""); setBusy("save");
    let saved = false;
    try {
      validate();
      const credentialsChanged = !canReuse || hasNewCredentials || (values.auth_mode === "token" && (values.token_type.trim() || "Bearer") !== provider?.token_type);
      const testedFresh = tested && Date.now() - tested.testedAt < providerPreflightTtlMs;
      if ((!provider || credentialsChanged) && !testedFresh) {
        if (tested) setTested(null);
        validateRequirements();
        if (turnstileRequired && !turnstileToken) throw new Error(tested ? "登录验证已过期，请重新完成 Cloudflare Turnstile 安全验证后保存" : "请先完成 Cloudflare Turnstile 安全验证");
        try {
          const testedAt = Date.now();
          const result = await api<TestResult>("/providers/test", "POST", { ...payload(values, provider?.id), ...(turnstileRequired ? { turnstile_token: turnstileToken } : {}) });
          if (!result.ok) throw new Error("登录验证失败，请检查站点和凭据");
          setTested({ ...result, testedAt });
        } finally { if (turnstileRequired) resetChallenge(); }
      }
      const result = await api<Provider>(provider ? `/providers/${provider.id}` : "/providers", provider ? "PUT" : "POST", payload(values));
      saved = true;
      try { await api<Provider>(`/providers/${result.id}/refresh`, "POST"); }
      catch (failure) { notify(`服务提供商已保存，同步暂未完成：${errorMessage(failure)}`, true); await onSaved(); onClose(); return; }
      await onSaved(); onClose(); notify(provider ? "服务提供商已更新" : "服务提供商登录成功，已添加");
    } catch (failure) { setError(saved ? `已保存，但列表更新失败：${errorMessage(failure)}` : errorMessage(failure)); }
    finally { if (turnstileRequired && turnstileToken) resetChallenge(); setBusy(null); }
  }
  return <Modal title={provider ? "编辑服务提供商" : "添加服务提供商"} subtitle="连接站点，集中查看余额与使用情况" onClose={onClose} busy={Boolean(busy)} wide>
    <ModalForm onSubmit={submit} onClose={onClose} busy={Boolean(busy)} error={error} submit={provider ? "保存更改" : "登录并添加"}>
      <Field label="站点名称" hint={provider?.official ? "官方预置名称，不可修改" : undefined}><input autoFocus={!provider?.official} value={values.name} onChange={(event) => update("name", event.target.value)} placeholder="例如：杂鱼AI" maxLength={100} required disabled={Boolean(busy) || Boolean(provider?.official)} /></Field>
      <fieldset className="provider-platforms" disabled={Boolean(busy) || Boolean(provider?.official)}><legend>服务提供商平台{provider?.official ? "（官方预置，不可修改）" : ""}</legend>{(["sub2api", "newapi"] as const).map((kind) => <button type="button" key={kind} className={`provider-platform ${values.kind === kind ? "selected" : ""}`} aria-pressed={values.kind === kind} onClick={() => update("kind", kind)}><span className={`provider-type-icon ${kind}`}><Server size={19} /></span><span><strong>{labels[kind]}</strong><small>{kind === "sub2api" ? "邮箱登录 · Access Token" : "账号登录 · 系统访问令牌"}</small></span>{values.kind === kind ? <CheckCircle2 size={18} /> : <ChevronRight size={17} />}</button>)}</fieldset>
      <Field label="站点地址" hint={provider?.official ? "官方预置地址，不可修改" : "填写站点首页地址，不需要添加 /v1 或 /api。"}><input type="url" value={values.base_url} onChange={(event) => update("base_url", event.target.value)} placeholder="https://api.zayuapi.com" required disabled={Boolean(busy) || Boolean(provider?.official)} autoComplete="url" /></Field>
      <fieldset className="provider-auth-options" disabled={Boolean(busy)}><legend>认证方式</legend>{(["password", "token"] as const).map((mode) => <label key={mode} className={values.auth_mode === mode ? "selected" : ""}><input type="radio" name="provider-auth" value={mode} checked={values.auth_mode === mode} onChange={() => update("auth_mode", mode)} />{mode === "password" ? <ShieldCheck size={17} /> : <KeyRound size={17} />}<span>{mode === "password" ? "账号密码" : values.kind === "sub2api" ? "Sub2API 令牌" : "New API 用户 Key"}</span></label>)}</fieldset>
      {values.auth_mode === "password" ? <div className="form-grid provider-form-grid"><Field label={values.kind === "sub2api" ? "登录邮箱" : "登录账号"}><input value={values.username} onChange={(event) => update("username", event.target.value)} placeholder={values.kind === "sub2api" ? "name@example.com" : "请输入站点登录账号"} required disabled={Boolean(busy)} autoComplete="username" /></Field><Field label="登录密码" hint={canReuse && provider?.has_password ? "留空保留已保存的密码" : undefined}><SecretInput value={values.password} onChange={(value) => update("password", value)} placeholder={canReuse && provider?.has_password ? "已保存，留空保持不变" : "请输入登录密码"} required={!(canReuse && provider?.has_password)} disabled={Boolean(busy)} /></Field></div> : values.kind === "sub2api" ? <>
        <Field label="Access Token" hint="使用站点的登录访问令牌；也可仅填写 Refresh Token。"><SecretInput value={values.access_token} onChange={(value) => update("access_token", value)} placeholder={canReuse && provider?.has_access_token ? "已保存，留空保持不变" : "粘贴 Access Token"} disabled={Boolean(busy)} /></Field>
        <div className="form-grid provider-form-grid"><Field label="Refresh Token" optional hint={canReuse && provider?.has_refresh_token ? "留空保留已保存的刷新令牌" : "用于访问令牌过期后刷新登录"}><SecretInput value={values.refresh_token} onChange={(value) => update("refresh_token", value)} placeholder={canReuse && provider?.has_refresh_token ? "已保存，留空保持不变" : "粘贴 Refresh Token"} disabled={Boolean(busy)} /></Field><Field label="Token Type"><input value={values.token_type} onChange={(event) => update("token_type", event.target.value)} placeholder="Bearer" disabled={Boolean(busy)} /></Field></div>
      </> : <div className="form-grid provider-form-grid"><Field label="用户 ID" optional hint="新版 nap_ 令牌可留空；旧版系统令牌仍需填写。"><input value={values.user_id} onChange={(event) => update("user_id", event.target.value)} placeholder="旧版站点填写，例如：123" disabled={Boolean(busy)} inputMode="numeric" /></Field><Field label="系统访问令牌" hint="新版请使用 nap_ 令牌，需授权 profile:read、usage:read；导入密钥另需 api_key:read、api_key:reveal，创建密钥需 api_key:write。旧版令牌仍可使用。"><SecretInput value={values.access_token} onChange={(value) => update("access_token", value)} placeholder={canReuse && provider?.has_access_token ? "已保存，留空保持不变" : "粘贴系统访问令牌"} required={!(canReuse && provider?.has_access_token)} disabled={Boolean(busy)} /></Field></div>}
      <div className="form-divider" />
      <div className="form-grid provider-form-grid"><Field label="充值倍率" hint="人民币展示金额 = 美元金额 × 充值倍率。"><input type="number" value={values.price_multiplier} onChange={(event) => update("price_multiplier", event.target.value)} min="0.000001" step="any" required disabled={Boolean(busy)} /></Field><Field label="备注" optional><input value={values.notes} onChange={(event) => update("notes", event.target.value)} placeholder="用于区分站点或记录说明" maxLength={500} disabled={Boolean(busy)} /></Field></div>
      {identityChanges.length > 0 && <p className="provider-form-note"><AlertCircle size={15} /><span>已更换{identityChanges.join("、")}，{hasNewCredentials ? "保存时将验证新凭据。" : `原凭据不会复用，请填写新的${values.auth_mode === "password" ? "登录密码" : values.kind === "sub2api" ? "访问令牌或刷新令牌" : "系统访问令牌"}。`}</span></p>}
      <div className="provider-test-row"><Button type="button" onClick={() => void test()} busy={busy === "test"} disabled={Boolean(busy)}><ShieldCheck size={16} />测试登录</Button><span>验证登录后获取真实余额；凭据不会在列表中显示。</span></div>
      {authRequirementsKey && requirementsLoading && <div className="provider-turnstile-state" role="status"><LoaderCircle size={15} className="spin" />正在读取此站点的安全验证要求…</div>}
      {authRequirementsKey && currentRequirements?.status === "error" && <div className="provider-turnstile-state is-error" role="alert"><AlertCircle size={15} /><span>读取安全验证要求失败：{currentRequirements.error}</span><Button type="button" className="small-button" onClick={() => setRequirementsRetry((value) => value + 1)} disabled={Boolean(busy)}>重试</Button></div>}
      {turnstileRequired && !turnstileSiteKey && <div className="provider-turnstile-state is-error" role="alert"><AlertCircle size={15} /><span>此站点要求 Cloudflare Turnstile 验证，但未返回可用的站点密钥。</span></div>}
      {turnstileRequired && Boolean(turnstileSiteKey) && <div className="provider-turnstile" role="group" aria-labelledby={turnstileLabelId}><p id={turnstileLabelId}>此 Sub2API 站点要求完成 Cloudflare 安全验证后才能登录。</p><div ref={turnstileRef} className="provider-turnstile-widget" />{widgetLoading && <span className="provider-turnstile-hint" role="status"><LoaderCircle size={14} className="spin" />正在加载安全验证…</span>}{widgetError && <span className="provider-turnstile-hint is-error" role="alert">{widgetError}<Button type="button" className="small-button" onClick={() => setWidgetRetry((value) => value + 1)} disabled={Boolean(busy)}>重新加载</Button></span>}{!widgetLoading && !widgetError && !turnstileToken && <span className="provider-turnstile-hint">请在上方完成验证。</span>}{turnstileToken && <span className="provider-turnstile-hint is-success" role="status"><CheckCircle2 size={14} />验证完成</span>}</div>}
      {tested && <div className="provider-test-success" role="status"><CheckCircle2 size={18} /><div><strong>登录验证成功</strong><span>余额 {money(tested.balance)} · 今日消费 {money(tested.today_usage)}</span></div><Check size={16} /></div>}
    </ModalForm>
  </Modal>;
}

function ProviderCard({ provider, busy, onRefresh, onEdit, onDelete }: { provider: Provider; busy: boolean; onRefresh: () => void; onEdit: () => void; onDelete: () => void }) {
  const awaitingLogin = Boolean(provider.official && !hasCredentials(provider));
  const multiplier = provider.price_multiplier ?? 1;
  const metrics = [
    { label: "可用余额", value: provider.balance, icon: Wallet },
    { label: "今日消费", value: provider.today_usage, icon: CreditCard },
    { label: "累计充值", value: provider.total_recharged, icon: CircleDollarSign, estimated: provider.total_recharged_estimated },
  ];
  return <article className={`provider-card ${provider.status === "error" ? "has-error" : ""}`}>
    <div className="provider-card-heading">
      <span className={`provider-type-icon ${provider.kind}`}><Server size={22} /></span>
      <div className="provider-identity"><div><h2>{provider.name}</h2>{provider.official && <span className="provider-official"><ShieldCheck size={12} />官方</span>}<Badge>{labels[provider.kind]}</Badge></div><a className="provider-site" href={provider.base_url} target="_blank" rel="noopener noreferrer" title={`打开 ${provider.name} 站点`}>{provider.base_url}<ExternalLink size={12} /></a></div>
      <Badge variant={provider.status === "connected" ? "green" : provider.status === "error" && !awaitingLogin ? "red" : "neutral"} dot>{awaitingLogin ? "未登录" : provider.status === "connected" ? "已连接" : provider.status === "error" ? "同步失败" : "待同步"}</Badge>
      {(provider.official || !provider.readonly) && <div className="provider-card-actions"><IconButton title={`编辑 ${provider.name}`} onClick={onEdit} disabled={busy}><Pencil size={16} /></IconButton>{!provider.official && <IconButton title={`删除 ${provider.name}`} onClick={onDelete} disabled={busy} className="destructive"><Trash2 size={16} /></IconButton>}</div>}
    </div>
    <div className="provider-metrics">{metrics.map(({ label, value, icon: Icon, estimated }) => <div className="provider-metric" key={label}><span><Icon size={15} />{label}{estimated && <span className="provider-estimate" title="根据站点余额与累计消费估算">估算</span>}</span><strong title={value == null ? "站点尚未返回此数据" : `${value} USD × ${multiplier}`}>{estimated && value != null ? "≈ " : ""}{money(value == null ? null : value * multiplier, "CNY")}</strong><small>{money(value)} <span>USD</span></small></div>)}</div>
    {(provider.notes || provider.total_usage != null) && <div className="provider-card-details">{provider.notes && <p title={provider.notes}>{provider.notes}</p>}{provider.total_usage != null && <span>累计消费 <strong>{money(provider.total_usage)}</strong></span>}</div>}
    {awaitingLogin && <div className="provider-login-state" role="status"><span><KeyRound size={19} /></span><div><strong>尚未登录，统计暂不可用</strong><p>配置站点登录凭据后，即可读取余额与用量。</p><p><Button className="small-button" onClick={onEdit} disabled={busy}><KeyRound size={14} />配置登录</Button></p></div></div>}
    {provider.status === "error" && !awaitingLogin && <div className="provider-sync-error" role="status"><AlertCircle size={15} /><span>{provider.last_error || "同步失败，请检查站点和登录凭据后重试。"}{provider.last_synced_at ? " · 当前保留上次成功获取的数据" : ""}</span></div>}
    <div className={`provider-card-footer${awaitingLogin ? " is-official" : ""}`}><span>{awaitingLogin ? <><ShieldCheck size={13} />官方预置 · 固定置顶</> : busy ? <><LoaderCircle size={13} className="spin" />正在同步站点…</> : <><span className={`provider-sync-dot ${provider.status}`} />{provider.last_synced_at ? `更新于 ${time(provider.last_synced_at)}` : "尚未获取统计数据"}</>}</span><div><a className="provider-visit" href={provider.base_url} target="_blank" rel="noopener noreferrer">登录站点<ArrowUpRight size={14} /></a>{!awaitingLogin && <Button className="small-button" busy={busy} onClick={onRefresh}>{!busy && <RefreshCw size={13} />}刷新</Button>}</div></div>
  </article>;
}

function ProviderDelete({ provider, onClose, onDeleted, notify }: { provider: Provider; onClose: () => void; onDeleted: () => Promise<void>; notify: Notice }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function remove() {
    setBusy(true); setError("");
    try { await api(`/providers/${provider.id}`, "DELETE"); await onDeleted(); onClose(); notify("服务提供商已删除"); }
    catch (failure) { setError(errorMessage(failure)); }
    finally { setBusy(false); }
  }
  return <Confirm title="删除服务提供商" message={`确认删除“${provider.name}”？此操作会移除本地保存的站点与登录凭据，不会删除服务提供商站点中的账号或余额。`} busy={busy} error={error} onClose={onClose} onConfirm={() => void remove()} />;
}

export function ProvidersPage({ revision = 0, notify, onLoadingChange }: { revision?: number; notify: Notice; onLoadingChange?: (loading: boolean) => void }) {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState("all");
  const [editing, setEditing] = useState<Provider | "new" | null>(null);
  const [deleting, setDeleting] = useState<Provider | null>(null);
  const [refreshing, setRefreshing] = useState<string[]>([]);
  const [refreshingAll, setRefreshingAll] = useState(false);
  const requestId = useRef(0);
  const reload = useCallback(async () => {
    const request = ++requestId.current;
    setLoading(true); setError("");
    try { const result = await api<Provider[]>("/providers"); if (request === requestId.current) setProviders(result); }
    catch (failure) { if (request === requestId.current) setError(errorMessage(failure)); }
    finally { if (request === requestId.current) setLoading(false); }
  }, []);
  useEffect(() => { void reload(); return () => { requestId.current += 1; }; }, [revision, reload]);
  useEffect(() => { onLoadingChange?.(loading); }, [loading, onLoadingChange]);
  const filtered = useMemo(() => providers.filter((provider) => (filter === "all" || provider.kind === filter) && `${provider.name} ${provider.base_url} ${provider.username} ${provider.notes} ${labels[provider.kind]}`.toLowerCase().includes(search.trim().toLowerCase())).sort((a, b) => Number(Boolean(b.pinned)) - Number(Boolean(a.pinned))), [providers, search, filter]);
  const refreshable = providers.filter((provider) => !provider.readonly && (!provider.official || hasCredentials(provider)));
  const balances = providers.filter((provider) => provider.balance != null && Number.isFinite(provider.balance));
  const totalBalance = balances.length ? balances.reduce((sum, provider) => sum + (provider.balance || 0) * (provider.price_multiplier ?? 1), 0) : null;
  async function refresh(provider: Provider, silent = false) {
    setRefreshing((current) => [...current, provider.id]);
    try { const result = await api<Provider>(`/providers/${provider.id}/refresh`, "POST"); setProviders((current) => current.map((item) => item.id === provider.id ? result : item)); if (!silent) notify(`${provider.name} 已同步`); return true; }
    catch (failure) { if (!silent) notify(`${provider.name} 同步失败：${errorMessage(failure)}`, true); return false; }
    finally { setRefreshing((current) => current.filter((id) => id !== provider.id)); }
  }
  async function refreshOne(provider: Provider) { const ok = await refresh(provider); if (!ok) await reload(); }
  async function refreshAll() {
    setRefreshingAll(true);
    let success = 0;
    try { for (const provider of refreshable) if (await refresh(provider, true)) success += 1; await reload(); notify(success === refreshable.length ? "已配置的服务提供商已同步" : `已同步 ${success} / ${refreshable.length} 个服务提供商，失败站点保留上次数据`, success !== refreshable.length); }
    finally { setRefreshingAll(false); }
  }
  return <>
    <div className="page-heading provider-page-heading"><div><div className="eyebrow">PROVIDER MANAGEMENT</div><h1>服务提供商<span className="title-count">{providers.length}</span></h1></div><Button className="primary" onClick={() => setEditing("new")}><Plus size={17} />添加服务提供商</Button></div>
    <div className="list-summary provider-overview"><div><span className="summary-icon"><Server size={19} /></span><span>服务提供商<strong>{providers.length}</strong></span></div><div><span className="summary-icon"><ShieldCheck size={19} /></span><span>已连接<strong>{providers.filter((provider) => provider.status === "connected").length}<small> / {providers.length}</small></strong></span></div><div className="provider-balance-summary"><span className="summary-icon"><Wallet size={19} /></span><span>余额总计<strong>{money(totalBalance, "CNY")}</strong></span></div></div>
    <div className="provider-list-toolbar"><SearchInput value={search} onChange={setSearch} placeholder="搜索站点名称、地址或备注…" /><div className="provider-toolbar-options"><ProviderPlatformFilter value={filter} onChange={setFilter} /><Button onClick={() => void refreshAll()} busy={refreshingAll} disabled={loading || refreshable.length === 0 || refreshing.length > 0}>{!refreshingAll && <RefreshCw size={15} />}全部刷新</Button></div></div>
    {error && <div className="provider-load-error"><ErrorBox message={error} /><Button className="small-button" onClick={() => void reload()} busy={loading}>重新加载</Button></div>}
    {loading && providers.length === 0 ? <div className="provider-loading" role="status"><LoaderCircle size={22} className="spin" /><span>正在加载服务提供商…</span></div> : filtered.length ? <div className="provider-list" aria-busy={loading || refreshingAll}>{filtered.map((provider) => <ProviderCard key={provider.id} provider={provider} busy={refreshing.includes(provider.id)} onRefresh={() => void refreshOne(provider)} onEdit={() => setEditing(provider)} onDelete={() => setDeleting(provider)} />)}</div> : !error && <section className="data-section provider-empty"><Empty title={search || filter !== "all" ? "没有匹配的服务提供商" : "连接你的第一个服务提供商"} detail={search || filter !== "all" ? "试试其他关键词，或切换到全部平台。" : "支持 Sub2API 和 New API，登录后即可集中查看余额与消费。"} action={search || filter !== "all" ? <Button onClick={() => { setSearch(""); setFilter("all"); }}>清除筛选</Button> : <Button className="primary" onClick={() => setEditing("new")}><Plus size={16} />添加服务提供商</Button>} /></section>}
    {providers.length > 0 && <div className="provider-list-caption"><span>共 {providers.length} 个服务提供商{filtered.length !== providers.length ? ` · 显示 ${filtered.length} 个` : ""}</span><span>金额按各站点充值倍率换算</span></div>}
    {editing && <ProviderForm provider={editing === "new" ? undefined : editing} onClose={() => setEditing(null)} onSaved={reload} notify={notify} />}
    {deleting && <ProviderDelete provider={deleting} onClose={() => setDeleting(null)} onDeleted={reload} notify={notify} />}
  </>;
}
