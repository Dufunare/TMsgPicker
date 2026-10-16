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
| Image display | Album expansion previously required per-photo clicks to load originals | Expanding a group immediately authorizes automatic full-resolution photo loading; show thumbnails/minithumbnails until originals arrive, and automatically restore cached originals on reopen |
| Media scheduler | Single global FIFO, no completed thumbnail cache | Max 2 global calls / 1 per account; original > avatar > thumbnail queue priority; per-file in-flight sharing even with AbortSignals, subscriber-level abort, cancel queued jobs when all subscribers leave |
| WebView media cache | Only a separate avatar cache | Thumbnail LRU: 128 items / 12MiB chars / 512KiB each. Original-photo LRU: 8 items / 64MiB chars / 36MiB per entry. Both have TTL and per-account Session/generation invalidation; no unbounded full-resolution retention |
| Chat details | Hundreds of sequential getChat calls | 32-message request batches, preserving order and membership semantics |
| Scan result disposal | Switching chats hid old result in Vue but left native AccountRuntime.results allocated | release_query invalidates matching run, cancels old scan, and frees backend results outside locks; start_query/release_query/cancel_query dispatch on blocking workers to avoid holding Tauri UI thread; late events rejected |

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
- Media scheduler: bounded concurrency, full-photo priority, multiple AbortSignal
  subscribers sharing one file request, independent cancellation, quick
  collapse/reopen, separate bounded thumbnail/original cache eviction and
  per-session invalidation.
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
   other operations on that same account. The scheduler bounds concurrency,
   shares requests, and cancels unstarted work but cannot forcibly interrupt
   an already submitted TDLib synchronous download. Automatic original loading
   can increase disk/network work when many Albums are expanded.
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
  originals load automatically (without per-item click), thumbnails appear
  as fallbacks, already loaded originals reappear on warm reopen without a
  second download, queued media is cancelled, and memory caches stay bounded.
- Concurrent profiles: scan A while browsing chats/media on B, and then
  compare same-account chat/media latency under an active scan.
- Repeat ten times to see whether the working set reaches a plateau.
  Garbage collectors are nondeterministic; RSS not immediately falling
  is not by itself proof of a leak.

No FPS or memory improvement percentage should be published without an
actual benchmark. Tests here are synthetic and cannot establish UX smoothness
on a user's Windows machine.


## Media lifecycle and UX contract

1. Expanding a Post's media group mounts its member previews and **automatically**
   requests each photo's full-resolution file. No per-photo click or confirmation.
   Small TDLib minithumbnails / cached thumbnails display immediately, then
   originals replace them when available. Video and other media still use their
   available cover previews; this change does not autoplay or fetch huge videos.
2. Each component owns two cancellation signals (thumbnail and original). A
   thumbnail finishing after the original may **not** overwrite it. Collapsing
   a group settles its queued listeners promptly; unstarted jobs with no
   subscribers are removed. Requests already submitted to synchronous TDLib
   cannot be forcibly stopped; a reopened component may join their same flight.
3. Cache keys include profile ID, account generation, file ID, and size variant.
   A hot reopen restores the original immediately; an expired/evicted entry is
   automatically requested again with no additional user action. Original cache
   is a bounded **optimization**, not permanent storage. Session reset clears
   both original and thumbnail caches; TDLib's own file cache remains managed
   by TDLib, never manually deleted.
4. Original requests are prioritized over queued noninteractive media requests.
   Concurrency is still at most 1 per account and 2 globally. Work already
   running is never preempted, and many automatic originals can cause queues
   while their native downloads complete. UI throughput should be measured
   on actual Windows hardware.
5. start_query, cancel_query, and release_query use background tasks, and
   potentially large ScanResults are dropped after relevant mutexes are
   released. This reduces UI-thread waiting, not the CPU cost of cloning
   or sending a final large result via IPC.
