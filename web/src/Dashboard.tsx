import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import {
  ArrowDownLeft,
  ArrowUpRight,
  CalendarDays,
  ChartNoAxesCombined,
  CheckCheck,
  ChevronRight,
  Clock3,
  Database,
  Gauge,
  Layers3,
  RefreshCw,
  ShieldAlert,
  Zap,
} from "lucide-react";
import {
  compactCount,
  emptyStats,
  number,
  percent,
  tokenCount,
} from "./api";
import { Loading, useResource } from "./resources";
import type { Stats } from "./api";
import { Badge, Button, ErrorBox, IconButton } from "./components";
import type { Notice } from "./components";

function beginning(daysAgo: number) {
  const date = new Date();
  date.setUTCDate(date.getUTCDate() - daysAgo);
  return date.toISOString().slice(0, 10);
}

const DashboardChart = lazy(() => import("./DashboardChart"));
export function Dashboard({ notify }: { notify: Notice }) {
  const [from, setFrom] = useState(() => beginning(6));
  const [to, setTo] = useState(() => beginning(0));
  const [query, setQuery] = useState({ from, to });
  const start = Date.parse(`${query.from}T00:00:00Z`) / 1000;
  const end = Date.parse(`${query.to}T23:59:59Z`) / 1000;
  const { data, loading, error, load } = useResource<Stats>(`/stats?from=${start}&to=${end}&recent=false`);
  const stats = data ?? emptyStats;
  const [metric, setMetric] = useState<"tokens" | "requests">("tokens");
  useEffect(() => {
    const visible = () => { if (!document.hidden) void load(); };
    const timer = window.setInterval(() => { if (!document.hidden) void load(true); }, 30_000);
    document.addEventListener("visibilitychange", visible);
    return () => { window.clearInterval(timer); document.removeEventListener("visibilitychange", visible); };
  }, [load]);
  const chartData = useMemo(() => {
    const rows = new Map(
      stats.daily.map((row) => [row.date.slice(0, 10), row]),
    );
    const result = [];
    const date = new Date(`${query.from}T00:00:00Z`);
    const end = new Date(`${query.to}T00:00:00Z`);
    for (
      let count = 0;
      date <= end && count < 366;
      count++, date.setUTCDate(date.getUTCDate() + 1)
    ) {
      const key = date.toISOString().slice(0, 10);
      const row = rows.get(key);
      result.push({
        date: key.slice(5).replace("-", "/"),
        uncached: Math.max(
          0,
          (row?.input_tokens || 0) - (row?.cached_tokens || 0),
        ),
        cached: row?.cached_tokens || 0,
        output: row?.output_tokens || 0,
        requests: row?.total_requests || 0,
        cyber: row?.cyber_count || 0,
      });
    }
    return result;
  }, [stats.daily, query]);
  const summary = stats.summary;
  function preset(days: number) {
    const next = { from: beginning(days - 1), to: beginning(0) };
    setFrom(next.from);
    setTo(next.to);
    setQuery(next);
  }
  function applyDates() {
    if (!from || !to || from > to) {
      notify("请选择有效的起止日期", true);
      return;
    }
    if ((new Date(to).getTime() - new Date(from).getTime()) / 86400000 > 365) {
      notify("单次查询范围最多 366 天", true);
      return;
    }
    setQuery({ from, to });
  }
  return (
    <>
      <div className="page-heading">
        <div>
          <div className="eyebrow">WORKSPACE OVERVIEW</div>
          <h1>
            仪表盘
            <span className="heading-dot" />
          </h1>
          <p>用量、性能与请求状态</p>
        </div>
      </div>
      <div className="dashboard-toolbar">
        <div className="segmented" aria-label="查询时间范围">
          {[
            { label: "今天", days: 1 },
            { label: "近 7 天", days: 7 },
            { label: "近 30 天", days: 30 },
          ].map((item) => (
            <button
              key={item.days}
              className={
                query.from === beginning(item.days - 1) &&
                query.to === beginning(0)
                  ? "selected"
                  : ""
              }
              onClick={() => preset(item.days)}
            >
              {item.label}
            </button>
          ))}
        </div>
        <div className="date-range">
          <CalendarDays size={15} />
          <input
            type="date"
            aria-label="开始日期"
            value={from}
            max={to}
            onChange={(event) => setFrom(event.target.value)}
          />
          <span>至</span>
          <input
            type="date"
            aria-label="结束日期"
            value={to}
            min={from}
            onChange={(event) => setTo(event.target.value)}
          />
          <IconButton title="查询日期范围" onClick={applyDates}>
            <ChevronRight size={17} />
          </IconButton>
        </div>
      </div>
      {error && (
        <div className="inline-error">
          <ErrorBox message={error} />
          <Button onClick={() => void load(true)}>
            <RefreshCw size={15} />
            重试
          </Button>
        </div>
      )}
      {!data && loading && <Loading label="统计数据" />}
      {data && <>
      <div
        className={`metrics-grid ${loading ? "is-loading" : ""}`}
        aria-busy={loading}
      >
        <div className="metric">
          <div className="metric-top">
            <span>总 Token 用量</span>
            <Layers3 size={17} />
          </div>
          <div className="metric-value token-metric-value">
            <b title={number(summary.total_tokens)}>
              {tokenCount(summary.total_tokens)}
            </b>
            <span>tokens</span>
          </div>
          <div className="metric-bottom">
            <span className="legend-dot series-primary" />
            <span title={number(summary.input_tokens)}>
              输入 {tokenCount(summary.input_tokens)}
            </span>
            <span className="metric-separator" />
            <span title={number(summary.output_tokens)}>
              输出 {tokenCount(summary.output_tokens)}
            </span>
          </div>
        </div>
        <div className="metric">
          <div className="metric-top">
            <span>总请求数</span>
            <ArrowUpRight size={18} />
          </div>
          <div className="metric-value">
            {number(summary.total_requests)}
            <span>次</span>
          </div>
          <div className="metric-bottom">
            <span className="legend-dot series-honey" />
            {stats.accounts.total} 个账号
            <span className="metric-separator" />
            {stats.keys} 个密钥
          </div>
        </div>
        <div className="metric">
          <div className="metric-top">
            <span>请求成功率</span>
            <CheckCheck size={18} />
          </div>
          <div className="metric-value">{percent(summary.success_rate)}</div>
          <div className="metric-bottom">
            <Badge variant="green" dot>
              正常响应
            </Badge>
            <span>所选时间段</span>
          </div>
        </div>
        <div className="metric">
          <div className="metric-top">
            <span>平均响应耗时</span>
            <Clock3 size={17} />
          </div>
          <div className="metric-value">
            {number(summary.avg_latency_ms)}
            <span>ms</span>
          </div>
          <div className="metric-bottom">
            <span className="legend-dot series-lilac" />
            端到端请求耗时
          </div>
        </div>
      </div>
      <div className="throughput-strip">
        <div>
          <span className="strip-icon">
            <Gauge size={17} />
          </span>
          <span>
            实时 RPM
            <strong>
              {number(summary.rpm)}
              <small>req/min</small>
            </strong>
          </span>
        </div>
        <div>
          <span className="strip-icon">
            <Zap size={17} />
          </span>
          <span>
            实时 TPM
            <strong title={number(summary.tpm)}>
              {tokenCount(summary.tpm)}
              <small>tokens/min</small>
            </strong>
          </span>
        </div>
        <div>
          <span className="strip-icon">
            <Database size={17} />
          </span>
          <span>
            缓存命中率<strong>{percent(summary.cache_hit_rate)}</strong>
          </span>
        </div>
        <div>
          <span className="strip-icon">
            <ShieldAlert size={17} />
          </span>
          <span>
            Cyber 事件
            <strong title={number(summary.cyber_count)}>
              {compactCount(summary.cyber_count)}
              <small>次</small>
            </strong>
          </span>
        </div>
      </div>
      <div className="analytics-layout">
        <section className="chart-section">
          <div className="section-heading">
            <div>
              <h2>
                <ChartNoAxesCombined size={18} />
                用量趋势
              </h2>
              <span className="section-subtitle">每日数据 · UTC</span>
            </div>
            <div className="segmented small">
              <button
                className={metric === "tokens" ? "selected" : ""}
                onClick={() => setMetric("tokens")}
              >
                Token
              </button>
              <button
                className={metric === "requests" ? "selected" : ""}
                onClick={() => setMetric("requests")}
              >
                请求
              </button>
            </div>
          </div>
          <div className="chart-legend">
            {metric === "tokens" ? (
              <>
                <span>
                  <i className="series-primary" />
                  未缓存输入
                </span>
                <span>
                  <i className="series-soft" />
                  缓存输入
                </span>
                <span>
                  <i className="series-honey" />
                  输出
                </span>
              </>
            ) : (
              <>
                <span>
                  <i className="series-primary" />
                  总请求
                </span>
                <span>
                  <i className="series-lilac" />
                  Cyber 事件
                </span>
              </>
            )}
          </div>
          <div className="chart">
            <Suspense fallback={<Loading label="图表" />}><DashboardChart data={chartData} metric={metric} /></Suspense>
          </div>
          <div className="chart-bottom">
            <span className="status-dot" />
            {loading
              ? "正在更新数据"
              : summary.total_requests
                ? "每 30 秒更新"
                : "尚无请求记录"}
            <span>
              {query.from} — {query.to}
            </span>
          </div>
        </section>
        <section className="token-section">
          <div className="section-heading">
            <h2>Token 分布</h2>
            <ArrowDownLeft size={17} />
          </div>
          <div className="token-total">
            <span>累计消耗</span>
            <strong title={number(summary.total_tokens)}>
              {tokenCount(summary.total_tokens)}
            </strong>
            <small>TOKENS</small>
          </div>
          <div className="distribution-bar">
            {summary.total_tokens > 0 ? (
              <>
                <i
                  className="series-primary"
                  style={{ flexGrow: Math.max(0, summary.uncached_tokens) }}
                />
                <i
                  className="series-soft"
                  style={{ flexGrow: summary.cached_tokens }}
                />
                <i
                  className="series-honey"
                  style={{ flexGrow: summary.output_tokens }}
                />
              </>
            ) : (
              <i className="no-distribution" />
            )}
          </div>
          <div className="distribution-row">
            <span>
              <i className="legend-dot series-primary" />
              未缓存输入
            </span>
            <strong title={number(summary.uncached_tokens)}>
              {tokenCount(summary.uncached_tokens)}
            </strong>
          </div>
          <div className="distribution-row">
            <span>
              <i className="legend-dot series-soft" />
              缓存输入
            </span>
            <strong title={number(summary.cached_tokens)}>
              {tokenCount(summary.cached_tokens)}
            </strong>
          </div>
          <div className="distribution-row">
            <span>
              <i className="legend-dot series-honey" />
              输出 Token
            </span>
            <strong title={number(summary.output_tokens)}>
              {tokenCount(summary.output_tokens)}
            </strong>
          </div>
          <div className="cache-rate">
            <span>缓存命中率</span>
            <strong>{percent(summary.cache_hit_rate)}</strong>
          </div>
        </section>
      </div>
      </>}
    </>
  );
}
