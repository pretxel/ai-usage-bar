# token-visualizations Specification

## Purpose
Extend the frontend with richer token-usage visualizations — per-model token composition mini bars, a token-type stacked timeline, and a provider token comparison panel — using existing data from the backend report.

## Requirements

### Requirement: Per-model token composition bar
Each row in the "Spend by model" panel SHALL display a small stacked horizontal bar below the cost track showing the proportion of input, output, cache-read, and cache-write tokens for that model. The bar SHALL use the existing `.seg-in`, `.seg-out`, `.seg-cw`, `.seg-cr` CSS color classes. Zero-value segments SHALL be omitted. The bar SHALL be 6px in height and span the full width of the track area.

#### Scenario: Model with all token types
- **WHEN** a model has 1000 input, 500 output, 200 cache-read, and 100 cache-write tokens
- **THEN** the mini bar shows four colored segments with widths proportional to 1000/1800, 500/1800, 200/1800, 100/1800

#### Scenario: Model with only input and output
- **WHEN** a model has 800 input and 400 output tokens but zero cache tokens
- **THEN** the mini bar shows only two segments (input and output), filling 800/1200 and 400/1200 of the bar width

#### Scenario: Single-token-type model
- **WHEN** a model only has output tokens
- **THEN** the mini bar is a single solid segment of the output color

### Requirement: Token-type stacked timeline
The "Daily activity" timeline SHALL support a third display mode (`"token-breakdown"`) in addition to the existing `"cost"` and `"total-tokens"` modes. In this mode, each day column SHALL be a vertical stack of colored segments representing input, output, cache-read, and cache-write tokens. The total column height SHALL be proportional to the day's total token count. Segments narrower than 2px SHALL be hidden. The mode selector SHALL be a three-option selector (cost / tokens / breakdown) replacing the existing two-state toggle.

#### Scenario: Cost mode unchanged
- **WHEN** the mode is `"cost"`
- **THEN** the timeline renders as before — each column height proportional to the day's cost

#### Scenario: Token breakdown mode shows stacked segments
- **WHEN** the mode is `"token-breakdown"`
- **THEN** each day column has up to four stacked segments colored by token type, stacking upward in the order: input (bottom), output, cache-read, cache-write (top)

#### Scenario: Zero-value segments are hidden
- **WHEN** a day has zero cache-write tokens
- **THEN** no cache-write segment is rendered in that day's column

### Requirement: Provider token comparison panel
The dashboard SHALL display a new panel titled "Tokens by provider" comparing token composition across available providers. Each provider SHALL have a horizontal stacked bar showing input, output, cache-read, and cache-write proportions, labeled with the provider name, total token count, and message count. Bar widths SHALL be proportional to the provider with the highest total token count. Providers with zero tokens SHALL be excluded.

#### Scenario: Multiple providers with usage
- **WHEN** two providers have non-zero token usage
- **THEN** each provider row shows a stacked bar proportional to the larger provider's total

#### Scenario: Provider with zero tokens
- **WHEN** a provider has zero tokens across all categories
- **THEN** that provider is not shown in the panel

### Requirement: All visuals use existing color scheme
All new visualizations SHALL reuse the existing `.seg-in` (violet, input), `.seg-out` (teal, output), `.seg-cw` (amber, cache-write), and `.seg-cr` (gray, cache-read) CSS classes. No new token-type color variables SHALL be introduced.
