/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorBlockSyncBridge
import Mysticeti.ValidatorBlockProduction
import Mysticeti.ValidatorProposalObligation

namespace Mysticeti

/-! Exact-next parent lists derived from one finite causal history.

This file does not assume a quorum block layer. It starts with one valid target
block and its complete parent-first causal history. The proof follows one parent
branch down to the requested child round. That child supplies its complete
quorum parent list for the next local proposal.
-/

/-- The maximum durable signer floor among validator indices below `count`. -/
def validatorSignerFloorMaximumUpTo
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId) : Nat → Nat
  | 0 => 0
  | count + 1 =>
      max (validatorSignerFloorMaximumUpTo world count)
        (world.validatorState count).highestSignedRound

/-- Each in-range validator floor is at most the finite ghost maximum. -/
theorem validator_signer_floor_le_maximum
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator count : Nat}
    (validatorInRange : validator < count) :
    (world.validatorState validator).highestSignedRound ≤
      validatorSignerFloorMaximumUpTo world count := by
  induction count generalizing validator with
  | zero => omega
  | succ previous inductionHypothesis =>
      simp only [validatorSignerFloorMaximumUpTo]
      by_cases validatorIsLast : validator = previous
      · subst validator
        exact Nat.le_max_right _ _
      · exact Nat.le_trans (inductionHypothesis (by omega))
          (Nat.le_max_left _ _)

/-- The round after the ghost maximum is above every current validator floor.
-/
theorem validator_floor_lt_round_after_maximum
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator count : Nat}
    (validatorInRange : validator < count) :
    (world.validatorState validator).highestSignedRound <
      validatorSignerFloorMaximumUpTo world count + 1 := by
  exact Nat.lt_succ_of_le
    (validator_signer_floor_le_maximum validatorInRange)

/-- The ghost maximum restricted to correct, available validators. -/
def correctValidatorSignerFloorMaximumUpTo
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId) : Nat → Nat
  | 0 => 0
  | count + 1 =>
      max (correctValidatorSignerFloorMaximumUpTo faults world count)
        (if faults.correctAvailable count then
          (world.validatorState count).highestSignedRound
        else 0)

/-- Each correct, available validator floor is at most the correct-only ghost
maximum. -/
theorem correct_validator_signer_floor_le_maximum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator count : Nat}
    (validatorInRange : validator < count)
    (validatorCorrect : faults.correctAvailable validator = true) :
    (world.validatorState validator).highestSignedRound ≤
      correctValidatorSignerFloorMaximumUpTo faults world count := by
  induction count generalizing validator with
  | zero => omega
  | succ previous inductionHypothesis =>
      simp only [correctValidatorSignerFloorMaximumUpTo]
      by_cases validatorIsLast : validator = previous
      · subst validator
        simpa [validatorCorrect] using
          (Nat.le_max_right
            (correctValidatorSignerFloorMaximumUpTo faults world previous)
            (world.validatorState previous).highestSignedRound)
      · exact Nat.le_trans
          (inductionHypothesis (by omega) validatorCorrect)
          (Nat.le_max_left _ _)

/-- A positive correct-only maximum is the durable signer floor of one correct,
available validator in the finite prefix. -/
theorem positive_correct_signer_floor_maximum_has_owner
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {count : Nat}
    (positive : 0 < correctValidatorSignerFloorMaximumUpTo faults world count) :
    ∃ validator,
      validator < count ∧
      faults.correctAvailable validator = true ∧
      (world.validatorState validator).highestSignedRound =
        correctValidatorSignerFloorMaximumUpTo faults world count := by
  induction count with
  | zero =>
      simp [correctValidatorSignerFloorMaximumUpTo] at positive
  | succ previous inductionHypothesis =>
      simp only [correctValidatorSignerFloorMaximumUpTo] at positive ⊢
      let earlier := correctValidatorSignerFloorMaximumUpTo faults world previous
      let last := if faults.correctAvailable previous then
        (world.validatorState previous).highestSignedRound else 0
      by_cases lastAtMostEarlier : last ≤ earlier
      · have maximumIsEarlier : max earlier last = earlier :=
          Nat.max_eq_left lastAtMostEarlier
        have earlierPositive : 0 < earlier := by
          rw [maximumIsEarlier] at positive
          exact positive
        rcases inductionHypothesis earlierPositive with
          ⟨validator, validatorInRange, validatorCorrect, validatorMaximum⟩
        exact ⟨validator, by omega, validatorCorrect, by
          rw [maximumIsEarlier]
          exact validatorMaximum⟩
      · have earlierAtMostLast : earlier ≤ last :=
          Nat.le_of_lt (Nat.lt_of_not_ge lastAtMostEarlier)
        have maximumIsLast : max earlier last = last :=
          Nat.max_eq_right earlierAtMostLast
        have lastPositive : 0 < last := by
          rw [maximumIsLast] at positive
          exact positive
        have previousCorrect : faults.correctAvailable previous = true := by
          unfold last at lastPositive
          split at lastPositive
          · assumption
          · omega
        refine ⟨previous, by omega, previousCorrect, ?_⟩
        rw [maximumIsLast]
        simp [last, previousCorrect]

/-- The round after the correct-only maximum is above each current correct,
available signer floor. -/
theorem correct_validator_floor_lt_round_after_maximum
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {validator count : Nat}
    (validatorInRange : validator < count)
    (validatorCorrect : faults.correctAvailable validator = true) :
    (world.validatorState validator).highestSignedRound <
      correctValidatorSignerFloorMaximumUpTo faults world count + 1 := by
  exact Nat.lt_succ_of_le
    (correct_validator_signer_floor_le_maximum validatorInRange
      validatorCorrect)

/-- A valid positive-round block has at least one parent. -/
private theorem valid_positive_block_has_parent
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    {block : ValidatorBlock BlockId}
    (_positive : 0 < block.reference.round)
    (valid : block.HasQuorumImmediateParents config) :
    block.parents ≠ [] := by
  intro noParents
  have noAuthors : block.parentAuthors = VoterSet.empty := by
    funext validator
    simp [ValidatorBlock.parentAuthors, noParents, VoterSet.empty]
  have quorumPositive := config.thresholds.quorum_positive
  have quorumParents := valid.2.2
  rw [noAuthors, weight_empty] at quorumParents
  omega

/-- A complete causal history contains one valid block at each positive round
up to its target round. The proof follows one parent at a time. -/
theorem causal_history_has_valid_block_at_positive_round
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {round : Nat}
    (positive : 0 < round)
    (atMostTarget : round ≤ capsule.targetRound) :
    ∃ block,
      block ∈ capsule.history ∧
      block.reference.round = round ∧
      block.HasQuorumImmediateParents config := by
  let distance := capsule.targetRound - round
  have targetEquation : round + distance = capsule.targetRound := by
    dsimp [distance]
    omega
  have descend : ∀ remaining current,
      current + remaining = capsule.targetRound →
      0 < current →
      ∃ block,
        block ∈ capsule.history ∧
        block.reference.round = current ∧
        block.HasQuorumImmediateParents config := by
    intro remaining
    induction remaining with
    | zero =>
        intro current currentAtTarget _currentPositive
        have sameRound : current = capsule.targetRound := by omega
        exact ⟨capsule.targetBlock,
          capsule.target_and_parents_in_history.1,
          by simp [CausalRecoveryCapsule.targetRound, sameRound],
          capsule.targetValid⟩
    | succ smaller inductionHypothesis =>
        intro current currentBelowTarget currentPositive
        have nextEquation : current + 1 + smaller = capsule.targetRound := by
          omega
        rcases inductionHypothesis (current + 1) nextEquation (by omega) with
          ⟨child, childMember, childRound, childValid⟩
        have childHasParent := valid_positive_block_has_parent (by omega)
          childValid
        obtain ⟨parentReference, parentMember⟩ :=
          List.exists_mem_of_ne_nil child.parents childHasParent
        have parentRound := childValid.2.1 parentReference parentMember
        have parentPositive : 0 < parentReference.round := by omega
        rcases capsule.historyClosed child childMember parentReference
            parentMember with parentGenesis | ⟨parent, parentInHistory,
              parentReferenceExact⟩
        · have parentRoundZero :=
            capsule.genesisParentsAreRoundZero parentReference parentGenesis
          omega
        · have parentRoundExact : parent.reference.round = current := by
            rw [parentReferenceExact]
            omega
          exact ⟨parent, parentInHistory, parentRoundExact,
            capsule.positiveHistoryBlocksValid parent parentInHistory (by
              omega)⟩
  exact descend distance round targetEquation positive

/-- One causal child supplies a complete quorum parent list for any earlier
exact-next target. No layer-existence premise is used. -/
theorem causal_history_supplies_exact_next_parent_list
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {floor : Nat}
    (floorBelowTarget : floor < capsule.targetRound) :
    ∃ child,
      child ∈ capsule.history ∧
      child.reference.round = floor + 1 ∧
      child.HasQuorumImmediateParents config ∧
      ∀ parent, parent ∈ child.parents →
        parent.round = floor ∧
        (parent.round = 0 ∨
          ∃ parentBlock,
            parentBlock ∈ capsule.history ∧
            parentBlock.reference = parent) := by
  have childRoundPositive : 0 < floor + 1 := by omega
  have childRoundAtMostTarget : floor + 1 ≤ capsule.targetRound := by omega
  rcases causal_history_has_valid_block_at_positive_round capsule
      childRoundPositive childRoundAtMostTarget with
    ⟨child, childMember, childRound, childValid⟩
  refine ⟨child, childMember, childRound, childValid, ?_⟩
  intro parent parentMember
  have parentRound := childValid.2.1 parent parentMember
  refine ⟨by omega, ?_⟩
  rcases capsule.historyClosed child childMember parent parentMember with
    genesis | ⟨parentBlock, parentBlockMember, parentReference⟩
  · exact Or.inl (capsule.genesisParentsAreRoundZero parent genesis)
  · exact Or.inr ⟨parentBlock, parentBlockMember, parentReference⟩

/-- After one validator has accepted and retained the finite causal history,
that local state has a recovery parent list for its exact next round.

Genesis references are permanent local data. Each positive parent must remain
strictly above the local GC round.
-/
theorem accepted_causal_history_supplies_recovery_exact_next_parents
    {BlockId CommitId : Type}
    {config : ValidatorEpochConfig CommitId}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (state : ValidatorLocalState BlockId CommitId)
    {floor : Nat}
    (floorBelowTarget : floor < capsule.targetRound)
    (historyAccepted : ∀ block, block ∈ capsule.history →
      state.accepted block.reference = true)
    (historyRetained : ∀ block, block ∈ capsule.history →
      state.retained block.reference = true)
    (genesisUsable : ∀ reference : ValidatorBlockRef BlockId,
      reference.round = 0 →
      state.accepted reference = true ∧ state.retained reference = true)
    (gcUsable : floor = 0 ∨ state.gcRound < floor) :
    ∃ parents,
      ValidatorProposalParentListReady .commitProgressRecovery config state
        (floor + 1) parents := by
  rcases causal_history_supplies_exact_next_parent_list capsule floorBelowTarget
      with ⟨child, _childMember, childRound, childValid, parentSources⟩
  refine ⟨child.parents, ?_, ?_⟩
  · refine ⟨childValid.1, ?_, childValid.2.2⟩
    intro parent parentMember
    have source := parentSources parent parentMember
    refine ⟨?_, ?_⟩
    · have immediate := childValid.2.1 parent parentMember
      omega
    · rcases source.2 with parentGenesis | ⟨parentBlock,
          parentBlockMember, parentReference⟩
      · exact (genesisUsable parent parentGenesis).1
      · rw [← parentReference]
        exact historyAccepted parentBlock parentBlockMember
  · intro parent parentMember
    have source := parentSources parent parentMember
    constructor
    · rcases source.2 with parentGenesis | ⟨parentBlock,
          parentBlockMember, parentReference⟩
      · exact (genesisUsable parent parentGenesis).2
      · rw [← parentReference]
        exact historyRetained parentBlock parentBlockMember
    · have parentRound : parent.round = floor := by
        have immediate := childValid.2.1 parent parentMember
        omega
      rcases gcUsable with floorZero | gcBelowFloor
      · exact Or.inl (by omega)
      · exact Or.inr (by omega)

/-- A current positive signer tip has one locally retained causal source.

This is the restart boundary. The source is the tip owner's local durable
storage, not an assumed remote useful peer. Its pin ends after each correct
requester accepts the finite history.
-/
structure ValidatorDurableRestartCausalSourceRules
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (syncRules : ValidatorBlockSyncExecutionRules timed)
    (snapshot : Time) where
  currentPositiveTipHasSource : ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    0 < ((timed.execution.trace snapshot).validatorState author
      ).highestSignedRound →
    ∃ block capsule,
      ((timed.execution.trace snapshot).validatorState author).ownBlockAt
          ((timed.execution.trace snapshot).validatorState author
            ).highestSignedRound = some block.reference ∧
      capsule.targetBlock = block ∧
      CausalRecoveryCapsuleExecutionSource syncRules capsule author snapshot ∧
      (∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ∀ current, snapshot ≤ current →
          (¬∀ historyBlock, historyBlock ∈ capsule.history →
            ((timed.execution.trace current).validatorState requester).accepted
              historyBlock.reference = true) →
          ∀ historyBlock, historyBlock ∈ capsule.history →
            syncRules.sourceProtected author historyBlock.reference current) ∧
      (∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ∀ historyBlock, historyBlock ∈ capsule.history →
          ∀ current, snapshot ≤ current →
            ((timed.execution.trace current).validatorState requester).accepted
                historyBlock.reference = false →
            ¬syncRules.goalObsolete requester historyBlock.reference current)

/-- A positive correct-only ghost maximum has a concrete local restart source
at its correct owner. -/
theorem positive_correct_maximum_has_durable_restart_source
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
    {snapshot : Time}
    (restart : ValidatorDurableRestartCausalSourceRules syncRules snapshot)
    (positive : 0 < correctValidatorSignerFloorMaximumUpTo faults
      (timed.execution.trace snapshot) config.authorityCount) :
    ∃ owner block capsule,
      owner < config.authorityCount ∧
      faults.correctAvailable owner = true ∧
      ((timed.execution.trace snapshot).validatorState owner).highestSignedRound =
        correctValidatorSignerFloorMaximumUpTo faults
          (timed.execution.trace snapshot) config.authorityCount ∧
      ((timed.execution.trace snapshot).validatorState owner).ownBlockAt
          ((timed.execution.trace snapshot).validatorState owner
            ).highestSignedRound = some block.reference ∧
      capsule.targetBlock = block ∧
      CausalRecoveryCapsuleExecutionSource syncRules capsule owner snapshot ∧
      (∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ∀ current, snapshot ≤ current →
          (¬∀ historyBlock, historyBlock ∈ capsule.history →
            ((timed.execution.trace current).validatorState requester).accepted
              historyBlock.reference = true) →
          ∀ historyBlock, historyBlock ∈ capsule.history →
            syncRules.sourceProtected owner historyBlock.reference current) ∧
      (∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ∀ historyBlock, historyBlock ∈ capsule.history →
          ∀ current, snapshot ≤ current →
            ((timed.execution.trace current).validatorState requester).accepted
                historyBlock.reference = false →
            ¬syncRules.goalObsolete requester historyBlock.reference current) := by
  rcases positive_correct_signer_floor_maximum_has_owner positive with
    ⟨owner, ownerInRange, ownerCorrect, ownerMaximum⟩
  have ownerPositive : 0 <
      ((timed.execution.trace snapshot).validatorState owner).highestSignedRound := by
    rw [ownerMaximum]
    exact positive
  rcases restart.currentPositiveTipHasSource owner ownerInRange ownerCorrect
      ownerPositive with
    ⟨block, capsule, own, targetBlock, source, protection, required⟩
  exact ⟨owner, block, capsule, ownerInRange, ownerCorrect, ownerMaximum,
    own, targetBlock, source, protection, required⟩

/-- Local storage and request rules for causal histories of produced blocks.

The source host retains each finite history while a correct requester still
needs an item. The requester keeps that item as a live block-sync goal. These
are one-host storage and retry rules. They do not state that delivery or
acceptance has already completed.
-/
structure ValidatorProducedCausalHistoryRules
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
  /-- Persistence atomically opens one finite recovery-history pin. The pin is
  not required before persistence or after every requester accepts it. -/
  sourceForPersistedBlock : ∀ time author block,
    ValidatorLocalActionOccurs (timed.execution.events time) author
      (.persistProposal block) →
    ∃ capsule : CausalRecoveryCapsule (BlockId := BlockId) config,
      capsule.targetBlock = block ∧
      CausalRecoveryCapsuleExecutionSource syncRules capsule author (time + 1) ∧
      (∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ∀ current, time + 1 ≤ current →
          (¬∀ block, block ∈ capsule.history →
            ((timed.execution.trace current).validatorState requester).accepted
              block.reference = true) →
          ∀ block, block ∈ capsule.history →
            syncRules.sourceProtected author block.reference current) ∧
      (∀ requester,
        requester < config.authorityCount →
        faults.correctAvailable requester = true →
        ∀ block, block ∈ capsule.history →
          ∀ current, time + 1 ≤ current →
            ((timed.execution.trace current).validatorState requester).accepted
                block.reference = false →
            ¬syncRules.goalObsolete requester block.reference current)
  genesisAccepted : ∀ time observer reference,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    reference.round = 0 →
    ((timed.execution.trace time).validatorState observer).accepted reference =
      true
  acceptedRecordsRepresentative : ∀ time observer reference,
    observer < config.authorityCount →
    faults.correctAvailable observer = true →
    reference.author < config.authorityCount →
    faults.correctAvailable reference.author = true →
    ((timed.execution.trace time).validatorState observer).accepted reference =
      true →
    ((timed.execution.trace time).validatorState observer).acceptedRepresentative
      reference.round reference.author = some reference

/-- One produced correct block is eventually accepted and recorded by one
correct observer. Delivery and acceptance follow from the main execution.
-/
theorem produced_block_eventually_accepted_by_correct_observer
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
    (rules : ValidatorProducedCausalHistoryRules syncRules)
    {persistTime author observer : Nat}
    {block : ValidatorBlock BlockId}
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (afterGst : network.gst ≤ persistTime + 1)
    (active : ∀ time, persistTime + 1 ≤ time →
      (timed.execution.trace time).epochActive = true)
    (persisted : ValidatorLocalActionOccurs (timed.execution.events persistTime)
      author (.persistProposal block)) :
    ∃ finish,
      persistTime + 1 ≤ finish ∧
      ((timed.execution.trace finish).validatorState observer).acceptedRepresentative
        block.reference.round author = some block.reference := by
  have authorFacts := validator_local_action_occurrence_is_correct_available
    (timed.execution.stepsFollowRules persistTime) persisted
  rcases rules.sourceForPersistedBlock persistTime author block persisted with
    ⟨capsule, targetBlock, source, sourceProtection, required⟩
  have retainedSource :=
    causal_recovery_capsule_to_retained_validator_history source
  have parentFirst := capsule.parent_first_validator_history (by
    intro genesisReference genesisRound
    exact rules.genesisAccepted (persistTime + 1) observer genesisReference
      observerInRange observerCorrect genesisRound)
  rcases retained_parent_first_history_eventually_accepted syncRules
      retainedSource observerInRange observerCorrect afterGst active
      (sourceProtection observer observerInRange observerCorrect)
      (required observer observerInRange observerCorrect) parentFirst with
    ⟨finish, startBeforeFinish, acceptedHistory⟩
  have targetAccepted := acceptedHistory capsule.targetBlock
    capsule.target_and_parents_in_history.1
  have recorded := rules.acceptedRecordsRepresentative finish observer
    capsule.targetBlock.reference observerInRange observerCorrect (by
      rw [targetBlock]
      have stored := persist_proposal_occurrence_stores_own_block
        timed.execution persisted
      exact ((timed.execution.statesWellFormed (persistTime + 1) author
        authorFacts.1).ownBlockIsSound block.reference.round block.reference
          stored).1 ▸ authorFacts.1) (by
      rw [targetBlock]
      have stored := persist_proposal_occurrence_stores_own_block
        timed.execution persisted
      exact ((timed.execution.statesWellFormed (persistTime + 1) author
        authorFacts.1).ownBlockIsSound block.reference.round block.reference
          stored).1 ▸ authorFacts.2) targetAccepted
  refine ⟨finish, startBeforeFinish, ?_⟩
  have blockAuthor : block.reference.author = author := by
    have stored := persist_proposal_occurrence_stores_own_block
      timed.execution persisted
    exact (timed.execution.statesWellFormed (persistTime + 1) author
      authorFacts.1).ownBlockIsSound block.reference.round block.reference
        stored |>.1
  simpa [targetBlock, blockAuthor] using recorded

/-- Accepted representatives persist in the main validator trace. -/
theorem accepted_representative_persists_in_validator_execution
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {observer round author earlier later : Nat}
    {reference : ValidatorBlockRef BlockId}
    (observerInRange : observer < config.authorityCount)
    (ordered : earlier ≤ later)
    (accepted : ((execution.trace earlier).validatorState observer
      ).acceptedRepresentative round author = some reference) :
    ((execution.trace later).validatorState observer).acceptedRepresentative
      round author = some reference := by
  exact (execution.durableStateMonotone observer earlier later observerInRange
    ordered).accepted_representative_persists accepted

/-- Concrete persistence evidence for every correct validator in one round. -/
def EveryCorrectAvailableValidatorPersistedRound
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (start round : Nat) : Prop :=
  ∀ author,
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ∃ persistTime block,
      start ≤ persistTime ∧
      block.reference.author = author ∧
      block.reference.round = round ∧
      ValidatorLocalActionOccurs (execution.events persistTime) author
        (.persistProposal block)

/-- If every correct validator has a concrete persistence event in one round,
finite main-trace block sync eventually makes that round common at all correct
validators.
-/
theorem persisted_correct_round_eventually_becomes_common
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
    (rules : ValidatorProducedCausalHistoryRules syncRules)
    {start round : Nat}
    (afterGst : network.gst ≤ start)
    (active : ∀ time, start ≤ time →
      (timed.execution.trace time).epochActive = true)
    (persisted : EveryCorrectAvailableValidatorPersistedRound timed.execution
      start round) :
    ∃ finish,
      start ≤ finish ∧
      EveryCorrectAvailableValidatorAccepted faults
        (timed.execution.trace finish) round := by
  let authorDone := fun author time =>
    faults.correctAvailable author = true →
      ∀ observer,
        observer < config.authorityCount →
        faults.correctAvailable observer = true →
        (((timed.execution.trace time).validatorState observer
          ).acceptedRepresentative round author).isSome = true
  have authorDonePersists : ∀ author earlier later,
      earlier ≤ later → authorDone author earlier →
      authorDone author later := by
    intro author earlier later ordered done authorCorrect observer
      observerInRange observerCorrect
    have earlierSome := done authorCorrect observer observerInRange
      observerCorrect
    cases earlierValue : ((timed.execution.trace earlier).validatorState
        observer).acceptedRepresentative round author with
    | none => simp [earlierValue] at earlierSome
    | some reference =>
        have laterValue :=
          accepted_representative_persists_in_validator_execution
            timed.execution observerInRange ordered earlierValue
        simp [laterValue]
  have eachAuthor : ∀ author,
      author < config.authorityCount →
      faults.correctAvailable author = true →
      ∃ finish, start ≤ finish ∧ authorDone author finish := by
    intro author authorInRange authorCorrect
    rcases persisted author authorInRange authorCorrect with
      ⟨persistTime, block, startBeforePersist, blockAuthor, blockRound,
        persistOccurs⟩
    have afterGstAtSource : network.gst ≤ persistTime + 1 :=
      Nat.le_trans afterGst
        (Nat.le_trans startBeforePersist (Nat.le_succ _))
    have activeAtSource : ∀ time, persistTime + 1 ≤ time →
        (timed.execution.trace time).epochActive = true := by
      intro time sourceBeforeTime
      exact active time (Nat.le_trans startBeforePersist
        (Nat.le_trans (Nat.le_succ _) sourceBeforeTime))
    let reference := block.reference
    have referenceRound : reference.round = round := by
      exact blockRound
    have referenceAuthor : reference.author = author := by
      exact blockAuthor
    let observerDone := fun observer time =>
      observer < config.authorityCount →
      faults.correctAvailable observer = true →
        ((timed.execution.trace time).validatorState observer
          ).acceptedRepresentative round author = some reference
    have observerDonePersists : ∀ observer earlier later,
        earlier ≤ later → observerDone observer earlier →
        observerDone observer later := by
      intro observer earlier later ordered done observerInRange
        observerCorrect
      exact accepted_representative_persists_in_validator_execution
        timed.execution observerInRange ordered
          (done observerInRange observerCorrect)
    have eachObserver : ∀ observer,
        observer < config.authorityCount →
        faults.correctAvailable observer = true →
        ∃ finish, start ≤ finish ∧ observerDone observer finish := by
      intro observer observerInRange observerCorrect
      rcases produced_block_eventually_accepted_by_correct_observer rules
          observerInRange observerCorrect afterGstAtSource activeAtSource
          persistOccurs with
        ⟨finish, sourceBeforeFinish, accepted⟩
      have startBeforeFinish : start ≤ finish :=
        Nat.le_trans startBeforePersist
          (Nat.le_trans (Nat.le_succ _) sourceBeforeFinish)
      refine ⟨finish, startBeforeFinish, fun _ _ => ?_⟩
      simpa [reference, referenceRound, referenceAuthor] using accepted
    rcases eventually_every_selected_validator faults.correctAvailable
        observerDone start observerDonePersists eachObserver with
      ⟨finish, startBeforeFinish, allObservers⟩
    refine ⟨finish, startBeforeFinish, ?_⟩
    intro _authorCorrect observer observerInRange observerCorrect
    have accepted := allObservers observer observerInRange observerCorrect
      observerInRange observerCorrect
    simp [accepted]
  rcases eventually_every_selected_validator faults.correctAvailable authorDone
      start authorDonePersists eachAuthor with
    ⟨finish, startBeforeFinish, allAuthors⟩
  refine ⟨finish, startBeforeFinish, ?_⟩
  intro observer author observerInRange observerCorrect authorInRange
    authorCorrect
  exact allAuthors author authorInRange authorCorrect authorCorrect observer
    observerInRange observerCorrect

end Mysticeti
