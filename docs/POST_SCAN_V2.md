# Post Scan Engine V2

## Contract (QuerySpec version 2)

Post is the unit of every user-visible scope, filter, sort, output limit, and
selection. TDLib Message is only the history pagination and resource unit.

- `recent {count:N}`: most recent N logical Posts (before filtering).
- `dates {from,to,limit_posts:Some(N)}`: latest N Posts with Post.date in
  the half-open UTC interval `[from,to)`. `date_window_exhausted=false` when
  N was reached before the older boundary, not a claim about the entire interval.
- `dates {from,to,limit_posts:None}`: all Posts in the interval subject to
  independent, explicit resource limits.
- `Post.date` is the greatest member timestamp, not the caption owner's
  timestamp. All discovered members remain in the Post even if some fall outside
  the user's date interval.
- Filtering, sorting and `output_limit` apply only *after* the Post scope is
  established; an output limit never truncates source scanning.
- Old QuerySpec version 1 is rejected, not silently reinterpreted. The obsolete
  `budget` key cannot replace `limit_posts`.

## Architecture

```text
TDLib getChatHistory / getChatMessageByDate
  -> scan/history.rs       cursor, retries, dedup, optional date seek
  -> scan/accumulator.rs   cross-page contiguous Album assembly
  -> scan/scope.rs         Post-based Recent/Dates inclusion and stopping
  -> scan/mod.rs           cancellation, budget, progress, result snapshots
  -> core::query_posts     filters, metrics, sort, output limit
  -> UI ScanCoverage + PostCompleteness
```

`core::assemble` remains a pure grouping/metric routine (max views/forwards
and caption-bearing owner of comments/reactions). Streaming accumulation uses
it only for completed groups, assigning boundary evidence separately.

`getChatMessageByDate(to-1)` is an optional acceleration. The reader uses up
to 99 newer neighbors with `getChatHistory(offset=-99)`, but only keeps this
starting point when the returned window both contains the date anchor and
shows an out-of-range newer message. Otherwise the reader restarts at history
HEAD to avoid losing eligible newer Posts. This optimization is not a
proof of arbitrary historical chronology. Standard history traversal starts at
HEAD.

## Two independent statuses

- ScanStatus: `Running / Complete / BudgetExceeded / Interrupted / Error`.
- StopReason: concrete reason, such as `PostLimitReached`,
  `DateLowerBoundReached`, `HistoryExhausted`, `CursorStalled`,
  `ResourceLimitReached`, or `UserCancelled`.
- `coverage.scope_complete`: the requested *scope* was covered.
- `coverage.date_window_exhausted`: whether all Posts in the interval were
  traversed; may be false when the requested top N was satisfied.
- `stats`: raw messages fetched, examined Posts, matched/returned Posts, pages.

PostCompleteness is independent:

- **Verified**: a standalone Message is self-contained.
- **Assumed**: contiguous Album members and both boundaries were observed
  (or the older bound was the end of history). TDLib does not supply an Album
  member count; this is deliberately *not* Verified.
- **Unknown**: an Album's boundary was not proven (interrupted, budget
  exhaustion, ambiguous starting edge).
- **Truncated**: reserved for *known* omission of specific members; never
  inferred from a small member count or applied to all Albums indiscriminately.

An Album may be Assumed while ScanStatus is Complete. An unfinished Album can
be Unknown while ScanStatus is BudgetExceeded or Interrupted.

## Resource and forward safety

Resource limits are internal (`ScanLimits`: up to 100,000 raw messages,
2,500 history pages, 300 seconds; up to 64 additional messages exclusively
for an open Album boundary). They do not change the Post count in QuerySpec.
If limits are reached before the target scope, use BudgetExceeded, not
Complete. User cancellation and stalled history are Interrupted. The current
run's progress and results remain associated with its account and run ID.

The UI flags Unknown/Truncated Posts. Both preview and actual forwarding
enforce a Rust-side `allow_incomplete` flag, default false. The frontend
must obtain a separate, explicit confirmation before setting it true.
The existing safe-close/Unknown send receipt model and post-forward safeguards
remain intact.

## Known limitations

- A continuous history is strong circumstantial evidence for an Album, not
  cryptographic verification of every member.
- Message IDs provide paging order; pathological nonmonotonic message dates
  across different Posts may weaken early date-bound stopping.
- The date locator falls back to head scanning when upper context is not safe.
  Extremely old history can therefore hit a resource limit instead of silently
  skipping unobserved Posts.
- No real Telegram account or Session is needed for regression tests.

## Required local QA

Run `pnpm test`, `pnpm build`, and
`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib --tests`,
then `cargo clippy --manifest-path src-tauri/Cargo.toml --no-default-features --all-targets -- -D warnings`.

On Windows, check old/new Date queries, high-volume channels, Album boundaries,
preview and confirmation for Unknown/Truncated posts, and whether date seeks
and fallbacks visibly explain resource limits. Do not use live Telegram
forwarding during automated tests.
