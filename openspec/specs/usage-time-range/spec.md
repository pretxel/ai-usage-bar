# usage-time-range Specification

## Purpose
TBD - created by archiving change add-day-range-filter. Update Purpose after archive.
## Requirements
### Requirement: Day-range scoping of the usage report

The system SHALL accept a day-range argument on the usage report request with the
values: all-time, 30, 15, and 7 days. When a finite range (7/15/30) is supplied,
the report's totals and roll-ups (total cost, total tokens, total messages,
sessions, models, by-day, by-project, active days, average per day) SHALL be
computed only over entries whose date falls inside the window. When all-time is
supplied (or the argument is omitted), the report SHALL be computed over all
entries — identical to the prior behavior.

#### Scenario: Finite range scopes the aggregates
- **WHEN** the report is requested with a 7-day range
- **THEN** every total and roll-up in the report reflects only entries dated
  within the last 7 days, and entries outside that window are excluded

#### Scenario: All-time is the default
- **WHEN** the report is requested with no range argument or the all-time value
- **THEN** the report aggregates over all entries, unchanged from prior behavior

### Requirement: Window anchoring

The system SHALL anchor a finite window to the current UTC date. "Last N days"
SHALL mean the N calendar days ending on the current UTC date, inclusive — an
entry is in the window when its date is on or after `(today_utc - (N - 1) days)`.
Entries with no parseable date SHALL be excluded from any finite window.

#### Scenario: Inclusive trailing window
- **WHEN** the current UTC date is 2026-06-07 and a 7-day range is selected
- **THEN** entries dated 2026-06-01 through 2026-06-07 are included and entries
  dated 2026-05-31 or earlier are excluded

#### Scenario: Undated entries dropped from a finite window
- **WHEN** a finite range is selected and an entry has no parseable date
- **THEN** that entry is excluded from the report

### Requirement: Provider summary independent of range

The provider summary chips SHALL continue to be computed over all entries
regardless of the selected day range, so per-source totals stay stable as the user
changes the range. The day-range argument SHALL be composable with the existing
provider filter.

#### Scenario: Provider chips ignore the range
- **WHEN** a finite range is selected
- **THEN** each provider chip still shows that provider's all-time total

#### Scenario: Range and provider filter combine
- **WHEN** a provider filter and a finite range are both active
- **THEN** the report aggregates only entries that match the provider AND fall
  inside the window

### Requirement: Range selector in the UI

The dashboard SHALL present a day-range selector exposing All time, Last 30 days,
Last 15 days, and Last 7 days. Selecting a range SHALL re-fetch the report for that
window, and range-dependent panel hints SHALL reflect the active window. The
default selection SHALL be All time.

#### Scenario: Selecting a range updates the dashboard
- **WHEN** the user selects "Last 30 days"
- **THEN** the dashboard re-fetches and displays figures scoped to the last 30
  days, and the range-dependent hints reflect that window

#### Scenario: Default selection
- **WHEN** the dashboard first loads
- **THEN** the day-range selector defaults to All time

