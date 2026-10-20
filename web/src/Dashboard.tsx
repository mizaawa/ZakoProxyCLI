import { useEffect, useMemo, useState } from "react";
import {
  ArrowDownLeft,
  ArrowUpRight,
  CalendarDays,
  ChartNoAxesCombined,
  CheckCheck,
  ChevronRight,
  Clock3,
  Database,
  Download,
  Gauge,
  Layers3,
  RefreshCw,
  ShieldAlert,
  Zap,
} from "lucide-react";
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import {
  api,
  compactCount,
  emptyStats,
  errorMessage,
  number,
  percent,
  tokenCount,
} from "./api";
import type { Stats } from "./api";
import { Badge, Button, ErrorBox, IconButton } from "./components";
import type { Notice } from "./components";

// Chart colours mirror the .series-* legend classes in style.css.
const CHART = {
  primary: "#e04c8b",
  soft: "#ff9cc2",
  honey: "#e9a512",
  lilac: "#9b7be6",
  grid: "#fbe1ec",
  tick: "#8f6378",
  ink: "#4a2a3d",
} as const;

function beginning(daysAgo: number) {
  const date = new Date();
  date.setUTCDate(date.getUTCDate() - daysAgo);
  return date.toISOString().slice(0, 10);
}

export function Dashboard({
  revision,
  notify,
}: {
  revision: number;
  notify: Notice;
}) {
  const [from, setFrom] = useState(beginning(6));
  const [to, setTo] = useState(beginning(0));
  const [query, setQuery] = useState({ from, to });
  const [stats, setStats] = useState<Stats>(emptyStats);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const [metric, setMetric] = useState<"tokens" | "requests">("tokens");
  useEffect(() => {
    let cancelled = false;
    const load = async (initial: boolean) => {
      if (initial) setLoading(true);
      try {
        const start = Math.floor(
          new Date(`${query.from}T00:00:00Z`).getTime() / 1000,
        );
        const end = Math.floor(
          new Date(`${query.to}T23:59:59Z`).getTime() / 1000,
        );
        const result = await api<Stats>(`/stats?from=${start}&to=${end}`);
        if (!cancelled) {
          setStats(result);
          setError("");
        }
      } catch (failure) {
        if (!cancelled) setError(errorMessage(failure));
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    void load(true);
    const interval = window.setInterval(() => void load(false), 30_000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [query, revision, retry]);
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
  function exportDaily() {
    const csv = [
      "date,input_tokens,output_tokens,cached_tokens,total_requests,success_count,cyber_count",
      ...stats.daily.map((row) =>
        [
          row.date,
          row.input_tokens,
          row.output_tokens,
          row.cached_tokens,
          row.total_requests,
          row.success_count,
          row.cyber_count,
        ].join(","),
      ),
    ].join("\r\n");
    const url = URL.createObjectURL(
      new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" }),
    );
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `zako-usage-${query.from}-${query.to}.csv`;
    anchor.click();
    URL.revokeObjectURL(url);
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
        <Button onClick={exportDaily} disabled={loading}>
          <Download size={16} />
          导出数据
        </Button>
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
          <Button onClick={() => setRetry((value) => value + 1)}>
            <RefreshCw size={15} />
            重试
          </Button>
        </div>
      )}
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
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart
                data={chartData}
                margin={{ top: 10, right: 12, bottom: 0, left: -15 }}
              >
                <defs>
                  {(["primary", "soft", "honey", "lilac"] as const).map((tone) => (
                    <linearGradient key={tone} id={`zako-fill-${tone}`} x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor={CHART[tone]} stopOpacity={0.32} />
                      <stop offset="100%" stopColor={CHART[tone]} stopOpacity={0.02} />
                    </linearGradient>
                  ))}
                </defs>
                <CartesianGrid
                  stroke={CHART.grid}
                  strokeDasharray="4 6"
                  vertical={false}
                />
                <XAxis
                  dataKey="date"
                  tick={{ fontSize: 11, fill: CHART.tick }}
                  axisLine={false}
                  tickLine={false}
                  minTickGap={25}
                  dy={8}
                />
                <YAxis
                  tickFormatter={metric === "tokens" ? tokenCount : compactCount}
                  tick={{ fontSize: 11, fill: CHART.tick }}
                  axisLine={false}
                  tickLine={false}
                  allowDecimals={false}
                  width={metric === "tokens" ? 72 : 60}
                  domain={[0, "auto"]}
                />
                <Tooltip
                  cursor={{ stroke: CHART.soft, strokeDasharray: "4 4" }}
                  contentStyle={{
                    borderRadius: 16,
                    border: `1.5px solid ${CHART.grid}`,
                    fontSize: 12,
                    boxShadow: "0 12px 28px -12px rgb(214 55 122 / 35%)",
                  }}
                  itemStyle={{ color: CHART.ink }}
                  formatter={(value) =>
                    metric === "tokens" ? (
                      <span title={number(Number(value || 0))}>
                        {tokenCount(Number(value || 0))}
                      </span>
                    ) : (
                      compactCount(Number(value || 0))
                    )
                  }
                  labelStyle={{ color: CHART.tick, fontWeight: 700, marginBottom: 7 }}
                />
                {metric === "tokens" ? (
                  <>
                    <Area
                      isAnimationActive={false}
                      type="monotone"
                      stackId="tokens"
                      dataKey="uncached"
                      name="未缓存输入"
                      stroke={CHART.primary}
                      fill="url(#zako-fill-primary)"
                      fillOpacity={1}
                      strokeWidth={2}
                    />
                    <Area
                      isAnimationActive={false}
                      type="monotone"
                      stackId="tokens"
                      dataKey="cached"
                      name="缓存输入"
                      stroke={CHART.soft}
                      fill="url(#zako-fill-soft)"
                      fillOpacity={1}
                      strokeWidth={1.5}
                    />
                    <Area
                      isAnimationActive={false}
                      type="monotone"
                      stackId="tokens"
                      dataKey="output"
                      name="输出"
                      stroke={CHART.honey}
                      fill="url(#zako-fill-honey)"
                      fillOpacity={1}
                      strokeWidth={1.5}
                    />
                  </>
                ) : (
                  <>
                    <Area
                      isAnimationActive={false}
                      type="monotone"
                      dataKey="requests"
                      name="总请求"
                      stroke={CHART.primary}
                      fill="url(#zako-fill-primary)"
                      fillOpacity={1}
                      strokeWidth={2}
                    />
                    <Area
                      isAnimationActive={false}
                      type="monotone"
                      dataKey="cyber"
                      name="Cyber 事件"
                      stroke={CHART.lilac}
                      fill="url(#zako-fill-lilac)"
                      fillOpacity={1}
                      strokeWidth={1.5}
                    />
                  </>
                )}
              </AreaChart>
            </ResponsiveContainer>
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
    </>
  );
}
