## Why

The dashboard only ever shows all-time totals. There's no way to ask "what did I
spend in the last week?" — the most common question for someone watching cost.
A day-range filter (last 7 / 15 / 30 days, plus all-time) lets users scope every
figure to a recent window without leaving the app.

## What Changes

- Add a **day-range filter** with presets: All time, Last 30 days, Last 15 days,
  Last 7 days. Default stays **All time** (current behavior).
- `get_usage` gains a `days` argument. When set, the report's cost/token/model/
  day/project aggregates and totals are computed only over entries inside the
  window; when unset (all-time), behavior is unchanged.
- The window is anchored to the current UTC date (matching the existing "today"
  logic): "last N days" = the N calendar days ending today, inclusive. Entries
  with no parseable date are excluded from any finite window.
- The provider summary chips stay computed over **all** entries (unchanged) so the
  per-source totals remain stable regardless of the selected range.
- Frontend adds a range selector control; selecting a range re-fetches and the
  panel hints ("Last X days") reflect the active window.

## Capabilities

### New Capabilities
- `usage-time-range`: scoping the usage report to a preset trailing day window
  (all-time / 30 / 15 / 7 days), including how the window is anchored, which
  aggregates it affects, and how the UI selects and reflects it.

### Modified Capabilities
<!-- No existing specs; nothing to modify. -->

## Impact

- `src-tauri/src/lib.rs` — `get_usage` signature, `build_report` filtering, the
  tray title path (stays all-time), new backend test.
- `src/types.ts` — mirror the new request param / any echoed range field.
- `src/App.tsx` — range state, selector control, re-fetch wiring, panel hint copy.
- No new dependencies. Still offline, no network/telemetry.
