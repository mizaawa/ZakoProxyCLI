import { useEffect, useId, useRef, useState } from "react";
import {
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Download,
  FileClock,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { useResource, useResources } from "./resources";
import { api, errorMessage, number, tokenCount } from "./api";
import type { RequestLog, RequestLogExport, RequestLogPage } from "./api";
import { Badge, Button, Confirm, Empty, ErrorBox } from "./components";
import type { Notice } from "./components";

function duration(milliseconds: number | null | undefined) {
  if (milliseconds === null || milliseconds === undefined) return "—";
  if (milliseconds < 1000) return `${number(milliseconds)} ms`;
  if (milliseconds < 60_000) return `${(milliseconds / 1000).toFixed(2)} s`;
  return `${(milliseconds / 60_000).toFixed(2)} min`;
}

function timestamp(value: number) {
  return new Date(value * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function Metric({
  label,
  value,
  tone = "neutral",
  title,
}: {
  label: string;
  value: string;
  tone?: "neutral" | "green" | "blue" | "amber" | "purple";
  title?: string;
}) {
  return (
    <span className={`log-metric log-metric-${tone}`} title={title}>
      <span>{label}</span>
      <strong>{value}</strong>
    </span>
  );
}

// Large installations can have thousands of pages. A direct jump stays constant-size.
function LogPageJump({ page, totalPages, disabled, onChange }: {
  page: number; totalPages: number; disabled: boolean; onChange: (page: number) => void;
}) {
  const [value, setValue] = useState(String(page));
  return <form className="log-page-jump" onSubmit={(e) => {
    e.preventDefault();
    const next = Number(value);
    if (Number.isInteger(next) && next >= 1 && next <= totalPages) onChange(next);
  }}>
    <input aria-label="跳转到页码" type="number" min={1} max={totalPages} required disabled={disabled}
      value={value} onChange={(e) => setValue(e.target.value)} />
    <Button type="submit" disabled={disabled}>跳转</Button>
  </form>;
}

function LogPageSelect({
  page,
  totalPages,
  disabled,
  onChange,
}: {
  page: number;
  totalPages: number;
  disabled: boolean;
  onChange: (page: number) => void;
}) {
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const search = useRef({ text: "", time: 0 });
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(Math.max(0, page - 1));
  const unavailable = disabled || totalPages < 1;

  function positionMenu() {
    const button = trigger.current;
    const popup = menu.current;
    if (!button || !popup) return;
    const rect = button.getBoundingClientRect();
    const viewport = window.visualViewport;
    const viewTop = viewport?.offsetTop ?? 0;
    const viewLeft = viewport?.offsetLeft ?? 0;
    const viewHeight = viewport?.height ?? window.innerHeight;
    const viewWidth = viewport?.width ?? window.innerWidth;
    const desiredHeight = Math.min(300, totalPages * 36 + 42);
    const below = viewTop + viewHeight - rect.bottom - 14;
    const above = rect.top - viewTop - 14;
    const upward = below < desiredHeight && above > below;
    const height = Math.min(desiredHeight, Math.max(80, upward ? above : below));
    const width = Math.min(Math.max(rect.width, 190), viewWidth - 24);
    popup.style.width = `${width}px`;
    popup.style.maxHeight = `${height}px`;
    popup.style.left = `${Math.max(viewLeft + 12, Math.min(rect.left, viewLeft + viewWidth - width - 12))}px`;
    popup.style.top = `${upward ? Math.max(viewTop + 12, rect.top - height - 6) : rect.bottom + 6}px`;
  }

  function close() {
    menu.current?.hidePopover();
    setOpen(false);
  }

  function show(index = Math.min(totalPages - 1, Math.max(0, page - 1))) {
    if (unavailable) return;
    setActive(index);
    positionMenu();
    menu.current?.showPopover();
    setOpen(true);
  }

  function choose(index: number) {
    if (index < 0 || index >= totalPages) return;
    onChange(index + 1);
    close();
    trigger.current?.focus();
  }

  useEffect(() => {
    setActive(Math.min(totalPages - 1, Math.max(0, page - 1)));
  }, [page, totalPages]);

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
  }, [open, totalPages]);

  useEffect(() => {
    if (open) menu.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active, open]);

  useEffect(() => {
    if (unavailable && open) close();
  }, [unavailable, open]);

  function onKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    if (unavailable) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) show();
      else setActive((index) => (index + (event.key === "ArrowDown" ? 1 : -1) + totalPages) % totalPages);
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const index = event.key === "Home" ? 0 : totalPages - 1;
      if (!open) show(index);
      else setActive(index);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (open) choose(active);
      else show();
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      event.stopPropagation();
      close();
    } else if (event.key === "Tab") {
      close();
    } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      const now = Date.now();
      const text = (now - search.current.time < 700 ? search.current.text : "") + event.key;
      search.current = { text, time: now };
      const value = Number(text);
      const index = Number.isInteger(value) && value >= 1 && value <= totalPages && String(value) === text ? value - 1 : -1;
      if (index >= 0) {
        event.preventDefault();
        if (open) setActive(index);
        else show(index);
      }
    }
  }

  return (
    <span className="log-page-select">
      <button
        ref={trigger}
        type="button"
        className={`log-page-select-trigger${open ? " is-open" : ""}`}
        role="combobox"
        aria-label="跳转到页码"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={`${id}-list`}
        aria-activedescendant={open ? `${id}-option-${active}` : undefined}
        disabled={unavailable}
        onKeyDown={onKeyDown}
        onClick={() => open ? close() : show()}
      >
        <span className="log-page-select-current">第 <strong>{Math.min(page, totalPages)}</strong> 页</span>
        <span className="log-page-select-total">共 {totalPages} 页</span>
        <ChevronDown size={15} className="log-page-select-chevron" aria-hidden="true" />
      </button>
      <div
        ref={menu}
        id={`${id}-list`}
        className="log-page-select-menu"
        popover="auto"
        role="listbox"
        aria-label="选择页码"
        onToggle={(event) => setOpen(event.currentTarget.matches(":popover-open"))}
      >
        <div className="log-page-select-heading" role="presentation">
          <span>选择页码</span><span>{totalPages} 页</span>
        </div>
        {open && Array.from({ length: totalPages }, (_, index) => (
          <div
            id={`${id}-option-${index}`}
            key={index + 1}
            role="option"
            aria-selected={index + 1 === page}
            data-index={index}
            className={`log-page-select-option${index === active ? " is-active" : ""}`}
            onPointerMove={() => setActive(index)}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => choose(index)}
          >
            <span>第 {index + 1} 页</span><small>/ 共 {totalPages} 页</small>
            {index + 1 === page && <Check size={15} strokeWidth={2.3} aria-hidden="true" />}
          </div>
        ))}
      </div>
    </span>
  );
}

function LogRow({ row }: { row: RequestLog }) {
  const succeeded = row.status >= 200 && row.status < 300;
  const redirected = row.status >= 300 && row.status < 400;
  const statusLabel = succeeded ? "成功" : redirected ? "重定向" : "失败";
  const statusTone = succeeded ? "green" : redirected ? "amber" : "red";
  const account =
    row.account_name ||
    "未匹配账号";
  const key =
    row.key_name ||
    "未匹配密钥";
  const cached = row.usage.cache_read_input_tokens ?? row.usage.cached_tokens;
  const created = row.usage.cache_creation_input_tokens ?? 0;
  const totalTokens = row.usage.input_tokens + row.usage.output_tokens;
  const cacheRate = row.usage.input_tokens
    ? `${Math.min(100, (cached / row.usage.input_tokens) * 100).toFixed(1)}%`
    : "—";
  const generationMs =
    row.first_token_ms === null || row.first_token_ms === undefined
      ? null
      : row.latency_ms - row.first_token_ms;
  const tps =
    generationMs !== null && generationMs > 0 && row.usage.output_tokens > 0
      ? ((row.usage.output_tokens * 1000) / generationMs).toFixed(2)
      : "—";
  return (
    <li>
      <details className={`request-log-row request-log-${statusTone}`}>
        <summary className="request-log-summary">
          <span className="request-log-overview">
            <time className="log-time mono" dateTime={new Date(row.timestamp * 1000).toISOString()}>
              {timestamp(row.timestamp)}
            </time>
            <strong className="log-model" title={row.model || "未指定模型"}>
              {row.model || "未指定模型"}
            </strong>
            <Badge>{row.protocol || "未知协议"}</Badge>
            <Badge variant={statusTone} dot>
              {row.status || "未响应"} {statusLabel}
            </Badge>
            {!!row.cyber && <Badge variant="amber">Cyber</Badge>}
            <span className="log-source" title={`账号：${account} · 密钥：${key}`}>
              {account} / {key}
            </span>
            <ChevronDown className="log-expand-icon" size={15} aria-hidden="true" />
            <span className="visually-hidden">展开或收起请求 {row.id} 的详情</span>
          </span>
          <span className="request-log-metrics">
            <span className="log-metric-group" aria-label="Token 用量与缓存">
              <Metric label="Token" value={tokenCount(totalTokens)} title={`合计 ${number(totalTokens)} Token`} />
              <Metric label="输入" value={tokenCount(row.usage.input_tokens)} tone="green" title={number(row.usage.input_tokens)} />
              <Metric label="输出" value={tokenCount(row.usage.output_tokens)} tone="blue" title={number(row.usage.output_tokens)} />
              <Metric label="缓存命中" value={tokenCount(cached)} tone="green" title={`读取 ${number(cached)} Token`} />
              <Metric label="缓存写入" value={tokenCount(created)} tone="amber" title={`创建 ${number(created)} Token`} />
              <Metric label="命中率" value={cacheRate} />
            </span>
            <span className="log-metric-group" aria-label="耗时与生成速度">
              <Metric label="首字" value={duration(row.first_token_ms)} tone="purple" title="首字耗时（TTFT）；未采集时显示 —" />
              <Metric label="总耗时" value={duration(row.latency_ms)} />
              <Metric label="TPS" value={tps} tone="blue" title="每秒生成 Token 数 = 输出 Token ÷（总耗时 − 首字耗时）；数据不足时显示 —" />
            </span>
          </span>
        </summary>
        <div className="request-log-detail">
          <dl className="log-detail-grid">
            <div><dt>请求 ID</dt><dd className="mono">{row.id}</dd></div>
            <div><dt>请求时间</dt><dd>{new Date(row.timestamp * 1000).toLocaleString("zh-CN", { hour12: false })}</dd></div>
            <div><dt>模型 / 协议</dt><dd>{row.model || "—"} / {row.protocol || "—"}</dd></div>
            <div><dt>请求路径</dt><dd className="mono">{row.request_path || "—"}</dd></div>
            <div><dt>账号</dt><dd>{account}<span className="log-detail-id mono">{row.account_id || "—"}</span></dd></div>
            <div><dt>密钥</dt><dd>{key}<span className="log-detail-id mono">{row.key_id || "—"}</span></dd></div>
            <div><dt>HTTP 状态码</dt><dd>{row.status || "未收到响应"} · {statusLabel}</dd></div>
            <div><dt>上游尝试次数</dt><dd>{number(row.attempts)}</dd></div>
            <div><dt>输入 / 输出 Token</dt><dd>{number(row.usage.input_tokens)} / {number(row.usage.output_tokens)}</dd></div>
            <div><dt>缓存读取 / 写入 Token</dt><dd>{number(cached)} / {number(created)} · 命中率 {cacheRate}</dd></div>
            <div><dt>首字 / 总耗时</dt><dd>{duration(row.first_token_ms)} / {duration(row.latency_ms)}</dd></div>
            <div><dt>生成速度</dt><dd>{tps === "—" ? "—（数据不足）" : `${tps} Token/s`}</dd></div>
            <div><dt>Cyber</dt><dd>{row.cyber ? `已触发${typeof row.cyber === "number" ? ` ${number(row.cyber)} 次` : ""}` : "未触发"}</dd></div>
          </dl>
          {row.error && (
            <div className="log-error-detail">
              <strong>错误原因 / 上游报错</strong>
              <pre>{row.error}</pre>
            </div>
          )}
        </div>
      </details>
    </li>
  );
}

export function RequestLogs({ notify }: { notify: Notice }) {
  const cache = useResources();
  const [page, setPage] = useState(1);
  const { data, error, loading, load } = useResource<RequestLogPage>(`/logs?page=${page}`);
  const [exporting, setExporting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState("");
  useEffect(() => {
    if (data && data.page !== page) {
      cache.get<RequestLogPage>(`/logs?page=${data.page}`).set(data);
      setPage(data.page);
    }
  }, [cache, data, page]);

  async function exportLogs() {
    setExporting(true);
    try {
      const result = await api<RequestLogExport>("/logs/export");
      const url = URL.createObjectURL(new Blob([JSON.stringify(result, null, 2)], {
        type: "application/json;charset=utf-8",
      }));
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `zako-call-logs-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      notify(`已导出 ${number(result.logs.length)} 条调用日志`);
    } catch (failure) {
      notify(errorMessage(failure), true);
    } finally {
      setExporting(false);
    }
  }

  async function deleteLogs() {
    setDeleting(true);
    setDeleteError("");
    try {
      const result = await api<{ deleted: number }>("/logs", "DELETE");
      setConfirmDelete(false);
      cache.invalidate("/logs", "/keys", "/stats");
      cache.get<RequestLogPage>("/logs?page=1").set({ items: [], page: 1, page_size: 20, total: 0, total_pages: 1, retention_days: data?.retention_days ?? 7 });
      setPage(1);
      notify(`已删除 ${number(result.deleted)} 条调用日志`);
    } catch (failure) {
      setDeleteError(errorMessage(failure));
    } finally {
      setDeleting(false);
    }
  }

  const retentionDays = data?.retention_days ?? 7;
  const totalPages = Math.max(1, data?.total_pages ?? 1);
  const currentData = data?.page === page ? data : null;
  const total = data?.total ?? 0;
  const busy = (!data && loading) || deleting;
  return (
    <>
      <div className="page-heading log-page-heading">
        <div>
          <div className="eyebrow">REQUEST LOGS</div>
          <h1>调用日志<span className="title-count">{number(total)}</span></h1>
          <p>默认保留最近 7 天{retentionDays !== 7 ? ` · 当前保留 ${retentionDays} 天` : ""} · 过期自动清理</p>
        </div>
        <div className="log-page-actions">
          <Button onClick={() => void load(true)} disabled={loading || deleting}>
            <RefreshCw size={15} className={loading ? "spin" : ""} />刷新
          </Button>
          <Button onClick={() => void exportLogs()} busy={exporting} disabled={deleting}>
            {!exporting && <Download size={15} />}导出 JSON
          </Button>
          <Button className="danger" disabled={busy || exporting || !total} onClick={() => {
            setDeleteError("");
            setConfirmDelete(true);
          }}>
            <Trash2 size={15} />删除全部
          </Button>
        </div>
      </div>
      <ErrorBox message={error} />
      <section className="request-logs-section" aria-label="调用日志" aria-busy={loading}>
        <div className="log-section-heading">
          <h2><FileClock size={17} />请求记录</h2>
          <span>每页 20 条 · 按时间倒序 · 点击记录展开详情</span>
        </div>
        {!currentData && loading ? (
          <Empty title="正在加载调用日志" />
        ) : !currentData && error ? (
          <Empty title="调用日志加载失败" action={<Button onClick={() => void load(true)}>重新加载</Button>} />
        ) : currentData?.items.length ? (
          <ol className="request-log-list">
            {currentData.items.map((row) => <LogRow key={row.id} row={row} />)}
          </ol>
        ) : (
          <Empty title="暂无调用日志" detail={`最近 ${retentionDays} 天没有调用记录`} />
        )}
        <div className="log-pagination">
          <span className="log-pagination-count" aria-live="polite">
            {currentData?.items.length && !loading
              ? `第 ${(page - 1) * 20 + 1}–${(page - 1) * 20 + currentData.items.length} 条，共 ${number(total)} 条`
              : `共 ${number(total)} 条`}
          </span>
          <div className="log-pagination-controls">
            <Button disabled={busy || !!error || page <= 1} onClick={() => setPage((value) => value - 1)}>
              <ChevronLeft size={15} />上一页
            </Button>
            {totalPages > 100 ? <LogPageJump key={page} page={page} totalPages={totalPages} disabled={busy} onChange={setPage} /> : (
            <LogPageSelect
              page={Math.min(page, totalPages)}
              totalPages={totalPages}
              disabled={busy || !!error || !total}
              onChange={setPage}
            />
            )}
            <Button disabled={busy || !!error || page >= totalPages} onClick={() => setPage((value) => value + 1)}>
              下一页<ChevronRight size={15} />
            </Button>
          </div>
        </div>
      </section>
      {confirmDelete && (
        <Confirm title="删除全部调用日志" message="确认删除全部调用日志？此操作不可撤销。需要保留记录时，请先导出 JSON。" busy={deleting} error={deleteError} onClose={() => setConfirmDelete(false)} onConfirm={() => void deleteLogs()} />
      )}
    </>
  );
}
