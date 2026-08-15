<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 proof assumptions and refinement obligations

This ledger records the assumptions that connect the Lean model to the current
Mysticeti v3 Rust implementation and its operating environment.

Lean checks each theorem from its stated inputs. An input to an assumption
structure is not a declared Lean `axiom`, but the theorem is still conditional on
that input. Apply a theorem to Rust only when each proof obligation is discharged or
each environmental assumption is explicitly accepted.

## Assumption boundary

A primitive liveness assumption must describe one simple boundary:

- one authenticated message is delivered after GST;
- one continuously enabled task at a correct validator eventually runs;
- one local consensus action that stays enabled at a correct validator completes
  within a stated post-GST bound;
- one correct local clock continues to advance;
- one durable write remains available after restart;
- one stated set of validators is Byzantine, crashed, or available;
- one random source has a stated distribution and independence rule.

A deterministic Rust function or state transition is a refinement theorem target.
It is not an environmental assumption. A quorum entering recovery, consecutive
quorum block layers, a usable anchor, and a greater commit index are protocol
results. The proof must derive them. It must not accept them as primitive
assumptions.

## Shared proof model

This is one global assumption catalog for the safety and liveness proofs. Commit
progress recovery does not own these assumptions. Each theorem uses only the
conditions that apply to its result.

| Shared condition | Main use | Classification |
|---|---|---|
| Byzantine stake is at most `f`. Byzantine plus crashed or otherwise non-progressing stake is at most `f + c`, and a stable correct set remains active. | Safety and liveness | Standard fault bound with a protocol-specific crash budget. |
| Correct validators use the same authenticated epoch configuration. They derive the same leader schedule, round leader selection, and selected leader slot order from one common committed prefix. | Safety and liveness | Standard agreement requirement and Rust refinement target. The current node-local threshold overrides do not enforce this condition. |
| Signatures bind all modeled vote fields. One validator identity counts at most once on each side of one decision. | Safety | Standard authentication and vote-accounting requirement. |
| After GST, each protocol message between correct validators arrives within `delta`. | Liveness | Standard partial synchrony. |
| Each covered local consensus action completes within a positive bound `epsilon`, with `epsilon < delta`. | Bounded liveness and recovery timing | Additional processor-synchrony assumption. Message partial synchrony does not imply it. |
| Each continuously enabled protocol task at a correct validator eventually runs. | Liveness | Standard weak task fairness. A finite `epsilon` already gives a stronger result for the actions that it covers. |
| Evidence used by a decision is in the common commit chain or retained committed prefix before live DAG GC removes it. | Safety and liveness | Protocol and Rust refinement target, not an environment assumption. |
| The leader schedule has stake `S` with `f + c < S`. Each applicable round leader selection has stake `P_r` with `A <= P_r`. Current v3 has `P_r = S`. | Leader-based liveness | Protocol configuration conditions. They are not network assumptions and do not by themselves prove an anchor. |
| While one commit index is stalled, each pending round has one common leader-slot order that is modeled as an independent uniform permutation of the leader schedule. | Leader-based liveness | Accepted probabilistic model of the deterministic seeded shuffle. The current proof uses the first slot from each permutation. |

The Byzantine fault bound, authentication, post-GST delivery, and weak task
fairness are conventional assumptions. The separate crash budget `c` is specific to
the v3 hybrid threshold model, but it is not an unusual proof technique.

The less standard assumptions are the finite local-processing bound and independent
uniform leader-order sampling. The schedule and selection stake bounds should be
enforced or derived from epoch state. They should not remain deployment
assumptions.

Useful-peer retention is not a base assumption for steady-state consensus. It is a
conditional requirement for block sync and commit sync after a validator misses
old consensus data or restarts. A correct peer must then supply the required block
or commit, or verified commit sync must move the validator to a state that no
longer needs it. This requirement does not apply to transaction payloads. A
validator or user can resubmit a transaction.

The safety proof also needs the common commit-chain, evidence-refinement,
first-trigger, and GC conditions below. The liveness proof also needs derived block
sync, commit sync, pipeline, finalizer-trigger, and durability results. These are
proof or refinement goals. They are not additional primitive environment
assumptions.

## Status values

- **Discharged in Lean**: Lean proves the complete claim inside the model.
- **Enforced in Rust**: the current Rust path rejects or prevents a violation.
- **Partially verified**: code checks or tests cover part of the claim, but no
  complete refinement proof exists.
- **Abstraction gap**: the Lean predicate does not yet map to exact Rust events or
  time.
- **Accepted modeling assumption**: the proof intentionally uses the stated model
  for a protocol mechanism.
- **Environmental assumption**: the claim is an operating, network, or adversary
  assumption outside the local decision function.
- **Open proof obligation**: the proof needs the claim, but current evidence does
  not establish it.
- **Known mismatch**: the current implementation contradicts the claim or does not
  implement a required rule.

A `Known mismatch` blocks the affected implementation claim. An `Abstraction gap`,
`Environmental assumption`, `Open proof obligation`, or `Partially verified` status
identifies a remaining condition. It is not a proof failure inside Lean.

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

The six known mismatches block an end-to-end claim for the current implementation. The
other open conditions define the remaining refinement and environment boundary.

## Maintenance rules

1. Keep each `ASM-*` identifier stable. Do not reuse an old identifier for a new
   claim.
2. Reference the identifier in the Lean comment next to the related model input.
3. Reference the identifier from each implementation-gap item that can close it.
4. Change a status only with code, a proof, a test, or stated environment evidence.
5. Recheck all implementation evidence after relevant Rust changes.
6. Do not add a protocol result as a primitive assumption. Put an unproved result
   under an open proof obligation and list the simple contracts that must derive it.
7. Give each deterministic Rust subclaim one of two results: **Missing Rust
   behavior** or **Verified current Rust behavior**. Keep environmental assumptions
   and accepted models in separate sections.

## Periodic Lean-to-Rust refinement review

This section is the canonical list of Lean concepts that need a Rust mapping. It
does not replace the detailed `ASM-*` entries. A verified item has a source trace
and focused test evidence at the reviewed revision. This result does not mean that
Lean reads or verifies Rust source.

Last complete source review: 2026-08-15, against the `consensus/` tree at
`d630b4452a8`.

### Missing Rust behavior

#### Commit progress recovery

| Review ID | Missing behavior or guarantee | Current evidence |
|---|---|---|
| `REF-RECOVERY-ENTRY` | Start commit progress recovery from the current `DagState` commit timestamp when the stall timeout expires, the validator is not behind the observed quorum commit index, own-round recovery is complete, and the epoch is active. After restart, use the last flushed commit timestamp. Before the first commit, use the epoch start. Keep eligibility and pacing keyed to the unchanged commit index. Do not make recovery proposals while the observed quorum commit index is ahead. End recovery when the epoch stops. | Rust has no proposer recovery mode or stall trigger. The existing synchronizer stall timer is only a block-sync fallback for a commit-lagging validator. |
| `REF-NEXT-ROUND-TARGET` | In recovery, propose only at `highestKnownOwnProposalRound + 1`. A threshold-clock jump must not change this target or reset recovery pacing. | The current proposer uses the threshold-clock round. |
| `REF-RECOVERY-PACING` | Use a schedule-independent recovery delay that grows without a fixed limit while the commit index is unchanged. Define one local event that starts each delay. | Current leader timers are fixed for one round and reset when the threshold clock changes. |
| `REF-RECOVERY-PARENTS` | Wait for immediate-parent quorum, request missing parents, disable score-based exclusion for the immediate parent round, and include every timely, locally unique parent block. Include no block from a validator when the local DAG knows an equivocation. | General block sync exists, but recovery attempts do not start it for their target round. Smart ancestor selection can omit a score-excluded parent after it reaches quorum. The current normal proposer can still include one primary block for a known equivocator. |
| `REF-RECOVERY-FRONTIER` | Map each recovery validator's highest known own round to a persisted signed block and the finite causal-parent interval needed to reach the proof-selected maximum round. | Lean derives the maximum. Rust must not select or announce it. The current peer recovery path returns only a round and does not import the block or its causal history. |
| `REF-RECOVERY-GC-FRONTIER` | Keep each exact-next target above GC, or define a safe resume rule when the highest own round is at or below GC. | No commit progress recovery legal-frontier rule exists. |
| `REF-RECOVERY-LAYER-MAPPING` | Map recovery proposals and accepted blocks to retained pending rounds for the complete anchor window. | The recovery mode does not exist, so this state mapping cannot yet be established. |

#### Other features

| Review ID | Feature | Missing behavior or guarantee | Current evidence |
|---|---|---|---|
| `REF-EPOCH-CONFIG` | Epoch configuration | Put `f`, `c`, v3 activation, leader schedule parameters, GC depth, and all proof-relevant values in common authenticated epoch state. Reject incompatible values. | `f` and `c` come from process-local environment variables. Several v3 values are hard-coded during startup. See `ASM-SAFE-PARAMETERS`. |
| `REF-INTEGER-BOUNDS` | Integer refinement | Use checked arithmetic and explicit limits for threshold calculations, rounds, commit indexes, transaction indexes, schedule counters, and collection-size conversions. | Lean uses natural numbers. Rust uses bounded integer types, and not all operations have checked bounds. |
| `REF-V3-ACTIVATION` | V3 activation | Activate v3 from epoch protocol state. | Normal startup sets `enable_v3` to `false`. |
| `REF-V3-TRANSACTION-PATH` | Transaction voting | Produce `BlockV3`, enforce `enable_v3` implies transaction voting, and implement the modeled v3 transaction finalizer. | Generic signatures cover `BlockV3`, but the proposer produces only `BlockV1` or `BlockV2`, and the modeled finalizer is absent. |
| `REF-AMNESIA-SIGNER-GUARD` | Restart safety | Prevent reuse of one epoch signing key after all local consensus storage is lost. | Peer recovery is a useful availability mechanism, but it cannot find a signed block that no correct peer retained. |
| `REF-PARENT-SYNC` | Synchronization | Under the stated useful-peer, network, and task-fairness conditions, prove that each required causal block is eventually accepted, or commit sync advances state so that the block is no longer needed. Handle an empty peer set without a panic and give retry selection a fair rule. | Direct, periodic, and commit-stall block-sync paths exist. The end-to-end progress result is not established. See `ASM-LIVE-BLOCK-SYNC`. |
| `REF-COMMIT-SYNC-PROGRESS` | Synchronization | Under the stated peer, retention, network, task, and consumer conditions, prove that commit sync or the live block path eventually extends the local commit stream. | Commit-sync checks and retry mechanisms exist. The progress theorem, trailing partial batch, and backpressure cases remain open. See `ASM-LIVE-COMMIT-SYNC`. |
| `REF-COMMON-COMMIT-CHAIN` | Commit safety | Establish that all correct validators refine one common index-and-digest commit chain across local commit, commit sync, and restart. | Local and synced chain checks exist. The cross-validator refinement theorem is still open. See `ASM-SAFE-COMMIT-CHAIN`. |
| `REF-GC-EVIDENCE` | Garbage collection and finalization | Enforce the required v3 GC depth and establish complete decision-evidence retention across local commit, commit sync, replay, restart, and transaction finalization. | Local block retention is verified below. The current tree has no modeled v3 transaction path, and the complete prefix theorem is open. See `ASM-SAFE-GC`. |
| `REF-LEADER-BOUNDS` | Leader liveness | Enforce `f + c < S` and `A <= P_r` from actual epoch weights. | Current v3 uses `P_r = S`, but startup does not enforce these liveness bounds. See `ASM-LIVE-LEADER`. |
| `REF-LEADER-ORDER-COMPATIBILITY` | Leader order | Bind a protocol-stable or version-gated schedule tie-order and round slot-order algorithm, and add compatibility vectors. | Both orders depend on `StdRng`. Their output is deterministic for one fixed build, but it is not a stable protocol algorithm across build configurations or dependency versions. |
| `REF-FINALIZER-TAIL` | Epoch lifecycle | Define and implement the result for pending finalizer state when an epoch stops before a later trigger exists. | Shutdown can end the epoch with a pending transaction result. See `ASM-LIVE-FINALIZER-TRIGGER`. |
| `REF-ROUND-CATCHUP` | Liveness for old leader blocks | Implement the stronger safe intermediate-round proposal rule if this stronger liveness property is required. | The current threshold clock can jump across proposal rounds. Commit progress recovery proves only commit-index progress. See `ASM-LIVE-ROUND-CATCHUP`. |

### Verified current Rust behavior

| Review ID | Lean concept | Verified Rust behavior |
|---|---|---|
| `REF-THRESHOLD-CONSTRUCTION` | Actual `N`, scaled `f` and `c`, `Q`, `A`, and validity threshold | For non-overflowing inputs, `Committee::new_v3` computes the configured thresholds and checks both Lean safety inequalities. |
| `REF-AUTHENTICATION` | One authenticated validator identity signs one complete modeled block | `SignedBlock` signs the hash of the complete serialized block. Block verification uses the named validator's protocol key. Validator transport uses committee-key mutual TLS. |
| `REF-VOTE-DEDUP` | One validator identity counts once in one voter set | `StakeAggregator`, immediate-parent checks, leader decisions, transaction vote tracking, and commit certificates deduplicate by `AuthorityIndex`. |
| `REF-COMMIT-STATE` | Local commit index, commit reference, and protocol timestamp | These values come from one `DagState` trusted commit. `DagState::flush` persists the complete commit, and restart reloads it. The separate local `Instant` for commit advancement is not persisted and is not this value. |
| `REF-OWN-PROPOSAL-ROUND` | Highest known own proposal round during normal restart and peer-assisted recovery | Durable restart restores the latest own block and rejects proposal rounds at or below it. Empty-store recovery disables proposal until verified peer replies from validity-threshold stake establish a round floor. The strict total-amnesia guarantee is the separate missing item above. |
| `REF-DURABLE-PROPOSAL` | A completed proposal is durable before broadcast | The proposer accepts and flushes its own block before it returns. Core broadcasts the block only after that return. |
| `REF-PARENT-QUORUM` | Immediate-parent quorum for an ordinary verified block | `BlockVerifier` counts distinct round-`R - 1` parent validators and rejects a block below quorum. |
| `REF-BLOCK-PARENT-ACCEPTANCE` | Parent acceptance in the ordinary live block path | `BlockManager` suspends an above-GC child until all listed above-GC parents are accepted. Certified commits use a separate verified explicit-block path. |
| `REF-BLOCK-SYNC-MECHANISMS` | Missing-block recovery mechanisms | Rust has direct fetch, periodic fetch, stored-history fetch, and commit-stall fallback paths. This does not by itself prove eventual progress. |
| `REF-COMMIT-SYNC-CHECKS` | Certified commit input checks | Commit sync checks start index, consecutive indexes, digest links, distinct quorum votes on the range tip, explicit block references, gap buffering, and ordered Core delivery. |
| `REF-LEADER-SCHEDULE` | Leader schedule membership, order, and interval from fixed inputs | Given the same epoch inputs, committed prefix, and fixed build, RNG implementation, and RNG configuration, live processing and replay derive the same ordered schedule. Membership changes only at commit-index schedule boundaries. The next commit index and configured update interval identify the current schedule interval; Rust has no separate version field. |
| `REF-ROUND-LEADER-SELECTION` | Round leader selection and selected leader slot order | Each stored pending round has one slot for every schedule member. Rust applies one deterministic round-seeded permutation and keeps status updates in that order. Current v3 therefore has `P_r = S`. |
| `REF-DIRECT-DECISION` | Direct selected leader slot result | `LeaderSlotDecider::try_direct_decide` matches the modeled commit, skip, and undecided cases, including voter deduplication. |
| `REF-INDIRECT-DECISION` | Indirect selected leader slot results | `LeaderSlotDecider::try_indirect_decide` scans the anchor history, forms certification-threshold evidence with unique validator stake, and returns one ordered final result for each input slot. |
| `REF-PENDING-ROUNDS` | Pending-round range and FlexCommitter scan | Pending rounds are contiguous. A complete anchor window base is in the indirect-decision scan; newer stored rounds can be anchor evidence only. The ordered anchor scan and commit-round scan match the executable Lean functions. |
| `REF-GC-BOUNDARY` | Local block GC boundary and retention | `gc_round` uses the preceding commit. Cache eviction does not remove blocks above GC. Local v3 commit construction copies its selected sub-DAG before Core records the new commit. The finalizer owns its pending `CommittedSubDag` values and block map, so live DAG cache eviction does not remove them. |
| `REF-FLEX-RESULT` | Record a commit found by the executable scan | `FlexCommitter::try_commit` runs direct decisions, the descending indirect scan, and commit construction. `Core::try_commit_v3` passes each result directly to `Core::post_commit`, which adds the commit to `DagState`. |

### Accepted model and environment

The independent uniform first-slot interpretation is an accepted probability
model. Rust implements a common deterministic round-seeded permutation for a fixed
build; it does not sample independent random seeds. Post-GST delivery, local
response bound `epsilon`, weak task fairness, and useful remote data sources are
environmental conditions. They are not missing local Rust functions.

### Review evidence

The 2026-08-15 review traced the named Rust functions and ran focused tests:

- v3 committee construction: 4 tests passed;
- block signature, block verifier, TLS allowlist, and vote-deduplication tests
  passed;
- leader schedule: 24 tests passed;
- FlexCommitter: 27 tests passed;
- LeaderSlotDecider: 18 tests passed;
- local v3 commit recording: 1 test passed;
- durable restart and own-round recovery: 12 focused tests passed;
- block sync, commit sync, parent handling, and GC: 10 focused tests passed.

The review did not use `test_core_recover_from_store_v3` as finalizer-recovery
evidence. The detached finalizer panics because vote information is missing, but
the test process exits with status 0. A nonzero-GC restart test for a pending
decision window is still needed.

For each periodic review:

1. Record the reviewed `consensus/` source revision and date.
2. Trace each listed Rust symbol and run its focused tests.
3. Update the current state only when code or test evidence changes.
4. Update Lean and the related `ASM-*` entry when a Rust meaning changes.
5. Treat a renamed or removed Rust symbol as a failed review until its replacement is
   mapped.

## ASM-MATH-THRESHOLDS

- **Claim:** For `N = 5f + 3c + 1`, `Q = 4f + 2c + 1`, and
  `A = 2f + c + 1`, the model has `0 < A`, `N + f < Q + A`, and
  `N + f + A <= 2Q`.
- **Type:** Mathematical.
- **Status:** Discharged in Lean.
- **Effect if false:** Safety.
- **Lean use:** [`Thresholds.nominalHybrid`](../lean/Mysticeti/Thresholds.lean)
  constructs the required inequalities.
- **Rust evidence:** `Committee::new_v3` uses these formulas only as the nominal
  case. It scales `f` and `c` to the actual validator set stake, sets
  `A = 2f + c + 1`, and sets `Q = N - f - c`. It then checks the two safety
  inequalities. Actual `N` does not always equal `5f + 3c + 1`.
- **Discharge:** No Lean work is required. Close the Rust mapping through
  `ASM-SAFE-PARAMETERS` and `ASM-REFINE-INTEGERS`.

## ASM-SAFE-PARAMETERS

- **Claim:** All correct nodes in one epoch use the same validator set, `N`, `f`,
  `Q`, `A`, validity threshold, `gc_depth`, and v3 leader configuration. From one
  common committed prefix, they derive the same leader schedule membership, round
  leader selection, and selected leader slot order.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** Every weighted quorum theorem uses one `Thresholds` value and one
  stake function.
- **Rust evidence:** `apply_v3_threshold_overrides` reads `f` and `c` from local
  process environment variables. Correct nodes can therefore derive different
  values.
- **Discharge:** Put all threshold inputs in authenticated epoch protocol state. Add
  all proof-relevant configuration values to the same state. Prove deterministic
  schedule and selection derivation from the common commit chain. Add
  mixed-configuration rejection tests.

## ASM-SAFE-FAULT-BOUND

- **Claim:** Byzantine validator stake is at most the configured `f` value. After
  GST, Byzantine plus crashed or otherwise non-progressing validator stake is at
  most `f + c`. A stable set of correct, non-crashed validators remains active.
- **Type:** Adversary and availability model.
- **Status:** Environmental assumption.
- **Effect if false:** Safety for the `f` bound; liveness for the `f + c` bound.
- **Lean use:** [`FaultBounded`](../lean/Mysticeti/Thresholds.lean) bounds the
  intersection of incompatible voter sets. The commit progress recovery view uses
  `nonProgress` for the union of Byzantine and crashed or unavailable validators.
- **Rust evidence:** The implementation cannot identify all Byzantine authorities
  while the protocol runs.
- **Discharge:** State both bounds in the protocol threat model. Ensure that epoch
  configuration does not claim smaller `f` or `c` values than the supported
  deployment model.

## ASM-SAFE-AUTHENTICATION

- **Claim:** A verified signature binds the authority, epoch, round, block contents,
  transaction cutoff, transaction votes, and commit votes that the model uses.
- **Type:** Rust refinement.
- **Status:** Enforced in Rust.
- **Effect if false:** Safety.
- **Lean use:** Voter-set membership assumes that one authenticated block supplies
  the modeled vote for its named authority.
- **Rust evidence:** The
  [Tonic validator network](../../core/src/network/tonic_network.rs) uses mutual
  TLS. The server accepts validator certificates whose network public keys are in
  the committee, and the client verifies the remote validator's committee network
  key. TLS identifies the connection peer. Consensus vote attribution uses a
  separate protocol signature. [`SignedBlock`](../../core/src/block.rs) serializes
  and hashes the complete `Block`, signs the digest with the author's protocol key,
  and verifies it with that author's committee protocol key before Core accepts the
  block. Thus, the signature covers every serialized block field, including v3
  transaction cutoffs and votes. Tests reject a wrong author, wrong key, and
  malformed signature.
- **Discharge:** Keep the TLS allowlist, complete-block signature, author-key check,
  and negative signature tests. Lean treats signature unforgeability as a
  cryptographic primitive and does not model TLS or Ed25519 internals.

## ASM-SAFE-NON-EQUIVOCATION

- **Claim:** A correct validator contributes to only one side of each leader-slot or
  transaction decision. Byzantine equivocation can count once on each side, but one
  validator identity cannot count more than once on either side.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** `OnlyFaultyOverlap` is used by `LeaderEvidence` and
  `TransactionEvidence`.
- **Rust evidence:** Vote aggregation uses validator identity, and the proposer
  persists its own block state before Core broadcasts the block.
  [`StakeAggregator`](../../core/src/stake_aggregator.rs) stores validator
  identities in a `BTreeSet`; `add` and `add_unique` count one validator's stake
  only once. The leader decision code, transaction vote tracker, current finalizer,
  and commit sync use this aggregator. The block verifier also rejects two parent
  references from the same validator. Thus, vote deduplication is enforced.

  The normal crash-and-restart path also prevents two published own blocks for one
  round when the consensus store remains durable. `ValidatorProposer` adds the new
  block to `DagState` and flushes it before Core broadcasts it. `DagState` rejects a
  second uncommitted block in the validator's own slot. After restart,
  `DagState::new` loads the latest stored own block, and the proposer rejects a
  proposal round that is not greater than its recovered or synchronized highest
  own proposal round.

  Empty-store amnesia recovery has a smaller guarantee. The proposer stays disabled
  until the synchronizer gets valid replies from at least validity-threshold stake
  and reports the highest own round in those replies. Unit, authority, and
  simulation tests cover the successful recovery path. This peer query is a
  best-effort recovery mechanism. It cannot prove that it found a signed block that
  only a Byzantine peer retained. Therefore, the strict invariant across loss of
  all durable signer state is not yet enforced.
- **Discharge:** Treat durable signer state as part of a correct validator. If a
  validator can lose its consensus store and reuse the same epoch signing key, keep
  its highest signed proposal round in separate durable signer state, or prevent it
  from signing again in that epoch. Keep the current peer-sync recovery and tests as
  an availability aid. Keep equivocation conformance vectors for one vote on each
  side.

## ASM-SAFE-PARENT-QUORUM

- **Claim:** Every accepted non-genesis block has at least `Q` stake in verified
  immediate parents.
- **Type:** Rust refinement.
- **Status:** Enforced in Rust.
- **Effect if false:** Safety.
- **Lean use:** The direct-versus-indirect argument needs an anchor quorum.
- **Rust evidence:** The block verifier checks immediate-parent quorum stake.
- **Discharge:** Keep a boundary test that uses the same configured `Q` as the Lean
  threshold model. Parameter agreement remains under `ASM-SAFE-PARAMETERS`.

## ASM-SAFE-EVIDENCE-REFINEMENT

- **Claim:** The Rust leader and transaction decision functions create the same
  voter sets, signed vote witnesses, cutoffs, unique-authority counts, and outcomes
  as the Lean evidence relations. A v3 proposer sets
  `transaction_votes_cutoff_round` to the maximum of its causal-history GC round
  and transaction vote-tracker GC round.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`LeaderEvidence`](../lean/Mysticeti/Leader.lean),
  [`TransactionVote`](../lean/Mysticeti/Finalizer.lean),
  [`TransactionVoteProduction`](../lean/Mysticeti/Finalizer.lean), and
  [`TransactionEvidence`](../lean/Mysticeti/Finalizer.lean). A counted direct
  accept has a signed witness that passes the numeric cutoff classifier.
- **Rust evidence:** Focused Rust tests cover the current leader decisions and the
  current transaction-voting path. The current proposer does not create `BlockV3`,
  and the current tree does not contain the modeled v3 transaction finalizer.
  Therefore, the v3 cutoff and transaction-decision mapping is not implemented.
  The Lean model is hand written and has no executable conformance suite.
- **Discharge:** Add shared decision vectors for leader decisions, transaction
  decisions, cutoffs, first-trigger selection, and equivocation.

## ASM-SAFE-COMMIT-CHAIN

- **Claim:** Correct nodes process one common commit chain with no index gap and
  with each `previous_digest` equal to the preceding commit digest. A commit can be
  produced by the local commit rule or installed through commit sync.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`CommitStream`](../lean/Mysticeti/CommitChain.lean) represents one
  common stream. Its current `Continuous` predicate checks indices only.
- **Rust evidence:** `FlexCommitter::build_commit` produces local commits. Commit
  sync verifies the fetched index and digest chain, buffers gaps, and verifies
  quorum commit votes before Core installs the commits. The v3 Core path checks the
  local digest boundary. The finalizer checks consecutive indices.
- **Discharge:** Extend the Lean commit model with digests and previous digests.
  Prove that local commit production, commit-sync installation, replay, and recovery
  refine the same commit chain. Prove verification of quorum commit votes for the
  synced range tip as a condition of commit-sync installation, not as a condition
  of every commit.

## ASM-SAFE-FIRST-TRIGGER

- **Claim:** Correct nodes use the same first eligible depth-two commit as the
  indirect decision trigger for one target.
- **Type:** Protocol and Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`first_trigger_agreement`](../lean/Mysticeti/CommitChain.lean) uses
  two views of the same stream and the same first-eligible rule.
  [`firstEligible_predecessor`](../lean/Mysticeti/CommitChain.lean) proves that the
  commit before this trigger is still below the depth-two boundary.
- **Rust evidence:** The current `CommitFinalizer` consumes consecutive commits in
  order, but it does not implement the modeled v3 depth-two transaction trigger.
  There is no complete cross-node proof that all input paths select the same
  trigger.
- **Discharge:** Derive trigger equality from `ASM-SAFE-COMMIT-CHAIN`, and check trigger
  order at every finalizer input boundary.

## ASM-SAFE-COMMITTED-PREFIX

- **Claim:** Before the first depth-two trigger, the committed prefix contains every
  correct direct accept voter that is also in the trigger anchor quorum.
- **Type:** Protocol and Rust refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety.
- **Lean use:** `correctCommitAnchorInCertificate` and
  `anchorInCommittedPrefix` are the remaining refinement inputs. Lean combines
  `anchorInCommittedPrefix` with the GC theorem and the signed transaction vote
  witness to derive the direct-versus-indirect inclusion fact.
- **Rust evidence:** `FlexCommitter::build_commit` constructs each local v3 sub-DAG.
  For a commit installed through commit sync,
  `FlexCommitter::handle_certified_commit` reconstructs the v3 sub-DAG from the
  explicit block list in `CertifiedCommit`. The current `CommitFinalizer` receives
  these sub-DAGs in order, but it does not implement the modeled v3 transaction
  rule. No complete lemma connects both paths, a multi-leader commit, commit sync,
  and the modeled finalizer prefix.
- **Discharge:** Prove the committed-prefix lemma for local commit production and
  commit-sync installation. Add an integration invariant for the exact first
  trigger.

## ASM-SAFE-GC

- **Claim:** Core reads the GC boundary from the preceding commit before it records
  the new commit. The v3 schedule starts each next leader decision after the last
  committed leader round. For transaction votes, the signed cutoff covers both
  causal-history block GC and vote-tracker GC. V3 sub-DAG construction copies the
  required next-round anchor blocks into the finalizer's pending committed prefix
  before a later DAG GC operation can remove them.
- **Type:** Rust refinement.
- **Status:** Partially verified.
- **Effect if false:** Safety.
- **Lean use:** [`CoreGcState.evidence_retained`](../lean/Mysticeti/GarbageCollection.lean)
  proves the Core decision-to-anchor window.
  [`TransactionGcWindow.voting_round_retained`](../lean/Mysticeti/GarbageCollection.lean)
  proves the deep-target and near-target transaction cases.
  [`causal_history_gc_rejects`](../lean/Mysticeti/Finalizer.lean) and
  [`vote_tracker_gc_rejects`](../lean/Mysticeti/Finalizer.lean) prove the signed
  cutoff behavior. `BlockEvidenceStore` keeps the live DAG and pending committed
  prefix separate.
- **Rust evidence:** The current v3 path does not enforce `gc_depth > 2`.
  `FlexCommitter::build_commit` reads the old `DagState::gc_round()` before
  `Core::post_commit` records the new commit. The current proposer does not create
  `BlockV3` and does not produce the signed v3 cutoff. The current finalizer keeps
  pending `CommittedSubDag` values, and the commit-sync path uses the explicit block
  list in `CertifiedCommit` instead of the local DFS. The transaction-cutoff and
  complete local, commit-sync, replay, and recovery mapping is not proved.
- **Discharge:** Enforce `gc_depth > 2`. Prove that every required anchor voting
  block enters the exact `CommittedSubDag` prefix on all input paths. Prove that the
  finalizer processes that prefix before it can lose the evidence. Test the minimum
  supported GC depth, a slow finalizer, commit sync, and restart recovery.

## ASM-CONFIG-V3-ACTIVATION

- **Claim:** The analyzed FlexCommitter and modeled v3 transaction-finalization path
  is active for the epoch to which a proof claim is applied.
- **Type:** Configuration applicability.
- **Status:** Known mismatch.
- **Effect if false:** Applicability.
- **Lean use:** All model definitions describe the v3 path.
- **Rust evidence:** Normal Sui startup sets `enable_v3` to false. The current tree
  contains the FlexCommitter path but not the modeled v3 proposal and transaction
  finalizer.
- **Discharge:** Add versioned epoch activation, rollback rules, and mixed-version
  tests. Do not use a local process flag.

## ASM-CONFIG-VOTING

- **Claim:** `enable_v3` implies `transaction_voting_enabled` for every correct node
  in the epoch.
- **Type:** Configuration refinement.
- **Status:** Known mismatch.
- **Effect if false:** Safety.
- **Lean use:** The transaction safety theorem always uses the v3 voting rule.
- **Rust evidence:** The current `CommitFinalizer` bypasses voting when transaction
  voting is disabled. No constructor invariant connects the two flags. The current
  proposer also does not create `BlockV3`, so the implication alone does not supply
  the modeled v3 transaction rule.
- **Discharge:** Use one versioned feature value, or reject a configuration that
  enables v3 without transaction voting.

## ASM-REFINE-INTEGERS

- **Claim:** Rust integer types represent every natural number used by the model
  without overflow, truncation, or an invalid conversion.
- **Type:** Data refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Safety and liveness.
- **Lean use:** Stake, thresholds, rounds, commit indices, transaction indices, and
  schedule values use unbounded `Nat`.
- **Rust evidence:** The implementation uses several bounded integer types. There is
  no single checked bound for the full path.
- **Discharge:** Document maximum values, use checked arithmetic, and add exhaustive
  small-value and boundary-value property tests.

## ASM-LIVE-PARTIAL-SYNCHRONY

- **Claim:** There is an unknown GST after which each authenticated protocol message
  between correct processes is delivered within `delta`.
- **Type:** Network environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** [`PartialSynchrony`](../lean/Mysticeti/PartialSynchrony.lean) states
  this condition directly.
- **Rust evidence:** Network code can measure delay and reconnect, but it cannot
  enforce the network condition.
- **Discharge:** Keep this as an explicit deployment assumption. Do not use it to
  infer peer selection, data retention, task scheduling, or storage progress.

## ASM-LIVE-ROUND-CATCHUP

- **Claim:** After activation, a round jump creates each required intermediate
  proposal before it creates the proposal for the observed future round.
- **Type:** Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `SafeRoundChanges` and
  `ConsensusLivenessStageObligations.safeRoundChanges`.
- **Rust evidence:** `ThresholdClock` jumps to the observed round, and `Proposer`
  creates at most one block for the final clock round. The current code does not
  create required intermediate blocks.
- **Discharge:** Implement the safe intermediate-proposal loop and add a
  deterministic v3 simulation test. This rule is sufficient for the stronger
  liveness property for old leader blocks.

## ASM-LIVE-COMMIT-PROGRESS-RECOVERY

- **Claim:** The local recovery rule at one correct validator has these parts. The
  validator reads its current last-commit timestamp, or the last flushed timestamp
  after restart. It enters recovery when the local timeout expires and it is not
  behind the observed quorum commit index. Recovery eligibility and pacing stay
  keyed to that commit index. It does not make recovery proposals while the
  observed quorum commit index is ahead, and recovery ends when the epoch stops.
  If `P` is its highest known own proposal round, its only
  recovery proposal target is `P + 1`. It waits or starts block sync when quorum
  parents in round `P` are not available. It persists an enabled proposal before
  broadcast. While the commit index is unchanged, its schedule-independent
  proposal delay can grow without a fixed bound. For round `P`, a recovery proposal
  disables score-based ancestor exclusion. It includes the available block from
  each validator if the local DAG has exactly one such block. The recovery
  proposal includes no block from that validator when the DAG has more than one
  block for it. Normal score-based ancestor selection continues for older rounds.
- **Type:** Protocol and Rust refinement.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `NextRoundProposalTargets` models the local proposal target.
  `RecoveryEntryProcess` and `stalled_reaches_recovery_quorum_within` derive a
  recovery quorum from one local entry action per validator.
  `recoveryFrontierRound`, `recovery_frontier_is_attained`, and
  `recovery_authority_has_exact_next_path_to_frontier` derive a common proof
  frontier and the finite exact-next path to it from available parent quorums.
  `RecoveryBlockFlow`, `recovery_block_flow_visible_at_deadline`, and
  `recovery_quorum_has_chosen_layers_by_deadline` derive consecutive quorum block
  layers from proposal, delivery, and acceptance actions.
  `timely_first_slot_voting_from_parent_rule` derives the next-round direct-voter
  set from timely availability, the recovery immediate-parent rule, and the direct
  decision rule. `timely_progressing_first_slot_is_usable` then derives a usable
  anchor.
  `recovery_layers_to_usable_anchors` uses the accepted eventual sampling trace.
  `covered_usable_anchor_window_advances_modeled_flex_committer` and
  `full_flex_anchor_window_advances_commit_index` prove the deterministic
  FlexCommitter result. `commit_progress_recovery_from_processes` composes these
  results into commit-index progress in the Lean process model.
- **Rust evidence:** `DagState::last_commit_timestamp_ms` exposes the current
  in-memory commit timestamp. `DagState::flush` makes buffered commits durable, and
  `DagState::new` loads the last flushed commit after restart. `Context::clock`
  exposes local time. `BlockVerifier` enforces the immediate-parent quorum, and
  `BlockManager` accepts required parents before it accepts a child above GC. The
  current code does not have commit progress recovery, a commit-stall trigger for
  block production, an exact-next proposal mode, or adaptive recovery pacing.
  Current smart ancestor selection can omit a score-excluded immediate parent after
  it reaches quorum. Thus, it does not implement the recovery parent rule.

  The final local commit sequence is present in the current code.
  `Core::add_blocks` accepts blocks and calls `Core::try_commit_local`.
  `Core::try_commit_v3` calls `FlexCommitter::try_commit` in a loop.
  `FlexCommitter::try_commit` runs direct decisions, checks for a commit round,
  runs descending indirect decisions when necessary, checks again, and calls
  `build_commit`. When it returns a commit, `Core::try_commit_v3` calls
  `Core::post_commit`, which adds the commit to `DagState`. There is no separate
  queued commit action between these calls. The
  [design evidence](../design/commit_progress_recovery.md#current-flexcommitter-to-core-path)
  lists the component tests, Core test, and test commands for this path.

The `Known mismatch` status applies to the recovery policy as a whole. It does not
apply to every Rust fact in this section. The
[periodic Lean-to-Rust review](#periodic-lean-to-rust-refinement-review) gives the
binary result for each smaller Rust contract. It lists recovery entry, exact-next
targeting, pacing, parent selection, frontier history, legal GC frontier, and layer
mapping under **Missing Rust behavior — Commit progress recovery**. It lists the
pending-round scan and the final FlexCommitter-to-Core sequence under **Verified
current Rust behavior**.

- **Discharge:** Implement the local recovery rule and map each local Rust event to
  the corresponding Lean action or state effect. In particular, prove recovery
  entry and persistence. Map the common proof frontier to the maximum last signed
  round among the recovery quorum. Prove that valid causal history and block sync
  supply the finite parent interval, or that a commit occurs first. Verify each
  exact-next proposal, persistence before broadcast, block acceptance and retention,
  and inclusion of every timely, locally unique immediate parent. Verify that
  each pending round is in Rust's stored pending-round range and that the indirect
  scan visits the base of each complete anchor window. Newer stored rounds can be
  anchor evidence without being indirect-decision targets. Lean then derives the
  complete candidate-window mapping. Accept
  `FirstSlotSamplingTrace` as the almost-sure trace result of
  `ASM-LIVE-FIRST-SLOT-SAMPLING`, or add a probability model that derives it.
  The high-level recovery quorum, quorum block layer, usable anchor, and
  commit-progress implications are now discharged in Lean relative to these local
  contracts.
  The current Rust call path records the final result. Weak task fairness is still
  needed so that Core processes input events; it is not needed for a separate
  action between `try_commit` and `post_commit`. Keep the Rust-to-Lean state mapping
  as a mapping obligation. Add one focused regression test with an old
  undecided prefix and the required usable anchor window. The test must show that
  `try_commit_v3` records a commit and advances the commit index. Also keep a
  negative test with a shorter anchor window. Future changes to these Rust functions
  must preserve these results or update the model and proof. Use the epoch start
  timestamp when the commit index is zero. Use saturating subtraction for a future
  commit timestamp. Add deterministic v3 simulation tests.

## ASM-LIVE-LEADER

- **Claim:** Let `N` be actual validator set stake, `S` be leader schedule stake,
  and `P_r` be round leader selection stake for one pending leader round `r`. The
  structural relation is `P_r <= S <= N`. Given the separate non-progress stake
  bound `f + c`, the schedule viability bound is `f + c < S`. A protocol that uses
  a smaller round leader selection also needs a leader fairness rule. The sufficient
  quorum-coverage bound is `A <= P_r`. Current v3 selects the full leader schedule
  for every pending leader round, so its structural and coverage bounds are
  `A <= S = P_r <= N`. The per-slot safety proof and the quorum-coverage lemma do
  not use the optional resource policy `P_r <= Q`. These static bounds do not prove
  a usable anchor.
- **Type:** Protocol and configuration refinement.
- **Status:** Open proof obligation.
- **Effect if false:** Liveness.
- **Lean use:** `ConsensusLivenessStageObligations.fairRoundLeaderSelection` is a
  temporary stage obligation that must use these bounds with a separate fairness
  result. For commit progress recovery,
  `LeaderScheduleViabilityBounds` and
  `RoundLeaderSelectionCoverageBounds` separate schedule viability from per-round
  quorum coverage. `V3LeaderScheduleCoverageBounds` and
  `v3_coverage_applies_to_schedule_and_round` derive the v3 specialization from
  actual set weights.
  `quorum_and_round_leader_selection_intersect_outside_byzantine_stake` proves the
  per-round weighted intersection. The optional work cap has separate
  `RoundLeaderSelectionResourceBound` and `V3LeaderScheduleResourceBound` types.
  `leader_schedule_contains_progress_and_locally_included_stake` proves an
  existence result after local ancestor exclusion.
  `exclusion_cap_does_not_protect_a_fixed_first_slot` proves that this result does
  not supply direct votes for one fixed first selected leader slot.
- **Rust evidence:** `LeaderScheduleV3` builds the Rust `allowed_leaders` field by
  excluding low-score stake. This ordered field is the leader schedule.
  `FlexCommitter` creates one selected leader slot for every listed validator in
  each pending round at or above `min_next_leader_round`. It applies a
  deterministic round-based permutation to the slot order. Thus, current v3 uses
  the full leader schedule as the round leader selection for those rounds.
  `AncestorStateManager` caps local excluded stake at two thirds of the configured
  bad-node stake percentage. Its separate score cutoff is 20 percent of the highest
  score. These local sets need not be equal at different proposers. The schedule
  percentage rule does not directly enforce `A <= S`. The deterministic order has
  no proved fairness property that supplies the required anchor.
- **Discharge:** Define and enforce the schedule viability and round leader
  selection coverage bounds from the actual committee weights. Report `P_r <= Q`
  only if the deployment enables this resource policy. Prove usable-anchor progress
  only in recovery stage 3. Add stake-bound, equivocation, selection, and slot-order
  tests.

## ASM-LIVE-FIRST-SLOT-SAMPLING

- **Claim:** Assume that separate refinement results give one fixed leader schedule,
  one fixed non-progress set, and one common selected leader slot order at all
  correct validators while a commit index is stalled. Conditional on this state,
  each pending leader round samples one uniform random permutation of the leader
  schedule. Samples for different rounds are independent and are not controlled by
  the network or task scheduler. If the schedule has `m` members and `h > 0` of them
  are correct and non-crashed, the probability that the first selected leader slot
  is correct is `p = h / m`. A run of `k` rounds has a correct first slot in every
  round with probability `p^k`. The correct validators can be the same or different.
  Repeated disjoint runs contain such a run with probability one. Current v3 uses
  `k = indirectCommitDepth + 1 = 3` for the recovery proof.
- **Type:** Accepted probabilistic protocol model and Rust refinement.
- **Status:** Accepted modeling assumption.
- **Effect if false:** Liveness.
- **Lean use:** The deterministic Lean trace does not model probability.
  `FirstSlotSamplingTrace` states the almost-sure trace consequence: after recovery
  layers exist, an eventual candidate window has the required consecutive
  progressing first slots unless a commit already occurred.
  `recovery_layers_to_usable_anchors` combines this trace property with post-GST
  delivery, recovery production, and timely voting. A future probability model can
  prove that the failure probability after `j` disjoint three-round runs is
  `(1 - p^3)^j`, which approaches zero.
- **Rust evidence:** `FlexCommitter` seeds `StdRng` from the round number and
  shuffles the complete ordered leader schedule for each pending round. All correct
  validators with the same schedule compute the same deterministic permutation.
  The liveness analysis accepts the permutation output as independent and uniform
  relative to the fixed fault set.
- **Discharge:** No code change is required for the accepted probabilistic model.
  A strict proof of the exact deterministic sequence would instead need a weighted
  coverage theorem or a deterministic order with a bounded correct-first run.
  Discharge the fixed schedule and common-order preconditions through
  `ASM-SAFE-PARAMETERS` and the committed-prefix refinement. They are not sampling
  assumptions.
- **Accepted model:** Model each round's seeded shuffle as one common,
  independent, uniform random permutation of the leader schedule. Use a run of
  `indirectCommitDepth + 1` correct first slots. This is an accepted liveness
  abstraction for the deterministic Rust shuffle. It is not a safety assumption or
  a deterministic theorem about `StdRng`.

## ASM-LIVE-BLOCK-SYNC

- **Claim:** Target theorem: each required missing DAG block is eventually verified
  and accepted, or an installed commit advances the committed history and GC
  boundary so that Core no longer needs that block.
- **Type:** Derived Rust progress theorem.
- **Status:** Abstraction gap.
- **Effect if false:** Liveness.
- **Lean use:** Missing-ancestor recovery is hidden inside the bounded consensus
  phase transitions. There is no explicit block-sync state or theorem.
- **Rust evidence:** `BlockManager` suspends blocks with missing ancestors.
  `Synchronizer` uses direct requests, periodic requests, and a stored-history
  fallback after commit progress stalls.
- **Discharge:** Model missing blocks, accepted blocks, and the garbage-collection
  case. Prove
  eventual resolution under `ASM-LIVE-PEER-FAIRNESS` and
  `ASM-LIVE-TASK-FAIRNESS`. Use `Eventually` unless a code-derived bound exists.

## ASM-LIVE-COMMIT-SYNC

- **Claim:** Target theorem: each missing commit needed for progress is eventually
  installed through commit sync from a verified commit range whose tip has quorum
  commit votes, or the local commit rule reproduces it after the live block path
  supplies the required DAG blocks.
- **Type:** Derived Rust progress theorem.
- **Status:** Partially verified.
- **Effect if false:** Liveness.
- **Lean use:** `FinalizerLivenessStageObligations.continuousCommitStream` and
  `triggerEventually` assume the resulting stream.
- **Rust evidence:** Commit sync schedules complete batches, retries across peers,
  verifies the chain and the quorum certificate for the final commit in each range,
  buffers gaps, and sends ordered ranges to Core. A trailing partial batch is
  delegated to broadcast, subscription, and block sync.
- **Discharge:** Prove eventual stream extension under the peer and task assumptions.
  Model full batches, partial results, the incomplete tail, backpressure, and
  retention of the commits, certifying vote blocks, and sub-DAG blocks used by
  commit sync.

## ASM-LIVE-PEER-FAIRNESS

- **Claim:** This condition applies only when a lagging or restarted validator must
  fetch old consensus state. For each block or commit that the validator still
  needs, at least one known correct peer returns the item after GST, or verified
  commit sync moves the validator to a state that no longer needs it.
- **Type:** Network, storage, and peer-availability environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness after missing state or restart.
- **Lean use:** This is the useful-peer environment input for the open block-sync
  and commit-sync progress theorems. Eventual selection of that peer must be derived
  from the retry transition and its fairness or random-selection rule. The
  steady-state consensus theorem does not need this condition for messages that
  post-GST delivery already supplies.
- **Rust evidence:** Sync paths shuffle peers. The block-sync fallback selects one
  random peer per attempt. Some paths assume that the known-peer set is not empty.
- **Discharge:** State a consensus-block and commit retention, peer-discovery, and
  request-service contract. Then prove eventual useful-peer selection from
  deterministic fair peer rotation, or use an explicit probabilistic selection
  model. Transaction payloads are outside this contract because a validator or
  user can resubmit them.

## ASM-LIVE-TASK-FAIRNESS

- **Claim:** If one action in a correct proposer, block synchronizer, commit
  synchronizer, Core, finalizer, storage, or consumer task stays enabled, the
  runtime eventually schedules that action. This is weak task fairness. Queue
  capacity and consumer progress are separate conditions.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness.
- **Lean use:** `BoundedLocalProcessing` includes scheduler progress for each
  covered recovery action. Weak fairness remains necessary for continuously
  enabled retry loops and protocol tasks that do not have a finite response bound.
  The Lean model does not represent Tokio scheduling directly.
- **Rust evidence:** Tokio tasks, channels, retry loops, and monitors implement the
  work. The Lean trace does not model scheduling, shutdown, queue capacity, or task
  failure.
- **Discharge:** Define which shutdowns are outside the theorem. Model task guards
  and actions. State queue and consumer progress separately. Add progress monitors
  and tests for saturated queues, stalled consumers, task restart, and storage delay.

## ASM-LIVE-LOCAL-RESPONSE

- **Claim:** Local consensus computation takes at most a positive symbolic time
  `epsilon`, with `epsilon < delta`. This bound applies to one local consensus
  action that stays enabled at a correct, running validator. The aggregate bound
  includes task and queue scheduling, message verification, DAG acceptance,
  proposal construction, a required proposal flush, and handoff to the network
  transport. It also includes making a received result visible to the next local
  consensus action. The same uniform bound applies separately to each covered
  action. The message `sentAt` event is the transport send. The `deliveredAt` event
  is delivery to the receiving handler.
- **Type:** Runtime environment.
- **Status:** Environmental assumption.
- **Effect if false:** Liveness results that require timely local voting.
- **Lean use:** `BoundedLocalProcessing` states the general boundary.
  `protocol_packet_becomes_locally_visible` uses `delta + epsilon`.
  `protocol_packet_becomes_locally_visible_before_two_delays` uses
  `epsilon < delta`.
  `recovery_wait_eventually_covers_block_visibility` proves that an unbounded
  recovery wait eventually covers the difference between proposal send times, one
  message delay, and one local acceptance action.
  Weak task fairness alone cannot ensure that a correct first-slot proposal runs
  and becomes visible before the next-round proposals.
- **Rust evidence:** The receive path includes task queues, blocking-pool
  verification, the Core channel, Core processing, and DAG insertion. The proposal
  path flushes the block before Core hands it to the broadcast path. Tokio, storage,
  and the operating system do not give one hard bound for this complete path.
  Production load controls and task monitoring try to keep local response finite.
- **Discharge:** This proof accepts the guaranteed bound as the processor-speed
  part of the partial-synchrony environment. Define the covered Rust actions. A
  probabilistic response bound is also possible, but it needs a separate
  probability model.
- **Accepted model:** Use a positive symbolic local-computation bound `epsilon`
  with `epsilon < delta`. Do not use a fixed processing constant. With this bound,
  `delta + epsilon` bounds transport send through remote DAG visibility, and
  `delta + 2 * epsilon` bounds local proposal enablement through remote DAG
  visibility. A recovery timer also needs a derived bound from its local start
  event to the relevant proposal-enable or transport-send event.

## ASM-LIVE-PIPELINE-BOUNDS

- **Claim:** Target theorem: each modeled proposal, delivery, support, certificate,
  decision, and commit transition completes within its stated `delta` or
  `2 * delta` bound.
- **Type:** Derived timing refinement.
- **Status:** Abstraction gap.
- **Effect if false:** Liveness bound.
- **Lean use:** The fields of `ConsensusLivenessStageObligations` produce the
  `10 * delta` result.
- **Rust evidence:** Rust has separate scheduler periods, request timeouts, retry
  delays, processing time, and backpressure. They are not derived from the network
  `delta`.
- **Discharge:** Define each phase predicate on concrete Rust events. Either derive a
  bound that includes code timers and local processing, or weaken the production
  claim to eventual progress.

## ASM-LIVE-FINALIZER-TRIGGER

- **Claim:** Target theorem: every pending transaction eventually receives a later
  first eligible depth-two trigger, including at shutdown and the epoch tail.
- **Type:** Derived protocol and lifecycle theorem.
- **Status:** Known mismatch.
- **Effect if false:** Liveness.
- **Lean use:** `FinalizerLivenessStageObligations.triggerEventually`.
- **Rust evidence:** The current finalizer uses a different transaction trigger.
  For both the current path and the modeled v3 path, shutdown can leave pending
  state without a later same-epoch commit, and no explicit epoch-tail rule defines
  the result.
- **Discharge:** Keep consensus active through all required triggers, persist and
  replay pending state across the boundary, or define and prove a safe tail rule.

## ASM-LIVE-DURABILITY

- **Claim:** Target theorem: a produced commit enters the finalizer, a trigger
  produces one decision, and the decision becomes durable and reaches the consumer
  within the modeled bounds.
- **Type:** Derived Rust and timing theorem.
- **Status:** Partially verified.
- **Effect if false:** Safety and liveness.
- **Lean use:** `triggerToDecision`, `decisionToDurableOutput`, and
  `transaction_liveness.commitEntersFinalizer`.
- **Rust evidence:** The v3 commit path sends ordered committed sub-DAGs to the
  current finalizer. The current finalizer persists finalized results before it
  sends them to the consumer. The modeled v3 transaction decisions are not
  implemented. The model also does not map crashes, channel delay, storage latency,
  or replay to the `delta` bound.
- **Discharge:** Define the durable event, crash boundary, and replay relation. Test a
  crash before persistence, after persistence, and before consumer delivery.
