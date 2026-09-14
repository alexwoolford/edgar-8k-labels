# Capture contract (work sqlite)

Decision: **capture the 8-K / 8-K/A item trickle, not filing bodies and not the daily EDGAR tar.gz feed.** Work sqlite is `{--db}` (prod `/var/lib/edgar-8k-labels/edgar-8k-labels.sqlite`). Logical name `edgar-8k-labels`. There is no published `current/` copy. The collector watches work sqlite only.

Canonical contract: [capturable-state design principles](https://github.com/alexwoolford/capturable-state/blob/main/docs/design-principles.md) §0 / §7 and [datetime.md](https://github.com/alexwoolford/capturable-state/blob/main/docs/datetime.md). Capture the trickle, not the hose.

Pin: `capturable-state` git tag `v0.1.1` (not a path dep; do not copy `src/*.rs`).

## What is captured

| Table / stream | Capture? | Mode | Why |
| --- | --- | --- | --- |
| `filings` | **yes** | after | Product. Key `accession` |
| `ingest_runs` | **yes** | after | Did last night finish? |
| HTTP cache / submissions JSON | **no** | — | In-memory per ingest day |
| Filing `.txt` bodies | **no** | — | Hose-adjacent |
| `company_tickers_exchange.json` | **no** | — | Ticker denormalized onto `filings` |
| `_outbox` | platform | — | Generated |

Identity: accession is stable (P1). Filings are insert-once; an identical rerun must not emit a new outbox row (`ON CONFLICT … WHERE` any column differs). An 8-K/A is a **new** accession. Soft-delete unused in v1.

`items` is comma-separated TEXT, sorted unique (`1.01,2.01`). Empty → SQL NULL. Do not guess. Mosaic unnests with `string_to_array`.

These are **labels**, not leads. Item 2.01 is the disclosure, not a mosaic-generated signal.

Do not `collect --snapshot` this database.

## Clocks

| Layer | Columns | Type |
| --- | --- | --- |
| Facts | `filed_date` | TEXT `YYYY-MM-DD` |
| Facts | run `started_at` / `finished_at` | TEXT `YYYY-MM-DDTHH:MM:SSZ` |
| Envelope | `_outbox.ts`, `deleted_at` | INTEGER Unix seconds |

## Announce / nudge

`install()` on work sqlite. `ReadWritePaths` include `/var/lib/state-capture/announce` (required when the collector is present) and `-/run/state`. `edgar` must be in group `state-capture`. Collector host inventory lives in mosaic `deploy/ct-firehose/`, not in this crate.
