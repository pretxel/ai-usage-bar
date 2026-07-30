# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Tauri v2 desktop app (React 19 + TypeScript frontend, Rust backend) that reads local
AI coding-assistant transcripts, prices the token usage, and visualizes spend. macOS-only
for now (menu-bar tray + bundle target). No network, no telemetry — everything is parsed
on disk.

Naming is inconsistent across layers — they all refer to the same app: product name
`Token Tracker`, npm package `token-tracker`, Rust crate `ai-usage-bar` / lib
`ai_usage_bar_lib`, repo dir `ai-usage-bar`.

## Commands

```bash
pnpm install
pnpm tauri dev      # run the desktop app (Vite on :1420 + Rust)
pnpm tauri build    # bundle .app / .dmg under src-tauri/target/release/bundle/
pnpm dev            # frontend only, no Tauri shell (backend invoke() calls fail)
pnpm build          # tsc typecheck + vite build

# Backend tests — run against your REAL ~/.claude and ~/.codex data, not fixtures
cd src-tauri && cargo test report_smoke -- --nocapture
cd src-tauri && cargo test codex_only_filter
```

There is no JS test suite and no linter configured.

## Architecture

Two single-file layers plus a thin TS type mirror. The whole app is one Tauri command,
`get_usage(provider)`, returning a `UsageReport`.

### Backend — `src-tauri/src/lib.rs` (the entire backend lives here)

Pipeline: walk transcript dirs → parse JSONL per source → dedup → price → aggregate.

- **Sources & dirs** — `claude_dir()` (`~/.claude/projects`, override `CLAUDE_CONFIG_DIR`)
  and `codex_dir()` (`~/.codex/sessions`, override `CODEX_HOME`). Missing dirs are skipped,
  not errors. `collect_entries()` walks both trees in parallel with rayon.
- **Parsing** — `parse_claude_file` emits one `Entry` per assistant message that has a
  `usage` block. `parse_codex_file` is stateful: it tracks model/cwd/session from
  `session_meta` and `turn_context` lines, then emits an `Entry` per `token_count`
  `event_msg` using `last_token_usage` (per-turn delta). Codex `cached_input_tokens` maps
  to `cache_read`; `input − cached` to `input`; Codex has no cache-write.
- **Dedup** — by `Entry.dedup_key`. Claude keys are `c:{requestId}:{messageId}` (collapses
  retries/sidechains that repeat usage); Codex keys are line-unique (`x:{path}#{idx}`).
- **Pricing** — `claude_pricing()` / `openai_pricing()` return
  `(input, cache_write, cache_read, output)` USD per 1M tokens, matched by substring on the
  model id. Unknown / `<synthetic>` models price at $0 but still count tokens. **These are
  the knobs to edit when list prices change.**
- **Aggregation** — `build_report()` rolls entries up by model / day / project / provider.
  Two things to know: the `providers` summary is always computed over ALL entries
  (ignores the filter) so the UI can show stable per-source chips; "today" is computed in
  **UTC** to match transcript timestamps.

### Frontend — `src/App.tsx` (all components in one file)

`App` calls `invoke("get_usage", { provider: filter })`, holds the `UsageReport`, and
re-fetches when the backend emits `usage-changed`. All charts are hand-rolled CSS bars /
grids — **no chart library**; don't add one. `Timeline` and `MonthMatrix` fill missing
calendar days client-side from the sparse `byDay` array.

- `src/types.ts` — TS interfaces that MUST mirror the Rust structs. Rust serializes
  `#[serde(rename_all = "camelCase")]`, so a snake_case field added in Rust arrives as
  camelCase here. Add a field in `lib.rs` → add it in `types.ts` too.
- `src/format.ts` — `money` / `compact` / `prettyDate` / `ago` display helpers.

### Live updates — `start_watching()` in `lib.rs`

A debounced (`notify` + `notify-debouncer-full`, 800ms) recursive watcher on both transcript
dirs. On a create/modify/remove it updates the tray title and emits `usage-changed`. The
debouncer is stashed in Tauri state so it outlives setup.

### Tray

Set up in `run()`. Title is today's combined estimated cost (`tray_title()`). Menu: open
window / quit. The tray, not the window, is the app's primary surface.

## Conventions

- Adding a provider or model: extend the pricing fn + `display_name` fn in `lib.rs`, and the
  `familyOf` color bucket in `types.ts`.
- Keep cost framing honest in UI copy — these are **estimates** from public list pricing, not
  a billing statement (existing footer/empty-state copy reflects this).
