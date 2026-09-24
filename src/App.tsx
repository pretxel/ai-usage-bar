/**
 * Token Tracker — dashboard UI.
 *
 * Renders usage stats, model spend bars, token composition, daily timeline,
 * monthly heatmap, provider comparison, and top projects. All charts are
 * hand-rolled CSS; no chart library is used.
 *
 * The app calls `invoke("get_usage", { provider, days })` to fetch a
 * `UsageReport` from the Rust backend and listens for `usage-changed` events
 * emitted by the filesystem watcher to auto-refresh.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  DayRange,
  DayUsage,
  ModelUsage,
  ProjectUsage,
  ProviderFilter,
  ProviderUsage,
  UsageReport,
  familyOf,
  providerAccent,
} from "./types";
import { ago, clock, compact, money, prettyDate, shortDay } from "./format";
import "./App.css";

type Metric = "cost" | "tokens" | "breakdown";

export default function App() {
  const [report, setReport] = useState<UsageReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  // `loading` is a foreground fetch (first load, filter or range change) and
  // dims the dashboard; `refreshing` is a watcher-triggered background fetch
  // that swaps data in place without disturbing the view.
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const latest = useRef(0);
  const [metric, setMetric] = useState<Metric>("cost");
  const [filter, setFilter] = useState<ProviderFilter>("all");
  const [range, setRange] = useState<DayRange>("all");

  const load = useCallback(
    async (background = false) => {
      // Only the newest request may commit, so a slow response for an old
      // filter can't overwrite a newer one.
      const id = ++latest.current;
      if (background) {
        setRefreshing(true);
      } else {
        setLoading(true);
        setError(null);
      }
      try {
        const r = await invoke<UsageReport>("get_usage", {
          provider: filter,
          days: range === "all" ? null : range,
        });
        if (id === latest.current) {
          setReport(r);
          setError(null);
        }
      } catch (e) {
        // A failed background refresh keeps the last good report on screen.
        if (id === latest.current && !background) {
          setError(typeof e === "string" ? e : String(e));
        }
      } finally {
        if (id === latest.current) {
          setLoading(false);
          setRefreshing(false);
        }
      }
    },
    [filter, range],
  );

  useEffect(() => {
    load();
  }, [load]);

  // Live updates: backend watches transcript dirs and emits on change.
  useEffect(() => {
    const un = listen("usage-changed", () => load(true));
    return () => {
      un.then((off) => off());
    };
  }, [load]);

  return (
    <div className="app">
      <div className="bg-glow" aria-hidden />
      <Header report={report} busy={loading || refreshing} />
      <main className={"content" + (loading && report ? " busy" : "")}>
        {error && <ErrorState message={error} onRetry={() => load()} />}
        {!error && loading && !report && <Loading />}
        {!error && report && (
          <Dashboard
            report={report}
            metric={metric}
            onMetric={setMetric}
            filter={filter}
            onFilter={setFilter}
            range={range}
            onRange={setRange}
          />
        )}
      </main>
    </div>
  );
}

function Header({
  report,
  busy,
}: {
  report: UsageReport | null;
  busy: boolean;
}) {
  return (
    <header className="header">
      <div className="brand">
        <img className="brand-logo" src="/logo.svg" alt="" aria-hidden />
        <div className="brand-text">
          <h1>Token Tracker</h1>
          <p>
            {report
              ? `${report.fileCount.toLocaleString()} transcripts · updated ${clock(
                  report.generatedAt,
                )}`
              : "Reading local Claude Code usage…"}
          </p>
        </div>
      </div>
      <div className={"live" + (busy ? " busy" : "")} title="Updates automatically when transcripts change">
        <span className="live-dot" aria-hidden />
        {busy ? "Updating…" : "Live"}
      </div>
    </header>
  );
}

function Dashboard({
  report,
  metric,
  onMetric,
  filter,
  onFilter,
  range,
  onRange,
}: {
  report: UsageReport;
  metric: Metric;
  onMetric: (m: Metric) => void;
  filter: ProviderFilter;
  onFilter: (f: ProviderFilter) => void;
  range: DayRange;
  onRange: (r: DayRange) => void;
}) {
  const t = report.totalTokens;
  const empty = report.totalMessages === 0;
  const window = range === "all" ? undefined : range;
  return (
    <>
      <div className="filters">
        <ProviderTabs
          providers={report.providers}
          value={filter}
          onChange={onFilter}
        />
        <RangeTabs value={range} onChange={onRange} />
      </div>

      {empty ? (
        <EmptyState dirs={report.dataDirs} filtered={filter !== "all" || range !== "all"} />
      ) : (
      <>
      <section className="stats">
        <Stat label="Estimated spend" value={money(report.totalCost)} accent="violet" big />
        <Stat label="Today" value={money(report.todayCost)} accent="amber" />
        <Stat
          label="Avg / day"
          value={money(report.avgCostPerDay)}
          sub={`over ${compact(report.activeDays)} active days`}
        />
        <Stat
          label="Total tokens"
          value={compact(t.total)}
          sub={`${cachePct(t.cacheRead, t.total)} cached`}
          title={`${compact(t.input)} input · ${compact(t.output)} output · ${compact(
            t.cacheCreation,
          )} cache write · ${compact(t.cacheRead)} cache read`}
        />
        <Stat label="Messages" value={compact(report.totalMessages)} />
        <Stat
          label="Sessions"
          value={compact(report.totalSessions)}
          sub={`${report.byProject.length} projects`}
        />
      </section>

      <div className="grid">
        <Panel title="Spend by model" hint="Estimated, current public pricing">
          <ModelBars models={report.models} />
        </Panel>

        <div className="stack-col">
          <Panel title="Token composition" hint="Across all models">
            <TokenComposition report={report} />
          </Panel>
          <Panel title="Tokens by provider" hint="Bar length relative to the largest source">
            <ProviderTokenComparison providers={report.providers} />
          </Panel>
        </div>
      </div>

      <Panel
        title="Daily activity"
        hint={`Last ${window ?? 90} days`}
        right={
          <Toggle
            value={metric}
            onChange={onMetric}
            options={[
              { value: "cost", label: "Cost" },
              { value: "tokens", label: "Tokens" },
              { value: "breakdown", label: "Breakdown" },
            ]}
          />
        }
      >
        <Timeline byDay={report.byDay} metric={metric} days={window} />
      </Panel>

      <Panel
        title="Activity calendar"
        hint="Last 12 months · follows the Cost / Tokens toggle"
      >
        <ActivityCalendar byDay={report.byDay} metric={metric} />
      </Panel>

      <Panel title="Top projects" hint={`${report.byProject.length} total`}>
        <Projects projects={report.byProject} />
      </Panel>

      <footer className="foot">
        Reading <code>{report.dataDirs.join("  ·  ")}</code>. Costs are estimates
        based on token counts and public list pricing — not a billing statement.
      </footer>
      </>
      )}
    </>
  );
}

function ProviderTabs({
  providers,
  value,
  onChange,
}: {
  providers: ProviderUsage[];
  value: ProviderFilter;
  onChange: (f: ProviderFilter) => void;
}) {
  const avail = providers.filter((p) => p.available);
  if (avail.length < 2) return null; // only meaningful with >1 source
  const allCost = avail.reduce((s, p) => s + p.cost, 0);
  return (
    <div className="provtabs">
      <button className={value === "all" ? "on" : ""} onClick={() => onChange("all")}>
        <span className="pt-label">All sources</span>
        <span className="pt-cost">{money(allCost)}</span>
      </button>
      {avail.map((p) => (
        <button
          key={p.provider}
          className={value === p.provider ? "on" : ""}
          onClick={() => onChange(p.provider as ProviderFilter)}
        >
          <span className={"pt-dot " + providerAccent(p.provider)} />
          <span className="pt-label">{p.displayName}</span>
          <span className="pt-cost">{money(p.cost)}</span>
        </button>
      ))}
    </div>
  );
}

function RangeTabs({
  value,
  onChange,
}: {
  value: DayRange;
  onChange: (r: DayRange) => void;
}) {
  const opts: { value: DayRange; label: string }[] = [
    { value: "all", label: "All time" },
    { value: 30, label: "30 days" },
    { value: 15, label: "15 days" },
    { value: 7, label: "7 days" },
  ];
  return (
    <div className="rangetabs">
      {opts.map((o) => (
        <button
          key={String(o.value)}
          className={value === o.value ? "on" : ""}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Stat({
  label,
  value,
  sub,
  accent,
  big,
  title,
}: {
  label: string;
  value: string;
  sub?: string;
  title?: string;
  accent?: "violet" | "amber";
  big?: boolean;
}) {
  return (
    <div className={"stat" + (big ? " big" : "") + (accent ? " a-" + accent : "")} title={title}>
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
      {sub && <span className="stat-sub">{sub}</span>}
    </div>
  );
}

function Panel({
  title,
  hint,
  right,
  children,
}: {
  title: string;
  hint?: string;
  right?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="panel">
      <div className="panel-head">
        <div>
          <h2>{title}</h2>
          {hint && <span className="panel-hint">{hint}</span>}
        </div>
        {right}
      </div>
      <div className="panel-body">{children}</div>
    </section>
  );
}

const MODELS_COLLAPSED = 8;

function ModelBars({ models }: { models: ModelUsage[] }) {
  const [expanded, setExpanded] = useState(false);
  const max = Math.max(...models.map((m) => m.cost), 0.0001);
  const hidden = models.length - MODELS_COLLAPSED;
  const shown = expanded || hidden <= 1 ? models : models.slice(0, MODELS_COLLAPSED);
  return (
    <div className="bars">
      {shown.map((m) => {
        const t = m.tokens;
        const tsegs = [
          { key: "input", value: t.input, cls: "seg-in" },
          { key: "output", value: t.output, cls: "seg-out" },
          { key: "cacheCreation", value: t.cacheCreation, cls: "seg-cw" },
          { key: "cacheRead", value: t.cacheRead, cls: "seg-cr" },
        ].filter((s) => s.value > 0);
        const ttotal = t.total || 1;
        return (
          <div className="bar-row" key={m.model}>
            <div className="bar-row-head">
              <span className={"dot f-" + familyOf(m.model)} />
              <span className="bar-name">{m.displayName}</span>
              <span className="bar-meta">
                {compact(m.tokens.total)} tok · {compact(m.messages)} msg
              </span>
              <span className="bar-value">{money(m.cost)}</span>
            </div>
            <div className="track">
              <div
                className={"fill f-" + familyOf(m.model)}
                style={{ width: `${Math.max((m.cost / max) * 100, 1.5)}%` }}
              />
            </div>
            {tsegs.length > 1 && (
              <div className="mini-bar" title={`${compact(t.input)} in · ${compact(t.output)} out · ${compact(t.cacheCreation)} cw · ${compact(t.cacheRead)} cr`}>
                {tsegs.map((s) => (
                  <div
                    key={s.key}
                    className={"mini-seg " + s.cls}
                    style={{ width: `${(s.value / ttotal) * 100}%` }}
                  />
                ))}
              </div>
            )}
          </div>
        );
      })}
      {hidden > 1 && (
        <button className="more" onClick={() => setExpanded((e) => !e)}>
          {expanded ? "Show fewer" : `Show ${hidden} more models`}
        </button>
      )}
    </div>
  );
}

/** Share of `part` in `total` as a percentage label, e.g. "97.8%". */
function cachePct(part: number, total: number): string {
  return total > 0 ? `${((part / total) * 100).toFixed(1)}%` : "0%";
}

function TokenComposition({ report }: { report: UsageReport }) {
  const t = report.totalTokens;
  const segs = [
    { key: "input", label: "Input", value: t.input, cls: "seg-in" },
    { key: "output", label: "Output", value: t.output, cls: "seg-out" },
    { key: "cacheCreation", label: "Cache write", value: t.cacheCreation, cls: "seg-cw" },
    { key: "cacheRead", label: "Cache read", value: t.cacheRead, cls: "seg-cr" },
  ];
  const total = t.total || 1;
  return (
    <div className="compose">
      <div className="stack">
        {segs.map((s) => (
          <div
            key={s.key}
            className={"stack-seg " + s.cls}
            style={{ width: `${(s.value / total) * 100}%` }}
            title={`${s.label}: ${s.value.toLocaleString()}`}
          />
        ))}
      </div>
      <ul className="legend">
        {segs.map((s) => (
          <li key={s.key}>
            <span className={"swatch " + s.cls} />
            <span className="legend-label">{s.label}</span>
            <span className="legend-value">{compact(s.value)}</span>
            <span className="legend-pct">{((s.value / total) * 100).toFixed(1)}%</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function buildTimeline(byDay: DayUsage[], days = 90): DayUsage[] {
  const valid = byDay.filter((d) => /^\d{4}-\d{2}-\d{2}$/.test(d.date));
  if (valid.length === 0) return [];
  const map = new Map(valid.map((d) => [d.date, d]));
  const latest = valid[valid.length - 1].date;
  const end = new Date(latest + "T00:00:00Z");
  const out: DayUsage[] = [];
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(end);
    d.setUTCDate(d.getUTCDate() - i);
    const key = d.toISOString().slice(0, 10);
    out.push(
      map.get(key) ?? {
        date: key,
        cost: 0,
        messages: 0,
        tokens: { input: 0, output: 0, cacheCreation: 0, cacheRead: 0, total: 0 },
      },
    );
  }
  return out;
}

function Timeline({
  byDay,
  metric,
  days: windowDays,
}: {
  byDay: DayUsage[];
  metric: Metric;
  days?: number;
}) {
  const days = useMemo(
    () => buildTimeline(byDay, windowDays ?? 90),
    [byDay, windowDays],
  );
  const [hover, setHover] = useState<number | null>(null);
  const valueOf = (d: DayUsage) =>
    metric === "cost" ? d.cost : d.tokens.total;
  const max = Math.max(...days.map(valueOf), metric === "cost" ? 0.01 : 1);

  if (days.length === 0) return <div className="empty-mini">No dated activity yet.</div>;

  const active = hover != null ? days[hover] : days[days.length - 1];
  const captionVal =
    metric === "cost"
      ? money(active.cost)
      : metric === "breakdown"
        ? `${compact(active.tokens.total)} tokens`
        : `${compact(active.tokens.total)} tokens`;

  const segsOf = (d: DayUsage) =>
    [
      { key: "input", value: d.tokens.input, cls: "seg-in", label: "Input" },
      { key: "output", value: d.tokens.output, cls: "seg-out", label: "Output" },
      { key: "cacheCreation", value: d.tokens.cacheCreation, cls: "seg-cw", label: "Cache write" },
      { key: "cacheRead", value: d.tokens.cacheRead, cls: "seg-cr", label: "Cache read" },
    ];

  return (
    <div className="timeline">
      <div className="timeline-caption">
        <span className="tl-date">{prettyDate(active.date + "T00:00:00Z")}</span>
        <span className="tl-val">{captionVal}</span>
        <span className="tl-msg">{compact(active.messages)} messages</span>
      </div>
      <div className="cols" onMouseLeave={() => setHover(null)}>
        {days.map((d, i) => {
          const v = valueOf(d);
          const h = v <= 0 ? 0 : Math.max((v / max) * 100, 2);
          return (
            <div
              key={d.date}
              className={"col" + (hover === i ? " hot" : "") + (v <= 0 ? " zero" : "")}
              onMouseEnter={() => setHover(i)}
              title={`${shortDay(d.date)} · ${
                metric === "cost" ? money(d.cost) : compact(d.tokens.total) + " tok"
              }`}
            >
              {metric === "breakdown" && v > 0 ? (
                <div className="col-stack" style={{ height: `${h}%` }}>
                  {segsOf(d).map((s) => {
                    const segH = max > 0 ? (s.value / max) * 100 : 0;
                    if (segH < 2) return null;
                    return (
                      <div
                        key={s.key}
                        className={"col-seg " + s.cls}
                        style={{ flex: s.value || 0.001 }}
                        title={`${s.label}: ${compact(s.value)}`}
                      />
                    );
                  })}
                </div>
              ) : (
                <div className="col-fill" style={{ height: `${h}%` }} />
              )}
            </div>
          );
        })}
      </div>
      <div className="timeline-axis">
        <span>{shortDay(days[0].date)}</span>
        <span>{shortDay(days[Math.floor(days.length / 2)].date)}</span>
        <span>{shortDay(days[days.length - 1].date)}</span>
      </div>
    </div>
  );
}

const CAL_WEEKS = 53;
const DAY_MS = 86_400_000;
const WEEKDAY_LABELS = ["", "Mon", "", "Wed", "", "Fri", ""];

/** UTC midnight of `d`, as epoch ms. Transcript dates and "today" are UTC. */
function utcMidnight(d: Date): number {
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
}

function isoDay(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

/** 25th / 50th / 75th percentile (nearest rank) of the non-zero values. */
function quartiles(values: number[]): [number, number, number] {
  const nz = values.filter((v) => v > 0).sort((a, b) => a - b);
  if (nz.length === 0) return [0, 0, 0];
  const at = (p: number) => nz[Math.max(0, Math.ceil(p * nz.length) - 1)];
  return [at(0.25), at(0.5), at(0.75)];
}

/** Intensity 0–4: 0 is no activity, 1–4 are quartile buckets of non-zero days. */
function levelOf(v: number, [q1, q2, q3]: [number, number, number]): number {
  if (v <= 0) return 0;
  if (v <= q1) return 1;
  if (v <= q2) return 2;
  if (v <= q3) return 3;
  return 4;
}

function ActivityCalendar({ byDay, metric }: { byDay: DayUsage[]; metric: Metric }) {
  const { cells, labels } = useMemo(() => {
    const valueOf = new Map<string, number>();
    for (const d of byDay) valueOf.set(d.date, metric === "cost" ? d.cost : d.tokens.total);

    // 53 Sunday-start weeks ending with the week that contains today.
    const today = utcMidnight(new Date());
    const start = today - (new Date(today).getUTCDay() + 7 * (CAL_WEEKS - 1)) * DAY_MS;
    const slots = Array.from({ length: CAL_WEEKS * 7 }, (_, i) => {
      const ms = start + i * DAY_MS;
      const date = isoDay(ms);
      return { ms, date, future: ms > today, value: valueOf.get(date) ?? 0 };
    });
    const q = quartiles(slots.filter((s) => !s.future).map((s) => s.value));
    const cells = slots.map((s) => ({ ...s, level: levelOf(s.value, q) }));

    // Label the week column that holds each month's 1st. The leading partial
    // month gets a label only when there is room before the next one.
    const labels: { col: number; text: string }[] = [];
    for (let col = 0; col < CAL_WEEKS; col++) {
      const first = slots.slice(col * 7, col * 7 + 7).find((s) => s.date.endsWith("-01"));
      if (first) labels.push({ col, text: monthShort(first.ms) });
    }
    if (labels.length === 0 || labels[0].col > 2) labels.unshift({ col: 0, text: monthShort(start) });
    return { cells, labels };
  }, [byDay, metric]);

  const fmt = (v: number) => (metric === "cost" ? money(v) : compact(v) + " tokens");

  return (
    <div className="cal">
      <div className="cal-months">
        {labels.map((l) => (
          <span key={l.col} style={{ gridColumn: `${l.col + 2} / span 4` }}>
            {l.text}
          </span>
        ))}
      </div>
      <div className="cal-grid">
        {WEEKDAY_LABELS.map((w, i) => (
          <span key={`w${i}`} className="cal-weekday">
            {w}
          </span>
        ))}
        {cells.map((c) =>
          c.future ? (
            <span key={c.date} className="cal-cell cal-future" />
          ) : (
            <span
              key={c.date}
              className={`cal-cell cal-l${c.level}`}
              title={`${longDay(c.ms)} · ${c.level === 0 ? "No activity" : fmt(c.value)}`}
            />
          ),
        )}
      </div>
      <div className="cal-legend">
        <span>Less</span>
        {[0, 1, 2, 3, 4].map((l) => (
          <span key={l} className={`cal-cell cal-l${l}`} />
        ))}
        <span>More</span>
      </div>
    </div>
  );
}

function monthShort(ms: number): string {
  return new Date(ms).toLocaleDateString("en-US", { month: "short", timeZone: "UTC" });
}

/** "Tue, Sep 22, 2026" */
function longDay(ms: number): string {
  return new Date(ms).toLocaleDateString("en-US", {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  });
}

function Projects({ projects }: { projects: ProjectUsage[] }) {
  const top = projects.slice(0, 12);
  const max = Math.max(...top.map((p) => p.cost), 0.0001);
  return (
    <div className="projects">
      {top.map((p) => (
        <div className="proj" key={p.path} title={p.path}>
          <div className="proj-bar">
            <div
              className="proj-fill"
              style={{ width: `${Math.max((p.cost / max) * 100, 2)}%` }}
            />
          </div>
          <div className="proj-info">
            <span className="proj-name">{p.project}</span>
            <span className="proj-meta">
              {compact(p.messages)} msg · {compact(p.sessions)} sessions · {ago(p.lastUsed)}
            </span>
          </div>
          <span className="proj-cost">{money(p.cost)}</span>
        </div>
      ))}
    </div>
  );
}

function ProviderTokenComparison({
  providers,
}: {
  providers: ProviderUsage[];
}) {
  const ranked = useMemo(() => {
    return providers
      .filter((p) => p.tokens.total > 0)
      .sort((a, b) => b.tokens.total - a.tokens.total);
  }, [providers]);
  const maxTotal = ranked.length > 0 ? ranked[0].tokens.total : 1;

  if (ranked.length === 0) return null;

  return (
    <div className="prov-compare">
      {ranked.map((p) => {
        const t = p.tokens;
        const segs = [
          { key: "input", value: t.input, cls: "seg-in", label: "Input" },
          { key: "output", value: t.output, cls: "seg-out", label: "Output" },
          { key: "cacheCreation", value: t.cacheCreation, cls: "seg-cw", label: "Cache write" },
          { key: "cacheRead", value: t.cacheRead, cls: "seg-cr", label: "Cache read" },
        ].filter((s) => s.value > 0);
        return (
          <div className="pc-row" key={p.provider}>
            <div className="pc-label">
              <span className={"pt-dot " + providerAccent(p.provider)} />
              <span className="pc-name">{p.displayName}</span>
              <span className="pc-meta">{compact(p.messages)} msg</span>
              <span className="pc-total">{compact(t.total)}</span>
            </div>
            <div className="pc-track">
              {segs.map((s) => (
                <div
                  key={s.key}
                  className={"pc-seg " + s.cls}
                  style={{ width: `${(s.value / maxTotal) * 100}%` }}
                  title={`${s.label}: ${compact(s.value)}`}
                />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function Toggle<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string }[];
}) {
  return (
    <div className="toggle">
      {options.map((o) => (
        <button
          key={o.value}
          className={value === o.value ? "on" : ""}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Loading() {
  return (
    <div className="state">
      <div className="big-spinner" />
      <p>Scanning transcripts…</p>
    </div>
  );
}

function ErrorState({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="state">
      <h2>Couldn’t read usage</h2>
      <p className="muted">{message}</p>
      <button className="refresh" onClick={onRetry}>
        Try again
      </button>
    </div>
  );
}

function EmptyState({ dirs, filtered }: { dirs: string[]; filtered: boolean }) {
  return (
    <div className="state">
      <h2>{filtered ? "No usage for this source" : "No usage yet"}</h2>
      <p className="muted">
        {filtered
          ? "Switch back to All sources, or use this tool — it updates automatically."
          : "No assistant messages found. Use Claude Code or Codex — usage appears here automatically."}
      </p>
      {dirs.length > 0 && <p className="muted"><code>{dirs.join("  ·  ")}</code></p>}
    </div>
  );
}
