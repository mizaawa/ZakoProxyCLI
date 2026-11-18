import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { compactCount, number, tokenCount } from "./api";

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

type Point = { date: string; uncached: number; cached: number; output: number; requests: number; cyber: number };
export default function DashboardChart({ data, metric }: { data: Point[]; metric: "tokens" | "requests" }) {
  return (
    <ResponsiveContainer width="100%" height="100%">
      <AreaChart
        data={data}
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
  );
}
