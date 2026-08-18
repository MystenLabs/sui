/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorRecoveryParentNeedExecution

namespace Mysticeti

/-! Durable recursive parent needs at one requester.

A local child body can create needs only for its own missing direct parents.
The requester state does not contain a remote holder or a future result. Each
active need has a past same-host body origin and stays active until the parent
is accepted, the epoch ends, the local commit head passes its baseline, or the
local GC frontier reaches the parent round.
-/

/-- One requester-local recursive goal created from one concrete child body. -/
structure ValidatorRecoveryRecursiveParentNeed
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId) where
  baselineCommit : ValidatorCommitHead CommitId
  child : ValidatorBlock BlockId
  parent : ValidatorBlockRef BlockId
  parentInChild : parent ∈ child.parents

/-- One requester has at most one current origin for each exact parent
reference. A new body can refresh the same reference after a commit advance. -/
structure ValidatorRecoveryRecursiveParentNeedState
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  active : ValidatorBlockRef BlockId →
    Option (ValidatorRecoveryRecursiveParentNeed (BlockId := BlockId) config)

/-- Fundamental single-host rules for recursive recovery dependency work. -/
structure ValidatorRecoveryRecursiveParentNeedExecution
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
    ValidatorRecoveryRecursiveParentNeedState BlockId CommitId config
  /-- Processing a local child body records each concrete missing direct parent
  at the end of that execution batch. -/
  localBodyLatchesMissingDirectParent : ∀ time validator child parent,
    (timed.execution.trace (time + 1)).epochActive = true →
    ValidatorLocalBlockBodyAt timed time validator child →
    parent ∈ child.parents →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound <
      parent.round →
    ((timed.execution.trace (time + 1)).validatorState validator).accepted
        parent = false →
    ∃ need,
      (trace (time + 1) validator).active parent = some need ∧
        need.baselineCommit =
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead ∧
        need.parent = parent
  /-- Every active reference has an earlier same-host child-body origin. -/
  activeNeedHasLocalBodyOrigin : ∀ time validator reference need,
    (trace time validator).active reference = some need →
    need.parent = reference ∧
      ∃ originTime,
        originTime < time ∧
          ValidatorLocalBlockBodyAt timed originTime validator
            need.child
  /-- Every active reference is above this requester's current GC frontier. -/
  activeNeedIsAboveGc : ∀ time validator reference need,
    (trace time validator).active reference = some need →
    ((timed.execution.trace time).validatorState validator).gcRound <
      reference.round
  /-- An active goal uses the requester's current durable commit head as its
  baseline. A local commit advance clears or replaces that goal. -/
  activeNeedBaselineIsCurrent : ∀ time validator reference need,
    (trace time validator).active reference = some need →
    need.baselineCommit =
      ((timed.execution.trace time).validatorState validator).commitHead
  /-- An unresolved active need persists across one local transition while its
  epoch and commit baseline stay current. -/
  activeNeedPersistsOneStep : ∀ time validator reference need,
    (trace time validator).active reference = some need →
    (timed.execution.trace (time + 1)).epochActive = true →
    ((timed.execution.trace (time + 1)).validatorState
      validator).commitHead.index ≤ need.baselineCommit.index →
    ((timed.execution.trace (time + 1)).validatorState validator).gcRound <
      reference.round →
    ((timed.execution.trace (time + 1)).validatorState validator).accepted
        reference = false →
    (trace (time + 1) validator).active reference = some need
  /-- Only an active same-host need keeps its exact block-sync goal live. -/
  activeNeedIsNotObsolete : ∀ time validator reference need,
    (trace time validator).active reference = some need →
    ((timed.execution.trace time).validatorState validator).accepted reference =
      false →
    ¬syncRules.goalObsolete validator reference time

namespace ValidatorRecoveryRecursiveParentNeedExecution

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

/-- A delivered or accepted local child creates or refreshes only its exact
direct-parent goal. The chosen goal has one real same-host child origin. -/
theorem local_body_creates_exact_recursive_parent_need
    (recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules)
    {time validator : Time} {child : ValidatorBlock BlockId}
    {parent : ValidatorBlockRef BlockId}
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (body : ValidatorLocalBlockBodyAt timed time validator child)
    (parentInChild : parent ∈ child.parents)
    (parentAboveGc :
      ((timed.execution.trace (time + 1)).validatorState validator).gcRound <
        parent.round)
    (parentMissing :
      ((timed.execution.trace (time + 1)).validatorState validator).accepted
          parent = false) :
    ∃ need,
      (recursive.trace (time + 1) validator).active parent = some need ∧
        need.baselineCommit =
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead ∧
        need.parent = parent ∧
        ∃ originTime,
          originTime < time + 1 ∧
            ValidatorLocalBlockBodyAt timed originTime validator
              need.child := by
  rcases recursive.localBodyLatchesMissingDirectParent time validator child
      parent activeAfter body parentInChild parentAboveGc parentMissing with
    ⟨need, active, baseline, parentExact⟩
  have origin := recursive.activeNeedHasLocalBodyOrigin (time + 1) validator
    parent need active
  exact ⟨need, active, baseline, parentExact, origin.2⟩

/-- One active recursive need stays live through any finite interval in which
the epoch remains active, the requester commit head stays at its baseline, and
the exact parent remains missing. -/
theorem active_need_persists_while_current
    (recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules)
    {start finish validator : Time} {reference : ValidatorBlockRef BlockId}
    {need : ValidatorRecoveryRecursiveParentNeed (BlockId := BlockId) config}
    (ordered : start ≤ finish)
    (activeAtStart :
      (recursive.trace start validator).active reference = some need)
    (activeEpoch : ∀ time, start ≤ time → time ≤ finish →
      (timed.execution.trace time).epochActive = true)
    (headCurrent : ∀ time, start ≤ time → time ≤ finish →
      ((timed.execution.trace time).validatorState
        validator).commitHead.index ≤ need.baselineCommit.index)
    (aboveGc : ∀ time, start ≤ time → time ≤ finish →
      ((timed.execution.trace time).validatorState validator).gcRound <
        reference.round)
    (parentMissing : ∀ time, start ≤ time → time ≤ finish →
      ((timed.execution.trace time).validatorState validator).accepted
          reference = false) :
    (recursive.trace finish validator).active reference = some need := by
  have advance : ∀ offset,
      start + offset ≤ finish →
      (recursive.trace (start + offset) validator).active reference =
        some need := by
    intro offset
    induction offset with
    | zero =>
        intro _
        simpa using activeAtStart
    | succ offset inductionHypothesis =>
        intro nextBeforeFinish
        have nextBeforeFinish' : start + offset + 1 ≤ finish := by
          simpa [Nat.add_assoc] using nextBeforeFinish
        have currentBeforeFinish : start + offset ≤ finish :=
          Nat.le_trans (Nat.le_add_right _ 1) nextBeforeFinish'
        have activeCurrent := inductionHypothesis currentBeforeFinish
        have startBeforeNext : start ≤ start + offset + 1 :=
          Nat.le_trans (Nat.le_add_right start offset)
            (Nat.le_add_right _ 1)
        simpa [Nat.add_assoc] using recursive.activeNeedPersistsOneStep
          (start + offset) validator reference need activeCurrent
          (activeEpoch (start + offset + 1) startBeforeNext nextBeforeFinish')
          (headCurrent (start + offset + 1) startBeforeNext nextBeforeFinish')
          (aboveGc (start + offset + 1) startBeforeNext nextBeforeFinish')
          (parentMissing (start + offset + 1) startBeforeNext nextBeforeFinish')
  obtain ⟨offset, finishShape⟩ := Nat.exists_eq_add_of_le ordered
  subst finish
  exact advance offset (Nat.le_refl _)

/-- A current active recursive need gives the exact nonobsolete request premise
used by one block-sync attempt. -/
theorem active_need_gives_live_block_sync_goal
    (recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules)
    {time validator : Time} {reference : ValidatorBlockRef BlockId}
    {need : ValidatorRecoveryRecursiveParentNeed (BlockId := BlockId) config}
    (active :
      (recursive.trace time validator).active reference = some need)
    (missing :
      ((timed.execution.trace time).validatorState validator).accepted
          reference = false) :
    need.parent = reference ∧
      ((timed.execution.trace time).validatorState validator).gcRound <
        reference.round ∧
      (∃ originTime,
        originTime < time ∧
          ValidatorLocalBlockBodyAt timed originTime validator
            need.child) ∧
      ¬syncRules.goalObsolete validator reference time := by
  have origin := recursive.activeNeedHasLocalBodyOrigin time validator reference
    need active
  exact ⟨origin.1,
    recursive.activeNeedIsAboveGc time validator reference need active,
    origin.2,
    recursive.activeNeedIsNotObsolete time validator reference need active
      missing⟩

end ValidatorRecoveryRecursiveParentNeedExecution

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

/-- A persisted proposal exposes the commit baseline that opened its exact
source-local capsule pin. -/
theorem persisted_proposal_has_pinned_capsule_source_with_baseline
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {time validator : Time} {block : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrectAvailable : faults.correctAvailable validator = true)
    (activeAfter : (timed.execution.trace (time + 1)).epochActive = true)
    (occurs : ValidatorLocalActionOccurs (timed.execution.events time)
      validator (.persistProposal block)) :
    ∃ capsuleId entry,
      entry.capsule.targetBlock = block ∧
        entry.baselineCommit =
          ((timed.execution.trace (time + 1)).validatorState
            validator).commitHead ∧
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
  exact ⟨capsuleId, entry, target, baseline, stored, nowPinned,
    pins.pinned_capsule_is_execution_source validatorInRange
      validatorCorrectAvailable stored nowPinned⟩

end ValidatorRecoverySourcePinExecution

namespace ValidatorRecoveryRecursiveParentNeedExecution

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

/-- One active exact-reference need above the requester's GC frontier gets its
block body through the normal block request path, unless the requester's local
commit head advances first.

The correct source keeps its concrete capsule pin for the active epoch. Source
commit and GC movement do not end service for this finite pinned item. -/
theorem pinned_history_item_eventually_delivered_or_requester_commit_advance
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules)
    {start holder requester : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId} {entry}
    {block : ValidatorBlock BlockId}
    {need : ValidatorRecoveryRecursiveParentNeed (BlockId := BlockId) config}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (activeEpoch : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (requesterAboveGc : ∀ time, start ≤ time →
      ((timed.execution.trace time).validatorState requester).gcRound <
        block.reference.round)
    (stored : (pins.trace start holder).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace start holder).pinned capsuleId = true)
    (member : block ∈ entry.capsule.history)
    (activeNeed :
      (recursive.trace start requester).active block.reference = some need) :
    ∃ finish, start ≤ finish ∧
      (ValidatorLocalBlockBodyAt timed finish requester block ∨
        need.baselineCommit.index <
          ((timed.execution.trace finish).validatorState
            requester).commitHead.index) := by
  classical
  by_cases eventualResult : ∃ finish, start ≤ finish ∧
      (ValidatorLocalBlockBodyAt timed finish requester block ∨
        need.baselineCommit.index <
          ((timed.execution.trace finish).validatorState
            requester).commitHead.index)
  · exact eventualResult
  · have sourceAtStart := pins.pinned_capsule_is_execution_source
      holderInRange holderCorrectAvailable stored pinned
    have historyAtStart :=
      causal_recovery_capsule_to_retained_validator_history sourceAtStart
    have blockAtStart := historyAtStart.item member
    have noBody : ∀ time, start ≤ time →
        ¬ValidatorLocalBlockBodyAt timed time requester block := by
      intro time ordered body
      exact eventualResult ⟨time, ordered, Or.inl body⟩
    have requesterHeadCurrent : ∀ time, start ≤ time →
        ((timed.execution.trace time).validatorState
          requester).commitHead.index ≤ need.baselineCommit.index := by
      intro time ordered
      have notAdvanced : ¬need.baselineCommit.index <
          ((timed.execution.trace time).validatorState
            requester).commitHead.index := by
        intro advanced
        exact eventualResult ⟨time, ordered, Or.inr advanced⟩
      omega
    have parentMissing : ∀ time, start ≤ time →
        ((timed.execution.trace time).validatorState requester).accepted
            block.reference = false := by
      intro time ordered
      cases accepted : ((timed.execution.trace time).validatorState
          requester).accepted block.reference with
      | false => rfl
      | true =>
          have catalogAtTime := timed.execution.blockCatalogMonotone start time
            ordered block.reference.id block blockAtStart.catalog
          exact False.elim
            (noBody time ordered (.acceptedCatalogued accepted catalogAtTime))
    have activeAt : ∀ time, start ≤ time →
        (recursive.trace time requester).active block.reference = some need := by
      intro time ordered
      exact recursive.active_need_persists_while_current ordered activeNeed
        (by
          intro current currentAfterStart _
          exact activeEpoch current currentAfterStart)
        (by
          intro current currentAfterStart _
          exact requesterHeadCurrent current currentAfterStart)
        (by
          intro current currentAfterStart _
          exact requesterAboveGc current currentAfterStart)
        (by
          intro current currentAfterStart _
          exact parentMissing current currentAfterStart)
    rcases retained_validator_block_body_delivered_or_obsolete_within_bound
        syncRules blockAtStart requesterInRange requesterCorrectAvailable
        afterGst activeEpoch
        (by
          intro time timeAfterStart _missing _notObsolete
          have pinAtTime := pins.pin_persists_while_epoch_active timeAfterStart
            stored pinned (by
              intro current currentAfterStart _
              exact activeEpoch current currentAfterStart)
          exact pins.pinnedReferenceIsSourceProtected time holder block.reference
            ⟨capsuleId, entry, block, pinAtTime.1, pinAtTime.2, member, rfl⟩) with
      ⟨finish, startBeforeFinish, _finishBound, bodyOrObsolete⟩
    rcases bodyOrObsolete with body | obsolete
    · exact False.elim (noBody finish startBeforeFinish body)
    · have live := recursive.activeNeedIsNotObsolete finish requester
          block.reference need (activeAt finish startBeforeFinish)
          (parentMissing finish startBeforeFinish)
      exact False.elim (live obsolete)

/-- Recursive synchronization stops when the requester's GC frontier reaches
the exact reference. Since GC is determined by the durable commit head, moving
the frontier past an active reference proves local commit progress. -/
theorem pinned_history_item_eventually_delivered_or_requester_progress
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (recursive : ValidatorRecoveryRecursiveParentNeedExecution syncRules)
    {start holder requester : Time}
    {capsuleId : ValidatorRecoveryCapsuleKey BlockId} {entry}
    {block : ValidatorBlock BlockId}
    {need : ValidatorRecoveryRecursiveParentNeed (BlockId := BlockId) config}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (activeEpoch : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (stored : (pins.trace start holder).capsuleAt capsuleId = some entry)
    (pinned : (pins.trace start holder).pinned capsuleId = true)
    (member : block ∈ entry.capsule.history)
    (activeNeed :
      (recursive.trace start requester).active block.reference = some need) :
    ∃ finish, start ≤ finish ∧
      (ValidatorLocalBlockBodyAt timed finish requester block ∨
        need.baselineCommit.index <
          ((timed.execution.trace finish).validatorState
            requester).commitHead.index) := by
  by_cases gcRoot : ∃ finish, start ≤ finish ∧
      block.reference.round ≤
        ((timed.execution.trace finish).validatorState requester).gcRound
  · rcases gcRoot with ⟨finish, ordered, atRoot⟩
    have aboveAtStart := recursive.activeNeedIsAboveGc start requester
      block.reference need activeNeed
    have baselineAtStart := recursive.activeNeedBaselineIsCurrent start requester
      block.reference need activeNeed
    have gcAdvanced :
        ((timed.execution.trace start).validatorState requester).gcRound <
          ((timed.execution.trace finish).validatorState requester).gcRound := by
      omega
    have monotone := timed.execution.durableStateMonotone requester start finish
      requesterInRange ordered
    have commitAdvanced :
        ((timed.execution.trace start).validatorState requester).commitHead.index <
          ((timed.execution.trace finish).validatorState
            requester).commitHead.index := by
      by_cases sameIndex :
          ((timed.execution.trace start).validatorState
              requester).commitHead.index =
            ((timed.execution.trace finish).validatorState
              requester).commitHead.index
      · have sameHead := monotone.2.2.1 sameIndex
        have gcAtStart := timed.execution.correctGcRoundMatchesCommitHead start
          requester requesterInRange requesterCorrectAvailable
        have gcAtFinish := timed.execution.correctGcRoundMatchesCommitHead finish
          requester requesterInRange requesterCorrectAvailable
        rw [gcAtStart, gcAtFinish, sameHead] at gcAdvanced
        omega
      · have headLe := monotone.1
        omega
    have baselineAdvanced : need.baselineCommit.index <
        ((timed.execution.trace finish).validatorState
          requester).commitHead.index := by
      rw [baselineAtStart]
      exact commitAdvanced
    exact ⟨finish, ordered, Or.inr baselineAdvanced⟩
  · have requesterAboveGc : ∀ time, start ≤ time →
        ((timed.execution.trace time).validatorState requester).gcRound <
          block.reference.round := by
      intro time ordered
      have notAtRoot : ¬block.reference.round ≤
          ((timed.execution.trace time).validatorState requester).gcRound := by
        intro atRoot
        exact gcRoot ⟨time, ordered, atRoot⟩
      omega
    rcases pinned_history_item_eventually_delivered_or_requester_commit_advance
        pins recursive holderInRange holderCorrectAvailable requesterInRange
        requesterCorrectAvailable afterGst activeEpoch requesterAboveGc stored
        pinned member activeNeed with
      ⟨finish, ordered, body | advanced⟩
    · exact ⟨finish, ordered, Or.inl body⟩
    · exact ⟨finish, ordered, Or.inr advanced⟩

end ValidatorRecoveryRecursiveParentNeedExecution

end Mysticeti
