/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorPostGstCausalQueueService

namespace Mysticeti

/-! A coarse transfer budget for the post-GST causal-work queue.

`ValidatorPostGstCausalQueueServiceRules` assumes a strict service margin
`cAdd < cService` outright. This module derives that margin from two named
quantities: how many blocks can reach one validator in a fixed time, and how
many new required references ordinary round advancement adds in that time.

The budget is coarse. Its unit is whole blocks in one `delta`, and the model
does not give a larger block a longer transfer time. That is enough for the
margin, which counts queue items rather than bytes.

The budget is assumed large enough that ordinary round advancement is not
affected. The references that new rounds require are inside `productionBound`,
and `budgetExceedsProduction` puts that demand strictly below the budget. The
cap therefore binds only on bulk movement: recursive causal block sync and
commit sync over many blocks. The surplus `intervalBudget - productionBound` is
the rate at which a backlog drains.

The model covers one validator's ingest capacity. It is not a network model. It
has no cross-flow contention, no shared bottleneck, and no queueing discipline
between peers. Message delivery stays capacity-blind, so `ASM-LIVE-PARTIAL-SYNCHRONY`
still carries one `delta` for every protocol message whatever its size.

The budget itself is `ASM-LIVE-TRANSFER-BUDGET`. It supplies the service margin
of `ASM-LIVE-POST-GST-CAUSAL-SERVICE`.
-/

/-- A per-validator block-transfer budget and the queue behavior it supports.

`transferCap` is the max-throughput assumption itself. `backlogUsesBudget` says
that a validator with a full queue actually uses that capacity.
-/
structure ValidatorCausalQueueTransferBudget
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (validator start : Nat)
    extends ValidatorCausalQueueBehavior timed validator start where
  /-- The accounting interval, counted in whole `delta` periods. -/
  deltasPerInterval : Nat
  deltasPerIntervalPositive : 0 < deltasPerInterval
  serviceIntervalIsWholeDeltas :
    serviceInterval = deltasPerInterval * network.delta
  /-- At most this many whole blocks can reach the validator in one `delta`. -/
  blocksPerDelta : Nat
  /-- An upper bound on the number of rounds in one accounting interval. -/
  roundsPerInterval : Nat
  roundsPerIntervalPositive : 0 < roundsPerInterval
  /-- New above-GC references that ordinary round advancement requires in one
  interval. One author can produce at most one block in each round. -/
  productionBound : Nat
  /-- The production bound is committee size times the round bound. -/
  productionBoundIsCommitteeRounds :
    productionBound = config.authorityCount * roundsPerInterval
  /-- The budget is large enough for round advancement, with room to spare. -/
  budgetExceedsProduction :
    productionBound < blocksPerDelta * deltasPerInterval
  workAddedBound : ∀ interval, workAdded interval ≤ productionBound
  /-- The base queue removal cap is the transfer budget. -/
  workRemovalCapIsIntervalBudget :
    workRemovalCap = blocksPerDelta * deltasPerInterval
  /-- A full queue uses the whole budget. -/
  backlogUsesBudget : ∀ interval,
    blocksPerDelta * deltasPerInterval ≤ (pending interval).length →
      blocksPerDelta * deltasPerInterval ≤ workRemoved interval
  lowBacklogClearsOld : ∀ interval,
    (pending interval).length < blocksPerDelta * deltasPerInterval →
      ∀ reference, reference ∈ pending interval →
        reference ∉ pending (interval + 1)

namespace ValidatorCausalQueueTransferBudget

variable {BlockId CommitId PacketId : Type}
variable [DecidableEq BlockId]
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {validator start : Nat}

/-- Blocks that one accounting interval can move. -/
def intervalBudget
    (budget : ValidatorCausalQueueTransferBudget timed validator start) : Nat :=
  budget.blocksPerDelta * budget.deltasPerInterval

/-- A populated committee and a positive round bound give positive production. -/
theorem productionBoundPositive
    (budget : ValidatorCausalQueueTransferBudget timed validator start) :
    0 < budget.productionBound := by
  have authorityCountPositive : 0 < config.authorityCount := by
    have validatorInRange := budget.validatorInRange
    omega
  rw [budget.productionBoundIsCommitteeRounds]
  exact Nat.mul_pos authorityCountPositive budget.roundsPerIntervalPositive

/-- Capacity left over after ordinary round advancement. This is the rate at
which a backlog drains. -/
def drainRate
    (budget : ValidatorCausalQueueTransferBudget timed validator start) : Nat :=
  budget.intervalBudget - budget.productionBound

theorem drainRatePositive
    (budget : ValidatorCausalQueueTransferBudget timed validator start) :
    0 < budget.drainRate := by
  have margin := budget.budgetExceedsProduction
  unfold drainRate intervalBudget
  omega

/-- The budget supplies the assumed service margin. Every later result about the
causal-work queue therefore holds without a separate margin assumption. -/
def toServiceRules
    (budget : ValidatorCausalQueueTransferBudget timed validator start) :
    ValidatorPostGstCausalQueueServiceRules timed validator start :=
  { budget.toValidatorCausalQueueBehavior with
    cAdd := budget.productionBound
    cService := budget.intervalBudget
    serviceMargin := budget.budgetExceedsProduction
    workAddedBound := budget.workAddedBound
    highBacklogService := budget.backlogUsesBudget
    lowBacklogClearsOld := budget.lowBacklogClearsOld }

/-- A full queue drains by at least the spare capacity in each interval. This is
the quantitative form of the strict decrease that the service rules give. -/
theorem high_backlog_drains_by_spare_capacity
    (budget : ValidatorCausalQueueTransferBudget timed validator start)
    {interval : Nat}
    (high : budget.intervalBudget ≤ (budget.pending interval).length) :
    (budget.pending (interval + 1)).length + budget.drainRate ≤
      (budget.pending interval).length := by
  have added := budget.workAddedBound interval
  have removed := budget.backlogUsesBudget interval high
  have balance := budget.queueBalance interval
  have margin := budget.budgetExceedsProduction
  unfold drainRate intervalBudget at *
  omega

/-! ### The budget is necessary, not only sufficient

The results below do not use `budgetExceedsProduction`. They show what the
transfer cap alone forces when arrivals reach the budget.
-/

/-- An interval whose removal does not exceed its arrivals cannot shrink the
queue. -/
theorem no_margin_no_drain
    (budget : ValidatorCausalQueueTransferBudget timed validator start)
    {interval : Nat}
    (saturated : budget.workRemoved interval ≤ budget.workAdded interval) :
    (budget.pending interval).length ≤
      (budget.pending (interval + 1)).length := by
  have balance := budget.queueBalance interval
  omega

/-- If arrivals in one interval reach the transfer budget, the backlog cannot
shrink in that interval. A budget at or below the arrival rate therefore gives
no catch-up, whatever the rest of the pipeline does. -/
theorem saturated_interval_does_not_drain
    (budget : ValidatorCausalQueueTransferBudget timed validator start)
    {interval : Nat}
    (saturated : budget.intervalBudget ≤ budget.workAdded interval) :
    (budget.pending interval).length ≤
      (budget.pending (interval + 1)).length := by
  have cap := budget.workRemovedBound interval
  rw [budget.workRemovalCapIsIntervalBudget] at cap
  unfold intervalBudget at *
  exact budget.no_margin_no_drain (Nat.le_trans cap saturated)

end ValidatorCausalQueueTransferBudget

end Mysticeti
