## 1. Backend — windowed aggregation (`src-tauri/src/lib.rs`)

- [x] 1.1 Add a `range` field to `UsageReport` (echo selected window; `"all"` or the day count) and keep serde camelCase
- [x] 1.2 Change `build_report` signature to accept the day window (e.g. `days: Option<u32>`), treating `None`/`0` as all-time
- [x] 1.3 Compute the cutoff date once as `today_utc - (days - 1)` using `Utc::now()`; keep the provider summary loop over ALL entries (unchanged)
- [x] 1.4 In the filtered aggregation loop, skip entries when a finite window is set and `e.date == "unknown"` or `e.date < cutoff`
- [x] 1.5 Set `range` in the returned `UsageReport`; keep `tray_title()` on the all-time path (pass all-time)
- [x] 1.6 Update `get_usage` to accept and forward the `days` argument

## 2. Backend — tests

- [x] 2.1 Add a `cargo test` (mirroring `codex_only_filter`) that builds a finite-range report and asserts its totals are ≤ the all-time totals and `byDay` holds no entry older than the cutoff

## 3. Frontend types (`src/types.ts`)

- [x] 3.1 Add the `range` field to the `UsageReport` interface and a `DayRange` type (`"all" | 7 | 15 | 30`) to mirror the Rust struct

## 4. Frontend UI (`src/App.tsx`)

- [x] 4.1 Add `range` state in `App` and include it in the `invoke("get_usage", …)` args and the `load` dependency list so it re-fetches like the provider filter
- [x] 4.2 Add a day-range selector control (All time / 30 / 15 / 7) — reuse the `Toggle`/`ProviderTabs` pattern — and wire its `onChange`
- [x] 4.3 Pass `range` down to `Dashboard` and update range-dependent panel hints (e.g. "Daily activity") to reflect the active window; pass the window width into `buildTimeline` so the axis matches

## 5. Verify

- [x] 5.1 `cd src-tauri && cargo test -- --nocapture` passes against real local data
- [x] 5.2 `pnpm build` (tsc + vite) passes with no type errors
- [ ] 5.3 `pnpm tauri dev` — switch each range and confirm totals/byDay/models rescope while provider chips stay all-time
