<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Commit progress recovery for Mysticeti v3

## Status and scope

This document specifies commit progress recovery for the Mysticeti v3 round-jump
liveness gap. The recovery mode is not implemented.

Lean proves the conditional
[`commit_progress_recovery_liveness`](../lean/Mysticeti/CommitProgressRecovery.lean)
theorem. The theorem proves the composition from a commit stall, through an
overlapping recovery interval and a recent anchor window, to a greater commit
index. It does not prove that the current Rust code supplies those inputs. The
multi-leader, synchronization, and `FlexCommitter` refinement inputs remain known
implementation gaps.

The required property is commit progress:

> After GST, the commit index eventually increases.

GST is the unknown time after which the network delay has a fixed upper bound.
This document does not require every old honest leader block or transaction to
commit. It does not change the safety rules for leader or transaction decisions.

The chosen design uses
[`ASM-LIVE-COMMIT-PROGRESS-RECOVERY`](../docs/ASSUMPTIONS.md#asm-live-commit-progress-recovery)
[`ASM-LIVE-LEADER`](../docs/ASSUMPTIONS.md#asm-live-leader), and
[`ASM-LIVE-FIRST-SLOT-SAMPLING`](../docs/ASSUMPTIONS.md#asm-live-first-slot-sampling).
[`ASM-LIVE-ROUND-CATCHUP`](../docs/ASSUMPTIONS.md#asm-live-round-catchup) is a
stronger alternative. It gives liveness for old leader blocks. It is not an input
to the commit progress recovery theorem.

## Problem

[`ThresholdClock::add_block`](../../core/src/threshold_clock.rs) can move directly
to the round of one accepted future block. The future block is valid only if its
causal history contains quorum stake from the preceding round. However, a validator
does not create blocks for the omitted rounds.

This restriction is necessary:

> After a validator creates a block in round `R`, it must not create another block
> in round `R` or any earlier round.

Therefore, recovery cannot first create a block in `R` and then create old blocks.
The proposed fix creates only blocks at the current threshold-clock round.

The current Lean liveness theorem uses a stronger rule. It creates each required
intermediate proposal before it creates the future-round proposal. That rule proves
liveness for old leader blocks. It is not known to be necessary for commit progress
in v3.

## Proposed behavior

Add a commit progress recovery mode for v3. A validator enters this mode when all
these conditions are true:

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
uses commit sync. It does not enter commit progress recovery.

### Stall time

Use the current `DagState` last-commit timestamp as the timeout base:

```rust
let stall_base_timestamp_ms = dag_state
    .last_commit_timestamp_ms()
    .max(context.epoch_start_timestamp_ms);
let stalled_for_ms = context
    .clock
    .timestamp_utc_ms()
    .saturating_sub(stall_base_timestamp_ms);
let commit_is_stale = stalled_for_ms
    >= commit_progress_recovery_timeout.as_millis() as u64;
```

`DagState::last_commit_timestamp_ms()` returns zero before the first commit. The
epoch start timestamp gives the correct base in that case.

The recovery state does not need separate durable storage. During one process run,
Core uses the current commit in `DagState`. After restart, `DagState::new` loads the
last flushed commit. `DagState::flush` is the durability point. A run-time timer is
still necessary to wake Core when the timeout expires. Core must check the
conditions again when it handles the timer event.

The commit timestamp is consensus time. It is not the local time when Core installs
the commit. If Core installs an old commit, the timeout condition can remain true.
Recovery can then continue until the chain reaches a recent commit.

Use saturating subtraction. The current block verification can accept a block
timestamp that is ahead of the local clock. A future commit timestamp must delay
the trigger instead of causing an integer underflow. Before activation, either
enforce `Parameters::max_forward_time_drift` on every block input path or keep the
future-timestamp bound as an explicit liveness assumption.

### Recovery proposal

The **selected leader slot availability wait** is the normal, non-timeout proposer
rule. If `R - 1` is a pending leader round, `DagState` must have at least one cached
block in every selected leader slot for that round. One slot can contain zero, one,
or multiple blocks. The wait set is empty when `R - 1` is below
`min_next_leader_round`.

When recovery is active, an authority does this for each new threshold-clock round:

1. Read the current threshold-clock round `R`.
2. Check that `R` is greater than the last block round signed by this authority.
3. Check that quorum stake of valid parents exists in round `R - 1`.
4. Do not use the selected leader slot availability wait for round `R - 1`.
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

`ValidatorProposer::try_new_block(true)` already skips the selected leader slot
availability wait and the minimum round delay. The existing maximum leader timeout
uses this path. Commit progress recovery adds a persistent commit-stall trigger. It
must use a named proposal mode instead of giving the Boolean `force` flag another
implicit meaning. It can share the existing block-construction code.

The recovery mode must not remove all pacing. A recovery proposal needs both a
mature schedule-independent pacing deadline and an immediate parent quorum. Key the
pacing state by commit index, not by target round. The delay must increase while the
commit index is unchanged, or the liveness model must state a known upper bound for
`delta`. Reset the pacing state only after commit progress. This wait does not
require the leader schedule. It gives blocks from correct validators in the round
leader selection more time to reach the next-round voters. It does not by itself
prevent selective delivery by a Byzantine validator.

The `no_last_known_proposed_round` gate must remain active. The existing
high-propagation-delay gate can remain active only if the liveness proof shows that
it eventually clears. Otherwise, recovery needs a rate-limited override for that
gate. This gate is different from the schedule-independent recovery delay.

If a new high-round block moves the threshold clock while recovery is active,
record the new clock round and create the next recovery block only at that round. Do
not create any omitted old block. A target-round change must not reset the recovery
attempt count, the pacing deadline, or a deadline that is already mature. If the
deadline is mature but the new target has no parent quorum, retry as soon as the
parent quorum becomes available.

### Exit and re-entry

Recompute the recovery condition after each local or synchronized commit.

- A recent commit makes the timestamp condition false and ends recovery.
- An old commit can leave the timestamp condition true. Recovery then continues.
- A quorum commit index greater than the local index pauses recovery and gives
  commit sync priority.
- Epoch shutdown ends recovery.

This behavior does not require a durable recovery-mode flag.

## No coordinated recovery round

The protocol does not select or certify one recovery round `R`.

After GST, assume that block sync and commit sync complete, protocol tasks at
correct, non-crashed validators continue to run, correct local clocks progress with
bounded drift, and no commit occurs. Each correct, non-crashed validator then
satisfies its local timeout condition and stays eligible for recovery. The entry
times can differ. Eventually, correct, non-crashed validators with quorum stake are
in recovery at the same time.

The refinement proof must show that a round `R` exists during that overlap. It must
not assume that Core knows `R` in advance. A valid block at round `R + 1` already
has quorum parents in round `R`, so normal block validity supplies one quorum block
layer. It does not supply the complete recovery window.

This overlap argument is not yet a complete v3 liveness proof. The proof must also
show that later high-round blocks cannot keep the correct validators on different
recovery layers forever.

## Recovery distances

The proof does not use an independent constant for the recovery window. It derives
the required layer count from two protocol distances:

- `directVoteRoundOffset` is the distance from a leader block to its direct-vote
  blocks. Its current value is one.
- `indirectCommitDepth` is the minimum distance from a decision round to an
  indirect anchor. Its current value is two.

The current Lean model fixes the direct-vote offset at one. For indirect depth `d`,
it therefore needs `d + 1` consecutive quorum block layers. The first `d` layers
are candidate anchor rounds. The next layer supplies direct votes for the last
candidate round. Current v3 has depth two, so the window contains layers `R`,
`R + 1`, and `R + 2`. The value three is derived from these two protocol rules. It
is not a separate configuration value or a fixed completion bound.

For a future protocol with direct-vote offset `v`, `d + v` is the expected general
form. The current Lean theorem does not prove that general form.

For depth two, usable anchors in adjacent rounds `R` and `R + 1` cover the two rounds
immediately below the recovery window. The descending indirect scan can then
continue through the older pending prefix. A single anchor can leave one of these
rounds blocked.

The Lean theorem
[`quorum_block_layer_window_yields_anchor_window`](../lean/Mysticeti/CommitProgressRecovery.lean)
proves the layer-count calculation for any anchor count. Its multi-leader input is
stronger than a quorum block layer statement. It assumes that the Rust scan has a
usable anchor in each candidate round. Full finality for every selected leader slot,
with at least one commit result, is one stronger sufficient condition. The theorem
does not derive either condition from quorum stake alone.

More layers do not by themselves fix split votes for one selected leader slot.
Byzantine equivocation, missing blocks, or a schedule mismatch can require a
protocol rule or a stronger delivery argument before the multi-leader input holds.

## Leader schedule and round leader selection

Use these terms in the specification:

- The **validator set** is the complete set of validators for the epoch.
- The **leader schedule** is the subset of the validator set that is eligible for
  leader selection during one leader schedule interval. This interval is defined
  by commit indices, not by a fixed range of rounds.
- The **round leader selection** is the subset of the leader schedule that the
  protocol selects in one round.
- A **selected leader slot** is the pair of one round and one selected validator.

In Rust, `Committee` represents the validator set. `AuthorityIndex` identifies one
validator.

Many traditional protocols use the full validator set as the leader schedule. A
deterministic function then selects one validator in each round. A multi-leader
protocol can select a fixed number of validators in each round. In both cases, the
round leader selection can be smaller than the leader schedule.

Current v3 has a different relation. For each pending leader round at or above
`min_next_leader_round`, it selects every validator in the leader schedule:

```text
round leader selection = leader schedule
```

The Rust field `NextCommitLeaderSchedule::allowed_leaders` stores the ordered
membership list for the leader schedule. `PendingCommitState::get_or_create_round_state`
copies every validator from that field into one selected leader slot for the round.
It then applies a deterministic round-based permutation. It does not select a
smaller per-round subset.

Recovery proposal eligibility must not depend on the leader schedule. A validator
at an old commit index can have an old schedule. Recovery therefore skips the
selected leader slot availability wait and uses the normal parent quorum for block
production. Commit decisions still depend on common leader schedule membership and
selected leader slot order.

### Stake bounds

Use the actual validator set and thresholds from the epoch committee. Let:

```text
N = total validator set stake
f = maximum Byzantine stake
c = maximum crashed or otherwise non-progressing stake after GST
A = 2f + c + 1
Q = N - f - c
```

Here, `A` is the certification threshold and `Q` is the quorum threshold. Rust
`Committee::new_v3` scales `f` and `c` to the actual validator set stake. The main
proof uses the resulting `committee.certification_threshold()` and
`committee.quorum_threshold()` values. It uses these checked safety inequalities:

```text
N + f < Q + A
N + f + A <= 2Q
```

The closed formulas `N = 5f + 3c + 1` and `Q = 4f + 2c + 1` describe the nominal
case only. They are not identities for every Rust committee after scaling.

Let:

```text
S   = total stake in the leader schedule
P_r = total stake in the round leader selection for round r
```

Because the round leader selection is a subset of the leader schedule:

```text
P_r <= S <= N
```

The leader schedule viability bound is:

```text
f + c < S <= N
```

The lower bound ensures that the leader schedule contains positive stake from a
correct, non-crashed validator when Byzantine and crashed stake is at most `f + c`.
It does not ensure that a smaller round leader selection contains this stake. A
protocol that selects one or a fixed number of leaders still needs a leader
fairness condition. It must eventually select a correct, non-crashed validator.

For one round, the proof uses a separate quorum-coverage bound:

```text
A <= P_r
```

The lower bound gives a deterministic quorum-coverage property. The threshold
inequality is:

```text
Q + A > N + f
```

Therefore, every quorum block layer intersects a round leader selection of stake at
least `A` in more than `f` stake. This intersection contains positive stake from a
correct validator in the round leader selection. The theorem
`quorum_and_round_leader_selection_intersect_outside_byzantine_stake` in
[`CommitProgressRecovery.lean`](../lean/Mysticeti/CommitProgressRecovery.lean)
proves this weighted result.

The lower bound `A <= P_r` is sufficient for this quorum-coverage result. It is not
sufficient for commit progress. It does not make all selected leader slots final,
and it does not ensure that the anchor scan finds a commit. It is also not necessary
for all leader protocols. For example, a one-leader protocol can use a smaller
`P_r` and obtain liveness from eventual correct leader selection.

The optional bound `P_r <= Q` is a resource policy for work in one round. It is not
used by the per-slot safety proof or the quorum-coverage lemma. A round leader
selection with total stake above `Q` can add work and can add an undecided slot that
blocks the anchor scan. This liveness effect remains in the usable-anchor
obligation. A stake limit also does not limit the number of selected leader slots
when validator weights are uneven. Use a separate slot-count limit if one is
necessary.

| Relation | Purpose |
|---|---|
| `P_r <= S <= N` | Required set structure |
| `f + c < S` | Leader schedule viability |
| `A <= P_r` | Sufficient quorum coverage in round `r` |
| `P_r <= Q` | Optional work limit |
| `P_r = S` | Current v3 selection rule for pending leader rounds |

### V3 specialization

Current v3 selects the full leader schedule in each pending leader round. Therefore:

```text
P_r = S
```

The structural and quorum-coverage bounds reduce to this v3 condition:

```text
A <= S = P_r <= N
```

This condition applies to both the leader schedule and the round leader selection
only because they are equal in v3. Its lower bound implies the schedule viability
bound because `A > f + c`.

If the implementation also enables the optional resource policy, the combined
resource bounds are:

```text
A <= S = P_r <= Q
```

Rejecting this optional upper bound does not affect the per-slot safety proof or
the quorum-coverage lemma. The effect on anchor-scan liveness remains an explicit
proof obligation.

The Lean theorems `v3_schedule_coverage_implies_viability`,
`v3_full_schedule_gives_round_leader_selection_coverage`, and
`v3_coverage_applies_to_schedule_and_round` check these implications. The theorem
`schedule_bounds_do_not_force_round_leader_selection` checks that a viable leader
schedule does not by itself give round leader selection coverage.

The optional resource interval is nonempty when `A <= Q`. This arithmetic fact does
not prove that an arbitrary weighted validator set has a subset with stake in the
interval. If no such subset exists, the implementation can reject the optional
`Q` work cap or use a larger schedule with a documented cost.

The current score rule limits excluded stake by a percentage. It does not directly
enforce `A <= S`. Before v3 activation, the implementation must compute `S` from
the actual leader schedule and enforce this liveness lower bound for each
leader schedule interval. It should report `S <= Q` only if the deployment enables
that resource policy. It should also compute `P_r` separately if a later protocol
version uses a smaller round leader selection.

### Safety and liveness meaning

For one fixed, common selected leader slot, leader decision safety uses the same
`Q` and `A` intersection rules. This threshold arithmetic does not depend on `S`,
`P_r`, or the number of selected leader slots. Global commit safety also requires
correct validators to derive the same leader schedule version, round leader
selection, and selected leader slot order.

The stake bounds are not a complete liveness proof. A block from a correct validator
in the round leader selection must reach the direct voters before they create their
next-round blocks. Each selected leader slot must then become final, or the ordered
anchor scan must find a commit before its first undecided slot. Selective Byzantine
delivery can split votes and leave a slot undecided. The current deterministic slot
shuffle has no proved fairness property that removes this case.

### Selected leader slot finality in Rust

For one selected leader slot in leader round `r`,
`LeaderSlotDecider::try_direct_decide` first requires quorum block stake in round
`r + directVoteRoundOffset`. It then evaluates each block in the slot:

- the block has a direct commit result when quorum child stake references it;
- the block has a direct skip result when quorum voting stake does not reference it;
- an empty slot has a skip result after the voting layer reaches quorum;
- a slot with a block that has neither quorum result stays undecided.

A Byzantine validator can send different blocks to different voters. This can split
the correct votes. The quorum block layer fact and the `A <= P_r` bound do not
prevent an undecided selected leader slot.

`FlexCommitter::find_anchor_block` scans selected leader slots in deterministic
order across the start round and later pending rounds. A skip result lets the scan
continue. A commit result supplies the anchor. An undecided result stops the scan.
Thus, the usable-anchor condition for the complete scan is:

> The scan finds a commit before it finds an undecided selected leader slot.

The Lean model uses `UsableAnchorOrder` for one round of this scan. The complete
Rust scan concatenates this round's selected leader slot order with the orders for
later pending rounds. Lean also proves that “all selected leader slots are final
and at least one slot has a commit result” is a stronger sufficient condition for
one usable scan fragment for one round.

`FlexCommitter::find_commit_leader_round` has a stronger output condition. Every
selected leader slot in that round and each earlier pending round must be final.
The output round must also have a commit result in at least one selected leader
slot. A direct commit status can act as an indirect anchor before Core builds a
commit from it. The reverse scan uses later anchors to make older selected leader
slots final.

The remaining proof obligation is to show that commit progress recovery repeatedly
creates enough usable anchor rounds despite Byzantine vote splits. One valid option
is a protocol rule that gives all correct validators the same commit-or-skip result
for each selected leader slot. Another option is a proved fairness rule for the
selected leader slot order. The current code enforces neither option as a protocol
invariant.

#### Current seeded shuffle

Two adjacent usable anchor rounds can have different correct validators in their
first selected leader slots. They do not need the same validator. Let `a_r` be the
validator in the first selected leader slot for round `r`. For a fixed set `F` of
Byzantine, crashed, or unavailable validators, the required order property is:

```text
eventually a_r is not in F and a_(r + 1) is not in F
```

If each round used an independent uniform random permutation, and `p > 0` were the
fraction of leader schedule members outside `F`, one disjoint pair of rounds would
satisfy this property with probability `p^2`. The probability that none of `k`
disjoint pairs satisfies it would be `(1 - p^2)^k`, which approaches zero. This
would give almost-sure liveness. The two correct validators can be the same or
different.

The Rust implementation does not make independent random choices. It creates a new
`StdRng` from the public round number and shuffles the ordered leader schedule. This
is one deterministic function of the round and schedule. The `rand` library does
not give a coverage property for consecutive seeds. Thus, the probabilistic
calculation is not a proof for the Rust order.

The exact deterministic condition can be stated as a graph property. For a finite
round window, create one vertex for each leader schedule member and add the edge
`(a_r, a_(r + 1))` for each adjacent round pair. A set `F` prevents every adjacent
correct pair exactly when `F` is a vertex cover of this graph. Therefore, the
window guarantees an adjacent correct pair when every vertex cover has stake
greater than the Byzantine-plus-unavailable stake bound. This condition permits
two different correct validators.

The current implementation does not check this weighted vertex-cover condition,
and its seeded shuffle has no specified bound that implies it for every future
round window. The Lean counterexample `alternating_order_has_no_adjacent_correct_first`
also shows that a valid per-round permutation and ordinary leader fairness are not
sufficient. For example, the first slots can repeat as `B, H1, B, H2`, where `B`
is Byzantine and `H1` and `H2` are correct. Both correct validators are selected
first repeatedly, but no two adjacent first slots are correct.
A deterministic repeated-first-slot order is a simple sufficient alternative, but
it is not the only possible order.

**Decision, 2026-08-14:** The current recovery design accepts
`ASM-LIVE-FIRST-SLOT-SAMPLING`. While the commit index is stalled, model each
round's shuffle as one common, independent, uniform random permutation of the
stable leader schedule. The non-progress set is fixed during this period. If `p`
is the fraction of schedule members that are correct and non-crashed, an adjacent
round pair has two correct first slots with probability `p^2`. The correct
validators can be the same or different. Repeated disjoint pairs succeed with
probability one. The deterministic round-seeded `StdRng` shuffle is accepted as a
pseudorandom implementation of this model; this is not a deterministic coverage
claim.

Leader decisions still use the schedule derived from the local commit chain. The
Rust refinement must show one of these facts:

1. Commit sync makes the correct validators use the same leader schedule membership
   and selected leader slot order before their decisions must agree.
2. The v3 decision rules make progress while correct validators are at different
   commit indices and schedule states.

This design does not require a certified commit prefix. Local commits are normal
outputs of the commit rule. `CertifiedCommit` is only the commit-sync input type.
If no commit occurs at one commit index, the leader schedule membership does not
change at that validator. Correct validators must still use the same committed
prefix, leader schedule membership, round leader selection, and selected leader
slot order when they use one anchor scan. Equal numeric commit indices are not
sufficient if the commit digests differ. The refinement proof must cover validators
that start recovery at different commit indices. If a commit changes the schedule,
the target commit progress has already occurred and Core recomputes the recovery
condition.

## Garbage collection and synchronization

Recovery creates recent blocks only. It does not create a block at or below the GC
round. The existing parent-quorum check requires the parent blocks to remain
available in the live DAG.

Commit sync can install commits and move the GC boundary while recovery is active.
Core must recompute all recovery conditions after it installs those commits.

The Rust refinement must include these cases:

- block sync obtains each required recent parent block;
- commit sync takes priority when the local commit index is behind;
- GC does not remove a parent or decision witness before Core uses it;
- restart loads the last flushed commit and the last known own proposal round;
- transaction vote GC produces a cutoff that remains below the recovery block
  round.

## Rust integration

### Configuration

Add `commit_progress_recovery_timeout` to `consensus_config::Parameters`. Use 10
seconds as the first default. This is an operational timeout. Different finite
values change recovery latency, not decision safety.

Do not use this timeout to select a round. Do not store an in-memory start time as
the stall base. Use the current `DagState` commit timestamp and the epoch start
timestamp. After restart, the current value comes from the last flushed commit.

### Component wiring

Create `Arc<CommitVoteMonitor>` before `Core::new_validator`. Pass the same monitor
to Core, `Synchronizer`, `CommitSyncer`, and the services that already observe commit
votes. Core must read `quorum_commit_index()` when it evaluates recovery. It must not
use `is_commit_lagging`, because that helper permits a configured gap.

Add a small `CommitProgressRecoveryTask` next to `LeaderTimeoutTask`. The task only
wakes Core. It does not decide that recovery is active. A periodic check is enough
because Core derives the real elapsed time from the current commit timestamp. Stop
the task before `CoreThread` during authority shutdown.

Track a recovery attempt count and pacing deadline for the current commit index.
Increase the schedule-independent recovery delay after each recovery proposal that
does not lead to a commit. A threshold-clock change updates only the target round.
It does not reset the attempt count or deadline. When the deadline expires, keep it
mature until Core creates a recovery block or the commit index changes. Use checked
time arithmetic. A finite maximum delay requires an explicit deployment bound for
`delta`; otherwise, the delay must be able to grow until it exceeds the post-GST
message delay.

Add a named command such as:

```rust
CoreThreadCommand::TryCommitProgressRecovery
```

When Core receives this command, it rechecks every gate. A stale timer event must
not create a block after a new commit, during commit sync, before own-block recovery
completes, or after epoch shutdown.

### Core gate

Add one helper that returns a reason, not only a Boolean. Its checks are:

1. V3 is active and Core is a validator.
2. The last known own proposal round is available.
3. `local_commit_index >= quorum_commit_index`.
4. The current `DagState` commit timestamp is stale by at least
   `commit_progress_recovery_timeout`.
5. The threshold-clock round is greater than both the last local proposal round and
   the last known own proposal round.
6. The current DAG has quorum parent stake in the immediate preceding round.

If the local commit index is lower than the quorum commit index, return a
`commit_sync_ahead` reason. Commit sync keeps priority. If the indices are equal or
the local index is higher, recovery can proceed after the other checks pass.

Call this helper from the recovery command and after these events:

- a threshold-clock advance in `Core::add_blocks`;
- commit-sync installation in `Core::add_certified_commits`;
- completion of last-known-own-block recovery;
- a propagation-gate state change.

`Core::post_commit` does not need a recovery flag to clear. The next helper call
reads the new current index and timestamp. `DagState::flush` makes the commit
durable. An old installed commit can keep the timestamp stale. In that case,
recovery remains eligible until a later commit is recent enough.

### Proposal mode

Replace the proposal `force: bool` at the internal construction boundary with an
explicit mode, for example:

```rust
enum ProposalMode {
    Normal,
    LeaderTimeout,
    CommitProgressRecovery,
}
```

`CommitProgressRecovery` has these rules:

- use the current threshold-clock round only;
- bypass the selected leader slot availability wait;
- use a schedule-independent recovery delay that increases while the commit index
  is unchanged;
- keep the last-known-own-round gate;
- keep the immediate-parent quorum check;
- include no new transactions;
- still include valid transaction votes, the transaction vote cutoff, and commit
  votes;
- flush the signed block before broadcast;
- create at most one block for the authority and round.

Do not use the existing `force = true` behavior without change. It bypasses both
the selected leader slot availability wait and `min_round_delay`. The named recovery mode
must separate these two controls. The recovery delay must not depend on selected
leader slots. Under standard partial synchrony, it must eventually exceed the
post-GST message delay.

The high-propagation-delay gate needs an explicit policy. The liveness theorem
assumes that it cannot block recovery forever. The first implementation can either
prove that the round prober clears it after GST or let commit progress recovery
bypass it with a rate limit. Record the chosen policy in the assumption ledger
and test it under repeated future-round input.

### Metrics

Add metrics for the check result, active duration, proposals, local commit index,
observed quorum commit index, leader schedule stake, and round leader selection
stake. Current v3 reports equal values for the last two metrics. Use stable reason
labels such as `timeout_not_reached`, `commit_sync_ahead`, `own_round_unknown`,
`no_parent_quorum`, `propagation_gate`, and `proposed`.

Do not change the threshold clock to create old blocks. Do not add an exact-round
proposal API that can sign below the last proposed round.

## Formal verification

[`CommitProgressRecovery.lean`](../lean/Mysticeti/CommitProgressRecovery.lean) keeps
commit progress recovery separate from the strong theorem for old leader blocks.

The main theorem states:

> Under partial synchrony, eventual block and commit synchronization, fair task
> execution, correct clock progress, a finite recovery timeout, and the v3
> recovery quorum block layer obligations, a
> stalled correct, non-crashed validator eventually observes a greater commit
> index.

Lean checks these facts:

- the non-progress set contains every Byzantine validator, and a recovery quorum
  contains only correct, non-crashed validators;
- `f + c < S <= N` gives a leader schedule with positive stake from a correct,
  non-crashed validator;
- leader schedule viability bounds alone do not give round leader selection
  coverage;
- `A <= P_r` makes every quorum block layer contain positive stake from correct
  validators in the round leader selection;
- `P_r <= Q` is a separate optional resource bound;
- v3 selection of the full schedule gives `A <= S = P_r <= N`;
- full finality of the selected leader slots plus one selected leader slot with a
  commit result gives a usable scan fragment for one round;
- for current v3, the recovery layer count is the indirect depth plus one direct
  vote layer;
- the recovery-window base is existential, not selected by the protocol;
- every layer in the witness window is above the modeled block-GC boundary;
- a recovery proposal cannot be at or below the authority's last signed round;
- each temporal recovery stage stays at the baseline commit index until the final
  stage increases it;
- the assumed temporal recovery stages compose to eventual commit-index growth.

These items remain Rust refinement assumptions:

- the current commit timestamp, the last flushed restart value, and correct local
  clocks cause staggered recovery entry and eventual recovery quorum overlap;
- future threshold-clock jumps cannot prevent a consecutive recent layer window;
- schedule-independent pacing and direct voting give a usable ordered anchor scan
  in each required anchor round;
- correct validators use the same committed prefix, leader schedule membership,
  round leader selection, and selected leader slot order for the relevant scans;
- the `FlexCommitter` descending scan maps the anchor window to a greater commit
  index;
- leader schedule changes are derived from committed history;
- block GC and transaction vote GC;
- block sync, commit sync, and restart.

The proof must not claim liveness for old leader blocks or transaction inclusion.

## Tests

Add focused unit tests for these cases:

- no commit uses the epoch start timestamp;
- restart uses the last flushed commit timestamp;
- saturating subtraction handles a future commit timestamp;
- an old synchronized commit keeps recovery eligible;
- a recent commit ends recovery;
- a local commit index below the quorum commit index selects commit sync;
- a recovery proposal never uses the same or an earlier round;
- a recovery block keeps the parent quorum and transaction vote cutoff rules.

Add deterministic simulation tests for these cases:

- correct, non-crashed validators enter recovery at different times;
- one future block causes a large round jump;
- a layer window shorter than
  `indirectCommitDepth + directVoteRoundOffset` does not give the assumed anchor
  window;
- a sufficient derived layer window advances the commit index;
- a Byzantine leader equivocates or withholds its block;
- leader schedule stake is at and below `f + c`, and above that bound;
- round leader selection stake is below `A` and at `A`;
- the optional resource policy reports round leader selection stake at `Q` and
  above `Q` without treating the latter as a correctness failure;
- v3 round leader selection equals the leader schedule as a set, and the two sets
  have equal total stake;
- every selected leader slot becomes final, or the test records the exact
  undecided slot that blocks the anchor scan;
- a v3 leader schedule changes during recovery;
- block sync or commit sync runs during recovery;
- GC and restart occur during recovery;
- repeated future blocks try to keep validators on different layers.

Add one threshold-mapping test with actual total stake `55` and reference inputs
`malicious_stake = crash_stake = 1250`. Rust scales these inputs to `f = c = 6`.
The test must use `A = 19` and `Q = 43`. It must not substitute the nominal values
for `N = 49`.

Run the tests with v3, `FlexCommitter`, and the v3 leader schedule enabled.

## Activation condition

Do not treat the timer and recent-block implementation as a complete fix by
themselves. Activate this policy only after the simulation tests pass and the Rust
refinement supplies the inputs to `commit_progress_recovery_liveness`. The Lean
composition theorem alone does not discharge
`ASM-LIVE-COMMIT-PROGRESS-RECOVERY` for the implementation. The stronger
`ASM-LIVE-ROUND-CATCHUP` rule is not an activation condition for this narrower
commit-progress design.
