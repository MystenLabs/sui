/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorAcceptedCapsuleCutoff

namespace Mysticeti

/-! Receiver-relative linear causal-backlog bounds.

A comparison with one selected parent capsule is not sufficient. A correct
child can merge a sibling parent's causal branch which is absent from that
selected capsule. This module instead counts the exact bodies which are still
unresolved at one receiver.

The source rule is current/past only. It applies to an actual persisted block,
its projected causal capsule, and a receiver state which already has a round
cutoff. It does not state a future delivery, timer, proposal, window, layer, or
commit result.
-/

/-- Number of bodies in one exact history at one round. Capsule reference
uniqueness makes this also the number of distinct admitted references at that
round. -/
def validatorCausalHistoryItemsAtRound
    {BlockId : Type} (round : Nat) : List (ValidatorBlock BlockId) → Nat
  | [] => 0
  | block :: remaining =>
      if block.reference.round = round then
        validatorCausalHistoryItemsAtRound round remaining + 1
      else
        validatorCausalHistoryItemsAtRound round remaining

/-- Number of bodies whose rounds are in `(floor, ceiling]`. -/
def validatorCausalHistoryItemsBetweenRounds
    {BlockId : Type} (floor ceiling : Nat) :
    List (ValidatorBlock BlockId) → Nat
  | [] => 0
  | block :: remaining =>
      if floor < block.reference.round ∧
          block.reference.round ≤ ceiling then
        validatorCausalHistoryItemsBetweenRounds floor ceiling remaining + 1
      else
        validatorCausalHistoryItemsBetweenRounds floor ceiling remaining

/-- Extending the upper endpoint by one partitions the exact history into the
old interval and the new endpoint round. -/
theorem validator_causal_history_items_between_rounds_succ
    {BlockId : Type} (blocks : List (ValidatorBlock BlockId))
    {floor ceiling : Nat} (floorAtMostCeiling : floor ≤ ceiling) :
    validatorCausalHistoryItemsBetweenRounds floor (ceiling + 1) blocks =
      validatorCausalHistoryItemsBetweenRounds floor ceiling blocks +
        validatorCausalHistoryItemsAtRound (ceiling + 1) blocks := by
  induction blocks with
  | nil =>
      simp [validatorCausalHistoryItemsBetweenRounds,
        validatorCausalHistoryItemsAtRound]
  | cons block remaining inductionHypothesis =>
      simp only [validatorCausalHistoryItemsBetweenRounds,
        validatorCausalHistoryItemsAtRound]
      by_cases atNext : block.reference.round = ceiling + 1
      · have inExtended : floor < block.reference.round ∧
            block.reference.round ≤ ceiling + 1 := by
          omega
        have notInOld : ¬(floor < block.reference.round ∧
            block.reference.round ≤ ceiling) := by
          omega
        rw [if_pos inExtended, if_neg notInOld, if_pos atNext,
          inductionHypothesis]
        omega
      · by_cases inOld : floor < block.reference.round ∧
            block.reference.round ≤ ceiling
        · have inExtended : floor < block.reference.round ∧
              block.reference.round ≤ ceiling + 1 := by
            omega
          rw [if_pos inExtended, if_pos inOld, if_neg atNext,
            inductionHypothesis]
          omega
        · have notInExtended : ¬(floor < block.reference.round ∧
              block.reference.round ≤ ceiling + 1) := by
            intro inExtended
            have atMostOrExact : block.reference.round ≤ ceiling ∨
                block.reference.round = ceiling + 1 := by
              omega
            rcases atMostOrExact with atMost | exactNext
            · exact inOld ⟨inExtended.1, atMost⟩
            · exact atNext exactNext
          rw [if_neg notInExtended, if_neg inOld, if_neg atNext,
            inductionHypothesis]

/-- The interval `(floor, ceiling]` is empty when its upper endpoint is not
above its lower endpoint. -/
theorem validator_causal_history_items_between_rounds_eq_zero
    {BlockId : Type} (blocks : List (ValidatorBlock BlockId))
    {floor ceiling : Nat} (ceilingAtMostFloor : ceiling ≤ floor) :
    validatorCausalHistoryItemsBetweenRounds floor ceiling blocks = 0 := by
  induction blocks with
  | nil => simp [validatorCausalHistoryItemsBetweenRounds]
  | cons block remaining inductionHypothesis =>
      simp only [validatorCausalHistoryItemsBetweenRounds]
      have outside : ¬(floor < block.reference.round ∧
          block.reference.round ≤ ceiling) := by
        omega
      rw [if_neg outside, inductionHypothesis]

/-- A uniform admission cap at each round gives a linear bound on the number
of bodies in a round interval. -/
theorem validator_causal_history_items_between_rounds_le_linear
    {BlockId : Type} (blocks : List (ValidatorBlock BlockId))
    (maxAdmittedRefsPerRound floor ceiling : Nat)
    (perRound : ∀ round,
      validatorCausalHistoryItemsAtRound round blocks ≤
        maxAdmittedRefsPerRound) :
    validatorCausalHistoryItemsBetweenRounds floor ceiling blocks ≤
      (ceiling - floor) * maxAdmittedRefsPerRound := by
  induction ceiling with
  | zero =>
      rw [validator_causal_history_items_between_rounds_eq_zero blocks
        (Nat.zero_le floor)]
      simp
  | succ previous inductionHypothesis =>
      by_cases floorAtMostPrevious : floor ≤ previous
      · have split := validator_causal_history_items_between_rounds_succ
          blocks floorAtMostPrevious
        rw [split]
        have previousBound := inductionHypothesis
        have endpointBound := perRound (previous + 1)
        have distanceStep : previous + 1 - floor =
            (previous - floor) + 1 := by
          omega
        rw [distanceStep, Nat.add_mul]
        exact Nat.add_le_add previousBound (by simpa using endpointBound)
      · have floorAtLeastNext : previous + 1 ≤ floor := by omega
        have intervalEmpty :=
          validator_causal_history_items_between_rounds_eq_zero blocks
            floorAtLeastNext
        have distanceZero : previous + 1 - floor = 0 := by omega
        simp [intervalEmpty, distanceZero]

/-- At one receiver state, every body in the projected capsule through one
round is already accepted or is at or below the current GC root. -/
def ValidatorAcceptedCausalCapsuleRoundCutoffAt
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
    (time receiver floor : Nat) : Prop :=
  ∀ block, block ∈ capsule.history → block.reference.round ≤ floor →
    ValidatorReferenceAcceptedOrGcRootAt timed.execution time receiver
      block.reference

/-- A complete accepted capsule cutoff supplies a cutoff at every round. -/
theorem validator_accepted_causal_capsule_cutoff_gives_round_cutoff
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {time receiver floor : Nat}
    (cutoff : ValidatorAcceptedCausalCapsuleCutoffAt timed capsule time
      receiver) :
    ValidatorAcceptedCausalCapsuleRoundCutoffAt timed capsule time receiver
      floor := by
  intro block member _atMostFloor
  exact cutoff block member

/-- The receiver's current GC round gives an exact cutoff for the part of any
projected history at or below that round. -/
theorem validator_gc_round_gives_causal_capsule_round_cutoff
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {capsule : CausalRecoveryCapsule (BlockId := BlockId) config}
    {time receiver : Nat} :
    ValidatorAcceptedCausalCapsuleRoundCutoffAt timed capsule time receiver
      ((timed.execution.trace time).validatorState receiver).gcRound := by
  intro block member atMostGc
  exact Or.inr atMostGc

/-- An exact prefix-coverage fact lets an accepted earlier capsule supply the
round cutoff for a later capsule. This fact is not implied by one parent edge:
a sibling-parent merge can add older history. -/
def ValidatorCausalCapsulePrefixCoveredBy
    {BlockId CommitId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    (earlier later : CausalRecoveryCapsule (BlockId := BlockId) config)
    (floor : Nat) : Prop :=
  ∀ block, block ∈ later.history → block.reference.round ≤ floor →
    block.reference ∈ earlier.history.map ValidatorBlock.reference

/-- Prefix coverage transfers one accepted earlier capsule to a later
receiver-relative round cutoff. -/
theorem validator_causal_capsule_prefix_coverage_gives_round_cutoff
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
    {earlier later : CausalRecoveryCapsule (BlockId := BlockId) config}
    {time receiver floor : Nat}
    (coverage : ValidatorCausalCapsulePrefixCoveredBy earlier later floor)
    (cutoff : ValidatorAcceptedCausalCapsuleCutoffAt timed earlier time
      receiver) :
    ValidatorAcceptedCausalCapsuleRoundCutoffAt timed later time receiver
      floor := by
  intro block member atMostFloor
  rcases List.mem_map.mp (coverage block member atMostFloor) with
    ⟨earlierBlock, earlierMember, sameReference⟩
  rw [← sameReference]
  exact cutoff earlierBlock earlierMember

/-- A receiver cutoff makes each unresolved body belong to the remaining
round interval. -/
theorem validator_unresolved_history_items_le_between_rounds
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {blocks : List (ValidatorBlock BlockId)}
    {time receiver floor ceiling : Nat}
    (cutoff : ∀ block, block ∈ blocks → block.reference.round ≤ floor →
      ValidatorReferenceAcceptedOrGcRootAt timed.execution time receiver
        block.reference)
    (upper : ∀ block, block ∈ blocks →
      block.reference.round ≤ ceiling) :
    validatorUnresolvedHistoryItemsAt timed.execution time receiver
        blocks ≤
      validatorCausalHistoryItemsBetweenRounds floor ceiling blocks := by
  induction blocks with
  | nil =>
      simp [validatorUnresolvedHistoryItemsAt,
        validatorCausalHistoryItemsBetweenRounds]
  | cons block remaining inductionHypothesis =>
      have blockUpper : block.reference.round ≤ ceiling :=
        upper block (by simp)
      have remainingUpper : ∀ item, item ∈ remaining →
          item.reference.round ≤ ceiling := by
        intro item member
        exact upper item (by simp [member])
      have remainingCutoff : ∀ item, item ∈ remaining →
          item.reference.round ≤ floor →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution time receiver
            item.reference := by
        intro item member atMostFloor
        exact cutoff item (by simp [member]) atMostFloor
      have tailBound := inductionHypothesis remainingCutoff remainingUpper
      by_cases ready : ValidatorReferenceAcceptedOrGcRootAt timed.execution
          time receiver block.reference
      · rw [validator_unresolved_history_items_cons_of_ready ready]
        simp only [validatorCausalHistoryItemsBetweenRounds]
        split
        · exact Nat.le_trans tailBound (Nat.le_add_right _ _)
        · exact tailBound
      · have aboveFloor : floor < block.reference.round := by
          have notAtMost : ¬block.reference.round ≤ floor := by
            intro atMost
            exact ready (cutoff block (by simp) atMost)
          omega
        rw [validator_unresolved_history_items_cons_of_not_ready ready]
        simp [validatorCausalHistoryItemsBetweenRounds, aboveFloor, blockUpper,
          tailBound]

/-- Current/past source mapping for one actual persisted block.

`maxAdmittedRefsPerRound` is a protocol admission cap on the exact recoverable
closure of one correct block. Current Rust limits immediate parents, but it
does not yet enforce this transitive per-round cap in the presence of merged
equivocating branches. -/
structure ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
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
    (maxAdmittedRefsPerRound : Nat) : Type where
  capsuleFor : ValidatorBlock BlockId →
    CausalRecoveryCapsule (BlockId := BlockId) config
  capsuleTargetsBlock : ∀ block,
    (capsuleFor block).targetBlock = block
  correctPersistHasProjectedSourceAndRoundAdmission : ∀
    {block : ValidatorBlock BlockId} {persistTime author : Nat},
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ValidatorLocalActionOccurs (timed.execution.events persistTime) author
      (.persistProposal block) →
    CausalRecoveryCapsuleExecutionSource syncRules (capsuleFor block) author
        (persistTime + 1) ∧
      (∀ historyBlock, historyBlock ∈ (capsuleFor block).history →
        historyBlock.reference.round ≤ block.reference.round) ∧
      ∀ round,
        validatorCausalHistoryItemsAtRound round
            (capsuleFor block).history ≤ maxAdmittedRefsPerRound

/-- A current receiver cutoff and a per-round admission cap give a linear
bound on the exact unresolved causal work. GC can supply the cutoff directly.
An accepted earlier block can supply it only with explicit prefix coverage. -/
theorem ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap.unresolved_le_linear_backlog
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
    {maxAdmittedRefsPerRound : Nat}
    (source : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {block : ValidatorBlock BlockId}
    {author persistTime syncAt receiver floor : Nat}
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author (.persistProposal block))
    (cutoff : ValidatorAcceptedCausalCapsuleRoundCutoffAt timed
      (source.capsuleFor block) syncAt receiver floor) :
    CausalRecoveryCapsuleExecutionSource syncRules (source.capsuleFor block)
        author (persistTime + 1) ∧
      validatorUnresolvedHistoryItemsAt timed.execution syncAt receiver
            (source.capsuleFor block).history ≤
        (block.reference.round - floor) * maxAdmittedRefsPerRound := by
  rcases source.correctPersistHasProjectedSourceAndRoundAdmission
      authorInRange authorCorrect persisted with
    ⟨capsuleSource, upper, perRound⟩
  refine ⟨capsuleSource, ?_⟩
  exact Nat.le_trans
    (validator_unresolved_history_items_le_between_rounds cutoff upper)
    (validator_causal_history_items_between_rounds_le_linear
      (source.capsuleFor block).history maxAdmittedRefsPerRound floor
        block.reference.round perRound)

/-- Proposed exact own-prior carry rule for the one-round specialization.

The actual next block carries the correct author's prior block. In addition,
the next recoverable closure does not introduce an older sibling branch below
that prior block's round. This prefix-coverage field is necessary: the parent
edge alone does not exclude such a merge in current Rust. -/
structure ValidatorPersistedOwnPriorCausalCarrySourceMap
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
    (maxAdmittedRefsPerRound : Nat) : Type where
  admission : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
    (syncRules := syncRules) maxAdmittedRefsPerRound
  correctPersistedOwnCarryHasCoveredPrefix : ∀
    {previousBlock nextBlock : ValidatorBlock BlockId}
    {persistTime author : Nat},
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ValidatorLocalActionOccurs (timed.execution.events persistTime) author
      (.persistProposal nextBlock) →
    previousBlock.reference.author = author →
    nextBlock.reference.author = author →
    previousBlock.reference ∈ nextBlock.parents →
    previousBlock.reference.round + 1 = nextBlock.reference.round →
    ValidatorCausalCapsulePrefixCoveredBy
      (admission.capsuleFor previousBlock)
      (admission.capsuleFor nextBlock) previousBlock.reference.round

/-- If the receiver has the complete prior own-block capsule, the exact
adjacent own-carry rule reduces all unresolved target history to at most one
admitted round. -/
theorem ValidatorPersistedOwnPriorCausalCarrySourceMap.unresolved_le_one_round
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
    {maxAdmittedRefsPerRound : Nat}
    (source : ValidatorPersistedOwnPriorCausalCarrySourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {previousBlock nextBlock : ValidatorBlock BlockId}
    {persistTime author receiver syncAt : Nat}
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author
        (.persistProposal nextBlock))
    (previousAuthor : previousBlock.reference.author = author)
    (nextAuthor : nextBlock.reference.author = author)
    (previousIsParent : previousBlock.reference ∈ nextBlock.parents)
    (adjacentRound : previousBlock.reference.round + 1 =
      nextBlock.reference.round)
    (priorCutoff : ValidatorAcceptedCausalCapsuleCutoffAt timed
      (source.admission.capsuleFor previousBlock) syncAt receiver) :
    CausalRecoveryCapsuleExecutionSource syncRules
        (source.admission.capsuleFor nextBlock) author (persistTime + 1) ∧
      validatorUnresolvedHistoryItemsAt timed.execution syncAt receiver
          (source.admission.capsuleFor nextBlock).history ≤
        maxAdmittedRefsPerRound := by
  have coverage := source.correctPersistedOwnCarryHasCoveredPrefix
    authorInRange authorCorrect persisted previousAuthor nextAuthor
      previousIsParent adjacentRound
  have nextCutoff :=
    validator_causal_capsule_prefix_coverage_gives_round_cutoff coverage
      priorCutoff
  rcases source.admission.unresolved_le_linear_backlog authorInRange
      authorCorrect persisted nextCutoff with
    ⟨capsuleSource, unresolvedBound⟩
  refine ⟨capsuleSource, ?_⟩
  have oneRound : nextBlock.reference.round -
      previousBlock.reference.round = 1 := by
    omega
  simpa [oneRound] using unresolvedBound

/-- The accepted-causal-closure invariant derives the prior cutoff used by the
constant incremental backlog theorem. Thus, the caller supplies the actual
accepted prior block, not a separate cutoff assumption. -/
theorem ValidatorPersistedOwnPriorCausalCarrySourceMap.unresolved_le_one_round_from_accepted_prior
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
    {maxAdmittedRefsPerRound : Nat}
    (source : ValidatorPersistedOwnPriorCausalCarrySourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {previousBlock nextBlock : ValidatorBlock BlockId}
    {persistTime author receiver previousHolder previousSourceTime syncAt : Nat}
    (previousSource : CausalRecoveryCapsuleExecutionSource syncRules
      (source.admission.capsuleFor previousBlock) previousHolder
        previousSourceTime)
    (previousSourceBeforeSync : previousSourceTime ≤ syncAt)
    (closure : ValidatorAcceptedCausalClosureAboveGcAt config
      (timed.execution.trace syncAt) receiver)
    (previousAccepted :
      ((timed.execution.trace syncAt).validatorState receiver).accepted
        previousBlock.reference = true)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author
        (.persistProposal nextBlock))
    (previousAuthor : previousBlock.reference.author = author)
    (nextAuthor : nextBlock.reference.author = author)
    (previousIsParent : previousBlock.reference ∈ nextBlock.parents)
    (adjacentRound : previousBlock.reference.round + 1 =
      nextBlock.reference.round) :
    CausalRecoveryCapsuleExecutionSource syncRules
        (source.admission.capsuleFor nextBlock) author (persistTime + 1) ∧
      validatorUnresolvedHistoryItemsAt timed.execution syncAt receiver
          (source.admission.capsuleFor nextBlock).history ≤
        maxAdmittedRefsPerRound := by
  have previousTargetAccepted :
      ((timed.execution.trace syncAt).validatorState receiver).accepted
          (source.admission.capsuleFor previousBlock).targetBlock.reference =
        true := by
    rw [source.admission.capsuleTargetsBlock]
    exact previousAccepted
  have priorCutoff := accepted_capsule_target_gives_receiver_cutoff
    previousSource previousSourceBeforeSync closure previousTargetAccepted
  exact source.unresolved_le_one_round authorInRange authorCorrect persisted
    previousAuthor nextAuthor previousIsParent adjacentRound priorCutoff

/-- The own-prior specialization gives a constant one-round synchronization
bound from an already accepted exact prior capsule. -/
theorem ValidatorPersistedOwnPriorCausalCarrySourceMap.history_ready_within_one_round
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
    {maxAdmittedRefsPerRound : Nat}
    (source : ValidatorPersistedOwnPriorCausalCarrySourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {previousBlock nextBlock : ValidatorBlock BlockId}
    {persistTime author receiver syncAt : Nat}
    (syncSource : ValidatorBlockParentSyncSource syncRules nextBlock receiver
      author (source.admission.capsuleFor nextBlock).history syncAt)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author
        (.persistProposal nextBlock))
    (previousAuthor : previousBlock.reference.author = author)
    (nextAuthor : nextBlock.reference.author = author)
    (previousIsParent : previousBlock.reference ∈ nextBlock.parents)
    (adjacentRound : previousBlock.reference.round + 1 =
      nextBlock.reference.round)
    (priorCutoff : ValidatorAcceptedCausalCapsuleCutoffAt timed
      (source.admission.capsuleFor previousBlock) syncAt receiver)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ syncAt)
    (active : ∀ time, syncAt ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ finish,
      syncAt ≤ finish ∧
        finish ≤ syncAt + maxAdmittedRefsPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules ∧
        ∀ historyBlock,
          historyBlock ∈ (source.admission.capsuleFor nextBlock).history →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution finish receiver
            historyBlock.reference := by
  have parentFirst : ParentFirstValidatorBlockHistory
      (ValidatorReferenceAcceptedOrGcRootAt timed.execution syncAt receiver)
      (source.admission.capsuleFor nextBlock).history :=
    parent_first_validator_block_history_mono (fun _ accepted => Or.inl accepted)
      syncSource.parentFirst
  rcases retained_parent_first_history_ready_within_unresolved_bound syncRules
      syncSource.history receiverInRange receiverCorrectAvailable afterGst active
        syncSource.protectedWhileIncomplete parentFirst with
    ⟨finish, syncBeforeFinish, finishBound, ready⟩
  rcases source.unresolved_le_one_round authorInRange authorCorrect persisted
      previousAuthor nextAuthor previousIsParent adjacentRound priorCutoff with
    ⟨_capsuleSource, unresolvedBound⟩
  refine ⟨finish, syncBeforeFinish, Nat.le_trans finishBound ?_, ready⟩
  exact Nat.add_le_add_left
    (Nat.mul_le_mul_right
      (validatorBlockSyncAcceptanceBound timed syncRules) unresolvedBound)
    syncAt

/-- The current accepted prior block and its causal-closure invariant derive
the complete prior cutoff before the constant-cost synchronization proof. -/
theorem ValidatorPersistedOwnPriorCausalCarrySourceMap.history_ready_within_one_round_from_accepted_prior
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
    {maxAdmittedRefsPerRound : Nat}
    (source : ValidatorPersistedOwnPriorCausalCarrySourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {previousBlock nextBlock : ValidatorBlock BlockId}
    {persistTime author receiver previousHolder previousSourceTime syncAt : Nat}
    (previousSource : CausalRecoveryCapsuleExecutionSource syncRules
      (source.admission.capsuleFor previousBlock) previousHolder
        previousSourceTime)
    (previousSourceBeforeSync : previousSourceTime ≤ syncAt)
    (closure : ValidatorAcceptedCausalClosureAboveGcAt config
      (timed.execution.trace syncAt) receiver)
    (previousAccepted :
      ((timed.execution.trace syncAt).validatorState receiver).accepted
        previousBlock.reference = true)
    (syncSource : ValidatorBlockParentSyncSource syncRules nextBlock receiver
      author (source.admission.capsuleFor nextBlock).history syncAt)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author
        (.persistProposal nextBlock))
    (previousAuthor : previousBlock.reference.author = author)
    (nextAuthor : nextBlock.reference.author = author)
    (previousIsParent : previousBlock.reference ∈ nextBlock.parents)
    (adjacentRound : previousBlock.reference.round + 1 =
      nextBlock.reference.round)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ syncAt)
    (active : ∀ time, syncAt ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ finish,
      syncAt ≤ finish ∧
        finish ≤ syncAt + maxAdmittedRefsPerRound *
          validatorBlockSyncAcceptanceBound timed syncRules ∧
        ∀ historyBlock,
          historyBlock ∈ (source.admission.capsuleFor nextBlock).history →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution finish receiver
            historyBlock.reference := by
  have previousTargetAccepted :
      ((timed.execution.trace syncAt).validatorState receiver).accepted
          (source.admission.capsuleFor previousBlock).targetBlock.reference =
        true := by
    rw [source.admission.capsuleTargetsBlock]
    exact previousAccepted
  have priorCutoff := accepted_capsule_target_gives_receiver_cutoff
    previousSource previousSourceBeforeSync closure previousTargetAccepted
  exact source.history_ready_within_one_round syncSource authorInRange
    authorCorrect persisted previousAuthor nextAuthor previousIsParent
      adjacentRound priorCutoff receiverInRange receiverCorrectAvailable
        afterGst active

/-- The linear unresolved count gives a linear block-sync completion bound for
one current protected parent-sync source. -/
theorem ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap.history_ready_within_linear_backlog
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
    {maxAdmittedRefsPerRound : Nat}
    (source : ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) maxAdmittedRefsPerRound)
    {block : ValidatorBlock BlockId}
    {author receiver persistTime syncAt floor : Nat}
    (syncSource : ValidatorBlockParentSyncSource syncRules block receiver author
      (source.capsuleFor block).history syncAt)
    (authorInRange : author < config.authorityCount)
    (authorCorrect : faults.correctAvailable author = true)
    (persisted : ValidatorLocalActionOccurs
      (timed.execution.events persistTime) author (.persistProposal block))
    (cutoff : ValidatorAcceptedCausalCapsuleRoundCutoffAt timed
      (source.capsuleFor block) syncAt receiver floor)
    (receiverInRange : receiver < config.authorityCount)
    (receiverCorrectAvailable : faults.correctAvailable receiver = true)
    (afterGst : network.gst ≤ syncAt)
    (active : ∀ time, syncAt ≤ time →
      (timed.execution.trace time).epochActive = true) :
    ∃ finish,
      syncAt ≤ finish ∧
        finish ≤ syncAt +
          ((block.reference.round - floor) * maxAdmittedRefsPerRound) *
            validatorBlockSyncAcceptanceBound timed syncRules ∧
        ∀ historyBlock, historyBlock ∈ (source.capsuleFor block).history →
          ValidatorReferenceAcceptedOrGcRootAt timed.execution finish receiver
            historyBlock.reference := by
  have parentFirst : ParentFirstValidatorBlockHistory
      (ValidatorReferenceAcceptedOrGcRootAt timed.execution syncAt receiver)
      (source.capsuleFor block).history :=
    parent_first_validator_block_history_mono (fun _ accepted => Or.inl accepted)
      syncSource.parentFirst
  rcases retained_parent_first_history_ready_within_unresolved_bound syncRules
      syncSource.history receiverInRange receiverCorrectAvailable afterGst active
        syncSource.protectedWhileIncomplete parentFirst with
    ⟨finish, syncBeforeFinish, finishBound, ready⟩
  rcases source.unresolved_le_linear_backlog authorInRange authorCorrect
      persisted cutoff with ⟨_capsuleSource, unresolvedBound⟩
  refine ⟨finish, syncBeforeFinish, Nat.le_trans finishBound ?_, ready⟩
  exact Nat.add_le_add_left
    (Nat.mul_le_mul_right
      (validatorBlockSyncAcceptanceBound timed syncRules) unresolvedBound)
    syncAt

end Mysticeti
