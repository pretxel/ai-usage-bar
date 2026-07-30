## Context

`get_usage(provider)` returns a single all-time `UsageReport` built by
`build_report` in `src-tauri/src/lib.rs`: walk → parse → dedup → price →
aggregate. The frontend (`src/App.tsx`) holds one `provider` filter and re-fetches
on a backend `usage-changed` event. The provider summary is deliberately computed
over ALL entries (stable chips); "today" is computed in UTC against the entry
`date` (first 10 chars of the timestamp). There is no time-window concept today.

Each `Entry` already carries a `date: String` (`YYYY-MM-DD`, or `"unknown"`),
which is the natural filter key — no parsing work is added.

## Goals / Non-Goals

**Goals:**
- Scope every total and roll-up to a trailing window of 7 / 15 / 30 days, with
  all-time as the default and unchanged path.
- Keep the change small and within the existing single-file backend / single-file
  frontend structure. No new dependencies, no chart library.
- Preserve the "provider chips are always all-time" invariant.

**Non-Goals:**
- Arbitrary custom date ranges or a date picker — presets only.
- Changing the tray title (stays today's all-time cost) or the watcher.
- Re-anchoring to "latest activity" instead of "now" (see Decisions).

## Decisions

**Filter in the backend, not the frontend.** Models, projects, totals, and active
days are all aggregated server-side; the frontend only receives the rolled-up
report. Re-deriving a windowed view client-side would mean shipping raw entries and
re-implementing aggregation in TS. Instead, add a `days` argument to `get_usage`
and apply it as a guard inside the existing aggregation loop in `build_report`.
*Alternative considered:* client-side filtering of `byDay` only — rejected because
it can't rescope `models`/`byProject`/totals, so the dashboard would be
inconsistent.

**Represent the range as `Option<u32>` days; `None`/`0` = all-time.** Maps cleanly
from the frontend (`"all" | 7 | 15 | 30`) and keeps the all-time path a no-op.
*Alternative:* a string enum in Rust — rejected as more ceremony than an integer
window width buys.

**Anchor the window to the current UTC date**, computing the cutoff once as
`today_utc - (days - 1)` and keeping entries with `date >= cutoff`. This matches
the existing `today` computation (also `Utc::now()`), so "Last 7 days" and "Today"
agree at the boundary. String comparison on `YYYY-MM-DD` is correct lexicographic
date order, so no date parsing is needed. *Alternative:* anchor to the latest
activity date (as `buildTimeline` does for the chart) — rejected because "last 7
days" should mean calendar-recent, not "7 days around my last session."

**Provider summary stays over all entries.** The provider loop already runs before
the filtered aggregation loop and ignores the provider filter; the `days` guard is
added only to the second loop, so chips remain all-time for free.

**Echo the active range in the report.** Add a `range` field (e.g. the day count or
`"all"`) to `UsageReport` so the UI can label hints from the response and keep
`types.ts` in lockstep with the Rust struct.

**Frontend: a `Toggle`-style selector + `range` state.** Reuse the existing
`Toggle` pattern (or a sibling of `ProviderTabs`) for All / 30 / 15 / 7, hold a
`range` state next to `filter`, include it in the `invoke` args and the `load`
dependency list so it re-fetches like the provider filter. Panel hints that say
"Last X days" derive from the selected range.

## Risks / Trade-offs

- **A finite range plus "today" with no recent activity shows mostly empty bars** →
  acceptable and honest; the window is calendar-anchored by design. The timeline
  can pass the selected width into `buildTimeline` so the axis matches the window.
- **Undated (`"unknown"`) entries vanish in a finite window** → correct: they
  can't be placed in a calendar window. They still appear in all-time. Documented
  in the spec.
- **Type drift between Rust struct and `types.ts`** → mitigated by adding the
  `range` field in both in the same change and the existing camelCase serde rule.
- **Backend test coverage** → add a `cargo test` that builds a finite-range report
  and asserts its totals are ≤ the all-time totals, mirroring `codex_only_filter`.

## Migration Plan

Additive and backward-compatible: `days` is optional, default all-time reproduces
current output. No data migration. Rollback = revert the change; the command still
accepts the old single-argument call shape because `days` is optional.
