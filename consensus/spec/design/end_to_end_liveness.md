<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# End-to-end liveness composition

`EndToEndLiveness.lean` defines the permitted input boundary and three final
goals: unbounded network DAG progress, unbounded network commit progress, and
pointwise commit catch-up. It does not state the final theorem. Some
composition results are still open.

The main input record contains only static configuration, fault bounds, partial
synchrony, bounded local execution, local transition rules, and the initial
genesis commit. `StaticLeaderScheduleInput` contains the indirect depth and the
schedule stake bound. It does not contain a favorable leader window.

The deterministic input also requires at least two validators. Current durable
send proofs select a receiver different from the sender. A one-validator epoch
is out of scope until the model has a local publication action.

See [Flex committer execution](flex_committer_execution.md) for the exact
pending-state refresh, conditional fixed-frontier rank, Rust terminal-loop
control flow, and proposal attempt. `ValidatorCoreHandlerRefinementRules` now
states the complete finite Core-handler boundary. Its Rust source mapping is
still open. The interface is not a future-progress assumption.

## Required proof split

The deterministic proof has two main stages:

1. Prove unbounded post-GST correct-held total-quorum frontiers. Each public
   witness has exact accepted, retained, catalogued, valid blocks at one
   correct holder. This stage uses proposal, persistence, ordinary broadcast,
   block sync, acceptance, recovery, and fair local work. It does not use a
   commit advance or a synchronized commit as its result.
2. Keep the stronger correct-authored recovery windows inside the construction.
   Combine those windows with the favorable leader-order result to derive
   actual successful FlexCommitter runs, unbounded exact commit installation,
   and pointwise prefix catch-up.

Stage 1 is now proved by `EndToEndLivenessInputs.network_dag_progress`. Stage 2
remains open.

`ValidatorCoreHandlerRefinementRules` treats commit processing in one qualifying
external handler as a finite internal interruption in stage 1. A
running-Core trace starts after `recover_validator` finishes; startup recovery
must establish the initial durable state before trace time zero. A
`ValidatorPacketDrivenBlockAcceptanceAt` witness identifies exact past ordinary
packet delivery and acceptance. `packetDrivenAcceptanceHasInputOrigin` maps it
to the complete nonempty `ValidatorCoreHandlerInputObservation` for that
`add_blocks` batch. For a fixed accepted DAG frontier, the prepared-rank proof
bounds the number of successful scans. A
`ValidatorFiniteCoreHandlerEpisode` also covers finite cleanup-driven
unsuspension, the terminal `none`, and the following ordinary proposal-attempt
invocation before return. Core runs this handler and the proposal callback
serially; no commit or block-accept action interleaves after the callback starts.
It does not say that the attempt succeeds. If commits
continue across handlers, each handler must still invoke the proposal attempt
before return, and protected proposal work must remain fairly scheduled. The
proof must not inspect the commit result to replace ordinary block production,
and it must not return a local commit advance as the network block-progress
result.

`ValidatorAuthorLocalPreAttemptScheduleAt` is the narrower install boundary. It
contains only a protected normal callback, an armed exact recovery timer, or an
occupied protected timer-arm goal. It requires that no proposal is latched yet.
At the first same-host install,
`firstInstallDisposesScheduledAttempt` returns exact past persistence or one
protected normal callback in the post-install state. A non-obsolete target is
unchanged. A target which crossed GC can move only to a legal higher safe-resume
target. The rule does not return a fresh recovery timer or another commit race.

A received remote block or commit batch can trigger this local processing. Its
receipt, processing, and installation are not future liveness premises.

The positive path does not use `applySyncedCommit` or future commit-sync
service. An actual synchronized install can close a safety or prefix race, but
it cannot supply either stage's liveness premise.

## Public theorem inputs

The current record contains only:

- the validator set, stake thresholds, fault bounds, and viable leader schedule;
- partial synchrony and bounded execution of enabled local work;
- exact local block-action effects, the GC-truncated parent DAG from each
  actual local commit-install action, and the bootstrap DAG for each current
  installed head;
- the exact durable local commit prefix, full commit heads, and current head;
- GC-aware parent acceptance and ordinary block-sync rules;
- requester-local recursive parent needs and retained recovered bodies;
- source-local recovery body pins and accepted block representatives;
- the durable exact-next parent need and timer-arm state;
- protected local proposal persistence and addressed send obligations;
- local pending-round, anchor, FlexCommitter state, and runtime mappings;
- the last-commit recovery-entry rule, increasing wait rule, and durable timer
  arm worker; and
- the exact genesis head at every correct, available validator.

No field says that a future quorum, block layer, anchor, commit output,
certificate, or common commit exists. No field can say that a commit advance
replaces the required network block layer.

The liveness path does not use commit synchronization, commit votes in blocks,
or replay manifests. Optional commit synchronization stays in the model only so
that the safety proof can restrict a synchronized install to verified data.

The proof accepts one ordinary block-sync behavior. After a correct validator
gets a block body, its synchronizer keeps fetching every missing causal ancestor
above its local GC round. Each fetched body exposes its direct parents and
continues the walk. References at or below local GC are opaque committed roots.
The requester does not fetch, pin, or accept their bodies again. A source whose
GC is higher can still serve old stored data to a peer whose GC is lower. A
later Rust review must check this accepted behavior against the implementation.

Timer-arm completion is serialized with the local signer floor. The same local
step stores the exact timer, or a commit installation advances the commit head.
A higher-round signing action cannot silently replace the pending recovery
timer goal.

## Leader-order boundary

The canonical model samples one independent uniform ranking of the complete
validator set in each round. The round leader selection is that ranking with all
validators outside the current leader schedule removed. This gives one model for
all schedule sizes. A schedule change can use past samples, but it cannot use the
current or future ranking.

`IndependentUniformRoundRankingLaw` contains only a conditional chance relation
and standard probability-one rules. For every past-dependent viable leader
schedule, the chance that its first selected leader slot is correct and
available is at least one over the validator count.
`adaptive_viable_schedule_has_favorable_windows_probability_one` uses this fact
and countable intersection to get favorable windows after every start. It does
not take a favorable trace.

`UniformRankingEndToEndExecutionFamily` maps each complete-ranking sample to one
deterministic execution. It requires the validator set, correct-available set,
and leader schedule for each commit head to stay fixed across samples. It also
requires the configured round leader selection to equal the sampled ranking
restricted to that leader schedule.

`UniformRankingExecutionSourceMap` fixes the stake, threshold numbers, fault
sets, network bounds, program, local-action bound, commit functions, wait rule,
and genesis across samples. It also gives an ordered sample boundary for each
round. State at boundary `r` and earlier events must depend only on rankings
before `r`. This stops the execution family from using a future ranking to
choose its current commit head or local behavior. A local FlexCommitter scan can
use round `r` only after the sample boundary for round `r`. This map does not
assume that protocol rounds continue forever. Block production must prove that
result.

`EndToEndProbabilityCapstone.lean` now derives the actual reference-validator
commit head from this prefix-causal state. Its adaptive schedule is therefore
the schedule used by that execution. A real probability model must interpret
`probabilityOne`, its conditional lower bound, and countable intersection.
These are mathematical sampling rules, not protocol progress assumptions.

`PastDependentCommitHeadChoice` gives the exact non-anticipating shape. Its
leader schedule is always viable because the static schedule stake bound gives
a correct, available member. In the internal branch where no next commit has
already finished, `favorable_future_gives_stable_commit_head_path` restricts the
all-start event to `CommitHeadFirstSlotLeaderPathCoverageAfter` for only the
current head. It does not quantify unused commit IDs.

`causal_commit_head_favorable_windows_probability_one` proves that this actual
causal-head event has probability one. The remaining probability gap is the
deterministic protocol theorem. It must first prove network DAG progress. It
then uses DAG growth and the favorable receiver path to prove the exact commit
step. The current transfer lemma names that gap; it is not the final theorem.

The repeated-first rule is a separate deterministic option.
`DeterministicLeaderCoverageInput.first_slot_path_coverage` proves its complete
path coverage. Current Rust does not use that order.

## Smallest composition edges

Each open result below is an internal theorem. It must not become a field of
`EndToEndLivenessInputs`.

| From | To | Smallest result | Status |
| --- | --- | --- | --- |
| Local last-commit time and clock progress during one fixed-head interval | An active recovery snapshot, or the end of that interval | `commit_advance_or_active_recovery_snapshot` chooses the proof time internally. | Proved as a supporting recovery result; the public network-DAG theorem no longer depends on this fixed-head route |
| Enabled local proposal, persist, send, and delivery actions | One actual block flow | `ready_state_builds_strict_recovery_broadcast_or_commit_advance` builds a stored and sent recovery block, or finds a local commit first. `strict_recovery_broadcast_eventually_accepted_via_parent_sync` accepts the block after its parents. | Proved as a supporting recovery route; the operational-frontier proof supplies the public network-DAG result |
| One delivered ordinary block body | Accepted causal closure | Each local body creates needs only for its missing direct parents above the requester GC round. Fetching a parent body repeats this rule. GC-aware buffering accepts the finite closure from the requester's committed-root frontier to the first block. The final block is accepted only after this closure. | The recursive need, body pin, and GC-aware acceptance rules are public one-host inputs. Lean still needs to derive its finite capsule from the accepted closure; the capsule is not an independent progress premise |
| Current per-host operational frontier | A later exact successor carrier or a higher frontier | `ValidatorNormalFrontierPacemakerRules` supplies the forced attempt and watcher disposition. `ValidatorCurrentTipSubscriptionExecution` supplies exact-or-newer subscription replay when the successor was already signed. | Proved in Lean; exact mappings to the current Rust retry and subscription paths remain open |
| One maximal correct-held operational frontier `H` | A correct-held total-quorum layer at `H + 1` or later | The owner sends one exact successor carrier. Every correct, available validator reaches frontier `H`, produces or replays its own exact `H + 1` block, and sends it to one correct holder. Parent-fetch results use the current GC round. Finite correct-stake aggregation gives the next layer. | Proved by `operational_frontier_pacemaker_gives_strict_progress`; no new timeout or proactive replay mechanism is required |
| Protected causal data and an exact-next timer goal | Exact-next recovery proposals | Use recursive block sync, a durable parent need, a timer keyed by the local head and target, and a concrete proposal latch after one common alignment layer exists. | Requester needs, source pins, and timer work are mapped. Local `G + 2` bootstrap alone does not give round convergence |
| Per-host recovery state | A finite family of timer-paced strict broadcasts | Derive `indirectDepth + 2` adjacent round snapshots. The first `indirectDepth + 1` rounds hold the selected leader window. The last round supplies child blocks that vote for the last leader. A concurrent commit restarts the construction; it is not DAG progress by itself. | Fresh timer-paced finite windows are proved for a stable suffix. Continuation after a commit remains open |
| Adjacent fresh timer-paced broadcasts and increasing waits | Timely leader acceptance | Derive a fixed per-round increase bound for timer-start spread from proposal send, addressed delivery, parent-ready acceptance, and timer-arm bounds. The growing wait then places each correct selected leader before the next snapshot. | Local bounds are proved. The spread recurrence and recovery-origin parent-inclusion adapter remain open |
| Timely accepted leaders and accepted child blocks | Actual direct-vote quorum range | `adjacent_recovery_snapshots_give_exact_direct_vote` gives each pointwise vote. `pointwise_direct_votes_eventually_give_trace_quorum` gives the actual quorum stake at one observer. | Pointwise and finite aggregation results are proved; the finite snapshot family remains open |
| Uniform complete-ranking law | Favorable selected leader path | `causal_commit_head_favorable_windows_probability_one` covers the causally reached head, and `causal_favorable_windows_give_stable_reference_head_path` gives the exact stable-head suffix. | Proved; deterministic protocol composition remains open |
| Actual direct-quorum range | Actual successful run or commit advance | `trace_direct_quorum_range_runs_exact_committer_or_installed_next` builds the anchor window and exact pure Flex result, protects local `runCommitter` work, and checks the scan again at the action state. If the prior index changed first, it returns an installed next-index witness. | Proved from the local Flex and commit-prefix maps |
| Two exact successful Flex inputs | One exact commit output | `successful_cross_view_try_reference_flex_outputs_agree` covers direct and indirect results, different local scan horizons, and no shared candidate premise. | Exact conditional agreement proved; derive its direct-prefix and causal-material premises from the main trace |
| One successful local Flex run | Durable local commit | `successful_local_flex_run_completes_and_persists_exact` maps the returned result to a protected record action and stores its exact reference. | Proved from the audited local Flex snapshot and runtime maps |
| Unbounded common DAG layers and one receiver's favorable path | A later actual local Flex run and durable local commit | Build the fresh adjacent leader, vote, and anchor window from commonly accepted exact blocks. The receiver can use a different run from the validator that installed the earlier reference. | Exact finite-window and local-run results exist; the unbounded-DAG-to-window composition remains open |
| One earlier exact install and one later local commit | The earlier exact entry in the lagging validator's prefix | `installed_next_precedes_any_later_local_commit` uses durable index monotonicity and exact-prefix safety. The two local commits can use different views and different outputs. | Proved |
| Network DAG progress and one receiver's favorable path | Next exact common installed reference | The receiver-local proof must build the actual adjacent vote evidence and derive `DerivedPointwiseCommonCommitStep`. It cannot take a future window, run, layer, or commit as an input. | Open; the finite exact-index induction is ready |
| Derived one-step result | Network commit progress and pointwise catch-up | `derived_common_commit_step_proves_network_commit_progress` and `derived_common_commit_step_proves_pointwise_commit_catch_up` perform finite exact-index induction from genesis. | Proved |
| Derived one-step result | Unbounded common commit progress | `derived_common_commit_step_proves_commit_progress` advances past the maximum starting index. | Proved |

## Exact remaining source maps

The final input record can add the following records only after each record maps
simple local behavior to the same `ValidatorExecution`:

- Cleanup alignment: map one actual accepted and retained witness block to a
  durable `.alignProposal` lock at a lower signer. Keep the exact witness and
  its one-branch-per-author quorum immediate parents above the effective cleanup
  boundary until the proposal persists. This map states only past and current
  local state. The proof must derive witness delivery and the common layer.
- Proposal engine: create each exact ready-proposal obligation from the current
  parent need and recovery timer. The public input maps an actual proposal
  action to the latch and runs it through persistence and addressed send work.
  A commit or cleanup update must preserve or legally rebase the current local
  phase. It must not require an accepted future child to enter a pending queue.
- Block synchronization and acceptance: the public input now maps each local
  body to direct-parent needs above local GC. It also maps body pins, local
  retry, packets, and GC-aware parent acceptance. The proof must still apply
  these one-host rules recursively to the finite causal history. Protocol
  acceptance is parent-first, so an accepted block has its required local
  causal history already accepted. Lean must derive the finite causal capsule
  from this accepted closure, not take capsule availability as a separate
  liveness result. Retention and source service remain separate facts. Live Rust
  reads exact blocks through `DagState::get_blocks`, which falls back from the
  recent cache to persisted storage. The remaining Lean refinement must map
  proposal flush and persisted exact-ancestor reads to the existing causal
  capsule and source-protection facts. For a block produced after a round jump,
  this map must keep its full dependency list, including an older explicit
  own-author ancestor. `ValidatorBlock.parents` is only the immediate-parent
  quorum projection. `SafeResumeBlock` shows the required distinction.
- Proposal readiness: connect one exact parent need and its accepted parents to
  the timer arm. The public timer worker then derives the exact timer or a
  commit-head advance. Round one uses the static canonical genesis parents.
- Core handler: `ValidatorPacketDrivenBlockAcceptanceAt` and
  `packetDrivenAcceptanceHasInputOrigin` map one delivered and accepted ordinary
  block to its complete nonempty `ValidatorCoreHandlerInputObservation`.
  `handlerInputOccurs` and `qualifyingInputHasFiniteHandler` map that
  `add_blocks` batch to the finite input-owned block store, each local commit
  scan and `post_commit`, cleanup-driven unsuspension, terminal `none`, and the
  following `try_propose(false)` invocation before return. This map states
  finite past and current control flow. It does not state that the attempt
  succeeds or that future DAG progress occurs. Certified-commit processing is
  outside this positive DAG source map. The Rust source mapping must distinguish
  direct input acceptance from GC-unsuspension. A GC-unsuspended block uses the
  enclosing handler continuation and does not create a new input. This source
  mapping remains open.
- Proposal-attempt continuation: `ValidatorCoreProposalAttemptContinuationRules`
  maps the already-actual normal attempt to an exact proposal action in the same
  batch, exact protected normal work, or current durable proposal, parent-need,
  or timer work. `qualifying_core_handler_input_has_current_proposal_continuation`
  applies only to an input accepted by `handlerInputOccurs`. It does not return
  a future proposal or future DAG progress. Its Rust source mapping remains
  open.
- Anchor and Flex input: selected leader slot order, exact block references,
  accepted causal closure, exact vote sets, and commit-body material. The public
  input already contains the pure same-state Flex snapshot mapping.
- Flex scan evidence: map direct commit voters and only the indirect
  certificates used by one reconstructed successful result to exact accepted
  child bodies in that run's finite DAG support. This is a downstream commit
  and safety refinement. The network DAG theorem does not use it as a source
  of block progress.
- Optional commit synchronization: constrain an actual synchronized install to
  a verified exact chain. This mapping is safety-only. The liveness proof does
  not use synchronization service, certificate production, or commit votes.

These are one-validator rules or source-to-model checks. They do not state a
quorum, block layer, anchor, certificate, or later commit.

## Exact internal theorem signatures

The following signatures are design targets. Do not add them as axioms or input
fields.

```lean
private theorem main_trace_network_dag_progress_liveness
    (inputs : EndToEndLivenessInputs config faults protocolPacket network) :
    NetworkDagProgressLiveness config faults network
      inputs.timedExecution.execution.trace

private theorem network_dag_progress_and_favorable_path_give_common_step
    (inputs : EndToEndLivenessInputs config faults protocolPacket network)
    (dagProgress : NetworkDagProgressLiveness config faults network
      inputs.timedExecution.execution.trace)
    (leaderPath : FirstSlotLeaderPathCoverage config faults
      inputs.leaderSchedule.indirectDepth) :
    DerivedPointwiseCommonCommitStep faults network
      inputs.timedExecution.execution.trace
```

The DAG theorem treats commit processing only as a finite local interruption.
It uses the handler's proposal-attempt invocation, then derives proposal success
from the separate proposal guards and fair-work rules. The downstream commit
theorem can use an already-advanced verified local head as an internal race
case. Neither theorem can take a produced layer, successful Flex run, carrier
block, later install, or catch-up result as an input.

The repeated-first deterministic theorem must have this public shape after all
local source maps are in the input record:

```lean
theorem end_to_end_liveness_on_leader_path
    (inputs : EndToEndLivenessInputs config faults protocolPacket network)
    (leaderPath : FirstSlotLeaderPathCoverage config faults
      inputs.leaderSchedule.indirectDepth) :
    EndToEndLivenessGoal inputs
```

The theorem body must not take another liveness implication. It must derive the
three goal parts through these four steps:

1. Local proposal, delivery, and acceptance rules give correct-held
   total-quorum DAG frontiers at unbounded rounds. A local commit does not end
   this proof. The construction keeps its stronger correct-only recovery
   windows private.
2. Those private windows, the favorable leader path, anchor rules, and
   FlexCommitter rules derive one exact common next-index step.
3. For each correct validator, later common DAG layers and a favorable window
   give an actual local FlexCommitter run and a later local commit. The run does
   not have to reproduce the source validator's view or output. Durable prefix
   safety gives the earlier exact reference at that validator. An actual
   install race can close this step, but future commit-sync service is not used.
4. Finite exact-index induction derives network commit progress and pointwise
   commit catch-up.

The architecture does not first assume or prove the two semantic commit goals
and then use them to recover the one-step result. The one-step result comes
from DAG growth and the per-validator favorable-window proof. The semantic
commit goals are its
corollaries.

The repeated-first theorem can use the stronger all-head path. One internal
commit step uses only `CommitHeadFirstSlotLeaderPathCoverageAfter` for its actual
prior head and its future suffix.

The canonical probability theorem must have this public shape after the
deterministic causal-head composition theorem is proved:

```lean
theorem end_to_end_liveness_probability_one
    (law : IndependentUniformRoundRankingLaw authorityCount)
    (family : UniformRankingEndToEndExecutionFamily BlockId CommitId PacketId
      Encoding authorityCount)
    (source : UniformRankingExecutionSourceMap family) :
    law.probabilityOne
      (fun sampled => (family.execution sampled).goal)
```

The theorem derives a past-dependent head choice from the state before each
future round. It then uses the ranking law. It does not take a favorable trace,
path, block layer, anchor, commit result, or propagation result.

`leaderPath` is the deterministic repeated-first alternative. It gives the
head-specific path only when one commit step needs it. It is a static property
of round leader selection, not a successful execution. The canonical
global-ranking proof must derive the same head-specific path for the head that
the execution has reached. It must use only future rankings after that head.
Its public probability theorem must not take either favorable event as an
input.

## Finite exact-index induction

Do not add the common-step proposition to `EndToEndLivenessInputs`. The
per-validator proof must derive it from DAG growth and the favorable leader
path.
`derived_common_commit_step_proves_commit_progress` then performs finite
exact-index induction from genesis.

## Type boundaries that still block direct composition

- The current recovery theorem gives a private common correct-only layer or a
  correct-host commit. The DAG proof must continue after each commit, retain a
  positive sourceable operational frontier, and project its exact bodies to a
  correct-held total-quorum layer at unbounded rounds. This is the missing
  bridge to `NetworkDagProgressLiveness`.
- DAG growth and the stable receiver head do not yet give the complete fresh
  adjacent-vote range for one favorable selected-leader window. This is the
  missing bridge to `DerivedPointwiseCommonCommitStep`.
- The per-validator path must cover both cases: the validator advances, or it keeps
  its head and accepts enough fresh DAG evidence to run its local committer.
  A local advance is only a finite internal interruption. It cannot remain
  a public alternative to the required DAG layer. Exact-prefix safety then
  identifies the same next reference after the later local commit.

Keep common-round, quorum, anchor, certificate, server, and common-step helpers
private or in an `Internal` namespace. They are proof results, not assumptions.
