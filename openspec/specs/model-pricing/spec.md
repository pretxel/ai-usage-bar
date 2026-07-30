# model-pricing Specification

## Purpose
Pricing and identification of known AI models: mapping a model id to USD-per-1M-token rates, a human-readable display name, and a UI color family. Rates are hard-coded estimates from public list pricing (no network lookup); unknown models price at $0 but still count tokens.
## Requirements
### Requirement: Fable 5 token pricing
The system SHALL price Claude model ids containing the substring `fable` (case-insensitive) at 10.00 USD per 1M input tokens, 12.50 USD per 1M cache-write tokens, 1.00 USD per 1M cache-read tokens, and 50.00 USD per 1M output tokens.

#### Scenario: Fable 5 entry is priced
- **WHEN** a transcript entry with model `claude-fable-5` reports 1,000,000 input, 1,000,000 cache-write, 1,000,000 cache-read, and 1,000,000 output tokens
- **THEN** its estimated cost is 73.50 USD (10.00 + 12.50 + 1.00 + 50.00)

#### Scenario: Suffixed Fable id matches
- **WHEN** a transcript entry reports model `claude-fable-5[1m]`
- **THEN** it is priced at the Fable 5 rates, not the 0 USD unknown-model fallback

### Requirement: Fable 5 display name
The system SHALL render Claude model ids of the `fable` family with the display name `Fable <version>`, deriving the version from the numeric tokens following the family token.

#### Scenario: Bare id
- **WHEN** the model id is `claude-fable-5`
- **THEN** the display name is `Fable 5`

### Requirement: Fable model family for UI grouping
The frontend SHALL classify model ids containing `fable` (case-insensitive) into a distinct `fable` model family with its own chart color, separate from `opus`, `sonnet`, `haiku`, `codex`, and `other`.

#### Scenario: familyOf classification
- **WHEN** `familyOf("claude-fable-5")` is called
- **THEN** it returns `"fable"`

### Requirement: Existing model pricing is unchanged
Adding the Fable family SHALL NOT alter pricing, display names, or family classification for `opus`, `sonnet`, `haiku`, or unknown models; unknown models SHALL continue to price at 0 USD while still counting tokens.

#### Scenario: Unknown model still free
- **WHEN** a transcript entry reports model `<synthetic>`
- **THEN** its estimated cost is 0 USD and its tokens are still aggregated
