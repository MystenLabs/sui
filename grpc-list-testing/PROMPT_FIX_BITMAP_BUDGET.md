# Fix: multi-leaf bitmap filter scan-budget exhaustion returns `INTERNAL` instead of a resumable `SCAN_LIMIT`

## Setup
Create a worktree off `main` (or off `nickv/grpc-testing` @ `bb17ee0d19` if you want
the repro harness in-tree). Work in the `sui` repo. This is a **server-side** fix in
`crates/sui-rpc-api`; do **not** touch any test harness under `grpc-list-testing/`.

## The symptom
`sui.rpc.v2alpha.LedgerService` List APIs (`ListTransactions`/`ListEvents`/
`ListCheckpoints`) return a hard gRPC **`INTERNAL` (code 13)** for **multi-leaf
filters** (e.g. `sender AND move_call`, `sender AND NOT move_call`) when the bitmap
scan budget is exhausted over a dense checkpoint range. The error body is:

```
2 concurrent errors
  [0] bitmap scan budget exhausted
  [1] bitmap scan budget exhausted
```

It should instead terminate the stream with a **resumable** `QUERY_END_REASON_SCAN_LIMIT`
plus a resume cursor (watermark), exactly as **single-leaf** filters already do — so the
client paginates past it. Budget exhaustion is designed backpressure, not a failure.

### Evidence (verified against deployed mainnet fullnode `sui_v1.75.0_…gf8ad478188`)
- Single-leaf dense filters resume fine (e.g. `sender` over a dense range drains 100+
  pages via the cursor). Only **multi-leaf** filters hit `INTERNAL`.
- On the wire the `INTERNAL` carries **0 items, 0 watermark frames, 0 status-detail
  entries** — the client gets no cursor, so it genuinely cannot resume. (This is NOT a
  client/harness bug; the harness already resumes on `SCAN_LIMIT`.)
- Server logs show **nothing** at the failure (no panic/ERROR/WARN) — it is a deliberate
  error mapping, not a crash.
- This is current deployed behavior; the relevant code (below) is in `1.75.0`.

## Root cause (precise)
File: `crates/sui-rpc-api/src/grpc/v2alpha/ledger_service/bitmap_scan.rs`, function
`drain_watermarked_buckets`, the `Some(Err(e))` arm (around line 293–313):

```rust
Some(Err(e)) => {
    if e.downcast_ref::<BitmapScanBudgetExceeded>().is_some() {   // <-- only matches a BARE error
        scan_limit_hit = true;
        next_range = coalesced_frontier.and_then(|f| {
            remaining_range_after(iter_range.clone(), f, direction.is_ascending())
        });
        break;                                                    // resumable SCAN_LIMIT path
    }
    let code = if e.downcast_ref::<BitmapScanCancelled>().is_some() {
        tonic::Code::Cancelled
    } else {
        tonic::Code::Internal                                     // <-- multi-leaf falls here
    };
    return Err(RpcError::new(code, e.to_string()));
}
```

For a **single-leaf** filter, budget exhaustion arrives as a bare
`BitmapScanBudgetExceeded` (raised in `BitmapScanBudget::take_one`,
`bitmap_scan.rs:544`) → the `downcast_ref` matches → resumable. 

For a **multi-leaf** filter, several leaves exhaust the shared budget in the same driver
round, and the iterator evaluator aggregates the per-leaf errors:
`crates/sui-inverted-index/src/bitmap_query/iter.rs:184` →
`MultiError::collapse(errors)`. A plain `downcast_ref::<BitmapScanBudgetExceeded>()`
**cannot see through `MultiError`**, so the `if` is false and it falls to
`tonic::Code::Internal`.

The codebase already documents this exact trap and provides the right tool:
- `crates/sui-inverted-index/src/bitmap_query/mod.rs:65-70` — `MultiError` doc: aggregated
  errors "should use [`error_contains`] to interrogate the aggregate."
- `mod.rs:108-118` — `pub fn error_contains<T>(&anyhow::Error) -> Option<&T>` downcasts the
  error directly **and** looks through `MultiError`. Exported at the crate root
  (`sui_inverted_index::error_contains`, `mod.rs:23`).
- The eval's own test uses it: `iter.rs:642` `error_contains::<BitmapScanLimitExceeded>(&e)`.

The inner errors collected into the `MultiError` on this path are the **raw per-leaf
errors** (`iter.rs:151` `errors.push(e)`), i.e. the private
`BitmapScanBudgetExceeded` from `RpcIndexesBitmapIterator` — so
`error_contains::<BitmapScanBudgetExceeded>` will match through the aggregate.

## The fix
In the `Some(Err(e))` arm of `drain_watermarked_buckets`, recognize the aggregate using
`error_contains` instead of plain `downcast_ref`, **with correct precedence for a mixed
aggregate**:

1. **Cancellation wins.** If `error_contains::<BitmapScanCancelled>(&e).is_some()` →
   `tonic::Code::Cancelled`. (A cancelled request must not be reported as resumable.)
2. **All-budget → resumable.** Only treat as `scan_limit_hit` (the resumable path) when
   **every** inner error is `BitmapScanBudgetExceeded`. A bare `BitmapScanBudgetExceeded`
   qualifies; a `MultiError` qualifies only if `multi.iter().all(|inner|
   inner.downcast_ref::<BitmapScanBudgetExceeded>().is_some())`.
   - **Do NOT** use `error_contains` (`contains`) for this step: a leaf storage error
     becomes an opaque `anyhow!(...)` (`bitmap_scan.rs:479`), so a `MultiError` can mix a
     real fault with a budget error. Masking a real fault as resumable would corrupt
     results. Use an **all-budget** predicate.
3. **Otherwise → `Internal`** (a genuine error is present), preserving `e.to_string()`.

`MultiError` is exported (`sui_inverted_index::MultiError`) and exposes `.iter()`. Add a
small private helper (e.g. `fn is_all_budget_exhaustion(e: &anyhow::Error) -> bool`) rather
than inlining, for testability.

### The one thing you MUST confirm with a test, not assume
The resumable path computes `next_range` from `coalesced_frontier`. If
`coalesced_frontier` is `None` (no leaf emitted a watermark before exhausting), the resume
is **cursorless** → a client livelock (re-issuing the same request fails identically). The
`take_first` mechanism (`bitmap_scan.rs:486-494, 550-557`) is designed to prevent this —
every leaf is allowed its first bucket free so it always emits a first watermark, giving
the merge a non-`None` frontier. **Your regression test must assert `coalesced_frontier`
is `Some(_)` and that `next_range` strictly advances** for the multi-leaf exhaustion case.
If you find a shape where the frontier is genuinely `None` after the fix, the fix is deeper
than the downcast swap (you'd need to synthesize a resume point at the scan-range start) —
surface that rather than shipping a cursorless `SCAN_LIMIT`.

## Tests (required)
Add unit tests in the `bitmap_scan.rs` `#[cfg(test)]` module — reuse the existing `drain(...)`
helper (`bitmap_scan.rs:667`) and the `wm()`/`budget_exceeded()` builders (`:655-665`):

1. **`multi_leaf_budget_exhaustion_is_resumable`** — feed a `MultiError` of two
   `BitmapScanBudgetExceeded` after a watermark, e.g. events
   `vec![wm(10), wm(25), Err(MultiError::collapse(vec![BitmapScanBudgetExceeded.into(),
   BitmapScanBudgetExceeded.into()]))]`. Assert `state.scan_limit_hit`,
   `state.coalesced_frontier == Some(25)`, and `state.next_range == Some(26..100)`
   (mirrors the existing single-error test `budget_exceeded_anchors_resume_past_frontier_ascending`,
   `:688`).
2. **`mixed_aggregate_with_real_error_is_internal`** — a `MultiError` containing one
   `BitmapScanBudgetExceeded` and one opaque `anyhow!("storage boom")`. Assert the drain
   returns `Err` with `tonic::Code::Internal` (NOT resumable). You'll need a `drain` variant
   that returns the `Result` instead of `.expect(...)`.
3. **`cancelled_in_aggregate_is_cancelled`** — a `MultiError` containing
   `BitmapScanCancelled` + `BitmapScanBudgetExceeded`. Assert `tonic::Code::Cancelled`.
4. (If not already covered) keep a **single-leaf** bare-`BitmapScanBudgetExceeded` test
   green to prove no regression on the working path.

Consider also a higher-level test in `crates/sui-inverted-index` mirroring
`unanchored_budget_exhaustion_resumes_at_watermark` (`iter.rs:607`) but for a **2-leaf**
query, asserting the merged frontier is emitted before the `MultiError`.

## Acceptance
- Multi-leaf filter + scan-budget exhaustion → stream terminates with
  `QUERY_END_REASON_SCAN_LIMIT` and an **advancing** resume cursor; the client following the
  cursor drains the full result set. No `INTERNAL`.
- A `MultiError` mixing a real/opaque error with budget errors still surfaces as `INTERNAL`
  (or `Cancelled` if cancellation present). No masking of real faults.
- New unit tests above fail pre-fix (the resumable + mixed cases) and pass post-fix.
- `cargo test -p sui-rpc-api` and `cargo test -p sui-inverted-index` green. Skip repo-wide
  build/lint; report the targeted test output as proof.

## Live verification (optional, needs cluster + Snowflake access)
The differential harness that surfaced this lives at
`grpc-list-testing/correctness/` on branch `nickv/grpc-testing`. Repro probes (read their
top comments for the plaintext port-forward invocation):
- `_mscan.py` / `_mdecode.py` — fire the two failing cases at the mainnet fullnode
  (`kubectl -n rpc-mainnet port-forward svc/sui-node-mainnet-rpc-alpha 19000:9000`) and
  show the raw `INTERNAL` + decoded `google.rpc.Status` (0 cursor, 0 details).
- The two failing case ids in `corpus.mainnet.jsonl`:
  `tx.sender_not_move_call.anchored.shared` (sender AND NOT move_call) and
  `tx.degenerate.dense_and_dense.empty.archival` (sender 0x0 AND move_call). Post-fix they
  should drain to completion (or to the harness `--max-drain` cap) instead of erroring.
Not required for the fix — the Rust regression tests above are the deliverable.
