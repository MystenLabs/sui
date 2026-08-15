<!--
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-->

# Mysticeti v3 implementation gaps

This report lists work that must finish before the related formal results apply to
the product. P0 items block v3 activation. P1 items block complete safety or
progress claims. P2 items reduce conformance and boundary risks.

The [assumption ledger](ASSUMPTIONS.md) gives the status of each condition.

## Commit progress recovery

### P0: implement commit progress recovery

Related assumptions: `ASM-LIVE-COMMIT-PROGRESS-RECOVERY`,
`ASM-LIVE-LOCAL-RESPONSE`, `ASM-LIVE-LEADER`, and
`ASM-LIVE-FIRST-SLOT-SAMPLING`.

The product can move to a future round without filling its own proposal sequence.
Partial synchrony alone does not ensure that commits resume.

Implement the [recovery design](../design/commit_progress_recovery.md). The product
must enter recovery after a commit stall, propose only one round after its highest
known signed round, preserve growing pacing across round jumps, and require a
valid immediate-parent quorum. It must request missing parents and disable
score-based exclusion only for the immediate parent round. It must keep
equivocation checks, durable-before-send behavior, legal old-block cleanup
boundaries, and safe exit rules.

The normal commit path and immediate-parent quorum check already exist. Recovery
must preserve them. It does not need a separate certified commit prefix.

The older [round-jump proposal](https://www.cs.yale.edu/flint/certikos/publications/sp26.pdf)
uses a stronger intermediate-proposal rule. That paper studies an older protocol.
This design proves only commit-index progress and does not claim the paper's full
trace unchanged.

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

Bind the leader-order algorithm to a protocol version. The current deterministic
shuffle is not stable across all build configurations and has no proved coverage
for the accepted probability model.

### P1: prove block and commit synchronization progress

Related assumptions: `ASM-LIVE-BLOCK-SYNC`, `ASM-LIVE-COMMIT-SYNC`,
`ASM-LIVE-PEER-FAIRNESS`, and `ASM-LIVE-TASK-FAIRNESS`.

Synchronization mechanisms exist, but their existence does not prove eventual
progress. Define conditions for retained old data, peer discovery, fair retries,
enabled local work, partial batches, empty peer sets, and temporary backpressure.
Then prove that each required item arrives or becomes unnecessary after verified
commit synchronization.

Where an empty peer set is valid, wait and retry instead of stopping the fetch
path.

This condition applies to old consensus state. Transaction payloads can be
submitted again.

### P1: define finalizer shutdown behavior

Related assumptions: `ASM-LIVE-FINALIZER-TRIGGER` and
`ASM-LIVE-DURABILITY`.

A finalizer can hold a transaction that needs a later trigger. Draining its input
does not settle this state. Keep consensus active until the trigger occurs, store
and replay pending state, or define another safe epoch-tail result.

### P1: close the common commit-chain proof

Related assumptions: `ASM-SAFE-COMMIT-CHAIN`, `ASM-SAFE-FIRST-TRIGGER`, and
`ASM-LIVE-COMMIT-SYNC`.

Show that local production, synchronization, recovery, restart, and old-block
cleanup give all correct validators one continuous index-and-digest chain and the
same first eligible trigger. Existing local checks support this claim, but the
cross-validator proof is open. No concrete fork is known.

### P1: protect committed-prefix evidence

Related assumptions: `ASM-SAFE-COMMITTED-PREFIX`, `ASM-SAFE-GC`, and
`ASM-SAFE-PARENT-QUORUM`.

Indirect decisions need complete ordered evidence, including all leaders in a
multi-leader commit. Show that required votes and triggers enter the pending
committed prefix on local, synchronization, recovery, replay, and restart paths.

Local ownership protects evidence after it enters that prefix. The open gap is
end-to-end inclusion, not deletion from the live block cache alone. Enforce the
required cleanup depth and signed transaction cutoff.

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
