# Performance, cache and memory audit (2026-10-16)

Baseline: main commit e89f355819ec07240185a421c8c7d02da15c73d3.
This report is a code audit plus deterministic Mock tests, NOT an actual
Windows/WebView frame-rate or peak-RSS benchmark. No real Telegram account or
Session was accessed by the tests.

## Implemented changes and known limitations

| Area | Prior behavior | Optimization |
|---|---|---|
| Scan ranking | Re-filter, clone and re-sort all eligible Posts at every 500ms progress update | Evaluate each completed Post once; a bounded BinaryHeap retains top K (K<=1000) using O(P log K) work and O(K) Post memory |
| Progress IPC | Up to full output_limit included in each result | Running snapshots include at most min(40,K) Posts; final still returns full top K |
| History cursor dedup | HashSet of MessageKey (two heap-allocated strings); repeated normalization of overlapping IDs | Per-chat numeric message IDs, checking duplicates before normalization; O(P) numeric ID storage still bounded by ScanLimits |
| Vue output | Up to 1000 Post cards mounted at once; immutable scan results deeply proxied | Fixed 40-card result pagination and immutable snapshots using Vue markRaw |
| Image display | Album expansion eagerly loaded up to 24MiB per photo via base64 IPC | First show thumbnail/minithumbnail; fetch full-resolution photo only on explicit user action |
| Media scheduler | Single global FIFO, no completed thumbnail cache | Max 2 global active calls, max 1 per account, avatar > thumbnail > full priority; abort queued work on component unmount |
| WebView media cache | Only a separate avatar cache | 128-item, 12MiB-character LRU; maximum 512KiB per entry, 5min positive/10s negative TTL; invalidated on Session/generation reset |
| Chat details | Hundreds of sequential getChat calls | 32-message request batches, preserving order and membership semantics |
| Scan result disposal | Switching chats hid old result in Vue but left native AccountRuntime.results allocated | release_query invalidates only matching run, cancels the old scan and frees backend results; late native events rejected |

## Correctness

Top-K and full-sort API share the exact comparator (NULL placement, ascending
or descending, deterministic date/chat/message tie-break). Filtering is
performed before ranking and matches use identical three-valued semantics.
Examined/matched counts are full-scope counts even when progressive UI
displays only 40 Posts. Forwarding selection continues to verify run and
Post keys in Rust.

All resource protections of Scan Engine V2 remain: 100,000 raw messages,
2,500 history pages, 300 seconds and bounded Album boundary allowance.
These limits are independent of user-facing Post scope.

## Verified by deterministic automation (not live-load claims)

- Top-K equals full sort for ascending, descending, NULL-heavy data.
- 12,000-message Mock history: exact top 1000 results and correct counts,
  at most 40 progressive Posts, bounded pagination.
- 70-chat mock: batch sizes 32/32/6 and correct order.
- Media scheduler: bounded concurrency, priority, cancel queued requests,
  discard stale active result, cache eviction, dedup and invalidation.
- Chat switch: only the matching native result released, newer run intact.

Commands: pnpm test; pnpm build; cargo test --manifest-path
src-tauri/Cargo.toml --no-default-features --lib --tests; cargo clippy
--manifest-path src-tauri/Cargo.toml --no-default-features --all-targets
-- -D warnings.

## Important remaining work

1. Final ScanResult can still transfer up to 1000 Posts with full member
   metadata in a single IPC message. Future: versioned lightweight progress,
   paginated native result fetch by run ID and on-demand member details.
2. photo_thumbnail still calls TDLib synchronous downloadFile while holding
   the per-account lifecycle mutex; a slow active download can stall chat and
   other operations on that same account. The new queue bounds contention
   and avoids automatic full-size loads; it does not make native I/O async.
3. TDLib-managed disk files are NOT reclaimed by the WebView's LRU. Add
   opt-in storage optimization and statistics through TDLib's supported
   APIs; do not manually remove live Session DBs or files.
4. Backend sent_updates/unknown_sends and frontend sendUpdates lack a
   mature retention/GC policy. Confirmed send receipts and Unknown recovery
   require a separate lifecycle and retention design.
5. 40 expanded Album cards can still be heavy. Profile a variable-height
   viewport virtualizer before adopting it; pagination avoids most cases.
6. Account lifecycle locks protect close/logout correctness and require
   careful redesign before allowing true parallelism within one account.

## Windows pressure-validation matrix

Compare the baseline and PR builds on the same machine; record median and
p95 latency, UI main-thread long tasks, working set, WebView JS heap,
TDLib memory, disk cache size and scroll smoothness:

- 1k / 10k / 50k / 100k raw messages, asc/desc, many NULL metrics,
  highly selective and deeply nested filters; compare top20 vs top1000.
- Albums crossing pagination and date boundaries; rapid query replacement
  and cancellation; verify no stale or cross-account results.
- Expand/collapse 10-20 Albums rapidly, then switch chat/account; ensure
  queued media work is cancelled, cache bounded and originals only load
  after a user click.
- Concurrent profiles: scan A while browsing chats/media on B, and then
  compare same-account chat/media latency under an active scan.
- Repeat ten times to see whether the working set reaches a plateau.
  Garbage collectors are nondeterministic; RSS not immediately falling
  is not by itself proof of a leak.

No FPS or memory improvement percentage should be published without an
actual benchmark. Tests here are synthetic and cannot establish UX smoothness
on a user's Windows machine.
