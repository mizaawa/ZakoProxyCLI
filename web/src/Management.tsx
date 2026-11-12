import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent } from "react";
import {
  Activity,
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  LoaderCircle,
  Check,
  Copy,
  Database,
  ExternalLink,
  FolderClosed,
  KeyRound,
  Link,
  ListFilter,
  Pencil,
  Plus,
  RefreshCw,
  Server,
  Trash2,
} from "lucide-react";
import { api, copy, errorMessage, number, time, tokenCount } from "./api";
import type { Account, Group, Profile, ProfileTemplate, ProviderBinding, ProxyTestResult, RelayKey, State } from "./api";
import {
  Badge,
  Button,
  Confirm,
  CopyButton,
  Empty,
  ErrorBox,
  Field,
  IconButton,
  Modal,
  ModalForm,
  SearchInput,
  Select,
  Toggle,
} from "./components";
import type { Notice } from "./components";
import { GroupSelect } from "./GroupSelect";
import { ProviderAccountImport } from "./ProviderAccountImport";
import type { ProviderAccountSelection } from "./ProviderAccountImport";

export type PageProps = {
  state: State;
  reload: () => Promise<void>;
  notify: Notice;
};

const modelNameOrder = new Intl.Collator("en", {
  numeric: true,
  sensitivity: "base",
});

function DeleteDialog({
  resource,
  item,
  onClose,
  reload,
  notify,
}: {
  resource: "groups" | "accounts" | "keys";
  item: { id: string; name: string };
  onClose: () => void;
  reload: () => Promise<void>;
  notify: Notice;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const label = { groups: "分组", accounts: "账号", keys: "密钥" }[resource];
  async function remove() {
    setBusy(true);
    setError("");
    try {
      await api(`/${resource}/${item.id}`, "DELETE");
      await reload();
      onClose();
      notify(`${label}已删除`);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Confirm
      title={`删除${label}`}
      message={`确认删除“${item.name}”？此操作不可撤销。${resource === "keys" ? "使用此密钥的应用将立即失去访问权限。" : resource === "accounts" ? "此账号将停止参与请求调度。" : "已被引用的分组需要先解除关联。"}`}
      busy={busy}
      error={error}
      onClose={onClose}
      onConfirm={() => void remove()}
    />
  );
}

function PageHeading({
  eyebrow,
  title,
  count,
  children,
}: {
  eyebrow: string;
  title: string;
  count: number;
  children: React.ReactNode;
}) {
  return (
    <div className="page-heading">
      <div>
        <div className="eyebrow">{eyebrow}</div>
        <h1>
          {title}
          <span className="title-count">{count}</span>
        </h1>
      </div>
      {children}
    </div>
  );
}

function TableFooter({
  count,
  total,
  noun,
}: {
  count: number;
  total: number;
  noun: string;
}) {
  return (
    <div className="table-footer">
      <span>
        {count === total
          ? `共 ${total} 个${noun}`
          : `筛选到 ${count} / ${total} 个${noun}`}
      </span>
      <span>所有更改即时生效</span>
    </div>
  );
}

function profileDisplayModels(profile: Profile) {
  // Keep both spellings in the API for exact model matching, while showing
  // the single public model ID on this profile card.
  return profile.id === "coldcoffee-deepseek-v4.1-flash-v1"
    ? ["deepseek-4.1-flash"]
    : profile.models;
}

function supportedModelsForGroup(groupId: string, accounts: Account[]) {
  const supported = new Set<string>();
  for (const account of accounts) {
    if (
      !account.enabled ||
      !account.group_ids.includes(groupId) ||
      (account.provider_binding &&
        (account.provider_binding.status !== "active" || !account.has_api_key))
    ) continue;
    for (const model of account.models) {
      if (model.trim()) supported.add(model);
    }
  }
  return supported;
}

function profilesSupportedByGroup(groupId: string, accounts: Account[], profiles: Profile[]) {
  const supported = supportedModelsForGroup(groupId, accounts);
  return profiles.filter((profile) => profile.models.some((model) => supported.has(model)));
}

export function GroupsPage({ state, reload, notify }: PageProps) {
  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<Group | "new" | null>(null);
  const [deleting, setDeleting] = useState<Group | null>(null);
  const filtered = state.groups.filter((group) =>
    `${group.name} ${group.description || ""}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  return (
    <>
      <PageHeading
        eyebrow="ROUTING GROUPS"
        title="分组管理"
        count={state.groups.length}
      >
        <Button className="primary" onClick={() => setEditing("new")}>
          <Plus size={17} />
          创建分组
        </Button>
      </PageHeading>
      <div className="list-summary">
        <div>
          <span className="summary-icon">
            <FolderClosed size={19} />
          </span>
          <span>
            分组<strong>{state.groups.length}</strong>
          </span>
        </div>
        <div>
          <span className="summary-icon">
            <Database size={19} />
          </span>
          <span>
            上游账号<strong>{state.accounts.length}</strong>
          </span>
        </div>
        <div>
          <span className="summary-icon">
            <KeyRound size={19} />
          </span>
          <span>
            密钥<strong>{state.keys.length}</strong>
          </span>
        </div>
      </div>
      <section className="data-section">
        <div className="list-toolbar">
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="搜索分组或简介…"
          />
          <span className="toolbar-meta">
            <ListFilter size={15} />
            全部分组
          </span>
        </div>
        {filtered.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>分组名称</th>
                  <th>简介</th>
                  <th>账号</th>
                  <th>密钥</th>
                  <th className="action-cell">操作</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((group) => {
                  return (
                    <tr key={group.id}>
                      <td>
                        <div className="entity-cell">
                          <span className="entity-icon">
                            <FolderClosed size={18} />
                          </span>
                          <div>
                            <strong>{group.name}</strong>
                            <span className="cell-secondary mono">
                              {group.id.slice(0, 8)}
                            </span>
                          </div>
                        </div>
                      </td>
                      <td>
                        {group.description?.trim() && (
                          <span className="group-description-preview">
                            {group.description}
                          </span>
                        )}
                      </td>
                      <td className="mono">
                        {
                          state.accounts.filter((account) =>
                            account.group_ids.includes(group.id),
                          ).length
                        }
                      </td>
                      <td className="mono">
                        {
                          state.keys.filter((key) => key.group_id === group.id)
                            .length
                        }
                      </td>
                      <td>
                        <div className="row-actions">
                          <IconButton
                            title="编辑分组"
                            onClick={() => setEditing(group)}
                          >
                            <Pencil size={16} />
                          </IconButton>
                          <IconButton
                            title="删除分组"
                            className="destructive"
                            onClick={() => setDeleting(group)}
                          >
                            <Trash2 size={16} />
                          </IconButton>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : (
          <Empty
            title={search ? "没有匹配的分组" : "还没有分组"}
            detail={search ? "尝试其他分组名称或简介" : "创建第一个分组"}
            action={
              !search && (
                <Button onClick={() => setEditing("new")}>
                  <Plus size={16} />
                  创建分组
                </Button>
              )
            }
          />
        )}
        <TableFooter
          count={filtered.length}
          total={state.groups.length}
          noun="分组"
        />
      </section>
      {editing && (
        <GroupForm
          group={editing === "new" ? undefined : editing}
          state={state}
          reload={reload}
          notify={notify}
          onClose={() => setEditing(null)}
        />
      )}
      {deleting && (
        <DeleteDialog
          resource="groups"
          item={deleting}
          onClose={() => setDeleting(null)}
          reload={reload}
          notify={notify}
        />
      )}
    </>
  );
}

function GroupForm({
  group,
  reload,
  notify,
  onClose,
}: PageProps & { group?: Group; onClose: () => void }) {
  const [name, setName] = useState(group?.name || "");
  const [description, setDescription] = useState(group?.description || "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api(
        group ? `/groups/${group.id}` : "/groups",
        group ? "PUT" : "POST",
        {
          name: name.trim(),
          description: description.trim(),
        },
      );
      await reload();
      onClose();
      notify(group ? "分组已更新" : "分组已创建");
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={group ? "编辑分组" : "创建分组"}
      subtitle="分组简介"
      onClose={onClose}
      busy={busy}
    >
      <ModalForm
        onSubmit={submit}
        onClose={onClose}
        busy={busy}
        error={error}
        submit={group ? "保存更改" : "创建分组"}
      >
        <Field label="分组名称">
          <input
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="例如：默认分组"
            required
            maxLength={100}
          />
        </Field>
        <Field label="简介" hint="可选；留空时不会显示">
          <textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            placeholder="填写此分组的用途或说明"
            rows={5}
            maxLength={2000}
            disabled={busy}
          />
        </Field>
      </ModalForm>
    </Modal>
  );
}

export function AccountsPage({ state, reload, notify }: PageProps) {
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState("all");
  const [editing, setEditing] = useState<Account | "new" | null>(null);
  const [deleting, setDeleting] = useState<Account | null>(null);
  const filtered = state.accounts.filter(
    (account) =>
      `${account.name} ${account.endpoint}`
        .toLowerCase()
        .includes(search.toLowerCase()) &&
      (filter === "all" ||
        (filter === "enabled" ? account.enabled : !account.enabled)),
  );
  const cooling = state.accounts.filter(
    (account) => (account.runtime?.cooldown_until || 0) > Date.now() / 1000,
  ).length;
  return (
    <>
      <PageHeading
        eyebrow="UPSTREAM ACCOUNTS"
        title="账号管理"
        count={state.accounts.length}
      >
        <Button className="primary" onClick={() => setEditing("new")}>
          <Plus size={17} />
          添加账号
        </Button>
      </PageHeading>
      <div className="list-summary">
        <div>
          <span className="summary-icon">
            <Database size={19} />
          </span>
          <span>
            全部账号<strong>{state.accounts.length}</strong>
          </span>
        </div>
        <div>
          <span className="summary-icon">
            <Check size={19} />
          </span>
          <span>
            已启用
            <strong>
              {state.accounts.filter((account) => account.enabled).length}
            </strong>
          </span>
        </div>
        <div>
          <span className="summary-icon">
            <RefreshCw size={18} />
          </span>
          <span>
            冷却中<strong>{cooling}</strong>
          </span>
        </div>
      </div>
      <section className="data-section">
        <div className="list-toolbar">
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="搜索账号或端点…"
          />
          <Select
            aria-label="账号状态筛选"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          >
            <option value="all">全部状态</option>
            <option value="enabled">已启用</option>
            <option value="disabled">已停用</option>
          </Select>
        </div>
        {filtered.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>账号</th>
                  <th>端点 / 协议</th>
                  <th>分组</th>
                  <th>优先级</th>
                  <th>RPM / 并发</th>
                  <th>状态</th>
                  <th className="action-cell">操作</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((account) => (
                  <tr key={account.id}>
                    <td>
                      <div className="entity-cell">
                        <span className="entity-icon">
                          <Database size={18} />
                        </span>
                        <div>
                          <strong>{account.name}</strong>
                          <span className="cell-secondary">
                            {account.models.length} 个模型
                          </span>
                        </div>
                      </div>
                    </td>
                    <td>
                      <span
                        className="endpoint-text mono"
                        title={account.endpoint}
                      >
                        {account.endpoint}
                      </span>
                      <span className="cell-secondary protocol-text">
                        {account.protocols.join(" + ")}
                      </span>
                    </td>
                    <td>
                      <div className="model-badges">
                        {account.group_ids.map((id) => (
                          <Badge key={id}>
                            {state.groups.find((group) => group.id === id)
                              ?.name || "未知分组"}
                          </Badge>
                        ))}
                        {!account.group_ids.length && (
                          <span className="muted">未分组</span>
                        )}
                      </div>
                    </td>
                    <td className="mono">P{account.priority}</td>
                    <td className="mono">
                      {account.rpm || "不限"}
                      <span className="muted"> / </span>
                      {account.concurrency || "不限"}
                    </td>
                    <td>
                      <Badge
                        variant={
                          !account.enabled
                            ? "neutral"
                            : account.provider_binding && account.provider_binding.status !== "active"
                              ? "red"
                            : (account.runtime?.cooldown_until || 0) >
                                Date.now() / 1000
                              ? "amber"
                              : "green"
                        }
                        dot
                      >
                        {!account.enabled
                          ? "已停用"
                          : account.provider_binding && account.provider_binding.status !== "active"
                            ? "绑定失效"
                          : (account.runtime?.cooldown_until || 0) >
                              Date.now() / 1000
                            ? "冷却中"
                            : "可用"}
                      </Badge>
                    </td>
                    <td>
                      <div className="row-actions">
                        <IconButton
                          title="编辑账号"
                          onClick={() => setEditing(account)}
                        >
                          <Pencil size={16} />
                        </IconButton>
                        <IconButton
                          title="删除账号"
                          className="destructive"
                          onClick={() => setDeleting(account)}
                        >
                          <Trash2 size={16} />
                        </IconButton>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <Empty
            title={
              search || filter !== "all" ? "没有匹配的账号" : "还没有上游账号"
            }
            detail={search ? "尝试其他账号名称或端点" : "添加第一个上游账号"}
            action={
              !search &&
              filter === "all" && (
                <Button onClick={() => setEditing("new")}>
                  <Plus size={16} />
                  添加账号
                </Button>
              )
            }
          />
        )}
        <TableFooter
          count={filtered.length}
          total={state.accounts.length}
          noun="账号"
        />
      </section>
      {editing && (
        <AccountForm
          account={editing === "new" ? undefined : editing}
          state={state}
          reload={reload}
          notify={notify}
          onClose={() => {
            const refreshBinding = editing !== "new" && Boolean(editing.provider_binding);
            setEditing(null);
            if (refreshBinding) void reload().catch((failure) => notify(errorMessage(failure), true));
          }}
        />
      )}
      {deleting && (
        <DeleteDialog
          resource="accounts"
          item={deleting}
          onClose={() => setDeleting(null)}
          reload={reload}
          notify={notify}
        />
      )}
    </>
  );
}

function AccountForm({
  account,
  state,
  reload,
  notify,
  onClose,
}: PageProps & { account?: Account; onClose: () => void }) {
  const [importSource, setImportSource] = useState<"api" | "provider">(account?.provider_binding ? "provider" : "api");
  const [providerBinding, setProviderBinding] = useState<ProviderBinding | null>(account?.provider_binding || null);
  const [providerReady, setProviderReady] = useState(false);
  const [providerLoading, setProviderLoading] = useState(false);
  const [name, setName] = useState(account?.name || "");
  const suggestedAccountName = useRef("");
  const [apiEndpoint, setApiEndpoint] = useState(account?.endpoint || "");
  const [providerEndpoint, setProviderEndpoint] = useState(account?.endpoint || "");
  const endpoint = importSource === "provider" ? providerEndpoint : apiEndpoint;
  const [secret, setSecret] = useState("");
  const [proxyUrl, setProxyUrl] = useState("");
  const [proxyChanged, setProxyChanged] = useState(false);
  const [testingProxy, setTestingProxy] = useState(false);
  const [proxyResult, setProxyResult] = useState<ProxyTestResult | null>(null);
  const [proxyError, setProxyError] = useState("");
  const proxyRevision = useRef(0);
  const [protocols, setProtocols] = useState(
    account?.protocols || ["responses", "compact", "chat", "anthropic"],
  );
  const [models, setModels] = useState<string[]>(account?.models || []);
  const [available, setAvailable] = useState<string[]>(account?.models || []);
  const [manualModel, setManualModel] = useState("");
  const [mappings, setMappings] = useState(
    Object.entries(account?.model_mappings || {}).map(([request, actual], id) => ({
      id,
      request,
      actual,
    })),
  );
  const nextMappingId = useRef(mappings.length);
  const [groups, setGroups] = useState<string[]>(account?.group_ids || []);
  const [priority, setPriority] = useState(account?.priority ?? 0);
  const [rpm, setRpm] = useState(account?.rpm || 0);
  const [concurrency, setConcurrency] = useState(account?.concurrency || 0);
  const [enabled, setEnabled] = useState(account?.enabled ?? true);
  const [busy, setBusy] = useState(false);
  const [discovering, setDiscovering] = useState(false);
  const [error, setError] = useState("");
  const canReuseApiKey = Boolean(account?.has_api_key && !account.provider_binding);
  const hasCredentials = importSource === "provider" ? providerReady && Boolean(providerBinding) : Boolean(secret.trim()) || canReuseApiKey;
  const credentialsInput = importSource === "provider"
    ? { provider_id: providerBinding?.provider_id, provider_key_id: providerBinding?.key_id }
    : { api_key: secret, provider_id: null, provider_key_id: null };
  const onProviderSelect = useCallback((selection: ProviderAccountSelection) => {
    setProviderBinding(selection.binding);
    setProviderEndpoint(selection.endpoint);
    if (selection.replaceFields) {
      const previousSuggestion = suggestedAccountName.current;
      setName((current) => !current.trim() || current === previousSuggestion ? selection.accountName : current);
      suggestedAccountName.current = selection.accountName;
    }
    setError("");
  }, []);
  function changeImportSource(source: "api" | "provider") {
    if (source === importSource) return;
    setImportSource(source); setProviderReady(false); setProviderLoading(false); setError("");
  }
  useEffect(() => {
    proxyRevision.current += 1;
    setProxyResult(null);
    setProxyError("");
  }, [proxyUrl, proxyChanged, endpoint, secret, protocols, importSource, providerBinding?.provider_id, providerBinding?.key_id]);
  const proxyInput = proxyChanged || !account ? { proxy_url: proxyUrl.trim() } : {};
  const hasProxy = !!proxyUrl.trim() || (!!account?.has_proxy && !proxyChanged);
  async function testProxy() {
    const revision = proxyRevision.current;
    setTestingProxy(true);
    setProxyResult(null);
    setProxyError("");
    try {
      const result = await api<ProxyTestResult>("/accounts/proxy-test", "POST", {
        endpoint: endpoint.trim(),
        ...credentialsInput,
        protocols,
        ...proxyInput,
        ...(account ? { account_id: account.id } : {}),
      });
      if (revision === proxyRevision.current) setProxyResult(result);
    } catch (failure) {
      if (revision === proxyRevision.current) setProxyError(errorMessage(failure));
    } finally {
      setTestingProxy(false);
    }
  }
  function selectModels(selected: string[]) {
    setModels(selected);
    const affected = mappings.filter(
      (mapping) => mapping.actual && !selected.includes(mapping.actual),
    );
    if (affected.length) {
      setMappings((current) =>
        current.map((mapping) =>
          selected.includes(mapping.actual) ? mapping : { ...mapping, actual: "" },
        ),
      );
      setError("模型映射的实际模型已取消勾选，请重新选择实际模型或删除该映射");
    }
  }
  async function discover() {
    setDiscovering(true);
    setError("");
    try {
      const result = await api<{ models: string[] }>(
        "/models/discover",
        "POST",
        {
          endpoint,
          ...credentialsInput,
          protocols,
          ...proxyInput,
          ...(account ? { account_id: account.id } : {}),
        },
      );
      setAvailable([...new Set([...result.models, ...models])]);
      notify(`已获取 ${result.models.length} 个模型`);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setDiscovering(false);
    }
  }
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (importSource === "provider" && (!providerReady || !providerBinding)) {
      setError(providerLoading ? "服务提供商信息正在读取，请稍候" : "请选择可用的服务提供商 API 密钥，或刷新密钥后重试");
      return;
    }
    if (importSource === "api" && !secret.trim() && !canReuseApiKey) {
      setError(account?.provider_binding ? "切换为 API 导入后，请输入新的 API Key" : "请填写 API Key");
      return;
    }
    if (!protocols.length) {
      setError("至少选择一种协议");
      return;
    }
    if (!groups.length) {
      setError("至少选择一个分组");
      return;
    }
    const aliases = new Set<string>();
    for (const mapping of mappings) {
      const request = mapping.request.trim();
      if (!request || !mapping.actual) {
        setError("请填写每条模型映射的请求模型，并选择实际模型");
        return;
      }
      if (!models.includes(mapping.actual)) {
        setError("模型映射的实际模型必须是已勾选的可用模型");
        return;
      }
      if (aliases.has(request)) {
        setError(`请求模型 ${request} 存在重复映射`);
        return;
      }
      aliases.add(request);
    }
    const modelMappings = Object.fromEntries(
      mappings.map((mapping) => [mapping.request.trim(), mapping.actual]),
    );
    setBusy(true);
    setError("");
    try {
      await api(
        account ? `/accounts/${account.id}` : "/accounts",
        account ? "PUT" : "POST",
        {
          name: name.trim(),
          endpoint: endpoint.trim(),
          ...credentialsInput,
          protocols,
          models,
          model_mappings: modelMappings,
          ...proxyInput,
          group_ids: groups,
          priority,
          rpm,
          concurrency,
          enabled,
        },
      );
      await reload();
      onClose();
      notify(account ? "账号已更新" : "账号已添加");
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={account ? "编辑账号" : "添加上游账号"}
      subtitle="连接与请求调度"
      onClose={onClose}
      busy={busy || discovering || testingProxy}
      wide
    >
      <ModalForm
        onSubmit={submit}
        onClose={onClose}
        busy={busy || discovering || testingProxy}
        error={error}
        submit={account ? "保存更改" : "添加账号"}
        submitDisabled={importSource === "provider" && (!providerReady || providerLoading)}
      >
        <div className="account-import-source" role="group" aria-label="账号导入方式">
          <button type="button" aria-pressed={importSource === "api"} disabled={busy || discovering || testingProxy} onClick={() => changeImportSource("api")}><KeyRound size={16} />从 API 导入</button>
          <button type="button" aria-pressed={importSource === "provider"} disabled={busy || discovering || testingProxy} onClick={() => changeImportSource("provider")}><Server size={16} />从服务商导入</button>
        </div>
        <Field label="账号名称">
          <input
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="例如：主线路"
            required
            maxLength={100}
          />
        </Field>
        {importSource === "provider" ? <ProviderAccountImport accountId={account?.provider_binding ? account.id : undefined} savedBinding={account?.provider_binding || null} initialBinding={providerBinding} disabled={busy || discovering || testingProxy} onSelect={onProviderSelect} onReadyChange={setProviderReady} onLoadingChange={setProviderLoading} /> : <>
        <Field label="上游端点">
          <input
            type="url"
            value={endpoint}
            onChange={(event) => setApiEndpoint(event.target.value)}
            placeholder="https://api.example.com/v1"
            required
            spellCheck={false}
          />
        </Field>
        <Field label="API Key" hint={canReuseApiKey ? "留空保留已有密钥" : account?.provider_binding ? "从服务商绑定切换后，请输入新的 API Key。" : undefined}>
          <input
            type="password"
            autoComplete="off"
            value={secret}
            onChange={(event) => setSecret(event.target.value)}
            required={!canReuseApiKey}
            placeholder={
              canReuseApiKey ? "已保存 · 输入新密钥可替换" : "sk-…"
            }
          />
        </Field>
        </>}
        <fieldset className="form-fieldset">
          <legend>支持协议</legend>
          <div className="protocol-options">
            {[
              { protocol: "responses", label: "Responses", path: "/v1/responses" },
              { protocol: "compact", label: "Compact", path: "/v1/responses/compact" },
              { protocol: "chat", label: "Chat Completions", path: "/v1/chat/completions" },
              { protocol: "anthropic", label: "Anthropic", path: "/v1/messages" },
            ].map(({ protocol, label, path }) => (
              <label className="protocol-option" key={protocol}>
                <input
                  type="checkbox"
                  checked={protocols.includes(protocol)}
                  onChange={(event) =>
                    setProtocols(
                      event.target.checked
                        ? [...protocols, protocol]
                        : protocols.filter((item) => item !== protocol),
                    )
                  }
                />
                <span>{label}</span>
                <code>{path}</code>
              </label>
            ))}
          </div>
        </fieldset>
        <div className="form-divider" />
        <div className="field">
          <div className="field-heading">
            <span className="field-label">
              可用模型<span>{models.length} 个已选</span>
            </span>
            <Button
              type="button"
              className="small-button"
              busy={discovering}
              disabled={busy || testingProxy || !endpoint || !hasCredentials}
              onClick={() => void discover()}
            >
              <RefreshCw size={14} />
              获取模型
            </Button>
          </div>
          {available.length > 0 && (
            <div className="model-select-tools">
              <button type="button" onClick={() => selectModels([...available])}>
                全选
              </button>
              <span>/</span>
              <button type="button" onClick={() => selectModels([])}>
                清空
              </button>
            </div>
          )}
          <div className="checkbox-list model-options">
            {available.length ? (
              available.map((model) => (
                <label className="check-option" key={model}>
                  <input
                    type="checkbox"
                    checked={models.includes(model)}
                    onChange={(event) =>
                      selectModels(
                        event.target.checked
                          ? [...models, model]
                          : models.filter((item) => item !== model),
                      )
                    }
                  />
                  <span className="mono">{model}</span>
                </label>
              ))
            ) : (
              <p className="field-hint">尚未获取模型</p>
            )}
          </div>
          <div className="inline-input">
            <input
              aria-label="手动添加模型"
              value={manualModel}
              onChange={(event) => setManualModel(event.target.value)}
              placeholder="手动添加模型 ID"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
              maxLength={200}
            />
            <IconButton
              title="添加模型"
              disabled={!manualModel.trim()}
              onClick={() => {
                const model = manualModel.trim();
                setAvailable([...new Set([...available, model])]);
                setModels([...new Set([...models, model])]);
                setManualModel("");
              }}
            >
              <Plus size={18} />
            </IconButton>
          </div>
          <fieldset
            className="form-fieldset model-mapping-fieldset"
            disabled={busy || discovering}
          >
            <legend>
              模型映射<span>{mappings.length} 条</span>
            </legend>
            {mappings.length > 0 && (
              <div className="model-mapping-list">
                {mappings.map((mapping, index) => (
                  <div className="model-mapping-row" key={mapping.id}>
                    <Field label="请求模型">
                      <input
                        aria-label={`请求模型 ${index + 1}`}
                        className="mono"
                        value={mapping.request}
                        onChange={(event) => {
                          const request = event.target.value;
                          setMappings((current) =>
                            current.map((row) =>
                              row.id === mapping.id ? { ...row, request } : row,
                            ),
                          );
                        }}
                        placeholder="例如：my-model"
                        autoCapitalize="none"
                        autoCorrect="off"
                        spellCheck={false}
                        maxLength={200}
                      />
                    </Field>
                    <ArrowRight
                      className="model-mapping-arrow"
                      size={16}
                      aria-hidden="true"
                    />
                    <Field label="实际模型">
                      <Select
                        aria-label={`实际模型 ${index + 1}`}
                        value={mapping.actual}
                        onChange={(event) => {
                          const actual = event.target.value;
                          setMappings((current) =>
                            current.map((row) =>
                              row.id === mapping.id ? { ...row, actual } : row,
                            ),
                          );
                        }}
                      >
                        <option value="" disabled>
                          选择实际模型
                        </option>
                        {models.map((model) => (
                          <option key={model} value={model}>
                            {model}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <IconButton
                      title={`删除模型映射 ${index + 1}`}
                      className="destructive model-mapping-remove"
                      onClick={() =>
                        setMappings((current) =>
                          current.filter((row) => row.id !== mapping.id),
                        )
                      }
                    >
                      <Trash2 size={16} />
                    </IconButton>
                  </div>
                ))}
              </div>
            )}
            <Button
              type="button"
              className="small-button model-mapping-add"
              disabled={!models.length}
              onClick={() => {
                const id = nextMappingId.current++;
                setMappings((current) => [
                  ...current,
                  { id, request: "", actual: "" },
                ]);
              }}
            >
              <Plus size={15} />
              添加映射
            </Button>
          </fieldset>
        </div>
        <div className="field account-proxy-field">
          <label className="field-label" htmlFor="account-proxy-url">代理设置（可选）</label>
          <div className="inline-input proxy-input">
            <input
              id="account-proxy-url"
              type="password"
              autoComplete="new-password"
              autoCapitalize="none"
              autoCorrect="off"
              spellCheck={false}
              maxLength={4096}
              value={proxyUrl}
              disabled={busy || discovering || testingProxy}
              onChange={(event) => { setProxyUrl(event.target.value); setProxyChanged(true); }}
              placeholder={account?.has_proxy && !proxyChanged ? "已保存代理 · 留空保留，输入新链接替换" : "socks5://用户名:密码@主机:端口"}
              aria-describedby="account-proxy-hint account-proxy-result"
            />
            <IconButton
              title="测试代理连接与延迟"
              disabled={busy || discovering || testingProxy || !hasProxy || !endpoint.trim() || !hasCredentials}
              aria-busy={testingProxy}
              onClick={() => void testProxy()}
            >
              {testingProxy ? <LoaderCircle className="spin" size={18} /> : <Activity size={18} />}
            </IconButton>
          </div>
          <div id="account-proxy-hint" className="field-hint proxy-hint">
            <span>{account?.has_proxy && !proxyChanged
              ? `当前：${account.proxy_display}；留空保留。`
              : proxyChanged && !proxyUrl.trim() ? "保存后不使用账号代理。" : "支持 socks5:// 和 socks5h://；保存后所有上游请求及模型获取均使用此代理。"}</span>
            {account?.has_proxy && (
              <button type="button" className="proxy-clear" disabled={busy || discovering || testingProxy} onClick={() => { setProxyUrl(""); setProxyChanged(!proxyChanged); }}>
                {proxyChanged ? "保留已保存代理" : "移除代理"}
              </button>
            )}
          </div>
          <div id="account-proxy-result" role="status" aria-live="polite">
            {testingProxy && <p className="proxy-result">正在通过代理连接上游…</p>}
            {proxyError && <p className="proxy-result failure"><AlertCircle size={16} />{proxyError}</p>}
            {proxyResult && <div className={`proxy-result ${proxyResult.ok ? "success" : proxyResult.connected ? "warning" : "failure"}`}>
              {proxyResult.ok ? <CheckCircle2 size={16} /> : <AlertCircle size={16} />}
              <span>
                {proxyResult.connected ? `代理已连通 · ${proxyResult.latency_ms} ms` : `代理连接失败 · ${proxyResult.latency_ms} ms`}
                {proxyResult.upstream_status !== null && ` · HTTP ${proxyResult.upstream_status}`}
                {proxyResult.error && <small>{proxyResult.error}</small>}
                {proxyResult.connected && !proxyResult.ok && <small>代理已连接到上游，请检查 API Key、端点和访问权限。</small>}
              </span>
            </div>}
          </div>
        </div>
        <fieldset className="form-fieldset">
          <legend>{importSource === "provider" ? "本地路由分组" : "所属分组"}</legend>
          {state.groups.length ? (
            <div className="group-options">
              {state.groups.map((group) => (
                <label className="check-option" key={group.id}>
                  <input
                    type="checkbox"
                    checked={groups.includes(group.id)}
                    onChange={(event) =>
                      setGroups(
                        event.target.checked
                          ? [...groups, group.id]
                          : groups.filter((id) => id !== group.id),
                      )
                    }
                  />
                  <FolderClosed size={15} />
                  <span>{group.name}</span>
                </label>
              ))}
            </div>
          ) : (
            <ErrorBox message="请先在分组管理中创建分组" />
          )}
        </fieldset>
        <div className="form-grid three">
          <Field label="优先级" hint="数值越小越优先">
            <input
              type="number"
              min={0}
              max={999999}
              value={priority}
              onChange={(event) => setPriority(Number(event.target.value))}
              required
            />
          </Field>
          <Field label="RPM 上限" hint="0 表示不限">
            <input
              type="number"
              min={0}
              max={1000000}
              value={rpm}
              onChange={(event) => setRpm(Number(event.target.value))}
              required
            />
          </Field>
          <Field label="并发上限" hint="0 表示不限">
            <input
              type="number"
              min={0}
              max={1000000}
              value={concurrency}
              onChange={(event) => setConcurrency(Number(event.target.value))}
              required
            />
          </Field>
        </div>
        <Toggle checked={enabled} onChange={setEnabled} label="启用此账号" />
      </ModalForm>
    </Modal>
  );
}

export function KeysPage({ state, reload, notify }: PageProps) {
  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<RelayKey | "new" | null>(null);
  const [deleting, setDeleting] = useState<RelayKey | null>(null);
  const [created, setCreated] = useState<{ name: string; key: string } | null>(
    null,
  );
  const [copying, setCopying] = useState("");
  const endpoint = `${window.location.origin}/v1`;
  const filtered = useMemo(
    () =>
      state.keys.filter((key) =>
        `${key.name} ${key.prefix}`
          .toLowerCase()
          .includes(search.toLowerCase()),
      ),
    [state.keys, search],
  );
  async function copyKey(id: string) {
    setCopying(id);
    try {
      const value = await api<{ key: string }>(`/keys/${id}/reveal`, "POST");
      await copy(value.key);
      notify("API Key 已复制");
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setCopying("");
    }
  }
  return (
    <>
      <PageHeading
        eyebrow="API ACCESS KEYS"
        title="密钥管理"
        count={state.keys.length}
      >
        <Button className="primary" onClick={() => setEditing("new")}>
          <Plus size={17} />
          创建密钥
        </Button>
      </PageHeading>
      <div className="endpoint-band">
        <div className="endpoint-icon">
          <Link size={19} />
        </div>
        <div>
          <span>API 端点</span>
          <code>{endpoint}</code>
        </div>
        <CopyButton value={endpoint} notify={notify} title="复制 API 端点" />
        <a
          href={`${window.location.origin}/health`}
          target="_blank"
          rel="noreferrer"
          className="icon-button"
          aria-label="查看服务状态"
          title="查看服务状态"
        >
          <ExternalLink size={16} />
        </a>
      </div>
      <section className="data-section">
        <div className="list-toolbar">
          <SearchInput
            value={search}
            onChange={setSearch}
            placeholder="搜索密钥名称…"
          />
          <span className="toolbar-meta">
            <KeyRound size={15} />
            {state.keys.filter((key) => key.enabled).length} 个已启用
          </span>
        </div>
        {filtered.length ? (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>密钥名称</th>
                  <th>API Key</th>
                  <th>分组</th>
                  <th className="numeric">Token 用量</th>
                  <th>状态</th>
                  <th>创建时间</th>
                  <th className="action-cell">操作</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((key) => (
                  <tr key={key.id}>
                    <td>
                      <div className="entity-cell">
                        <span className="entity-icon">
                          <KeyRound size={18} />
                        </span>
                        <strong>{key.name}</strong>
                      </div>
                    </td>
                    <td>
                      <div className="key-preview">
                        <code>{key.prefix.replace("...", "••••••••")}</code>
                        <IconButton
                          title="复制 API Key"
                          disabled={copying === key.id}
                          onClick={() => void copyKey(key.id)}
                        >
                          {copying === key.id ? (
                            <RefreshCw size={15} className="spin" />
                          ) : (
                            <Copy size={15} />
                          )}
                        </IconButton>
                      </div>
                    </td>
                    <td>
                      <Badge>
                        {state.groups.find((group) => group.id === key.group_id)
                          ?.name || "未知分组"}
                      </Badge>
                    </td>
                    <td className="numeric">
                      <strong
                        className="mono"
                        title={number(
                          (key.usage?.input_tokens || 0) +
                            (key.usage?.output_tokens || 0),
                        )}
                      >
                        {tokenCount(
                          (key.usage?.input_tokens || 0) +
                            (key.usage?.output_tokens || 0),
                        )}
                      </strong>
                      <span
                        className="cell-secondary"
                        title={number(key.usage?.cached_tokens || 0)}
                      >
                        缓存 {tokenCount(key.usage?.cached_tokens || 0)}
                      </span>
                    </td>
                    <td>
                      <Badge variant={key.enabled ? "green" : "neutral"} dot>
                        {key.enabled ? "已启用" : "已停用"}
                      </Badge>
                    </td>
                    <td className="mono muted">{time(key.created_at)}</td>
                    <td>
                      <div className="row-actions">
                        <IconButton
                          title="编辑密钥"
                          onClick={() => setEditing(key)}
                        >
                          <Pencil size={16} />
                        </IconButton>
                        <IconButton
                          title="删除密钥"
                          className="destructive"
                          onClick={() => setDeleting(key)}
                        >
                          <Trash2 size={16} />
                        </IconButton>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <Empty
            title={search ? "没有匹配的密钥" : "还没有 API 密钥"}
            detail={search ? "尝试其他密钥名称" : "创建第一个访问密钥"}
            action={
              !search && (
                <Button onClick={() => setEditing("new")}>
                  <Plus size={16} />
                  创建密钥
                </Button>
              )
            }
          />
        )}
        <TableFooter
          count={filtered.length}
          total={state.keys.length}
          noun="密钥"
        />
      </section>
      {editing && (
        <KeyForm
          relayKey={editing === "new" ? undefined : editing}
          state={state}
          reload={reload}
          notify={notify}
          onClose={() => setEditing(null)}
          onCreated={(name, key) => setCreated({ name, key })}
        />
      )}
      {deleting && (
        <DeleteDialog
          resource="keys"
          item={deleting}
          onClose={() => setDeleting(null)}
          reload={reload}
          notify={notify}
        />
      )}
      {created && (
        <Modal
          title="密钥已创建"
          subtitle={created.name}
          onClose={() => setCreated(null)}
        >
          <div className="modal-body">
            <div className="secret-success">
              <Check size={23} />
            </div>
            <Field label="API Key">
              <textarea
                className="secret-value mono"
                readOnly
                rows={3}
                value={created.key}
                onFocus={(event) => event.target.select()}
              />
            </Field>
            <div className="secret-endpoint">
              <span>API 端点</span>
              <code>{endpoint}</code>
            </div>
          </div>
          <div className="modal-footer">
            <Button onClick={() => setCreated(null)}>完成</Button>
            <Button
              className="primary"
              onClick={() => {
                void copy(created.key)
                  .then(() => notify("API Key 已复制"))
                  .catch((failure) => notify(errorMessage(failure), true));
              }}
            >
              <Copy size={16} />
              复制密钥
            </Button>
          </div>
        </Modal>
      )}
    </>
  );
}

function KeyForm({
  relayKey,
  state,
  reload,
  notify,
  onClose,
  onCreated,
}: PageProps & {
  relayKey?: RelayKey;
  onClose: () => void;
  onCreated: (name: string, key: string) => void;
}) {
  const [name, setName] = useState(relayKey?.name || "");
  const [group, setGroup] = useState(
    relayKey?.group_id || state.groups[0]?.id || "",
  );
  const [enabled, setEnabled] = useState(relayKey?.enabled ?? true);
  const [profiles, setProfiles] = useState<string[]>(relayKey?.profile_ids || []);
  const [defaultStrategyEnabled, setDefaultStrategyEnabled] = useState(
    relayKey?.default_strategy_enabled ?? false,
  );
  const [profileSearch, setProfileSearch] = useState("");
  const [templates, setTemplates] = useState<ProfileTemplate[]>([]);
  const [templateError, setTemplateError] = useState("");
  const [templateLoading, setTemplateLoading] = useState(true);
  const [templateId, setTemplateId] = useState("");
  const [templateModel, setTemplateModel] = useState("");
  const [localProfiles, setLocalProfiles] = useState<Profile[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const availableProfiles = useMemo(() => {
    const merged = new Map(state.profiles.map((profile) => [profile.id, profile]));
    for (const profile of localProfiles) merged.set(profile.id, profile);
    return [...merged.values()];
  }, [state.profiles, localProfiles]);
  const actualModels = useMemo(
    () => [...supportedModelsForGroup(group, state.accounts)].sort(modelNameOrder.compare),
    [group, state.accounts],
  );
  useEffect(() => {
    let active = true;
    void api<ProfileTemplate[]>("/profile-templates")
      .then((items) => { if (active) setTemplates(items); })
      .catch((failure) => { if (active) setTemplateError(errorMessage(failure)); })
      .finally(() => { if (active) setTemplateLoading(false); });
    return () => { active = false; };
  }, []);
  const selectedGroup = state.groups.find((item) => item.id === group);
  const supportedProfiles = useMemo(
    () => profilesSupportedByGroup(group, state.accounts, availableProfiles),
    [group, state.accounts, availableProfiles],
  );
  const supportedProfileIds = useMemo(
    () => new Set(supportedProfiles.map((profile) => profile.id)),
    [supportedProfiles],
  );
  const selectedProfiles = profiles.filter((id) => supportedProfileIds.has(id));
  const visibleProfiles = useMemo(() => {
    const query = profileSearch.trim().toLowerCase();
    const distinct = new Map<string, Profile>();
    for (const profile of supportedProfiles) {
      const equivalent = profile.metadata
        ? JSON.stringify([profile.metadata.source_revision, profile.metadata.variant, [...profile.models].sort()])
        : profile.id;
      if (!distinct.has(equivalent) || profiles.includes(profile.id)) {
        distinct.set(equivalent, profile);
      }
    }
    return [...distinct.values()]
      .sort(
        (a, b) =>
          modelNameOrder.compare(
            profileDisplayModels(a).join(" · "),
            profileDisplayModels(b).join(" · "),
          ) ||
          Number(Boolean(a.metadata)) - Number(Boolean(b.metadata)) ||
          modelNameOrder.compare(a.id, b.id),
      )
      .filter((profile) =>
        profile.name.toLowerCase().includes(query) || profile.models.some((model) => model.toLowerCase().includes(query)),
      );
  }, [supportedProfiles, profileSearch, profiles]);

  useEffect(() => {
    setProfiles((current) => {
      const supported = current.filter((id) => supportedProfileIds.has(id));
      return supported.length === current.length ? current : supported;
    });
  }, [supportedProfileIds]);

  function selectGroup(groupId: string) {
    const supported = new Set(
      profilesSupportedByGroup(groupId, state.accounts, availableProfiles).map((profile) => profile.id),
    );
    setProfiles((current) => current.filter((id) => supported.has(id)));
    setGroup(groupId);
    setProfileSearch("");
    setTemplateModel("");
  }

  function applyTemplate() {
    const template = templates.find((item) => item.id === templateId);
    if (!template || !actualModels.includes(templateModel)) return;
    const encoded = [...new TextEncoder().encode(templateModel)]
      .map((byte) => byte.toString(16).padStart(2, "0")).join("");
    if (encoded.length > 1024) { setTemplateError("模型 ID 过长，无法绑定策略"); return; }
    const id = `coldcoffee@${template.id}@${encoded}`;
    const profile: Profile = {
      id, name: `${template.name} · ${templateModel}`, models: [templateModel],
      model_family: "custom", instruction: "",
      metadata: { source_url: template.source_url, source_revision: template.source_revision,
        variant: template.id, validation: template.validation },
    };
    const overlapping = new Set(availableProfiles.filter((item) => item.models.includes(templateModel)).map((item) => item.id));
    setLocalProfiles((current) => [...current.filter((item) => item.id !== id), profile]);
    setProfiles((current) => [...current.filter((item) => !overlapping.has(item) && item !== id), id]);
    setProfileSearch(templateModel);
    setTemplateError("");
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!state.groups.some((item) => item.id === group)) {
      setError("请选择有效的所属分组");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const result = await api<{ key?: string }>(
        relayKey ? `/keys/${relayKey.id}` : "/keys",
        relayKey ? "PUT" : "POST",
        {
          name: name.trim(),
          group_id: group,
          enabled,
          profile_ids: selectedProfiles,
          default_strategy_enabled: defaultStrategyEnabled,
        },
      );
      await reload();
      onClose();
      if (result.key) onCreated(name, result.key);
      else notify("密钥已更新");
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={relayKey ? "编辑密钥" : "创建 API 密钥"}
      onClose={onClose}
      busy={busy}
      wide
    >
      <ModalForm
        onSubmit={submit}
        onClose={onClose}
        busy={busy}
        error={error}
        submit={relayKey ? "保存更改" : "创建密钥"}
      >
        <Field label="密钥名称">
          <input
            autoFocus
            value={name}
            onChange={(event) => setName(event.target.value)}
            required
            maxLength={100}
            placeholder="例如：开发环境"
          />
        </Field>
        <div className="key-group-setting">
          <GroupSelect value={group} groups={state.groups} onChange={selectGroup} disabled={busy} />
          {selectedGroup?.description?.trim() && (
            <p className="group-description" aria-label="分组简介">
              {selectedGroup.description}
            </p>
          )}
        </div>
        {!state.groups.length && <ErrorBox message="请先创建分组" />}
        <fieldset className="form-fieldset profile-fieldset" disabled={busy || !selectedGroup}>
          <legend>
            破甲策略<span>{selectedProfiles.length} 个已选</span>
          </legend>
          <details className="template-picker">
            <summary>添加冷咖啡策略模板</summary>
            <div className="template-picker-fields">
              <Field label="实际模型">
                <Select aria-label="模板绑定的实际模型" value={templateModel} onChange={(event) => setTemplateModel(event.target.value)}>
                  <option value="">选择此分组的可用模型</option>
                  {actualModels.map((model) => <option key={model} value={model}>{model}</option>)}
                </Select>
              </Field>
              <Field label="策略模板">
                <Select aria-label="冷咖啡策略模板" value={templateId} onChange={(event) => setTemplateId(event.target.value)} disabled={templateLoading}>
                  <option value="">{templateLoading ? "正在加载模板…" : "选择策略模板"}</option>
                  {templates.map((template) => <option key={template.id} value={template.id}>{template.name}</option>)}
                </Select>
              </Field>
              <Button type="button" onClick={applyTemplate} disabled={!actualModels.includes(templateModel) || !templateId || templateLoading}>应用模板</Button>
            </div>
            {!actualModels.length && <p className="field-hint">请先在此分组的已启用账号中配置模型。</p>}
            <p className="field-hint">应用后替换该模型已选的策略，保存密钥后生效。</p>
            {templateError && <ErrorBox message={templateError} />}
          </details>
          <div
            className="profile-search"
            onKeyDown={(event) => {
              if (event.key === "Enter" && event.target instanceof HTMLInputElement)
                event.preventDefault();
            }}
          >
            <SearchInput
              value={profileSearch}
              onChange={setProfileSearch}
              placeholder="搜索破甲策略"
            />
            <span className="profile-results" role="status">
              显示 {visibleProfiles.length} / {supportedProfiles.length} 个策略
            </span>
          </div>
          <div
            className="profile-options"
            role="region"
            aria-label="破甲策略列表"
            tabIndex={0}
          >
            {visibleProfiles.map((item) => (
              <div className="profile-option" key={item.id}>
                <label className="profile-choice">
                  <input
                    type="checkbox"
                    checked={selectedProfiles.includes(item.id)}
                    onChange={(event) => {
                      const checked = event.target.checked;
                      setProfiles((current) => {
                        if (!checked) return current.filter((id) => id !== item.id);
                        const overlapping = new Set(
                          availableProfiles
                            .filter((profile) =>
                              profile.models.some((model) => item.models.includes(model)),
                            )
                            .map((profile) => profile.id),
                        );
                        return [
                          ...current.filter((id) => supportedProfileIds.has(id) && !overlapping.has(id)),
                          item.id,
                        ];
                      });
                    }}
                  />
                  <span className="profile-checkbox" aria-hidden="true">
                    <Check size={13} strokeWidth={2.5} />
                  </span>
                  <span className="profile-description">
                    <strong>{profileDisplayModels(item).join(" · ")}</strong>
                    <small>{item.name}</small>
                  </span>
                </label>
              </div>
            ))}
            {visibleProfiles.length === 0 && (
              <p className="profile-empty">
                {!selectedGroup
                  ? "请先选择所属分组"
                  : supportedProfiles.length === 0
                    ? "此分组暂无支持的破甲策略"
                    : "没有匹配的模型，试试其他关键词"}
              </p>
            )}
          </div>
          <div className="default-strategy-setting">
            <Toggle
              label="开启破甲默认策略"
              checked={defaultStrategyEnabled}
              onChange={setDefaultStrategyEnabled}
              describedBy="default-strategy-description"
            />
            <p id="default-strategy-description" className="field-hint">
              开启后，未勾选模型将采取默认提示词注入尝试进行破甲，如造成无法正常使用现象请关闭此选项。
            </p>
          </div>
        </fieldset>
        <Toggle checked={enabled} onChange={setEnabled} label="启用此密钥" />
      </ModalForm>
    </Modal>
  );
}
