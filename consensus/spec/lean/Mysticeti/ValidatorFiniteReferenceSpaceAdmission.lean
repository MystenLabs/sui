/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorReceiverRelativeCausalBacklog

namespace Mysticeti

/-! A finite reference-space adapter for causal-capsule round admission.

This file supplies the static per-round cap which follows from the finite
block-ID space. It does not add a behavioral admission rule. For a fixed
round, a reference is determined by its author and block ID. A capsule has no
duplicate references, and every persisted capsule author is in the committee.
Therefore, one capsule contains at most
`authorityCount * blockIdCount` references at one round.

The adapter is isolated from the shared end-to-end composition. The concrete
Rust instantiation can use the 256-bit digest space for `blockIdCount`.
-/

variable {BlockId CommitId PacketId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {faults : FixedFaultInterval config}
variable {protocolPacket :
  AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
variable {network : AddressedPartialSynchrony config faults protocolPacket}
variable {program : ValidatorExecutionProgram BlockId CommitId}

/-- A finite block-ID witness without a global `Fintype` dependency.
`ASM-LIVE-FINITE-REFERENCE-SPACE`. -/
structure ValidatorFiniteBlockIdEncoding
    (BlockId : Type) (blockIdCount : Nat) where
  encode : BlockId → Fin blockIdCount
  injective : Function.Injective encode

/-- Encode every field of one validator block reference. -/
def validatorFiniteReferenceKey
    {BlockId : Type} {blockIdCount : Nat}
    (encoding : ValidatorFiniteBlockIdEncoding BlockId blockIdCount)
    (reference : ValidatorBlockRef BlockId) : Nat × Nat × Fin blockIdCount :=
  (reference.round, reference.author, encoding.encode reference.id)

/-- The full finite key space for one fixed round. -/
def validatorFiniteReferenceKeysAtRound
    (round authorityCount blockIdCount : Nat) :
    List (Nat × Nat × Fin blockIdCount) :=
  (List.range authorityCount).flatMap fun author =>
    (List.finRange blockIdCount).map fun blockId =>
      (round, author, blockId)

/-- Select the references at one round. -/
def validatorReferencesAtRound
    {BlockId : Type} (round : Nat) (references : List (ValidatorBlockRef BlockId)) :
    List (ValidatorBlockRef BlockId) :=
  references.filter fun reference => reference.round == round

/-- The exact history count is the length of the selected reference list. -/
theorem validator_causal_history_items_at_round_eq_reference_filter_length
    {BlockId : Type} (round : Nat) (blocks : List (ValidatorBlock BlockId)) :
    validatorCausalHistoryItemsAtRound round blocks =
      (validatorReferencesAtRound round
        (blocks.map ValidatorBlock.reference)).length := by
  induction blocks with
  | nil =>
      simp [validatorCausalHistoryItemsAtRound, validatorReferencesAtRound]
  | cons block remaining inductionHypothesis =>
      by_cases sameRound : block.reference.round = round
      · simp [validatorCausalHistoryItemsAtRound, validatorReferencesAtRound,
          inductionHypothesis, sameRound]
      · simp [validatorCausalHistoryItemsAtRound, validatorReferencesAtRound,
          inductionHypothesis, sameRound]

/-- The finite reference encoding is injective because it retains all three
reference fields. -/
theorem validator_finite_reference_key_injective
    {BlockId : Type} {blockIdCount : Nat}
    (encoding : ValidatorFiniteBlockIdEncoding BlockId blockIdCount) :
    Function.Injective (validatorFiniteReferenceKey encoding) := by
  intro left right sameKey
  rcases left with ⟨leftId, leftAuthor, leftRound⟩
  rcases right with ⟨rightId, rightAuthor, rightRound⟩
  simp only [validatorFiniteReferenceKey, Prod.mk.injEq] at sameKey
  rcases sameKey with ⟨sameRound, sameAuthor, sameEncodedId⟩
  have sameId : leftId = rightId := encoding.injective sameEncodedId
  cases sameId
  cases sameAuthor
  cases sameRound
  rfl

/-- The full key space at one round has the committee-size times block-ID-space
size. -/
theorem validator_finite_reference_keys_at_round_length
    (round authorityCount blockIdCount : Nat) :
    (validatorFiniteReferenceKeysAtRound round authorityCount blockIdCount).length =
      authorityCount * blockIdCount := by
  simp [validatorFiniteReferenceKeysAtRound, List.length_flatMap,
    List.map_const', List.sum_replicate_nat]

/-- A history with unique references and valid authors has at most the full
finite reference space at each round. -/
theorem validator_causal_history_items_at_round_le_finite_reference_space
    {BlockId : Type} {blockIdCount authorityCount : Nat}
    (encoding : ValidatorFiniteBlockIdEncoding BlockId blockIdCount)
    (blocks : List (ValidatorBlock BlockId))
    (referencesNodup : (blocks.map ValidatorBlock.reference).Nodup)
    (authorsInRange : ∀ block, block ∈ blocks →
      block.reference.author < authorityCount)
    (round : Nat) :
    validatorCausalHistoryItemsAtRound round blocks ≤
      authorityCount * blockIdCount := by
  classical
  let roundReferences := validatorReferencesAtRound round
    (blocks.map ValidatorBlock.reference)
  let encodedReferences := roundReferences.map
    (validatorFiniteReferenceKey encoding)
  have roundReferencesNodup : roundReferences.Nodup := by
    exact referencesNodup.filter _
  have encodedReferencesNodup : encodedReferences.Nodup := by
    apply roundReferencesNodup.map (validatorFiniteReferenceKey encoding)
    intro left right different sameKey
    exact different (validator_finite_reference_key_injective encoding sameKey)
  have encodedReferencesSubset : encodedReferences ⊆
      validatorFiniteReferenceKeysAtRound round authorityCount blockIdCount := by
    intro key keyMember
    rcases List.mem_map.mp keyMember with
      ⟨reference, referenceMember, rfl⟩
    have sourceMember :
        reference ∈ blocks.map ValidatorBlock.reference :=
      (List.mem_filter.mp referenceMember).1
    have referenceRound : reference.round = round := by
      simpa using (List.mem_filter.mp referenceMember).2
    rcases List.mem_map.mp sourceMember with
      ⟨block, blockMember, sameReference⟩
    have authorInRange : reference.author < authorityCount := by
      rw [← sameReference]
      exact authorsInRange block blockMember
    apply List.mem_flatMap.mpr
    refine ⟨reference.author, List.mem_range.mpr authorInRange, ?_⟩
    apply List.mem_map.mpr
    refine ⟨encoding.encode reference.id,
      List.mem_finRange (encoding.encode reference.id), ?_⟩
    simp [validatorFiniteReferenceKey, referenceRound]
  have lengthBound := encodedReferencesNodup.length_le_of_subset
    encodedReferencesSubset
  rw [validator_finite_reference_keys_at_round_length] at lengthBound
  rw [validator_causal_history_items_at_round_eq_reference_filter_length]
  simpa [roundReferences, encodedReferences] using lengthBound

/-- Current/past persisted-capsule facts needed by the finite-space adapter.

Reference uniqueness is already a field of `CausalRecoveryCapsule`. Author
range is already a field of `CausalRecoveryCapsuleExecutionSource`. This map
keeps only the actual persisted projection and the target-round upper bound.
`ASM-LIVE-CAPSULE-PROJECTION`. -/
structure ValidatorPersistedCausalCapsuleFiniteReferenceSourceMap
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed} : Type where
  capsuleFor : ValidatorBlock BlockId →
    CausalRecoveryCapsule (BlockId := BlockId) config
  capsuleTargetsBlock : ∀ block,
    (capsuleFor block).targetBlock = block
  correctPersistHasProjectedSourceAndRoundUpper : ∀
    {block : ValidatorBlock BlockId} {persistTime author : Nat},
    author < config.authorityCount →
    faults.correctAvailable author = true →
    ValidatorLocalActionOccurs (timed.execution.events persistTime) author
      (.persistProposal block) →
    CausalRecoveryCapsuleExecutionSource syncRules (capsuleFor block) author
        (persistTime + 1) ∧
      ∀ historyBlock, historyBlock ∈ (capsuleFor block).history →
        historyBlock.reference.round ≤ block.reference.round

/-- Turn the small finite-space source map into the existing per-round
admission interface. -/
def ValidatorPersistedCausalCapsuleFiniteReferenceSourceMap.toRoundAdmission
    {blockIdCount : Nat}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {syncRules : ValidatorBlockSyncExecutionRules timed}
    (source : ValidatorPersistedCausalCapsuleFiniteReferenceSourceMap
      (syncRules := syncRules))
    (encoding : ValidatorFiniteBlockIdEncoding BlockId blockIdCount) :
    ValidatorPersistedCausalCapsuleRoundAdmissionSourceMap
      (syncRules := syncRules) (config.authorityCount * blockIdCount) where
  capsuleFor := source.capsuleFor
  capsuleTargetsBlock := source.capsuleTargetsBlock
  correctPersistHasProjectedSourceAndRoundAdmission := by
    intro block persistTime author authorInRange authorCorrect persisted
    rcases source.correctPersistHasProjectedSourceAndRoundUpper authorInRange
        authorCorrect persisted with
      ⟨capsuleSource, upper⟩
    refine ⟨capsuleSource, upper, ?_⟩
    intro round
    exact validator_causal_history_items_at_round_le_finite_reference_space
      encoding (source.capsuleFor block).history
      (source.capsuleFor block).historyReferencesNodup
      capsuleSource.authorInRange round

end Mysticeti
