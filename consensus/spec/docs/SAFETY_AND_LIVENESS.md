<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Safety and liveness properties

This document gives the proof contract in plain language. It states what the
Lean model proves, what the end-to-end liveness theorem must prove, and what the
theorem does not require.

The [proof scope](PROOF_SCOPE.md) gives more detail. The
[assumption ledger](ASSUMPTIONS.md) lists all conditions. The
[implementation gaps](IMPLEMENTATION_GAPS.md) lists the checks and product work
that remain.

## Safety properties

The following safety results are proved in Lean, subject to the conditions in
the assumption ledger. The aggregate result is `mysticeti_v3_safety`.

### Leader decisions do not conflict

For one selected leader slot, two correct validators cannot get conflicting
final results. In particular, one valid view cannot commit the slot while
another valid view skips the same slot.

This result covers direct and indirect decisions. It permits Byzantine
equivocation, but each author is counted at most once on each side of a
decision.

### Transaction decisions do not conflict

For one transaction, two correct validators cannot get conflicting final
results. One valid view cannot accept the transaction while another valid view
rejects it.

This result covers direct and indirect transaction decisions. It also includes
the signed cleanup cutoff that controls old transaction votes.

### Correct validators agree on the commit at each index

Two correct, available validators that have installed a commit at one index have
installed the same commit. The commit head carries the index, the commit
identifier, and the leader round, so this is exact agreement on the digest.

`ExactCommitInstallProvenance.correct_validators_agree_on_commit_at_index`
states it. Safety has two parts, and the specification proves both: per-slot
exclusion in `mysticeti_v3_safety`, and this same-index agreement.
`MysticetiSafetyCapstone` names them together and lists what the second one
assumes.

### Exact commit references form one prefix

An exact commit reference is the pair `(commit index, commit digest)`. Correct
validators cannot install different digests at the same commit index when the
authenticated Flex-evidence, exact durable-prefix, and install-provenance rules
hold.

If a correct validator installs a later exact reference, its durable installed
prefix contains every earlier exact reference in that chain. A later local
FlexCommitter run cannot silently replace an earlier exact successor with a
different successor. The local successor result is
`later_correct_local_flex_run_cannot_skip_exact_successor`.

### GC does not change old decisions

GC can remove live block bodies, but it cannot change an installed commit or
the evidence that was already copied into the durable committed prefix.

For recovery, a validator does not fetch or accept a block body at or below its
own GC round. It treats such a reference as an old committed root. It fetches
and accepts only the causal history that is above its own GC round.

This cutoff is local. A source can still serve an older stored block to a peer
whose GC round is lower.

### The v3 threshold construction is tight

Quorum availability and the two weighted safety inequalities require:

```text
N >= 5f + 3c + 1
```

`Thresholds.hybrid_total_lower_bound` proves this lower bound from a checked
threshold structure and an unavailable-stake budget.

At equality, those three guarantees force the nominal values:

```text
Q = 4f + 2c + 1
A = 2f + c + 1
```

`Thresholds.tight_hybrid_total_has_nominal_thresholds` proves these two values
at equality.

This is tightness of the weighted intersection and availability guarantees. It
does not state that every unsafe parameter choice occurs in one executable
protocol trace.

### Causal reads contain honest parent stake

Each valid Mysticeti block has immediate parents from distinct authors with
quorum stake. After the Byzantine stake bound is removed, the remaining
non-Byzantine parent-author stake is at least `Q - f`. For every block above
round one in a causal recovery capsule, the capsule contains the exact parent
bodies that realize this bound. The stake result is
`ValidatorBlock.non_byzantine_parent_stake_at_least_quorum_minus_fault`.

This is a weighted, per-layer quality result. It is not Narwhal's raw block-count
result for a complete causal read. Mysticeti accepts uncertified blocks and its
causal history can contain Byzantine equivocations. Such extra blocks can make
an unrestricted raw honest-block fraction false.

### Every committed flush carries the same honest parent stake

The same bound holds for the block set that the product actually commits. With
`enable_v3`, one committed flush is one `FlexCommitter::build_commit` result. The
sorted vector of that flush goes into the `CommittedSubDag` and into the
serialized `CommitV1` body, and the bound holds for every block of it.
`CommitMaterializerView.sorted_commit_block_has_non_byzantine_parent_layer`
states this result for the sorted vector.

For each block in a flush and each distinct non-Byzantine parent author counted
by the bound, the exact parent reference stands in one of three places: the same
flush, which supplies the body; the committed mark that an earlier commit set;
or at or below the local GC round, where the walk stops. The third place is a
boundary result. A reference below the boundary can have been pruned without
entering any commit body. The counted parents are the
one-round-below layer that carries quorum stake, so the third place is reachable
only one round above the boundary. Above that round the complete honest layer is
inside the installed committed prefix. The stronger boundary result is
`CommitMaterializerView.committed_flush_block_covers_non_byzantine_parent_layer`.

A local commit sets its earlier marks through this same walk. A synchronized
certified commit sets them through `FlexCommitter::handle_certified_commit`
instead, so the earlier-marks set is the union of both routes.

### A committed flush is the exact eligible causal closure

The Rust materializer walk returns exactly the blocks that its ancestor filter
reaches from the committed leaders of the commit round. The filter follows a
reference only when the reference is above the local GC round and no earlier
commit took it.

Inclusion is conditional on the reads that Rust already relies on. Each leader
body is in the local `DagState`, and every followed ancestor body is present.
These reads are the `get_block(..).unwrap()` obligation inside the walk. The
Lean view records a finite, duplicate-free catalog domain.
`buildCommit_terminates` proves that the walk finishes with fuel at most one
more than the domain length. The walk also commits each block once. The leader
`set_committed` assertions, the repeated-reference `continue`, and the verifier
rule against two ancestors from one author supply this property.

Two hosts whose walk reads and ordered committed-leader decisions agree commit
the same duplicate-free block set. The deterministic seeded sort turns that set
into one commit body and one commit digest.

### Different block-GC horizons keep one durable prefix

If two correct validators cover one absolute commit index, their durable exact
commit entries agree at every index through that point. Their block-GC rounds
do not need to be equal or ordered. This result concerns installed commits. It
does not compare pending Flex decisions that two validators recompute from
different pruned DAG views. The theorem is
`ExactCommitInstallProvenance.exactInstalledPrefixesAgreeAcrossGcHorizons`.

### The adaptive schedule has one fixpoint

The v3 schedule is adaptive, so leader identity flows upward from committed
scores while a decision flows downward from its anchor. Without a restriction,
two schedules could each justify themselves.

Rust prevents this with `min_next_leader_round`. That value is one round above
the leader round of the last commit in the pending window. `FlexCommitter`
asserts that it is identical at one commit index, that it moves strictly forward
with the index, and that a schedule change drops every pending round below the
new gate. A schedule therefore governs only rounds at or above its gate, and
every commit that produced it has a leader round below that gate.

The decision at one round then reads a schedule built from strictly earlier
rounds. That stratification makes the recursion well founded, so at most one run
of decisions is consistent with the rule.

The proof is induction on the round. Agreement below a round gives one schedule
at that round, and one schedule gives one verdict. Prefix agreement is the
conclusion at each step, not a hypothesis.

Sequence agreement follows. `v3_committed_candidates_agree` states it at the
model's commit types: two consistent runs install the same ordered exact commit
candidates through every round bound, so they agree on the commit sequence and
on the leaders inside each commit. The common commit chain follows for the
adaptive schedule instead of being assumed.

The safety statement itself does not need a common per-host model.
`ExactCommitInstallProvenance.correct_validators_agree_on_commit_at_index`
proves it from authenticated Flex votes and install provenance, and neither
input holds a cross-host equality. The fixpoint result justifies the head-indexed
signatures that those inputs use, such as `firstPendingRoundForHead`. A second
route through a common per-host model was removed. It required cross-host
agreement of the commit material and the post-scan slots, and it added no safety
result.

### Exact commit prefixes fix adaptive schedule replay

For deterministic v3 scoring rules, two correct installed heads at the same
index replay to the same schedule state. They therefore have the same ordered
allowed-leader vector and the same selected order for each round. The model-level
result is
`ExactCommitInstallProvenance.exactInstalledHeadsAtSameIndexShareV3Schedule`.

The Lean model now contains that replay in the shape `LeaderScheduleV3` runs. It
keeps the three-deep pending window, scores `C-3` against `[C-2, C-1, C]`, reads
the exact commit index, the commit digest, the named leader, and the sorted
committed block bodies, and recomputes the allowed-leader vector only at an
update-interval boundary with a shuffle seeded from the last pending commit
digest. It also derives the next commit index and the minimum next leader round
that the proposer gate and the FlexCommitter read. The scorer input is the
materializer flush above. A mapped V3 run has at least one committed leader, and
all committed leaders have the commit-head round. The source map also requires
the same ordered committed-leader decisions for one exact head. Exact decision
replay must supply this condition because the sort seed uses leader-digest order.

The scoring calculation is one deterministic function of the four committed
materials. The model does not reproduce its arithmetic, because a shared schedule
needs only that equal inputs give equal results.

The model also proves that the incremental Rust accumulators are exact. The
running per-authority totals, the leader count, and the leader stakes always
equal a recomputation over the retained score entries. The `checked_sub` on an
evicted entry therefore cannot underflow, and a restart that recomputes from the
retained window reaches the same totals.

Two correct hosts with exact installed heads at one commit index therefore reach
the same replayed schedule state, so every read they take from it agrees. The
scored input at each step is one host's actual materializer output, and any other
host whose ordered leader decisions and walk reads agree computes the same input.

The remaining product conditions are the sort determinism for the commit body and
the source rules that bind these reads to the running code. They are listed as
`REF-COMMIT-MATERIALIZER-WALK`, `REF-COMMIT-BODY-ORDER`,
`REF-V3-SCHEDULE-SCORER`, and `REF-V3-SCHEDULE-READERS` in the assumption
ledger.

## End-to-end liveness properties

The final end-to-end theorem has three parts. These are the public liveness
goals in `ValidatorProcess.lean` and `EndToEndLiveness.lean`.

The required suffix starts after GST, while the epoch stays active. The
properties apply to validators that are correct and available under the fixed
fault model.

### 1. Unbounded network DAG progress

From every post-GST start state and for every requested minimum round, some
correct, available validator eventually holds a complete valid total-stake
quorum layer at or above the requested round. The layer can contain Byzantine
blocks. The holder has the exact bodies and has accepted them.

A local or synchronized commit does not satisfy this property. It can move GC
and can change the leader schedule at a schedule boundary. The next frontier
step reads the resulting current state. An in-flight block fetch completes
independently. When its result is processed, blocks at or below the current GC
round are dropped, and only above-GC dependencies remain acceptance work. A
schedule change can affect the early normal leader wait. The forced
max-timeout progress rule does not use that wait.

`ValidatorCoreHandlerInputObservation`,
`ValidatorPacketDrivenBlockAcceptanceAt`, and
`ValidatorCoreHandlerRefinementRules` provide separate implementation evidence
for the finite single-threaded Core path. The completed network-round theorem
does not use a commit result, commit synchronization, or that handler episode
as its progress result.

The packet-driven source map must distinguish direct handler-input acceptance
from later GC-unsuspension. A GC-unsuspended block stays in its enclosing
handler episode. It does not start a new positive DAG handler.

Repeating this property after every start gives an infinite common ordinary
DAG. This is the first proof stage. It does not use a FlexCommitter result,
commit installation, commit certificate, or future commit-sync service as a
liveness step.

### 2. Unbounded network commit progress

For every requested commit index, some correct, available validator eventually
installs an exact commit reference at that index or a later index.

The approved proof derives a stronger pointwise fact: each correct, available
validator later makes a local commit at or above the requested index. Validators
can finish at different times. This stronger result follows from the infinite
common DAG and recurring fresh favorable leader windows. It implies this
network property and supports the next exact-reference property.

### 3. Pointwise commit catch-up

If one correct, available validator installs an exact commit reference at or
after GST, then each correct, available validator eventually installs that same
exact reference.

The completion time can be different for each validator and each reference.
The theorem does not give one fixed numerical lag bound for all future commits.

The adopted positive proof path uses ordinary DAG blocks. A fixed-reference
quadratic wait aligns the local proposal windows. V2 current no-idle sources
derive later own blocks. A proposed no-skip source reconstructs the finite
intermediate round family that one favorable path needs. Pinned ordinary block
sync and commit-orthogonal retention make the selected leaders usable at each
queried receiver. Local FlexCommitter execution and exact-prefix induction then
derive the same exact reference at every correct, available validator.

Lean has a separate proved experiment that saves exact material from a past
successful Flex run, sends a reference manifest, fetches the exact bodies
parent-first, and runs a material-scoped replay action. It does not take a
future replay result as an input. This experiment is proposed behavior, not
current Rust subscription replay, and not an adopted liveness route.

An actual synchronized install can close an already occurring race, but future
commit-sync service, commit certificates, and commit votes are not liveness
premises. Commit sync can stop after commit-index catch-up while the ordinary
DAG still lags. Normal DAG propagation and block sync must complete the
liveness route.

Commit sync is covered by this conditional split:

1. If a verified synchronized install increases receiver `B`'s commit index,
   `B` has completed the current pointwise progress step. Exact sync-install
   provenance and exact-prefix safety constrain the installed reference.
2. If `B`'s commit index never increases on the selected suffix, no local or
   synchronized commit install can repeatedly cancel work on that suffix. The
   ordinary recovery theorem supplies progress.

This split needs no future commit-sync result. Product refinement still assumes
that commit-sync traffic and work cannot starve ordinary block sync,
subscription retry, proposal callbacks, or recovery timers. Rust supplies
subscription-resume and periodic-sync-failover control paths. Their eventual
service uses the existing partial-synchrony, peer-fairness, task-fairness,
block-sync, and queue-service assumptions.

GC has the same split. A newer GC round comes from a local commit advance, so it
already completes the current progress step. BlockManager also removes missing
dependencies at or below GC and releases their children. For the next progress
step, the exact no-skip recovery target still needs the existing no-idle and
safe-resume refinement. GC cleanup alone does not prove that scheduler rule.

## Properties that are not core liveness requirements

The end-to-end theorem does not require these stronger properties:

- Every correct validator produces an own block in every round as a public
  liveness result. The final conditional proof derives unbounded later
  authorship. Its proposed no-skip source reconstructs only one finite internal
  window.
- Every correct validator gets its own transactions into commits.
- Every honest proposal commits.
- All correct validators stay within one fixed round or commit-index distance.
- All correct validators enter each round at nearly the same time.
- Commit sync or commit votes make progress.

These properties can improve fairness, latency, or recovery. They are separate
from the core safety and liveness result. A lagging validator can make commit
progress with blocks from other validators.

## Transfer budget for catch-up

A correct validator can receive at most a fixed number of whole blocks in one
`delta`. The budget is coarse: a larger block does not take longer to transfer.

The budget is assumed large enough for ordinary round advancement, with room to
spare. The production bound is the committee size times a positive
rounds-per-interval bound. One author produces at most one block in each round.
The base queue also has a removal cap. The transfer model sets this cap to the
interval budget. The cap therefore binds only on bulk movement, which is
recursive causal block sync and commit sync over many blocks. The surplus over
what round advancement demands is the rate at which a lagging validator drains
its backlog.

This replaces an assumed service margin with an inequality between two named
quantities. It also makes the failure mode explicit: if arrivals reach the
budget, the backlog cannot shrink, so catch-up stops. The proof states that
necessity separately from the progress result.

## Leader-order and probability boundary

The deterministic theorem must derive each favorable leader window from one
specified leader-order rule. The accepted probability model treats each
round-seeded shuffle as an independent uniform pseudorandom permutation of the
current allowed-leader list. It proves that favorable future windows occur with
probability one after the deterministic composition is complete.
`independent_uniform_finite_failure_bound` gives the exact finite bound.
`every_fixed_start_has_favorable_window_probability_one` gives the operational
probability-one result for each fixed start.
`favorable_window_transfers_to_liveness_probability_one` transfers that result
through a deterministic liveness implication.

Lean uses only the first selected slot from each permutation. Current Rust uses
one deterministic round seed so all validators get the same result and can
reproduce it after a crash. A deterministic repeated-first order is a separate
proved option, but current Rust does not use it. The separate theorem is
`deterministic_repeated_first_depth_instance`.

## Required commit-install and GC order

Every commit install that can move the local GC round must use this local order:

1. Verify the exact commit and its block material.
2. Accept every block in that commit into the local `DagState`.
3. Install the commit head and move GC.
4. Retain the accepted, closed DAG frontier that is above the new GC round.

This rule applies to a local commit and to a completed synchronized commit. It
does not assume that another synchronized commit will occur.

The usable frontier can contain blocks from earlier installed commits. A single
commit record contains only the blocks that became newly committed in that
record. Therefore, the model requires the accumulated local above-GC frontier;
it does not require the current commit record to equal the complete frontier.

The current v3 synchronized-commit path accepts each certified commit's blocks
before it processes and records that commit. The full Lean-to-Rust refinement
must also check that the required above-GC closure stays accepted, catalogued,
and retained after the GC update.

## Proof status

The safety results above are kernel-checked under their stated inputs. The
lower recovery, GC-cutoff, block-sync, timer, local FlexCommitter, and exact
prefix lemmas also compile without `sorry`, `admit`, or declared axioms.

The first stage is complete in Lean. The theorem
`EndToEndLivenessInputs.network_dag_progress` derives unbounded correct-held
total-quorum layers. It uses `operational_frontier_pacemaker_gives_strict_progress`
for one `H` to `H + 1` step and
`operational_frontier_strict_progress_gives_network_dag_progress` for finite
iteration to any requested round.

This result is conditional on the local source maps in
`EndToEndLivenessInputs`. Current Rust has the required high-level mechanisms:
one forced maximum-timeout attempt, watcher retries for the two temporary
proposal blockers, and receiver-driven subscription replay of an exact or newer
own tip. The exact Rust-to-Lean trace mappings remain open.

The Lean model now has the local finite Core-handler boundary in
`ValidatorCoreHandlerRefinementRules`. The open source refinement must map this
boundary to the guarded ordinary `Core::add_blocks` path, including exact
packet-driven acceptance origin, finite GC-unsuspension, terminal `None`, and
the following `try_propose(false)` invocation. Certified-commit processing is
outside this positive DAG boundary and needs a separate model.
`ValidatorFiniteCoreHandlerEpisode.proposal_attempt_input_and_suffix` exposes
the attempt input and the finite suffix to the next trace state. The separate
`ValidatorCoreProposalAttemptContinuationRules` now classifies an exact proposal
action already in that suffix, exact protected normal work, or current durable
retry work. The open Rust mapping must justify this classification. The
collective frontier proof derives future DAG progress from the current work.
Commit output is not a DAG-progress result.

The second stage is complete in Lean under the proposed source rules. The final
theorem is `current_sources_give_end_to_end_liveness_probability_one`. It uses
fixed-reference pacing, V2 no-skip round catch-up, V2 current no-idle block
production, pinned sync, commit-orthogonal retention, local Flex execution, and
exact-prefix induction. It does not use a future layer, carrier block, anchor,
successful run, commit-sync result, or commit-vote result as an input.

The strict proof derives timer spread from actual prior broadcasts and pinned
sync. Its only promptness input is one action-local rule for an already-actual
exact-next timer. The other strict source fields are static, local, current, or
past.

The commit materializer and the v3 schedule replay are now modeled on the
running Rust loops. `CommitMaterializerView.buildCommit_materializes_exactly`
fixes the committed block set, `buildCommit_output_nodup` proves it is duplicate
free, `committed_flush_block_has_non_byzantine_parent_layer` carries the weighted
honest-parent bound to it, and
`ExactCommitInstallProvenance.exactInstalledHeadsAtSameIndexShareRustSchedule`
gives two correct hosts one replayed schedule state at one commit index. The open
rows are `REF-COMMIT-MATERIALIZER-WALK`, `REF-COMMIT-BODY-ORDER`,
`REF-V3-SCHEDULE-SCORER`, and `REF-V3-SCHEDULE-READERS`.
`CommitMaterializerView.buildCommit_terminates` now derives a finite fuel bound
from the finite catalog domain.

One safety-refinement obligation remains open. Lean now keeps the first direct
or indirect origin, the exact historical indirect evidence, and the ordered
first-anchor scan. It proves that later passes preserve the first result. Rust
records only `Direct` or `Indirect`; it does not retain the exact indirect anchor
and history. Rust must store them or provide a checked same-host reconstruction
across retained state, reset, and restart. Lean does not treat a cached indirect
result as a fresh direct quorum.
