<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Liveness proof boundary and plan

## Status

This document defines the required boundary for the Mysticeti v3 liveness proof.
Both the network-DAG stage and the ordinary-DAG commit stage meet this boundary
in Lean under proposed local source rules. The final theorem is
`current_sources_give_end_to_end_liveness_probability_one`. Lean also has a
separate conditional exact-replay experiment. That experiment is not the final
design.

The target model is standard **partial synchrony**. Message delay is unrestricted
before an unknown stabilization time. After stabilization, messages between
correct validators have a finite delay bound.

See [Flex committer execution](flex_committer_execution.md) for the local
pending-state refresh, conditional fixed-frontier rank, Rust terminal
fixed-point control flow, and later proposal attempt.
`ValidatorCoreHandlerRefinementRules` states the complete finite Core-handler
boundary. Its Rust source mapping is still open. The interface must not become
a future liveness premise.

The proof has two positive stages:

1. **Unbounded network DAG progress:** from every post-stabilization start and
   requested round, one correct validator later holds an exact positive
   total-stake quorum layer at that round or a later round. Every selected block
   is accepted, retained, catalogued, above local GC, and has a quorum of
   immediate parents. A commit can restart one construction, but it is not a
   substitute for DAG growth.
2. **Per-validator commit progress:** for each fixed correct, available
   validator and requested commit index, stronger correct-only recovery windows
   retained inside the first-stage construction and recurring favorable leader
   windows give a later local FlexCommitter install at that index or a later
   index.

Unbounded network commit progress follows from the second stage. Pointwise
commit catch-up follows from per-validator later local commits and exact-prefix
safety. The source validator's run and the lagging validator's later run can be
different.

The public network-DAG result does not require per-validator production. The
commit proof derives unbounded later own blocks through the V2 current source
package. Its proposed no-skip source reconstructs only one finite intermediate
window. The result does not require inclusion of a validator's own
transactions. It does not give one fixed numerical catch-up bound. A common
favorable window is an internal result for commit progress. Commit progress also
uses the leader decision rule.

In this document, **parent history** means a block and all parent blocks needed
to verify it.

## The theorem boundary

The main theorems can take basic environment conditions and simple behavior of
one validator as inputs. They must derive all behavior of a quorum or of the
complete protocol.

### Basic environment inputs

The following inputs are permitted:

- One fixed validator set and fixed fault sets during the stable proof interval.
- A Byzantine stake bound and an unavailable-stake bound.
- A fixed set of correct, available validators during the stable proof interval.
  Its quorum stake must be derived from the fault bounds.
- Bounded message delivery between correct validators after stabilization.
- Bounded work for protected local consensus actions.
- Fair execution of an action that stays enabled.
- Advancing correct clocks.
- A stated leader-order distribution, if the protocol uses a probability model.

A rotating unavailable set is not sufficient. It can disable a different local
action at each time while unavailable stake remains below the bound at each time.

### Permitted single-validator contracts

A single-validator contract can be somewhat abstract when it is intuitive, easy
to inspect, or a clear implementation target. Examples are:

- A correct validator signs at most one block in one round.
- It stores a proposal and the required parent history before it sends the
  proposal.
- It accepts a block only after it verifies the block and its required causal
  history. Lean must derive any finite causal capsule from this accepted
  closure. The capsule is not an independent liveness premise.
- It retains and serves active recovery data.
- It retries a missing-data request until the data arrives, the epoch ends, or
  its own durable commit prefix makes the request unnecessary.
- After GST, its sustained fetch, verification, and acceptance service for
  known above-GC causal work is strictly faster than the maximum rate at which
  advancing rounds can add required work to its queue.
- It can skip own rounds, but it eventually stores and sends own blocks at
  later unbounded rounds. It does not have to fill every intermediate own round.
- A proposal target stays enabled until the validator stores the proposal. A
  commit installation does not cancel all proposal work.
- A recovery proposal targets only the round after the validator's highest
  durable signed round.
- A later observed round does not replace that target or reset its deadline.
- The validator proposes when its local DAG has quorum immediate parents and its
  recovery deadline is ready.
- A recovery proposal does not wait forever for every selected leader slot.
- The validator applies a local commit result when the commit scan returns it.
- While the epoch is active, installing commits cannot cancel all enabled
  network proposal work. An enabled protected proposal is durable before send.
- Exact-next recovery proposes in `P + 1`. Cleanup safe resume is a separate,
  unproved exception.
- A local commit is installed only by the local commit-recording action.
- One qualifying external `add_blocks` handler finishes all commit work enabled
  by its finite nonempty accepted-block batch and finite GC-unsuspended blocks.
  The commit loop observes terminal `none`. Core then invokes the normal
  proposal attempt before the handler returns. This contract does not say that
  the attempt succeeds. Certified-commit processing is not a positive DAG
  handler input.
- If optional commit synchronization installs a commit, its exact reference and
  chain were verified. This is a safety rule, not a liveness mechanism. Commit
  sync can stop while the local ordinary DAG still lags.

These rules can use a quorum as a local guard. The proof must show that each
correct validator eventually obtains that local quorum.

### Source-to-model inputs

Source-to-model inputs are separate from environment inputs. They state that a
product value or action has the meaning used in Lean. Required mappings include:

- the authenticated epoch configuration, validator set, and thresholds;
- the local commit index, commit reference, protocol timestamp, and local
  installation time;
- the durable highest signed round and its block reference;
- accepted blocks, immediate parents, and the cleanup boundary;
- the leader schedule, ordered selected leader slots, and pending-round state;
- the commit candidate returned by the scan and the local action that records it.
- durable local block creation and addressed send actions;
- exact commit references and their chain relation;
- local commit recording;
- `ValidatorPacketDrivenBlockAcceptanceAt`,
  `packetDrivenAcceptanceHasInputOrigin`,
  `ValidatorCoreHandlerInputObservation`, and
  `ValidatorCoreHandlerRefinementRules`, from one delivered and accepted
  ordinary block through its qualifying nonempty `add_blocks` batch, every
  local scan, `post_commit`, and GC-unsuspension step, to terminal `none` and
  the following proposal-attempt invocation before return. The source map must
  distinguish direct input acceptance from GC-unsuspension inside an enclosing
  handler;
- `ValidatorCoreProposalAttemptContinuationRules` and
  `qualifying_core_handler_input_has_current_proposal_continuation`, from the
  already-actual normal attempt to a same-batch proposal action, exact protected
  normal work, or current durable proposal, parent-need, or timer work;
- ordinary block bodies, their direct parents, and exact-reference fetches above
  the requester's local cleanup round;
- synchronized commit installation checks, for safety only.

These mappings can be checked against source and tests. They must not assert a
distributed progress result.

### Results that must not be theorem inputs

The final theorem must derive all these results:

- Correct validators with quorum stake enter recovery.
- Parent quorums become available.
- Validators produce blocks in common rounds.
- Consecutive quorum block layers exist.
- A correct first selected leader is included by next-round quorum stake.
- A favorable leader window occurs.
- Usable anchors exist.
- The commit scan returns a new commit.
- A local commit index increases.
- Required ordinary block synchronization completes.
- Correct-held exact total-quorum layers at arbitrarily high rounds.
- The private correct-authored consecutive windows needed by the commit proof.
- Recurring fresh favorable leader windows on that DAG.
- Unbounded local commit installation at each correct, available validator.
- Exact-prefix agreement and pointwise inclusion of every earlier exact
  reference.

An internal stage-composition lemma can use one of these results. A final
liveness theorem must prove the result from the basic inputs, single-validator
contracts, and source-to-model mappings.

## Completed composition and remaining refinements

The network-DAG theorem and the fixed-reference ordinary-DAG commit theorem meet
the formal boundary. The final composition uses this sequence:

1. V2 current no-idle source rules derive one later own block for each correct,
   available author.
2. V2 no-skip round catch-up uses actual past signer-floor crossings to recover
   the finite exact production family for one selected favorable path.
3. Fixed-reference pacing, pinned ordinary block sync, and commit-orthogonal
   retention derive one receiver-local direct range.
4. Local FlexCommitter execution derives a receiver-local exact advance.
5. Exact-prefix induction derives network commit progress and pointwise
   catch-up.

None of the later blocks, finite window, sync completion, direct range, Flex run,
or install is a future theorem input. The remaining work is product behavior
and Rust-to-Lean source refinement. The strict proof derives timer spread from
actual prior broadcasts and pinned sync. It uses one proposed action-local
exact-next timer-promptness rule. Its other fields are static, local, current,
or past.

Separate replay modules model an exact-material alternative. Keep that design
outside the adopted liveness route. The final ordinary-DAG theorem does not use
it.

## Network DAG progress proof

Let `C` be the fixed set of correct, available validators after stabilization.
The fault bounds must prove that `C` has quorum stake. The epoch must remain
active for the requested finite round window.

Before recovery entry, use only the validator's durable commit time, current
commit head, local clock, and ordinary DAG state. If the commit head stays
unchanged, its local recovery deadline expires and its recovery action stays
enabled. This sequence must derive the recovery set. The theorem must not take a
recovery quorum or commit-synchronization result as an input.

The result is unconditional network block progress. A strict correct-host
commit-index increase can end one fixed-head attempt, but it cannot discharge
the theorem. The proof must restart from the later local state and still derive
a positive correct-held total-quorum layer. The proof keeps any stronger
correct-only common recovery window as an internal witness for the later commit
stage.

Before a proposal, Core can process local commit results from its current
accepted DAG. This work is a finite internal interruption. The fixed-frontier
rank bounds one already observed class of scan chains.
`ValidatorCoreHandlerRefinementRules` supplies a
`ValidatorFiniteCoreHandlerEpisode` that also covers cleanup-driven
unsuspension from the finite Core-owned store, terminal `none`, and the
following proposal-attempt invocation before return. It does not state that the
attempt succeeds. If commits continue across handlers, each handler must still
invoke the proposal attempt, and protected proposal work must remain fairly
scheduled. The network DAG proof does not inspect the commit result to obtain
block traffic. It derives proposal success through the separate ordinary
proposal rules after each finite interruption.

`ValidatorFiniteCoreHandlerEpisode.proposal_attempt_input_and_suffix` exposes
the exact attempt input at handler exit and the finite suffix to the next trace
state. `ValidatorCoreProposalAttemptContinuationRules` classifies an exact
proposal action in that suffix or exact protected or durable work in the next
state. This classification is a past and current state result. It is not a
future proposal-success input.

A received remote block or commit batch can trigger this local work. The proof
does not assume that such a batch will arrive, be processed, or install a
commit.

Use one durable proposal obligation in both normal operation and recovery. When
accepted-batch processing exposes a legal proposal round above the signer floor,
it latches the lowest such round before the proposer runs. A later round or a
commit installation cannot cancel this obligation. Proposal persistence creates
a durable send obligation, and only completion or an atomic replacement with
another legal target clears it. In commit-progress recovery, the normal target is
still exact-next. A larger target is only the witnessed cleanup safe-resume case.

The completed stage uses the current operational quorum frontier instead of a
stable commit-head interval. The local pacemaker produces or replays an exact
successor block. Ordinary delivery and current-GC parent processing carry that
successor to all correct, available validators. Each such validator then
produces or replays its own exact successor. Finite correct-stake aggregation
gives the next total-quorum layer. The public property does not require every
correct validator to produce at every round.

Before a local or synchronized commit installation advances cleanup, it accepts
and catalogues every exact commit-body block. After the cleanup change, it keeps
the accumulated closed frontier above the new boundary. This gives legal normal
parents for later proposal work. This is a local preservation rule, not a
positive synchronized-commit step.

Use the operational quorum frontier instead of a proof-selected alignment
round. Each correct host exposes its highest attained local quorum and a finite
source for that quorum. Take the finite maximum `H` across correct hosts.

The proof uses this sequence:

1. Select one correct holder of `H` and its exact accepted, retained,
   catalogued source.
2. If `H` is positive and meets the requested bound, project that source to the
   public correct-held total-quorum layer.
3. Otherwise, use the holder's exact threshold-clock target `H + 1` and ready
   parent list.
4. The local frontier pacemaker produces the exact successor, replays an
   already-signed exact successor, or exposes a higher frontier.
5. If the attempt creates a child, use the child's own exact immediate-parent
   quorum. Persist and broadcast the child through the normal proposal path.
6. Deliver one successor carrier to every correct, available validator. Each
   validator reaches frontier `H` and produces or replays its own `H + 1`
   block.
7. Deliver those exact blocks to one correct holder. Finite correct-stake
   aggregation gives a total quorum at `H + 1` or a later frontier.
8. Recompute the finite maximum and repeat.

The child quorum in step 5 uses the exact immediate-parent projection. The
source used in step 6 must retain the child's full dependency projection. A
round-jump block can include an older own-author ancestor in addition to its
immediate quorum parents. The sync proof must not erase that extra dependency.

`operational_frontier_pacemaker_gives_strict_progress` proves the strict step.
`operational_frontier_strict_progress_gives_network_dag_progress` repeats it by
well-founded induction. A fetch response uses the GC round that is current when
the response is processed. A dependency at or below that GC round is complete;
an above-GC dependency follows ordinary fetch and acceptance. No in-flight
fetch is rebased by a commit. The proof assumes no future block, timer callback,
stable head, or common layer.

The Lean composition is complete. Current Rust has the required high-level
mechanisms. The open work is an exact source mapping for the one-shot forced
timeout, the two temporary-blocker watcher retries, and the receiver-driven
exact-or-newer subscription replay.

### Catch-up parent witnesses

Historical catch-up must preserve the complete parent set named by an accepted
and retained child block. That set has at most one block per author and quorum
stake.

If an author equivocates, include one valid branch and ignore the other branches.
During historical catch-up, use the branch named by the child block. For a new
recovery proposal, a validator can use one locally selected valid branch. Different
validators do not need to select the same Byzantine branch. Safety already treats
that author as Byzantine, and parent stake counts the author only once.

The validator-indexed Lean state represents block references and local branch
choices. The proof that these fields follow the product parent selector remains a
source-to-model obligation.

Do not remove all blocks from the author only because an equivocation becomes
known. Removal can reduce a valid parent set below quorum and stop recovery. In
recovery, include the current accepted and retained representative for every
in-range immediate-parent author for which one exists. Count one branch per
author. Do not consult the predicted leader schedule or use score-based
exclusion for this round.

### Old-block cleanup

Exact-next catch-up uses only parent bodies above the requester's local cleanup
round. A reference at or below that round is an opaque committed root. The proof
does not fetch, pin, serve, or accept its body again.

Let `P` be the durable signer floor and `G` the local cleanup round. If the parent
round for `P + 1` is not genesis and is not above `G`, exact-next work is not
legal. The validator starts one fresh normal parent accumulator at:

```text
max(P + 1, G + 2)
```

This normal safe-resume proposal uses one locally accepted, retained parent per
selected author, all strictly above `G`. Its durable target and parent pool
survive commit churn. After this block is durable, recovery returns to exact-next
proposals.

For any received block above `G`, recursive synchronization creates needs only
for direct parents above the current local cleanup round. Each fetched body can
create more such needs. When cleanup reaches one pending reference, that reference
becomes a committed root and its request ends. A cleanup increase must come from
a local commit installation, which also rebases stale proposal work.

This rule does not select or announce a common recovery round. It also does not
make different local targets equal. Each actual safe-resume broadcast instead
contributes to the operational frontier. The proof recomputes the finite maximum
and continues from its correct source.

A sticky stale target is not sufficient. A valid target must stay above local
cleanup, or a commit-driven rebase must replace it with the fresh normal target.
Safe resume also needs a non-rollback signer record. Network data alone cannot
prove that the validator did not sign before restart.

## Commit-progress proof

Fix one correct, available validator and one requested commit index. The
first-stage proof retains private commonly accepted correct-authored windows
before it erases them to the public correct-held total-quorum result. The
leader-order theorem supplies recurring fresh favorable windows. The commit
proof selects a sufficiently late finite DAG prefix and derives one actual
local FlexCommitter install at or above the requested index. It then repeats
this argument for each validator. The erased public DAG predicate alone does
not supply this stronger receiver evidence.

Local commit catch-up work can interrupt the construction for a finite time.
It cannot become a separate liveness result. Future synchronized-commit
receipt or installation is not part of this stage.

### Recovery pacing

Let `P` be the highest durable own round and `T = P + 1`. First test whether the
local DAG contains quorum parents in `T - 1`. If it does not, keep one durable
parent-acquisition goal for `T` and request the missing parents. Do not require
an accepted block at `T` to create this goal.

The local threshold-clock round is the highest proposal round enabled by a known
quorum in its immediate parent round. When parents for `T` are present:

- If `T` is below that round, make the exact-next catch-up proposal without a
  selected-leader wait.
- If `T` equals that round, use the growing recovery wait before the proposal.

Knowledge of a later quorum does not replace the direct `T - 1` parent check.

Use one wait schedule keyed by a fixed reference round `R_c` and the absolute
target round `R`. Start the target timer only when its quorum parents are ready.
A later received block or commit-head change must not change its wait value. For
fixed values `b`, `l`, and `q`, use:

```text
gap = R - R_c
W(R) = b + l * gap + q * gap^2
```

Keep `R_c` fixed for the selected proof suffix.

Use the finite reference space to bound causal work. Let:

```text
M = authorityCount * blockIdCount
B = validatorBlockSyncAcceptanceBound
```

A persisted capsule has at most `M` unique references at each round. The
receiver's GC cutoff gives a finite unresolved set for a round-`R` target. The
quadratic wait must dominate the derived causal-visibility and timer-spread
costs.

Compute the late threshold before the proof selects a favorable suffix. Then
use V2 no-skip catch-up to derive the finite exact production family. The
strict theorem derives timer spread from actual prior broadcasts, pinned sync,
and one action-local exact-next timer-promptness rule. It does not add timer
spread, a future block, or a future timer as a public premise.

### Immediate parents

At the parent-selection snapshot, include the current locally accepted and
retained immediate-parent representative for each in-range author for which one
exists. If the author has multiple blocks, select one and ignore the other blocks
for this proposal. Count the author's stake once. Do not consult the predicted
leader schedule or use score-based ancestor exclusion for this round. Do not wait
for every author after the selected parents have quorum stake.

This local rule lets the proof derive that a timely correct first-slot block is in
the next-round blocks from quorum stake. That fact must not be a theorem input.

### Leader order

There are two useful strategies:

1. Keep the current order and use an explicit independent uniform sampling model.
   Lean must prove that a favorable run occurs with probability one. It must not
   take the successful trace as an input. The current deterministic round-seeded
   shuffle does not itself prove this distribution.
2. Use a deterministic coverage rule. Keep the same selected validator set. Put
   one schedule member first in the ordered selected leader slot list for `d + 1`
   consecutive rounds, then move the first position to the next member. Here, `d`
   is the indirect commit depth. Because the schedule contains a correct,
   available member, one complete favorable run occurs in a finite number of
   rounds. This argument applies to current v3, where `P_r = S`. A smaller round
   selection needs a separate coverage proof.

The second strategy gives the simpler proof. It changes only the common selected
leader slot order. It does not change the selected validator set, its stake, or
the thresholds.

The worst-case commit proof needs `d + 1` usable anchor rounds. If direct votes
come one round later, the last anchor needs one additional block layer. These
values come from the commit rule. They are not fixed recovery constants.

### Commit agreement from pointwise local progress

One validator that records one commit is only an intermediate result. If no
correct host is ahead, the commit stage uses this sequence:

1. Choose an arbitrary correct, available validator and a sufficiently late
   common finite DAG prefix.
2. Use one fresh favorable leader window in that prefix to derive exact leader,
   vote-child, and anchor evidence at this validator.
3. Use the code-faithful pending refresh, finite scan, and local task rules to
   derive one actual successful FlexCommitter run and local `recordCommit`.

If one correct host is already ahead, the preferred route stays on the ordinary
DAG:

1. Identify the old direct leader blocks that produced the exact next commit.
2. Prove that later correct validator blocks carry these blocks in their full
   causal histories.
3. Use ordinary broadcast, subscription, recursive block sync, and acceptance
   to make that causal evidence local at each lagging validator.
4. Build a later local direct anchor. The normal indirect scan from that anchor
   commits the old direct leaders.
5. Use exact-prefix safety to identify the same exact next reference.

This route does not fill skipped own rounds. It uses unbounded later authorship.
After GST, each correct validator's fetch, verification, and acceptance service
for known above-GC causal work must outpace the maximum work created by round
advancement. Therefore, a fixed required causal history is eventually local
even while rounds continue.

The proof must derive the later carrier blocks and anchors. It cannot start from
a source install and ask for a future source-specific carrier. An actual
synchronized install can close an already occurring safety race, but the proof
does not assume that such an install will occur.

This liveness path does not use commit votes in blocks, replay manifests, or
commit synchronization as progress inputs. Vote and certificate facts are
authenticated safety evidence for actual DAG blocks. Optional commit
synchronization can remain as an acceleration path. It can stop after
commit-index catch-up while the local ordinary DAG still lags, so it cannot
replace normal block synchronization. Its provenance is a safety obligation
only.

Lean also proves an exact-replay alternative. It saves exact material from a
past successful Flex run, sends a reference manifest, fetches the named bodies
parent-first, and runs a material-scoped replay action. This is a non-adopted
proof experiment. Current Rust does not implement it, and the adopted proof
must not depend on it.

## Recommended single-node changes

The following changes give the clearest proof path:

1. Add a hysteretic recovery-mode latch. Use the normal threshold-clock round
   only as its gap probe. Enter when the gap reaches `P_enter` or the time since
   any commit install reaches `T`. Stay active while the gap reaches `P_exit` or
   the time reaches `T`, where `P_exit < P_enter`. Exit only when both signals
   clear. Keep the exact recovery proposal target separate.
2. Keep one exact proposal target and deadline until completion.
3. Start each target timer when quorum parents make that target ready.
4. Do not let a higher observed round cancel the target or restart the wait.
5. Keep missing recovery parents as a priority synchronization goal.
6. Retry peers in a fair order and wait when no peer is connected.
7. Re-advertise the highest durable own recovery block and serve its parent
   history.
8. Fetch and pin only exact references above the requester's local cleanup
   round. Treat lower references as committed roots. Rebuild requester needs
   and body pins after restart.
9. Replace the normal selected-leader wait with the fixed-reference quadratic
   recovery wait. Derive its timer spread from actual prior broadcasts and sync
   plus an action-local exact-next timer-promptness rule. Give rate-limited
   recovery an override for the propagation-delay proposal stop. Keep parent
   quorum, signatures, and durable-before-send checks.
10. Include the current accepted and retained immediate-parent representative
    for each in-range author for which one exists. Ignore the author's other
    branches. Do not use schedule prediction or score exclusion for this list.
11. As optional resource hardening, limit admitted work per author and round.
    The sound proof cap comes from the finite reference space and does not need
    this smaller operational limit. Reserve processing for required parents,
    recovery deadlines, proposals, and commit scans.
12. Use a deterministic leader-order coverage rule, or specify and prove the
    probability model for the shared order.
13. During cleanup safe resume, persist the exact local target and parent need
    before proposal selection. Do not let a higher accepted batch replace this
    work without a legal commit-driven rebase.
14. Map each persisted proposal pin to the exact finite causal capsule used by
    the reference-space projection. Keep that retained history serveable for the
    active epoch. Do not require proactive full-history flooding.
15. Model the threshold-round signal and one-shot leader-timeout task. Keep its
    exact target until callback or a later signal, and classify a callback that
    returns without a proposal.
16. Bind selected-leader inclusion to recovery-origin parent snapshots. The
    current broad Lean field applies to every proposal snapshot and is not a
    valid source map for the current normal proposer.
17. Map current accepted and catalogued authenticated bodies from correct
    authors to their durable own block. Derive the exact time-zero or past
    persistence origin.
18. Expose the literal Flex scan input after pending-round refresh. Prove that
    the returned result comes from this same input.
19. Map the V2 selected-support, recursive-need, queue-source, and no-idle rules
    that derive unbounded later own-block production.
20. Implement the V2 no-skip proposal queue. Map every exact-next persistence
    used by the final window to one past commit-progress-recovery timer origin.
    Keep the exact `proposeNext` action, refreshed parent snapshot, gate,
    deadline, and persistence. Derive broadcast from existing obligation work.
21. Map pinned ordinary sync and commit-orthogonal above-GC retention to the
    exact selected leaders in the fixed-reference window.

These are local changes except for the common leader-order rule and any new
safe-resume rule. They do not change the quorum decision rules.

## Proof order

Implement the proof in this order:

1. Model validator-indexed state and addressed messages.
2. Derive quorum correct, available stake from the fault bounds.
3. Map `ValidatorPacketDrivenBlockAcceptanceAt`,
   `packetDrivenAcceptanceHasInputOrigin`, `ValidatorCoreHandlerInputObservation`,
   `handlerInputOccurs`, and `qualifyingInputHasFiniteHandler` to the guarded
   ordinary `add_blocks` handler. Cover pending refresh, every local
   `recordCommit`, cleanup-driven unsuspension from finite input, terminal
   `none`, and invocation of the normal proposal attempt before return. Do not
   infer proposal success from this interface. Model certified-commit processing
   separately. Do not map GC-unsuspension to a new handler input.
4. Map `ValidatorCoreProposalAttemptContinuationRules` to the already-actual
   attempt result. Continue a same-batch proposal through persistence and send,
   or run the exact protected or durable current work. Do not add a future
   proposal result.
5. Prove recovery entry from local time or round gap and local eligibility
   checks. A commit can rebase work, but cannot discharge block progress.
6. Prove source retention, exact proposal persistence, and addressed ordinary
   broadcast.
7. Prove recursive direct-parent needs, exact-reference service, delivery,
   buffering, and parent-first acceptance.
8. Prove exact-next and cleanup-safe proposal recovery for one host.
9. Treat every local commit branch as a finite interruption. Resume protected
   ordinary proposal work without using the commit result as DAG evidence.
10. Aggregate the one-host results into one positive sourceable operational
    frontier, and project its exact bodies to a correct-held total-quorum layer
    after every post-GST start and requested round.
11. This completes `NetworkDagProgressLiveness` without a commit alternative.
    Retain the stronger correct-only recovery windows privately for the next
    stage.
12. Derive unbounded later own blocks from the V2 current no-idle source package.
13. Use V2 no-skip catch-up to recover the finite exact production family for
    every candidate author and required offset.
14. Prove recurring favorable first-slot windows from the chosen leader-order
    rule. Do not assume a favorable execution.
15. Fix one correct, available receiver and one fixed reference. Choose a
    favorable base above every numeric threshold.
16. Derive timer spread from actual earlier broadcasts and pinned sync. Use one
    action-local exact-next timer-promptness rule.
17. Use pinned sync and commit-orthogonal retention to derive exact adjacent
    leader-parent evidence and the receiver-local direct range.
18. Reuse the deterministic pending refresh, anchor scan, and exact result
    proofs to obtain one actual successful local FlexCommitter run.
19. Map the result to one actual local `recordCommit` and durable exact install.
20. Use exact-prefix safety and finite exact-index induction to derive network
    commit progress and pointwise catch-up. Keep each finish time inside its
    validator quantifier.
21. Keep synchronized-install provenance as a safety-only race case. Do not use
    future commit synchronization in steps 3 through 20.

The full conditional composition is green in Lean. Steps 15 through 17 are
proved by the strict fixed-reference current-pacing theorem. The action-local
promptness rule and other current-source mappings remain product obligations.
A separate exact-replay experiment is proved, but it is not an adopted
liveness design.

The final review rule is simple:

> No final theorem input can state a collective progress result. This includes a
> recovery quorum, parent availability, a common block layer, quorum inclusion,
> a favorable leader window, a usable anchor, completed synchronization, or a
> later commit. It also includes eventual remote commit receipt, processing,
> installation, or an exact-install race.

The final-theorem input list must contain only the basic inputs,
single-validator contracts, probability law, and source-to-model mappings. The
ledger can also track derived proof goals, but it must label them as derived. The
gap report must separate missing product behavior from missing formal proofs.
