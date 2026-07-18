<!-- Copyright (c) Mysten Labs, Inc. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Subscription admission priority implementation notes

## What I found

The dispatcher had two inbound lanes with very different correctness roles but equal selection priority. `SubscriptionService::start` selected randomly between the checkpoint broadcast receiver and the 128-entry admission mailbox; under a join surge, admissions were continuously ready and competed with checkpoint fan-out for dispatcher turns. The original admission path also awaited a `Register` send into a 64-entry shard mailbox, so a full shard could park the only dispatcher inside admission work while the 1024-entry checkpoint broadcast buffer filled.

The public bound was not an overload boundary. `SubscriptionServiceHandle::register_subscription` used `mpsc::Sender::send().await`, so after 128 queued requests, more gRPC tasks waited outside the channel instead of receiving a retryable error. The gRPC common registration helper already mapped `None` to `Status::unavailable`, and the in-tree bridge subscriber retries subscription-start failures after five seconds.

Checkpoint backpressure and the lag fuse are intentional. Checkpoint sends still await every shard so shard FIFOs preserve gap-free ordering; if that makes the broadcast receiver report `RecvError::Lagged`, `handle_lag` sends `Clear(SourceLag)` to every shard and resets the checkpoint sequence tracker (`subscription/mod.rs:599-621`). The change therefore removes admission-induced blocking without weakening checkpoint/shard backpressure.

## Chosen design

The implementation combines four bounded mechanisms:

1. immediately shed at the public admission lane when its existing 128 slots are full;
2. use a checkpoint-first biased dispatcher select (`subscription/mod.rs:451-489`);
3. after each successfully handled checkpoint, process at most one already-queued admission, which prevents permanent admission starvation without creating a large admission batch ahead of the next checkpoint (`subscription/mod.rs:459-469`);
4. reserve shard capacity without awaiting and probe shards from the round-robin cursor, spilling into the first shard with room or rejecting if all are full (`subscription/mod.rs:624-673`).

This was chosen over biased selection alone because selection priority cannot help while the dispatcher is blocked inside an admission handler. A larger registration batch or time-based rate limiter would add a tunable checkpoint-delay window; the single fairness turn is bounded and normal idle-period admission remains unrestricted.

`SubscriberCounts::total` still counts live subscriptions plus admitted registrations pending in shard FIFOs, not requests merely queued in the admission lane (`subscription/mod.rs:531-535`). The dispatcher reserves shard capacity, completes the receiver handshake, creates the lifecycle guard, and enqueues `Register` without an await point. Rejected, canceled, or all-full attempts create no guard, preserving the `total == 0` checkpoint fast path and lifecycle accounting.

## File-by-file changes

### `crates/sui-rpc-api/src/subscription/mod.rs`

- `SubscriptionServiceHandle::register_subscription` uses `try_send`, returning `None` immediately on a full or closed admission mailbox (`:272-281`).
- The dispatcher uses `tokio::select! { biased; ... }`, keeps the checkpoint branch first, admits at most one queued request after a real checkpoint, skips that fairness turn after `Lagged`, and keeps serving established subscribers if all admission handles close (`:451-489`).
- The checkpoint fast-path comment defines queued versus admitted registration accounting (`:531-535`).
- `handle_message` is synchronous: it keeps the global cap check, probes shard capacity with `try_reserve`, allocates the subscriber channel only after a reservation, completes the oneshot before creating the guard, enqueues without awaiting, spills to other shards, and sheds if every shard is full (`:624-673`). Closed shard mailboxes remain fatal because a terminated shard violates the service invariant.
- Existing awaited `Checkpoint` and `Clear` shard sends remain unchanged.
- Five deterministic actor tests were added at `:893-1078`.

### `crates/sui-rpc-api/src/grpc/v2/subscription_service.rs`

- The `Unavailable` message now describes general service admission unavailability rather than only the global subscriber cap, because `None` can also mean a full ingress lane or all-full shard mailboxes (`:408-428`).

### `crates/sui-rpc-api/src/subscription/ANALYSIS.md`

- Records the full gRPC-to-shard dataflow, every channel bound, the starvation mechanics, candidate designs, counter/lag-fuse interactions, and the chosen design rationale.

## Test coverage added

- `full_public_admission_queue_returns_none_promptly` (`subscription/mod.rs:894`) proves a full public lane sheds rather than waiting.
- `prequeued_checkpoint_wins_and_registration_starts_at_next_checkpoint` (`:920`) prequeues both lanes and proves checkpoint priority plus the subscriber's FIFO start boundary.
- `post_checkpoint_turn_admits_one_without_starving_ready_checkpoints` (`:946`) proves bounded progress for both lanes under a checkpoint/admission backlog.
- `shard_admission_spills_over_or_sheds_without_accounting` (`:992`) proves round-robin spillover and verifies all-full rejection leaves `counters.total` and inflight gauges unchanged.
- `run_loop_lag_clears_subscribers_and_resets_sequence_tracker` (`:1040`) creates a broadcast lag through the actual actor loop and verifies source-lag teardown and tracker recovery.

## Verification

Run in the managed `subscription-admission-priority` worktree, stacked on `nickv/hash-cpu`, in repository-required order:

- `cargo check -p sui-rpc-api` — passed.
- `cargo test -p sui-rpc-api subscription` — passed: 32 tests, 0 failed, 115 filtered.
- `cargo xclippy` from `crates/sui-rpc-api` — passed with no findings.
- `cargo fmt -p sui-rpc-api` — passed.
