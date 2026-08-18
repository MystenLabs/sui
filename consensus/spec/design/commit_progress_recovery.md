<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Commit progress recovery for Mysticeti v3

## Status and goal

This policy is not implemented.

The system goal is:

> After the network becomes stable, the network continues to produce usable DAG
> progress or a later commit, the common commit chain grows without bound, and
> every correct, available validator eventually installs each common prefix while
> the epoch is active.

Each local increase occurs through local commit execution or verified commit
synchronization. All increases extend one common commit chain. One local commit
increase is only an intermediate result.

Continued own-block production by every correct, available validator is an
optional fairness property. It is not part of this core goal.

This is one part of consensus liveness. The policy does not guarantee that every
old leader block or transaction commits. It does not change decision safety.

The [liveness proof plan](liveness_proof_plan.md) defines the final theorem
boundary for block production and commit progress. The current Lean recovery
theorem still has inputs that are too abstract for that boundary.

The main related conditions are
[`ASM-LIVE-COMMIT-PROGRESS-RECOVERY`](../docs/ASSUMPTIONS.md#asm-live-commit-progress-recovery),
[`ASM-LIVE-LEADER`](../docs/ASSUMPTIONS.md#asm-live-leader), and
[`ASM-LIVE-FIRST-SLOT-SAMPLING`](../docs/ASSUMPTIONS.md#asm-live-first-slot-sampling).
The stronger
[`ASM-LIVE-ROUND-CATCHUP`](../docs/ASSUMPTIONS.md#asm-live-round-catchup)
supports liveness for old leader blocks. It is not required for commit progress
recovery.

## Problem

A validator can accept a valid block from a much higher round. Its local round can
then move past rounds in which it made no proposal.

After a validator signs a block in round `P`, it must not sign a block in round
`P` or an earlier round. Exact-next recovery therefore cannot fill old rounds. It
must start from the validator's highest known signed round and move forward
without another jump. A cleanup safe-resume jump is a separate exception. Use it
only when the required parent history is no longer usable. This exception is not
yet proved.

## Recovery trigger

Use two round-gap thresholds, `P_enter` and `P_exit`, where
`P_exit < P_enter`, and one time threshold `T`.

The round-gap probe is the next round that normal proposal processing would use:
`threshold_clock_round()`. It is not the exact-next recovery proposal target.
Let `roundGap` be the distance from the installed commit leader round to this
probe. Let `timeGap` be the time since the last local or synchronized commit
installation.

While v3 and the epoch are active:

- Enter recovery when `roundGap >= P_enter` or `timeGap >= T`.
- After entry, stay in recovery while `roundGap >= P_exit` or `timeGap >= T`.
- Exit only when `roundGap < P_exit` and `timeGap < T`.

A commit installation resets the time signal. It does not clear recovery when
the smaller round-gap condition remains. A threshold-clock change does not reset
the time signal. The implementation must latch this hysteretic mode. It must
restore the latch after restart or restore it conservatively from durable local
state. No numeric production values are specified here.

Parent acquisition, own-block recovery, and cleanup safe resume control when a
proposal can run. They do not delay entry into the mode.

An observed higher commit index does not make commit synchronization a liveness
gate. Ordinary block delivery and recursive causal-history fetch can let the
validator reproduce the missing commit locally. Optional commit synchronization
can accelerate this work, but recovery proposals do not wait for it.

## Next-round proposal policy

Let `P` be the highest round in which the validator is known to have signed a
block. A recovery proposal follows these rules:

1. Propose only in round `P + 1`.
2. Require the local round to have reached at least `P + 1`.
3. Require quorum stake of valid parent blocks from round `P`.
4. Do not wait for a block in every selected leader slot. Keep the separate
   recovery pacing rule.
5. Store the proposal before broadcast.
6. Use the normal signature, vote, cutoff, and one-block-per-round rules.

A later high-round block does not change this target. It also does not reset
recovery pacing. If parent quorum is missing, the validator waits and requests the
missing blocks. It does not choose a higher target.

Recovery proposals must contain no new transactions. They must still carry all
consensus votes and cutoff data required by the protocol.

## Immediate-parent rule

The normal score-based exclusion policy can omit a timely correct leader block.
Its total exclusion limit does not protect the first selected leader slot.

At the parent-selection snapshot, include the current locally accepted and
retained representative from each in-range immediate-parent author for which one
exists. If the author equivocates, include one selected branch and ignore the
others during parent selection. During historical catch-up, use the branch named
by the accepted and retained child. For a new recovery proposal, use the current
local representative.

Do not exclude all blocks from the author only because it equivocated. Such exclusion
can reduce a valid parent set below quorum and stop recovery. Different validators
can select different Byzantine branches. Safety already counts that author as
Byzantine, and parent stake counts the identity only once.

Do not apply score-based exclusion to these immediate-parent representatives.
This rule does not use the local predicted leader schedule. Wait and request
blocks when the included parent stake is below quorum.

Keep the normal ancestor policy for older rounds. This recovery rule does not
depend on the leader schedule. The current Lean parent rule counts authors only;
the exact branch choice remains a source-to-model obligation.

## Pacing

Recovery must leave enough time between proposals for blocks from the current
round to reach the next-round voters.

Current Rust does not implement the rule in this section. The leader-timeout
task starts from a local threshold-clock signal. It resets both fixed timers
when that signal moves to a higher round. The maximum-timeout path calls the
proposer with `force = true`, which skips the selected-leader presence check.
The proposer still requires an immediate-parent quorum. However, its score
policy can omit a selected leader after that quorum exists.

Use one wait schedule keyed by the unchanged commit prefix and absolute target
round:

- Reset the schedule only after the commit reference changes.
- Do not reset it after a local round change.
- A local round change does not restart an expired delay.
- For a target at the threshold-clock frontier, start its timer only after quorum
  parents make that target ready.
- A timer that expired before the parents arrived does not satisfy this wait.
- Make successive wait margins grow without bound. For every finite bound `B`,
  there must be a round `r0` such that every `r >= r0` has
  `W(r + 1) >= W(r) + B`.

After network stabilization, let `delta` bound message delivery. Let `epsilon`
bound the required local processing. The ideal model can use `epsilon = 0`; the
proof does not require `epsilon < delta`. If the first-slot validator flushes and
sends by `a + K`, and next-round parent selection is no earlier than `a + W`, use
`W >= K + delta + epsilon`. The proof must derive `K` from synchronization and
local actions.

The proof must also derive the timer-start relation. For one fresh aligned
round, all correct proposals start from an exact timer and no correct block for
that round exists before its timer deadline. Ordinary broadcast, addressed
delivery, parent-ready acceptance, and bounded timer-arm work then give a fixed
constant `K_start` such that:

```text
startSpread(r + 1) <= startSpread(r) + K_start
```

The current Lean components give each local bound separately. The missing join
must compose them into this recurrence. It must not add timer spread or a future
block as a theorem input. It also does not need a bound on recursive capsule
growth: in this fresh-round step, the receiver already accepted the prior
common parent layer.

This pacing rule gives each correct next-round proposer time to accept the correct
first-slot block before it selects parents. Including that block as a parent gives
a direct vote. The rule does not prevent a Byzantine leader from sending
different information to different validators.

Any separate propagation-delay gate must eventually clear after network
stabilization. Otherwise, recovery needs a rate-limited override for that gate.

## Exit and restart

Check recovery conditions after each commit installation.

- A commit resets the time signal but does not by itself end recovery.
- Recovery stays active while either exit signal remains.
- Recovery ends only when both exit signals are below their thresholds.
- A higher observed commit index does not cancel all proposal work.
- Epoch shutdown ends recovery.

The proposal timer stays keyed to its commit reference and absolute target. The
hysteretic mode is a separate local latch. After restart, restore it or enable it
conservatively from the last durable commit, the highest known own proposal
round, and the local threshold-clock state.

## No coordinated recovery round

Validators do not select, announce, or certify a common recovery round.

The local canonical cleanup target `max(P + 1, G + 2)` is not a round
convergence rule. Different hosts can use different `G` values. Also, a lower
host cannot walk to the highest target unless it has a quorum in every
intermediate parent round. The higher host's GC-truncated history need not
contain those rounds. Repeated commit-driven cleanup changes can therefore keep
the local targets different.

Use an actual witness-bound alignment step after local cleanup bootstrap. The
proof first derives one already stored and accepted correct block at a selected
higher round `R`. A lower host accepts and retains that exact block and its
quorum immediate parents. It then locks the block reference as its alignment
witness and makes one own proposal in round `R` with the witness's immediate
parent branches. A host that already signed in `R` does not sign again. This is
the local shape in `ValidatorSafeResume` and `.alignProposal`.

The witness is past or current data. It is not an assumed future child. The
main execution still needs a source and transition rule for this step. The lock,
the selected one-branch-per-author parent list, and the exact bodies must stay
protected until the proposal is durable. A commit installation must not move
the effective cleanup boundary past these parents or clear the lock. If the
witness is already obsolete before it is locked, the proof can select a newer
actual witness. It cannot assume that such a witness will arrive.

Current Rust has a partial catch-up behavior, not this lock. When the threshold
clock accepts its first higher-round block, it can set the next proposal round
to that block's round. More accepted stake can move the clock to the following
round before the host proposes. The proposer also has no durable witness target
that survives commit and cleanup processing. Therefore, this incidental case is
not the required alignment transition.

After correct quorum stake has produced in `R`, normal addressed delivery makes
that exact layer common. Exact-next recovery can then extend it. The current
main-trace Lean composition does not connect the isolated witness-bound action
to the proposal, persistence, GC, and block-sync rules.

## Leader schedule and selection

The **validator set** contains all validators in the epoch. A **leader schedule**
contains the validators that can have leader slots during one commit-index
interval. A **round leader selection** is the ordered selection for one pending
leader round. Each entry is a **selected leader slot**.

Current v3 selects every leader-schedule member in each pending leader round. It
changes only their order. Thus, current v3 has:

```text
round leader selection = leader schedule
```

All correct validators must use the same committed prefix, including its index and
digest, and the same schedule, selection, and selected leader slot order for a
decision. Equal numeric commit indices are not sufficient. Recovery proposal
eligibility does not use the leader schedule.

Use these stake names:

- `N`: total validator-set stake.
- `f`: maximum Byzantine stake.
- `c`: maximum additional unavailable stake after network stabilization.
- `Q`: quorum threshold.
- `A`: certification threshold.
- `S`: total leader-schedule stake.
- `P_r`: total round-leader-selection stake in round `r`.

Current v3 derives its thresholds from actual epoch stake:

```text
A = 2f + c + 1
Q = N - f - c
```

Safety requires:

```text
N + f < Q + A
N + f + A <= 2Q
```

The leader schedule and round selection require:

```text
P_r <= S <= N
f + c < S
A <= P_r
```

The bound `f + c < S` ensures that the schedule contains correct available stake.
The bound `A <= P_r` ensures that a quorum block layer contains some correct
selected stake. Neither bound alone proves an anchor.

Current v3 has `P_r = S`. The optional `P_r <= Q` policy limits work. Per-slot
safety and quorum coverage do not use this upper limit. A larger selection can add
an earlier undecided slot to the anchor scan. The usable-anchor proof must cover
this effect.

## Usable anchors

The commit scan processes selected leader slots in their common order:

- A `Skip` result lets the scan continue.
- A `Commit` result gives a usable anchor.
- An `Undecided` result stops the scan.

Therefore, a committed slot is usable only when it appears before the first
undecided slot in the scan. One sufficient, stronger condition is that all slots
in that scan fragment are final and at least one has a commit result.

A correct validator in the first slot gives a simpler condition. After the
recovery wait, validators with quorum stake receive its unique block and include
it in the next round. The first slot then has a direct commit result. Later
undecided slots do not block that anchor.

Let `d` be the indirect commit depth. The current scan needs `d + 1` consecutive
usable anchor rounds. The last anchor also needs its next-round voting layer.
Thus, the recovery window needs `d + 2` quorum block layers. Current v3 has depth
two, so it needs three usable anchor rounds and four quorum block layers. These
counts follow from the scan depth; they are not configured constants.

## Leader-order model

The accepted liveness model treats each round's complete selected leader slot
order as an independent uniform permutation of one stable schedule. The Byzantine,
crashed, and unavailable set stays fixed during the stalled period. All correct
validators use the same result. The network and task scheduler do not control the
sample.

For depth two, the model needs three consecutive rounds with a correct available
validator in the first slot. Repeated independent samples produce this event with
probability one.

The product uses a deterministic round-based shuffle. The proof treats this
shuffle as if it produced independent random orders. There is no proof that its
exact sequence has the required coverage for all schedules and start rounds.

A deterministic alternative puts each schedule member first for `d + 1`
consecutive rounds, then moves the first position to the next member. All schedule
members remain selected in every pending round. This rule derives a favorable
window from `f + c < S` and removes the probability assumption.

## Proof outline and limits

The current model proves the stake arithmetic, the deterministic anchor scan, and
stage-composition results. It also proves local lemmas for next-round paths,
bounded message delivery, recovery waits, direct decisions, and pending-round
indexes.

It does not yet connect these lemmas into a theorem with only fundamental
environment inputs and single-validator contracts. The
[liveness proof plan](liveness_proof_plan.md) gives the complete proof order.
The [assumption ledger](../docs/ASSUMPTIONS.md) records the product mappings.

## Garbage collection and synchronization

The parent round `P` and target round `P + 1` must be genesis or above the local
old-block cleanup boundary. If they are not, make one normal safe-resume proposal
at a legal target above cleanup. Then continue exact-next recovery. Do not fetch,
pin, or accept old bodies at or below the requester's local cleanup round. A
source can serve old stored data when it is above the receiving peer's cleanup
round.

The normal local jump only creates a legal local block. It does not align
different hosts. The witness-bound alignment step above is required before the
proof can claim one common same-round quorum layer. Its local pin also prevents
a commit-driven cleanup change from invalidating the selected immediate parents
after the host accepts the witness.

Old-block cleanup must keep the above-boundary parents and decision evidence
needed by the active recovery window. Evidence already copied into a pending
commit remains separate from the live block cache. The open requirement is
end-to-end availability across local production, ordinary block synchronization,
and restart.

Transaction payload retention is not required. A validator or user can submit a
transaction again.

The transaction-vote cutoff in a recovery proposal must remain below its proposal
round.

## Acceptance criteria

Do not activate recovery until tests show all these results:

- Different starting rounds converge through next-round proposals without a
  coordinated recovery round.
- Parent loss, synchronization, restart, and old-block cleanup do not break the
  active recovery window.
- Equivocation, withholding, schedule changes, and insufficient stake do not cause
  an unsafe proposal or a false progress claim.
- A complete anchor window advances the commit index. A shorter-window
  counterexample shows that the full bound is necessary in the worst case.
- Every correct, available validator continues to store and send its own blocks.
- One local commit result gets an exact-reference certificate.
- At one later state, every correct, available validator has installed the same
  exact commit reference. Its index is greater than every start index. A
  validator's current commit head can be a later descendant.

Also require all missing mappings used by the recovery proof to be satisfied and
explicit acceptance of the leader-order model. The
[implementation gaps](../docs/IMPLEMENTATION_GAPS.md) list the remaining product
work.
