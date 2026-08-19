<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 proof scope

## Result

The formal model has no unfinished proof placeholders or declared axioms in its
current lemmas. It proves safety, evidence retention, unbounded network DAG
progress, network commit progress, and pointwise exact-reference catch-up. The
theorem `current_sources_give_end_to_end_liveness_probability_one` completes
the adopted ordinary-DAG route in Lean under proposed local source rules and
the independent-uniform first-slot law. Exact replay remains a separate
non-adopted proof experiment. See the plain-language
[safety and liveness properties](SAFETY_AND_LIVENESS.md).

The model is not an end-to-end proof of the product. A result applies to the
product only when all related environment conditions and product-mapping
conditions hold. The [assumption ledger](ASSUMPTIONS.md) is the complete list
for these conditions. The [gap report](IMPLEMENTATION_GAPS.md) lists required
product changes.

## Safety

The safety model uses weighted validator sets:

- `N` is total validator-set stake.
- `f` is the maximum Byzantine stake.
- `Q` is the quorum threshold.
- `A` is the certification threshold.
- `c` is the additional unavailable-stake budget.

Safety requires:

```text
N + f < Q + A
N + f + A <= 2Q
```

The nominal v3 construction is:

```text
N = 5f + 3c + 1
Q = 4f + 2c + 1
A = 2f + c + 1
```

The proof shows that this construction satisfies both inequalities. The main
results use actual stake and actual thresholds. They do not require every
committee to have the nominal total. For actual `N`, v3 uses `A = 2f + c + 1`
and `Q = N - f - c`, then checks both inequalities.

### Leader decisions

For one fixed selected leader slot, a commit result and a skip result cannot both
be valid. This covers all direct and indirect result pairs. An equivocating
validator can count once on each side, but it cannot count twice on one side.

This is a per-slot result. Global safety also requires all correct validators to
use one commit chain, leader schedule, round leader selection, and selected leader
slot order.

### Transaction decisions

For one transaction, an accept result and a reject result cannot both be valid. A
next-round block votes to accept only when it references the target, the target is
above the signed vote cutoff, and the block has no explicit rejection. Every
other present next-round block votes to reject. A missing block supplies no vote.

The proof covers direct against direct, direct against indirect, and indirect
against indirect results. Durable restart behavior remains a product obligation.

The signed cutoff is the maximum of the block-cleanup round and the
transaction-vote cleanup round. A target at or below either boundary receives a
reject vote. An accept vote is above both boundaries.

### Committed material

The model contains the Rust v3 commit materializer walk. A successful
`FlexCommitter::build_commit` run returns exactly the blocks that its ancestor
filter reaches from the committed leaders of the commit round: above the local GC
round and not taken by an earlier commit. The conditions are the reads that Rust
already relies on. Each leader body is local, every followed ancestor body is in
the local `DagState`, and the loop finishes on the finite store.
`Linearizer::linearize_sub_dag` is the one-leader pre-v3 form of the same walk.

The walk commits each block once. Two hosts whose walk reads agree commit the
same duplicate-free block set. The deterministic seeded sort then gives one
commit body and one commit digest. That sort keys on the block round and on a
hash of the commit seed and the block digest, so its determinism reduces to
collision freedom of that hash.

The weighted honest-parent bound holds for every block in a committed flush. Each
counted non-Byzantine parent reference is in the same flush, which supplies its
body; or it carries the committed mark of an earlier commit; or it is at or below
the GC boundary, where the walk stops. The third case is a boundary result, not a
durability claim. The result also applies to the sorted committed vector that
`build_commit` puts in the sub-DAG and in the serialized commit body.

### Adaptive leader schedule

The model contains the `LeaderScheduleV3` replay bookkeeping: the three-deep
pending window, the sliding score window, the boundary-only allowed-leader
refresh, and the shuffle seed taken from the last pending commit digest. It also
derives the next commit index and the minimum next leader round. The scorer input
is the sorted materializer flush, so the exact commit reference fixes it.

The scoring calculation itself is one deterministic function of the four
committed materials. The model does not reproduce its arithmetic; equal inputs
give equal score entries, which is what a shared schedule needs.

The running per-authority totals, leader count, and leader stakes always equal a
recomputation over the retained score entries. The Rust `checked_sub` on an
evicted entry therefore cannot underflow. Two correct hosts with exact installed
heads at one commit index reach the same replayed schedule state, so every read
that the proposer and the FlexCommitter take from it agrees.

## Evidence retention and old-block cleanup

The model proves that the pre-commit cleanup boundary keeps the leader block, its
next-round votes, and the path to a depth-two anchor.

For transactions, the proof covers targets that are far below or close to their
first commit leader. The close case uses the first eligible depth-two trigger and
requires a cleanup depth greater than two.

Evidence already copied into a pending committed prefix is separate from the live
block cache. Later cleanup of the live cache cannot remove that copied evidence.
The product still needs an end-to-end guarantee that all required evidence enters
the prefix on local, synchronization, replay, and restart paths.

For one continuous common commit stream, the first eligible trigger is unique and
stays first as the visible prefix grows. Correct validators with the same stream
and trigger make the same indirect decision.

This result does not apply to an arbitrary larger prefix. A later prefix can
introduce a new accept certificate and change an earlier indirect reject
calculation. Safety depends on ordered processing and the same first trigger.

## Conditional progress

The progress model uses partial synchrony. Before the network stabilization time,
messages can have any delay. After that time, messages between correct validators
arrive within `delta`. Required local work completes within a finite bound
`epsilon`, and enabled protocol tasks eventually run. The proof does not require
`epsilon < delta`.

### Consensus progress

`EndToEndLivenessInputs.network_dag_progress` proves unbounded network DAG
progress from the local proposal, subscription replay, ordinary block-sync,
current-GC, and handler rules. It does not take a future quorum layer or a commit
advance as an input.

The adopted commit proof uses one fixed reference round for all local waits.
V2 current no-idle sources derive a later own block for each correct, available
host. The V2 no-skip source recovers the finite exact round family that one
favorable path needs. It does not supply a future window. The strict proof
derives timer spread from actual prior broadcasts, pinned sync, and one
action-local exact-next timer-promptness rule. Its source record contains only
static, local, current, or past facts.

Pinned ordinary block sync and commit-orthogonal retention make each selected
leader usable at the receiver before the next proposal snapshot. The resulting
direct range feeds one actual local FlexCommitter run. Exact-prefix induction
turns receiver-local progress into network commit progress and pointwise
catch-up. A local commit-head change does not split this proof into separate
no-ahead and already-ahead routes.

Lean also has a separate exact-replay proof experiment for the ahead case. It
saves the exact material from a past successful Flex run, sends a reference
manifest, fetches the bodies parent-first, and replays Flex on only that
material. It is proposed behavior, not current Rust subscription replay, and
not an adopted liveness design.

The target strong result starts after both network stabilization and catch-up
activation. It uses safe intermediate proposals after round jumps, a live-leader
rule, and continued task execution. This result is intended to cover old leader
blocks.

The model includes a local counterexample in which a direct jump omits one required
proposal. It does not claim that the complete published attack applies unchanged
to v3.

For one favorable leader window, the current stage-composition model gives a
`10 * delta` bound. This is a sum of supplied stage bounds, not a derived product
latency.

### Commit progress recovery

The model proves the stake, next-round, direct-vote, pending-round, anchor-scan,
and local Flex-install lemmas. It also proves the same-head timing and causal
catch-up route. `BlockId` has a fixed finite encoding. Unique in-range
references give one static per-round cap. The receiver GC round changes this cap
into a linear unresolved-history bound.

For one fixed reference round `R_c`, the proposed wait is:

```text
W(R) = b + l * (R - R_c) + q * (R - R_c)^2
```

The coefficient conditions make each adjacent wait margin dominate the
pointwise causal-visibility and timer-spread cost. The wait value does not use
the local commit head. The proof first selects a favorable base above every
numeric lower bound. It then derives the finite exact V2 production family,
pinned sync, accepted retention, and adjacent parent edges. This order avoids a
circular favorable-window choice.

The exact persisted capsule projection, proposal origin, timer-spread source,
and accepted retention rules are current or past refinements. They do not state
future progress. The separate exact-replay experiment does not contribute to
the final theorem.

The network-DAG result has no commit alternative. At every requested round,
it requires a later positive total-stake quorum layer held by one correct,
available validator. The holder has one exact accepted, retained, and
catalogued valid body for each selected author, above its local GC boundary.
This public network-DAG result does not require each correct validator to
produce its own block or to hold the layer. The commit proof separately derives
unbounded later own-block production at each correct, available validator. Its
proposed no-skip source reconstructs only the finite contiguous window that the
selected favorable path needs. The commit proof keeps stronger
correct-authored receiver windows inside the construction. It requires
unbounded exact commit references
and pointwise installation of each such reference at every correct, available
validator. Ordinary DAG blocks and recursive above-GC causal-history fetch
supply the positive local view. Commit synchronization and commit votes are not
liveness dependencies. Commit sync can stop after commit-index catch-up while
the local ordinary DAG still lags. It cannot prove DAG availability or replace
the normal block-sync route.

The pointwise catch-up finish can be different for each validator and commit
reference. The proof does not claim one fixed numerical lag bound. It also does
not require a correct validator's own blocks or transactions to enter a commit.

Commit progress recovery does not prove that every old correct leader block or
every transaction commits. See the
[recovery design](../design/commit_progress_recovery.md).

### Transaction progress

The current transaction theorem composes supplied commit-stream, trigger,
decision, storage, and consumer stages. It does not yet derive transaction
progress from fundamental inputs. The epoch-tail case also remains open.

Transaction payload retention is not required. A validator or user can submit a
transaction again.

## Leader conditions

Let `S` be leader-schedule stake and `P_r` be round-leader-selection stake in
round `r`. Progress uses:

```text
P_r <= S <= N
f + c < S
A <= P_r
```

Current v3 selects the full schedule in each pending leader round, so `P_r = S`.
The optional `P_r <= Q` limit controls work. Per-slot safety and quorum coverage
do not require it. A larger selection can still affect the ordered anchor scan.

The leader-order model treats each round-seeded shuffle as an independent
uniform pseudorandom permutation of the current allowed-leader list. All correct
validators still compute the same deterministic result. The list is fixed
before that round's shuffle. Lean uses only the first selected slot. Thus, full
tail uniformity is stronger than the proof needs. This is not a claim that the
runtime uses fresh entropy.

## Conditions and proof goals

Entries whose type is **Derived** are proof goals. They are not inputs to the
final liveness theorems.

The results depend on these groups of conditions:

- **Safety:** `ASM-MATH-THRESHOLDS`, `ASM-SAFE-PARAMETERS`,
  `ASM-SAFE-FAULT-BOUND`, `ASM-SAFE-AUTHENTICATION`,
  `ASM-SAFE-NON-EQUIVOCATION`, `ASM-SAFE-PARENT-QUORUM`,
  `ASM-SAFE-EVIDENCE-REFINEMENT`, `ASM-SAFE-COMMIT-CHAIN`,
  `ASM-SAFE-FIRST-TRIGGER`, `ASM-SAFE-COMMITTED-PREFIX`, and `ASM-SAFE-GC`.
- **Configuration:** `ASM-CONFIG-V3-ACTIVATION`, `ASM-CONFIG-VOTING`, and
  `ASM-REFINE-INTEGERS`.
- **Network and runtime:** `ASM-LIVE-PARTIAL-SYNCHRONY`,
  `ASM-LIVE-PEER-FAIRNESS`, `ASM-LIVE-TASK-FAIRNESS`,
  `ASM-LIVE-LOCAL-RESPONSE`, `ASM-LIVE-PIPELINE-BOUNDS`, and
  `ASM-LIVE-COMMIT-SYNC`. The commit-sync condition is resource isolation for
  the ordinary path. It is not a commit-sync success assumption.
- **Consensus progress:** `ASM-LIVE-FINITE-REFERENCE-SPACE`,
  `ASM-LIVE-ROUND-CATCHUP`,
  `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`, `ASM-LIVE-LOCAL-PROPOSAL`,
  `ASM-LIVE-LEADER`, `ASM-LIVE-FIRST-SLOT-SAMPLING`,
  `ASM-LIVE-POST-GST-CAUSAL-SERVICE`, and `ASM-LIVE-BLOCK-SYNC`.
- **Transaction progress:** `ASM-LIVE-FINALIZER-TRIGGER` and
  `ASM-LIVE-DURABILITY`.

The public network-DAG theorem does not use `ASM-LIVE-ROUND-CATCHUP`. The final
fixed-reference commit theorem does use it for its finite internal window.

## Current product status

Source review and focused tests support the current leader-decision mapping,
including thresholds, authentication, unique voter stake, parent quorum, common
ordering from fixed compatible inputs, decision scans, local evidence ownership,
and commit recording. This evidence is not a machine-checked proof that the source
follows the model.

The commit materializer and adaptive-schedule results above are stated on models
of the running Rust loops. Their open source rows are
`REF-COMMIT-MATERIALIZER-WALK`, `REF-COMMIT-BODY-ORDER`,
`REF-V3-SCHEDULE-SCORER`, and `REF-V3-SCHEDULE-READERS`.

The current product does not implement commit progress recovery, the
fixed-reference quadratic wait, or the V2 no-skip proposal sequence. Exact
successful-Flex material replay is a non-adopted proof experiment and is not a
current product requirement. Normal startup does
not enable the analyzed v3 path from shared epoch state. The product also does
not implement the modeled signed v3 transaction voting and finalization path.
Therefore, the related recovery and transaction results do not yet describe
product behavior.

The current review also does not prove the V2 current no-idle sources, pinned
sync sources, commit-orthogonal retention, exact-next timer promptness,
authenticated correct-body ownership, past recovery-timer origin mapping, or
the strict post-GST causal-service margin under the fastest permitted round
creation. These rules are conditional inputs to the completed Lean theorem.

Other open work includes common epoch parameters, complete commit-chain agreement,
synchronization progress, committed-prefix evidence, integer bounds, stable leader
ordering across compatible versions, and finalizer shutdown behavior.
