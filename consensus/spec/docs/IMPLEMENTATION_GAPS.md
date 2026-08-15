<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 implementation gaps

This report describes the current `consensus/` tree. `FlexCommitter` is wired into
`Core` on the v3 path. The current proposer does not create `BlockV3`, and the tree
does not contain `CommitFinalizerV3`. The Lean v3 transaction-finalization model
therefore has no current Rust implementation mapping.

The [assumption ledger](ASSUMPTIONS.md) defines the stable identifiers used in this
report. A gap closes only when its related proof obligation is discharged or its
environmental assumption is explicitly accepted.

## P0: implement and test commit progress recovery

Related assumption: `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`.

`ASM-LIVE-ROUND-CATCHUP` is a stronger alternative. It gives liveness for old
leader blocks. It is not an activation condition for the chosen commit-progress
design.

See the
[commit progress recovery design](../design/commit_progress_recovery.md) for the
proposed Rust behavior, proof obligations, and test plan.

The
[assumption ledger](ASSUMPTIONS.md#asm-live-commit-progress-recovery) separates
this work into three categories: missing Rust logic, Rust logic that is present but
not fully verified, and current Rust behavior that tests protect. Only the first
category is a missing implementation feature. The second category is a verification
gap. The third category is a behavior contract for future changes.

This is a confirmed implementation gap and an activation blocker. The strong Lean
liveness theorem uses a catch-up condition for old leader opportunities. The new
Lean process theorem proves recovery-quorum formation, consecutive quorum block
layers, usable-anchor formation, and commit-index progress from local action and
network contracts. It is not a liveness theorem for the current Rust code because
the recovery policy is not implemented and the Rust-to-Lean state mapping is not
machine checked.

[`ThresholdClock::add_block`](../../core/src/threshold_clock.rs) accepts one block
from a future round. It clears the old aggregator and moves the local clock to that
future round or the next round. It does not request proposals for the omitted
intermediate rounds.

[`Core::try_propose`](../../core/src/core.rs) calls
`ValidatorProposer::try_new_block` for the current clock round. It does not create
the required intermediate blocks first.

Standard partial synchrony does not establish this missing condition. The 2026
mechanized Mysticeti analysis gives an infinite counterexample for original
Mysticeti and a
[modified round-jump rule](https://www.cs.yale.edu/flint/certikos/publications/sp26.pdf).
That paper studies an older protocol and implementation. This work does not claim
that its complete infinite trace applies unchanged to the multi-leader v3 rules.
However, the same direct-jump mechanism is present, and the Lean counterexample
shows that the v3 code cannot satisfy the stated liveness contract without another
rule.

The intermediate-proposal rule is sufficient for liveness for old leader blocks.
It is not known to be necessary for commit-index progress. For the narrower
property, implement the linked commit progress recovery design.

The current Rust code does not have these parts:

- a `commit_progress_recovery_timeout` configuration value and stall trigger;
- a named proposal mode that proposes exactly one round above the highest known own
  proposal, keeps the immediate-parent quorum check, bypasses the selected leader
  slot availability wait, and keeps schedule-independent pacing;
- recovery state and pacing that remain keyed by commit index across future round
  jumps;
- a recovery legal-frontier rule for a highest own proposal round at or below GC;
- a recovery parent-selection rule that disables score-based ancestor exclusion for
  the immediate parent round and includes the locally unique available block from
  each validator;
- a positive symbolic post-GST processing bound `epsilon`, with
  `epsilon < delta`, and a recovery wait that can grow beyond the applicable
  network and processing bound;
- one exact local event that starts each recovery pacing interval, and a proof that
  bounds proposal skew from that event during a stable recovery period;
- Rust mappings for the local recovery-entry action, immediate-parent quorum
  readiness, the maximum last signed round that forms the execution-derived layer
  base, proposal persistence and broadcast, block acceptance, timely parent
  inclusion, pending rounds, and
  retained evidence. For the pending array, verify that each pending leader round
  is in the stored range and that the indirect scan visits each stored index;
- deterministic simulation tests for schedule changes, stake bounds, selective
  delivery, synchronization, GC, restart, and future timestamps.

The final local FlexCommitter-to-Core call path is not a missing implementation
feature. The
[design evidence](../design/commit_progress_recovery.md#current-flexcommitter-to-core-path)
traces the current synchronous call path and lists the tests that cover it. The
Rust-to-Lean state mapping is not machine checked. One focused old-prefix
recovery-window regression test is still missing. Future changes to the scan order,
pending-state cache, commit construction, or Core loop must keep the tests and the
Lean result valid.

The [design document](../design/commit_progress_recovery.md) is the canonical source
for the trigger logic, proposal rules, derived recovery distances, leader schedule
and round leader selection bounds, synchronization rules, test plan, and activation
condition. `CertifiedCommit` remains an input to commit sync. Commit progress
recovery does not require a separate certified commit prefix.

The immediate-parent quorum check is not missing. `BlockVerifier` rejects a child
without quorum stake from its immediate parent round. `BlockManager` does not accept
a child above GC until its required parents are accepted. Keep these current
properties and add the recovery progress proof around them.

## P0: put v3 activation in epoch protocol state

Related assumption: `ASM-CONFIG-V3-ACTIVATION`.

This is a confirmed activation gap.

[`to_consensus_protocol_config`](../../../crates/sui-core/src/consensus_manager/mod.rs)
sets `enable_v3` to `false`. Therefore, normal Sui startup does not use the new
FlexCommitter commit path. The modeled v3 proposal and transaction-finalization
path is also not present in the current `consensus/` tree.

Add a versioned `ProtocolConfig` field. Use an epoch-bound activation value. Add a
rollback plan and mixed-version tests. Do not use a node-local flag for activation.

## P0: remove node-local threshold inputs

Related assumptions: `ASM-MATH-THRESHOLDS`, `ASM-SAFE-PARAMETERS`, and
`ASM-REFINE-INTEGERS`.

This is a confirmed safety boundary.

[`apply_v3_threshold_overrides`](../../../crates/sui-core/src/consensus_manager/mod.rs)
reads `SUI_CONSENSUS_V3_MALICIOUS_STAKE` and
`SUI_CONSENSUS_V3_CRASH_STAKE` from each process environment. Two correct nodes can
therefore construct different `Q`, `A`, and validity thresholds for the same epoch.

Move `f` and `c`, or all derived thresholds, into authenticated epoch state. Every
correct node must derive the same values. Include the values in diagnostics and in
the protocol compatibility checks.

Use checked arithmetic in
[`Committee::new_v3`](../../config/src/committee.rs). In particular, check the
`5 * f + 3 * c` calculation and all threshold additions and multiplications before
v3 activation.

## P0: implement and bind v3 transaction voting

Related assumptions: `ASM-CONFIG-VOTING` and `ASM-SAFE-EVIDENCE-REFINEMENT`.

The current proposer creates `BlockV1` or `BlockV2`. It does not create `BlockV3`,
so it does not produce the signed v3 transaction cutoff used by the Lean proof.
The current tree uses [`CommitFinalizer`](../../core/src/commit_finalizer.rs); it
does not contain the modeled v3 transaction-finalization rule.

Implement the v3 proposal and transaction-finalization rules. Then add a constructor
check for this condition:

```text
enable_v3 implies transaction_voting_enabled
```

A single versioned v3 feature value is safer than two independent values. Until
this work is complete, the Lean transaction theorems do not make a claim about the
current Rust implementation.

## P1: establish the leader schedule and round leader selection liveness conditions

Related assumption: `ASM-LIVE-LEADER`.

[`LeaderScheduleV3::select_allowed_leaders_with_fixed_config`](../../core/src/leader_schedule_v3.rs)
can remove low-score stake from the leader schedule. Current v3 then uses every
schedule member in the round leader selection. The Lean liveness theorem requires
the leader schedule to contain positive stake from a correct, non-crashed
validator. A protocol with a smaller round leader selection must also eventually
select such a validator.

Add a runtime and protocol-config condition that connects these values:

- maximum excluded stake;
- Byzantine stake `f`;
- crash stake `c`;
- required live stake for proposal and certificate progress.

Prove that the leader schedule cannot contain only Byzantine or crashed validators.
For current v3, enforce the sufficient lower bound `A <= S = P_r` for each pending
leader round. The upper bounds are `S = P_r <= N`. Report `P_r <= Q` only if the
deployment enables this optional work limit. Require a positive leader schedule
window and update interval. Add property tests for all boundary values.

The ancestor exclusion stake cap is not a replacement for the recovery rule. It can
prove that one proposer has some correct, available schedule stake that is locally
included when the combined stake bound holds. It does not prove that the first
selected leader is outside the local exclusion sets of quorum stake of next-round
proposers. Disable score-based exclusion only for the immediate parent round during
commit progress recovery. Keep equivocation handling and normal exclusion for older
ancestors.

## P1: specify block-sync and commit-sync liveness

Related assumptions: `ASM-LIVE-BLOCK-SYNC`, `ASM-LIVE-COMMIT-SYNC`,
`ASM-LIVE-PEER-FAIRNESS`, `ASM-LIVE-TASK-FAIRNESS`, and
`ASM-LIVE-LOCAL-RESPONSE`.

The Lean model does not represent missing DAG blocks, suspended blocks, peers,
request retries, commit ranges, or consumer backpressure. The related eventual
progress claims are open theorem goals. They are not primitive assumptions.

The implementation has important recovery mechanisms:

- `BlockManager` suspends a block until its required ancestors are accepted.
- `Synchronizer` uses direct fetches, periodic fetches, and stored-history fetches.
- Periodic block sync resumes when commit sync does not make commit progress.
- `CommitSyncer` retries commit ranges, verifies the commit chain and certificate,
  buffers ranges across gaps, and sends consecutive ranges to Core.

For normal recovery with durable storage, the validator at the execution-derived
frontier can serve its recent block and causal parents. After local storage loss, a
peer that retained the block must serve it. The proof must map the applicable source
and retained range to the sync requests. This does not require a new
transaction-retention assumption. If the syncer can request only a random peer, the
retry proof must still show that it eventually selects a correct source.

Use these contracts only when a lagging or restarted validator needs old consensus
state:

1. A correct known peer retains each still-required DAG block and, for commit sync,
   each still-required commit range and its certifying vote blocks. Alternatively,
   verified commit sync moves the validator to a state that no longer needs the
   old item.
2. Peer discovery eventually provides such a peer.
3. Retry selection is fair. If selection remains random, use a probabilistic model
   and prove almost-sure progress.
4. A continuously enabled task at a correct validator eventually runs.
5. A correct peer answers a valid request for retained data.

These contracts do not require retention of transaction payloads. A validator or
user can resubmit a transaction.

Then model the request, response, verification, acceptance, installation, retry,
queue, and GC transitions. Prove these results:

- a missing block above GC is eventually accepted, or a commit removes the need;
- commit sync and the live block path process the trailing partial batch;
- sustained backpressure clears when the consumer has its own stated progress
  contract.

Add separate Lean state and progress theorems for block sync and commit sync. Use an
eventual-progress property unless a bound includes the exact scheduler periods,
timeouts, retry delays, storage time, and processing time. Do not derive this bound
from the network `delta` alone.

For the implementation, replace an empty-peer assertion with wait-and-retry behavior
where an empty peer set is valid. Use deterministic fair peer rotation, or document
the random selection model. Add tests for peer loss, data retention at the GC
boundary, commit-sync stall fallback, incomplete commit batches, and consumer
backpressure.

## P1: define finalizer tail behavior at shutdown

Related assumptions: `ASM-LIVE-FINALIZER-TRIGGER` and `ASM-LIVE-DURABILITY`.

[`CommitFinalizerHandle::stop`](../../core/src/commit_finalizer.rs) closes the input
channel and waits for the task. The task drains received commits. However,
`CommitFinalizer` can still contain pending commits that need a later trigger. The
modeled v3 finalizer also needs a later depth-two trigger.

Same-epoch recovery can replay unfinalized commits from storage. This does not by
itself define the epoch-end result for the last pending commits.

Add one explicit rule:

- keep consensus active until every accepted commit has its required trigger; or
- persist and replay the pending finalizer state across the epoch boundary; or
- define and prove a safe epoch-tail decision rule.

Add a shutdown test where the last input commit contains a pending transaction and
has no depth-two successor.

## P1: make the common commit-chain contract explicit

Related assumptions: `ASM-SAFE-COMMIT-CHAIN`, `ASM-SAFE-FIRST-TRIGGER`, and
`ASM-LIVE-COMMIT-SYNC`.

The indirect safety proof requires one continuous common commit stream and the same
first eligible trigger. The implementation has useful checks:

- the finalizer checks consecutive commit indices;
- the v3 commit-sync path checks the previous digest before it installs a commit;
- recovery checks for an index gap.

Complete the refinement proof for local commit production, commit-sync
installation, recovery, and garbage collection. Add one invariant helper that
validates the index, previous digest, and trigger order at every input boundary.
Use the helper in tests and in debug builds.

This item is a proof-closure gap. No concrete fork is known in the current code.

## P1: prove the committed-prefix and garbage-collection lemma

Related assumptions: `ASM-SAFE-COMMITTED-PREFIX`, `ASM-SAFE-GC`, and
`ASM-SAFE-PARENT-QUORUM`.

The transaction indirect rule counts accept voters from the complete buffered
commit prefix. This is necessary because one v3 commit can have more than one
leader. The rule rejects when that prefix has less than certificate stake.

Lean now checks the GC arithmetic in the protocol model:

- Core calculates `gc_round = last_commit_round - gc_depth` before it records the
  new commit.
- The next v3 leader decision round is after `last_commit_round`. Thus, the leader
  decision, next-round votes, and anchor path are above the old GC boundary.
- A transaction voting block uses the signed numeric cutoff. The cutoff is the
  maximum of the causal-history block-GC round and the vote-tracker GC round.
- A target at or below either proposer-side GC boundary is a reject vote.
- For a target far below its commit leader, the first commit preserves next-round
  evidence. For a target near its commit leader, the first trigger preserves it.
  The second case uses `gc_depth > 2` and the commit before the first trigger.
- Later DAG GC does not change the modeled pending committed-prefix store.

The proof still needs these implementation facts:

1. A correct accept voter cannot commit before its target block.
2. A depth-two leader has a verified immediate-parent quorum.
3. `FlexCommitter::build_commit` includes the target only above the GC boundary
   that it read before `Core::post_commit` records the new commit.
4. The complete anchor causal history has a quorum of voting-round blocks, not only
   a quorum of immediate parents at the anchor round.
5. Local `FlexCommitter::build_commit` and the commit-sync
   `FlexCommitter::handle_certified_commit` path include each required accept voter
   in the exact `CommittedSubDag` sequence before the first trigger.
6. Commit sync, replay, and recovery produce the same prefix and first trigger.
7. A slow finalizer keeps the blocks in its pending `CommittedSubDag` values after
   the live DAG cache removes those rounds.

The constructor checks `gc_depth > 2`, and the block verifier checks
immediate-parent quorum stake. The current proposer does not produce the signed v3
cutoff. Implement and test that rule before the transaction GC theorem is applied
to Rust. Then add an integration invariant that covers both v3 sub-DAG paths,
commit-sync recovery, the pending finalizer prefix, and the exact GC boundary.

The current `CommitFinalizer` direct path reads live DAG state. A slow finalizer can
lose a direct-decision opportunity after DAG GC. This loss does not make a false
quorum, but it affects liveness. The modeled buffered indirect path must complete
after this event.

## P2: close the natural-number to Rust-integer refinement

Related assumption: `ASM-REFINE-INTEGERS`.

Lean uses unbounded natural numbers. Rust uses `u32`, `u64`, `u128`, `u16`, and
`usize` in this path.

Document and check these bounds:

- the epoch cannot approach the maximum round or commit index;
- stake sums and threshold products cannot overflow;
- the maximum transaction count stays below the reserved `TransactionIndex` range;
- schedule counters and score products cannot overflow.

Reuse the Lean threshold equations in Rust property tests. Run the tests over small
exhaustive values and large boundary values.

## P2: add a Rust-to-Lean conformance suite

Related assumptions: `ASM-SAFE-AUTHENTICATION`, `ASM-SAFE-NON-EQUIVOCATION`, and
`ASM-SAFE-EVIDENCE-REFINEMENT`.

The Lean model is hand written. It does not yet prove that the Rust decision
functions implement the same relation.

Export small test vectors from Lean or duplicate the executable Lean functions in a
stable data format. Check these Rust functions against the vectors:

- `LeaderSlotDecider::try_direct_decide`;
- `LeaderSlotDecider::try_indirect_decide`;
- `FlexCommitter::find_anchor_block`;
- the descending loop in `FlexCommitter::try_indirect_commit`;
- `FlexCommitter::find_commit_leader_round`;
- `FlexCommitter::build_commit` and the Core commit-index update;
- after implementation, the v3 direct and indirect transaction decisions;
- after implementation, the first depth-two transaction trigger;
- after implementation, the v3 transaction cutoff rule, including both source GC
  rounds;
- the deep-target and near-target v3 sub-DAG GC cases.

Include equivocation vectors. One Byzantine authority can count once on each side,
but it cannot count more than once on either side.
