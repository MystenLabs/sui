<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 implementation gaps

This report lists work that must finish before the related formal results apply to
the product. P0 items block v3 activation. P1 items block complete safety or
progress claims. P2 items reduce conformance and boundary risks.

The [assumption ledger](ASSUMPTIONS.md) gives the status of each condition. The
[assumption evidence ledger](ASSUMPTION_EVIDENCE.md) records the reviewed Rust
and Lean evidence, its limits, and the files that require a focused recheck.

## Commit progress recovery

### P0: implement commit progress recovery

Related assumptions: `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`,
`ASM-LIVE-LOCAL-PROPOSAL`, `ASM-LIVE-LOCAL-RESPONSE`, `ASM-LIVE-LEADER`, and
`ASM-LIVE-FIRST-SLOT-SAMPLING`.

The product can move to a future round without filling its own proposal sequence.
Partial synchrony alone does not ensure that commits resume.

Implement the [recovery design](../design/commit_progress_recovery.md). Use the
normal `threshold_clock_round()` candidate as the round-gap probe. Do not use the
exact-next recovery target as this probe. Enter recovery when the gap reaches
`P_enter` or the time since any commit install reaches `T`. Stay active while the
gap reaches `P_exit` or the time reaches `T`, where `P_exit < P_enter`. Exit only
when both signals clear. A small commit must not clear recovery while the exit
gap remains. No production threshold values are selected in this specification.

The product must propose only one round after its highest known signed round,
preserve growing pacing across round jumps, and require a valid immediate-parent
quorum. It must request missing parents. In recovery, parent selection must
include the current locally accepted and retained representative for each
in-range author for which one exists. It must count an equivocating author once,
ignore its other branches, and bypass score-based exclusion for this immediate
parent round. This selection must not depend on the local predicted leader
schedule. The current `force=true` path does not implement this rule.

When block-progress recovery is active and the signer floor is above GC, cancel
or replace any stale normal ready proposal before it persists. Only a ready
proposal created from the current commit-progress recovery timer and its
refreshed parent list can persist. The executable proposal-obligation transition
does not yet enforce this same-host origin rule.

Bind each recovery `proposeNext` action to the protected job for one stored
timer generation. Keep its head, target, and deadline until the action runs.
Run the action once between the deadline and one local-action bound after the
deadline. If the head changed at the deadline, reject or re-arm that generation.
A stale retry or duplicate action must not bypass this check. The current
proposer does not keep this timer-generation-to-action provenance.

Cleanup safe resume is a separate round-jump exception. Local processing must
durably record the canonical target `max(P + 1, G + 2)` and its parent need
before the proposer runs. It must not wait for an accepted future block at that
target. A higher block cannot replace this work without a legal commit-driven
rebase. The target, its exact parent bodies, and its above-cleanup causal data
stay protected until the proposal is durable and sent. After that step,
recovery returns to exact-next proposals. A local or synchronized commit must
preserve an equivalent usable seed before it advances cleanup.

The canonical local target is not a network round-convergence rule. Local GC
rounds can differ. A lower host also need not have a quorum in every round
between its local target and the highest target. Therefore, local safe resume
followed by exact-next proposals does not by itself give one same-round correct
quorum.

Add a witness-bound alignment transition. It must consume one actual accepted
and retained block, lock its exact reference and quorum immediate parents, and
permit a lower signer to make one own block in that witness round. It must use
one selected branch per author. The lock and parent bodies must survive a local
or synchronized commit installation until the proposal is durable, or the host
must select a newer actual witness before it signs. This is the isolated
`ValidatorSafeResume` and `.alignProposal` shape. It is not wired to the main
trace. Current Rust can incidentally set its threshold clock to the round of a
first accepted higher block. However, more accepted stake can move it past that
round before proposal, and there is no durable witness lock across commit and
cleanup work. The source rule must not return a future witness or a future
common layer.

Current Rust also does not implement the modeled recovery timing. Its
leader-timeout task resets fixed minimum and maximum timers when the local
threshold clock advances. The maximum timeout uses `force = true`, which skips
the selected-leader presence check. Parent selection still requires a quorum,
but score exclusion can omit the selected leader after the quorum exists. The
recovery implementation needs a sticky exact-target timer with the growing
absolute-round wait and the full retained immediate-parent selection described
above.

Each qualifying external `add_blocks` handler must finish all commit work
enabled by its finite nonempty accepted-block batch and finite GC-unsuspended
blocks. Its local `try_commit_v3` loop must observe terminal `None`, then Core
must invoke `try_propose(false)` before return. This rule does not say that the
proposal attempt succeeds. Local commit processing must preserve or legally
rebase protected proposal work. A persisted block also creates a durable send
obligation. The proof does not assume that a later synchronized batch arrives.

The public network-DAG theorem needs only one correct-held total-quorum layer at
each requested height. The final commit theorem separately derives unbounded
later own blocks for every correct, available validator. Its proposed V2
no-skip rule reconstructs the finite exact window that the selected favorable
path needs.

The normal commit path and immediate-parent quorum check already exist. Recovery
must preserve them. It does not need a separate certified commit prefix.

The older [round-jump proposal](https://www.cs.yale.edu/flint/certikos/publications/sp26.pdf)
uses a stronger intermediate-proposal rule. That paper studies an older protocol.
The witness-bound step above uses only its same-round alignment shape. This
design does not import or claim the paper's full trace.

## Other activation blockers

### P0: activate v3 from epoch state

Related assumption: `ASM-CONFIG-V3-ACTIVATION`.

Normal startup does not enable v3 from authenticated epoch state. Add a versioned
epoch value, rollback rules, and mixed-version checks. Do not use a node-local
activation value.

### P0: use common threshold inputs

Related assumptions: `ASM-SAFE-PARAMETERS`, `ASM-MATH-THRESHOLDS`, and
`ASM-REFINE-INTEGERS`.

V3 fault inputs can come from local process settings. Correct validators can then
derive different thresholds. Put all proof-relevant inputs in authenticated epoch
state. Check arithmetic and compatibility before the epoch starts.

### P0: implement and bind transaction voting

Related assumptions: `ASM-CONFIG-VOTING` and
`ASM-SAFE-EVIDENCE-REFINEMENT`.

The product does not implement the complete modeled v3 proposal, transaction-vote,
and finalization path. Implement this path before the transaction theorems apply.
V3 activation must also activate its transaction voting rule.

## Other safety and progress work

### P1: refine finite commit-processing interruption

Related assumptions: `ASM-LIVE-CORE-HANDLER`, `ASM-LIVE-LOCAL-PROPOSAL`,
`ASM-LIVE-LOCAL-RESPONSE`, and `ASM-LIVE-TASK-FAIRNESS`.

Rust processes one external input in one synchronous Core handler. When
`Core::add_blocks` accepts at least one block, `try_commit_v3` repeats local
commit scans and `post_commit` until one scan returns `None`. Core then calls
`try_propose(false)`. The Lean fixed-frontier rank bounds only an already
observed scan chain whose accepted frontier does not change. It does not cover
blocks that `post_commit` unsuspends from the finite Core-owned store.

`ValidatorCoreHandlerRefinementRules` states the approved complete finite
handler boundary. Complete its source refinement against the guarded ordinary
`add_blocks` path. Map exact past delivery and acceptance to
`ValidatorPacketDrivenBlockAcceptanceAt`. Use
`packetDrivenAcceptanceHasInputOrigin` to identify the nonempty
`ValidatorCoreHandlerInputObservation`. Bind `handlerInputOccurs` and
`qualifyingInputHasFiniteHandler` to its exact handler episode. Do not treat
every nested accept or record as a new handler input. Bound the finite store at
handler entry. Map every scan and `post_commit`, cleanup-driven unsuspension,
terminal `None`, and the `try_propose(false)` invocation before return. Add
focused control-flow tests. The interface contains no proposal success,
produced block, future commit, future synchronized batch, or future DAG
progress. It makes remote or local commit processing a finite internal
interruption instead of a liveness result. Certified-commit processing is
outside this positive DAG record and needs a separate per-head model.

`packetDrivenAcceptanceHasInputOrigin` is a proposed local refinement. The
source mapping must distinguish a block accepted directly by the external
`add_blocks` call from a previously delivered block that a later GC update
unsuspends. The direct acceptance starts the qualifying input episode. The
GC-unsuspended acceptance stays inside its enclosing handler continuation and
must not start another episode.

Map the already-actual attempt through
`ValidatorCoreProposalAttemptContinuationRules`. The source rule can identify
an exact normal proposal action already in the handler suffix, exact protected
normal work at the next state, or current durable proposal, parent-need, or
timer work. `qualifying_core_handler_input_has_current_proposal_continuation`
excludes ghost episodes by requiring `handlerInputOccurs`. This local rule does
not assert a future proposal result. It remains useful implementation evidence,
but the completed network-round proof uses the smaller operational-frontier
pacemaker boundary described below.

### P1: pure post-GST quorum DAG growth

Related assumptions: `ASM-LIVE-LOCAL-PROPOSAL`, `ASM-LIVE-BLOCK-SYNC`,
`ASM-LIVE-PEER-FAIRNESS`, and `ASM-LIVE-TASK-FAIRNESS`.

Lean now composes operational-frontier proposal work, subscription tip replay,
addressed broadcast, block sync, parent-first acceptance, and finite
correct-stake aggregation into positive total-quorum layers held by one
correct, available validator at arbitrarily high rounds. The proof is
`EndToEndLivenessInputs.network_dag_progress`. Its strict successor step is
`operational_frontier_pacemaker_gives_strict_progress`, and finite iteration is
`operational_frontier_strict_progress_gives_network_dag_progress`.

A local or synchronized commit is not a positive branch. It can change the
current GC round and the current leader schedule. The next proof step reads the
new current operational frontier. A block-fetch operation does not need a
commit rebase. It finishes with a body or an error. When the receiver processes
a body, it uses the current GC round: a dependency above GC is accepted or
fetched, and a dependency at or below GC is a completed committed root.

The final network-round theorem does not receive a future carrier, future
commit, future synchronization result, or future quorum layer. It receives
only local state, source, scheduler, and transition rules. The forced
max-timeout progress rule does not use the normal leader wait. Therefore, a
leader-schedule update can change an early normal attempt without invalidating
the eventual frontier step.

The distributed Lean join is complete. The current Rust mechanisms are enough
at a high level, but their exact source mappings are open. The operational-
frontier proof gives exact ready parents and threshold-clock target `H + 1`.
`ValidatorNormalFrontierPacemakerRules` models the one-shot forced timeout and
the current watcher retries. The forced path can stop permanently for the old
round only when the block already exists or the local clock moved higher. A
missing recovered own-round value and excessive propagation delay are temporary
blockers. Their watchers make another forced attempt when they clear.

If the owner already signed `H + 1`,
`ValidatorCurrentTipSubscriptionExecution` models the current receiver-driven
subscription loop. A broken, ended, or idle stream terminates, and the receiver
retries. A successful subscription sends the requested cached own block or the
sender's latest own block. The proof accepts the exact tip or treats a newer tip
as higher frontier progress. No proactive all-peer replay mechanism is required.

After the construction retains a common correct-authored layer, derive a fixed
per-round increase bound for correct timer-start spread. Use ordinary proposal
send, post-GST delivery, parent-ready acceptance, and bounded timer-arm
completion. The growing-wait lemma can then prove that a correct selected
leader is accepted before every next-round recovery parent snapshot. Do not add
a timer-spread premise, a future block, or a capsule-growth bound to the public
input.

`ValidatorAnchorLocalRules.includesAcceptedCorrectImmediateParent` is currently
wider than this product rule. It applies to every `ValidatorProposalSnapshot`.
Narrow it to a completed recovery proposal, or derive it directly from that
proposal's refreshed retained-representative list. Current normal and forced
Rust parent selection does not satisfy the universal field.

### P1: implement exact round catch-up for fixed-reference windows

Related assumptions: `ASM-LIVE-ROUND-CATCHUP`,
`ASM-LIVE-LOCAL-PROPOSAL`, and `ASM-LIVE-TASK-FAIRNESS`.

The public network-DAG theorem does not need one own block from each correct
validator in every round. The fixed-reference favorable-window proof is
stronger. It reconstructs a finite consecutive family from each validator's
later unbounded own-block production.

Current Rust cannot support this reconstruction. `try_new_block` reads the
current threshold-clock round and checks only that it is higher than the last
own proposal. It can skip intermediate own rounds. A timeout callback can also
observe a newer threshold than its input round. It does not keep one fixed
timer key and proposal snapshot for each skipped target.

`ValidatorV2RoundCatchupSourceMap` isolates the proposed repair. An active
correct proposal persistence must use exactly `highestSignedRound + 1`. Each
exact-next persistence that the final V2 proof uses must identify its past
commit-progress-recovery timer origin. This origin includes the exact
`proposeNext` action, refreshed parent snapshot, fixed gate, deadline, and
persistence for the same block. Existing obligation workers derive the later
broadcast. These source rules do not return a future block or window. A generic
normal-proposal origin is not part of the final dependency closure.

Implement an ordered intermediate-proposal queue, or equivalent safe work.
Keep each exact target and timer key until it persists. Preserve the work
across commit processing and restart. Add tests that make the threshold clock
jump across several rounds and verify each intermediate persistence in order.
Also test timer replacement and commit interference. This work is not required
for the public total-quorum DAG theorem. It is required before the derived
fixed-reference timer-paced window applies to the product.

### P1: map the completed fixed-reference commit capstone to Rust

Related assumptions: `ASM-LIVE-LEADER`, `ASM-LIVE-FIRST-SLOT-SAMPLING`,
`ASM-LIVE-LOCAL-RESPONSE`, `ASM-LIVE-BLOCK-SYNC`,
`ASM-LIVE-POST-GST-CAUSAL-SERVICE`, and `ASM-SAFE-COMMIT-CHAIN`.

The Lean theorem
`current_sources_give_end_to_end_liveness_probability_one` completes the
ordinary-DAG commit route. It uses one fixed-reference quadratic wait, V2
no-skip round catch-up, V2 current no-idle block production, pinned sync,
commit-orthogonal retention, local FlexCommitter execution, and exact-prefix
induction. A commit-head advance is not a positive liveness result and does not
start a separate already-ahead proof branch.

Complete the Rust-to-Lean maps for the fixed-reference wait, actual timer
origins, V2 selected support, recursive needs, queue source, no-idle behavior,
pinned sync, retention across commit and GC, action-scoped first-leader parent
selection, literal post-refresh Flex input, and exact-prefix install
provenance. Implement or justify the action-local exact-next timer-promptness
rule, authenticated correct-body ownership, and the checked quadratic
coefficient. Lean derives the remaining timer spread from actual prior
broadcasts and pinned sync.

The proof derives each later block, finite intermediate family, delivery,
accepted direct range, local Flex run, and exact install. None is a future
input. A separate exact-replay proof experiment uses saved successful-Flex
material and a replay manifest. Current Rust does not implement it. It is not
an adopted liveness route or a required product change.

### P1: include the exact first Flex leader in proposal parents

Related assumptions: `ASM-LIVE-LEADER`, `ASM-LIVE-LOCAL-PROPOSAL`, and
`ASM-LIVE-FIRST-SLOT-SAMPLING`.

For an actual non-forced round `R + 1` proposal, bind parent selection to the
proposal action's exact pre-state and effective schedule. If the exact first
round-`R` Flex leader for that schedule has an accepted and retained
representative before parent selection, include that exact reference as an
immediate parent. Do not use a general schedule-set intersection. The overlap
must contain the receiver's exact first Flex leader, and quorum stake must
create actual children that reference it.

The current proposal waiter waits for all allowed leaders, but score-based
ancestor selection can still omit one after another parent quorum is ready.
Prevent this exclusion for the exact accepted and retained allowed leader. The
generic `force = true` path bypasses the waiter and does not satisfy this rule.
Commit-progress recovery can instead use its stronger retained-representative
parent rule.

Compare proposal and Flex schedules through an effective schedule key. After a
commit install, read the refreshed key. If `allowed_leaders` is unchanged, keep
compatible membership facts and remove only the obsolete committed prefix. If
the list changed, reset schedule-dependent facts and start a new comparison.
Do not remove all old facts only because the commit index advanced.

Evidence:
[EV-SCHEDULE-HEAD-LOCAL](ASSUMPTION_EVIDENCE.md#ev-schedule-head-local) and
[EV-FIRST-FLEX-LEADER-PARENT](ASSUMPTION_EVIDENCE.md#ev-first-flex-leader-parent).

### P2: measure the block-transfer budget

Related assumptions: `ASM-LIVE-TRANSFER-BUDGET` and
`ASM-LIVE-POST-GST-CAUSAL-SERVICE`.

The causal-work service margin now follows from a coarse transfer budget: the
whole blocks that can reach one correct validator in one `delta`, which must stay
strictly above what ordinary round advancement demands. Neither quantity is
measured today.

`max_blocks_per_fetch` and `max_blocks_per_sync` cap the blocks in one fetch and
one sync response, and `max_transactions_in_block_bytes` and
`max_num_transactions_in_block` cap one block. No component measures a link
budget, and no configured value is derived from one.

Add metrics for blocks received per peer per unit time and for the above-GC
references that new rounds add. Then choose the block and fetch limits so that
the budget stays above the production bound for the planned committee size and
round rate. A budget at or below that bound gives no catch-up, whatever the rest
of the pipeline does.

This model covers one validator's ingest capacity. Cross-flow contention, a
shared bottleneck, and the queueing discipline between peers are outside it, as
is any size-dependent transfer time.

### P1: protect signer state after complete data loss

Related assumption: `ASM-SAFE-NON-EQUIVOCATION`.

Normal durable restart restores the highest signed round. Complete local
consensus-state loss can remove this protection. Keep durable signer state outside
the lost store, disable the epoch signing key, or count the validator as faulty.

### P1: enforce leader viability

Related assumptions: `ASM-LIVE-LEADER` and
`ASM-LIVE-FIRST-SLOT-SAMPLING`.

Epoch configuration must ensure `f + c < S` and `A <= P_r` from actual stake.
Current v3 has `P_r = S`. The optional `P_r <= Q` rule limits work; it is not a
per-slot safety or quorum-coverage condition.

Bind the leader-order algorithm to a protocol version. The accepted model treats
the shuffle results for distinct round seeds as independent uniform
permutations. Lean uses only the first selected slot. The current `StdRng` and
shuffle dependency is not a stable protocol definition across all dependency
versions. Lean proves the finite geometric failure bound for the first-slot
consequence. A true measure-one theorem still needs a probability-measure and
limit foundation. The proved deterministic repeated-first rule is a separate
product alternative; current Rust does not implement it.

### P1: prove ordinary causal block synchronization progress

Related assumptions: `ASM-LIVE-BLOCK-SYNC`, `ASM-LIVE-PEER-FAIRNESS`, and
`ASM-LIVE-TASK-FAIRNESS`.

The proof accepts this abstract behavior for now: after a correct validator gets
one ordinary block body, its synchronizer keeps fetching every missing causal
ancestor above that validator's local cleanup round. Each fetched body reveals
its direct parents and continues the walk. The requester retries fair peers,
waits when no peer is connected, buffers children, and accepts blocks from
parents to children. References at or below local cleanup are committed roots.
They need no body recovery.

Acceptance is parent-first. Thus, an accepted block already has its required
local causal history accepted. Lean must derive its finite above-GC capsule from
that accepted closure. Capsule availability is not a separate liveness
assumption. Retention and exact source service remain separate obligations.

The modeled `ValidatorBlock.parents` list is only the immediate-parent
projection used for quorum validity. A Rust block that jumps rounds can also
name an older own-author ancestor. A receiver-sync theorem must keep that full
dependency projection, as `SafeResumeBlock` does. An immediate-parent quorum
alone does not describe every body that the receiver must fetch.

This one-body rule does not by itself prove unbounded block production. The
completed operational-frontier composition above combines it with proposal,
replay, delivery, and finite correct-stake aggregation.

A later Rust review must check this abstraction against peer discovery, retries,
partial batches, empty peer sets, backpressure, restart, exact-reference reads,
and cleanup changes. Do not add a below-cleanup exception to discharge it.

The live service already handles a recent-cache miss through
`DagState::get_blocks`, which falls back to persisted `store.read_blocks`.
Proposal publication also flushes the child and its ancestors before broadcast.
The remaining gap is a Lean refinement that maps these persisted exact-body
reads to the existing causal-capsule and source-protection facts. It is not a
missing Rust store fallback.

Commit synchronization and commit votes in blocks are not liveness mechanisms.
Optional commit synchronization can stay as an acceleration path. Its exact
verification and install provenance remain safety obligations. Commit sync can
stop after commit-index catch-up while the local ordinary DAG still lags. It
cannot replace normal block synchronization in the liveness proof.

This condition applies to old consensus state. Transaction payloads can be
submitted again.

### P1: isolate optional commit sync from ordinary recovery

Related assumption: `ASM-LIVE-COMMIT-SYNC`.

Current Rust has the required control transitions:

- A suspended block subscription checks the local commit index once per second.
  It resumes inside the one-batch catch-up band, and connection attempts retry
  with bounded exponential backoff.
- If commit lag suppresses periodic block sync and the local commit index does
  not change for ten seconds, periodic sync starts its failover mode. It stays
  active until the local index moves by one configured commit-sync batch.
- Certified-commit processing accepts the exact commit blocks before install,
  then attempts a proposal and signals a newer threshold-clock round.
- GC removes missing dependencies at or below the new GC round and releases
  children that depended on them.

These transitions do not reserve CPU, network, or queue capacity. Keep the
accepted rule that commit-sync traffic and work cannot starve ordinary block
fetch, subscription retry, proposal callbacks, or recovery timers. Also keep
the existing peer-fairness, task-fairness, block-sync, and queue-service rules.

No separate commit-install cancellation assumption is needed for one commit
progress step. If the receiver's commit index increases, the step is complete.
If it does not increase, no commit installation can repeatedly reset that
receiver on the analyzed suffix. For later steps, GC cleanup still needs the
existing recovery no-idle and safe-resume mapping. BlockManager cleanup alone
does not create the proof's exact no-skip recovery target.

### P1: import commit blocks before GC advances

Related assumptions: `ASM-SAFE-GC` and
`ASM-LIVE-COMMIT-PROGRESS-RECOVERY`.

Before a local or synchronized commit install changes the local GC round, the
host must accept and catalogue every exact block in that commit in `DagState`.
After GC changes, it must retain the accepted, closed DAG frontier above the new
GC round. Blocks at or below the new GC round can become committed roots.

The current v3 certified-commit path calls `accept_committed_blocks` before it
handles and records each commit. Complete the refinement check for atomic
storage, restart, and retention of the accumulated above-GC installed-prefix
frontier. One `CommitV1.blocks` list contains only newly committed blocks, so it
is not the complete retained frontier.

This rule does not require a future synchronized commit. It constrains a commit
install that already occurred and gives the host legal parents for later normal
proposal work.

### P1: define finalizer shutdown behavior

Related assumptions: `ASM-LIVE-FINALIZER-TRIGGER` and
`ASM-LIVE-DURABILITY`.

A finalizer can hold a transaction that needs a later trigger. Draining its input
does not settle this state. Keep consensus active until the trigger occurs, store
and replay pending state, or define another safe epoch-tail result.

### P1: close the common commit-chain proof

Related assumptions: `ASM-SAFE-COMMIT-CHAIN` and `ASM-SAFE-FIRST-TRIGGER`.

Lean already proves exact same-prior commit output, unique exact successors,
finite exact paths from genesis, and equal installed references at the same
index. These theorems depend on local source maps for authenticated cached
decisions, canonical commit construction, durable prefix completeness, and
local or verified synchronized install provenance.

Complete those Rust-to-Lean maps across local production, synchronization,
normal restart, and old-block cleanup. Then derive the common chain as the
existing theorem result. Do not add one shared chain as an E2E input. Connect
that exact installed chain separately to the transaction finalizer stream and
the least eligible trigger.

Evidence:
[EV-EXACT-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-exact-commit-prefix) and
[EV-DURABLE-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-durable-commit-prefix).

### P1: preserve cached decision origin

Related assumption: `ASM-SAFE-EVIDENCE-REFINEMENT`.

Rust keeps `LeaderSlot.decision` as direct or indirect. The Lean source map now
uses that first origin. A direct result needs exact direct-quorum evidence. An
indirect result needs the exact deciding anchor, its ordered scan origin, its
historical result, and the valid anchor history.

`Decision::Indirect` does not currently keep the anchor reference. Store that
reference and enough immutable history identity, or add a same-host reverse map
that reconstructs the exact first decision event. The Lean model proves the
sticky rule and first-anchor prefix stability. Complete the Rust refinement for
retained pending state, leader-schedule reset, and restart reconstruction. Do
not classify a preserved indirect commit or skip as a fresh direct-quorum
result. Do not replace this local provenance with a cross-validator agreement
assumption.

Evidence:
[EV-CACHED-INDIRECT-ORIGIN](ASSUMPTION_EVIDENCE.md#ev-cached-indirect-origin).

### P1: bind the commit materializer and the v3 scorer to Rust

Related assumptions: `ASM-SAFE-GC`, `ASM-SAFE-COMMIT-CHAIN`,
`ASM-SAFE-COMMITTED-PREFIX`, and `ASM-REFINE-INTEGERS`.

Lean now models the v3 `FlexCommitter::build_commit` walk and the
`LeaderScheduleV3` replay. `CommitMaterializerView.buildCommit_materializes_exactly`
proves that a successful walk commits exactly the blocks its ancestor filter
reaches, and `buildCommit_output_nodup` proves it commits each block once.
`CommitMaterializerView.committed_flush_block_has_non_byzantine_parent_layer`
carries the weighted honest-parent bound to that flush and locates each honest
parent body. `V3ScheduleState.addCommit_sound` and
`V3ReplayParameters.replayed_state_sound` prove that the incremental scorer
totals stay equal to a recomputation over the retained window, so the Rust
`checked_sub` cannot underflow.
`ExactCommitInstallProvenance.exactInstalledHeadsAtSameIndexShareRustSchedule`
gives one replayed schedule state to two correct hosts at one commit index, so
every read they take from it agrees.

Four source rows remain open.

- `REF-COMMIT-MATERIALIZER-WALK`. Map the walk to the Rust loop. Show that
  `get_block(..).unwrap()` finds every above-GC ancestor that no earlier commit
  took, and that the finite store and the committed marks end the loop. The Lean
  model uses a step budget for the loop instead of deriving the finite-store
  bound, and it uses a front stack where Rust uses a back stack. The visit order
  does not change the committed set, and the seeded sort fixes the vector.
- `REF-COMMIT-BODY-ORDER`. `sort_committed_blocks` keys on the block round and
  on a hash of the commit seed and the block digest, so equal keys need a hash
  collision. Confirm that, and map the named leader and the commit timestamp.
  The timestamp reads the leaders' one-round-below ancestors from `DagState`,
  which can include blocks that an earlier commit already took, so it is not a
  function of the flush alone. The pre-v3 `sort_sub_dag_blocks` keys only on
  round and author and has no tie-break; two committed blocks from one
  equivocating author can tie there.
- `REF-V3-SCHEDULE-SCORER`. Map `add_commit` to the modeled window. Confirm that
  the scoring calculation is a deterministic function of the four committed
  materials and reads no other local state, that `refresh_current_schedule`
  recomputes `allowed_leaders` only at an update-interval boundary, and that
  `select_allowed_leaders` seeds its shuffle from the last pending commit digest.
  The model does not reproduce the voting scan, the certifying scan, the
  equivocation rule, or the distinct-author stake sums, so those stay source
  obligations rather than checked Lean definitions.
- `REF-V3-SCHEDULE-READERS`. Confirm that proposer ancestor selection and the
  FlexCommitter read the current replayed allowed-leader vector, its round
  order, and the minimum next leader round, and not separately derived values.
  This row overlaps the first-Flex-leader parent work above.
- `REF-V3-SCHEDULE-INPUTS`. Map the other two `add_commit` input routes. A
  synchronized commit arrives from `FlexCommitter::handle_certified_commit`, and
  `LeaderScheduleV3::from_store` replays stored sub-DAGs over a bounded suffix
  that starts at `replay_start`. Lean replays from genesis and maps only the
  local build path, so it has no theorem that equates the bounded suffix replay
  with the full replay.

The Lean model is also more permissive than the running code in two places. The
walk drops a followed reference whose body is missing, and it accepts an empty
leader set, an already-committed leader, and a leader at or below GC; Rust
aborts in each case. `add_commit` carries no invariant for consecutive commit
indexes, strictly increasing leader rounds, a nonempty leader set that contains
the named leader, or the scan sentinel, all of which Rust asserts. A theorem
about a successful modeled run therefore still applies to a successful Rust run,
but the model does not reproduce the Rust failure conditions.

The Lean model also carries the local walk view for two hosts. Deriving one
committed block set from two hosts still needs their GC rounds, their prior
committed marks, their ancestor lists, and the walk-reachable bodies to agree.
That is the same completeness condition that
`ReferenceCommitMaterializerSourceMap` records. The prior committed marks are set
by this walk for a local commit and by
`FlexCommitter::handle_certified_commit` for a synchronized certified commit, so
both routes are part of that condition.

### P1: protect committed-prefix evidence

Related assumptions: `ASM-SAFE-COMMITTED-PREFIX`, `ASM-SAFE-GC`, and
`ASM-SAFE-PARENT-QUORUM`.

Indirect decisions need complete ordered evidence, including all leaders in a
multi-leader commit. Show that required votes and triggers enter the pending
committed prefix on local, synchronization, recovery, replay, and restart paths.

Local ownership protects evidence after it enters that prefix. The open gap is
end-to-end inclusion, not deletion from the live block cache alone. Enforce the
required cleanup depth and signed transaction cutoff.

Evidence:
[EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).

## Conformance and boundaries

### P2: check integer limits

Related assumption: `ASM-REFINE-INTEGERS`.

Set and check limits for stake, rounds, commit indices, transaction counts, schedule
counters, and collection sizes. Use checked arithmetic and test boundary values.

### P2: add a shared conformance suite

Related assumptions: `ASM-SAFE-AUTHENTICATION`,
`ASM-SAFE-NON-EQUIVOCATION`, and `ASM-SAFE-EVIDENCE-REFINEMENT`.

Use common test vectors for leader decisions, transaction decisions, trigger
selection, old-block cleanup, commit construction, and equivocation. Run the same
cases against the model and the product.
