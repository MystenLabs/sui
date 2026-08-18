<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 proof scope

## Result

The formal model has no unfinished proof placeholders or declared axioms in its
current lemmas. It proves safety, evidence retention, local liveness lemmas, and
stage composition. It does not yet prove the final network-progress and
commit-catch-up result from only fundamental network conditions and
single-validator rules. See the plain-language
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

The current consensus progress result composes assumed protocol stages. Its input
already supplies a favorable leader window, proposal and certificate delivery,
decision, and commit progress. It is not yet a block-production theorem from
partial synchrony.

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

The model proves stake, timing, next-round, direct-vote, pending-round,
anchor-scan, and stage-composition lemmas. However, its current top recovery
theorem still takes abstract contracts that state recovery quorum stake,
visibility of each recovery block, timely next-round inclusion, an eventual favorable
leader trace, and later local commit recording. An optional lower helper also
takes parent-quorum availability. These inputs contain important distributed
progress results.

The final theorem must derive these distributed results from fundamental inputs
and local validator rules. The
[liveness proof plan](../design/liveness_proof_plan.md) defines the boundary and
proof order.

The final network result has no commit alternative. At every requested round,
it requires a later positive total-stake quorum layer held by one correct,
available validator. The holder has one exact accepted, retained, and
catalogued valid body for each selected author, above its local GC boundary.
The result does not require each correct validator to produce its own block or
to hold the layer. The commit proof keeps stronger correct-authored receiver
windows inside the construction. It requires unbounded exact commit references
and pointwise installation of each such reference at every correct, available
validator. Ordinary DAG blocks and recursive above-GC causal-history fetch
supply the positive local view. Commit synchronization and commit votes are not
liveness dependencies.

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

The leader-order model treats each round's complete order as a common independent
uniform permutation. The formal trace assumes the almost-sure favorable run from
this probability model. It does not prove the probability result. The product uses
a deterministic round-based shuffle, and the proof does not establish the same
coverage for that exact sequence.

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
  `ASM-LIVE-LOCAL-RESPONSE`, and `ASM-LIVE-PIPELINE-BOUNDS`.
- **Consensus progress:** `ASM-LIVE-ROUND-CATCHUP`,
  `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`, `ASM-LIVE-LOCAL-PROPOSAL`,
  `ASM-LIVE-LEADER`, `ASM-LIVE-FIRST-SLOT-SAMPLING`,
  and `ASM-LIVE-BLOCK-SYNC`.
- **Transaction progress:** `ASM-LIVE-FINALIZER-TRIGGER` and
  `ASM-LIVE-DURABILITY`.

Commit progress recovery does not use `ASM-LIVE-ROUND-CATCHUP`.

## Current product status

Source review and focused tests support the current leader-decision mapping,
including thresholds, authentication, unique voter stake, parent quorum, common
ordering from fixed compatible inputs, decision scans, local evidence ownership,
and commit recording. This evidence is not a machine-checked proof that the source
follows the model.

The current product does not implement commit progress recovery. Normal startup
does not enable the analyzed v3 path from shared epoch state. The product also does
not implement the modeled signed v3 transaction voting and finalization path.
Therefore, the related recovery and transaction results do not yet describe
product behavior.

Other open work includes common epoch parameters, complete commit-chain agreement,
synchronization progress, committed-prefix evidence, integer bounds, stable leader
ordering across compatible versions, and finalizer shutdown behavior.
