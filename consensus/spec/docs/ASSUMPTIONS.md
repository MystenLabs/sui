<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 proof assumptions and product obligations

This ledger records the conditions that connect the formal model to the product
and its operating environment. A theorem is conditional even when the proof has
no declared axiom. Apply it to the product only when each related condition is
satisfied or explicitly accepted.

## Assumption boundary

A basic environment assumption describes one simple fact, such as bounded message
delivery, bounded local work, fair task execution, durable storage, a fault set,
or a probability distribution.

A quorum entering recovery, a sequence of block layers, a usable anchor, or a new
commit is not a basic assumption. The protocol proof must derive each such result.
The proof must show that each fixed product rule matches the model.

## Shared proof model

The safety and progress proofs share these conditions:

- Byzantine stake is at most `f`. Byzantine plus unavailable stake is at most
  `f + c`.
- Correct validators use one authenticated epoch configuration and one common
  commit chain, leader schedule, round leader selection, and selected leader slot
  order.
- Signatures bind all decision data. One validator counts at most once on each
  side of one decision.
- After network stabilization, messages between correct validators arrive within
  `delta`.
- Required local work finishes within `epsilon`, where `0 < epsilon < delta`.
- Correct local clocks continue to advance.
- Continuously enabled tasks at correct validators eventually run.
- Old-block cleanup keeps decision evidence until it is copied into the common
  committed prefix or the decision no longer needs it.
- Leader-schedule stake `S` satisfies `f + c < S`. Round-selection stake `P_r`
  satisfies `A <= P_r`. Current v3 has `P_r = S`.
- During one stalled commit index, each round's complete leader order follows the
  accepted independent uniform sampling model.

The local response bound and leader-order sampling model are less standard than
the other conditions. Useful-peer retention is required only when a lagging or
restarted validator needs old consensus data. Transaction payloads can be
submitted again.

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
| Partially verified | 7 |
| Environmental assumption | 5 |
| Open proof obligation | 3 |
| Abstraction gap | 2 |
| Accepted modeling assumption | 1 |
| Known mismatch | 6 |

A known mismatch blocks the affected product claim. Other open statuses identify
a condition; they do not indicate a failed proof inside the model.

## Maintenance rules

1. Keep each `ASM-*` and `REF-*` identifier stable.
2. Change a status only with proof, product, test, or environment evidence.
3. Do not turn a protocol result into a basic environment assumption.
4. Review all affected mappings after a product change.
5. Keep missing behavior separate from verified current behavior.

## Periodic Lean-to-Rust refinement review

This is the canonical source-to-model checklist. A verified row covers only its
stated behavior. It does not prove an end-to-end theorem.

Last complete source review: 2026-08-15, at `d630b4452a8`.

### Missing Rust behavior

#### Commit progress recovery

| Review ID | Required behavior or guarantee |
|---|---|
| `REF-RECOVERY-ENTRY` | Enter recovery from the commit timestamp only when the validator is caught up, own-block recovery is complete, and the epoch is active. Keep recovery keyed to the unchanged commit index. |
| `REF-NEXT-ROUND-TARGET` | Propose only one round after the highest known own proposal. A later observed round must not change this target. |
| `REF-RECOVERY-PACING` | Increase a schedule-independent delay while the commit index is unchanged. Round changes must not reset it. |
| `REF-RECOVERY-PARENTS` | Wait for immediate-parent quorum, request missing parents, include each unique immediate parent, ignore score exclusion there, and omit known equivocators. |
| `REF-RECOVERY-FRONTIER` | Connect every recovered own round to a durable signed block and the required parent history. The common maximum round remains a proof value. |
| `REF-RECOVERY-GC-FRONTIER` | Keep each recovery target above the old-block cleanup boundary or define a safe resume rule. |
| `REF-RECOVERY-LAYER-MAPPING` | Retain and identify all rounds in the complete recovery anchor window. |

#### Other features

| Review ID | Required behavior or guarantee |
|---|---|
| `REF-EPOCH-CONFIG` | Put all proof-relevant values in authenticated epoch state and reject incompatible values. |
| `REF-INTEGER-BOUNDS` | Set explicit numeric limits and use checked calculations for all modeled values. |
| `REF-V3-ACTIVATION` | Activate v3 from shared epoch state. |
| `REF-V3-TRANSACTION-PATH` | Implement the modeled v3 proposal, transaction-vote, cutoff, and finalization path. |
| `REF-AMNESIA-SIGNER-GUARD` | Prevent conflicting signatures after complete local consensus-state loss. |
| `REF-PARENT-SYNC` | Under the stated environment conditions, make each required block arrive or become unnecessary. Handle empty peer sets and fair retries. |
| `REF-COMMIT-SYNC-PROGRESS` | Under the stated environment conditions, make synchronization or live consensus extend the local commit stream. |
| `REF-COMMON-COMMIT-CHAIN` | Establish one index-and-digest commit chain across local production, synchronization, and restart. |
| `REF-GC-EVIDENCE` | Keep complete decision evidence across local production, synchronization, replay, restart, and transaction finalization. |
| `REF-LEADER-BOUNDS` | Enforce `f + c < S` and `A <= P_r` from actual epoch stake. |
| `REF-LEADER-ORDER-COMPATIBILITY` | Use a protocol-stable or version-gated schedule and round-order algorithm, with compatibility vectors. |
| `REF-FINALIZER-TAIL` | Define the result for pending finalizer state when an epoch ends before a later trigger. |
| `REF-ROUND-CATCHUP` | Add safe intermediate proposals if liveness for old leader blocks is required. |

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
| `REF-BLOCK-SYNC-MECHANISMS` | Direct, periodic, history, and stall-recovery block-fetch paths exist. Their existence does not prove progress. |
| `REF-COMMIT-SYNC-CHECKS` | Synchronized ranges are checked for indexes, digest links, block references, gaps, order, and quorum support on the range tip. Each commit does not have a separate certificate. |
| `REF-LEADER-SCHEDULE` | The same prefix and same fixed build and random-generator configuration produce the same ordered schedule and interval. |
| `REF-ROUND-LEADER-SELECTION` | Each stored pending v3 round contains the full schedule in one deterministic round order. Thus, `P_r = S`. |
| `REF-DIRECT-DECISION` | Direct selected-slot decisions use the modeled commit, skip, and undecided rules. |
| `REF-INDIRECT-DECISION` | Indirect selected-slot decisions use ordered, deduplicated certificate evidence. |
| `REF-PENDING-ROUNDS` | Pending rounds are consecutive. The scan visits each base that has a complete anchor window; newer stored rounds can supply anchor evidence only. |
| `REF-GC-BOUNDARY` | Local cleanup keeps above-boundary blocks. Pending commit state and finalizer state own copied evidence independently of the live cache. |
| `REF-FLEX-RESULT` | When the current decision scan finds a commit, the local commit state records it through the normal commit path. |

### Accepted model and environment

Independent uniform leader ordering is an accepted probability model. The product
uses a common deterministic round-based order for one fixed compatible build. The
model does not prove independent samples for that exact sequence.

Post-stabilization delivery, local response time, fair task execution, and a useful
remote data source are environment conditions. They are not missing local product
functions.

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

- **Claim:** Byzantine stake is at most `f`, and Byzantine plus unavailable stake is at most `f + c` after network stabilization.
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

- **Claim:** Product leader and transaction decisions use the same evidence, voter accounting, and result rules as the model. The signed cutoff is the maximum of the block-cleanup and vote-cleanup rounds.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Decision safety applies to the modeled evidence rules.
- **Rust evidence:** Leader decisions are reviewed; the modeled v3 transaction path is incomplete.
- **Discharge:** Add the transaction path and shared conformance vectors.

## ASM-SAFE-COMMIT-CHAIN

- **Claim:** Correct validators process one common, continuous index-and-digest commit chain in order.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Indirect decisions use one ordered commit stream.
- **Rust evidence:** Local and synchronized inputs have chain checks, but no cross-validator proof exists.
- **Discharge:** Prove the common-chain result across local production, synchronization, and restart.

## ASM-SAFE-FIRST-TRIGGER

- **Claim:** Correct validators use the same first eligible depth-two commit as the transaction trigger.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Indirect transaction agreement depends on one first trigger.
- **Rust evidence:** Ordered processing supports this rule when the common-chain claim holds.
- **Discharge:** Derive trigger equality from the common chain and verify all input paths.

## ASM-SAFE-COMMITTED-PREFIX

- **Claim:** Before the first trigger, the committed prefix contains every decision witness required by the indirect rule.
- **Type:** Protocol and Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety.
- **Lean use:** Direct-against-indirect safety needs the missing witness in the prefix.
- **Rust evidence:** Local and synchronized paths construct prefixes, but complete inclusion is not proved.
- **Discharge:** Prove witness inclusion for local, synchronization, replay, and restart paths.

## ASM-SAFE-GC

- **Claim:** Old-block cleanup uses the preceding commit boundary. It retains evidence until the decision rule no longer needs it or the evidence is copied into the committed prefix.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** Leader and transaction safety use retained anchor and vote evidence.
- **Rust evidence:** Local ownership is verified, but the required depth and end-to-end evidence path are not complete.
- **Discharge:** Enforce the required depth and close the complete evidence mapping.

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

## ASM-LIVE-ROUND-CATCHUP

- **Claim:** When old-leader liveness is required, a round jump creates every still-required intermediate proposal before a later proposal.
- **Type:** Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness for old leader blocks.
- **Lean use:** The stronger liveness proof uses safe intermediate proposals.
- **Rust evidence:** A local round can jump without these proposals.
- **Discharge:** Implement the stronger rule only if this property is required.

## ASM-LIVE-COMMIT-PROGRESS-RECOVERY

- **Claim:** A caught-up stalled validator enters recovery, proposes only its next own round, uses growing pacing, requires and synchronizes parents, and exits only on defined progress or lifecycle events.
- **Type:** Protocol and Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** The recovery proof derives common layers, anchors, and commit-index progress from these local rules.
- **Rust evidence:** The current product has no commit progress recovery mode.
- **Discharge:** Implement the recovery design and complete every recovery `REF-*` mapping.

## ASM-LIVE-LEADER

- **Claim:** The leader schedule and round leader selection satisfy `P_r <= S <= N`, `f + c < S`, and `A <= P_r`; selection also supplies usable leader opportunities.
- **Type:** Protocol and configuration refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Liveness.
- **Lean use:** Progress needs correct available scheduled stake and quorum coverage.
- **Rust evidence:** Current v3 has `P_r = S`, but startup does not enforce all bounds or anchor opportunity.
- **Discharge:** Enforce actual-stake bounds and prove or accept the leader-order rule.

## ASM-LIVE-FIRST-SLOT-SAMPLING

- **Claim:** During one stall, each pending round has one common independent uniform leader order over a stable schedule and non-progress set.
- **Type:** Accepted probabilistic protocol model and Rust refinement.
- **Status:** Accepted modeling assumption.
- **Effect if false:** Liveness.
- **Lean use:** The trace proof accepts eventual favorable consecutive first slots.
- **Rust evidence:** The product uses a deterministic round-based shuffle, not independent random samples.
- **Discharge:** Keep the probability model explicit or replace it with a proved deterministic coverage rule.

## ASM-LIVE-BLOCK-SYNC

- **Claim:** Each required missing block is eventually accepted, or verified commit synchronization makes it unnecessary.
- **Type:** Derived Rust progress theorem.
- **Status:** Abstraction gap.
- **Effect if false:** Liveness.
- **Lean use:** Consensus and recovery proofs require eventual parent availability.
- **Rust evidence:** Several fetch mechanisms exist, but end-to-end progress is not proved.
- **Discharge:** Model retention, peer choice, retries, acceptance, cleanup, and commit advancement.

## ASM-LIVE-COMMIT-SYNC

- **Claim:** Each required missing commit is eventually installed, or the live block path reproduces progress.
- **Type:** Derived Rust progress theorem.
- **Status:** Partially verified.
- **Effect if false:** Liveness.
- **Lean use:** Recovery and finalizer progress require a continuous commit stream.
- **Rust evidence:** Range checks and retries exist; trailing batches and backpressure remain open.
- **Discharge:** Prove eventual stream extension under the peer, task, and consumer conditions.

## ASM-LIVE-PEER-FAIRNESS

- **Claim:** When old consensus data is needed, a reachable correct peer retains and serves each required item.
- **Type:** Network, storage, and peer-availability environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness after missed state or restart.
- **Lean use:** Open synchronization results use this condition only for old data.
- **Rust evidence:** The product cannot ensure remote retention or reachability. Fair retry selection is a derived product result.
- **Discharge:** State the retention, reachability, and response contract.

## ASM-LIVE-TASK-FAIRNESS

- **Claim:** A continuously enabled protocol task at a correct validator eventually runs.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** Every temporal progress chain needs enabled work to continue.
- **Rust evidence:** Tasks and queues implement the work but cannot guarantee scheduler fairness.
- **Discharge:** Define allowed shutdown and failure states in the runtime model.

## ASM-LIVE-LOCAL-RESPONSE

- **Claim:** Correct local clocks advance, and each covered local consensus action completes within `epsilon`, where `0 < epsilon < delta`.
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
- **Lean use:** Bounded progress composes phase bounds into a complete result.
- **Rust evidence:** Separate timers exist, but no complete event-to-bound mapping exists.
- **Discharge:** Define concrete phase events and derive or measure each complete bound.

## ASM-LIVE-FINALIZER-TRIGGER

- **Claim:** Every pending transaction eventually gets an eligible later trigger or a defined safe epoch-tail result.
- **Type:** Derived protocol and lifecycle theorem.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** Transaction progress needs a trigger after the target commit.
- **Rust evidence:** The modeled trigger path is absent, and shutdown can leave pending state.
- **Discharge:** Implement the path and define the epoch-tail rule.

## ASM-LIVE-DURABILITY

- **Claim:** A decision becomes durable before exposure, survives restart, and eventually reaches its consumer.
- **Type:** Derived Rust and timing theorem.
- **Status:** Partially verified.
- **Effect if false:** Safety and liveness.
- **Lean use:** Transaction progress ends at durable output, not only an in-memory decision.
- **Rust evidence:** Durable-before-output patterns exist, but the modeled v3 path and complete restart result are open.
- **Discharge:** Define the durable event, crash boundary, replay rule, and consumer-progress condition.
