<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Commit progress recovery for Mysticeti v3

## Status and goal

This policy is not implemented.

The goal is:

> After the network becomes stable, the commit index eventually increases.

This is one part of consensus liveness. The policy does not guarantee that every
old leader block or transaction commits. It does not change decision safety.

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
`P` or an earlier round. Recovery therefore cannot fill old rounds. It must start
from the validator's highest known signed round and move forward without another
jump.

## Recovery trigger

A validator enters recovery only when all these conditions are true:

1. The current last-commit timestamp is older than the configured recovery period.
2. The local commit index is not below the commit index reported by quorum stake.
3. Recovery of the validator's highest known signed block is complete.
4. The epoch is active and v3 is enabled.

Measure the stalled period from the later of the current last-commit timestamp and
the epoch start. After restart, the last flushed commit supplies the initial
timestamp. A future timestamp delays recovery and must not cause an arithmetic
error. The deployment must bound how far a timestamp can be in the future, or the
liveness assumptions must state such a bound.

If the quorum commit index is higher, the validator uses commit synchronization.
It does not make recovery proposals. The validator checks all entry conditions
again when the recovery timer expires.

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

During recovery, use this rule for the immediate parent round:

1. Include a validator's block when exactly one valid block from that validator is
   locally available.
2. Include no block from a validator when the local DAG shows an equivocation.
3. Do not apply score-based exclusion to these immediate parents.
4. Wait and request blocks when the included parent stake is below quorum.

Keep the normal ancestor policy for older rounds. Keep all equivocation checks.
This recovery rule does not depend on the leader schedule.

## Pacing

Recovery must leave enough time between proposals for blocks from the current
round to reach the next-round voters.

Keep one growing delay for the unchanged commit index:

- Increase the delay after a recovery proposal that does not lead to a commit.
- Reset the delay only after the commit index increases.
- Do not reset it after a local round change.
- A local round change does not restart an expired delay.
- After a recovery proposal changes `P`, start the next, larger delay for the new
  target.
- If parents are missing at the deadline, propose when parent quorum becomes
  available.

After network stabilization, let `delta` bound message delivery. Let `epsilon`
bound the required local processing, with `0 < epsilon < delta`. The delay must
eventually cover proposal-time differences, delivery, and local processing.

This pacing rule gives a timely correct leader block enough time to receive votes.
It does not prevent a Byzantine leader from sending different information to
different validators.

Any separate propagation-delay gate must eventually clear after network
stabilization. Otherwise, recovery needs a rate-limited override for that gate.

## Exit and restart

Check recovery conditions after each local or synchronized commit.

- A recent commit ends recovery.
- An old commit can leave recovery active.
- A higher quorum commit index pauses proposals and gives commit synchronization
  priority.
- Epoch shutdown ends recovery.

Recovery state is keyed to the current commit index. It does not need a separate
durable mode flag. After restart, rebuild the state from the last durable commit
and the highest known own proposal round.

## No coordinated recovery round

Validators do not select, announce, or certify a common recovery round.

If no commit occurs first, correct available validators with quorum stake are
eventually in recovery in the same rounds. Their starting rounds can differ. For
the proof, let `R` be the highest starting round among them. `R` is a proof value,
not a protocol value.

The proof requires synchronization to supply the retained parent history below
`R`. Each validator then uses next-round proposals until it reaches `R`. Quorum
proposals in `R` provide parents for `R + 1`. Repetition creates consecutive
quorum block layers.

This argument needs two product guarantees. A reported own round must identify a
durable signed block and its required parent history. Required parents must become
available, or commit synchronization must make them unnecessary.

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
an earlier undecided slot to the anchor scan, so its effect remains part of the
condition that the scan finds a usable anchor.

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

## Proof outline and limits

The formal model proves this sequence:

1. If no commit occurs first, correct available validators with quorum stake enter
   recovery and remain there.
2. Next-round proposals and parent synchronization create consecutive quorum block
   layers.
3. Growing pacing, immediate-parent inclusion, and favorable leader order create
   the required usable anchors.
4. The anchor scan resolves the older pending prefix and increases the commit
   index.

The model also proves the deterministic anchor-scan result. It does not prove that
the product source follows every recovery action. The
[assumption ledger](../docs/ASSUMPTIONS.md) records that mapping.

## Garbage collection and synchronization

The parent round `P` and target round `P + 1` must be above the old-block cleanup
boundary. Otherwise, own-block recovery and commit synchronization must first
establish a safe proposal frontier.

Old-block cleanup must keep the parents and decision evidence needed by the active
recovery window. Evidence already copied into a pending commit remains separate
from the live block cache. The open requirement is end-to-end availability across
local production, synchronization, replay, and restart.

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

Also require all missing mappings used by the recovery proof to be satisfied and
explicit acceptance of the leader-order model. The
[implementation gaps](../docs/IMPLEMENTATION_GAPS.md) list the remaining product
work.
