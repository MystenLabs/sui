<!-- Copyright (c) Mysten Labs, Inc. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Subscription admission priority implementation notes

## What I found

The dispatcher had two inbound lanes with very different correctness roles but equal selection priority. `SubscriptionService::start` selected randomly between the checkpoint broadcast receiver and the 128-entry admission mailbox; under a join surge, admissions were continuously ready and competed with checkpoint fan-out for dispatcher turns. The original admission path also awaited a `Register` send into a 64-entry shard mailbox, so a full shard could park the only dispatcher inside admission work while the 1024-entry checkpoint broadcast buffer filled.

The public bound was not an overload boundary. `SubscriptionServiceHandle::register_subscription` used `mpsc::Sender::send().await`, so after 128 queued requests, more gRPC tasks waited outside the channel instead of receiving a retryable error. The gRPC common registration helper already mapped `None` to `Status::unavailable`, and the in-tree bridge subscriber retries subscription-start failures after five seconds.

Checkpoint backpressure and the lag fuse are intentional. Checkpoint sends still await every shard so shard FIFOs preserve gap-free ordering; if that makes the broadcast receiver report `RecvError::Lagged`, `handle_lag` sends `Clear(SourceLag)` to every shard and resets the checkpoint sequence tracker (`subscription/mod.rs:599-621`). The change therefore removes admission-induced blocking without weakening checkpoint/shard backpressure.

## Current admission tuning

The dispatcher combines four bounded mechanisms:

1. `ADMISSION_MAILBOX_SIZE = 4096` absorbs cold-join bursts in a lane containing only `SubscriptionSpec` values and oneshot senders. The queue is roughly 1 MiB at capacity, and a full lane returns the retryable `Unavailable` path immediately.
2. A checkpoint-first biased `select!` gives every ready checkpoint priority over admission work.
3. `ADMISSION_BATCH_SIZE = 128` bounds each admission turn. The dispatcher runs a turn after every successful checkpoint fan-out, guaranteeing progress during a continuously ready checkpoint backlog while limiting the delay before the next checkpoint poll to well under one millisecond.
4. Shard capacity is reserved with non-blocking round-robin probes. If every 64-entry shard mailbox is full, the popped request occupies one dispatcher-local pending slot and is retried before any newer lane entry. `RETAINED_ADMISSION_RETRY_INTERVAL = 1 ms` prevents hot-spinning between probes, while checkpoint arrival preempts the timer and triggers an immediate post-fan-out retry. No request removed from the public lane is shed because of shard saturation.

`SubscriberCounts::total` counts live subscriptions plus admitted registrations pending in shard FIFOs, not requests in the admission lane or the retained-pending slot. The dispatcher completes the receiver handshake, creates the lifecycle guard, and enqueues `Register` without an await point. Cap rejection and canceled handshakes create no guard, preserving the `total == 0` checkpoint fast path and lifecycle accounting.

## File-by-file changes

### `crates/sui-rpc-api/src/subscription/mod.rs`

- `SubscriptionServiceHandle::register_subscription` uses `try_send`, returning `None` immediately on a full or closed 4096-entry admission mailbox.
- The dispatcher uses a checkpoint-first biased `select!` and runs bounded 128-request admission turns after checkpoint fan-out and normal admission selection.
- A request popped while all shard mailboxes are full stays in `pending_admission`; pending work is retried ahead of the public lane after a 1 ms timer or immediately after checkpoint fan-out, without awaiting a particular shard's capacity.
- The checkpoint fast-path comment defines queued versus admitted registration accounting.
- `handle_message` keeps the global cap check, probes shard capacity with `try_reserve`, allocates the subscriber channel only after a reservation, completes the oneshot before creating the guard, and enqueues without awaiting. Closed shard mailboxes remain fatal because a terminated shard violates the service invariant.
- Awaited `Checkpoint` and `Clear` shard sends remain unchanged, and `SHARD_MAILBOX_SIZE` remains 64.

### `crates/sui-rpc-api/src/grpc/v2/subscription_service.rs`

- The `Unavailable` message describes general service admission unavailability because `None` can mean the global subscriber cap or a full ingress lane.

### `crates/sui-rpc-api/src/subscription/ANALYSIS.md`

- Records the full gRPC-to-shard dataflow, every channel bound, the starvation mechanics, candidate designs, counter/lag-fuse interactions, and the chosen design rationale.

## Test coverage added

- `full_public_admission_queue_returns_none_promptly` proves a full public lane sheds rather than waiting.
- `prequeued_checkpoint_wins_and_registration_starts_at_next_checkpoint` prequeues both lanes and proves checkpoint priority plus the subscriber's FIFO start boundary.
- `post_checkpoint_turn_batches_admissions_without_starving_ready_checkpoints` proves one fairness turn admits multiple queued requests while checkpoint priority remains intact.
- `shard_admission_spills_over_when_preferred_shard_is_full` proves round-robin spillover.
- `saturated_admission_is_retained_ahead_of_newer_requests` proves all-full shard saturation retains the popped request and preserves FIFO order when capacity returns.
- `retained_admission_retry_parks_between_capacity_probes` uses paused Tokio time to prove sustained saturation parks the dispatcher between probes and admits the request after capacity returns.
- `run_loop_lag_clears_subscribers_and_resets_sequence_tracker` creates broadcast lag through the actor loop and verifies source-lag teardown and tracker recovery.

## Verification

Run in the managed `subscription-admission-priority` worktree, stacked on `nickv/hash-cpu`, in repository-required order:

- `cargo check -p sui-rpc-api` — passed.
- `cargo test -p sui-rpc-api subscription` — passed: 34 tests, 0 failed, 115 filtered.
- `cargo xclippy` from `crates/sui-rpc-api` — passed with no findings.
- `cargo fmt -p sui-rpc-api` — passed.
