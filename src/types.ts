/**
 * TypeScript mirrors of the Rust structs in `src-tauri/src/lib.rs`.
 *
 * Rust serializes with `#[serde(rename_all = "camelCase")]`, so every
 * snake_case field added in `lib.rs` must appear as camelCase here.
 * Provider colors and model-family buckets are mapped via `familyOf()`
 * and `providerAccent()`.
 */

export interface Tokens {
  input: number;
  output: number;
  cacheCreation: number;
  cacheRead: number;
  total: number;
}

export interface ModelUsage {
  provider: string;
  model: string;
  displayName: string;
  messages: number;
  tokens: Tokens;
  cost: number;
}

export interface DayUsage {
  date: string;
  cost: number;
  messages: number;
  tokens: Tokens;
}

export interface ProjectUsage {
  project: string;
  path: string;
  messages: number;
  sessions: number;
  cost: number;
  tokens: Tokens;
  lastUsed: string;
}

export interface ProviderUsage {
  provider: string;
  displayName: string;
  cost: number;
  messages: number;
  sessions: number;
  tokens: Tokens;
  available: boolean;
}

export interface UsageReport {
  generatedAt: string;
  filter: string;
  range: string;
  dataDirs: string[];
  fileCount: number;
  totalCost: number;
  todayCost: number;
  avgCostPerDay: number;
  activeDays: number;
  totalMessages: number;
  totalSessions: number;
  totalTokens: Tokens;
  providers: ProviderUsage[];
  models: ModelUsage[];
  byDay: DayUsage[];
  byProject: ProjectUsage[];
}

export type ProviderFilter = "all" | "claude" | "codex" | "opencode";

/** Trailing day-range window for the report. "all" = all-time. */
export type DayRange = "all" | 7 | 15 | 30;

export type ModelFamily =
  | "fable"
  | "mythos"
  | "opus"
  | "sonnet"
  | "haiku"
  | "codex"
  | "opencode"
  | "other";

/** Colour bucket for a model id (used for bar/dot accent classes). */
export function familyOf(model: string): ModelFamily {
  const m = model.toLowerCase();
  if (m.includes("fable")) return "fable";
  if (m.includes("mythos")) return "mythos";
  if (m.includes("opus")) return "opus";
  if (m.includes("sonnet")) return "sonnet";
  if (m.includes("haiku")) return "haiku";
  if (m.includes("opencode")) return "opencode";
  if (
    m.includes("gpt") ||
    m.includes("codex") ||
    /^o\d/.test(m) ||
    m.includes("-o3") ||
    m.includes("-o4")
  )
    return "codex";
  return "other";
}

/** Accent class for a provider chip. */
export function providerAccent(provider: string): string {
  if (provider === "codex") return "prov-codex";
  if (provider === "claude") return "prov-claude";
  if (provider === "opencode") return "prov-opencode";
  return "prov-other";
}
