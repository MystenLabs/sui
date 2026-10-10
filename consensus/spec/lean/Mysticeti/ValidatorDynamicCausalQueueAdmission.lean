/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorPostGstCausalQueueService
import Mysticeti.ValidatorRecoveryCapsuleSyncExecution

namespace Mysticeti

/-!
Dynamic BlockManager admission for the post-GST causal-work queue.

The fixed capsule is source data. It is not a fetch manifest. The requester
starts with only the target body. Processing that body admits each unresolved
direct parent above GC. Processing a later fetched body applies the same rule
to that body's direct parents.

The adapter below contains only a current sampled queue fact derived from a
body that was already local. It contains no future body, delivery, acceptance,
carrier, proposal, Flex result, or commit install.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- A past local body admits one unresolved direct parent to the sampled
causal-work queue.

`bodyTime + 1` is the end of the execution batch which processed the body. The
queue sample is not earlier than that batch. The parent is admitted only when
it is still unaccepted and above the GC round at the sample. -/
structure ValidatorDynamicCausalQueueAdmission
    [DecidableEq BlockId]
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {validator start : Nat}
    (queue : ValidatorPostGstCausalQueueServiceRules timed validator start) :
    Prop where
  localBodyAdmitsMissingDirectParent : ∀
      {bodyTime interval : Time}
      {child : ValidatorBlock BlockId}
      {parent : ValidatorBlockRef BlockId},
    bodyTime + 1 ≤
      validatorPostGstCausalQueueSampleTime start queue.serviceInterval
        interval →
    ValidatorLocalBlockBodyAt timed bodyTime validator child →
    parent ∈ child.parents →
    ((timed.execution.trace
      (validatorPostGstCausalQueueSampleTime start queue.serviceInterval
        interval)).validatorState validator).accepted parent = false →
    ((timed.execution.trace
      (validatorPostGstCausalQueueSampleTime start queue.serviceInterval
        interval)).validatorState validator).gcRound < parent.round →
    queue.known interval parent

namespace ValidatorDynamicCausalQueueAdmission

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

/-- A delivered capsule target and dynamic direct-parent admission expose every
capsule body or reach its GC root.

The proof walks the finite capsule in reverse topological order. A processed
child body admits only its direct parent. Queue service makes that exact parent
accepted or GC-obsolete. An accepted parent supplies its catalogued body, which
can reveal the next parent. A child at or below GC makes its older parent a GC
root without a fetch. -/
theorem delivered_target_eventually_discovers_with_dynamic_queue
    {start sourceStart targetTime firstInterval holder requester : Time}
    (queue : ValidatorPostGstCausalQueueServiceRules timed requester start)
    (admission : ValidatorDynamicCausalQueueAdmission queue)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (stored : (pins.trace sourceStart holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace sourceStart holder).pinned capsuleKey = true)
    (sourceBeforeTarget : sourceStart ≤ targetTime)
    (targetProcessedBeforeFirstSample : targetTime + 1 ≤
      validatorPostGstCausalQueueSampleTime start queue.serviceInterval
        firstInterval)
    (targetBody : ValidatorLocalBlockBodyAt timed targetTime requester
      entry.capsule.targetBlock) :
    ∃ finishInterval,
      firstInterval ≤ finishInterval ∧
        ∀ block, block ∈ entry.capsule.history →
          ValidatorRecoveryCapsuleSyncExecution.BodyObservedOrGcRootAt
            (timed := timed) sourceStart
              (validatorPostGstCausalQueueSampleTime start
                queue.serviceInterval finishInterval) requester block := by
  classical
  let sampleTime := fun interval =>
    validatorPostGstCausalQueueSampleTime start queue.serviceInterval interval
  have source := pins.pinned_capsule_is_execution_source holderInRange
    holderCorrectAvailable stored pinned
  have sourceBeforeFirstSample : sourceStart ≤ sampleTime firstInterval :=
    Nat.le_trans sourceBeforeTarget
      (Nat.le_trans (Nat.le_add_right targetTime 1)
        targetProcessedBeforeFirstSample)
  have targetReady :
      ValidatorRecoveryCapsuleSyncExecution.BodyObservedOrGcRootAt
        (timed := timed) sourceStart (sampleTime firstInterval) requester
          entry.capsule.targetBlock :=
    Or.inl ⟨targetTime, sourceBeforeTarget,
      Nat.le_trans (Nat.le_add_right targetTime 1)
        targetProcessedBeforeFirstSample, targetBody⟩
  have advance : ∀ remaining processed currentInterval,
      entry.capsule.history.reverse = processed ++ remaining →
      firstInterval ≤ currentInterval →
      (∀ block, block ∈ processed →
        ValidatorRecoveryCapsuleSyncExecution.BodyObservedOrGcRootAt
          (timed := timed) sourceStart (sampleTime currentInterval) requester
            block) →
      ∃ finishInterval,
        currentInterval ≤ finishInterval ∧
          ∀ block, block ∈ processed ++ remaining →
            ValidatorRecoveryCapsuleSyncExecution.BodyObservedOrGcRootAt
              (timed := timed) sourceStart (sampleTime finishInterval)
                requester block := by
    intro remaining
    induction remaining with
    | nil =>
        intro processed currentInterval _split _firstBeforeCurrent
          processedReady
        exact ⟨currentInterval, Nat.le_refl _, by simpa using processedReady⟩
    | cons block tail inductionHypothesis =>
        intro processed currentInterval reverseSplit firstBeforeCurrent
          processedReady
        have currentTimeAfterSource : sourceStart ≤ sampleTime currentInterval :=
          Nat.le_trans sourceBeforeFirstSample
            (queue.sample_time_mono firstBeforeCurrent)
        have blockReady : ∃ nextInterval,
            currentInterval ≤ nextInterval ∧
              ValidatorRecoveryCapsuleSyncExecution.BodyObservedOrGcRootAt
                (timed := timed) sourceStart (sampleTime nextInterval)
                  requester block := by
          by_cases isTarget : block = entry.capsule.targetBlock
          · subst block
            exact ⟨currentInterval, Nat.le_refl _,
              ValidatorRecoveryCapsuleSyncExecution.body_observed_or_gc_root_mono
                (timed := timed) requesterInRange
                  (queue.sample_time_mono firstBeforeCurrent) targetReady⟩
          · have originalSplit :
                entry.capsule.history =
                  tail.reverse ++ block :: processed.reverse := by
              have reversed := congrArg List.reverse reverseSplit
              simpa [List.reverse_append, List.append_assoc] using reversed
            rcases
                ValidatorRecoveryCapsuleSyncExecution.non_target_history_block_has_child_in_suffix
                  entry.capsule originalSplit isTarget with
              ⟨child, childInProcessedReverse, blockParentOfChild⟩
            have childInProcessed : child ∈ processed := by
              simpa using childInProcessedReverse
            have childMember : child ∈ entry.capsule.history := by
              rw [originalSplit]
              simp [childInProcessedReverse]
            have blockMember : block ∈ entry.capsule.history := by
              rw [originalSplit]
              simp
            have childValid := entry.capsule.positiveHistoryBlocksValid child
              childMember (entry.capsule.historyBlocksPositive child childMember)
            have parentRound :=
              childValid.2.1 block.reference blockParentOfChild
            rcases processedReady child childInProcessed with
              childObserved | childAtRoot
            · rcases childObserved with
                ⟨childObservedAt, sourceBeforeChild, childBeforeCurrent,
                  childBody⟩
              let nextInterval := currentInterval + 1
              have currentBeforeNext : currentInterval ≤ nextInterval := by
                exact Nat.le_add_right _ _
              have currentTimeBeforeNext :
                  sampleTime currentInterval < sampleTime nextInterval := by
                simpa [nextInterval, sampleTime] using
                  queue.next_sample_is_later currentInterval
              have childProcessedBeforeNext :
                  childObservedAt + 1 ≤ sampleTime nextInterval := by
                exact Nat.lt_of_le_of_lt childBeforeCurrent
                  currentTimeBeforeNext
              cases parentAccepted :
                  ((timed.execution.trace (sampleTime nextInterval)).validatorState
                    requester).accepted block.reference with
              | true =>
                  have catalogAtNext := timed.execution.blockCatalogMonotone
                    sourceStart (sampleTime nextInterval)
                      (Nat.le_trans currentTimeAfterSource
                        (Nat.le_of_lt currentTimeBeforeNext))
                      block.reference.id block (source.catalog block blockMember)
                  exact ⟨nextInterval, currentBeforeNext, Or.inl
                    ⟨sampleTime nextInterval,
                      Nat.le_trans currentTimeAfterSource
                        (Nat.le_of_lt currentTimeBeforeNext), Nat.le_refl _,
                      .acceptedCatalogued parentAccepted catalogAtNext⟩⟩
              | false =>
                  by_cases parentAtRoot : block.reference.round ≤
                      ((timed.execution.trace
                        (sampleTime nextInterval)).validatorState
                          requester).gcRound
                  · exact ⟨nextInterval, currentBeforeNext, Or.inr parentAtRoot⟩
                  · have parentAboveGc :
                        ((timed.execution.trace
                          (sampleTime nextInterval)).validatorState
                            requester).gcRound < block.reference.round := by
                      omega
                    have parentKnown :=
                      admission.localBodyAdmitsMissingDirectParent
                        childProcessedBeforeNext childBody blockParentOfChild
                          parentAccepted parentAboveGc
                    rcases queue.known_work_eventually_resolved nextInterval with
                      ⟨finishInterval, nextBeforeFinish, resolved⟩
                    have currentBeforeFinish :
                        currentInterval ≤ finishInterval :=
                      Nat.le_trans currentBeforeNext nextBeforeFinish
                    rcases resolved block.reference parentKnown with
                      parentAcceptedAtFinish | parentAtRootAtFinish
                    · have sourceBeforeFinish :
                          sourceStart ≤ sampleTime finishInterval :=
                        Nat.le_trans currentTimeAfterSource
                          (queue.sample_time_mono currentBeforeFinish)
                      have catalogAtFinish :=
                        timed.execution.blockCatalogMonotone sourceStart
                          (sampleTime finishInterval) sourceBeforeFinish
                            block.reference.id block
                              (source.catalog block blockMember)
                      exact ⟨finishInterval, currentBeforeFinish, Or.inl
                        ⟨sampleTime finishInterval, sourceBeforeFinish,
                          Nat.le_refl _, .acceptedCatalogued
                            parentAcceptedAtFinish catalogAtFinish⟩⟩
                    · exact ⟨finishInterval, currentBeforeFinish,
                        Or.inr parentAtRootAtFinish⟩
            · exact ⟨currentInterval, Nat.le_refl _, Or.inr (by omega)⟩
        rcases blockReady with ⟨nextInterval, currentBeforeNext, readyAtNext⟩
        have nextSplit :
            entry.capsule.history.reverse =
              (processed ++ [block]) ++ tail := by
          simpa [List.append_assoc] using reverseSplit
        have processedReadyAtNext : ∀ item, item ∈ processed ++ [block] →
            ValidatorRecoveryCapsuleSyncExecution.BodyObservedOrGcRootAt
              (timed := timed) sourceStart (sampleTime nextInterval) requester
                item := by
          intro item itemMember
          rcases List.mem_append.mp itemMember with
            itemInProcessed | itemIsBlock
          · exact
              ValidatorRecoveryCapsuleSyncExecution.body_observed_or_gc_root_mono
                (timed := timed) requesterInRange
                  (queue.sample_time_mono currentBeforeNext)
                    (processedReady item itemInProcessed)
          · have sameBlock : item = block := by simpa using itemIsBlock
            simpa [sameBlock] using readyAtNext
        rcases inductionHypothesis (processed ++ [block]) nextInterval nextSplit
            (Nat.le_trans firstBeforeCurrent currentBeforeNext)
            processedReadyAtNext with
          ⟨finishInterval, nextBeforeFinish, allReady⟩
        exact ⟨finishInterval, Nat.le_trans currentBeforeNext nextBeforeFinish,
          by simpa [List.append_assoc] using allReady⟩
  rcases advance entry.capsule.history.reverse [] firstInterval (by simp)
      (Nat.le_refl _) (by simp) with
    ⟨finishInterval, firstBeforeFinish, allReady⟩
  refine ⟨finishInterval, firstBeforeFinish, ?_⟩
  intro block blockMember
  apply allReady
  simpa using blockMember

/-- Dynamic queue admission and normal parent-first BlockManager acceptance
make one fixed pinned capsule accepted or GC-obsolete at one common time.

Only the target body is initially local. Transitive references enter the queue
when the already-local child body reveals them. -/
theorem delivered_target_eventually_resolves_pinned_capsule
    {start sourceStart targetTime firstInterval holder requester : Time}
    (queue : ValidatorPostGstCausalQueueServiceRules timed requester start)
    (admission : ValidatorDynamicCausalQueueAdmission queue)
    (pins : ValidatorRecoverySourcePinExecution syncRules)
    (acceptance : ValidatorRecoveryGcParentReadyAcceptanceRules timed)
    {capsuleKey : ValidatorRecoveryCapsuleKey BlockId} {entry}
    (holderInRange : holder < config.authorityCount)
    (holderCorrectAvailable : faults.correctAvailable holder = true)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrectAvailable : faults.correctAvailable requester = true)
    (stored : (pins.trace sourceStart holder).capsuleAt capsuleKey = some entry)
    (pinned : (pins.trace sourceStart holder).pinned capsuleKey = true)
    (sourceBeforeTarget : sourceStart ≤ targetTime)
    (targetProcessedBeforeFirstSample : targetTime + 1 ≤
      validatorPostGstCausalQueueSampleTime start queue.serviceInterval
        firstInterval)
    (targetBody : ValidatorLocalBlockBodyAt timed targetTime requester
      entry.capsule.targetBlock) :
    ∃ finish,
      validatorPostGstCausalQueueSampleTime start queue.serviceInterval
          firstInterval ≤ finish ∧
        ∀ block, block ∈ entry.capsule.history →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution finish requester
            block.reference := by
  rcases delivered_target_eventually_discovers_with_dynamic_queue queue
      admission pins holderInRange holderCorrectAvailable requesterInRange
      stored pinned sourceBeforeTarget targetProcessedBeforeFirstSample
      targetBody with
    ⟨discoveryInterval, firstBeforeDiscovery, discovered⟩
  let discoveryTime := validatorPostGstCausalQueueSampleTime start
    queue.serviceInterval discoveryInterval
  rcases
      ValidatorRecoveryCapsuleSyncExecution.discovered_history_eventually_accepted_or_gc_root
        pins acceptance holderInRange holderCorrectAvailable requesterInRange
          requesterCorrectAvailable stored pinned discovered with
    ⟨finish, discoveryBeforeFinish, ready⟩
  refine ⟨finish, ?_, ?_⟩
  · exact Nat.le_trans (queue.sample_time_mono firstBeforeDiscovery)
      (Nat.le_trans (Nat.le_add_right discoveryTime 1) discoveryBeforeFinish)
  · intro block blockMember
    simpa [ValidatorReferenceAcceptedOrGcRootAt] using
      ready block blockMember

end ValidatorDynamicCausalQueueAdmission

end Mysticeti
