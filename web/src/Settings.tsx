import { useState } from "react";
import type { FormEvent } from "react";
import {
  Check,
  Clock3,
  Fingerprint,
  KeyRound,
  LockKeyhole,
  PlugZap,
  Route,
  Save,
  ShieldCheck,
} from "lucide-react";
import { api, errorMessage } from "./api";
import type { Settings } from "./api";
import {
  Badge,
  Button,
  CopyButton,
  ErrorBox,
  Field,
  Toggle,
} from "./components";
import type { Notice } from "./components";
import { useResource, useResources, ResourceStatus } from "./resources";
import { SystemUpdates } from "./SystemUpdates";
import type { UpdateCheck } from "./useUpdateCheck";

type SettingsProps = { username: string; notify: Notice; updateCheck: UpdateCheck; updateFocusRequest: number };
export function SettingsPage(props: SettingsProps) {
  const query = useResource<Settings>("/settings");
  return <><ResourceStatus label="系统设置" queries={[query]} />{query.data && <SettingsForm {...props} settings={query.data} />}</>;
}
function SettingsForm({ settings, username, notify, updateCheck, updateFocusRequest }: SettingsProps & { settings: Settings }) {
  const cache = useResources();
  const [form, setForm] = useState<Settings>(settings);
  const [previousSettings, setPreviousSettings] = useState(settings);
  const [securityEnabled, setSecurityEnabled] = useState(
    !!settings.security_path,
  );
  const [turnstileSecret, setTurnstileSecret] = useState("");
  const [turnstileClear, setTurnstileClear] = useState(false);
  const [turnstileTestBusy, setTurnstileTestBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordBusy, setPasswordBusy] = useState(false);
  const [passwordError, setPasswordError] = useState("");
  const [currentVersion, setCurrentVersion] = useState("");
  const changes =
    JSON.stringify({
      ...form,
      security_path: securityEnabled
        ? form.security_path.replace(/^\/+|\/+$/g, "")
        : "",
      turnstile_secret_key: turnstileClear ? "" : turnstileSecret || undefined,
    }) !== JSON.stringify(settings);
  // Revalidation may finish after typing starts. Update only a pristine draft.
  if (previousSettings !== settings) {
    setPreviousSettings(settings);
    if (form === previousSettings && securityEnabled === !!previousSettings.security_path) {
      setForm(settings);
      setSecurityEnabled(!!settings.security_path);
    }
  }
  const entry = `${window.location.origin}/${settings.security_path || ""}`;
  function update<K extends keyof Settings>(key: K, value: Settings[K]) {
    setForm((previous) => ({ ...previous, [key]: value }));
  }
  async function save(event: FormEvent) {
    event.preventDefault();
    setError("");
    const securityPath = securityEnabled
      ? form.security_path.replace(/^\/+|\/+$/g, "")
      : "";
    if (securityEnabled && !securityPath) {
      setError("请输入安全入口路径");
      return;
    }
    setBusy(true);
    try {
      const saved = await api<Settings>("/settings", "PUT", {
        ...form,
        security_path: securityPath,
        turnstile_secret_key: turnstileClear
          ? ""
          : turnstileSecret || undefined,
      });
      window.history.replaceState(null, "", `/${securityPath}`);
      cache.get<Settings>("/settings").set(saved);
      if (saved.retention_days !== settings.retention_days) cache.invalidate("/logs", "/stats", "/keys");
      setForm(saved);
      setTurnstileSecret("");
      setTurnstileClear(false);
      notify("系统设置已保存");
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(false);
    }
  }
  async function changePassword(event: FormEvent) {
    event.preventDefault();
    setPasswordError("");
    if (newPassword !== confirmPassword) {
      setPasswordError("两次输入的新密码不一致");
      return;
    }
    setPasswordBusy(true);
    try {
      await api("/password", "POST", {
        current_password: currentPassword,
        new_password: newPassword,
        confirm_password: confirmPassword,
      });
      notify("密码已更新，请重新登录");
      window.dispatchEvent(new Event("session-expired"));
    } catch (failure) {
      setPasswordError(errorMessage(failure));
    } finally {
      setPasswordBusy(false);
    }
  }
  async function testTurnstile() {
    setTurnstileTestBusy(true);
    try {
      await api("/settings/turnstile-test", "POST", {
        turnstile_site_key: form.turnstile_site_key,
        ...(turnstileSecret ? { turnstile_secret_key: turnstileSecret } : {}),
      });
      notify("Cloudflare Turnstile 连接正常");
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setTurnstileTestBusy(false);
    }
  }
  return (
    <>
      <div className="page-heading">
        <div>
          <div className="eyebrow">WORKSPACE PREFERENCES</div>
          <h1>系统设置</h1>
          <p>安全、调度与数据</p>
        </div>
        <Button
          type="submit"
          form="settings-form"
          className="primary"
          disabled={!changes}
          busy={busy}
        >
          <Save size={16} />
          保存更改
        </Button>
      </div>
      <div className="settings-layout">
        <form id="settings-form" onSubmit={save}>
          <section className="settings-section">
            <div className="settings-section-heading">
              <span className="settings-icon">
                <ShieldCheck size={20} />
              </span>
              <div>
                <h2>安全入口</h2>
                <p>控制台访问地址</p>
              </div>
              <Badge
                variant={settings.security_path ? "green" : "neutral"}
                dot
              >
                {settings.security_path ? "已启用" : "未启用"}
              </Badge>
            </div>
            <div className="settings-section-body">
              <div className="setting-toggle-row">
                <span>
                  <strong>启用安全入口</strong>
                  <small>隐藏默认控制台地址</small>
                </span>
                <Toggle
                  label=""
                  checked={securityEnabled}
                  onChange={(enabled) => {
                    setSecurityEnabled(enabled);
                    if (enabled && !form.security_path)
                      update("security_path", "console");
                  }}
                />
              </div>
              <Field label="入口路径">
                <span
                  className={`prefixed-input ${!securityEnabled ? "disabled" : ""}`}
                >
                  <span>/</span>
                  <input
                    aria-label="入口路径"
                    value={form.security_path}
                    onChange={(event) =>
                      update("security_path", event.target.value)
                    }
                    disabled={!securityEnabled}
                    placeholder="console-secret"
                    maxLength={100}
                    pattern="[a-zA-Z0-9_\x2D]+"
                  />
                </span>
              </Field>
              <div className="current-entry">
                <LockKeyhole size={14} />
                <span>当前入口</span>
                <code>{entry}</code>
                <CopyButton
                  value={entry}
                  notify={notify}
                  title="复制控制台入口"
                />
              </div>
              <div className="setting-toggle-row">
                <span>
                  <strong>Cloudflare Turnstile</strong>
                  <small>
                    {settings.turnstile_enabled
                      ? "Cloudflare Turnstile 已启用"
                      : "登录时进行人机安全校验"}
                  </small>
                </span>
                <Badge
                  variant={settings.turnstile_enabled ? "green" : "neutral"}
                  dot
                >
                  {settings.turnstile_enabled
                    ? "Cloudflare Turnstile 已启用"
                    : "Cloudflare Turnstile 未启用"}
                </Badge>
              </div>
              <Field label="Cloudflare Turnstile 站点密钥" hint="用于显示登录安全校验组件">
                <input
                  value={form.turnstile_site_key}
                  onChange={(event) => update("turnstile_site_key", event.target.value)}
                  placeholder="0x4AAAAAAA..."
                  maxLength={256}
                />
              </Field>
              <Field
                label="Cloudflare Turnstile 密钥"
                hint={
                  settings.turnstile_enabled
                    ? "已保存密钥；输入新值可替换"
                    : "保存后登录将强制进行安全校验"
                }
              >
                <input
                  type="password"
                  value={turnstileSecret}
                  onChange={(event) => {
                    setTurnstileSecret(event.target.value);
                    setTurnstileClear(false);
                  }}
                  placeholder={settings.turnstile_enabled ? "已配置" : "输入 Secret Key"}
                  maxLength={512}
                />
              </Field>
              <div className="turnstile-actions">
                <Button type="button" onClick={() => void testTurnstile()} busy={turnstileTestBusy}>
                  <PlugZap size={15} />
                  测试连接
                </Button>
                {settings.turnstile_enabled && (
                  <Button
                    type="button"
                    onClick={() => {
                      setTurnstileClear(true);
                      setTurnstileSecret("");
                      notify("已标记清除 Turnstile 密钥，请保存更改");
                    }}
                  >
                    清除 Cloudflare Turnstile
                  </Button>
                )}
              </div>
            </div>
          </section>
          <section className="settings-section">
            <div className="settings-section-heading">
              <span className="settings-icon">
                <Route size={20} />
              </span>
              <div>
                <h2>账号调度</h2>
                <p>优先级、失败重试与动态冷却</p>
              </div>
            </div>
            <div className="settings-section-body">
              <div className="scheduling-order">
                <span className="order-node">
                  <span>01</span>优先账号
                </span>
                <i />
                <span className="order-node">
                  <span>02</span>失败重试
                </span>
                <i />
                <span className="order-node">
                  <span>03</span>次优先账号
                </span>
              </div>
              <div className="form-grid three">
                <Field label="最大重试次数" hint="单次请求的重试上限">
                  <span className="suffixed-input">
                    <input
                      type="number"
                      value={form.max_retries}
                      onChange={(event) =>
                        update("max_retries", Number(event.target.value))
                      }
                      min={0}
                      max={10}
                      required
                    />
                    <span>次</span>
                  </span>
                </Field>
                <Field label="连续失败阈值" hint="多账号池达到阈值后进入冷却">
                  <span className="suffixed-input">
                    <input
                      type="number"
                      value={form.failure_threshold}
                      onChange={(event) =>
                        update("failure_threshold", Number(event.target.value))
                      }
                      min={1}
                      max={100}
                      required
                    />
                    <span>次</span>
                  </span>
                </Field>
                <Field label="冷却时间" hint="到期恢复；全池冷却时提前释放">
                  <span className="suffixed-input">
                    <input
                      type="number"
                      value={form.cooldown_seconds}
                      onChange={(event) =>
                        update("cooldown_seconds", Number(event.target.value))
                      }
                      min={1}
                      max={86400}
                      required
                    />
                    <span>秒</span>
                  </span>
                </Field>
              </div>
              <div className="settings-note">
                <Check size={14} />
                <span>单账号不进入冷却；多账号全部冷却时整体释放并重新轮询。优先级数值越小越先尝试。</span>
              </div>
            </div>
          </section>
          <section className="settings-section">
            <div className="settings-section-heading">
              <span className="settings-icon">
                <Clock3 size={20} />
              </span>
              <div>
                <h2>请求与数据</h2>
                <p>超时与历史记录</p>
              </div>
            </div>
            <div className="settings-section-body">
              <div className="form-grid">
                <Field label="请求超时时间">
                  <span className="suffixed-input">
                    <input
                      type="number"
                      value={form.request_timeout_seconds}
                      onChange={(event) =>
                        update(
                          "request_timeout_seconds",
                          Number(event.target.value),
                        )
                      }
                      min={5}
                      max={3600}
                      required
                    />
                    <span>秒</span>
                  </span>
                </Field>
                <Field label="历史数据保留时间">
                  <span className="suffixed-input">
                    <input
                      type="number"
                      value={form.retention_days}
                      onChange={(event) =>
                        update("retention_days", Number(event.target.value))
                      }
                      min={1}
                      max={3650}
                      required
                    />
                    <span>天</span>
                  </span>
                </Field>
              </div>
            </div>
          </section>
          <ErrorBox message={error} />
        </form>
        <aside className="settings-aside">
          <span className="settings-aside-icon">
            <Fingerprint size={27} strokeWidth={1.5} />
          </span>
          <span className="eyebrow">ADMINISTRATOR</span>
          <strong>{username}</strong>
          <div>
            <span className="status-dot" />
            当前管理员
          </div>
          <hr />
          <span>部署模式</span>
          <strong className="deployment-label">独立部署</strong>
          <span>版本</span>
          <code>{currentVersion || "读取中…"}</code>
        </aside>
      </div>
      <SystemUpdates
        notify={notify}
        onVersion={setCurrentVersion}
        updateCheck={updateCheck}
        focusRequest={updateFocusRequest}
      />
      <section className="settings-section password-section">
        <div className="settings-section-heading">
          <span className="settings-icon">
            <KeyRound size={20} />
          </span>
          <div>
            <h2>管理员密码</h2>
            <p>更新后所有登录会话将失效</p>
          </div>
        </div>
        <form onSubmit={changePassword}>
          <div className="settings-section-body">
            <div className="form-grid three">
              <Field label="当前密码">
                <input
                  type="password"
                  autoComplete="current-password"
                  value={currentPassword}
                  onChange={(event) => setCurrentPassword(event.target.value)}
                  required
                  placeholder="验证当前密码"
                />
              </Field>
              <Field label="新密码" hint="至少 12 个字符">
                <input
                  type="password"
                  autoComplete="new-password"
                  value={newPassword}
                  onChange={(event) => setNewPassword(event.target.value)}
                  minLength={12}
                  required
                  placeholder="输入新密码"
                />
              </Field>
              <Field label="确认新密码">
                <input
                  type="password"
                  autoComplete="new-password"
                  value={confirmPassword}
                  onChange={(event) => setConfirmPassword(event.target.value)}
                  minLength={12}
                  required
                  placeholder="再次输入新密码"
                />
              </Field>
            </div>
            <ErrorBox message={passwordError} />
            <div className="password-actions">
              <Button busy={passwordBusy} type="submit">
                <KeyRound size={15} />
                更新密码
              </Button>
            </div>
          </div>
        </form>
      </section>
    </>
  );
}
