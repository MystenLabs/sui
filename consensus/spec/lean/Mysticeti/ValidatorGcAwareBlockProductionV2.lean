/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorDagProgressComposition
import Mysticeti.ValidatorDynamicCausalQueueAdmission

namespace Mysticeti

/-!
A current-state source boundary for GC-aware fixed-author block production.

This module does not use one ghost capsule for a quorum of unrelated parents.
The selected support is an actual requester-local latch for the exact active
parent need. Each missing selected reference has its own recursive need and
pinned source. A separate projection maps that exact recursive need into the
sampled BlockManager queue. Processing a fetched body uses the same dynamic
direct-parent admission rule for newly found ancestors.

GC obsolescence and commit rebase are different facts. Queue service can make
one reference obsolete because GC reached its round. A parent-need rebase has
an actual commit-install event in that local batch. Recovery readiness first
becomes a current timer input. A later proposal result is derived by the timer
and proposal-worker theorems; it is not a field of a source record.
-/

/-- The exact quorum support stored for one active requester parent need. -/
structure ValidatorRecoverySelectedSupport
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  need : ValidatorRecoveryParentNeed BlockId CommitId config
  parents : List (ValidatorBlockRef BlockId)
  parentsWithinNeed : ∀ parent, parent ∈ parents →
    parent ∈ need.candidateRefs
  parentAuthorsNodup : (parents.map ValidatorBlockRef.author).Nodup
  parentsAreImmediate : ∀ parent, parent ∈ parents →
    parent.round + 1 = need.targetRound
  parentsHaveQuorum : config.thresholds.quorum ≤
    weight config.authorityCount config.stake (validatorParentAuthors parents)

/-- Isolated durable storage for the selected support latch. -/
structure ValidatorRecoverySelectedSupportState
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  active : Option (ValidatorRecoverySelectedSupport BlockId CommitId config)

/-- Current requester-local support selection.

The trace is the implementation state which owns the selected list. A support
can exist only for the exact active need. If one active need is not ready, the
host has already stored one support. Static genesis parents are accepted when
they enter the latch.
-/
structure ValidatorRecoverySelectedSupportExecution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait) where
  trace : Time → Nat →
    ValidatorRecoverySelectedSupportState BlockId CommitId config
  storedSupportMatchesActiveNeed : ∀ time validator support,
    (trace time validator).active = some support →
    (needs.trace time validator).active = some support.need
  storedGenesisParentIsAccepted : ∀ time validator support parent,
    (trace time validator).active = some support →
    parent ∈ support.parents →
    parent.round = 0 →
    ((timed.execution.trace time).validatorState validator).accepted parent =
      true
  unreadyActiveNeedHasStoredSupport : ∀ time validator need,
    (needs.trace time validator).active = some need →
    ¬ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need →
    ∃ support, (trace time validator).active = some support

/-- One actual selected-support latch at one trace state. -/
structure ValidatorRecoverySelectedSupportAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
    (supports : ValidatorRecoverySelectedSupportExecution needs)
    (time validator : Time) where
  support : ValidatorRecoverySelectedSupport BlockId CommitId config
  stored : (supports.trace time validator).active = some support

namespace ValidatorRecoverySelectedSupportAt

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}
variable {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
  protocolPacket network program}
variable {syncRules : ValidatorBlockSyncExecutionRules timed}
variable {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
variable {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
  network program timed waits}
variable {arms : ValidatorRecoveryTimerArmExecution timerSource}
variable {pins : ValidatorRecoverySourcePinExecution syncRules}
variable {recoveryWait : Time}
variable {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
variable {supports : ValidatorRecoverySelectedSupportExecution needs}

/-- The stored selected support names the exact current parent need. -/
theorem needActive
    {time validator : Time}
    (selected : ValidatorRecoverySelectedSupportAt supports time validator) :
    (needs.trace time validator).active = some selected.support.need :=
  supports.storedSupportMatchesActiveNeed time validator selected.support
    selected.stored

/-- A selected static genesis parent is already accepted. -/
theorem genesisAccepted
    {time validator : Time}
    (selected : ValidatorRecoverySelectedSupportAt supports time validator)
    {parent : ValidatorBlockRef BlockId}
    (member : parent ∈ selected.support.parents)
    (roundZero : parent.round = 0) :
    ((timed.execution.trace time).validatorState validator).accepted parent =
      true :=
  supports.storedGenesisParentIsAccepted time validator selected.support parent
    selected.stored member roundZero

end ValidatorRecoverySelectedSupportAt

/-- One exact missing selected parent has one requester-local recursive need
and one concrete correct pinned source.

The source is linked to the active need in one of two ways. A pinned-tip need
uses its own exact capsule key on this requester. A delivered-child need uses
the exact local child which created the recursive parent need. -/
structure ValidatorSelectedParentFetchSourceAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
    {supports : ValidatorRecoverySelectedSupportExecution needs}
    (recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules)
    {time validator : Time}
    (selected : ValidatorRecoverySelectedSupportAt supports time validator)
    (parent : ValidatorBlockRef BlockId) where
  parentSelected : parent ∈ selected.support.parents
  recursiveNeed : ValidatorRecoveryRecursiveParentNeed (BlockId := BlockId)
    config
  recursiveActive :
    (recursive.trace time validator).active parent = some recursiveNeed
  recursiveParentExact : recursiveNeed.parent = parent
  holder : Nat
  holderInRange : holder < config.authorityCount
  holderCorrectAvailable : faults.correctAvailable holder = true
  capsuleKey : ValidatorRecoveryCapsuleKey BlockId
  entry : ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config
  block : ValidatorBlock BlockId
  blockReference : block.reference = parent
  sourceStored : (pins.trace time holder).capsuleAt capsuleKey = some entry
  sourcePinned : (pins.trace time holder).pinned capsuleKey = true
  blockInSource : block ∈ entry.capsule.history
  linkedToNeed :
    (selected.support.need.capsuleKey = some capsuleKey ∧
      holder = validator) ∨
    ∃ child,
      selected.support.need.sourceBlock = some child ∧
        recursiveNeed.child = child

/-- The exact BlockManager projection for one stored selected support.

`sourceProjectsRootNeedToQueue` is the root admission boundary. It consumes a
current recursive need and its current pinned source. `dynamicAdmission`
projects every later local child body to its missing direct parents.
-/
structure ValidatorSelectedSupportQueueAt
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
    {supports : ValidatorRecoverySelectedSupportExecution needs}
    (recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules)
    {time validator : Time}
    (selected : ValidatorRecoverySelectedSupportAt supports time validator) where
  queue : ValidatorPostGstCausalQueueServiceRules timed validator time
  dynamicAdmission : ValidatorDynamicCausalQueueAdmission queue
  sourceFor : ∀ parent,
    parent ∈ selected.support.parents →
    ((timed.execution.trace time).validatorState validator).accepted parent =
      false →
    ((timed.execution.trace time).validatorState validator).gcRound <
      parent.round →
    Nonempty (ValidatorSelectedParentFetchSourceAt recursive selected parent)
  sourceProjectsRootNeedToQueue : ∀ parent
      (source : ValidatorSelectedParentFetchSourceAt recursive selected parent),
    queue.known 0 parent

/-- Current per-support queue construction. No field names a later queue
result, accepted block, proposal, or commit. -/
structure ValidatorSelectedSupportQueueSourceMap
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
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
    (supports : ValidatorRecoverySelectedSupportExecution needs)
    (recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules) :
    Prop where
  queueFor : ∀ time validator
      (selected : ValidatorRecoverySelectedSupportAt supports time validator),
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    network.gst ≤ time →
    (∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true) →
    Nonempty (ValidatorSelectedSupportQueueAt recursive selected)

namespace ValidatorSelectedSupportQueueSourceMap

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
variable {syncRules : ValidatorBlockSyncExecutionRules timed}
variable {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
variable {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
  network program timed waits}
variable {arms : ValidatorRecoveryTimerArmExecution timerSource}
variable {pins : ValidatorRecoverySourcePinExecution syncRules}
variable {recoveryWait : Time}
variable {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
variable {supports : ValidatorRecoverySelectedSupportExecution needs}
variable {recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules}

/-- Every selected root finishes as accepted or GC-obsolete at one common
later time. Root admission uses its actual recursive need. Dynamic admission
is retained in the queue witness for every newly found dependency. -/
theorem selected_support_eventually_resolved
    (sourceMap : ValidatorSelectedSupportQueueSourceMap supports recursive)
    {start validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true)
    (selected : ValidatorRecoverySelectedSupportAt supports start validator) :
    ∃ finish,
      start ≤ finish ∧
        ∀ parent, parent ∈ selected.support.parents →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution finish
            validator parent := by
  classical
  rcases sourceMap.queueFor start validator selected validatorInRange
      validatorCorrectAvailable afterGst active with ⟨queueAt⟩
  rcases queueAt.queue.known_work_eventually_resolved 0 with
    ⟨finishInterval, _zeroBeforeFinish, knownResolved⟩
  let finish := validatorPostGstCausalQueueSampleTime start
    queueAt.queue.serviceInterval finishInterval
  have startBeforeFinish : start ≤ finish := by
    simp [finish, validatorPostGstCausalQueueSampleTime]
  refine ⟨finish, startBeforeFinish, ?_⟩
  intro parent parentSelected
  cases acceptedAtStart :
      ((timed.execution.trace start).validatorState validator).accepted parent
      with
  | true =>
      exact Or.inl (timed.execution.accepted_block_persists validatorInRange
        startBeforeFinish acceptedAtStart)
  | false =>
      by_cases atOrBelowGc : parent.round ≤
          ((timed.execution.trace start).validatorState validator).gcRound
      · exact validator_reference_accepted_or_gc_root_persists
          timed.execution validatorInRange startBeforeFinish (Or.inr atOrBelowGc)
      · have aboveGc :
            ((timed.execution.trace start).validatorState validator).gcRound <
              parent.round := by omega
        let rootSource := Classical.choice
          (queueAt.sourceFor parent parentSelected acceptedAtStart aboveGc)
        have known := queueAt.sourceProjectsRootNeedToQueue parent rootSource
        have resolved := knownResolved parent known
        simpa [finish, ValidatorCausalReferenceResolvedAt,
          ValidatorReferenceAcceptedOrGcRootAt] using resolved

end ValidatorSelectedSupportQueueSourceMap

namespace ValidatorRecoverySelectedSupportAt

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
variable {syncRules : ValidatorBlockSyncExecutionRules timed}
variable {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
variable {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
  network program timed waits}
variable {arms : ValidatorRecoveryTimerArmExecution timerSource}
variable {pins : ValidatorRecoverySourcePinExecution syncRules}
variable {recoveryWait : Time}
variable {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
variable {supports : ValidatorRecoverySelectedSupportExecution needs}
variable {recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules}
variable {obligations : ValidatorProposalObligationExecution timed}

/-- Accepted selected parents make any active accumulator extension of the
latched need ready. The active-need GC fence rules out a positive immediate
parent which is at or below GC. -/
theorem ready_of_active_extension_and_accepted
    {start finish validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (selected : ValidatorRecoverySelectedSupportAt supports start validator)
    {laterNeed : ValidatorRecoveryParentNeed BlockId CommitId config}
    (laterActive : (needs.trace finish validator).active = some laterNeed)
    (extension : ValidatorRecoveryParentNeedAccumulatorExtends
      selected.support.need laterNeed)
    (allAccepted : ∀ parent, parent ∈ selected.support.parents →
      ((timed.execution.trace finish).validatorState validator).accepted
        parent = true) :
    ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace finish).validatorState validator) laterNeed := by
  refine ⟨selected.support.parents, ?_, ?_⟩
  · intro parent member
    exact extension.2.2.2.2 parent
      (selected.support.parentsWithinNeed parent member)
  · refine ⟨⟨selected.support.parentAuthorsNodup, ?_,
      selected.support.parentsHaveQuorum⟩, ?_⟩
    · intro parent member
      exact ⟨by
        rw [extension.2.2.2.1]
        exact selected.support.parentsAreImmediate parent member,
        allAccepted parent member⟩
    · intro parent member
      have candidate : parent ∈ laterNeed.candidateRefs :=
        extension.2.2.2.2 parent
          (selected.support.parentsWithinNeed parent member)
      have permitted : parent.round = 0 ∨
          ((timed.execution.trace finish).validatorState validator).gcRound <
            parent.round := by
        by_cases roundZero : parent.round = 0
        · exact Or.inl roundZero
        · right
          have immediate := selected.support.parentsAreImmediate parent member
          have sameTarget := extension.2.2.2.1
          have fence := needs.activeNeedFencesTargetRound finish validator
            laterNeed laterActive
          rcases fence with ⟨targetOne, gcZero⟩ | targetAboveGc
          · rw [sameTarget] at targetOne
            omega
          · rw [sameTarget] at targetAboveGc
            omega
      have retained := needs.acceptedCandidateIsPinned finish validator
        laterNeed parent laterActive candidate permitted
          (allAccepted parent member)
      exact ⟨retained, permitted⟩

/-- A ready normal parent build is one strict production phase. -/
theorem normal_build_eventually_produces_strict
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (ready : ValidatorNormalParentBuildReadyAt timed start validator) :
    ValidatorStrictPhaseProduction timed start validator := by
  rcases normal_parent_build_ready_eventually_produces_advancing_block
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable ready with
    ⟨targetRound, persistTime, finish, block, startBeforePersist,
      persistBeforeFinish, blockRound, aboveFloor, _persisted, stored, sent,
      _floorAtFinish⟩
  have startBeforeFinish : start ≤ finish := Nat.le_trans startBeforePersist
    (Nat.le_trans (Nat.le_succ _) persistBeforeFinish)
  refine ⟨finish, targetRound, block.reference, startBeforeFinish,
    Nat.le_of_lt aboveFloor, ?_, ?_, ?_⟩
  · exact round_above_signer_floor_is_not_sent validatorInRange aboveFloor
  · simpa [blockRound] using stored
  · simpa [blockRound] using sent

/-- One actual normal selected-support phase produces a sent own block.

Commit installs do not replace normal work. Queue service resolves the fixed
selected roots. If the target was signed first, that concrete block is sent.
Otherwise, the preserved need uses the now-ready selected list.
-/
theorem normal_selected_support_eventually_produces_strict
    (queueSource : ValidatorSelectedSupportQueueSourceMap supports recursive)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true)
    (selected : ValidatorRecoverySelectedSupportAt supports start validator)
    (normalOrigin : selected.support.need.proposalOrigin = .normal) :
    ValidatorStrictPhaseProduction timed start validator := by
  rcases queueSource.selected_support_eventually_resolved validatorInRange
      validatorCorrectAvailable afterGst active selected with
    ⟨resolvedAt, startBeforeResolved, resolved⟩
  obtain ⟨offset, rfl⟩ := Nat.exists_eq_add_of_le startBeforeResolved
  rcases active_normal_parent_need_extends_or_target_reached_after needs
      validatorInRange selected.needActive normalOrigin active offset with
    ⟨laterNeed, laterActive, extension⟩ | reached
  · have allAccepted : ∀ parent, parent ∈ selected.support.parents →
        ((timed.execution.trace (start + offset)).validatorState
          validator).accepted parent = true := by
      intro parent member
      rcases resolved parent member with accepted | atGc
      · exact accepted
      · by_cases roundZero : parent.round = 0
        · exact timed.execution.accepted_block_persists validatorInRange
            (Nat.le_add_right start offset)
            (selected.genesisAccepted member roundZero)
        · have immediate := selected.support.parentsAreImmediate parent member
          have sameTarget := extension.2.2.2.1
          have fence := needs.activeNeedFencesTargetRound (start + offset)
            validator laterNeed laterActive
          rcases fence with ⟨targetOne, gcZero⟩ | targetAboveGc
          · rw [sameTarget] at targetOne
            omega
          · rw [sameTarget] at targetAboveGc
            omega
    have ready := selected.ready_of_active_extension_and_accepted
      validatorInRange laterActive extension allAccepted
    have laterNormal : laterNeed.proposalOrigin = .normal :=
      extension.1.trans normalOrigin
    have buildReady := needs.ready_normal_need_gives_protected_build
      validatorInRange validatorCorrectAvailable laterActive laterNormal ready
    have laterProduction := normal_build_eventually_produces_strict latchSource
      effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable buildReady
    exact strict_phase_production_starts_earlier validatorInRange
      (Nat.le_add_right start offset) laterProduction
  · rcases normal_parent_need_target_reached_eventually_sends_advancing_block
        needs obligations authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable selected.needActive reached with
      ⟨finish, block, startBeforeFinish, aboveFloor, stored, sent⟩
    exact ⟨finish, block.reference.round, block.reference,
      startBeforeFinish, Nat.le_of_lt aboveFloor,
      round_above_signer_floor_is_not_sent validatorInRange aboveFloor,
      stored, sent⟩

end ValidatorRecoverySelectedSupportAt

/-- A deterministic second validator used only to instantiate one addressed
broadcast result. -/
private def gcAwareProductionOtherReceiver (validator : Nat) : Nat :=
  if validator = 0 then 1 else 0

private theorem gc_aware_production_other_receiver_in_range
    {validator authorityCount : Nat}
    (authorityCountAtLeastTwo : 1 < authorityCount) :
    gcAwareProductionOtherReceiver validator < authorityCount := by
  simp only [gcAwareProductionOtherReceiver]
  split
  · exact authorityCountAtLeastTwo
  · omega

private theorem gc_aware_production_other_receiver_is_different
    {validator : Nat} :
    gcAwareProductionOtherReceiver validator ≠ validator := by
  simp only [gcAwareProductionOtherReceiver]
  split <;> omega

/-- An at-or-above addressed broadcast is one strict production phase. -/
theorem validator_at_or_above_broadcast_is_strict_production
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {start validator minimumRound : Time}
    (validatorInRange : validator < config.authorityCount)
    (broadcast : ValidatorAuthorLocalAtOrAboveBroadcastAt timed obligations
      start validator minimumRound) :
    ValidatorStrictPhaseProduction timed start validator := by
  rcases broadcast with ⟨round, _minimum, ⟨⟨result, resultRound⟩⟩⟩
  have startBeforeFinish : start ≤ result.finish :=
    Nat.le_trans result.startBeforePersistence
      (Nat.le_trans (Nat.le_succ _) result.persistenceBeforeFinish)
  have aboveFloor :
      ((timed.execution.trace start).validatorState
        validator).highestSignedRound < round := by
    simpa [resultRound] using result.proposalRoundAboveStartFloor
  refine ⟨result.finish, round, result.proposal.block.reference,
    startBeforeFinish, Nat.le_of_lt aboveFloor,
    round_above_signer_floor_is_not_sent validatorInRange aboveFloor, ?_, ?_⟩
  · simpa [resultRound] using result.ownBlockStoredAtFinish
  · simpa [resultRound] using result.sentOwnBlockAtFinish

/-- A current recovery timer input is a real production phase.

The timer theorem consumes same-author commit races internally. This adapter
only forgets the concrete addressed-broadcast evidence.
-/
theorem validator_current_timer_input_eventually_produces_strict
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {obligations : ValidatorProposalObligationExecution timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {thresholds : ValidatorBlockProgressRecoveryThresholds}
    {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
    (arms : ValidatorRecoveryTimerArmExecution timerSource)
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (input : ValidatorRecoveryTimerCurrentInputAt timed start validator)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true) :
    ValidatorStrictPhaseProduction timed start validator := by
  let receiver := gcAwareProductionOtherReceiver validator
  have broadcast :=
    current_recovery_input_eventually_produces_at_or_above_broadcast arms
      continuation pacing latchSource effects authorityCountAtLeastTwo
        validatorInRange validatorCorrectAvailable
          (gc_aware_production_other_receiver_in_range authorityCountAtLeastTwo)
          gc_aware_production_other_receiver_is_different input active
  exact validator_at_or_above_broadcast_is_strict_production validatorInRange
    broadcast

namespace ValidatorRecoveryParentNeedExecution

/-- One active recovery need has only three active-epoch next states: it keeps
or extends the same recovery work, an actual commit rebase installs normal
work, or the signer has reached its target. -/
theorem active_recovery_need_steps_to_recovery_normal_or_target
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {need}
    (active : (needs.trace time validator).active = some need)
    (recoveryOrigin : need.proposalOrigin = .commitProgressRecovery)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true) :
    (∃ nextNeed,
      (needs.trace (time + 1) validator).active = some nextNeed ∧
        ValidatorRecoveryParentNeedAccumulatorExtends need nextNeed ∧
        nextNeed.proposalOrigin = .commitProgressRecovery) ∨
      (∃ nextNeed,
        (needs.trace (time + 1) validator).active = some nextNeed ∧
          nextNeed.proposalOrigin = .normal) ∨
      need.targetRound ≤
        ((timed.execution.trace (time + 1)).validatorState
          validator).highestSignedRound := by
  have transition := needs.transitionsFollowRules time validator
  rw [activeAfter] at transition
  cases transition <;>
    simp_all [ValidatorRecoveryParentNeedAccumulatorExtends,
      ValidatorRecoveryParentNeedIsRebase]

end ValidatorRecoveryParentNeedExecution

namespace ValidatorRecoverySelectedSupportAt

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
variable {syncRules : ValidatorBlockSyncExecutionRules timed}
variable {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
variable {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
  network program timed waits}
variable {arms : ValidatorRecoveryTimerArmExecution timerSource}
variable {pins : ValidatorRecoverySourcePinExecution syncRules}
variable {recoveryWait : Time}
variable {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
variable {supports : ValidatorRecoverySelectedSupportExecution needs}
variable {obligations : ValidatorProposalObligationExecution timed}
variable {recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules}
variable {thresholds : ValidatorBlockProgressRecoveryThresholds}
variable {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}
variable {thresholds : ValidatorBlockProgressRecoveryThresholds}
variable {mode : ValidatorBlockProgressRecoveryModeExecution timed thresholds}

/-- Accepted selected recovery parents first create the real current timer
input. The timer and proposal workers then derive production. -/
theorem accepted_recovery_support_eventually_produces_strict
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : ∀ later, time ≤ later →
      (timed.execution.trace later).epochActive = true)
    (selected : ValidatorRecoverySelectedSupportAt supports time validator)
    (recoveryOrigin :
      selected.support.need.proposalOrigin = .commitProgressRecovery)
    (allAccepted : ∀ parent, parent ∈ selected.support.parents →
      ((timed.execution.trace time).validatorState validator).accepted parent =
        true) :
    ValidatorStrictPhaseProduction timed time validator := by
  have extension : ValidatorRecoveryParentNeedAccumulatorExtends
      selected.support.need selected.support.need :=
    ValidatorRecoveryParentNeedExecution.accumulator_extends_refl _
  have ready := selected.ready_of_active_extension_and_accepted
    validatorInRange selected.needActive extension allAccepted
  rcases ready with ⟨parents, _parentsWithinNeed, parentsReady⟩
  have mainFacts := needs.activeNeedMatchesMain time validator
    selected.support.need selected.needActive
  have targetExact :=
    selected.support.need.recoveryTargetIsExactNext recoveryOrigin
  have targetCurrent : selected.support.need.targetRound =
      ((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1 := by
    rw [← mainFacts.2.1]
    exact targetExact
  have quorumReady : ValidatorRecoveryParentQuorumReadyAt config
      ((timed.execution.trace time).validatorState validator)
      (((timed.execution.trace time).validatorState
        validator).highestSignedRound + 1) := by
    refine ⟨parents, ?_⟩
    simpa [recoveryOrigin, targetCurrent] using parentsReady
  have currentInput :=
    timerSource.active_parent_quorum_state_gives_current_timer_input time
      validator (active time (Nat.le_refl _)) validatorInRange
        validatorCorrectAvailable quorumReady
  exact validator_current_timer_input_eventually_produces_strict arms
    continuation pacing latchSource effects authorityCountAtLeastTwo
      validatorInRange validatorCorrectAvailable currentInput active

/-- One actual recovery selected-support phase produces a sent own block.

The first selected list stays useful while the recovery head is unchanged. If
an actual commit rebases the need, the proof switches to the current normal
need and obtains its current stored support. No future rebase, timer result, or
proposal is an input.
-/
theorem recovery_selected_support_eventually_produces_strict
    {recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules}
    (queueSource : ValidatorSelectedSupportQueueSourceMap supports recursive)
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true)
    (selected : ValidatorRecoverySelectedSupportAt supports start validator)
    (recoveryOrigin :
      selected.support.need.proposalOrigin = .commitProgressRecovery) :
    ValidatorStrictPhaseProduction timed start validator := by
  rcases queueSource.selected_support_eventually_resolved validatorInRange
      validatorCorrectAvailable afterGst active selected with
    ⟨resolvedAt, startBeforeResolved, resolved⟩
  obtain ⟨resolvedOffset, rfl⟩ :=
    Nat.exists_eq_add_of_le startBeforeResolved
  have target_reached_gives_production : ∀
      {phaseAt : Time}
      {need : ValidatorRecoveryParentNeed BlockId CommitId config},
      start ≤ phaseAt →
      (needs.trace phaseAt validator).active = some need →
      need.targetRound ≤
        ((timed.execution.trace (phaseAt + 1)).validatorState
          validator).highestSignedRound →
      ValidatorStrictPhaseProduction timed start validator := by
    intro phaseAt need startBeforePhase activeNeed reached
    rcases normal_parent_need_target_reached_eventually_sends_advancing_block
        needs obligations authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable activeNeed (offset := 1) reached with
      ⟨finish, block, phaseBeforeFinish, aboveFloor, stored, sent⟩
    have phaseProduction : ValidatorStrictPhaseProduction timed phaseAt
        validator :=
      ⟨finish, block.reference.round, block.reference, phaseBeforeFinish,
        Nat.le_of_lt aboveFloor,
        round_above_signer_floor_is_not_sent validatorInRange aboveFloor,
        stored, sent⟩
    exact strict_phase_production_starts_earlier validatorInRange
      startBeforePhase phaseProduction
  have walk : ∀ count,
      (∃ currentNeed,
        (needs.trace (start + count) validator).active = some currentNeed ∧
          ValidatorRecoveryParentNeedAccumulatorExtends
            selected.support.need currentNeed ∧
          currentNeed.proposalOrigin = .commitProgressRecovery) ∨
        (∃ currentNeed,
          (needs.trace (start + count) validator).active = some currentNeed ∧
            currentNeed.proposalOrigin = .normal) ∨
        ValidatorStrictPhaseProduction timed start validator := by
    intro count
    induction count with
    | zero =>
        left
        exact ⟨selected.support.need, by simpa using selected.needActive,
          ValidatorRecoveryParentNeedExecution.accumulator_extends_refl _,
          recoveryOrigin⟩
    | succ count inductionHypothesis =>
        rcases inductionHypothesis with recoveryCurrent | normalOrProduction
        · rcases recoveryCurrent with
            ⟨currentNeed, currentActive, initialExtends, currentRecovery⟩
          have nextEpoch :
              (timed.execution.trace ((start + count) + 1)).epochActive =
                true := by
            apply active
            exact Nat.le_trans (Nat.le_add_right start count) (Nat.le_succ _)
          rcases needs.active_recovery_need_steps_to_recovery_normal_or_target
              currentActive currentRecovery nextEpoch with
            ⟨nextNeed, nextActive, currentExtends, nextRecovery⟩ |
              normalOrReached
          · left
            refine ⟨nextNeed, ?_,
              ValidatorRecoveryParentNeedExecution.accumulator_extends_trans
                initialExtends currentExtends, nextRecovery⟩
            simpa [Nat.add_assoc] using nextActive
          · rcases normalOrReached with
              ⟨nextNeed, nextActive, nextNormal⟩ | reached
            · right
              left
              exact ⟨nextNeed, by simpa [Nat.add_assoc] using nextActive,
                nextNormal⟩
            · right
              right
              exact target_reached_gives_production
                (Nat.le_add_right start count) currentActive reached
        · rcases normalOrProduction with normalCurrent | produced
          · rcases normalCurrent with
              ⟨currentNeed, currentActive, currentNormal⟩
            have nextEpoch :
                (timed.execution.trace ((start + count) + 1)).epochActive =
                  true := by
              apply active
              exact Nat.le_trans (Nat.le_add_right start count) (Nat.le_succ _)
            rcases active_normal_parent_need_survives_one_step_or_target_reached
                needs currentActive currentNormal nextEpoch with
              ⟨nextNeed, nextActive, currentExtends⟩ | reached
            · right
              left
              refine ⟨nextNeed, by simpa [Nat.add_assoc] using nextActive,
                ?_⟩
              exact currentExtends.1.trans currentNormal
            · right
              right
              exact target_reached_gives_production
                (Nat.le_add_right start count) currentActive reached
          · exact Or.inr (Or.inr produced)
  rcases walk resolvedOffset with recoveryCurrent | normalOrProduction
  · rcases recoveryCurrent with
      ⟨currentNeed, currentActive, extension, currentRecovery⟩
    have allAccepted : ∀ parent, parent ∈ selected.support.parents →
        ((timed.execution.trace (start + resolvedOffset)).validatorState
          validator).accepted parent = true := by
      intro parent member
      rcases resolved parent member with accepted | atGc
      · exact accepted
      · by_cases roundZero : parent.round = 0
        · exact timed.execution.accepted_block_persists validatorInRange
            (Nat.le_add_right start resolvedOffset)
            (selected.genesisAccepted member roundZero)
        · have immediate := selected.support.parentsAreImmediate parent member
          have sameTarget := extension.2.2.2.1
          have fence := needs.activeNeedFencesTargetRound
            (start + resolvedOffset) validator currentNeed currentActive
          rcases fence with ⟨targetOne, gcZero⟩ | targetAboveGc
          · rw [sameTarget] at targetOne
            omega
          · rw [sameTarget] at targetAboveGc
            omega
    have ready := selected.ready_of_active_extension_and_accepted
      validatorInRange currentActive extension allAccepted
    rcases ready with ⟨parents, _within, parentsReady⟩
    have mainFacts := needs.activeNeedMatchesMain (start + resolvedOffset)
      validator currentNeed currentActive
    have targetExact := currentNeed.recoveryTargetIsExactNext currentRecovery
    have targetCurrent : currentNeed.targetRound =
        ((timed.execution.trace (start + resolvedOffset)).validatorState
          validator).highestSignedRound + 1 := by
      rw [← mainFacts.2.1]
      exact targetExact
    have quorumReady : ValidatorRecoveryParentQuorumReadyAt config
        ((timed.execution.trace (start + resolvedOffset)).validatorState
          validator)
        (((timed.execution.trace (start + resolvedOffset)).validatorState
          validator).highestSignedRound + 1) := by
      refine ⟨parents, ?_⟩
      simpa [currentRecovery, targetCurrent] using parentsReady
    have currentInput :=
      timerSource.active_parent_quorum_state_gives_current_timer_input
        (start + resolvedOffset) validator
        (active _ (Nat.le_add_right start resolvedOffset)) validatorInRange
          validatorCorrectAvailable quorumReady
    have laterProduction :=
      validator_current_timer_input_eventually_produces_strict arms
        continuation pacing latchSource effects authorityCountAtLeastTwo
          validatorInRange validatorCorrectAvailable currentInput
          (by
            intro later resolvedBeforeLater
            exact active later (Nat.le_trans
              (Nat.le_add_right start resolvedOffset) resolvedBeforeLater))
    exact strict_phase_production_starts_earlier validatorInRange
      (Nat.le_add_right start resolvedOffset) laterProduction
  · rcases normalOrProduction with normalCurrent | produced
    · rcases normalCurrent with
        ⟨currentNeed, currentActive, currentNormal⟩
      by_cases ready : ValidatorRecoveryParentNeedReadyAt
          ((timed.execution.trace (start + resolvedOffset)).validatorState
            validator) currentNeed
      · have buildReady := needs.ready_normal_need_gives_protected_build
          validatorInRange validatorCorrectAvailable currentActive
            currentNormal ready
        have laterProduction := normal_build_eventually_produces_strict
          latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable buildReady
        exact strict_phase_production_starts_earlier validatorInRange
          (Nat.le_add_right start resolvedOffset) laterProduction
      · rcases supports.unreadyActiveNeedHasStoredSupport
            (start + resolvedOffset) validator currentNeed currentActive ready with
          ⟨nextSupport, nextStored⟩
        let nextSelected : ValidatorRecoverySelectedSupportAt supports
            (start + resolvedOffset) validator := ⟨nextSupport, nextStored⟩
        have nextActive := nextSelected.needActive
        have sameNeed : nextSupport.need = currentNeed := by
          rw [currentActive] at nextActive
          exact (Option.some.inj nextActive).symm
        have nextNormal : nextSupport.need.proposalOrigin = .normal := by
          rw [sameNeed]
          exact currentNormal
        have laterProduction :=
          nextSelected.normal_selected_support_eventually_produces_strict
            queueSource latchSource effects authorityCountAtLeastTwo
              validatorInRange validatorCorrectAvailable
              (Nat.le_trans afterGst (Nat.le_add_right start resolvedOffset))
              (by
                intro later resolvedBeforeLater
                exact active later (Nat.le_trans
                  (Nat.le_add_right start resolvedOffset) resolvedBeforeLater))
              nextNormal
        exact strict_phase_production_starts_earlier validatorInRange
          (Nat.le_add_right start resolvedOffset) laterProduction
    · exact produced

end ValidatorRecoverySelectedSupportAt

/-- A recorded parent-need rebase has a real same-host commit install. GC
obsolescence alone is not a rebase source. -/
structure ValidatorCommitOnlyParentNeedRebaseAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (time validator : Time) where
  before : ValidatorRecoveryParentNeed BlockId CommitId config
  after : ValidatorRecoveryParentNeed BlockId CommitId config
  head : ValidatorCommitHead CommitId
  beforeActive : (needs.trace time validator).active = some before
  beforeRecovery : before.proposalOrigin = .commitProgressRecovery
  rebaseRecorded : (needs.event time validator).rebaseNeed = some after
  installed : ValidatorCommitInstallOccurs (timed.execution.events time)
    validator head
  isRebase : ValidatorRecoveryParentNeedIsRebase before after
    ((timed.execution.trace (time + 1)).validatorState validator).commitHead
  freshNormal : ValidatorFreshNormalAccumulatorNeedSourceAt timed (time + 1)
    validator after

namespace ValidatorRecoveryParentNeedExecution

/-- The existing requester transition rules derive the commit-only rebase
witness from the current recorded event. -/
theorem recorded_rebase_has_commit_only_source
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    {time validator : Time} {rebased}
    (recorded : (needs.event time validator).rebaseNeed = some rebased) :
    Nonempty (ValidatorCommitOnlyParentNeedRebaseAt needs time validator) := by
  rcases needs.rebasedNeedHasCommitSource time validator rebased recorded with
    ⟨need, head, active, recovery, installed, isRebase, fresh⟩
  exact ⟨⟨need, rebased, head, active, recovery, recorded, installed,
    isRebase, fresh⟩⟩

end ValidatorRecoveryParentNeedExecution

/-! ## Current no-idle phase boundary -/

/-- Current proposal or timer work which does not require a pending parent
fetch. Every constructor contains only current trace state or a protected
current action. -/
inductive ValidatorGcAwareImmediateProposalPhaseAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (obligations : ValidatorProposalObligationExecution timed)
    (time validator : Time) : Prop where
  | ready {round : Nat} {proposal : ValidatorReadyProposal BlockId} :
      (obligations.trace time validator).readyProposal = some proposal →
      proposal.block.reference.round = round →
      ValidatorGcAwareImmediateProposalPhaseAt obligations time validator
  | exactNext (proposal : ValidatorExactNextProposalAt timed time validator) :
      ValidatorGcAwareImmediateProposalPhaseAt obligations time validator
  | normal (proposal : ValidatorNormalProposalAt timed time validator) :
      ValidatorGcAwareImmediateProposalPhaseAt obligations time validator
  | normalBuild
      (build : ValidatorNormalParentBuildReadyAt timed time validator) :
      ValidatorGcAwareImmediateProposalPhaseAt obligations time validator
  | persistedUnsent
      {round : Nat} {reference : ValidatorBlockRef BlockId} {receiver : Nat} :
      ((timed.execution.trace time).validatorState validator).highestSignedRound =
        round →
      ((timed.execution.trace time).validatorState validator).ownBlockAt round =
        some reference →
      ((timed.execution.trace time).validatorState validator).sentOwnBlockAt
          round = false →
      receiver < config.authorityCount →
      receiver ≠ validator →
      (obligations.trace time validator).sendGoal reference receiver = true →
      ValidatorGcAwareImmediateProposalPhaseAt obligations time validator
  | recoveryTimer
      (input : ValidatorRecoveryTimerCurrentInputAt timed time validator) :
      ValidatorGcAwareImmediateProposalPhaseAt obligations time validator

/-- One current strict V2 phase. A parent-fetch phase contains the actual
selected-support latch. Every other phase contains current local work. -/
inductive ValidatorGcAwareStrictProposalPhaseAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
    (supports : ValidatorRecoverySelectedSupportExecution needs)
    (obligations : ValidatorProposalObligationExecution timed)
    (time validator : Time) : Prop where
  | immediate
      (phase : ValidatorGcAwareImmediateProposalPhaseAt obligations time
        validator) :
      ValidatorGcAwareStrictProposalPhaseAt supports obligations time validator
  | selected
      (support : ValidatorRecoverySelectedSupportAt supports time validator) :
      ValidatorGcAwareStrictProposalPhaseAt supports obligations time validator

/-- The only remaining no-idle implementation map.

If the host has no active parent need, the current proposal engine has one
ready, protected, persisted-send, or current-timer action. Active parent needs
do not appear in this field: the V2 support, normal-build, and timer rules
derive their phases below. -/
structure ValidatorGcAwareNoIdleSourceMap
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits}
    {arms : ValidatorRecoveryTimerArmExecution timerSource}
    {pins : ValidatorRecoverySourcePinExecution syncRules}
    {recoveryWait : Time}
    (needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait)
    (obligations : ValidatorProposalObligationExecution timed) : Prop where
  noActiveNeedHasImmediatePhase : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace time).epochActive = true →
    (needs.trace time validator).active = none →
    ValidatorGcAwareImmediateProposalPhaseAt obligations time validator

namespace ValidatorGcAwareNoIdleSourceMap

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
variable {syncRules : ValidatorBlockSyncExecutionRules timed}
variable {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
variable {timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
  network program timed waits}
variable {arms : ValidatorRecoveryTimerArmExecution timerSource}
variable {pins : ValidatorRecoverySourcePinExecution syncRules}
variable {recoveryWait : Time}
variable {needs : ValidatorRecoveryParentNeedExecution pins arms recoveryWait}
variable {supports : ValidatorRecoverySelectedSupportExecution needs}
variable {obligations : ValidatorProposalObligationExecution timed}

/-- Every active parent need already has a V2 phase. An unready need has its
stored support. A ready normal need has a protected build. A ready recovery
need has a real current timer input. -/
theorem active_need_has_phase
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (epochActive : (timed.execution.trace time).epochActive = true)
    {need : ValidatorRecoveryParentNeed BlockId CommitId config}
    (needActive : (needs.trace time validator).active = some need) :
    ValidatorGcAwareStrictProposalPhaseAt supports obligations time
      validator := by
  by_cases ready : ValidatorRecoveryParentNeedReadyAt
      ((timed.execution.trace time).validatorState validator) need
  · cases origin : need.proposalOrigin with
    | normal =>
        have build := needs.ready_normal_need_gives_protected_build
          validatorInRange validatorCorrectAvailable needActive origin ready
        exact .immediate (.normalBuild build)
    | commitProgressRecovery =>
        rcases ready with ⟨parents, _parentsWithinNeed, parentsReady⟩
        have mainFacts := needs.activeNeedMatchesMain time validator need
          needActive
        have targetExact := need.recoveryTargetIsExactNext origin
        have targetCurrent : need.targetRound =
            ((timed.execution.trace time).validatorState
              validator).highestSignedRound + 1 := by
          rw [← mainFacts.2.1]
          exact targetExact
        have quorumReady : ValidatorRecoveryParentQuorumReadyAt config
            ((timed.execution.trace time).validatorState validator)
            (((timed.execution.trace time).validatorState
              validator).highestSignedRound + 1) := by
          refine ⟨parents, ?_⟩
          simpa [origin, targetCurrent] using parentsReady
        have input :=
          timerSource.active_parent_quorum_state_gives_current_timer_input time
            validator epochActive validatorInRange validatorCorrectAvailable
              quorumReady
        exact .immediate (.recoveryTimer input)
  · rcases supports.unreadyActiveNeedHasStoredSupport time validator need
        needActive ready with ⟨support, stored⟩
    exact .selected ⟨support, stored⟩

/-- The one no-active-need map and the derived active-need cases cover every
active correct host. -/
theorem active_host_has_phase
    (source : ValidatorGcAwareNoIdleSourceMap needs obligations)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (epochActive : (timed.execution.trace time).epochActive = true) :
    ValidatorGcAwareStrictProposalPhaseAt supports obligations time
      validator := by
  cases activeNeed : (needs.trace time validator).active with
  | none =>
      exact .immediate (source.noActiveNeedHasImmediatePhase time validator
        validatorInRange validatorCorrectAvailable epochActive activeNeed)
  | some need =>
      exact active_need_has_phase validatorInRange validatorCorrectAvailable
        epochActive activeNeed

/-- Every current immediate phase produces one strict own-block send. The
recovery constructor consumes the current timer input, including an exact
timer which is already stored. -/
theorem immediate_phase_eventually_produces_strict
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true)
    (phase : ValidatorGcAwareImmediateProposalPhaseAt obligations start
      validator) :
    ValidatorStrictPhaseProduction timed start validator := by
  cases phase with
  | ready ready sameRound =>
      rename_i round proposal
      let receiver := gcAwareProductionOtherReceiver validator
      rcases proposal_work_eventually_produces_sent_block_with_peer
          validatorInRange validatorCorrectAvailable receiver
          (gc_aware_production_other_receiver_in_range authorityCountAtLeastTwo)
          gc_aware_production_other_receiver_is_different
          (.ready proposal ready sameRound) with
        ⟨finish, reference, startBeforeFinish, stored, sent⟩
      have legal := obligations.readyProposalIsLegal start validator proposal
        ready
      have aboveFloor :
          ((timed.execution.trace start).validatorState
            validator).highestSignedRound < round := by
        simpa [sameRound] using legal.2.1
      exact ⟨finish, round, reference, startBeforeFinish,
        Nat.le_of_lt aboveFloor,
        round_above_signer_floor_is_not_sent validatorInRange aboveFloor,
        stored, sent⟩
  | exactNext proposal =>
      have production :=
        protected_exact_next_eventually_produces_exact_block_from_authority_count
          latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable proposal
      exact exact_next_production_is_strict validatorInRange production
  | normal proposal =>
      rcases protected_normal_proposal_eventually_produces_advancing_block
          latchSource effects authorityCountAtLeastTwo validatorInRange
            validatorCorrectAvailable proposal with
        ⟨persistTime, finish, block, startBeforePersist, persistBeforeFinish,
          blockRound, aboveFloor, _persisted, stored, sent, _floorAtFinish⟩
      have startBeforeFinish : start ≤ finish := Nat.le_trans
        startBeforePersist (Nat.le_trans (Nat.le_succ _) persistBeforeFinish)
      have blockAboveFloor :
          ((timed.execution.trace start).validatorState
            validator).highestSignedRound < block.reference.round := by
        simpa [blockRound] using aboveFloor
      exact ⟨finish, block.reference.round, block.reference,
        startBeforeFinish, Nat.le_of_lt blockAboveFloor,
        round_above_signer_floor_is_not_sent validatorInRange blockAboveFloor,
        stored, sent⟩
  | normalBuild build =>
      exact ValidatorRecoverySelectedSupportAt.normal_build_eventually_produces_strict
        latchSource effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable build
  | persistedUnsent sameFloor stored notSent receiverInRange receiverIsOther
      sendGoal =>
      rename_i round reference receiver
      rcases proposal_work_eventually_produces_sent_block_with_peer
          validatorInRange validatorCorrectAvailable receiver receiverInRange
          receiverIsOther
          (.persistedUnsent reference receiver sameFloor stored notSent
            receiverInRange receiverIsOther sendGoal) with
        ⟨finish, resultReference, startBeforeFinish, resultStored,
          resultSent⟩
      exact ⟨finish, round, resultReference, startBeforeFinish,
        Nat.le_of_eq sameFloor, notSent, resultStored, resultSent⟩
  | recoveryTimer input =>
      exact validator_current_timer_input_eventually_produces_strict arms
        continuation pacing latchSource effects authorityCountAtLeastTwo
          validatorInRange validatorCorrectAvailable input active

/-- Every selected-support phase produces one strict own-block send. Its
current need origin selects the normal or recovery proof. -/
theorem selected_phase_eventually_produces_strict
    (queueSource : ValidatorSelectedSupportQueueSourceMap supports recursive)
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true)
    (selected : ValidatorRecoverySelectedSupportAt supports start validator) :
    ValidatorStrictPhaseProduction timed start validator := by
  cases origin : selected.support.need.proposalOrigin with
  | normal =>
      exact selected.normal_selected_support_eventually_produces_strict
        queueSource latchSource effects authorityCountAtLeastTwo
          validatorInRange validatorCorrectAvailable afterGst active origin
  | commitProgressRecovery =>
      exact selected.recovery_selected_support_eventually_produces_strict
        queueSource continuation pacing latchSource effects
          authorityCountAtLeastTwo validatorInRange validatorCorrectAvailable
            afterGst active origin

/-- Every current V2 phase produces one strict own-block send. -/
theorem phase_eventually_produces_strict
    (queueSource : ValidatorSelectedSupportQueueSourceMap supports recursive)
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount)
    {start validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ later, start ≤ later →
      (timed.execution.trace later).epochActive = true)
    (phase : ValidatorGcAwareStrictProposalPhaseAt supports obligations start
      validator) :
    ValidatorStrictPhaseProduction timed start validator := by
  cases phase with
  | immediate immediate =>
      exact immediate_phase_eventually_produces_strict continuation pacing
        latchSource effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable active immediate
  | selected selected =>
      exact selected_phase_eventually_produces_strict queueSource continuation
        pacing latchSource effects authorityCountAtLeastTwo validatorInRange
          validatorCorrectAvailable afterGst active selected

/-- The V2 source package gives unbounded fixed-validator block production.
All parent-need cases are derived. The only host scheduler input is the
no-active-need current-action map. -/
theorem block_production_liveness
    (queueSource : ValidatorSelectedSupportQueueSourceMap supports recursive)
    (noIdle : ValidatorGcAwareNoIdleSourceMap needs obligations)
    (continuation : ValidatorAuthorLocalCommitContinuationRules
      (obligations := obligations) (arms := arms) mode)
    (pacing : ValidatorCommitProgressProposalPacingRules timerSource)
    (latchSource : ValidatorProposalLatchSourceMap obligations)
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (authorityCountAtLeastTwo : 1 < config.authorityCount) :
    BlockProductionLiveness config faults network timed.execution.trace := by
  intro validator start minimumRound validatorInRange validatorCorrectAvailable
    afterGst active
  have produceFrom : ∀ current,
      start ≤ current →
      ValidatorStrictPhaseProduction timed current validator := by
    intro current startBeforeCurrent
    have phase := noIdle.active_host_has_phase (supports := supports)
      validatorInRange validatorCorrectAvailable
        (active current startBeforeCurrent)
    exact phase_eventually_produces_strict queueSource continuation pacing
      latchSource effects authorityCountAtLeastTwo validatorInRange
        validatorCorrectAvailable (Nat.le_trans afterGst startBeforeCurrent)
      (by
        intro later currentBeforeLater
        exact active later
          (Nat.le_trans startBeforeCurrent currentBeforeLater))
      phase
  have iterate : ∀ count current oldRound oldReference,
      start ≤ current →
      ((timed.execution.trace current).validatorState validator).ownBlockAt
          oldRound = some oldReference →
      ((timed.execution.trace current).validatorState validator).sentOwnBlockAt
          oldRound = true →
      ∃ finish round reference,
        current ≤ finish ∧
          oldRound + count + 1 ≤ round ∧
          ((timed.execution.trace finish).validatorState validator).ownBlockAt
              round = some reference ∧
          ((timed.execution.trace finish).validatorState
            validator).sentOwnBlockAt round = true := by
    intro count
    induction count with
    | zero =>
        intro current oldRound oldReference startBeforeCurrent oldStored oldSent
        rcases sent_block_precedes_strict_phase_production validatorInRange
            (Nat.le_refl _) oldStored oldSent
            (produceFrom current startBeforeCurrent) with
          ⟨finish, round, reference, currentBeforeFinish, higher, stored,
            sent⟩
        exact ⟨finish, round, reference, currentBeforeFinish, by omega,
          stored, sent⟩
    | succ count inductionHypothesis =>
        intro current oldRound oldReference startBeforeCurrent oldStored oldSent
        rcases sent_block_precedes_strict_phase_production validatorInRange
            (Nat.le_refl _) oldStored oldSent
            (produceFrom current startBeforeCurrent) with
          ⟨middle, middleRound, middleReference, currentBeforeMiddle, higher,
            middleStored, middleSent⟩
        rcases inductionHypothesis middle middleRound middleReference
            (Nat.le_trans startBeforeCurrent currentBeforeMiddle) middleStored
            middleSent with
          ⟨finish, round, reference, middleBeforeFinish, roundBound, stored,
            sent⟩
        refine ⟨finish, round, reference,
          Nat.le_trans currentBeforeMiddle middleBeforeFinish, ?_, stored,
            sent⟩
        omega
  rcases produceFrom start (Nat.le_refl _) with
    ⟨firstFinish, firstRound, firstReference, startBeforeFirst,
      firstFloorBelow, _firstNotSent, firstStored, firstSent⟩
  rcases iterate minimumRound firstFinish firstRound firstReference
      startBeforeFirst firstStored firstSent with
    ⟨finish, round, reference, firstBeforeFinish, roundBound, stored, sent⟩
  refine ⟨finish, round, Nat.le_trans startBeforeFirst firstBeforeFinish,
    ?_, ?_, ?_, sent⟩
  · omega
  · omega
  · simp [stored]

end ValidatorGcAwareNoIdleSourceMap

end Mysticeti
