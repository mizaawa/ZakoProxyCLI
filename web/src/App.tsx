import { Component, Fragment, Suspense, lazy, useCallback, useEffect, useRef, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import {
  Activity,
  ArrowRight,
  ChevronRight,
  CircleHelp,
  Database,
  Eye,
  EyeOff,
  FileClock,
  Fish,
  FolderClosed,
  KeyRound,
  LayoutDashboard,
  LoaderCircle,
  LogOut,
  Menu,
  MessageSquareText,
  RefreshCw,
  Settings2,
  Server,
  ShieldCheck,
  X,
} from "lucide-react";
import { api, errorMessage } from "./api";
import type { Settings } from "./api";
import { ResourceCache } from "./resourceCache";
import { Resources, Loading, keyListPath } from "./resources";
import { Button, ErrorBox, Field, IconButton, Toast } from "./components";
import { Dashboard } from "./Dashboard";
import { PromoBanner } from "./PromoBanner";
import { Mascot } from "./Mascot";
import { useUpdateCheck } from "./useUpdateCheck";
import { version } from "../package.json";

const loadManagement = () => import("./Management");
const pageModules = {
  playground: () => import("./Playground"),
  keys: loadManagement, groups: loadManagement, accounts: loadManagement,
  providers: () => import("./Providers"), logs: () => import("./RequestLogs"),
  settings: () => import("./Settings"),
};
const Playground = lazy(() => pageModules.playground().then((m) => ({ default: m.Playground })));
const GroupsPage = lazy(() => loadManagement().then((m) => ({ default: m.GroupsPage })));
const AccountsPage = lazy(() => loadManagement().then((m) => ({ default: m.AccountsPage })));
const KeysPage = lazy(() => loadManagement().then((m) => ({ default: m.KeysPage })));
const ProvidersPage = lazy(() => pageModules.providers().then((m) => ({ default: m.ProvidersPage })));
const RequestLogs = lazy(() => pageModules.logs().then((m) => ({ default: m.RequestLogs })));
const SettingsPage = lazy(() => pageModules.settings().then((m) => ({ default: m.SettingsPage })));

class PageBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <div className="page-load-error" role="alert"><ErrorBox message="页面未能加载，请刷新后重试。" /><Button onClick={() => window.location.reload()}>重新加载</Button></div> : this.props.children;
  }
}

declare global {
  interface Window {
    turnstile?: {
      render: (element: HTMLElement, options: { sitekey: string; callback: (token: string) => void; "expired-callback": () => void; "error-callback": () => void }) => string;
      reset: (widgetId?: string) => void;
      remove?: (widgetId: string) => void;
    };
  }
}

const navigation = [
  { id: "playground", label: "聊天", icon: MessageSquareText, section: "对话" },
  { id: "dashboard", label: "仪表盘", icon: LayoutDashboard, section: "数据和调用" },
  { id: "keys", label: "密钥管理", icon: KeyRound },
  { id: "groups", label: "分组管理", icon: FolderClosed, section: "构建和管理" },
  { id: "accounts", label: "账号管理", icon: Database },
  { id: "providers", label: "服务提供商", icon: Server },
  { id: "logs", label: "调用日志", icon: FileClock, section: "系统设置" },
  { id: "settings", label: "系统设置", icon: Settings2 },
] as const;
type Page = (typeof navigation)[number]["id"];
function warmCode(page: Page) {
  if (page !== "dashboard") void pageModules[page]().catch(() => {}); // Render has a retry boundary.
}
// Start reads alongside the page chunk, not in a waterfall after it arrives.
const firstReads: Record<Page, string[]> = {
  dashboard: [], playground: [keyListPath, "/groups"], keys: ["/keys", "/groups"],
  groups: ["/groups", "/accounts", keyListPath], accounts: ["/accounts", "/groups"],
  providers: ["/providers"], logs: ["/logs?page=1"], settings: ["/settings"],
};
function preparePage(page: Page, cache: ResourceCache) {
  warmCode(page);
  for (const path of firstReads[page]) void cache.get(path).load();
}

function Brand() {
  return (
    <div className="brand">
      <span className="brand-mark">
        <Fish size={23} strokeWidth={2.3} aria-hidden="true" />
      </span>
      <span className="brand-word">
        ZAKO<span>API RELAY</span>
      </span>
    </div>
  );
}

function Authentication({
  initialized,
  turnstileSiteKey,
  onSuccess,
}: {
  initialized: boolean;
  turnstileSiteKey: string;
  onSuccess: () => Promise<void>;
}) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [visible, setVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [turnstileToken, setTurnstileToken] = useState("");
  const turnstileRef = useRef<HTMLDivElement>(null);
  const turnstileWidget = useRef<string | null>(null);
  useEffect(() => {
    if (!initialized || !turnstileSiteKey || !turnstileRef.current) return;
    const render = () => {
      if (!window.turnstile || !turnstileRef.current || turnstileWidget.current) return;
      turnstileWidget.current = window.turnstile.render(turnstileRef.current, {
        sitekey: turnstileSiteKey,
        callback: setTurnstileToken,
        "expired-callback": () => setTurnstileToken(""),
        "error-callback": () => setTurnstileToken(""),
      });
    };
    if (window.turnstile) render();
    else {
      const script = document.createElement("script");
      script.src = "https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit";
      script.async = true;
      script.defer = true;
      script.addEventListener("load", render);
      document.head.appendChild(script);
      return () => script.removeEventListener("load", render);
    }
  }, [initialized, turnstileSiteKey]);
  async function submit(event: FormEvent) {
    event.preventDefault();
    setError("");
    if (!initialized && password !== confirm) {
      setError("两次输入的密码不一致");
      return;
    }
    if (initialized && turnstileSiteKey && !turnstileToken) {
      setError("请完成 Cloudflare Turnstile 安全校验");
      return;
    }
    setBusy(true);
    try {
      await api(initialized ? "/login" : "/setup", "POST", {
        username,
        password,
        ...(initialized && turnstileSiteKey
          ? { turnstile_token: turnstileToken }
          : {}),
      });
      await onSuccess();
    } catch (failure) {
      if (initialized && turnstileToken && window.turnstile) {
        window.turnstile.reset(turnstileWidget.current || undefined);
        setTurnstileToken("");
      }
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="auth-shell">
      <PromoBanner />
      <header>
        <Brand />
        <span className="auth-label">
          <ShieldCheck size={15} />
          管理控制台
        </span>
      </header>
      <main className="auth-main">
        <div className="auth-symbol">
          <Mascot size={64} />
        </div>
        <div className="auth-heading">
          <span className="eyebrow">ZAKO CONSOLE</span>
          <h1>{initialized ? "欢迎回来" : "创建管理员账号"}</h1>
          <p>
            {initialized
              ? "登录你的 API 工作空间"
              : "设置此站点的首个管理员账号"}
          </p>
        </div>
        <form onSubmit={submit} className="auth-form">
          <Field label="管理员账号">
            <input
              autoFocus
              autoComplete="username"
              autoCapitalize="none"
              spellCheck={false}
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              required
              maxLength={64}
              placeholder="输入账号"
            />
          </Field>
          <Field label="密码">
            <span className="password-input">
              <input
                type={visible ? "text" : "password"}
                autoComplete={initialized ? "current-password" : "new-password"}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                required
                placeholder="输入密码"
              />
              <IconButton
                title={visible ? "隐藏密码" : "显示密码"}
                onClick={() => setVisible(!visible)}
              >
                {visible ? <EyeOff size={17} /> : <Eye size={17} />}
              </IconButton>
            </span>
          </Field>
          {!initialized && (
            <Field label="确认密码">
              <input
                type="password"
                autoComplete="new-password"
                value={confirm}
                onChange={(event) => setConfirm(event.target.value)}
                required
                placeholder="再次输入密码"
              />
            </Field>
          )}
          {initialized && turnstileSiteKey && (
            <div ref={turnstileRef} className="turnstile-widget" />
          )}
          <ErrorBox message={error} />
          <Button className="primary auth-submit" busy={busy} type="submit">
            {initialized ? "登录工作空间" : "创建并进入工作空间"}
            <ArrowRight size={17} />
          </Button>
        </form>
        <div className="auth-note">
          <span className="status-dot" />
          独立部署 · 数据由本站存储
        </div>
      </main>
      <footer>
        <span>© {new Date().getFullYear()} ZAKO</span>
        <span>API Relay Console</span>
      </footer>
    </div>
  );
}

export default function App() {
  const [initialized, setInitialized] = useState<boolean | null>(null);
  const [turnstileSiteKey, setTurnstileSiteKey] = useState("");
  const [session, setSession] = useState<{ username: string; cache: ResourceCache } | null>(null);
  const sessionCache = useRef<ResourceCache | null>(null);
  const [page, setPage] = useState<Page>(() => {
    const stored = localStorage.getItem("zako-page");
    return navigation.some((item) => item.id === stored)
      ? (stored as Page)
      : "dashboard";
  });
  const currentPage = useRef(page);
  const bootRequest = useRef<Promise<void> | null>(null);
  const [mobileNav, setMobileNav] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [refreshing, setRefreshing] = useState(false);
  const [updateFocusRequest, setUpdateFocusRequest] = useState(0);
  const updateCheck = useUpdateCheck(!!session);
  const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(
    null,
  );
  const notify = useCallback(
    (text: string, isError = false) => {
      if (session && sessionCache.current === session.cache) setNotice({ text, error: isError });
    },
    [session],
  );
  function navigate(target: Page) {
    setMobileNav(false);
    if (target === currentPage.current || !session) return;
    currentPage.current = target;
    preparePage(target, session.cache);
    setPage(target);
  }
  const endSession = useCallback(() => {
    // A settings edit may have changed the next login challenge since bootstrap.
    const settings = sessionCache.current?.get<Settings>("/settings").getSnapshot().data;
    if (settings) setTurnstileSiteKey(settings.turnstile_enabled ? settings.turnstile_site_key : "");
    sessionCache.current?.dispose();
    sessionCache.current = null;
    setSession(null);
    setNotice(null);
    setMobileNav(false);
  }, []);
  const boot = useCallback(() => {
    if (bootRequest.current) return bootRequest.current;
    setLoading(true);
    setError("");
    const request = api<{
      authenticated: boolean; initialized: boolean; username: string; turnstile_site_key: string;
    }>("/bootstrap").then((info) => {
      setInitialized(info.initialized);
      setTurnstileSiteKey(info.turnstile_site_key || "");
      if (info.authenticated) {
        const cache = new ResourceCache((path, signal) => api(path, "GET", undefined, signal));
        sessionCache.current = cache;
        preparePage(currentPage.current, cache);
        setSession({ username: info.username, cache });
      }
    }).catch((failure) => setError(errorMessage(failure))).finally(() => {
      bootRequest.current = null;
      setLoading(false);
    });
    bootRequest.current = request;
    return request;
  }, []);
  useEffect(() => {
    void boot();
    window.addEventListener("session-expired", endSession);
    return () => window.removeEventListener("session-expired", endSession);
  }, [boot, endSession]);
  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 4200);
    return () => window.clearTimeout(timer);
  }, [notice]);
  useEffect(() => {
    localStorage.setItem("zako-page", page);
    setMobileNav(false);
    if (page !== "settings") setUpdateFocusRequest(0);
  }, [page]);
  useEffect(() => {
    if (!mobileNav) return;
    const listener = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMobileNav(false);
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [mobileNav]);
  useEffect(() => {
    document.documentElement.classList.toggle("nav-open-lock", mobileNav);
    document.body.classList.toggle("nav-open-lock", mobileNav);
    return () => {
      document.documentElement.classList.remove("nav-open-lock");
      document.body.classList.remove("nav-open-lock");
    };
  }, [mobileNav]);
  async function refresh() {
    setRefreshing(true);
    try {
      await session?.cache.refreshActive();
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setRefreshing(false);
    }
  }
  async function logout() {
    try {
      await api("/logout", "POST");
      endSession();
    } catch (failure) {
      notify(errorMessage(failure), true);
    }
  }
  if (loading)
    return (
      <div className="startup-shell">
        <PromoBanner />
        <div className="full-loading">
          <Brand />
          <LoaderCircle className="spin" size={23} />
          <span>正在连接工作空间</span>
        </div>
      </div>
    );
  if (error)
    return (
      <div className="startup-shell">
        <PromoBanner />
        <div className="full-loading">
          <Brand />
          <ErrorBox message={error} />
          <Button onClick={() => void boot()}>
            <RefreshCw size={16} />
            重新连接
          </Button>
        </div>
      </div>
    );
  if (!session)
    return (
      <>
        <Authentication initialized={!!initialized} turnstileSiteKey={turnstileSiteKey} onSuccess={boot} />
        {notice && <Toast {...notice} onClose={() => setNotice(null)} />}
      </>
    );
  const activePage = navigation.find((item) => item.id === page)!;
  return (
    <div className={`app-shell ${mobileNav ? "mobile-nav-open" : ""}`}>
      <button
        className={`nav-backdrop ${mobileNav ? "is-open" : ""}`}
        aria-label="关闭导航"
        onClick={() => setMobileNav(false)}
      />
      <aside className={`sidebar ${mobileNav ? "nav-open" : ""}`}>
        <div className="sidebar-brand">
          <Brand />
          <IconButton
            title="关闭导航"
            className="mobile-nav-close"
            onClick={() => setMobileNav(false)}
          >
            <X size={20} />
          </IconButton>
        </div>
        <div className="workspace-label">
          <span className="workspace-avatar">Z</span>
          <span>
            默认工作空间<small>PERSONAL WORKSPACE</small>
          </span>
          <ChevronRight size={15} />
        </div>
        <nav aria-label="主导航">
          {navigation.map((item) => (
            <Fragment key={item.id}>
              {"section" in item && <div className="nav-label">{item.section}</div>}
              <button
                className={`nav-item ${page === item.id ? "active" : ""}`}
                onPointerEnter={() => warmCode(item.id)}
                onFocus={() => warmCode(item.id)}
                onClick={() => navigate(item.id)}
                aria-current={page === item.id ? "page" : undefined}
              >
                <item.icon size={19} strokeWidth={1.7} />
                <span>{item.label}</span>
                {page === item.id && <span className="nav-indicator" />}
              </button>
            </Fragment>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="relay-status">
            <Activity size={16} />
            <span>Relay 运行中</span>
            <span className="status-dot" />
          </div>
          <div className="sidebar-version">
            <span>ZAKO / v{version}</span>
            <a
              href="https://github.com/mizaawa/zakoai-proxy-cli"
              target="_blank"
              rel="noreferrer"
              aria-label="GitHub"
              title="GitHub"
            >
              <CircleHelp size={16} />
            </a>
          </div>
        </div>
      </aside>
      <div className={`workspace${page === "playground" ? " playground-workspace" : ""}`}>
        <PromoBanner />
        <header className="topbar">
          <div className="breadcrumb">
            <IconButton
              title="打开导航"
              className="menu-button"
              onClick={() => setMobileNav(true)}
            >
              <Menu size={21} />
            </IconButton>
            <span className="breadcrumb-workspace">工作空间</span>
            <ChevronRight size={14} className="breadcrumb-chevron" />
            <span>{activePage.label}</span>
            {updateCheck.status?.update_available && (
              <button
                type="button"
                className="update-available-badge"
                title={`发现新版本 ${updateCheck.status.latest?.tag || ""}，前往系统更新`}
                aria-label="有新版本，前往系统设置更新"
                onClick={() => {
                  void navigate("settings");
                  setUpdateFocusRequest((value) => value + 1);
                }}
              >
                <span className="update-available-dot" aria-hidden="true" />
                有新版本
              </button>
            )}
          </div>
          <div className="topbar-actions">
            <span className="live-state">
              <span className="status-dot" />
              系统在线
            </span>
            <span className="topbar-divider" />
            <IconButton
              title="刷新数据"
              disabled={refreshing}
              onClick={() => void refresh()}
            >
              <RefreshCw size={17} className={refreshing ? "spin" : ""} />
            </IconButton>
            <span className="user-avatar" title={session.username}>
              {session.username?.slice(0, 1).toUpperCase() || "A"}
            </span>
            <IconButton title="退出登录" onClick={() => void logout()}>
              <LogOut size={17} />
            </IconButton>
          </div>
        </header>
        <main className={`main-content${page === "playground" ? " playground-main" : ""}`}>
          <div className="page-content">
            <Resources.Provider value={session.cache}>
              <PageBoundary key={page}>
                <Suspense fallback={<Loading label={activePage.label} />}>
                  {page === "dashboard" && <Dashboard notify={notify} />}
                  {page === "playground" && <Playground notify={notify} />}
                  {page === "groups" && <GroupsPage notify={notify} />}
                  {page === "accounts" && <AccountsPage notify={notify} />}
                  {page === "keys" && <KeysPage notify={notify} />}
                  {page === "providers" && <ProvidersPage notify={notify} />}
                  {page === "logs" && <RequestLogs notify={notify} />}
                  {page === "settings" && <SettingsPage username={session.username} notify={notify} updateCheck={updateCheck} updateFocusRequest={updateFocusRequest} />}
                </Suspense>
              </PageBoundary>
            </Resources.Provider>
          </div>
        </main>
        <footer className="workspace-footer">
          <span>ZAKO API RELAY</span>
          <span>
            <ShieldCheck size={12} />
            自托管工作空间
          </span>
        </footer>
      </div>
      {notice && <Toast {...notice} onClose={() => setNotice(null)} />}
    </div>
  );
}
