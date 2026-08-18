/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorRecoveryCapsuleSyncExecution
import Mysticeti.ValidatorTimerSpreadRecurrence

namespace Mysticeti

/-!
Accepted causal capsules give receiver cutoffs.

The validator acceptance invariant is parent-first above GC. A causal capsule
contains only the target and blocks on paths to that target, in parent-first
order. Thus, when one receiver has accepted the target, every capsule body is
also accepted or is at or below that receiver's current GC round.

This module proves a current-state fact. It does not state future delivery,
synchronization, proposal, layer, timer, or commit progress.
-/

/-- Acceptance of one exact capsule target gives an accepted-or-GC cutoff for
the complete capsule at the same receiver and time.

The source time is used only to identify each immutable exact body in the
global catalog. -/
theorem accepted_capsule_target_gives_receiver_cutoff
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
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {holder receiver sourceTime cutoffTime : Nat}
    (source : CausalRecoveryCapsuleExecutionSource syncRules capsule holder
      sourceTime)
    (sourceBeforeCutoff : sourceTime ≤ cutoffTime)
    (closure : ValidatorAcceptedCausalClosureAboveGcAt config
      (timed.execution.trace cutoffTime) receiver)
    (targetAccepted :
      ((timed.execution.trace cutoffTime).validatorState receiver).accepted
        capsule.targetBlock.reference = true) :
    ValidatorAcceptedCausalCapsuleCutoffAt timed capsule cutoffTime receiver := by
  classical
  let ready := fun block : ValidatorBlock BlockId =>
    ValidatorReferenceAcceptedOrGcRootAt timed.execution cutoffTime receiver
      block.reference
  have targetReady : ready capsule.targetBlock := Or.inl targetAccepted
  have advance : ∀ remaining processed,
      capsule.history.reverse = processed ++ remaining →
      (∀ block, block ∈ processed → ready block) →
      ∀ block, block ∈ processed ++ remaining → ready block := by
    intro remaining
    induction remaining with
    | nil =>
        intro processed _reverseSplit processedReady block blockMember
        exact processedReady block (by simpa using blockMember)
    | cons block tail inductionHypothesis =>
        intro processed reverseSplit processedReady
        have blockReady : ready block := by
          by_cases isTarget : block = capsule.targetBlock
          · simpa [isTarget] using targetReady
          · have originalSplit :
                capsule.history = tail.reverse ++ block :: processed.reverse := by
              have reversed := congrArg List.reverse reverseSplit
              simpa [List.reverse_append, List.append_assoc] using reversed
            rcases
                ValidatorRecoveryCapsuleSyncExecution.non_target_history_block_has_child_in_suffix
                  capsule originalSplit isTarget with
              ⟨child, childInProcessedReverse, blockParentOfChild⟩
            have childInProcessed : child ∈ processed := by
              simpa using childInProcessedReverse
            have childReady := processedReady child childInProcessed
            have childMember : child ∈ capsule.history := by
              rw [originalSplit]
              simp [childInProcessedReverse]
            have childValid := capsule.positiveHistoryBlocksValid child
              childMember (capsule.historyBlocksPositive child childMember)
            have parentRound :=
              childValid.2.1 block.reference blockParentOfChild
            rcases childReady with childAccepted | childAtRoot
            · by_cases blockAtRoot : block.reference.round ≤
                  ((timed.execution.trace cutoffTime).validatorState
                    receiver).gcRound
              · exact Or.inr blockAtRoot
              · have blockAboveGc :
                    ((timed.execution.trace cutoffTime).validatorState
                        receiver).gcRound < block.reference.round := by
                  omega
                have childCatalogAtSource := source.catalog child childMember
                have childCatalogAtCutoff :=
                  timed.execution.blockCatalogMonotone sourceTime cutoffTime
                    sourceBeforeCutoff child.reference.id child
                      childCatalogAtSource
                exact Or.inl
                  (closure.acceptedBodyHasAcceptedParentsAboveGc
                    child.reference child block.reference childCatalogAtCutoff
                      rfl childAccepted blockParentOfChild blockAboveGc)
            · exact Or.inr (by omega)
        have nextSplit : capsule.history.reverse =
            (processed ++ [block]) ++ tail := by
          simpa [List.append_assoc] using reverseSplit
        have processedReadyNext : ∀ item, item ∈ processed ++ [block] →
            ready item := by
          intro item itemMember
          rcases List.mem_append.mp itemMember with
            itemInProcessed | itemIsBlock
          · exact processedReady item itemInProcessed
          · have sameBlock : item = block := by simpa using itemIsBlock
            simpa [sameBlock] using blockReady
        intro item itemMember
        exact inductionHypothesis (processed ++ [block]) nextSplit
          processedReadyNext item (by
            simpa [List.append_assoc] using itemMember)
  have allReverseReady := advance capsule.history.reverse [] (by simp) (by simp)
  intro block blockMember
  exact allReverseReady block (by simpa using blockMember)

end Mysticeti
