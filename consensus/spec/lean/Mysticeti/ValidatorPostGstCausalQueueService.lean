/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorExecutionLemmas
import Mysticeti.ValidatorTimedExecution
import Mysticeti.ValidatorCausalRecoveryCapsule
import Mysticeti.ValidatorOperationalFrontierCollectiveSuccessor

namespace Mysticeti

/-!
A GC-aware service model for one correct validator's post-GST causal-work
queue.

The model samples the real validator trace at one fixed interval. It records
only queue state, work accounting, and local transition rules. It does not
contain a future block, carrier, acceptance witness, quorum layer, Flex run,
or commit install.

New work is bounded by `cAdd` in each interval. When the queue contains at
least `cService` items, the interval removes at least `cService` items. The
strict margin `cAdd < cService` makes the queue smaller. When the queue is
smaller than `cService`, the next interval removes each old item. The queue
accounting invariant then derives that each removed known item was accepted or
became obsolete because GC moved.
-/

/-- The sampled time at one post-GST service interval. -/
def validatorPostGstCausalQueueSampleTime
    (start serviceInterval interval : Nat) : Nat :=
  start + interval * serviceInterval

/-- One causal reference is complete for a validator if it is accepted or no
longer required because the local GC round reached it. -/
def ValidatorCausalReferenceResolvedAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (validator time : Nat)
    (reference : ValidatorBlockRef BlockId) : Prop :=
  ((timed.execution.trace time).validatorState validator).accepted reference =
      true ∨
    reference.round ≤
      ((timed.execution.trace time).validatorState validator).gcRound

/-- Accepted or GC-obsolete causal work stays resolved. -/
theorem validator_causal_reference_resolved_persists
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator earlier later : Nat}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (ordered : earlier ≤ later)
    (resolved : ValidatorCausalReferenceResolvedAt timed validator earlier
      reference) :
    ValidatorCausalReferenceResolvedAt timed validator later reference := by
  rcases resolved with accepted | obsolete
  · exact Or.inl (timed.execution.accepted_block_persists validatorInRange
      ordered accepted)
  · exact Or.inr (Nat.le_trans obsolete
      (timed.execution.durableStateMonotone validator earlier later
        validatorInRange ordered).2.2.2.2.2.2.2.2.2.2.2)

/-- Local post-GST causal-queue service rules for one fixed validator.

`known` identifies causal references that have entered this queue model.
`knownAccounted` requires each known reference to be pending or already
resolved. Queue removals include normal acceptance and GC obsolescence.

`workAdded` and `workRemoved` are accounting values for one service interval.
The balance equation prevents the rate bounds from creating a queue decrease
without matching local queue work. -/
structure ValidatorPostGstCausalQueueServiceRules
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
    (validator start : Nat) where
  validatorInRange : validator < config.authorityCount
  validatorCorrectAvailable : faults.correctAvailable validator = true
  startAfterGst : network.gst ≤ start
  active : ∀ time, start ≤ time →
    (timed.execution.trace time).epochActive = true
  serviceInterval : Nat
  serviceIntervalPositive : 0 < serviceInterval
  cAdd : Nat
  cService : Nat
  serviceMargin : cAdd < cService
  pending : Nat → List (ValidatorBlockRef BlockId)
  known : Nat → ValidatorBlockRef BlockId → Prop
  workAdded : Nat → Nat
  workRemoved : Nat → Nat
  knownMonotone : ∀ {earlier later reference},
    earlier ≤ later → known earlier reference → known later reference
  pendingIsRequired : ∀ interval reference,
    reference ∈ pending interval →
      ((timed.execution.trace
        (validatorPostGstCausalQueueSampleTime start serviceInterval interval)
          ).validatorState validator).accepted reference = false ∧
      ((timed.execution.trace
        (validatorPostGstCausalQueueSampleTime start serviceInterval interval)
          ).validatorState validator).gcRound < reference.round
  knownAccounted : ∀ interval reference,
    known interval reference →
      reference ∈ pending interval ∨
        ValidatorCausalReferenceResolvedAt timed validator
          (validatorPostGstCausalQueueSampleTime start serviceInterval interval)
          reference
  workAddedBound : ∀ interval, workAdded interval ≤ cAdd
  queueBalance : ∀ interval,
    (pending (interval + 1)).length + workRemoved interval =
      (pending interval).length + workAdded interval
  highBacklogService : ∀ interval,
    cService ≤ (pending interval).length →
      cService ≤ workRemoved interval
  lowBacklogClearsOld : ∀ interval,
    (pending interval).length < cService →
      ∀ reference, reference ∈ pending interval →
        reference ∉ pending (interval + 1)

/-- Pointwise admission of one already-present parent-sync history into the
fixed receiver's causal queue.

The parent-sync source exists at or before the first sample. The adapter maps
only a reference that is still unaccepted and above GC at that sample. It does
not state that delivery, acceptance, or a later proposal will occur.

The `unresolvedAboveGcIsKnown` field is the exact BlockManager refinement
boundary. The parent-sync source by itself does not prove this field. -/
structure ValidatorBlockParentSyncQueueAdmissionAt
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (queue : ValidatorPostGstCausalQueueServiceRules timed validator start)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {block : ValidatorBlock BlockId}
    {holder sourceAt firstInterval : Nat}
    {history : List (ValidatorBlock BlockId)}
    (source : ValidatorBlockParentSyncSource syncRules block validator holder
      history sourceAt) : Prop where
  sourceBeforeFirstSample : sourceAt ≤
    validatorPostGstCausalQueueSampleTime start queue.serviceInterval
      firstInterval
  unresolvedAboveGcIsKnown : ∀ historyBlock,
    historyBlock ∈ history →
    ((timed.execution.trace
      (validatorPostGstCausalQueueSampleTime start queue.serviceInterval
        firstInterval)).validatorState validator).accepted
          historyBlock.reference = false →
    ((timed.execution.trace
      (validatorPostGstCausalQueueSampleTime start queue.serviceInterval
        firstInterval)).validatorState validator).gcRound <
          historyBlock.reference.round →
    queue.known firstInterval historyBlock.reference

namespace ValidatorPostGstCausalQueueServiceRules

/-- Each queue sample is in the active post-GST suffix. -/
theorem sample_is_active_after_gst
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (rules : ValidatorPostGstCausalQueueServiceRules timed validator start)
    (interval : Nat) :
    network.gst ≤
        validatorPostGstCausalQueueSampleTime start rules.serviceInterval
          interval ∧
      (timed.execution.trace
        (validatorPostGstCausalQueueSampleTime start rules.serviceInterval
          interval)).epochActive = true := by
  have startBeforeSample : start ≤
      validatorPostGstCausalQueueSampleTime start rules.serviceInterval
        interval := by
    unfold validatorPostGstCausalQueueSampleTime
    exact Nat.le_add_right start _
  exact ⟨Nat.le_trans rules.startAfterGst startBeforeSample,
    rules.active _ startBeforeSample⟩

/-- A positive service interval makes the next sample strictly later. -/
theorem next_sample_is_later
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (rules : ValidatorPostGstCausalQueueServiceRules timed validator start)
    (interval : Nat) :
    validatorPostGstCausalQueueSampleTime start rules.serviceInterval interval <
      validatorPostGstCausalQueueSampleTime start rules.serviceInterval
        (interval + 1) := by
  unfold validatorPostGstCausalQueueSampleTime
  rw [Nat.add_mul]
  have positive := rules.serviceIntervalPositive
  omega

/-- Sample times do not go backwards. -/
theorem sample_time_mono
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (rules : ValidatorPostGstCausalQueueServiceRules timed validator start)
    {earlier later : Nat}
    (ordered : earlier ≤ later) :
    validatorPostGstCausalQueueSampleTime start rules.serviceInterval earlier ≤
      validatorPostGstCausalQueueSampleTime start rules.serviceInterval later := by
  unfold validatorPostGstCausalQueueSampleTime
  exact Nat.add_le_add_left (Nat.mul_le_mul_right rules.serviceInterval ordered)
    start

/-- A high-backlog interval strictly decreases the queue size. -/
theorem high_backlog_strictly_decreases
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (rules : ValidatorPostGstCausalQueueServiceRules timed validator start)
    {interval : Nat}
    (high : rules.cService ≤ (rules.pending interval).length) :
    (rules.pending (interval + 1)).length <
      (rules.pending interval).length := by
  have added := rules.workAddedBound interval
  have removed := rules.highBacklogService interval high
  have balance := rules.queueBalance interval
  have margin := rules.serviceMargin
  omega

/-- The strict service margin eventually reaches a low-backlog interval. -/
private theorem eventually_reaches_low_backlog
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (rules : ValidatorPostGstCausalQueueServiceRules timed validator start)
    (firstInterval : Nat) :
    ∃ lowInterval,
      firstInterval ≤ lowInterval ∧
        (rules.pending lowInterval).length < rules.cService := by
  by_cases low : (rules.pending firstInterval).length < rules.cService
  · exact ⟨firstInterval, Nat.le_refl _, low⟩
  · have high : rules.cService ≤ (rules.pending firstInterval).length := by
      omega
    have decrease := rules.high_backlog_strictly_decreases high
    rcases eventually_reaches_low_backlog rules (firstInterval + 1) with
      ⟨lowInterval, nextBeforeLow, lowAtFinish⟩
    exact ⟨lowInterval, Nat.le_trans (Nat.le_add_right firstInterval 1)
      nextBeforeLow, lowAtFinish⟩
termination_by (rules.pending firstInterval).length
decreasing_by exact decrease

/-- Every reference known at one interval eventually becomes accepted or
GC-obsolete at one common later sample.

The result is stronger than pointwise completion. One finish sample works for
all references that were known at `firstInterval`, including one complete
finite causal history. -/
theorem known_work_eventually_resolved
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (rules : ValidatorPostGstCausalQueueServiceRules timed validator start)
    (firstInterval : Nat) :
    ∃ finishInterval,
      firstInterval ≤ finishInterval ∧
        ∀ reference, rules.known firstInterval reference →
          ValidatorCausalReferenceResolvedAt timed validator
            (validatorPostGstCausalQueueSampleTime start rules.serviceInterval
              finishInterval) reference := by
  rcases eventually_reaches_low_backlog rules firstInterval with
    ⟨lowInterval, firstBeforeLow, low⟩
  refine ⟨lowInterval + 1,
    Nat.le_trans firstBeforeLow (Nat.le_add_right lowInterval 1), ?_⟩
  intro reference knownAtFirst
  have knownAtLow := rules.knownMonotone firstBeforeLow knownAtFirst
  rcases rules.knownAccounted lowInterval reference knownAtLow with
      pending | resolved
  · have knownAtNext := rules.knownMonotone
      (Nat.le_add_right lowInterval 1) knownAtLow
    rcases rules.knownAccounted (lowInterval + 1) reference knownAtNext with
        pendingAtNext | resolvedAtNext
    · exact False.elim
        (rules.lowBacklogClearsOld lowInterval low reference pending
          pendingAtNext)
    · exact resolvedAtNext
  · exact validator_causal_reference_resolved_persists
      rules.validatorInRange (rules.sample_time_mono
        (Nat.le_add_right lowInterval 1)) resolved

/-- A fixed known causal history eventually has every reference accepted or
GC-obsolete at one common trace time. -/
theorem known_causal_history_eventually_resolved
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (rules : ValidatorPostGstCausalQueueServiceRules timed validator start)
    {firstInterval : Nat}
    (history : List (ValidatorBlockRef BlockId))
    (historyKnown : ∀ reference, reference ∈ history →
      rules.known firstInterval reference) :
    ∃ finishTime,
      validatorPostGstCausalQueueSampleTime start rules.serviceInterval
          firstInterval ≤ finishTime ∧
        ∀ reference, reference ∈ history →
          ValidatorCausalReferenceResolvedAt timed validator finishTime
            reference := by
  rcases rules.known_work_eventually_resolved firstInterval with
    ⟨finishInterval, firstBeforeFinish, resolved⟩
  refine ⟨validatorPostGstCausalQueueSampleTime start rules.serviceInterval
      finishInterval, rules.sample_time_mono firstBeforeFinish, ?_⟩
  intro reference referenceInHistory
  exact resolved reference (historyKnown reference referenceInHistory)

/-- The same result for the exact reference list in one current or past causal
capsule. The capsule is finite source data. It is not a future carrier. -/
theorem known_causal_capsule_eventually_resolved
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (rules : ValidatorPostGstCausalQueueServiceRules timed validator start)
    {firstInterval : Nat}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (historyKnown : ∀ block, block ∈ capsule.history →
      rules.known firstInterval block.reference) :
    ∃ finishTime,
      validatorPostGstCausalQueueSampleTime start rules.serviceInterval
          firstInterval ≤ finishTime ∧
        ∀ block, block ∈ capsule.history →
          ValidatorCausalReferenceResolvedAt timed validator finishTime
            block.reference := by
  let references := capsule.history.map (fun block => block.reference)
  have referencesKnown : ∀ reference, reference ∈ references →
      rules.known firstInterval reference := by
    intro reference member
    simp only [references, List.mem_map] at member
    rcases member with ⟨block, blockInHistory, blockReference⟩
    subst reference
    exact historyKnown block blockInHistory
  rcases rules.known_causal_history_eventually_resolved references
      referencesKnown with
    ⟨finishTime, startBeforeFinish, resolved⟩
  refine ⟨finishTime, startBeforeFinish, ?_⟩
  intro block blockInHistory
  exact resolved block.reference (by
    simp only [references, List.mem_map]
    exact ⟨block, blockInHistory, rfl⟩)

/-- One already-present parent-sync source enters the rate-controlled queue and
finishes at one common accepted-or-GC sample.

References which were already accepted or below GC at the first sample stay
ready by durable-state monotonicity. The pointwise admission adapter puts each
remaining history reference in `known`. The queue theorem derives its later
resolution. -/
theorem block_parent_sync_source_eventually_resolved
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (queue : ValidatorPostGstCausalQueueServiceRules timed validator start)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {block : ValidatorBlock BlockId}
    {holder sourceAt firstInterval : Nat}
    {history : List (ValidatorBlock BlockId)}
    {source : ValidatorBlockParentSyncSource syncRules block validator holder
      history sourceAt}
    (admission : ValidatorBlockParentSyncQueueAdmissionAt
      (firstInterval := firstInterval) queue source) :
    ∃ finishTime,
      validatorPostGstCausalQueueSampleTime start queue.serviceInterval
          firstInterval ≤ finishTime ∧
        ∀ historyBlock, historyBlock ∈ history →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution finishTime
            validator historyBlock.reference := by
  rcases queue.known_work_eventually_resolved firstInterval with
    ⟨finishInterval, firstBeforeFinish, knownResolved⟩
  let firstTime := validatorPostGstCausalQueueSampleTime start
    queue.serviceInterval firstInterval
  let finishTime := validatorPostGstCausalQueueSampleTime start
    queue.serviceInterval finishInterval
  have firstBeforeFinishTime : firstTime ≤ finishTime := by
    exact queue.sample_time_mono firstBeforeFinish
  refine ⟨finishTime, firstBeforeFinishTime, ?_⟩
  intro historyBlock blockInHistory
  cases acceptedAtFirst : ((timed.execution.trace firstTime).validatorState
      validator).accepted historyBlock.reference with
  | true =>
      exact Or.inl (timed.execution.accepted_block_persists
        queue.validatorInRange firstBeforeFinishTime acceptedAtFirst)
  | false =>
      by_cases atOrBelowGc : historyBlock.reference.round ≤
          ((timed.execution.trace firstTime).validatorState validator).gcRound
      · have readyAtFirst : ValidatorCausalReferenceResolvedAt timed validator
            firstTime historyBlock.reference := Or.inr atOrBelowGc
        have readyAtFinish := validator_causal_reference_resolved_persists
          queue.validatorInRange firstBeforeFinishTime readyAtFirst
        simpa [ValidatorCausalReferenceResolvedAt,
          ValidatorReferenceAcceptedOrGcRootAt] using readyAtFinish
      · have aboveGc :
            ((timed.execution.trace firstTime).validatorState validator).gcRound <
              historyBlock.reference.round := by
          omega
        have known := admission.unresolvedAboveGcIsKnown historyBlock
          blockInHistory (by simpa [firstTime] using acceptedAtFirst)
            (by simpa [firstTime] using aboveGc)
        have readyAtFinish := knownResolved historyBlock.reference known
        simpa [finishTime, ValidatorCausalReferenceResolvedAt,
          ValidatorReferenceAcceptedOrGcRootAt] using readyAtFinish

end ValidatorPostGstCausalQueueServiceRules

end Mysticeti
