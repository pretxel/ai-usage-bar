# Contributing to Token Tracker

Thanks for your interest! Here's how to get started.

## Setup

```bash
# Prerequisites: Node.js 20+, pnpm, Rust (stable)
pnpm install
pnpm tauri dev    # run the app locally
```

See the [README](./README.md) for more detail on the stack and architecture.

## Finding work

Check out:

- [Open issues](https://github.com/edselserrano/token-tracker/issues)
- [Discussions](https://github.com/edselserrano/token-tracker/discussions)

Pick something tagged `good first issue` if you're new.

## Working on a change

1. Fork and branch from `main`.
2. Make your change. Keep each PR focused on one thing.
3. Run backend tests against your real local data:
   ```bash
   cd src-tauri && cargo test report_smoke -- --nocapture
   ```
4. Keep the PR description concise — what, why, and how you tested it.

## Architecture notes

The entire backend is a single Rust file (`src-tauri/src/lib.rs`). The pipeline:
walk transcript dirs → parse JSONL → dedup → price → aggregate. Costs are
**estimates** from public list pricing. No network, no telemetry.

The frontend is a single React component file (`src/App.tsx`) with hand-rolled
CSS bar charts — no chart library.

When adding a Rust struct field, add the matching `camelCase` field to
`src/types.ts`. Field names in the report JSON are always camelCase.

## Pricing updates

If OpenAI or Anthropic list prices change, edit the relevant function in
`src-tauri/src/lib.rs`:

- `claude_pricing()` — Anthropic models
- `openai_pricing()` — OpenAI / Codex models

Tests in the `tests` module validate the pricing tables and help catch regressions.

## What this project values

- **Privacy first.** No network calls, no telemetry, no analytics. Every change
  must preserve this.
- **Honest framing.** The UI says "Estimated spend" and the footer notes these
  are estimates — not a billing statement.
- **Works with real data.** Tests run against the actual transcript directories
  on your machine, not fixtures. If your test needs data, use the tools yourself
  to generate transcripts.
