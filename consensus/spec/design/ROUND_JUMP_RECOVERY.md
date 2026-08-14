<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Round-jump recovery for Mysticeti v3

## Status and scope

This document proposes a fix for the Mysticeti v3 round-jump liveness gap. The
proposal is not yet implemented or proved.

The required property is commit progress:

> After GST, the commit index eventually increases.

GST is the unknown time after which the network delay has a fixed upper bound.
This document does not require every old honest leader block or transaction to
commit. It does not change the safety rules for leader or transaction decisions.

The related proof obligations are
[`ASM-LIVE-ROUND-CATCHUP`](../docs/ASSUMPTIONS.md#asm-live-round-catchup) and
[`ASM-LIVE-COMMIT-RECOVERY`](../docs/ASSUMPTIONS.md#asm-live-commit-recovery).

## Problem

[`ThresholdClock::add_block`](../../core/src/threshold_clock.rs) can move directly
to the round of one accepted future block. The future block is valid only if its
causal history contains quorum stake from the preceding round. However, a validator
does not create blocks for the omitted rounds.

This restriction is necessary:

> After an authority creates a block in round `R`, it must not create another block
> in round `R` or any earlier round.

Therefore, recovery cannot first create a block in `R` and then create old blocks.
The proposed fix creates only blocks at the current threshold-clock round.

The current Lean liveness theorem uses a stronger rule. It creates each required
intermediate proposal before it creates the future-round proposal. That rule proves
old-leader liveness. It is not known to be necessary for commit progress in v3.

## Proposed behavior

Add a commit recovery mode for v3. A validator enters this mode when all these
conditions are true:

1. The last commit timestamp is at least the recovery timeout behind the local
   clock.
2. The local commit index is greater than or equal to the observed quorum commit
   index.
3. Recovery of the last known block from this authority is complete.
4. The epoch is active and v3 is enabled.

Condition 2 uses `CommitVoteMonitor::quorum_commit_index()`. It must use the exact
comparison. `is_commit_lagging` permits a configured index gap and is not sufficient
for this recovery gate.

If the local commit index is less than the observed quorum commit index, the node
uses commit sync. It does not enter commit recovery.

### Stall time

Use the persisted commit timestamp as the timeout base:

```rust
let stall_base_timestamp_ms = dag_state
    .last_commit_timestamp_ms()
    .max(context.epoch_start_timestamp_ms);
let stalled_for_ms = context
    .clock
    .timestamp_utc_ms()
    .saturating_sub(stall_base_timestamp_ms);
let commit_is_stale = stalled_for_ms >= commit_recovery_timeout.as_millis() as u64;
```

`DagState::last_commit_timestamp_ms()` returns zero before the first commit. The
epoch start timestamp gives the correct base in that case.

The recovery state does not need separate durable storage. `DagState::new` loads
the last durable commit after restart. A run-time timer is still necessary to wake
Core when the timeout expires. Core must check the conditions again when it handles
the timer event.

The commit timestamp is consensus time. It is not the local time when Core installs
the commit. If Core installs an old commit, the timeout condition can remain true.
Recovery can then continue until the chain reaches a recent commit.

Use saturating subtraction. The current block verification can accept a block
timestamp that is ahead of the local clock. A future commit timestamp must delay
the trigger instead of causing an integer underflow. Before activation, either
enforce `Parameters::max_forward_time_drift` on every block input path or keep the
future-timestamp bound as an explicit liveness assumption.

### Recovery proposal

When recovery is active, an authority does this for each new threshold-clock round:

1. Read the current threshold-clock round `R`.
2. Check that `R` is greater than the last block round signed by this authority.
3. Check that quorum stake of valid parents exists in round `R - 1`.
4. Do not wait for the allowed leaders of round `R - 1`.
5. Create one normal `BlockV3` in round `R`.
6. Persist the block before broadcast.

The recovery block is not a new wire type. It has no new transactions. It still
carries the valid v3 transaction votes, transaction vote cutoff, and commit votes
that a normal block requires.

The proposal path must keep these existing checks:

- one block per authority and round;
- a proposal round greater than the last known own proposal round;
- quorum stake from the immediate parent round;
- normal signature and block verification;
- the transaction vote cutoff below the proposal round;
- persist before broadcast.

`Proposer::try_new_block(true)` already skips the leader wait and the minimum round
delay. The implementation should add a named recovery entry point instead of giving
the Boolean `force` flag another implicit meaning. The new entry point can share the
existing block-construction code.

The `no_last_known_proposed_round` gate must remain active. The propagation-delay
gate can remain active only if the liveness proof assumes that it eventually clears.
Otherwise, recovery needs a separate bounded override for that gate.

If a new future block moves the threshold clock while recovery is active, record the
new clock round and create the next recovery block only at that round. Do not create
any omitted old block.

### Exit and re-entry

Recompute the recovery condition after each local or synchronized commit.

- A recent commit makes the timestamp condition false and ends recovery.
- An old commit can leave the timestamp condition true. Recovery then continues.
- A quorum commit index greater than the local index pauses recovery and gives
  commit sync priority.
- Epoch shutdown ends recovery.

This behavior does not require a durable recovery-mode flag.

## No selected recovery round

The protocol does not select or certify one recovery round `R`.

After GST, assume that block sync and commit sync complete, correct tasks continue
to run, and no commit occurs. Each correct validator then reaches the same timeout
condition and stays eligible for recovery. The entry times can differ. Because the
timeout is finite, all correct stake eventually has an overlapping recovery period.

The proof must derive a round `R` during that overlap. It must not assume that Core
knows `R` in advance. A valid block at round `R + 1` already has quorum parents in
round `R`, so normal block validity supplies the quorum boundary.

This overlap argument is not yet a complete v3 liveness proof. The proof must also
show that later future blocks cannot keep the correct validators on different
recovery layers forever.

## Why three recent layers matter

A candidate recovery suffix has blocks in rounds `R`, `R + 1`, and `R + 2`.

- Blocks in `R + 1` vote on v3 leader slots in `R`.
- Blocks in `R + 2` vote on v3 leader slots in `R + 1`.
- Direct decisions in the two adjacent leader rounds can give `FlexCommitter` two
  adjacent anchors.
- Its descending indirect scan can use these anchors to decide an older pending
  prefix and advance the commit index.

Three layers are the minimum shape for this argument. They are not a fixed recovery
bound. Byzantine leaders, equivocation, missing blocks, or schedule changes can
require more layers.

The missing theorem must prove that recovery produces usable direct decisions for
the current multi-leader rule. A quorum of blocks in each layer alone does not prove
this fact.

## Leader schedule

Recovery block production must not depend on a validator knowing the current
allowed leaders. A validator at an old commit index can have an old v3 leader
schedule. Recovery therefore skips the leader wait and uses only the normal parent
quorum for block production.

Leader decisions still use the schedule derived from the local commit chain. The
proof must show one of these facts:

1. Commit sync makes the correct validators use the same schedule before their
   decisions must agree.
2. The v3 decision rules make progress while correct validators are at different
   commit indices and schedule states.

This design does not require a certified commit prefix. Local commits are normal
outputs of the commit rule. `CertifiedCommit` is only the commit-sync input type.

## Garbage collection and synchronization

Recovery creates recent blocks only. It does not create a block at or below the GC
round. The existing parent-quorum check requires the parent blocks to remain
available in the live DAG.

Commit sync can install commits and move the GC boundary while recovery is active.
Core must recompute all recovery conditions after it installs those commits.

The proof must include these cases:

- block sync obtains each required recent parent block;
- commit sync takes priority when the local commit index is behind;
- GC does not remove a parent or decision witness before Core uses it;
- restart loads the last durable commit and the last known own proposal round;
- transaction vote GC produces a cutoff that remains below the recovery block
  round.

## Rust integration

The first implementation should make these small changes:

1. Add `commit_recovery_timeout` to `consensus_config::Parameters`. Use 10 seconds
   as the initial default. Different finite values affect liveness timing, not
   safety.
2. Give Core read access to `CommitVoteMonitor::quorum_commit_index()`.
3. Add a recovery timer that sends a named Core command. Do not persist the timer.
4. Recheck the timestamp, commit indices, epoch, and proposal guards in Core.
5. Add a named proposer method that creates one recovery block at the current
   threshold-clock round.
6. On each threshold-clock advance, try the recovery proposal while recovery is
   active.
7. Recompute the recovery condition after `Core::post_commit` and after commit-sync
   installation.
8. Add metrics for recovery entry, exit, duration, proposal count, blocked reason,
   local commit index, and observed quorum commit index.

Do not change the threshold clock to create old blocks. Do not add an exact-round
proposal API that can sign below the last proposed round.

## Formal verification

Add a Lean model for commit recovery. Keep it separate from the current strong
old-leader liveness theorem.

The main theorem must state:

> Under partial synchrony, eventual block and commit synchronization, fair task
> execution, a finite recovery timeout, and the v3 recovery-layer obligations, a
> stalled correct validator eventually observes a greater commit index.

The model must include:

- staggered recovery entry;
- no selected recovery round;
- no proposal at or below the last proposed round;
- future threshold-clock jumps during recovery;
- multi-leader direct decisions and depth-two indirect anchors;
- schedule changes from committed history;
- block GC and transaction vote GC;
- block sync, commit sync, and restart.

The proof must not claim old-leader or transaction inclusion liveness.

## Tests

Add focused unit tests for these cases:

- no commit uses the epoch start timestamp;
- restart uses the last durable commit timestamp;
- saturating subtraction handles a future commit timestamp;
- an old synchronized commit keeps recovery eligible;
- a recent commit ends recovery;
- a local commit index below the quorum commit index selects commit sync;
- a recovery proposal never uses the same or an earlier round;
- a recovery block keeps the parent quorum and transaction vote cutoff rules.

Add deterministic simulation tests for these cases:

- correct validators enter recovery at different times;
- one future block causes a large round jump;
- two recent layers do not give the assumed two-anchor shape;
- three or more recent layers advance the commit index;
- a Byzantine leader equivocates or withholds its block;
- a v3 leader schedule changes during recovery;
- block sync or commit sync runs during recovery;
- GC and restart occur during recovery;
- repeated future blocks try to keep validators on different layers.

Run the tests with v3, `FlexCommitter`, and the v3 leader schedule enabled.

## Activation condition

Do not treat the timer and recent-block implementation as a complete fix by
themselves. Activate this policy only after the simulation tests pass and the Lean
commit-progress theorem discharges `ASM-LIVE-COMMIT-RECOVERY`.
