/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorAdjacentRecoveryPropagation
import Mysticeti.ValidatorFlexPendingRefresh
import Mysticeti.ValidatorOperationalFrontierCollectiveSuccessor

namespace Mysticeti

/-! Receiver-relative causal catch-up bounds for timer spread.

The existing capsule rate bounds the complete source history. Timer spread
needs the smaller amount of work that is unresolved at one receiver after an
exact prior causal cutoff. This module proves that entries which are already
ready cost no synchronization time. It then states the narrow current-state
mapping from an adjacent exact cutoff to the remaining one-round work.

The mapping has no future timer, proposal, packet, layer, or commit result.
-/

/-- Number of history entries which are not accepted and are above the local
GC round at one trace state. -/
def validatorUnresolvedHistoryItemsAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (time receiver : Nat) : List (ValidatorBlock BlockId) → Nat
  | [] => 0
  | block :: remaining =>
      if (execution.trace time).validatorState receiver |>.accepted
          block.reference then
        validatorUnresolvedHistoryItemsAt execution time receiver remaining
      else if block.reference.round ≤
          ((execution.trace time).validatorState receiver).gcRound then
        validatorUnresolvedHistoryItemsAt execution time receiver remaining
      else
        validatorUnresolvedHistoryItemsAt execution time receiver remaining + 1

/-- A ready head entry contributes no unresolved work. -/
@[simp] theorem validator_unresolved_history_items_cons_of_ready
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time receiver : Nat} {block : ValidatorBlock BlockId}
    {remaining : List (ValidatorBlock BlockId)}
    (ready : ValidatorReferenceAcceptedOrGcRootAt execution time receiver
      block.reference) :
    validatorUnresolvedHistoryItemsAt execution time receiver
        (block :: remaining) =
      validatorUnresolvedHistoryItemsAt execution time receiver remaining := by
  rcases ready with accepted | atRoot
  · simp [validatorUnresolvedHistoryItemsAt, accepted]
  · cases acceptedValue :
        ((execution.trace time).validatorState receiver).accepted block.reference
    · simp [validatorUnresolvedHistoryItemsAt, acceptedValue, atRoot]
    · simp [validatorUnresolvedHistoryItemsAt, acceptedValue]

/-- A head entry which is neither accepted nor a GC root contributes one
unresolved work item. -/
@[simp] theorem validator_unresolved_history_items_cons_of_not_ready
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {time receiver : Nat} {block : ValidatorBlock BlockId}
    {remaining : List (ValidatorBlock BlockId)}
    (notReady : ¬ValidatorReferenceAcceptedOrGcRootAt execution time receiver
      block.reference) :
    validatorUnresolvedHistoryItemsAt execution time receiver
        (block :: remaining) =
      validatorUnresolvedHistoryItemsAt execution time receiver remaining + 1 := by
  have notAccepted :
      ((execution.trace time).validatorState receiver).accepted
        block.reference = false := by
    cases acceptedValue :
        ((execution.trace time).validatorState receiver).accepted block.reference
    · rfl
    · exact False.elim (notReady (Or.inl acceptedValue))
  have aboveGc : ¬block.reference.round ≤
      ((execution.trace time).validatorState receiver).gcRound :=
    fun atRoot => notReady (Or.inr atRoot)
  simp [validatorUnresolvedHistoryItemsAt, notAccepted, aboveGc]

/-- The unresolved count cannot increase as accepted state and the GC round
move forward. -/
theorem validator_unresolved_history_items_mono
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {earlier later receiver : Nat} {blocks : List (ValidatorBlock BlockId)}
    (receiverInRange : receiver < config.authorityCount)
    (ordered : earlier ≤ later) :
    validatorUnresolvedHistoryItemsAt execution later receiver blocks ≤
      validatorUnresolvedHistoryItemsAt execution earlier receiver blocks := by
  induction blocks with
  | nil => simp [validatorUnresolvedHistoryItemsAt]
  | cons block remaining inductionHypothesis =>
      by_cases readyEarlier : ValidatorReferenceAcceptedOrGcRootAt execution
          earlier receiver block.reference
      · have readyLater : ValidatorReferenceAcceptedOrGcRootAt execution later
            receiver block.reference :=
          validator_reference_accepted_or_gc_root_persists execution
            receiverInRange ordered readyEarlier
        rw [validator_unresolved_history_items_cons_of_ready readyEarlier,
          validator_unresolved_history_items_cons_of_ready readyLater]
        exact inductionHypothesis
      · by_cases readyLater : ValidatorReferenceAcceptedOrGcRootAt execution
            later receiver block.reference
        · rw [validator_unresolved_history_items_cons_of_not_ready readyEarlier,
            validator_unresolved_history_items_cons_of_ready readyLater]
          exact Nat.le_trans inductionHypothesis (Nat.le_add_right _ _)
        · rw [validator_unresolved_history_items_cons_of_not_ready readyEarlier,
            validator_unresolved_history_items_cons_of_not_ready readyLater]
          exact Nat.add_le_add_right inductionHypothesis 1

/-- Remove the first block from a retained history without changing its
current source facts. -/
private theorem retained_validator_block_history_tail
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {holder time : Nat} {block : ValidatorBlock BlockId}
    {remaining : List (ValidatorBlock BlockId)}
    (source : RetainedValidatorBlockHistory execution holder
      (block :: remaining) time) :
    RetainedValidatorBlockHistory execution holder remaining time := by
  refine {
    holderInRange := source.holderInRange
    holderCorrectAvailable := source.holderCorrectAvailable
    retained := ?_
    accepted := ?_
    catalog := ?_
    authorInRange := ?_
    validParents := ?_ }
  · intro item member
    exact source.retained item (by simp [member])
  · intro item member
    exact source.accepted item (by simp [member])
  · intro item member
    exact source.catalog item (by simp [member])
  · intro item member
    exact source.authorInRange item (by simp [member])
  · intro item member
    exact source.validParents item (by simp [member])

/-- A parent-first history is ready within one block-sync cost for each entry
that was unresolved at the initial receiver state.

This is stronger than the length bound when the receiver already has an exact
causal cutoff. It does not assert that a future history exists. -/
theorem retained_parent_first_history_ready_within_unresolved_bound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (rules : ValidatorBlockSyncExecutionRules timed)
    {holder requester : Nat} {blocks : List (ValidatorBlock BlockId)}
    {start : Time}
    (source : RetainedValidatorBlockHistory timed.execution holder blocks start)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (protectedWhileIncomplete : ∀ time,
      start ≤ time →
      (¬∀ item, item ∈ blocks →
        ((timed.execution.trace time).validatorState requester).accepted
          item.reference = true) →
      ∀ item, item ∈ blocks →
        rules.sourceProtected holder item.reference time)
    (parentFirst : ParentFirstValidatorBlockHistory
      (ValidatorReferenceAcceptedOrGcRootAt timed.execution start requester)
      blocks) :
    ∃ finish,
      start ≤ finish ∧
      finish ≤ start +
        validatorUnresolvedHistoryItemsAt timed.execution start requester blocks *
          validatorBlockSyncAcceptanceBound timed rules ∧
      ∀ block, block ∈ blocks →
        ValidatorReferenceAcceptedOrGcRootAt timed.execution finish requester
          block.reference := by
  induction blocks generalizing start with
  | nil =>
      exact ⟨start, Nat.le_refl start, by
        simp [validatorUnresolvedHistoryItemsAt], by simp⟩
  | cons block remaining inductionHypothesis =>
      have blockMember : block ∈ block :: remaining := by simp
      by_cases blockReadyAtStart : ValidatorReferenceAcceptedOrGcRootAt
          timed.execution start requester block.reference
      · have remainingSource :=
          retained_validator_block_history_tail source
        have remainingParentFirst : ParentFirstValidatorBlockHistory
            (ValidatorReferenceAcceptedOrGcRootAt timed.execution start requester)
            remaining := by
          exact parent_first_validator_block_history_mono (by
            intro reference ready
            rcases ready with readyInitially | sameBlock
            · exact readyInitially
            · rw [sameBlock]
              exact blockReadyAtStart) parentFirst.2
        rcases inductionHypothesis remainingSource afterGst active (by
              intro time startBeforeTime remainingIncomplete item member
              exact protectedWhileIncomplete time startBeforeTime (by
                intro allAccepted
                apply remainingIncomplete
                intro remainingItem remainingMember
                exact allAccepted remainingItem (by simp [remainingMember]))
                item (by simp [member])) remainingParentFirst with
          ⟨finish, startBeforeFinish, finishBound, remainingReady⟩
        refine ⟨finish, startBeforeFinish, ?_, ?_⟩
        · rw [validator_unresolved_history_items_cons_of_ready
            blockReadyAtStart]
          exact finishBound
        · intro item member
          simp only [List.mem_cons] at member
          rcases member with sameBlock | inRemaining
          · subst item
            exact validator_reference_accepted_or_gc_root_persists
              timed.execution requesterInRange startBeforeFinish
                blockReadyAtStart
          · exact remainingReady item inRemaining
      · have blockSource := source.item blockMember
        rcases retained_validator_block_accepted_or_obsolete_within_bound rules
            blockSource requesterInRange requesterCorrectAvailable afterGst
            active (by
              intro time startBeforeTime blockNotAccepted _notObsolete
              exact protectedWhileIncomplete time startBeforeTime (by
                intro complete
                have accepted := complete block blockMember
                rw [blockNotAccepted] at accepted
                simp at accepted) block blockMember)
            (by simpa [ValidatorReferenceAcceptedOrGcRootAt] using
              parentFirst.1) with
          ⟨blockFinish, startBeforeBlockFinish, blockFinishBound,
            blockAcceptedOrObsolete⟩
        have blockReady : ValidatorReferenceAcceptedOrGcRootAt timed.execution
            blockFinish requester block.reference := by
          rcases blockAcceptedOrObsolete with accepted | obsolete
          · exact Or.inl accepted
          · exact Or.inr (rules.goalObsoleteIsAtOrBelowGc requester
              block.reference blockFinish requesterInRange
                requesterCorrectAvailable obsolete)
        by_cases remainingAlreadyReady : ∀ item, item ∈ remaining →
            ValidatorReferenceAcceptedOrGcRootAt timed.execution blockFinish
              requester item.reference
        · refine ⟨blockFinish, startBeforeBlockFinish, ?_, ?_⟩
          · calc
              blockFinish ≤
                  start + validatorBlockSyncAcceptanceBound timed rules :=
                blockFinishBound
              _ ≤ start +
                  validatorUnresolvedHistoryItemsAt timed.execution start
                      requester (block :: remaining) *
                    validatorBlockSyncAcceptanceBound timed rules := by
                have unresolvedPositive : 0 <
                    validatorUnresolvedHistoryItemsAt timed.execution start
                      requester (block :: remaining) := by
                  rw [validator_unresolved_history_items_cons_of_not_ready
                    blockReadyAtStart]
                  omega
                exact Nat.add_le_add_left
                  (Nat.le_mul_of_pos_left _ unresolvedPositive) start
          · intro item member
            simp only [List.mem_cons] at member
            rcases member with sameBlock | inRemaining
            · subst item
              exact blockReady
            · exact remainingAlreadyReady item inRemaining
        · have fullSourceAtBlockFinish :=
            retained_validator_block_history_persists rules source
              startBeforeBlockFinish (by
                intro item member time timeAfterStart timeBeforeBlockFinish
                exact protectedWhileIncomplete time timeAfterStart (by
                  intro allAcceptedAtTime
                  apply remainingAlreadyReady
                  intro remainingItem remainingMember
                  exact Or.inl
                    (timed.execution.accepted_block_persists requesterInRange
                      timeBeforeBlockFinish
                        (allAcceptedAtTime remainingItem
                          (by simp [remainingMember])))) item member)
          have remainingSource : RetainedValidatorBlockHistory timed.execution
              holder remaining blockFinish :=
            retained_validator_block_history_tail fullSourceAtBlockFinish
          have remainingParentFirst : ParentFirstValidatorBlockHistory
              (ValidatorReferenceAcceptedOrGcRootAt timed.execution blockFinish
                requester) remaining := by
            exact parent_first_validator_block_history_mono (by
              intro reference ready
              rcases ready with readyAtStart | sameBlock
              · exact validator_reference_accepted_or_gc_root_persists
                  timed.execution requesterInRange startBeforeBlockFinish
                    readyAtStart
              · rw [sameBlock]
                exact blockReady) parentFirst.2
          rcases inductionHypothesis remainingSource
              (Nat.le_trans afterGst startBeforeBlockFinish) (by
                intro time blockFinishBeforeTime
                exact active time
                  (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime)) (by
                intro time blockFinishBeforeTime remainingIncomplete item member
                exact protectedWhileIncomplete time
                  (Nat.le_trans startBeforeBlockFinish blockFinishBeforeTime) (by
                    intro allAccepted
                    apply remainingIncomplete
                    intro remainingItem remainingMember
                    exact allAccepted remainingItem (by simp [remainingMember]))
                  item (by simp [member])) remainingParentFirst with
            ⟨finish, blockFinishBeforeFinish, finishBound, remainingReady⟩
          have unresolvedMono := validator_unresolved_history_items_mono
            (execution := timed.execution) requesterInRange
              startBeforeBlockFinish (blocks := remaining)
          refine ⟨finish, Nat.le_trans startBeforeBlockFinish
            blockFinishBeforeFinish, ?_, ?_⟩
          · calc
              finish ≤ blockFinish +
                  validatorUnresolvedHistoryItemsAt timed.execution blockFinish
                      requester remaining *
                    validatorBlockSyncAcceptanceBound timed rules := finishBound
              _ ≤ blockFinish +
                  validatorUnresolvedHistoryItemsAt timed.execution start requester
                      remaining *
                    validatorBlockSyncAcceptanceBound timed rules :=
                Nat.add_le_add_left
                  (Nat.mul_le_mul_right _ unresolvedMono) blockFinish
              _ ≤ (start + validatorBlockSyncAcceptanceBound timed rules) +
                  validatorUnresolvedHistoryItemsAt timed.execution start requester
                      remaining *
                    validatorBlockSyncAcceptanceBound timed rules :=
                Nat.add_le_add_right blockFinishBound _
              _ = start +
                  validatorUnresolvedHistoryItemsAt timed.execution start requester
                      (block :: remaining) *
                    validatorBlockSyncAcceptanceBound timed rules := by
                rw [validator_unresolved_history_items_cons_of_not_ready
                  blockReadyAtStart]
                simp [Nat.add_mul]
                ac_rfl
          · intro item member
            simp only [List.mem_cons] at member
            rcases member with sameBlock | inRemaining
            · subst item
              exact validator_reference_accepted_or_gc_root_persists
                timed.execution requesterInRange blockFinishBeforeFinish
                  blockReady
            · exact remainingReady item inRemaining

/-- One receiver has processed the exact prior capsule history. -/
def ValidatorAcceptedCausalCapsuleCutoffAt
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (time receiver : Nat) : Prop :=
  ∀ block, block ∈ capsule.history →
    ValidatorReferenceAcceptedOrGcRootAt timed.execution time receiver
      block.reference

/-- Number of exact references in one capsule history which are not in the
prior capsule closure. This count is source-local. It does not inspect one
receiver's accepted state or GC round. -/
def validatorCausalCapsuleNovelHistoryItems
    {BlockId CommitId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    (previous : CausalRecoveryCapsule (BlockId := BlockId) config) :
    List (ValidatorBlock BlockId) → Nat
  | [] => 0
  | block :: remaining =>
      if block.reference ∈
          previous.history.map ValidatorBlock.reference then
        validatorCausalCapsuleNovelHistoryItems previous remaining
      else
        validatorCausalCapsuleNovelHistoryItems previous remaining + 1

/-- An accepted prior capsule cutoff makes every unresolved next-history item
source-local novelty. GC roots can only reduce the receiver count further. -/
theorem validator_unresolved_history_items_le_capsule_novel_history_items
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {previous : CausalRecoveryCapsule (BlockId := BlockId) config}
    {time receiver : Nat} {blocks : List (ValidatorBlock BlockId)}
    (cutoff : ValidatorAcceptedCausalCapsuleCutoffAt timed previous time
      receiver) :
    validatorUnresolvedHistoryItemsAt timed.execution time receiver blocks ≤
      validatorCausalCapsuleNovelHistoryItems previous blocks := by
  induction blocks with
  | nil =>
      simp [validatorUnresolvedHistoryItemsAt,
        validatorCausalCapsuleNovelHistoryItems]
  | cons block remaining inductionHypothesis =>
      by_cases inPrevious : block.reference ∈
          previous.history.map ValidatorBlock.reference
      · rcases List.mem_map.mp inPrevious with
          ⟨previousBlock, previousMember, sameReference⟩
        have ready : ValidatorReferenceAcceptedOrGcRootAt timed.execution time
            receiver block.reference := by
          rw [← sameReference]
          exact cutoff previousBlock previousMember
        rw [validator_unresolved_history_items_cons_of_ready ready]
        simp only [validatorCausalCapsuleNovelHistoryItems, inPrevious, if_true]
        exact inductionHypothesis
      · simp only [validatorCausalCapsuleNovelHistoryItems, inPrevious,
          if_false]
        by_cases ready : ValidatorReferenceAcceptedOrGcRootAt timed.execution
            time receiver block.reference
        · rw [validator_unresolved_history_items_cons_of_ready ready]
          exact Nat.le_trans inductionHypothesis (Nat.le_add_right _ _)
        · rw [validator_unresolved_history_items_cons_of_not_ready ready]
          exact Nat.add_le_add_right inductionHypothesis 1

/-- Current/past source mapping for one persisted adjacent capsule.

The rule counts only exact references which the correct child's capsule adds
above the included prior capsule closure. It has no receiver, future timer,
packet, layer, or commit result. -/
structure ValidatorPersistedCausalCapsuleRoundNoveltySourceMap
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
    (newWorkPerRound : Nat) : Type where
  /-- One immutable block selects one exact causal-history projection. -/
  capsuleFor : ValidatorBlock BlockId →
    CausalRecoveryCapsule (BlockId := BlockId) config
  capsuleTargetsBlock : ∀ block,
    (capsuleFor block).targetBlock = block
  /-- One actual correct persistence has both deterministic source-local
  projections and adds at most one configured round of exact causal-history
  references above the included parent closure. The prior source is read in
  the pre-persist state. The child source is read after the same batch. -/
  correctPersistHasProjectedSourcesAndBoundedNovelty : ∀
    {previousBlock nextBlock : ValidatorBlock BlockId}
    {persistTime author : Nat},
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ValidatorLocalActionOccurs (timed.execution.events persistTime) author
        (.persistProposal nextBlock) →
    previousBlock.reference ∈ nextBlock.parents →
    previousBlock.reference.round + 1 = nextBlock.reference.round →
    CausalRecoveryCapsuleExecutionSource syncRules (capsuleFor previousBlock)
        author persistTime ∧
      CausalRecoveryCapsuleExecutionSource syncRules (capsuleFor nextBlock)
        author (persistTime + 1) ∧
      validatorCausalCapsuleNovelHistoryItems (capsuleFor previousBlock)
          (capsuleFor nextBlock).history ≤ newWorkPerRound

/-- The receiver-relative delta rule bounds the concrete synchronization cost
for one adjacent capsule by one round of new work. -/
theorem ValidatorPersistedCausalCapsuleRoundNoveltySourceMap.unresolved_sync_cost_le
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {newWorkPerRound : Nat}
    (source : ValidatorPersistedCausalCapsuleRoundNoveltySourceMap
      (syncRules := syncRules) newWorkPerRound)
    {previousBlock nextBlock : ValidatorBlock BlockId}
    {author receiver persistTime syncAt : Nat}
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author
        (.persistProposal nextBlock))
    (previousIsParent : previousBlock.reference ∈ nextBlock.parents)
    (adjacentRound : previousBlock.reference.round + 1 =
      nextBlock.reference.round)
    (cutoff : ValidatorAcceptedCausalCapsuleCutoffAt timed
      (source.capsuleFor previousBlock) syncAt receiver) :
    CausalRecoveryCapsuleExecutionSource syncRules
        (source.capsuleFor previousBlock) author persistTime ∧
      CausalRecoveryCapsuleExecutionSource syncRules
          (source.capsuleFor nextBlock) author (persistTime + 1) ∧
      validatorUnresolvedHistoryItemsAt timed.execution syncAt receiver
            (source.capsuleFor nextBlock).history *
              validatorBlockSyncAcceptanceBound timed syncRules ≤
        newWorkPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules := by
  have unresolvedWithinNovelty :=
    validator_unresolved_history_items_le_capsule_novel_history_items
      (blocks := (source.capsuleFor nextBlock).history) cutoff
  rcases source.correctPersistHasProjectedSourcesAndBoundedNovelty authorInRange
      authorCorrect persisted previousIsParent adjacentRound with
    ⟨previousSource, nextSource, noveltyBound⟩
  exact ⟨previousSource, nextSource, Nat.mul_le_mul_right _
    (Nat.le_trans unresolvedWithinNovelty noveltyBound)⟩

/-- One actual adjacent parent-sync source finishes within the receiver-relative
one-round delta. Entries which were already accepted or at the GC root at
`syncAt` contribute zero to this bound. -/
theorem ValidatorPersistedCausalCapsuleRoundNoveltySourceMap.adjacent_history_ready_within_delta
    {BlockId CommitId PacketId : Type} [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    {newWorkPerRound : Nat}
    (delta : ValidatorPersistedCausalCapsuleRoundNoveltySourceMap
      (syncRules := syncRules) newWorkPerRound)
    {previousBlock nextBlock : ValidatorBlock BlockId}
    {author receiver persistTime syncAt : Nat}
    (syncSource : ValidatorBlockParentSyncSource syncRules
      (delta.capsuleFor nextBlock).targetBlock receiver author
        (delta.capsuleFor nextBlock).history syncAt)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author
        (.persistProposal nextBlock))
    (previousIsParent : previousBlock.reference ∈ nextBlock.parents)
    (adjacentRound : previousBlock.reference.round + 1 =
      nextBlock.reference.round)
    (cutoff : ValidatorAcceptedCausalCapsuleCutoffAt timed
      (delta.capsuleFor previousBlock) syncAt receiver)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ syncAt)
    (active : ∀ time, syncAt ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ finish,
      syncAt ≤ finish ∧
        finish ≤ syncAt + newWorkPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules ∧
        ∀ block, block ∈ (delta.capsuleFor nextBlock).history →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution finish receiver
            block.reference := by
  have parentFirst : ParentFirstValidatorBlockHistory
      (ValidatorReferenceAcceptedOrGcRootAt timed.execution syncAt receiver)
      (delta.capsuleFor nextBlock).history :=
    parent_first_validator_block_history_mono (fun _ accepted => Or.inl accepted)
      syncSource.parentFirst
  rcases retained_parent_first_history_ready_within_unresolved_bound syncRules
      syncSource.history receiverInRange receiverCorrectAvailable afterGst active
        syncSource.protectedWhileIncomplete parentFirst with
    ⟨finish, syncBeforeFinish, finishBound, ready⟩
  rcases delta.unresolved_sync_cost_le authorInRange authorCorrect persisted
      previousIsParent adjacentRound cutoff with
    ⟨_previousSource, _nextSource, unresolvedBound⟩
  exact ⟨finish, syncBeforeFinish, Nat.le_trans finishBound
    (Nat.add_le_add_left unresolvedBound syncAt), ready⟩

/-- One round of receiver-relative causal synchronization plus local timer-arm
work. -/
def validatorReceiverRelativeTimerSpreadStepCost
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (newWorkPerRound : Nat) : Nat :=
  newWorkPerRound *
      validatorBlockSyncAcceptanceBound timed syncRules +
    timed.localActionBound + 2

/-- A lower bound from the prior earliest deadline and an upper bound from the
prior latest start give one additive timer-spread recurrence step. -/
theorem receiver_relative_timer_start_span_successor
    {previousEarliest previousLatest nextEarliest nextLatest previousSpread
      wait stepCost : Nat}
    (previousSpan : previousLatest ≤ previousEarliest + previousSpread)
    (nextLower : previousEarliest + wait ≤ nextEarliest)
    (nextUpper : nextLatest ≤ previousLatest + wait + stepCost) :
    nextLatest ≤ nextEarliest + (previousSpread + stepCost) := by
  omega

/-- Repeating one additive spread step gives a linear bound in the round gap.
This is pure arithmetic. It does not assume that any future timer exists. -/
theorem additive_timer_spread_recurrence_is_linear
    (spread : Nat → Nat) (stepCost distance : Nat)
    (step : ∀ offset, spread (offset + 1) ≤ spread offset + stepCost) :
    spread distance ≤ spread 0 + distance * stepCost := by
  induction distance with
  | zero => simp
  | succ previous inductionHypothesis =>
      calc
        spread (Nat.succ previous) ≤ spread previous + stepCost := by
          simpa only [Nat.succ_eq_add_one] using step previous
        _ ≤ (spread 0 + previous * stepCost) + stepCost :=
          Nat.add_le_add_right inductionHypothesis stepCost
        _ = spread 0 + Nat.succ previous * stepCost := by
          simp [Nat.succ_mul, Nat.add_assoc]

/-- Generic current-state authentication and durable correct-author ownership.

The rule applies to any exact accepted and catalogued body. It is not specific
to a timer or a future proposal. The authentication predicate binds the whole
body, including its exact reference. Correct ownership also states the local
persistence-before-publication and durable-storage refinement. -/
structure ValidatorAuthenticatedAcceptedBodyOwnershipRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program} : Type where
  authenticated : ValidatorBlock BlockId → Prop
  acceptedCataloguedBodyIsAuthenticated : ∀
    {time observer : Nat} {block : ValidatorBlock BlockId},
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    ((timed.execution.trace time).validatorState observer).accepted
        block.reference = true →
    (timed.execution.trace time).blockCatalog block.reference.id = some block →
    authenticated block
  correctAuthenticatedAcceptedBodyIsOwned : ∀
    {time observer : Nat} {block : ValidatorBlock BlockId},
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    block.reference.author < config.authorityCount →
    faults.correctAvailable block.reference.author = true →
    ((timed.execution.trace time).validatorState observer).accepted
        block.reference = true →
    (timed.execution.trace time).blockCatalog block.reference.id = some block →
    authenticated block →
    ((timed.execution.trace time).validatorState
        block.reference.author).ownBlockAt block.reference.round =
      some block.reference

/-- A correct initial-quorum parent above its author's time-zero signer floor
has an exact past proposal-persistence origin.

The timer's representative and body come from current well-formed state.
Authentication gives correct-author ownership. The ordinary trace theorem then
derives the past action, and the time-zero signer floor removes the restored
prefix case. -/
theorem correct_initial_quorum_parent_has_past_persist_origin
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (ownership : ValidatorAuthenticatedAcceptedBodyOwnershipRules
      (timed := timed))
    {start : ValidatorRecoveryTimerStart BlockId CommitId}
    (started : timerSource.timerStarted start)
    {parent : ValidatorBlockRef BlockId}
    (included : parent ∈ timerSource.initialQuorumParents start)
    (parentAuthorInRange : parent.author < config.authorityCount)
    (parentAuthorCorrect : faults.correctAvailable parent.author = true)
    (aboveInitialFloor :
      ((timed.execution.trace 0).validatorState
        parent.author).highestSignedRound < parent.round) :
    ∃ persistTime block,
      persistTime < start.startedAt ∧
        ValidatorLocalActionOccurs (timed.execution.events persistTime)
          parent.author (.persistProposal block) ∧
        block.reference = parent := by
  have timerValidatorInRange := timerSource.validatorInRange start started
  have timerValidatorCorrect :=
    timerSource.validatorCorrectAvailable start started
  have representative :=
    timerSource.initialQuorumParentsAreRepresentativesAtStart start started
      parent included
  have representativeSound :=
    (timed.execution.statesWellFormed start.startedAt start.validator
      timerValidatorInRange).acceptedRepresentativeIsSound
        (start.targetRound - 1) parent.author parent representative
  rcases representativeSound with
    ⟨_parentAuthor, _parentRound, accepted, block, catalogued,
      blockReference⟩
  have blockAccepted :
      ((timed.execution.trace start.startedAt).validatorState
        start.validator).accepted block.reference = true := by
    simpa only [blockReference] using accepted
  have blockCatalogued :
      (timed.execution.trace start.startedAt).blockCatalog
        block.reference.id = some block := by
    simpa only [blockReference] using catalogued
  have authenticated :=
    ownership.acceptedCataloguedBodyIsAuthenticated timerValidatorInRange
      timerValidatorCorrect blockAccepted blockCatalogued
  have ownedAtStart :=
    ownership.correctAuthenticatedAcceptedBodyIsOwned timerValidatorInRange
      timerValidatorCorrect (by simpa only [blockReference] using
        parentAuthorInRange) (by simpa only [blockReference] using
          parentAuthorCorrect) blockAccepted blockCatalogued authenticated
  have parentOwnedAtStart :
      ((timed.execution.trace start.startedAt).validatorState
        parent.author).ownBlockAt parent.round = some parent := by
    simpa only [blockReference] using ownedAtStart
  rcases validator_trace_own_block_has_past_persist_origin timed
      parentOwnedAtStart with initial | persisted
  · have initialBelowFloor :=
      (timed.execution.statesWellFormed 0 parent.author parentAuthorInRange)
        |>.ownBlockDoesNotExceedSignerFloor parent.round parent initial
    exact False.elim ((Nat.not_lt_of_ge initialBelowFloor) aboveInitialFloor)
  · rcases persisted with
      ⟨persistTime, persistedBlock, beforeStart, occurs, _round,
        exactReference⟩
    exact ⟨persistTime, persistedBlock, beforeStart, occurs, exactReference⟩

/-- Two actual proposal persistences by one validator with the same exact
reference occur in the same logical-time batch. -/
theorem persist_proposal_reference_times_are_equal
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator left right : Nat}
    {leftBlock rightBlock : ValidatorBlock BlockId}
    (validatorInRange : validator < config.authorityCount)
    (leftOccurs : ValidatorLocalActionOccurs (timed.execution.events left)
      validator (.persistProposal leftBlock))
    (rightOccurs : ValidatorLocalActionOccurs (timed.execution.events right)
      validator (.persistProposal rightBlock))
    (sameReference : leftBlock.reference = rightBlock.reference) :
    left = right := by
  have sameRound : leftBlock.reference.round = rightBlock.reference.round :=
    congrArg ValidatorBlockRef.round sameReference
  have notLeftBeforeRight : ¬left < right := by
    intro leftBeforeRight
    have stored := persist_proposal_occurrence_stores_own_block
      timed.execution leftOccurs
    have floorAtLeft :=
      (timed.execution.statesWellFormed (left + 1) validator validatorInRange)
        |>.ownBlockDoesNotExceedSignerFloor leftBlock.reference.round
          leftBlock.reference stored
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator (left + 1) right
        validatorInRange (Nat.succ_le_iff.mpr leftBeforeRight)).2.2.2.2.2.2.1
    have rightBelow := persist_proposal_occurrence_starts_below_proposed_round
      timed.execution rightOccurs
    rw [← sameRound] at rightBelow
    exact (Nat.not_lt_of_ge (Nat.le_trans floorAtLeft floorMonotone)) rightBelow
  have notRightBeforeLeft : ¬right < left := by
    intro rightBeforeLeft
    have stored := persist_proposal_occurrence_stores_own_block
      timed.execution rightOccurs
    have floorAtRight :=
      (timed.execution.statesWellFormed (right + 1) validator validatorInRange)
        |>.ownBlockDoesNotExceedSignerFloor rightBlock.reference.round
          rightBlock.reference stored
    have floorMonotone :=
      (timed.execution.durableStateMonotone validator (right + 1) left
        validatorInRange (Nat.succ_le_iff.mpr rightBeforeLeft)).2.2.2.2.2.2.1
    have leftBelow := persist_proposal_occurrence_starts_below_proposed_round
      timed.execution leftOccurs
    rw [sameRound] at leftBelow
    exact (Nat.not_lt_of_ge (Nat.le_trans floorAtRight floorMonotone)) leftBelow
  exact Nat.le_antisymm (Nat.le_of_not_gt notRightBeforeLeft)
    (Nat.le_of_not_gt notLeftBeforeRight)

/-- A correct prior proposal selected in the actual next timer's initial
quorum puts that next timer strictly after the prior proposal deadline.

This is the lower-start edge needed by a timer-spread recurrence. It refers
only to two actual past/current timer records and exact proposal persistence. -/
theorem correct_initial_parent_gives_next_timer_lower_start
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {waits : CommonRoundWaitSchedule (ValidatorCommitHead CommitId)}
    (timerSource : ValidatorRecoveryTimerSourceMap faults protocolPacket
      network program timed waits)
    (ownership : ValidatorAuthenticatedAcceptedBodyOwnershipRules
      (timed := timed))
    {author round : Nat}
    (previous : ValidatorTimerPacedRoundProduction timed waits author round)
    {nextStart : ValidatorRecoveryTimerStart BlockId CommitId}
    (nextStarted : timerSource.timerStarted nextStart)
    {parent : ValidatorBlockRef BlockId}
    (included : parent ∈ timerSource.initialQuorumParents nextStart)
    (exactParent : parent = previous.snapshot.block.reference)
    (aboveInitialFloor :
      ((timed.execution.trace 0).validatorState
        parent.author).highestSignedRound < parent.round) :
    previous.timerStartedAt + waits.wait previous.commitHead round <
      nextStart.startedAt := by
  have parentAuthor : parent.author = author := by
    calc
      parent.author = previous.snapshot.block.reference.author := by
        rw [exactParent]
      _ = previous.snapshot.proposer := previous.snapshot.blockIsOwnProposal
      _ = author := previous.proposer
  have parentAuthorInRange : parent.author < config.authorityCount := by
    rw [parentAuthor]
    simpa [previous.proposer] using previous.snapshot.proposerInRange
  have parentAuthorCorrect : faults.correctAvailable parent.author = true := by
    rw [parentAuthor]
    simpa [previous.proposer] using
      previous.snapshot.proposerCorrectAvailable
  rcases correct_initial_quorum_parent_has_past_persist_origin timerSource
      ownership nextStarted included parentAuthorInRange parentAuthorCorrect
        aboveInitialFloor with
    ⟨persistTime, persistedBlock, persistedBeforeStart, persisted,
      persistedReference⟩
  have persistedByAuthor : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author
        (.persistProposal persistedBlock) := by
    simpa only [parentAuthor] using persisted
  have sameReference : previous.snapshot.block.reference =
      persistedBlock.reference := by
    rw [persistedReference, exactParent]
  have samePersistTime : previous.persistTime = persistTime :=
    persist_proposal_reference_times_are_equal
      (by simpa [previous.proposer] using previous.snapshot.proposerInRange)
      previous.persistenceOccurs persistedByAuthor sameReference
  have deadlineBeforePersistence :
      previous.timerStartedAt + waits.wait previous.commitHead round <
        previous.persistTime := by
    exact Nat.lt_of_le_of_lt previous.deadlineBeforeProposal
      (Nat.lt_of_succ_le previous.proposalBeforePersistence)
  rw [samePersistTime] at deadlineBeforePersistence
  exact Nat.lt_trans deadlineBeforePersistence persistedBeforeStart

end Mysticeti
