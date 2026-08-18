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
the assumption ledger.

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

### Exact commit references form one prefix

An exact commit reference is the pair `(commit index, commit digest)`. Correct
validators cannot install different digests at the same commit index when the
authenticated Flex-evidence, exact durable-prefix, and install-provenance rules
hold.

If a correct validator installs a later exact reference, its durable installed
prefix contains every earlier exact reference in that chain. A later local
FlexCommitter run cannot silently replace an earlier exact successor with a
different successor.

### GC does not change old decisions

GC can remove live block bodies, but it cannot change an installed commit or
the evidence that was already copied into the durable committed prefix.

For recovery, a validator does not fetch or accept a block body at or below its
own GC round. It treats such a reference as an old committed root. It fetches
and accepts only the causal history that is above its own GC round.

This cutoff is local. A source can still serve an older stored block to a peer
whose GC round is lower.

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

The positive proof path uses ordinary DAG blocks. Each lagging validator gets
later common DAG layers, uses a fresh favorable window, runs its local
FlexCommitter, and installs a later exact reference. Exact-prefix safety then
shows that its durable prefix contains the earlier exact reference. The later
run does not have to reproduce the source validator's view or output.

An actual synchronized install can close an already occurring race, but future
commit-sync service, commit certificates, and commit votes are not liveness
premises.

## Properties that are not core liveness requirements

The end-to-end theorem does not require these stronger properties:

- Every correct validator produces its own blocks at unbounded rounds.
- Every correct validator gets its own transactions into commits.
- Every honest proposal commits.
- All correct validators stay within one fixed round or commit-index distance.
- All correct validators enter each round at nearly the same time.
- Commit sync or commit votes make progress.

These properties can improve fairness, latency, or recovery. They are separate
from the core safety and liveness result. A lagging validator can make commit
progress with blocks from other validators.

## Leader-order and probability boundary

The deterministic theorem must derive each favorable leader window from one
specified leader-order rule. The canonical probability model uses one common
independent uniform validator ranking for each round and proves that favorable
future windows occur with probability one, after the deterministic composition
is complete.

Current Rust uses a deterministic round-seeded shuffle. The proof does not claim
that this shuffle is an independent uniform sample or that it has the same
coverage. A deterministic repeated-first order is a separate proved option, but
current Rust does not use it.

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

The remaining second-stage proof must combine that infinite common DAG with
recurring favorable leader windows and actual prepared Flex scans to derive
unbounded local commit progress at every correct, available validator.
Exact-prefix safety must then derive exact-reference catch-up. A future layer,
favorable window, successful run, commit-sync result, or commit-vote result must
not be added as a theorem input.

One safety-refinement obligation remains open. Lean now keeps the first direct
or indirect origin, the exact historical indirect evidence, and the ordered
first-anchor scan. It proves that later passes preserve the first result. Rust
records only `Direct` or `Indirect`; it does not retain the exact indirect anchor
and history. Rust must store them or provide a checked same-host reconstruction
across retained state, reset, and restart. Lean does not treat a cached indirect
result as a fresh direct quorum.
