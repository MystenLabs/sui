<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 proof assumptions and product obligations

This ledger records the conditions that connect the formal model to the product
and its operating environment. A theorem is conditional even when the proof has
no declared axiom. Apply it to the product only when each related condition is
satisfied or explicitly accepted.

The [assumption evidence ledger](ASSUMPTION_EVIDENCE.md) records the exact Rust
and Lean evidence, limits, source revisions, and focused revalidation triggers
for reviewed assumptions. The [implementation gap report](IMPLEMENTATION_GAPS.md)
records missing product work.

## Assumption boundary

A basic environment assumption describes one simple fact, such as bounded message
delivery, bounded local work, fair task execution, durable storage, a fault set,
or a probability distribution.

A quorum entering recovery, a sequence of block layers, a usable anchor, or a new
commit is not a basic assumption. The protocol proof must derive each such result.
The proof must show that each fixed product rule matches the model.

The network-round theorem meets this boundary. It derives each future quorum
layer from local current-state and execution rules. The adopted commit route
uses ordinary DAG propagation, fixed-reference quadratic waits, V2 no-skip
round catch-up, pinned block sync, commit-orthogonal retention, local
FlexCommitter execution, and exact-prefix induction. The theorem
`current_sources_give_end_to_end_liveness_probability_one` completes this route
in Lean under the listed proposed source rules. The strict proof derives timer
spread from actual prior broadcasts, pinned sync, and one action-local
exact-next timer-promptness rule. Its fields are static, local, current, or
past. A separate exact-replay proof experiment is not an adopted liveness route.

## Shared proof model

### Fundamental environment inputs

- Byzantine stake is at most `f`. Byzantine plus unavailable stake is at most
  `f + c`. The Byzantine and unavailable sets stay fixed during one stable proof
  interval.
- After network stabilization, messages between correct validators arrive within
  `delta`.
- Required local work finishes within a finite bound `epsilon`.
- Correct local clocks continue to advance. Timers do not expire early, and each
  finite timer expires. Accepted commit timestamps have a bounded future offset.
- Continuously enabled tasks at correct validators eventually run.
- Commit-sync requests, responses, verification, and installation do not starve
  ordinary block fetch, subscription retry, Core proposal callbacks, or recovery
  timers. Commit-sync success itself is not assumed.

### Source-to-model and local product conditions

- Correct validators use one authenticated epoch configuration, validator set,
  threshold set, and protocol-versioned leader-order algorithm. For the same
  exact commit head and effective schedule key, that algorithm gives the same
  leader schedule and selected slot order.
- Signatures bind all decision data. One validator counts at most once on each
  side of one decision.
- Durable storage, accepted blocks, cleanup state, pending rounds, and local
  commit actions have the meanings used in Lean.
- The running-Core execution trace starts after `recover_validator` finishes.
  Startup recovery is outside this trace and must establish its initial durable
  state before trace time zero.
- Each qualifying external Core input handler completes all finite commit work
  that its batch enables, including finite cleanup-driven unsuspension. In the
  positive DAG proof, a qualifying handler is an `add_blocks` call that accepts
  at least one ordinary block. It observes the terminal no-more-commits result
  and invokes the normal proposal attempt before it returns. Core executes the
  handler and proposal callback serially. A commit or block-accept action does
  not interleave after that callback starts. This rule does not say that the
  proposal succeeds.
- Leader-schedule stake `S` satisfies `f + c < S`. Round-selection stake `P_r`
  satisfies `A <= P_r`. Current v3 has `P_r = S`.

### Accepted probability model

During one stalled commit index, each round's complete leader order follows the
stated probability law. The accepted design uses independent uniform orders, but
the current trace theorem still takes a favorable trace. A proved deterministic
coverage rule can replace this input.

### Derived proof goals

The protocol proofs must derive synchronization progress, one common commit
chain, recovery quorum stake, total-quorum block layers at unbounded rounds,
usable direct ranges, and unbounded local commit progress at every correct,
available validator. The public network-DAG goal does not require
per-validator production. The adopted commit proof derives unbounded later own
blocks through the V2 current source package. It then uses the proposed
no-skip rule to recover only the finite intermediate window that the favorable
path needs. The ledger keeps these results as proof goals, not final-theorem
inputs.

## Status values

- **Discharged in Lean** means that the formal model proves the claim.
- **Enforced in Rust** means that the current product prevents a violation.
- **Partially verified** means that product evidence covers part of the claim.
- **Environmental assumption** means that the operating environment supplies the
  claim.
- **Open proof obligation** means that a required result is not yet established.
- **Abstraction gap** means that model events lack a complete product mapping.
- **Accepted modeling assumption** means that the proof intentionally uses the
  stated model.
- **Known mismatch** means that the current product contradicts or lacks the rule.

## Current status

| Status | Count |
|---|---:|
| Discharged in Lean | 1 |
| Enforced in Rust | 2 |
| Partially verified | 8 |
| Environmental assumption | 4 |
| Open proof obligation | 4 |
| Abstraction gap | 1 |
| Accepted modeling assumption | 4 |
| Known mismatch | 7 |

A known mismatch blocks the affected product claim. Other open statuses identify
a condition; they do not indicate a failed proof inside the model.

## Maintenance rules

1. Keep each `ASM-*` and `REF-*` identifier stable.
2. Change a status only with proof, product, test, or environment evidence.
3. Do not turn a protocol result into a basic environment assumption.
4. Review all affected mappings after a product change.
5. Keep missing behavior separate from verified current behavior.
6. Keep detailed source evidence and revalidation triggers in the assumption
   evidence ledger.

## Periodic Lean-to-Rust refinement review

This is the canonical source-to-model checklist. A verified row covers only its
stated behavior. It does not prove an end-to-end theorem.

Last complete source review: 2026-08-15, at `d630b4452a8`.

Focused leader-schedule, exact-prefix, restart, and finalizer review:
2026-08-17, at `2fecfec37462785ccd6684195aac9131e54ad251`. See the
[assumption evidence ledger](ASSUMPTION_EVIDENCE.md).

Focused commit-sync safety, subscription-resume, periodic-sync-failover, and GC
review: 2026-08-18, at `2e05fcf9cbeba4d42b0cc4145312ae053dba14dc`.
See [EV-COMMIT-SYNC-COVERAGE](ASSUMPTION_EVIDENCE.md#ev-commit-sync-coverage).

### Missing Rust behavior and open source refinements

#### Commit progress recovery

| Review ID | Required behavior or guarantee |
|---|---|
| `REF-RECOVERY-ENTRY` | Use the normal `threshold_clock_round()` candidate only as the round-gap probe. Enter when this gap reaches `P_enter` or the time since any commit install reaches `T`. Stay active while the gap reaches `P_exit` or the time reaches `T`, where `P_exit < P_enter`. Exit only when both signals clear. Keep the exact-next recovery target separate. |
| `REF-NEXT-ROUND-TARGET` | Exact-next recovery proposes only one round after the highest known own proposal. A separate, unproved cleanup safe-resume step can use one witnessed jump when old parent history is unusable. |
| `REF-RECOVERY-PACING` | Use one fixed reference round `R_c` and the quadratic wait `W(R) = b + l * (R - R_c) + q * (R - R_c)^2`. The wait value does not depend on the local commit head. Derive timer spread from actual prior broadcasts and sync. Use one action-local exact-next timer-promptness rule. Start a frontier timer only after quorum parents are ready. One timer generation owns one proposal action in its bounded deadline window. Do not reuse an earlier expired or stale timer. The coefficient `q` must dominate the derived fixed-reference timer-spread and causal-visibility costs. |
| `REF-RECOVERY-PARENTS` | Wait for immediate-parent quorum and request missing parents. At parent selection, include the current locally accepted and retained representative for every in-range author for which one exists. Count an equivocating author once, ignore its other branches, do not consult the predicted leader schedule, and do not use score exclusion there. |
| `REF-RECOVERY-PROPOSAL-ORIGIN` | While block-progress recovery is active above GC, cancel or replace stale normal ready work before persistence. An actual persisted ready proposal in this state must come from the current recovery timer and its refreshed parent selection. |
| `REF-RECOVERY-TIMER-ORIGIN` | Map each actual timer-paced proposal to its exact earlier timer start for the same validator, commit head, and target round. Make that timer key unique. From a current parent-ready state, select a bounded exact timer start or show that the local commit head already advanced. These are current or past facts. They do not state that a future timer or proposal exists. |
| `REF-RECOVERY-FRONTIER` | Connect every recovered own round to a durable signed block and the required parent history. The common maximum round remains a proof value. |
| `REF-RECOVERY-GC-FRONTIER` | Keep each recovery target above the old-block cleanup boundary or define a safe resume rule. |
| `REF-RECOVERY-LAYER-MAPPING` | Retain and identify all rounds in the complete recovery anchor window. |
| `REF-CAUSAL-CAPSULE-PROJECTION` | For each actual correct proposal persistence action, identify the durable pinned recovery capsule with the exact static capsule used by the backlog projection. Keep unique references, in-range authors, and the target-round upper bound. This equality is a past persistence refinement. It does not state future delivery or acceptance. |
| `REF-LOCAL-PROPOSAL-PROGRESS` | Map the current one-shot maximum timeout and its watcher retries. A forced attempt can stop for the old round only if that round is already signed or the threshold clock moved higher. A missing recovered own-round value or excessive propagation delay is temporary. The related watcher makes another forced attempt when that blocker clears. Installing a commit must not interrupt an active Core callback. |
| `REF-CURRENT-TIP-REPLAY` | Map the current receiver-driven subscription path. A broken, ended, or idle stream terminates. The correct receiver retries. A successful subscription sends the requested cached own block, if it is available, or the sender's latest own block. The proof uses the exact requested tip or treats a newer tip as higher frontier progress. |
| `REF-POST-GST-CAUSAL-SERVICE` | Map the V2 current no-idle source package. Its selected support, recursive need, queue-source, and no-idle rules derive unbounded later own-block production at each correct, available host. Map pinned ordinary block sync and commit-orthogonal above-GC retention separately. The no-skip round-catch-up rule derives only the finite intermediate window that fixed-reference pacing needs. None of these source rules supplies a future block, window, Flex result, or commit. |

#### Other features

| Review ID | Required behavior or guarantee |
|---|---|
| `REF-EPOCH-CONFIG` | Put all proof-relevant values in authenticated epoch state and reject incompatible values. |
| `REF-INTEGER-BOUNDS` | Set explicit numeric limits and use checked calculations for all modeled values. |
| `REF-FINITE-BLOCK-ID-SPACE` | Map `BlockId` injectively into one fixed finite space. For current Rust, use the fixed byte-array space of `BlockDigest`. Together with the finite authority set and unique capsule references, this gives the static per-round reference cap. |
| `REF-V3-ACTIVATION` | Activate v3 from shared epoch state. |
| `REF-V3-TRANSACTION-PATH` | Implement the modeled v3 proposal, transaction-vote, cutoff, and finalization path. |
| `REF-AMNESIA-SIGNER-GUARD` | Prevent conflicting signatures after complete local consensus-state loss. |
| `REF-CORE-HANDLER-COMPLETION` | Map each nonempty ordinary `add_blocks` batch to `ValidatorCoreHandlerInputObservation` through `handlerInputOccurs` and `qualifyingInputHasFiniteHandler`. Map direct packet-driven input acceptance through `ValidatorPacketDrivenBlockAcceptanceAt` and `packetDrivenAcceptanceHasInputOrigin`. Do not classify later GC-unsuspension as a new input; use its enclosing handler episode. Cover the finite input batch and GC-unsuspended block work, observe the terminal no-more-commits result, and invoke `try_propose(false)` before return. Map the already-actual attempt through `ValidatorCoreProposalAttemptContinuationRules` to a same-batch proposal action, protected normal work, or current durable retry work. These mappings must not assert proposal success or future DAG progress. |
| `REF-PARENT-SYNC` | For an ordinary block, keep fetching each missing causal-history block above the requester's local cleanup round. Handle empty peer sets, recursive direct-parent discovery, and fair retries. References at or below local cleanup are committed roots and require no recovery. |
| `REF-FLEX-ACCEPTED-BODY-OWNERSHIP` | At one actual Flex action-before state, map each authenticated accepted body from a correct author to that author's exact durable `ownBlockAt` entry. Derive the restored time-zero origin or an exact earlier proposal-persistence action from this current fact. Do not state future production. |
| `REF-FLEX-POST-REFRESH-INPUT` | Map each actual `runCommitter` action to the literal scan input that Rust reads after it refreshes pending rounds. The same action must return the result computed from that input. Result equality is derived from the two views of the same action; it is not a separate future-result premise. |
| `REF-COMMIT-SYNC-PROGRESS` | Optional commit synchronization can accelerate catch-up. Subscription suspension must be released after local catch-up, and periodic ordinary sync must resume when commit-index progress stalls. Commit-sync work must not starve ordinary sync or proposal work. Commit sync can still stop after commit-index catch-up while the local ordinary DAG lags. Therefore, the final liveness proof must not depend on commit-sync success or commit votes in blocks. |
| `REF-COMMON-COMMIT-CHAIN` | Establish one index-and-digest commit chain across local production, synchronization, and restart. |
| `REF-GC-EVIDENCE` | Keep complete decision evidence across local production, synchronization, replay, restart, and transaction finalization. |
| `REF-LEADER-BOUNDS` | Enforce `f + c < S` and `A <= P_r` from actual epoch stake. |
| `REF-LEADER-ORDER-COMPATIBILITY` | Use a protocol-stable or version-gated schedule and round-order algorithm, with compatibility vectors. |
| `REF-FINALIZER-TAIL` | Define the result for pending finalizer state when an epoch ends before a later trigger. |
| `REF-ROUND-CATCHUP` | If fixed-reference liveness needs skipped rounds, make each active correct proposal persistence use exactly `highestSignedRound + 1`. The final path binds each recovered intermediate persistence to its earlier commit-progress-recovery timer generation, exact `proposeNext` action, and refreshed parent snapshot for the same block. These are current or past facts. They do not state that a future intermediate block exists. |

### Verified current Rust behavior

| Review ID | Verified behavior |
|---|---|
| `REF-THRESHOLD-CONSTRUCTION` | For non-overflowing inputs, threshold construction checks both safety inequalities. |
| `REF-AUTHENTICATION` | Protocol signatures cover the complete block, author keys are checked, and validator connections use mutual authentication. |
| `REF-VOTE-DEDUP` | One validator identity counts once in each voter set. |
| `REF-COMMIT-STATE` | One durable commit supplies the local index, reference, and protocol timestamp after restart. |
| `REF-OWN-PROPOSAL-ROUND` | Normal restart restores the own-round floor. Peer-assisted empty-store recovery sets a verified floor. Complete amnesia remains open. |
| `REF-DURABLE-PROPOSAL` | A local proposal is durable before broadcast. |
| `REF-PARENT-QUORUM` | An ordinary accepted non-genesis block has immediate parents from distinct validators with quorum stake. |
| `REF-BLOCK-PARENT-ACCEPTANCE` | The ordinary live path accepts required above-boundary parents before their child. Certified commits use a separate checked path. |
| `REF-BLOCK-SYNC-MECHANISMS` | Direct, periodic, history, and stall-recovery block-fetch paths exist. When commit lag suppresses periodic sync, a commit-index stall starts the periodic failover path. Their existence and this transition do not by themselves prove peer service or scheduling progress. |
| `REF-COMMIT-SYNC-CHECKS` | Synchronized ranges are checked for indexes, digest links, block references, gaps, order, and quorum support on the range tip. Each commit does not have a separate certificate. |
| `REF-LEADER-SCHEDULE` | The same prefix and same fixed build and random-generator configuration produce the same ordered schedule and interval. |
| `REF-ROUND-LEADER-SELECTION` | Each stored pending v3 round contains the full schedule in one deterministic round order. Thus, `P_r = S`. |
| `REF-DIRECT-DECISION` | Direct selected-slot decisions use the modeled commit, skip, and undecided rules. |
| `REF-INDIRECT-DECISION` | Indirect selected-slot decisions use ordered, deduplicated certificate evidence. |
| `REF-DECISION-ORIGIN` | `update_slot_decision` records whether the first result was direct or indirect. It changes only an undecided slot and asserts result equality on a later update. The exact indirect anchor and history are not stored. |
| `REF-PENDING-ROUNDS` | Pending rounds are consecutive. The scan visits each base that has a complete anchor window; newer stored rounds can supply anchor evidence only. |
| `REF-GC-BOUNDARY` | Local cleanup keeps above-boundary blocks. Pending commit state and finalizer state own copied evidence independently of the live cache. |
| `REF-COMMIT-INSTALL-DAG` | Before a local or synchronized commit install moves GC, accept and catalogue every exact commit-body block. Retain the accumulated closed frontier above the new GC round. |
| `REF-FLEX-RESULT` | When the current decision scan finds a commit, the local commit state records it through the normal commit path. |
| `REF-COMMIT-MATERIALIZER-WALK` | With `enable_v3`, `FlexCommitter::build_commit` marks every committed leader of the commit round and walks their causal history. It follows an ancestor only when the reference is above the local GC round and no earlier commit took it, and it marks each followed reference before it pushes the body. `get_block(..).unwrap()` must find every such body in the local `DagState`. The finite store and the committed marks must end the loop. A repeated ancestor reference is dropped by `if !set_committed(..) { continue; }`; block verification also rejects two ancestors from one author. `Linearizer::linearize_sub_dag` is the one-leader pre-v3 form of the same walk. Rust also aborts on an empty leader set, on a leader that is already committed, and on a committed block at or below GC. The Lean walk has no such failure, so a modeled run can succeed where Rust panics. |
| `REF-COMMIT-BODY-ORDER` | `sort_committed_blocks` keys on the block round and on `hash(seed \|\| digest)`, with a seed over the committed leader digests. Distinct committed references must therefore get distinct keys. The named leader is the last block of the sorted vector, and `calculate_commit_timestamp` reads the leaders' one-round-below ancestors from `DagState`, which can include blocks that an earlier commit already took. The pre-v3 `sort_sub_dag_blocks` keys only on round and author and has no such tie-break. |
| `REF-V3-SCHEDULE-SCORER` | `LeaderScheduleV3::add_commit` keeps a three-deep pending window and scores `C-3` against `[C-2, C-1, C]`. Its scoring calculation must be a deterministic function of those four committed materials: it must read only the commit index, the commit digest, the named leader, and the sorted committed block bodies with their `ancestors()`, and no other local state. The model takes that calculation as one function and does not reproduce its arithmetic, so the voting scan, the certifying scan, the equivocation rule, and the distinct-author stake sums stay source obligations. Its running per-authority totals move by `checked_add` and `checked_sub` over the sliding entry window. `refresh_current_schedule` recomputes `allowed_leaders` only at an update-interval boundary, and `select_allowed_leaders` seeds its shuffle from the commit digest of the last pending commit. Rust also asserts consecutive commit indexes, strictly increasing leader rounds across the pending window, a nonempty leader set that contains the named leader, and a scan that ends above its bound. The Lean model does not carry those invariants, so it accepts histories that Rust would abort. |
| `REF-V3-SCHEDULE-READERS` | Proposer ancestor selection and the FlexCommitter must read the allowed-leader vector, the round order, and the minimum next leader round of the current replayed schedule state, not separately derived values. |
| `REF-V3-SCHEDULE-INPUTS` | `LeaderScheduleV3::add_commit` is also called for a synchronized commit that `FlexCommitter::handle_certified_commit` built, and `from_store` replays committed sub-DAGs from storage over a bounded suffix that starts at `replay_start`. The Lean model replays from genesis and maps only the local build path. Map the other two input routes, and show that the bounded suffix replay reaches the same schedule state. |

### Accepted model and environment

Independent uniform leader ordering is an accepted probability model. It is not
a claim about current Rust. The product uses a common deterministic round-based
order for one fixed compatible build. The model does not prove independent
samples for that exact sequence.

Post-stabilization delivery, local response time, and fair task execution are
environment conditions. Retention, serving, peer retry, and request processing
are local product rules.

For each review, record the source revision and date, check every row affected by
the change, and update the related assumption when a meaning changes.

The current review does not establish finalizer recovery for a pending decision
window with a nonzero cleanup boundary. A test that can hide failure in detached
finalizer work is not evidence for this result.

## ASM-MATH-THRESHOLDS

- **Claim:** The nominal values `N = 5f + 3c + 1`, `Q = 4f + 2c + 1`, and `A = 2f + c + 1` satisfy both safety inequalities.
- **Type:** Mathematical.
- **Status:** Discharged in Lean.
- **Effect if false:** Safety.
- **Lean use:** The safety proofs use the two threshold inequalities.
- **Rust evidence:** Product threshold construction checks both inequalities for actual stake.
- **Discharge:** Keep the actual-threshold mapping and boundary tests current.

## ASM-SAFE-PARAMETERS

- **Claim:** All correct validators in one epoch use one authenticated validator set, threshold set, cleanup depth, leader schedule, and feature set.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** Every weighted decision uses one common configuration.
- **Rust evidence:** Some v3 inputs can come from local process settings.
- **Discharge:** Move all proof-relevant inputs into authenticated epoch state.

## ASM-SAFE-FAULT-BOUND

- **Claim:** During one stable liveness interval, the Byzantine and unavailable sets are fixed. Byzantine stake is at most `f`, and their combined stake is at most `f + c`.
- **Type:** Adversary and availability model.
- **Status:** Environmental assumption.
- **Effect if false:** Safety for `f`; liveness for `f + c`.
- **Lean use:** Quorum intersection and progress results use these bounds.
- **Rust evidence:** The product cannot identify all faulty or unavailable validators.
- **Discharge:** State and monitor the deployment fault model.

## ASM-SAFE-AUTHENTICATION

- **Claim:** A verified signature binds the validator, epoch, round, complete block, and all decision data.
- **Type:** Rust refinement.
- **Status:** Enforced in Rust.
- **Effect if false:** Safety.
- **Lean use:** Voter sets treat each authenticated block as one validator's statement.
- **Rust evidence:** Complete-block signatures and mutual validator authentication enforce this claim.
- **Discharge:** Preserve these checks and their compatibility tests.

## ASM-SAFE-NON-EQUIVOCATION

- **Claim:** A correct validator does not sign conflicting blocks. A validator identity counts no more than once on each side of a decision.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Conflicting evidence can overlap only in Byzantine stake.
- **Rust evidence:** Vote sets deduplicate identities, and durable restart preserves the signed-round floor.
- **Discharge:** Add a signer guard for complete local-state loss or count that validator as faulty.

## ASM-SAFE-PARENT-QUORUM

- **Claim:** Every accepted non-genesis block has at least `Q` distinct stake in verified immediate parents.
- **Type:** Rust refinement.
- **Status:** Enforced in Rust.
- **Effect if false:** Safety.
- **Lean use:** Indirect safety uses the anchor's parent quorum.
- **Rust evidence:** Ordinary block verification rejects insufficient or duplicate parent stake.
- **Discharge:** Preserve the check for every accepted block path.

## ASM-SAFE-EVIDENCE-REFINEMENT

- **Claim:** Product leader and transaction decisions use the same evidence, voter accounting, result rules, and cached decision origin as the model. A cached indirect result retains its exact deciding anchor and ordered history. Once a slot is committed or skipped, later direct and indirect rule runs preserve that result. The signed cutoff is the maximum of the block-cleanup and vote-cleanup rounds.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** The first-decision relation keeps direct evidence for direct results. For an indirect result, it keeps the historical anchor, history, result, and ordered scan prefix. It proves that later direct and indirect passes preserve the first result. Safety compares the reconstructed direct bases. It does not treat a cached indirect result as direct.
- **Rust evidence:** `RoundState::update_slot_decision` changes only an undecided slot, records the first direct or indirect origin, and asserts equality on a later update. A fully decided round can skip another indirect run. `Decision::Indirect` does not keep the deciding anchor reference or history. A restart or leader-schedule reset constructs new pending state; it must also construct new provenance. The modeled v3 transaction path is incomplete.
- **Evidence record:** [EV-CACHED-INDIRECT-ORIGIN](ASSUMPTION_EVIDENCE.md#ev-cached-indirect-origin).
- **Discharge:** Retain or reconstruct the exact indirect anchor, immutable history, and scan origin; map reset and restart reconstruction; add the transaction path; and add shared conformance vectors.

## ASM-SAFE-COMMIT-CHAIN

- **Claim:** Correct validators process one common, continuous index-and-digest commit chain in order.
- **Type:** Derived protocol and Rust refinement theorem.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Current safety lemmas use this condition. The final system proof must derive it from deterministic commit construction, exact-reference certification, and verified in-order installation.
- **Rust evidence:** Local and synchronized inputs have chain checks, but no cross-validator proof exists.
- **Evidence records:** [EV-EXACT-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-exact-commit-prefix) and [EV-DURABLE-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-durable-commit-prefix).
- **Discharge:** Prove the common-chain result across local production, synchronization, and restart.

## ASM-SAFE-FIRST-TRIGGER

- **Claim:** Correct validators use the same first eligible depth-two commit as the transaction trigger.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Indirect transaction agreement depends on one first trigger.
- **Rust evidence:** Ordered processing supports this rule when the common-chain claim holds.
- **Evidence record:** [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Derive trigger equality from the common chain and verify all input paths.

## ASM-SAFE-COMMITTED-PREFIX

- **Claim:** Before the first trigger, the committed prefix contains every decision witness required by the indirect rule.
- **Type:** Protocol and Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety.
- **Lean use:** Direct-against-indirect safety needs the missing witness in the prefix.
- **Rust evidence:** Local and synchronized paths construct prefixes, but complete inclusion is not proved.
- **Evidence record:** [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Prove witness inclusion for local, synchronization, replay, and restart paths.

## ASM-SAFE-GC

- **Claim:** Old-block cleanup uses the preceding commit boundary. Before a commit install moves GC, the host accepts and catalogues every exact commit-body block. After the move, it retains the accumulated closed DAG frontier above the new GC round. It retains decision evidence until the rule no longer needs it or copies the evidence into the committed prefix.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Leader and transaction safety use retained anchor and vote evidence. Post-GC liveness uses the accepted and retained above-GC frontier as legal proposal parents.
- **Rust evidence:** The v3 certified-commit path accepts each commit's blocks before it handles and records the commit. Local ownership is verified. Atomic persistence, restart, accumulated-prefix retention, and the complete evidence path still need a full refinement check.
- **Evidence records:** [EV-DURABLE-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-durable-commit-prefix) and [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Enforce the required depth, test accept-before-GC ordering for local and synchronized installs, and close the complete evidence and post-GC frontier mappings.

## ASM-CONFIG-V3-ACTIVATION

- **Claim:** Authenticated epoch state enables the analyzed v3 leader and transaction paths.
- **Type:** Configuration applicability.
- **Status:** Known mismatch.
- **Effect if false:** Applicability.
- **Lean use:** The model describes the v3 protocol path.
- **Rust evidence:** Normal startup does not enable the analyzed path from epoch state.
- **Discharge:** Add versioned activation, rollback rules, and mixed-version tests.

## ASM-CONFIG-VOTING

- **Claim:** V3 activation also activates the modeled transaction-voting rule for every correct validator.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** Transaction safety always uses v3 voting semantics.
- **Rust evidence:** V3 and transaction voting can be configured independently.
- **Discharge:** Use one feature value or reject an incompatible pair.

## ASM-REFINE-INTEGERS

- **Claim:** Product integer types and calculations represent every modeled value without overflow, truncation, or invalid conversion.
- **Type:** Data refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety and liveness.
- **Lean use:** The model uses unbounded natural numbers.
- **Rust evidence:** The product uses bounded types and does not check every modeled operation.
- **Discharge:** Set limits, use checked calculations, and test all boundaries.

## ASM-LIVE-PARTIAL-SYNCHRONY

- **Claim:** After an unknown stabilization time, each protocol message between correct validators arrives within `delta`.
- **Type:** Network environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** Progress proofs use bounded delivery only after stabilization.
- **Rust evidence:** The product can retry and measure delays but cannot enforce the network bound.
- **Discharge:** Keep this condition in the deployment model.

## ASM-LIVE-FINITE-REFERENCE-SPACE

- **Claim:** `BlockId` has one fixed finite injective encoding. A persisted
  causal capsule has unique block references, each author is in the finite
  authority set, and each history round is at or below the target block round.
  Therefore, one capsule has at most
  `authorityCount * blockIdCount` references at one round. The pinned capsule
  created by proposal persistence is exactly the capsule used by this bound.
- **Type:** Data and source refinement.
- **Status:** Partially verified.
- **Effect if false:** Commit liveness.
- **Lean use:** `ValidatorFiniteBlockIdEncoding` derives the static per-round
  cap. The receiver's GC round then gives a linear bound on unresolved history.
  The adopted proof combines pointwise visibility bounds with one
  fixed-reference quadratic wait. The wait does not change when a local commit
  head changes.
- **Rust evidence:** `BlockDigest` is a fixed byte array, and the committee has
  a finite authority count. The exact persisted-capsule pin projection and all
  capsule source fields still need a complete Rust-to-Lean map.
- **Evidence record:** [EV-FINITE-REFERENCE-SPACE-TIMING](ASSUMPTION_EVIDENCE.md#ev-finite-reference-space-timing).
- **Discharge:** Map the exact finite digest encoding, capsule uniqueness,
  author range, target-round upper bound, and persisted pin-projection equality.
  Check all modeled arithmetic against product integer limits.

## ASM-LIVE-ROUND-CATCHUP

- **Claim:** When fixed-reference liveness needs skipped rounds, an active
  correct validator persists only round `highestSignedRound + 1`. Each actual
  fresh intermediate persistence used by the final path has an earlier
  commit-progress-recovery timer generation, exact `proposeNext` action, and
  refreshed parent snapshot for the same block. Thus, one actual later own
  block can prove each required intermediate persistence. The source rules
  classify current or past actions. They do not supply a future block or
  window. A generic normal-proposal origin is not a final input.
- **Type:** Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** The completed conditional theorem does not apply to the
  product. Lean cannot reconstruct its finite exact timer-paced window from
  later unbounded own-block production.
- **Lean use:** `ValidatorV2RoundCatchupSourceMap` gives the two past-only
  source rules. `actual_high_own_block_gives_fresh_timer_paced_intermediate`
  uses the first signer-floor crossing to recover one exact target.
  `block_production_liveness_gives_backfilled_timer_paced_window` applies this
  result to every required author and offset. It returns only actual
  timer-paced productions.
- **Rust evidence:** `try_new_block` reads `threshold_clock_round()` and checks
  only that this round is higher than the last own proposal. The local round
  can skip intermediate own rounds. The timeout callback then calls
  `try_propose`, which reads the current threshold again. The current timeout
  task does not keep one unique fixed timer generation for each skipped round.
- **Evidence record:** [EV-ROUND-CATCHUP](ASSUMPTION_EVIDENCE.md#ev-round-catchup).
- **Discharge:** Add a safe no-skip proposal queue, or an equivalent
  intermediate-round worker. Persist each target in order. Keep the exact
  timer key and refreshed parent snapshot until that target runs. Add tests for
  threshold-clock jumps, timer resets, commit interference, and restart.

## ASM-LIVE-COMMIT-PROGRESS-RECOVERY

- **Claim:** An active v3 validator enters recovery when its normal
  threshold-clock proposal gap reaches `P_enter` or the time since any commit
  install reaches `T`. It stays active while the gap reaches `P_exit` or the
  time reaches `T`, where `P_exit < P_enter`. It exits only when both signals
  clear. This probe is separate from the exact-next target. The validator
  requests missing parents and proposes only after it accepts an
  immediate-parent quorum. Parent selection includes every current accepted and
  retained representative, one per author, without schedule prediction or
  score exclusion. Exact-next recovery uses the round after its highest durable
  own round. Cleanup safe resume durably records the canonical target
  `max(P + 1, G + 2)` and its parent need before proposal selection. It does not
  require an accepted future block at the target. While recovery is active
  above GC, stale normal ready work cannot persist. Persistence must use the
  current recovery proposal and the unique timer for the same validator and
  target round. The adopted commit proof uses one fixed reference round `R_c`
  and the wait
  `W(R) = b + l * (R - R_c) + q * (R - R_c)^2`. The final source boundary
  derives timer spread from actual prior broadcasts and sync plus one
  action-local exact-next timer-promptness rule.
- **Type:** Protocol and Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `ValidatorFixedReferenceStrictCurrentSourceMaps` supplies the
  fixed wait, one action-local exact-next timer-promptness rule, authenticated
  correct-body ownership, past recovery-timer origins, and the checked
  quadratic coefficient. The proof derives timer spread from actual prior
  broadcasts and pinned sync. V2 round catch-up supplies the finite exact
  production family. Commit-orthogonal retention makes each selected leader
  usable before the next proposal snapshot. Local Flex execution and
  exact-prefix induction then derive commit progress and pointwise catch-up. The theorem
  `current_sources_give_end_to_end_liveness_probability_one` completes this
  conditional chain.
- **Rust evidence:** The current product has no commit progress recovery mode.
  It uses fixed proposal waits, not this fixed-reference quadratic wait.
  `force=true` does not supply the full recovery parent-selection rule or the
  action-local exact-next timer-promptness rule.
- **Evidence record:** [EV-FINITE-REFERENCE-SPACE-TIMING](ASSUMPTION_EVIDENCE.md#ev-finite-reference-space-timing).
- **Discharge:** Implement the recovery design and complete every recovery
  `REF-*` mapping. Implement the fixed-reference wait, exact-next promptness,
  V2 round catch-up, and the remaining current-source mappings. Exact
  replay is not a required product refinement or liveness route.

## ASM-LIVE-LOCAL-PROPOSAL

- **Claim:** While the epoch is active, a correct, available validator with a valid current frontier target eventually stores and sends an own block, already has that round, or observes a higher threshold frontier. The one-shot forced timeout and the last-known-own-round and propagation-delay watchers supply the attempts. A local or synchronized commit cannot interrupt an active Core callback.
- **Type:** Single-validator Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Block-production and commit liveness.
- **Lean use:** The pure network-DAG proof applies this local rule after it
  derives the required parent and timer guards. That public DAG goal does not
  require per-validator production. The adopted commit proof also uses
  `ValidatorV2BlockProductionCurrentSourceMaps.blockProductionLiveness` to
  derive unbounded later own blocks before it applies finite no-skip catch-up.
- **Rust evidence:** The maximum timeout makes one forced attempt. A change to the recovered own round makes another forced attempt. A propagation-delay change makes another forced attempt when that blocker clears. A forced attempt bypasses leader presence and minimum-delay checks. It can still stop because the round is stale, the block already exists, the recovered own-round guard blocks a duplicate signature, or one of the two temporary blockers is active.
- **Evidence record:** [EV-NETWORK-ROUND-PROGRESS](ASSUMPTION_EVIDENCE.md#ev-network-round-progress).
- **Discharge:** Map each forced-return branch and the two watcher retries to `ValidatorNormalFrontierPacemakerRules`. Add tests for the stale, already-signed, recovered-round, and temporary-blocker cases.

## ASM-LIVE-CORE-HANDLER

- **Claim:** The running-Core trace starts after `recover_validator` finishes. Each qualifying external Core input handler then completes all commit work enabled by its finite batch and the finite blocks that GC cleanup unsuspends. In this positive DAG refinement, a qualifying handler is an `add_blocks` call that accepts at least one ordinary block. The local commit loop observes a terminal no-more-commits result. Core then invokes the normal proposal attempt before that handler returns. Core is single-threaded: a commit or block-accept action cannot interleave after the proposal callback starts.
- **Type:** Single-validator Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Block-production and commit liveness.
- **Lean use:** Trace time zero is the post-recovery running state; startup recovery is not an action source in this trace. `ValidatorPacketDrivenBlockAcceptanceAt` records exact past packet delivery and block acceptance. `packetDrivenAcceptanceHasInputOrigin` maps it to a qualifying `ValidatorCoreHandlerInputObservation`. `ValidatorCoreHandlerRefinementRules` then returns a `ValidatorFiniteCoreHandlerEpisode`. `qualifying_core_handler_input_has_terminal_scan_and_normal_proposal_attempt` exposes the terminal scan and later normal proposal attempt before return. `ValidatorFiniteCoreHandlerEpisode.proposal_attempt_input_and_suffix` connects the attempt input to the handler-exit state and the remaining finite event suffix. `qualifying_core_handler_input_has_current_proposal_continuation` then returns a same-batch proposal action or a `ValidatorCoreProposalContinuationAt` current-state witness. `ValidatorAuthorLocalPreAttemptScheduleAt` narrows commit interference to a protected normal callback, an armed exact recovery timer, or an occupied protected timer-arm goal, each before proposal latch. Raw parent readiness is not a schedule. The first-install rule returns exact past persistence or one protected post-install normal callback. These results do not return proposal success as an input, a common DAG layer, or a commit result.
- **Rust evidence:** On the v3 path, `try_commit_v3` loops until `FlexCommitter::try_commit` returns `None`. Each successful iteration calls `post_commit`, which can unsuspend blocks after GC changes. `Core::add_blocks` calls this loop and then `try_propose(false)` when the input accepts at least one block. The current action origin does not yet distinguish direct `add_blocks` input acceptance from later GC-unsuspension.
- **Discharge:** Map `ValidatorPacketDrivenBlockAcceptanceAt`, `packetDrivenAcceptanceHasInputOrigin`, `handlerInputOccurs`, `qualifyingInputHasFiniteHandler`, and `ValidatorCoreProposalAttemptContinuationRules` to the guarded ordinary `add_blocks` path. Add an exact direct-input acceptance origin. A GC-unsuspended block must use its enclosing handler continuation, not start another handler. Prove that finite input and finite GC-unsuspension give finite local commit work. Test the terminal no-more-commits result, the following proposal-attempt invocation, and its exact past or current continuation. Do not require that the attempt succeeds. Model certified-commit processing separately; do not use it as positive DAG progress.

## ASM-LIVE-LEADER

- **Claim:** The leader schedule and round leader selection satisfy `P_r <= S <= N`, `f + c < S`, and `A <= P_r`. For the same authenticated configuration, exact commit head, and effective schedule key, correct validators derive the same selected leader slot order. After a commit install, the proof checks the refreshed key. It keeps compatible facts when the allowed-leader list is unchanged and restarts schedule-dependent reasoning when the list changes.
- **Type:** Protocol and configuration refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Liveness.
- **Lean use:** Progress derives correct available scheduled stake and quorum coverage from these bounds. It compares proposal and Flex state only at one effective schedule key. A separate order rule derives a favorable leader window, and an action-scoped parent rule must include the exact first Flex leader.
- **Rust evidence:** Current v3 has `P_r = S`. Within one Core, the proposer and FlexCommitter receive the current `NextCommitLeaderSchedule`. Off-boundary commits retain `allowed_leaders`; a boundary commit can change it. FlexCommitter retains a compatible suffix or resets on a changed list. Startup does not enforce all stake bounds or the exact first-leader parent rule.
- **Evidence records:** [EV-SCHEDULE-HEAD-LOCAL](ASSUMPTION_EVIDENCE.md#ev-schedule-head-local) and [EV-FIRST-FLEX-LEADER-PARENT](ASSUMPTION_EVIDENCE.md#ev-first-flex-leader-parent).
- **Discharge:** Enforce actual-stake bounds, add the action-scoped proposal schedule read, prevent score exclusion of the accepted and retained exact first leader, and prove or accept the leader-order rule.

## ASM-LIVE-FIRST-SLOT-SAMPLING

- **Claim:** For each round, the round-seeded shuffle is modeled as an
  independent uniform permutation of the current allowed-leader list. The list
  is chosen before that round's shuffle. All correct validators use the same
  result for a round. The Byzantine and unavailable sets stay fixed during the
  proof interval.
- **Type:** Accepted probabilistic protocol model.
- **Status:** Accepted modeling assumption.
- **Effect if false:** Liveness.
- **Lean use:** Lean derives the probability-one causal-head favorable-window
  event from the abstract independent-uniform law. The conditional commit
  capstone then uses that event. The final entry theorem is
  `current_sources_give_end_to_end_liveness_probability_one`.
- **Rust evidence:** The product seeds the same deterministic shuffle from the
  round number. This gives validator agreement and crash-recovery
  reproducibility. The assumption treats the shuffle results for distinct round
  seeds as independent uniform pseudorandom permutations.
- **Boundary:** This is a pseudorandomness assumption. It is not a claim that the
  deterministic runtime uses fresh entropy. Lean uses only the first selected
  slot, so the proof also works with the weaker first-slot-only assumption. The
  first output word of the generator is not the proof boundary because the
  shuffle can consume more than one output word.
- **Evidence record:** [EV-FIRST-SLOT-PROBABILITY](ASSUMPTION_EVIDENCE.md#ev-first-slot-probability).
- **Discharge:** Keep the first-slot pseudorandomness model explicit. Pin the
  generator and shuffle algorithm if exact cross-version reproducibility is a
  protocol requirement, or replace the assumption with a proved deterministic
  coverage rule.

## ASM-LIVE-POST-GST-CAUSAL-SERVICE

- **Claim:** After GST, there is one fixed service interval and two finite bounds
  `C_add < C_service` for each correct, available validator. New rounds add at
  most `C_add` required above-GC references to its causal-work queue in one
  interval. If at least `C_service` items are pending, fetch, verification, and
  acceptance remove at least `C_service` items in that interval. If fewer items
  are pending, all pending items finish or become obsolete because GC moved.
  Thus, each fixed known above-GC causal history eventually becomes accepted
  while rounds can continue. The validator can skip authoring rounds, but it
  eventually stores and sends own blocks at later unbounded rounds. It does not
  have to author every intermediate round.
- **Type:** Accepted single-validator performance model.
- **Status:** Accepted modeling assumption.
- **Effect if false:** The completed conditional commit theorem does not apply
  to the product.
- **Lean use:** `BlockProductionLiveness` requires unbounded own-block
  production by each correct, available validator, not contiguous own rounds.
  `ValidatorV2BlockProductionCurrentSourceMaps.blockProductionLiveness` derives
  this result from selected support, recursive need, queue-source, and no-idle
  rules. The GC replacement and exact phase-continuity parts of these rules are
  proposed scheduler and refinement behavior. They are not derived from
  current Rust. V2 round catch-up then recovers the finite exact intermediate
  family. Fixed-reference pacing, pinned sync, retention, local Flex, and
  exact-prefix induction complete the Lean route.
- **Rust evidence:** Fetch, verification, parent-first acceptance, GC filtering,
  and batched block processing exist. Current evidence does not prove the
  strict sustained-rate comparison or unbounded per-validator authorship under
  continuous round advancement.
- **Evidence record:** [EV-POST-GST-CAUSAL-SERVICE](ASSUMPTION_EVIDENCE.md#ev-post-gst-causal-service).
- **Discharge:** Define measurable queue work and round-created work. Prove or
  enforce the strict service margin. Map the V2 selected support, recursive
  need, queue source, no-idle rule, and skipped-round proposal continuity.

## ASM-LIVE-BLOCK-SYNC

- **Claim:** After a correct, available validator receives an ordinary block body, its synchronizer fetches every missing causal-history block above the applicable cleanup round. Each fetch completes independently with bodies or an error. When a body is processed, the receiver uses its current GC round: it drops the body at or below GC, or discovers and fetches its missing above-GC parents. An error does not make an above-GC reference obsolete; normal retry can try it again. A commit does not rebase an in-flight fetch job.
- **Type:** Accepted single-validator synchronization model.
- **Status:** Accepted modeling assumption.
- **Effect if false:** Liveness.
- **Lean use:** Recursive direct-parent needs plus partial synchrony derive the
  finite above-cleanup causal closure of one received block. Parent-ready
  buffering then accepts it from parents to children. The finite reference-space
  theorem and the receiver's current GC cutoff give the pointwise visibility
  bound. The adopted final proof uses pinned sync and
  commit-orthogonal retention with fixed-reference pacing. It does not use
  commit synchronization as positive progress.
- **Rust evidence:** Several fetch mechanisms exist. The proof currently accepts their best-effort above-cleanup behavior. A complete source check is deferred.
- **Evidence record:** [EV-NETWORK-ROUND-PROGRESS](ASSUMPTION_EVIDENCE.md#ev-network-round-progress).
- **Discharge:** Later check peer choice, retries, buffering, restart recovery, cleanup handling, and exact-reference reads against Rust. Do not add a below-cleanup recovery path.

## ASM-LIVE-COMMIT-SYNC

- **Claim:** Commit sync is an optional acceleration path. Its traffic and local
  work do not starve ordinary block fetch, subscription retry, proposal work, or
  recovery timers. The final proof does not require commit-sync success. If an
  actual verified synchronized install advances the receiver's commit index,
  that install satisfies the receiver-progress branch. Otherwise, ordinary DAG
  synchronization, recovery proposals, local FlexCommitter execution, and the
  existing queue-service rules continue.
- **Type:** Accepted single-validator scheduling and resource-isolation model.
- **Status:** Accepted modeling assumption.
- **Effect if false:** Commit sync could prevent the ordinary path used by the
  liveness proof from running. Invalid synchronized installation would instead
  violate safety.
- **Lean use:** No commit-sync availability or result is a liveness input.
  `ValidatorReceiverCommitAdvance` is source-independent: a synchronized local
  advance closes the current receiver step, while its negation excludes every
  local commit-index advance on the analyzed suffix. Exact sync-install
  provenance restricts actual synchronized actions in the safety proof.
- **Rust evidence:** Subscription suspension uses a catch-up hysteresis band and
  checks local progress once per second. Subscription connection attempts retry
  with bounded exponential backoff. When commit lag suppresses periodic block
  sync, ten seconds without local commit-index movement enables periodic
  failover until one commit-sync batch of local progress occurs. Core verifies
  certified ranges, installs only a consecutive extension, accepts the exact
  commit blocks before GC moves, attempts a proposal, and signals a newer round.
  GC removes obsolete missing dependencies and unsuspends their children.
- **Limit:** Rust does not enforce a resource reservation that proves the
  accepted non-starvation claim. The failover and suspension loops also need the
  existing partial-synchrony, peer-fairness, task-fairness, block-sync, and
  queue-service assumptions to derive eventual progress. GC cleanup does not by
  itself prove the exact no-skip recovery target is re-armed; that is covered by
  the existing recovery no-idle and safe-resume refinements.
- **Evidence record:** [EV-COMMIT-SYNC-COVERAGE](ASSUMPTION_EVIDENCE.md#ev-commit-sync-coverage).
- **Discharge:** Keep verified sync installation mapped for safety. Keep the
  non-starvation rule explicit until scheduling and resource isolation enforce
  it. Do not add future sync availability, certification, vote carriage, or a
  synchronized commit result to liveness inputs.

## ASM-LIVE-PEER-FAIRNESS

- **Claim:** A correct validator retains and serves active local recovery data. A pending request retries authenticated validator peers in a fair order until the item arrives or becomes unnecessary. For ordinary blocks, this rule covers every recursively discovered missing causal-history reference above the applicable cleanup round.
- **Type:** Single-validator storage and synchronization refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness after missed state or restart.
- **Lean use:** The proof derives a correct data source, then combines local retry and service with post-stabilization delivery.
- **Rust evidence:** Storage and serving paths exist. Recovery-data retention and fair retry selection are incomplete.
- **Discharge:** Implement the local retention, service, and retry rules.

## ASM-LIVE-TASK-FAIRNESS

- **Claim:** A continuously enabled protocol task at a correct validator eventually runs.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** Every temporal progress chain needs enabled work to continue.
- **Rust evidence:** Tasks and queues implement the work but cannot guarantee scheduler fairness.
- **Discharge:** Define allowed shutdown and failure states in the runtime model.

## ASM-LIVE-LOCAL-RESPONSE

- **Claim:** Correct local clocks advance. A correct timer does not expire early, and every finite timer expires. Each covered local consensus action completes within a finite bound `epsilon`. An accepted commit timestamp has a bounded future offset.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Timely-vote liveness.
- **Lean use:** Recovery pacing uses a bound for proposal, storage, acceptance, and voting work.
- **Rust evidence:** The product has no end-to-end deadline for all covered local work.
- **Discharge:** Validate a deployment bound or weaken the timed result.

## ASM-LIVE-PIPELINE-BOUNDS

- **Claim:** Each timed protocol phase has a bound that includes its network, queue, storage, retry, and local processing delays.
- **Type:** Derived timing refinement.
- **Status:** Abstraction gap.
- **Effect if false:** Liveness bound.
- **Lean use:** Current stage composition accepts these bounds. The target proof derives each needed bound from messages and protected local actions.
- **Rust evidence:** Separate timers exist, but no complete event-to-bound mapping exists.
- **Discharge:** Define concrete phase events and derive or measure each complete bound.

## ASM-LIVE-FINALIZER-TRIGGER

- **Claim:** Every pending transaction eventually gets an eligible later trigger or a defined safe epoch-tail result.
- **Type:** Derived protocol and lifecycle theorem.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** Transaction progress needs a trigger after the target commit.
- **Rust evidence:** The modeled trigger path is absent, and shutdown can leave pending state.
- **Evidence record:** [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Implement the path and define the epoch-tail rule.

## ASM-LIVE-DURABILITY

- **Claim:** A decision becomes durable before exposure, survives restart, and eventually reaches its consumer.
- **Type:** Derived Rust and timing theorem.
- **Status:** Partially verified.
- **Effect if false:** Safety and liveness.
- **Lean use:** Transaction progress ends at durable output, not only an in-memory decision.
- **Rust evidence:** Durable-before-output patterns exist, but the modeled v3 path and complete restart result are open.
- **Evidence records:** [EV-DURABLE-COMMIT-PREFIX](ASSUMPTION_EVIDENCE.md#ev-durable-commit-prefix) and [EV-FINALIZER-TRIGGER-OUTPUT](ASSUMPTION_EVIDENCE.md#ev-finalizer-trigger-output).
- **Discharge:** Define the durable event, crash boundary, replay rule, and consumer-progress condition.
