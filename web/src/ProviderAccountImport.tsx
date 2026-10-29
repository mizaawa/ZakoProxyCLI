import { useEffect, useId, useRef, useState } from "react";
import type { FormEvent, KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { AlertCircle, Check, ChevronDown, KeyRound, Link, LoaderCircle, Plus, RefreshCw, Server } from "lucide-react";
import { api, errorMessage, time } from "./api";
import type { AccountProviderBinding, ProviderAccountKey, ProviderAccountKeys, ProviderBinding, ProviderKeyCreated, ProviderKeyGroup, ProviderKeyGroups, ProviderSummary } from "./api";
import { Badge, Button, ErrorBox, Modal } from "./components";
import "./account-import.css";

export type ProviderAccountSelection = { binding: ProviderBinding; endpoint: string; accountName: string; replaceFields: boolean };
type PickerOption = { id: string; name: string; detail: string; badge: string; available: boolean; action?: boolean };

function ImportPicker({ options, value, label, placeholder, detail, kind, loading, disabled, onChange, onCreate }: {
  options: PickerOption[];
  value: string;
  label: string;
  placeholder: string;
  detail: string;
  kind: "provider" | "key" | "group";
  loading: boolean;
  disabled: boolean;
  onChange: (id: string) => void;
  onCreate?: () => void;
}) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const selected = options.find((option) => option.id === value);
  const entries: PickerOption[] = onCreate ? [{ id: "create-provider-key", name: "从服务商创建", detail: "选择上游分组，创建新的 API 密钥", badge: "", available: true, action: true }, ...options] : options;
  const unavailable = disabled || loading || entries.length === 0;
  const Icon = kind === "provider" ? Server : KeyRound;
  function positionMenu() {
    if (!trigger.current || !menu.current) return;
    const rect = trigger.current.getBoundingClientRect();
    const viewport = window.visualViewport;
    const top = viewport?.offsetTop ?? 0;
    const left = viewport?.offsetLeft ?? 0;
    const width = viewport?.width ?? window.innerWidth;
    const height = viewport?.height ?? window.innerHeight;
    const below = top + height - rect.bottom - 14;
    const above = rect.top - top - 14;
    const desired = Math.min(340, entries.length * 72 + 42);
    const upward = below < desired && above > below;
    const popupHeight = Math.min(desired, Math.max(80, upward ? above : below));
    const popupWidth = Math.min(Math.max(rect.width, 260), width - 24);
    menu.current.style.width = `${popupWidth}px`;
    menu.current.style.maxHeight = `${popupHeight}px`;
    menu.current.style.left = `${Math.max(left + 12, Math.min(rect.left, left + width - popupWidth - 12))}px`;
    menu.current.style.top = `${upward ? Math.max(top + 12, rect.top - popupHeight - 6) : rect.bottom + 6}px`;
  }
  function close() { menu.current?.hidePopover(); setOpen(false); }
  function show(index = entries.findIndex((option) => !option.action && option.id === value && option.available)) {
    if (unavailable) return;
    setActive(entries[index]?.available ? index : move(-1, 1)); positionMenu(); menu.current?.showPopover(); setOpen(true);
  }
  function choose(index: number) {
    const option = entries[index];
    if (!option?.available) return;
    close(); trigger.current?.focus();
    if (option.action) onCreate?.(); else onChange(option.id);
  }
  function move(index: number, direction: 1 | -1) {
    for (let step = 1; step <= entries.length; step += 1) {
      const next = ((index + direction * step) % entries.length + entries.length) % entries.length;
      if (entries[next].available) return next;
    }
    return -1;
  }
  function onKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (unavailable) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) {
        const selectedIndex = entries.findIndex((option) => !option.action && option.id === value && option.available);
        show(selectedIndex >= 0 ? selectedIndex : move(event.key === "ArrowDown" ? -1 : 0, event.key === "ArrowDown" ? 1 : -1));
      }
      else setActive((index) => move(index, event.key === "ArrowDown" ? 1 : -1));
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const index = event.key === "Home" ? move(entries.length - 1, 1) : move(0, -1);
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
  }, [open, entries.length]);
  useEffect(() => { if (open) menu.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" }); }, [active, open]);
  useEffect(() => { if (unavailable && open) close(); }, [unavailable, open]);
  return <div className="account-provider-picker">
    <button ref={trigger} id={`${id}-trigger`} type="button" className={`account-provider-trigger${open ? " is-open" : ""}${selected ? "" : " is-placeholder"}`} role="combobox" aria-label={label} aria-haspopup="listbox" aria-expanded={open} aria-controls={`${id}-list`} aria-activedescendant={open && active >= 0 ? `${id}-option-${active}` : undefined} disabled={unavailable} onClick={() => open ? close() : show()} onKeyDown={onKeyDown}>
      {loading ? <LoaderCircle size={18} className="spin" aria-hidden="true" /> : <Icon size={18} aria-hidden="true" />}<span><strong>{loading ? "正在读取…" : selected?.name || placeholder}</strong><small>{selected?.detail || detail}</small></span><ChevronDown size={16} aria-hidden="true" />
    </button>
    <div ref={menu} id={`${id}-list`} className={`account-provider-menu${kind === "key" ? " account-key-menu" : ""}`} popover="auto" role="listbox" aria-label={label} onToggle={(event) => setOpen(event.currentTarget.matches(":popover-open"))}>
      <div className="account-provider-menu-heading" role="presentation">{label}<span>{options.length} 个{kind === "provider" ? "站点" : kind === "group" ? "分组" : "密钥"}</span></div>
      {entries.map((option, index) => <div key={`${option.action ? "action" : "item"}-${option.id}`} id={`${id}-option-${index}`} data-index={index} role="option" aria-selected={!option.action && option.id === value} aria-disabled={!option.available} className={`account-provider-option${active === index ? " is-active" : ""}${option.available ? "" : " is-unavailable"}${option.action ? " account-create-key-option" : ""}`} onPointerMove={() => { if (option.available) setActive(index); }} onMouseDown={(event) => event.preventDefault()} onClick={() => choose(index)}>{option.action ? <Plus size={18} aria-hidden="true" /> : <Icon size={17} aria-hidden="true" />}<span><strong>{option.name}</strong><small>{option.detail}</small></span>{!option.action && (kind === "key" ? <Badge variant={option.available ? "green" : "neutral"}>{option.badge}</Badge> : <span className="account-provider-kind">{option.badge}</span>)}{!option.action && option.id === value && <Check size={16} aria-hidden="true" />}</div>)}
    </div>
  </div>;
}

function keyStatus(key: ProviderAccountKey) {
  if (key.available) return "可导入";
  if (["disabled", "inactive", "2"].includes(key.status)) return "已停用";
  if (["expired", "3"].includes(key.status)) return "已过期";
  if (["quota_exhausted", "exhausted", "4"].includes(key.status)) return "额度已用尽";
  return "不可用";
}

type CreatedKeySelection = ProviderKeyCreated & { previousKeyIds: string[]; groupName: string };

function CreateProviderKey({ provider, onClose, onCreated }: {
  provider: ProviderSummary;
  onClose: (refresh?: boolean) => void;
  onCreated: (result: CreatedKeySelection) => void;
}) {
  const [groups, setGroups] = useState<ProviderKeyGroup[]>([]);
  const [groupId, setGroupId] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const [creating, setCreating] = useState(false);
  const [uncertain, setUncertain] = useState(false);
  const requestLock = useRef(false);
  const mounted = useRef(false);
  const selectedGroup = groups.find((group) => group.id === groupId);
  const keyName = selectedGroup ? `【ZakoAI】${selectedGroup.name}` : "选择分组后生成";
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => {
    let canceled = false;
    setLoading(true); setError(""); setGroups([]); setGroupId("");
    void api<ProviderKeyGroups>(`/providers/${encodeURIComponent(provider.id)}/groups`).then((result) => {
      if (canceled) return;
      if (result.provider_id !== provider.id || !Array.isArray(result.groups)) throw new Error("返回的服务提供商分组不匹配，请重试");
      setGroups(result.groups);
    }).catch((failure) => { if (!canceled) setError(errorMessage(failure)); }).finally(() => { if (!canceled) setLoading(false); });
    return () => { canceled = true; };
  }, [provider.id, revision]);
  function close() { if (!requestLock.current) onClose(uncertain); }
  async function create(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); event.stopPropagation();
    if (requestLock.current || uncertain || !selectedGroup || loading || creating) return;
    requestLock.current = true; setCreating(true); setError("");
    let result: ProviderKeyCreated;
    let previousKeyIds: string[];
    let submitted = false;
    try {
      const before = await api<ProviderAccountKeys>(`/providers/${encodeURIComponent(provider.id)}/keys`);
      if (before.provider_id !== provider.id || !Array.isArray(before.keys)) throw new Error("无法确认创建前的密钥列表，请重试。");
      previousKeyIds = before.keys.map((key) => key.id);
      if (!mounted.current) return;
      submitted = true;
      result = await api<ProviderKeyCreated>(`/providers/${encodeURIComponent(provider.id)}/keys`, "POST", { group_id: selectedGroup.id });
      if (result.provider_id !== provider.id || result.created !== true || result.group_id !== selectedGroup.id || typeof result.name !== "string" || !result.name.startsWith("【ZakoAI】")) throw new Error("创建结果尚未确认。");
    } catch (failure) {
      if (!mounted.current) return;
      requestLock.current = false; setCreating(false);
      // A failed response may follow a successful upstream write. Require a list refresh before another attempt.
      setUncertain(submitted);
      setError(submitted ? `${errorMessage(failure)} 请关闭弹窗并刷新密钥列表，确认结果后再操作。` : `${errorMessage(failure)} 尚未创建密钥，请重试。`);
      return;
    }
    if (mounted.current) onCreated({ ...result, previousKeyIds, groupName: result.name.slice("【ZakoAI】".length) });
  }
  return createPortal(<Modal title="从服务商创建" subtitle={`${provider.name} · 创建上游 API 密钥`} onClose={close} busy={creating}>
    <form className="account-key-create-form" onSubmit={create}>
      <div className="modal-body">
        <div className="account-import-field">
          <div className="account-import-heading"><span>选择上游分组</span>{!creating && !uncertain && <Button type="button" className="small-button" busy={loading} onClick={() => setRevision((value) => value + 1)}>{!loading && <RefreshCw size={14} />}刷新分组</Button>}</div>
          <ImportPicker kind="group" options={groups.map((group) => ({ id: group.id, name: group.name, detail: `分组倍率 ${group.multiplier == null ? "—" : `${group.multiplier}×`}`, badge: group.multiplier == null ? "—" : `${group.multiplier}×`, available: true }))} value={groupId} label="选择上游分组" placeholder="请选择上游分组" detail="从此服务提供商的可用分组中选择" loading={loading} disabled={creating || uncertain} onChange={setGroupId} />
          {!loading && !error && !groups.length && <p className="account-import-note"><AlertCircle size={16} />此服务提供商暂未返回可用于创建密钥的分组。</p>}
        </div>
        <div className="account-key-name-preview"><KeyRound size={18} /><span>密钥名称<strong>{keyName}</strong></span></div>
        <p className="account-import-note">创建后会自动刷新密钥列表，并选中新创建的密钥。</p>
        <ErrorBox message={error} />
      </div>
      <div className="modal-footer"><Button type="button" onClick={close} disabled={creating}>取消</Button><Button type="submit" className="primary" busy={creating} disabled={loading || !selectedGroup || uncertain}>{!creating && <Plus size={16} />}创建</Button></div>
    </form>
  </Modal>, document.body);
}

export function ProviderAccountImport({ accountId, savedBinding, initialBinding, disabled, onSelect, onReadyChange, onLoadingChange }: {
  accountId?: string;
  savedBinding: ProviderBinding | null;
  initialBinding: ProviderBinding | null;
  disabled: boolean;
  onSelect: (selection: ProviderAccountSelection) => void;
  onReadyChange: (ready: boolean) => void;
  onLoadingChange: (loading: boolean) => void;
}) {
  const [providers, setProviders] = useState<ProviderSummary[]>([]);
  const [providerId, setProviderId] = useState(initialBinding?.provider_id || "");
  const [keyId, setKeyId] = useState(initialBinding?.key_id || "");
  const [explicitKeySelection, setExplicitKeySelection] = useState(false);
  const [providerLoading, setProviderLoading] = useState(true);
  const [providerError, setProviderError] = useState("");
  const [providerRevision, setProviderRevision] = useState(0);
  const [keyData, setKeyData] = useState<ProviderAccountKeys | null>(null);
  const [keysLoading, setKeysLoading] = useState(Boolean(providerId));
  const [keysError, setKeysError] = useState("");
  const [keyRevision, setKeyRevision] = useState(0);
  const [bindingData, setBindingData] = useState<AccountProviderBinding | null>(null);
  const [bindingLoading, setBindingLoading] = useState(Boolean(accountId));
  const [bindingError, setBindingError] = useState("");
  const [bindingRevisionDone, setBindingRevisionDone] = useState(accountId ? -1 : 0);
  const [creatingFor, setCreatingFor] = useState<ProviderSummary | null>(null);
  const [createdNotice, setCreatedNotice] = useState("");
  const pendingCreatedKey = useRef<CreatedKeySelection | null>(null);
  const selectedProvider = providers.find((provider) => provider.id === providerId);
  const providerUnavailable = selectedProvider?.status === "not_authenticated";
  const selectedKey = keyData?.keys.find((key) => key.id === keyId);
  const savedSelection = Boolean(accountId && providerId === savedBinding?.provider_id && keyId === savedBinding?.key_id);
  const providerChangeConfirmed = explicitKeySelection && bindingData?.provider_binding?.status === "provider_changed";
  const savedSelectionAvailable = !savedSelection || Boolean(!bindingError && (bindingData?.available || providerChangeConfirmed));
  const ready = Boolean(selectedProvider && !providerUnavailable && !creatingFor && selectedKey?.available && keyData?.endpoint && !providerLoading && !providerError && !keysLoading && !keysError && !bindingLoading && savedSelectionAvailable);

  useEffect(() => {
    let canceled = false;
    setProviderLoading(true); setProviderError("");
    void api<ProviderSummary[]>("/providers").then((result) => { if (!canceled) setProviders(result); }).catch((failure) => { if (!canceled) setProviderError(errorMessage(failure)); }).finally(() => { if (!canceled) setProviderLoading(false); });
    return () => { canceled = true; };
  }, [providerRevision]);
  useEffect(() => {
    if (!accountId) { setBindingLoading(false); return; }
    let canceled = false;
    setBindingLoading(true); setBindingError("");
    void api<AccountProviderBinding>(`/accounts/${encodeURIComponent(accountId)}/provider-binding`).then((result) => {
      if (canceled) return;
      if (result.account_id !== accountId) throw new Error("账号绑定信息不匹配，请刷新重试");
      setBindingData(result);
    }).catch((failure) => { if (!canceled) setBindingError(errorMessage(failure)); }).finally(() => { if (!canceled) { setBindingLoading(false); setBindingRevisionDone(keyRevision); } });
    return () => { canceled = true; };
  }, [accountId, keyRevision]);
  useEffect(() => {
    if (!providerId) { setKeyData(null); setKeysLoading(false); return; }
    if (providerLoading) return;
    if (!selectedProvider || providerUnavailable) { setKeyData(null); setKeysLoading(false); setKeysError(""); return; }
    if (accountId && bindingRevisionDone !== keyRevision) return;
    let canceled = false;
    setKeysLoading(true); setKeysError(""); setKeyData(null);
    void api<ProviderAccountKeys>(`/providers/${encodeURIComponent(providerId)}/keys`).then((result) => {
      if (canceled) return;
      if (result.provider_id !== providerId || !Array.isArray(result.keys)) throw new Error("返回的服务提供商密钥信息不匹配，请重试");
      setKeyData(result);
    }).catch((failure) => { if (!canceled) setKeysError(errorMessage(failure)); }).finally(() => { if (!canceled) setKeysLoading(false); });
    return () => { canceled = true; };
  }, [providerId, keyRevision, accountId, bindingRevisionDone, providerLoading, selectedProvider, providerUnavailable]);
  useEffect(() => { onReadyChange(ready); }, [ready, onReadyChange]);
  useEffect(() => { onLoadingChange(providerLoading || keysLoading || bindingLoading || Boolean(creatingFor)); }, [providerLoading, keysLoading, bindingLoading, creatingFor, onLoadingChange]);
  useEffect(() => {
    const pending = pendingCreatedKey.current;
    if (!pending || pending.provider_id !== providerId || !keyData || disabled || providerLoading || providerError || keysLoading || keysError || bindingLoading || !selectedProvider) return;
    const previousIds = new Set(pending.previousKeyIds);
    const matches = keyData.keys.filter((key) => !previousIds.has(key.id) && key.name === pending.name && key.group_name === pending.groupName && key.available);
    if (matches.length === 1) {
      chooseKey(matches[0].id);
      setCreatedNotice(`已创建并自动选中密钥“${matches[0].name}”。`);
    } else if (matches.length > 1) {
      pendingCreatedKey.current = null;
      setCreatedNotice(`密钥“${pending.name}”已创建，但检测到多个新增同名密钥，请手动选择。`);
    } else {
      setCreatedNotice(`密钥“${pending.name}”已创建，上游列表尚未返回可用的新密钥。请点击“刷新密钥”，无需重复创建。`);
    }
  }, [keyData, providerId, disabled, providerLoading, providerError, keysLoading, keysError, bindingLoading, selectedProvider]);
  useEffect(() => {
    if (!ready || !selectedProvider || !selectedKey || !keyData) return;
    onSelect({ binding: { provider_id: providerId, key_id: selectedKey.id, key_name: selectedKey.name, group_name: selectedKey.group_name, group_multiplier: selectedKey.group_multiplier, status: selectedKey.status, last_synced_at: keyData.synced_at }, endpoint: keyData.endpoint, accountName: `${selectedProvider.name} · ${selectedKey.name || "上游密钥"}`.slice(0, 100), replaceFields: false });
  }, [ready, keyData, providerId, selectedKey, selectedProvider, onSelect]);

  function chooseProvider(id: string) {
    if (id === providerId || creatingFor || providers.find((provider) => provider.id === id)?.status === "not_authenticated") return;
    setCreatedNotice("");
    pendingCreatedKey.current = null;
    setExplicitKeySelection(false); setKeyId(""); setKeyData(null); setProviderId(id); onReadyChange(false);
  }
  function chooseKey(id: string) {
    const key = keyData?.keys.find((item) => item.id === id);
    if (!key?.available || !keyData || !selectedProvider || disabled || providerLoading || providerError || keysLoading || keysError || bindingLoading) return;
    pendingCreatedKey.current = null;
    setExplicitKeySelection(true); setKeyId(key.id);
    onSelect({ binding: { provider_id: providerId, key_id: key.id, key_name: key.name, group_name: key.group_name, group_multiplier: key.group_multiplier, status: key.status, last_synced_at: keyData.synced_at }, endpoint: keyData.endpoint, accountName: `${selectedProvider.name} · ${key.name || "上游密钥"}`.slice(0, 100), replaceFields: true });
  }
  function refreshKeys() {
    setExplicitKeySelection(false); setKeyData(null); setKeysLoading(Boolean(providerId)); onReadyChange(false);
    setKeyRevision((value) => value + 1);
  }
  function refreshProviders() {
    setProviderLoading(true); setProviderRevision((value) => value + 1);
    refreshKeys();
  }
  function openCreateKey() {
    if (!selectedProvider || providerUnavailable || disabled || keysLoading || bindingLoading || keysError || !keyData) return;
    pendingCreatedKey.current = null;
    setCreatingFor(selectedProvider);
  }
  function keyCreated(result: CreatedKeySelection) {
    setCreatingFor(null);
    if (result.provider_id !== providerId) return;
    pendingCreatedKey.current = result;
    setKeyId("");
    setCreatedNotice(`密钥“${result.name}”已在上游创建。若列表未更新，请点击“刷新密钥”，无需重复创建。`);
    refreshKeys();
  }
  const savedUnavailable = savedSelection && !bindingLoading && !bindingError && bindingData && !savedSelectionAvailable;
  const bindingMessage = savedUnavailable
    ? bindingData.provider_binding?.status === "provider_missing" ? "原服务提供商已删除，请重新选择站点与密钥。"
      : bindingData.provider_binding?.status === "provider_changed" ? "服务提供商的站点或认证身份已变更，请重新选择当前站点的 API 密钥，确认后保存。"
      : bindingData.provider_binding?.status === "missing" ? "原绑定密钥已被删除，请重新选择可用密钥。"
        : "原绑定密钥已不可用，请重新选择可用密钥。"
    : "";
  return <section className="account-provider-import" aria-label="服务提供商账号来源">
    <div className="account-import-field">
      <div className="account-import-heading"><span>选择服务提供商</span><Button type="button" className="small-button" disabled={disabled || keysLoading || bindingLoading || Boolean(creatingFor)} busy={providerLoading} onClick={refreshProviders}>{!providerLoading && <RefreshCw size={14} />}刷新站点</Button></div>
      <ImportPicker kind="provider" options={providers.map((provider) => ({ id: provider.id, name: provider.name, detail: provider.status === "not_authenticated" ? `${provider.base_url} · 未登录，暂不可导入` : provider.base_url, badge: provider.official ? `官方 · ${provider.status === "connected" ? "已连接" : provider.status === "error" ? "同步失败" : provider.status === "unverified" ? "待同步" : "未登录"}` : provider.kind === "sub2api" ? "Sub2API" : "New API", available: provider.status !== "not_authenticated" }))} value={providerId} label="选择服务提供商" placeholder={providerId ? "原服务提供商不可用" : "选择服务提供商"} detail="选择已添加的站点" loading={providerLoading} disabled={disabled || keysLoading || bindingLoading || Boolean(creatingFor)} onChange={chooseProvider} />
      <ErrorBox message={providerError} />
      {!providerLoading && !providerError && !providers.length && <p className="account-import-note"><AlertCircle size={16} />请先在“服务提供商”中添加站点，然后刷新列表。</p>}
      {!providerLoading && !providerError && providerId && !selectedProvider && <p className="account-import-note is-error"><AlertCircle size={16} />原服务提供商已删除，请重新选择。</p>}
    </div>
    <div className="account-import-field">
      <div className="account-import-heading"><span>选择 API 密钥</span><Button type="button" className="small-button" disabled={disabled || providerLoading || !providerId || providerUnavailable || Boolean(creatingFor)} busy={keysLoading || bindingLoading} onClick={refreshKeys}>{!keysLoading && !bindingLoading && <RefreshCw size={14} />}刷新密钥</Button></div>
      <ImportPicker kind="key" options={(keyData?.keys || []).map((key) => ({ id: key.id, name: key.name || "未命名密钥", detail: `${key.group_name || "站点未返回分组"} · 倍率 ${key.group_multiplier == null ? "—" : `${key.group_multiplier}×`}`, badge: keyStatus(key), available: key.available }))} value={keyId} label="选择 API 密钥" placeholder={keyId ? "原绑定密钥不可用" : "选择 API 密钥"} detail={providerId ? "选择用于此账号的上游密钥" : "请先选择服务提供商"} loading={keysLoading || bindingLoading} disabled={disabled || providerLoading || !selectedProvider || providerUnavailable || Boolean(providerError) || Boolean(creatingFor)} onChange={chooseKey} onCreate={keyData && !keysError ? openCreateKey : undefined} />
      {createdNotice && <p className="account-import-note account-key-created" role="status"><Check size={16} />{createdNotice}</p>}
      <ErrorBox message={keysError || (savedSelection ? bindingError : "")} />
      {bindingMessage && <p className="account-import-note is-error"><AlertCircle size={16} />{bindingMessage}</p>}
      {!keysLoading && !keysError && !bindingMessage && keyData && keyId && (!selectedKey || !selectedKey.available) && <p className="account-import-note is-error"><AlertCircle size={16} />{selectedKey ? `密钥“${selectedKey.name}”已不可用` : "原绑定密钥已被删除"}，请选择可用密钥。</p>}
      {!keysLoading && !keysError && keyData && !keyData.keys.length && <p className="account-import-note">此站点尚无 API 密钥，可以展开列表，选择“从服务商创建”。</p>}
      {ready && keyData && <div className="account-import-endpoint"><Link size={14} /><span>绑定端点<code>{keyData.endpoint}</code></span></div>}
      {keyData?.synced_at && <p className="account-import-synced">最近刷新：{time(keyData.synced_at)}</p>}
    </div>
    {creatingFor && <CreateProviderKey key={creatingFor.id} provider={creatingFor} onClose={(refresh) => { setCreatingFor(null); if (refresh) refreshKeys(); }} onCreated={keyCreated} />}
  </section>;
}
