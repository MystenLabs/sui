/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorBlockSyncBridge
import Mysticeti.ValidatorProposalLatchBridge

namespace Mysticeti

/-! One-validator storage pins for recovery capsules.

A pin starts from a local durable proposal or from the validator's durable tip
at trace time zero. The pin stays active until the epoch ends. Requester state
and a local commit advance cannot release it.
-/

/-- The target reference and commit index give one deterministic capsule key. -/
abbrev ValidatorRecoveryCapsuleKey (BlockId : Type) :=
  ValidatorBlockRef BlockId × Nat

/-- One capsule and the local commit head that opened its storage pin. -/
structure ValidatorPinnedRecoveryCapsule
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId) where
  baselineCommit : ValidatorCommitHead CommitId
  capsule : CausalRecoveryCapsule (BlockId := BlockId) config

/-- Isolated local storage and rebroadcast work for one validator. -/
structure ValidatorRecoverySourcePinState
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  capsuleAt : ValidatorRecoveryCapsuleKey BlockId →
    Option (ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config)
  pinned : ValidatorRecoveryCapsuleKey BlockId → Bool

/-- One batch of local source-pin inputs. -/
structure ValidatorRecoverySourcePinEvent
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  addCapsule : ValidatorRecoveryCapsuleKey BlockId →
    Option (ValidatorPinnedRecoveryCapsule (BlockId := BlockId) config)
  releaseCapsule : ValidatorRecoveryCapsuleKey BlockId → Bool

/-- One exact local source-pin state change. -/
structure ValidatorRecoverySourcePinTransition
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (epochActiveAfter : Bool)
    (before : ValidatorRecoverySourcePinState BlockId CommitId config)
    (event : ValidatorRecoverySourcePinEvent BlockId CommitId config)
    (after : ValidatorRecoverySourcePinState BlockId CommitId config) :
    Prop where
  capsuleUpdateExact : ∀ capsuleId entry,
    after.capsuleAt capsuleId = some entry ↔
      before.capsuleAt capsuleId = some entry ∨
        event.addCapsule capsuleId = some entry
  pinUpdateExact : ∀ capsuleId,
    after.pinned capsuleId = true ↔
      (before.pinned capsuleId = true ∨
        (event.addCapsule capsuleId).isSome = true) ∧
      event.releaseCapsule capsuleId = false
  activeEpochPreventsRelease : epochActiveAfter = true →
    ∀ capsuleId, event.releaseCapsule capsuleId = false

/-- A block occurs in one stored capsule history. -/
def ValidatorPinnedCapsuleContains
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (state : ValidatorRecoverySourcePinState BlockId CommitId config)
    (capsuleId : ValidatorRecoveryCapsuleKey BlockId)
    (block : ValidatorBlock BlockId) : Prop :=
  ∃ entry,
    state.capsuleAt capsuleId = some entry ∧
      block ∈ entry.capsule.history

/-- One source-local pin execution mapped to the main validator trace. -/
structure ValidatorRecoverySourcePinExecution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed) where
  trace : Time → Nat →
    ValidatorRecoverySourcePinState BlockId CommitId config
  event : Time → Nat →
    ValidatorRecoverySourcePinEvent BlockId CommitId config
  transitionsFollowRules : ∀ time validator,
    ValidatorRecoverySourcePinTransition config
      (timed.execution.trace (time + 1)).epochActive
      (trace time validator) (event time validator)
      (trace (time + 1) validator)
  /-- Each stored entry uses its target reference and baseline commit index as
  its exact key. -/
  storedCapsuleKeyExact : ∀ time validator capsuleId entry,
    (trace time validator).capsuleAt capsuleId = some entry →
    capsuleId =
      (entry.capsule.targetBlock.reference, entry.baselineCommit.index)
  /-- Each new entry uses the same deterministic key before storage. -/
  addedCapsuleKeyExact : ∀ time validator capsuleId entry,
    (event time validator).addCapsule capsuleId = some entry →
    capsuleId =
      (entry.capsule.targetBlock.reference, entry.baselineCommit.index)
  /-- All correct source capsules use this static genesis parent list. -/
  canonicalGenesisParents : List (ValidatorBlockRef BlockId)
  correctCapsuleUsesCanonicalGenesis : ∀ time validator capsuleId entry,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (trace time validator).capsuleAt capsuleId = some entry →
    entry.capsule.genesisParents = canonicalGenesisParents
  /-- Each added capsule comes from proposal persistence or from atomic local
  repinning after a commit install. -/
  addedCapsuleHasLocalSource : ∀ time validator capsuleId entry,
    (event time validator).addCapsule capsuleId = some entry →
    ((∃ block,
        ValidatorLocalActionOccurs (timed.execution.events time) validator
            (.persistProposal block) ∧
          entry.capsule.targetBlock = block) ∨
      (∃ head block,
        ValidatorCommitInstallOccurs (timed.execution.events time) validator
            head ∧
          ((timed.execution.trace (time + 1)).validatorState
            validator).ownBlockAt
              ((timed.execution.trace (time + 1)).validatorState
                validator).highestSignedRound = some block.reference ∧
          entry.capsule.targetBlock = block)) ∧
      entry.baselineCommit =
        ((timed.execution.trace (time + 1)).validatorState
          validator).commitHead
  /-- Each persisted proposal adds one exact finite capsule in the same batch. -/
  persistedProposalAddsCapsule : ∀ time validator block,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace (time + 1)).epochActive = true →
    ValidatorLocalActionOccurs (timed.execution.events time) validator
      (.persistProposal block) →
    ∃ capsuleId entry,
      (event time validator).addCapsule capsuleId = some entry ∧
        entry.capsule.targetBlock = block ∧
        entry.baselineCommit =
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead
  /-- A commit install atomically repins the current positive durable tip under
  the new local commit head. -/
  commitInstallRepinsCurrentTip : ∀ time validator head,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace (time + 1)).epochActive = true →
    ValidatorCommitInstallOccurs (timed.execution.events time) validator head →
    0 < ((timed.execution.trace (time + 1)).validatorState
      validator).highestSignedRound →
    ∃ block capsuleId entry,
      ((timed.execution.trace (time + 1)).validatorState validator).ownBlockAt
          ((timed.execution.trace (time + 1)).validatorState
            validator).highestSignedRound = some block.reference ∧
        (event time validator).addCapsule capsuleId = some entry ∧
        entry.capsule.targetBlock = block ∧
        entry.baselineCommit =
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead
  /-- Each initial capsule is the exact positive durable tip of its host. -/
  initialCapsuleHasTipSource : ∀ validator capsuleId entry,
    (trace 0 validator).capsuleAt capsuleId = some entry →
    (trace 0 validator).pinned capsuleId = true ∧
      0 < ((timed.execution.trace 0).validatorState
        validator).highestSignedRound ∧
      ∃ block,
        ((timed.execution.trace 0).validatorState validator).ownBlockAt
            ((timed.execution.trace 0).validatorState
              validator).highestSignedRound = some block.reference ∧
          entry.capsule.targetBlock = block ∧
          entry.baselineCommit =
            ((timed.execution.trace 0).validatorState validator).commitHead
  /-- Each positive initial durable tip has one exact local capsule. -/
  positiveInitialTipHasCapsule : ∀ validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace 0).epochActive = true →
    0 < ((timed.execution.trace 0).validatorState
      validator).highestSignedRound →
    ∃ block capsuleId entry,
      ((timed.execution.trace 0).validatorState validator).ownBlockAt
          ((timed.execution.trace 0).validatorState
            validator).highestSignedRound = some block.reference ∧
        (trace 0 validator).capsuleAt capsuleId = some entry ∧
        (trace 0 validator).pinned capsuleId = true ∧
        entry.capsule.targetBlock = block ∧
        entry.baselineCommit =
          ((timed.execution.trace 0).validatorState validator).commitHead
  /-- At every active time, the current positive durable tip has one exact pin
  under the current local commit head. -/
  currentPositiveTipHasCapsule : ∀ time validator,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    (timed.execution.trace time).epochActive = true →
    0 < ((timed.execution.trace time).validatorState
      validator).highestSignedRound →
    ∃ block capsuleId entry,
      ((timed.execution.trace time).validatorState validator).ownBlockAt
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound = some block.reference ∧
        (trace time validator).capsuleAt capsuleId = some entry ∧
        (trace time validator).pinned capsuleId = true ∧
        entry.capsule.targetBlock = block ∧
        entry.baselineCommit =
          ((timed.execution.trace time).validatorState validator).commitHead
  /-- Pinned history is exact local accepted, retained, and catalogued data. -/
  pinnedHistoryIsLocal : ∀ time validator capsuleId entry,
    (trace time validator).capsuleAt capsuleId = some entry →
    (trace time validator).pinned capsuleId = true →
    ∀ block, block ∈ entry.capsule.history →
      ((timed.execution.trace time).validatorState validator).accepted
          block.reference = true ∧
        ((timed.execution.trace time).validatorState validator).retained
          block.reference = true ∧
        (timed.execution.trace time).blockCatalog block.reference.id =
          some block ∧
        block.reference.author < config.authorityCount
  /-- Each reference held by an active recovery pin is protected from cleanup.
  Other local subsystems can protect more references. -/
  pinnedReferenceIsSourceProtected : ∀ time validator reference,
    (∃ capsuleId entry block,
      (trace time validator).capsuleAt capsuleId = some entry ∧
        (trace time validator).pinned capsuleId = true ∧
        block ∈ entry.capsule.history ∧
        block.reference = reference) →
    syncRules.sourceProtected validator reference time

namespace ValidatorRecoverySourcePinExecution

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

/-- An active pin cannot be released during an active epoch step. -/
theorem pin_persists_one_active_step
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {time validator : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (_stored : (pins.trace time validator).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace time validator).pinned capsuleId = true)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true) :
    (pins.trace (time + 1) validator).pinned capsuleId = true := by
  have step := pins.transitionsFollowRules time validator
  apply (step.pinUpdateExact capsuleId).2
  exact ⟨Or.inl pinned,
    step.activeEpochPreventsRelease activeAfter capsuleId⟩

/-- Compatibility wrapper for callers that also track the commit head. -/
theorem pin_persists_one_step
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {time validator : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (stored : (pins.trace time validator).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace time validator).pinned capsuleId = true)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (_headDidNotAdvance :
      ((timed.execution.trace (time + 1)).validatorState
        validator).commitHead.index ≤ entry.baselineCommit.index) :
    (pins.trace (time + 1) validator).pinned capsuleId = true :=
  pins.pin_persists_one_active_step stored pinned activeAfter

/-- A stored capsule entry also persists across one transition. -/
theorem capsule_persists_one_step
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {time validator : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (stored : (pins.trace time validator).capsuleAt capsuleId = some entry) :
    (pins.trace (time + 1) validator).capsuleAt capsuleId = some entry := by
  exact (pins.transitionsFollowRules time validator).capsuleUpdateExact
    capsuleId entry |>.2 (Or.inl stored)

/-- One pinned capsule remains pinned while the epoch is active. -/
theorem pin_persists_while_epoch_active
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {start finish validator : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (ordered : start ≤ finish)
    (stored : (pins.trace start validator).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace start validator).pinned capsuleId = true)
    (active : ∀ time, start ≤ time → time ≤ finish →
      (timed.execution.trace time).epochActive = true) :
    (pins.trace finish validator).capsuleAt capsuleId = some entry ∧
      (pins.trace finish validator).pinned capsuleId = true := by
  have advance : ∀ offset,
      start + offset ≤ finish →
      (pins.trace (start + offset) validator).capsuleAt capsuleId = some entry ∧
        (pins.trace (start + offset) validator).pinned capsuleId = true := by
    intro offset
    induction offset with
    | zero =>
        intro _
        simpa using And.intro stored pinned
    | succ offset inductionHypothesis =>
        intro nextBeforeFinish
        have nextBeforeFinish' : start + offset + 1 ≤ finish := by
          simpa [Nat.add_assoc] using nextBeforeFinish
        have currentBeforeFinish : start + offset ≤ finish :=
          Nat.le_trans (Nat.le_add_right _ 1) nextBeforeFinish'
        have current := inductionHypothesis currentBeforeFinish
        have startBeforeNext : start ≤ start + offset + 1 :=
          Nat.le_trans (Nat.le_add_right start offset)
            (Nat.le_add_right _ 1)
        have storedNext := pins.capsule_persists_one_step
          current.1
        have pinnedNext := pins.pin_persists_one_active_step
          current.1 current.2
          (active (start + offset + 1) startBeforeNext nextBeforeFinish')
        simpa [Nat.add_assoc] using And.intro storedNext pinnedNext
  obtain ⟨offset, finishShape⟩ := Nat.exists_eq_add_of_le ordered
  subst finish
  exact advance offset (Nat.le_refl _)

/-- Compatibility wrapper for callers that track a fixed commit baseline. -/
theorem pin_persists_while_head_is_current
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {start finish validator : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (ordered : start ≤ finish)
    (stored : (pins.trace start validator).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace start validator).pinned capsuleId = true)
    (active : ∀ time, start ≤ time → time ≤ finish →
      (timed.execution.trace time).epochActive = true)
    (_headCurrent : ∀ time, start ≤ time → time ≤ finish →
      ((timed.execution.trace time).validatorState
        validator).commitHead.index ≤ entry.baselineCommit.index) :
    (pins.trace finish validator).capsuleAt capsuleId = some entry ∧
      (pins.trace finish validator).pinned capsuleId = true :=
  pins.pin_persists_while_epoch_active ordered stored pinned active

/-- One active pin gives a concrete block-sync source at the same host. -/
theorem pinned_capsule_is_execution_source
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {time validator : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (stored : (pins.trace time validator).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace time validator).pinned capsuleId = true) :
    CausalRecoveryCapsuleExecutionSource syncRules entry.capsule validator
      time := by
  have localData := pins.pinnedHistoryIsLocal time validator capsuleId entry
    stored pinned
  refine
    { holderInRange := validatorInRange
      holderCorrectAvailable := validatorCorrectAvailable
      retained := fun block member => (localData block member).2.1
      accepted := fun block member => (localData block member).1
      catalog := fun block member => (localData block member).2.2.1
      authorInRange := fun block member => (localData block member).2.2.2
      protectedAtStart := ?_ }
  intro block member
  exact pins.pinnedReferenceIsSourceProtected time validator block.reference
    ⟨capsuleId, entry, block, stored, pinned, member, rfl⟩

/-- A persisted proposal creates one exact source-local capsule at the end of
the same execution batch. -/
theorem persisted_proposal_has_pinned_capsule_source
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {time validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.persistProposal block)) :
    ∃ capsuleId entry,
      entry.capsule.targetBlock = block ∧
        (pins.trace (time + 1) validator).capsuleAt capsuleId = some entry ∧
        (pins.trace (time + 1) validator).pinned capsuleId = true ∧
        CausalRecoveryCapsuleExecutionSource syncRules entry.capsule validator
          (time + 1) := by
  rcases pins.persistedProposalAddsCapsule time validator block validatorInRange
      validatorCorrectAvailable activeAfter occurs with
    ⟨capsuleId, entry, added, target, baseline⟩
  have step := pins.transitionsFollowRules time validator
  have stored := (step.capsuleUpdateExact capsuleId entry).2 (Or.inr added)
  have notReleased := step.activeEpochPreventsRelease activeAfter capsuleId
  have isSome :
      ((pins.event time validator).addCapsule capsuleId).isSome = true := by
    simp [added]
  have nowPinned := (step.pinUpdateExact capsuleId).2
    ⟨Or.inr isSome, notReleased⟩
  exact ⟨capsuleId, entry, target, stored, nowPinned,
    pins.pinned_capsule_is_execution_source validatorInRange
      validatorCorrectAvailable stored nowPinned⟩

/-- A positive durable tip at trace time zero has one exact pinned capsule
source at the same validator. -/
theorem initial_positive_tip_has_pinned_capsule_source
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (timed.execution.trace 0).epochActive = true)
    (positive : 0 < ((timed.execution.trace 0).validatorState
      validator).highestSignedRound) :
    ∃ block capsuleId entry,
      ((timed.execution.trace 0).validatorState validator).ownBlockAt
          ((timed.execution.trace 0).validatorState
            validator).highestSignedRound = some block.reference ∧
        entry.capsule.targetBlock = block ∧
        (pins.trace 0 validator).capsuleAt capsuleId = some entry ∧
        (pins.trace 0 validator).pinned capsuleId = true ∧
        CausalRecoveryCapsuleExecutionSource syncRules entry.capsule validator
          0 := by
  rcases pins.positiveInitialTipHasCapsule validator validatorInRange
      validatorCorrectAvailable active positive with
    ⟨block, capsuleId, entry, own, stored, pinned, target, _baseline⟩
  exact ⟨block, capsuleId, entry, own, target, stored, pinned,
    pins.pinned_capsule_is_execution_source validatorInRange
      validatorCorrectAvailable stored pinned⟩

/-- The current positive durable tip has one same-host source at any active
trace time. -/
theorem current_positive_tip_has_pinned_capsule_source
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {time validator : Time}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (active : (timed.execution.trace time).epochActive = true)
    (positive : 0 < ((timed.execution.trace time).validatorState
      validator).highestSignedRound) :
    ∃ block capsuleId entry,
      ((timed.execution.trace time).validatorState validator).ownBlockAt
          ((timed.execution.trace time).validatorState
            validator).highestSignedRound = some block.reference ∧
        entry.capsule.targetBlock = block ∧
        (pins.trace time validator).capsuleAt capsuleId = some entry ∧
        (pins.trace time validator).pinned capsuleId = true ∧
        CausalRecoveryCapsuleExecutionSource syncRules entry.capsule validator
          time := by
  rcases pins.currentPositiveTipHasCapsule time validator validatorInRange
      validatorCorrectAvailable active positive with
    ⟨block, capsuleId, entry, own, stored, pinned, target, _baseline⟩
  exact ⟨block, capsuleId, entry, own, target, stored, pinned,
    pins.pinned_capsule_is_execution_source validatorInRange
      validatorCorrectAvailable stored pinned⟩

end ValidatorRecoverySourcePinExecution

end Mysticeti
