/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorProcess

namespace Mysticeti

/-! Durable local data for recovery alignment.

A capsule contains block bodies that one validator retained. It contains an
accepted seed block, the exact bodies for that seed's immediate parents, and a
finite causal closure above the local GC boundary. The seed round `H` can equal
or exceed the durable signer floor `P`.

This file proves only facts about retained data and legal target rounds. It has
no message-delivery, task-scheduling, or future-action premise or conclusion.
-/

/-- Genesis remains usable after GC. Every other usable block is strictly above
the local GC boundary. -/
def RetainedBlockUsableAt {BlockId : Type}
    (gcRound : Nat) (block : ValidatorBlock BlockId) : Prop :=
  block.reference.round = 0 ∨ gcRound < block.reference.round

/-- One validator's durable restart capsule.

The accepted seed can be another validator's child above the signer floor. If
the seed is at the signer floor, it must be this validator's existing durable
own block. All lists contain block bodies, not only block references. -/
structure DurableRestartCapsule
    {BlockId CommitId : Type}
    (config : ValidatorEpochConfig CommitId)
    (holder : Nat) where
  signerFloor : Nat
  durableOwnBlock : ValidatorBlock BlockId
  durableOwnBlockAuthor : durableOwnBlock.reference.author = holder
  durableOwnBlockRound : durableOwnBlock.reference.round = signerFloor
  acceptedSeedBlock : ValidatorBlock BlockId
  seedAtOrAboveSignerFloor :
    signerFloor ≤ acceptedSeedBlock.reference.round
  seedAtFloorReusesOwnBlock :
    acceptedSeedBlock.reference.round = signerFloor →
      acceptedSeedBlock = durableOwnBlock
  immediateParentBodies : List (ValidatorBlock BlockId)
  immediateParentBodiesNonempty : immediateParentBodies ≠ []
  exactImmediateParentBodies :
    immediateParentBodies.map ValidatorBlock.reference =
      acceptedSeedBlock.parents
  validSeedParentSet :
    acceptedSeedBlock.HasQuorumImmediateParents config
  gcRound : Nat
  immediateParentsUsableAtLocalGc :
    ∀ parent, parent ∈ immediateParentBodies →
      RetainedBlockUsableAt gcRound parent
  servableCausalClosure : List (ValidatorBlock BlockId)
  closureReferencesNodup :
    (servableCausalClosure.map ValidatorBlock.reference).Nodup
  seedInClosure : acceptedSeedBlock ∈ servableCausalClosure
  immediateParentsInClosure :
    ∀ parent, parent ∈ immediateParentBodies →
      parent ∈ servableCausalClosure
  closureBodiesUsableAtLocalGc :
    ∀ block, block ∈ servableCausalClosure →
      RetainedBlockUsableAt gcRound block
  /-- The closure contains the body for each referenced parent above GC.
  Genesis and references at the GC boundary terminate this local closure. -/
  closureContainsAboveGcParents :
    ∀ block, block ∈ servableCausalClosure →
      ∀ parentRef, parentRef ∈ block.parents →
        gcRound < parentRef.round →
          ∃ parentBody, parentBody ∈ servableCausalClosure ∧
            parentBody.reference = parentRef

namespace DurableRestartCapsule

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {holder : Nat}

/-- The accepted seed round `H`. -/
def seedRound
    (capsule : DurableRestartCapsule (BlockId := BlockId) config holder) : Nat :=
  capsule.acceptedSeedBlock.reference.round

/-- The exact retained immediate-parent references of the accepted seed. -/
def immediateParentRefs
    (capsule : DurableRestartCapsule (BlockId := BlockId) config holder) :
    List (ValidatorBlockRef BlockId) :=
  capsule.immediateParentBodies.map ValidatorBlock.reference

/-- Each retained immediate-parent body matches one reference in the seed. -/
theorem immediate_parent_reference_mem
    (capsule : DurableRestartCapsule (BlockId := BlockId) config holder)
    {parent : ValidatorBlock BlockId}
    (parentMember : parent ∈ capsule.immediateParentBodies) :
    parent.reference ∈ capsule.acceptedSeedBlock.parents := by
  rw [← capsule.exactImmediateParentBodies]
  exact List.mem_map.mpr ⟨parent, parentMember, rfl⟩

/-- Every exact immediate-parent body is in the servable closure. -/
theorem serves_immediate_parent
    (capsule : DurableRestartCapsule (BlockId := BlockId) config holder)
    {parent : ValidatorBlock BlockId}
    (parentMember : parent ∈ capsule.immediateParentBodies) :
    parent ∈ capsule.servableCausalClosure :=
  capsule.immediateParentsInClosure parent parentMember

/-- The capsule can resolve each above-GC parent reference in its finite local
closure to a retained block body. -/
theorem serves_above_gc_dependency
    (capsule : DurableRestartCapsule (BlockId := BlockId) config holder)
    {block : ValidatorBlock BlockId}
    (blockMember : block ∈ capsule.servableCausalClosure)
    {parentRef : ValidatorBlockRef BlockId}
    (parentMember : parentRef ∈ block.parents)
    (aboveGc : capsule.gcRound < parentRef.round) :
    ∃ parentBody, parentBody ∈ capsule.servableCausalClosure ∧
      parentBody.reference = parentRef :=
  capsule.closureContainsAboveGcParents block blockMember
    parentRef parentMember aboveGc

/-- The seed has a positive round because it has an immediate parent body. -/
theorem seed_round_positive
    (capsule : DurableRestartCapsule (BlockId := BlockId) config holder) :
    0 < capsule.seedRound := by
  obtain ⟨parent, parentMember⟩ :=
    List.exists_mem_of_ne_nil capsule.immediateParentBodies
      capsule.immediateParentBodiesNonempty
  have referenceMember := capsule.immediate_parent_reference_mem parentMember
  have immediate :=
    capsule.validSeedParentSet.2.1 parent.reference referenceMember
  unfold seedRound
  omega

/-- The local parent round is genesis, or it is strictly above the local GC
boundary. The closure also forces a zero GC boundary for a round-one seed. -/
theorem local_gc_seed_window
    (capsule : DurableRestartCapsule (BlockId := BlockId) config holder) :
    (capsule.seedRound = 1 ∧ capsule.gcRound = 0) ∨
      capsule.gcRound + 1 < capsule.seedRound := by
  obtain ⟨parent, parentMember⟩ :=
    List.exists_mem_of_ne_nil capsule.immediateParentBodies
      capsule.immediateParentBodiesNonempty
  have referenceMember := capsule.immediate_parent_reference_mem parentMember
  have immediate :=
    capsule.validSeedParentSet.2.1 parent.reference referenceMember
  have parentUsable :=
    capsule.immediateParentsUsableAtLocalGc parent parentMember
  rcases parentUsable with parentGenesis | parentAboveGc
  · have seedRoundOne : capsule.seedRound = 1 := by
      unfold seedRound
      omega
    have seedUsable := capsule.closureBodiesUsableAtLocalGc
      capsule.acceptedSeedBlock capsule.seedInClosure
    rcases seedUsable with seedGenesis | seedAboveGc
    · unfold seedRound at seedRoundOne
      omega
    · left
      exact ⟨seedRoundOne, by
        unfold seedRound at seedRoundOne
        omega⟩
  · right
    unfold seedRound
    omega

/-- A seed at the signer floor is exactly the durable own block. -/
theorem seed_at_floor_is_existing_own_block
    (capsule : DurableRestartCapsule (BlockId := BlockId) config holder)
    (atFloor : capsule.seedRound = capsule.signerFloor) :
    capsule.acceptedSeedBlock = capsule.durableOwnBlock := by
  exact capsule.seedAtFloorReusesOwnBlock atFloor

end DurableRestartCapsule

/-- A finite fixed set of correct, available validators and their local durable
capsules. The list has no duplicate validator. -/
structure FixedCorrectAvailableCapsules
    (BlockId CommitId : Type)
    (config : ValidatorEpochConfig CommitId) where
  correctAvailable : List Nat
  correctAvailableNodup : correctAvailable.Nodup
  correctAvailableNonempty : correctAvailable ≠ []
  capsule :
    (validator : Nat) → DurableRestartCapsule (BlockId := BlockId) config validator

namespace FixedCorrectAvailableCapsules

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}

/-- The maximum natural number in a finite list. -/
def listMaximum : List Nat → Nat
  | [] => 0
  | value :: values => Nat.max value (listMaximum values)

theorem member_le_listMaximum
    {value : Nat} {values : List Nat}
    (member : value ∈ values) :
    value ≤ listMaximum values := by
  induction values with
  | nil => simp at member
  | cons head tail ih =>
      simp only [List.mem_cons] at member
      simp only [listMaximum]
      rcases member with rfl | tailMember
      · exact Nat.le_max_left _ _
      · exact Nat.le_trans (ih tailMember) (Nat.le_max_right _ _)

theorem listMaximum_mem
    {values : List Nat}
    (nonempty : values ≠ []) :
    listMaximum values ∈ values := by
  induction values with
  | nil => contradiction
  | cons head tail ih =>
      cases tail with
      | nil => simp [listMaximum]
      | cons next rest =>
          have tailNonempty : next :: rest ≠ [] := by simp
          have tailMember := ih tailNonempty
          simp only [listMaximum]
          change Nat.max head (listMaximum (next :: rest)) ∈
            head :: next :: rest
          by_cases tailLeHead : listMaximum (next :: rest) ≤ head
          · have equal : Nat.max head (listMaximum (next :: rest)) = head :=
              Nat.max_eq_left tailLeHead
            rw [equal]
            exact List.mem_cons_self
          · have headLeTail : head ≤ listMaximum (next :: rest) :=
              Nat.le_of_lt (Nat.lt_of_not_ge tailLeHead)
            have equal : Nat.max head (listMaximum (next :: rest)) =
                listMaximum (next :: rest) := Nat.max_eq_right headLeTail
            rw [equal]
            exact List.mem_cons_of_mem head tailMember

/-- The durable signer floor `P` for one correct, available validator. -/
def signerFloor
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config)
    (validator : Nat) : Nat :=
  (capsules.capsule validator).signerFloor

/-- The accepted seed round `H` for one correct, available validator. -/
def seedRound
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config)
    (validator : Nat) : Nat :=
  (capsules.capsule validator).seedRound

/-- The finite list of seed rounds in the correct, available set. -/
def seedRounds
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config) :
    List Nat :=
  capsules.correctAvailable.map capsules.seedRound

theorem seed_rounds_nonempty
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config) :
    capsules.seedRounds ≠ [] := by
  simpa [seedRounds] using capsules.correctAvailableNonempty

/-- The maximum accepted seed round in the fixed set. -/
def maximumSeedRound
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config) : Nat :=
  listMaximum capsules.seedRounds

theorem maximum_seed_round_mem
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config) :
    capsules.maximumSeedRound ∈ capsules.seedRounds := by
  exact listMaximum_mem capsules.seed_rounds_nonempty

/-- A correct, available validator holds a capsule at the maximum seed round. -/
theorem exists_maximum_seed_holder
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config) :
    ∃ holder ∈ capsules.correctAvailable,
      capsules.seedRound holder = capsules.maximumSeedRound := by
  have member := capsules.maximum_seed_round_mem
  simp only [seedRounds, List.mem_map] at member
  obtain ⟨holder, holderMember, holderRound⟩ := member
  exact ⟨holder, holderMember, holderRound⟩

/-- Each correct, available seed round is at most the maximum seed round. -/
theorem seed_round_le_maximum
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config)
    {validator : Nat}
    (validatorCorrect : validator ∈ capsules.correctAvailable) :
    capsules.seedRound validator ≤ capsules.maximumSeedRound := by
  apply member_le_listMaximum
  simp only [seedRounds, List.mem_map]
  exact ⟨validator, validatorCorrect, rfl⟩

/-- The maximum seed round is at or above every correct, available validator's
durable signer floor. -/
theorem signer_floor_le_maximum_seed
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config)
    {validator : Nat}
    (validatorCorrect : validator ∈ capsules.correctAvailable) :
    capsules.signerFloor validator ≤ capsules.maximumSeedRound := by
  exact Nat.le_trans
    (capsules.capsule validator).seedAtOrAboveSignerFloor
    (capsules.seed_round_le_maximum validatorCorrect)

/-- A retained seed supplies its exact quorum parent bodies at one validator's
GC boundary. The bodies are also present in the holder's finite closure. -/
structure SuppliesRetainedParentLayerAt
    {holder : Nat}
    (source : DurableRestartCapsule (BlockId := BlockId) config holder)
    (gcRound : Nat) : Prop where
  validParentSet : source.acceptedSeedBlock.HasQuorumImmediateParents config
  exactParentBodies :
    source.immediateParentBodies.map ValidatorBlock.reference =
      source.acceptedSeedBlock.parents
  parentBodiesUsable :
    ∀ parent, parent ∈ source.immediateParentBodies →
      RetainedBlockUsableAt gcRound parent
  parentBodiesServable :
    ∀ parent, parent ∈ source.immediateParentBodies →
      parent ∈ source.servableCausalClosure

/-- A maximum seed capsule supplies a retained quorum parent layer that is
usable above each correct, available validator's local GC boundary. -/
theorem maximum_seed_parent_layer_usable_for_all
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config)
    {holder : Nat}
    (_holderCorrect : holder ∈ capsules.correctAvailable)
    (holderMaximum :
      capsules.seedRound holder = capsules.maximumSeedRound) :
    ∀ validator ∈ capsules.correctAvailable,
      SuppliesRetainedParentLayerAt (capsules.capsule holder)
        (capsules.capsule validator).gcRound := by
  intro validator validatorCorrect
  refine {
    validParentSet := (capsules.capsule holder).validSeedParentSet
    exactParentBodies :=
      (capsules.capsule holder).exactImmediateParentBodies
    parentBodiesUsable := ?_
    parentBodiesServable :=
      (capsules.capsule holder).immediateParentsInClosure
  }
  intro parent parentMember
  have parentReferenceMember :=
    (capsules.capsule holder).immediate_parent_reference_mem parentMember
  have parentImmediate :=
    (capsules.capsule holder).validSeedParentSet.2.1
      parent.reference parentReferenceMember
  have validatorSeedLeMaximum :=
    capsules.seed_round_le_maximum validatorCorrect
  have validatorWindow :=
    (capsules.capsule validator).local_gc_seed_window
  rcases validatorWindow with genesisWindow | aboveGcWindow
  · by_cases parentGenesis : parent.reference.round = 0
    · exact Or.inl parentGenesis
    · right
      rcases genesisWindow with ⟨validatorRound, validatorGc⟩
      change (capsules.capsule validator).acceptedSeedBlock.reference.round ≤
        capsules.maximumSeedRound at validatorSeedLeMaximum
      change (capsules.capsule holder).acceptedSeedBlock.reference.round =
        capsules.maximumSeedRound at holderMaximum
      omega
  · right
    change (capsules.capsule validator).acceptedSeedBlock.reference.round ≤
      capsules.maximumSeedRound at validatorSeedLeMaximum
    change (capsules.capsule holder).acceptedSeedBlock.reference.round =
      capsules.maximumSeedRound at holderMaximum
    change (capsules.capsule validator).gcRound + 1 <
      (capsules.capsule validator).acceptedSeedBlock.reference.round at aboveGcWindow
    omega

/-- The local choice at a retained safe-resume target. Equality means that the
validator reuses its existing own block. Strict inequality permits one new
proposal at the target. -/
structure SafeResumeTargetFor
    {holder validator : Nat}
    (source : DurableRestartCapsule (BlockId := BlockId) config holder)
    (localCapsule : DurableRestartCapsule (BlockId := BlockId) config validator) :
    Prop where
  targetAtOrAboveSignerFloor :
    localCapsule.signerFloor ≤ source.seedRound
  retainedParentLayer :
    SuppliesRetainedParentLayerAt source localCapsule.gcRound
  reuseExistingOrPropose :
    (source.seedRound = localCapsule.signerFloor ∧
      localCapsule.seedRound = source.seedRound ∧
      localCapsule.acceptedSeedBlock = localCapsule.durableOwnBlock) ∨
    localCapsule.signerFloor < source.seedRound

/-- The maximum retained seed supplies a safe-resume target for every correct,
available validator. No future block or delivery fact is an input. -/
theorem maximum_seed_supplies_safe_resume_target
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config)
    {holder : Nat}
    (holderCorrect : holder ∈ capsules.correctAvailable)
    (holderMaximum :
      capsules.seedRound holder = capsules.maximumSeedRound) :
    ∀ validator ∈ capsules.correctAvailable,
      SafeResumeTargetFor (capsules.capsule holder)
        (capsules.capsule validator) := by
  intro validator validatorCorrect
  have targetAboveFloor :=
    capsules.signer_floor_le_maximum_seed validatorCorrect
  have retainedLayer :=
    capsules.maximum_seed_parent_layer_usable_for_all
      holderCorrect holderMaximum validator validatorCorrect
  have localSeedLeMaximum :=
    capsules.seed_round_le_maximum validatorCorrect
  refine {
    targetAtOrAboveSignerFloor := ?_
    retainedParentLayer := retainedLayer
    reuseExistingOrPropose := ?_
  }
  · change (capsules.capsule validator).signerFloor ≤
      (capsules.capsule holder).seedRound
    change (capsules.capsule validator).signerFloor ≤
      capsules.maximumSeedRound at targetAboveFloor
    change (capsules.capsule holder).seedRound =
      capsules.maximumSeedRound at holderMaximum
    omega
  · by_cases targetAtFloor :
        (capsules.capsule holder).seedRound =
          (capsules.capsule validator).signerFloor
    · left
      have localSeedAtFloor :
          (capsules.capsule validator).seedRound =
            (capsules.capsule validator).signerFloor := by
        change (capsules.capsule validator).seedRound ≤
          capsules.maximumSeedRound at localSeedLeMaximum
        change (capsules.capsule holder).seedRound =
          capsules.maximumSeedRound at holderMaximum
        exact Nat.le_antisymm (by omega)
          (capsules.capsule validator).seedAtOrAboveSignerFloor
      exact ⟨targetAtFloor,
        localSeedAtFloor.trans targetAtFloor.symm,
        (capsules.capsule validator).seed_at_floor_is_existing_own_block
          localSeedAtFloor⟩
    · right
      change (capsules.capsule holder).seedRound =
        capsules.maximumSeedRound at holderMaximum
      change (capsules.capsule validator).signerFloor ≤
        capsules.maximumSeedRound at targetAboveFloor
      omega

/-- A maximum holder and its safe-resume targets exist in every nonempty fixed
correct, available set. -/
theorem exists_maximum_seed_safe_resume_targets
    (capsules : FixedCorrectAvailableCapsules BlockId CommitId config) :
    ∃ holder ∈ capsules.correctAvailable,
      capsules.seedRound holder = capsules.maximumSeedRound ∧
      ∀ validator ∈ capsules.correctAvailable,
        SafeResumeTargetFor (capsules.capsule holder)
          (capsules.capsule validator) := by
  obtain ⟨holder, holderCorrect, holderMaximum⟩ :=
    capsules.exists_maximum_seed_holder
  exact ⟨holder, holderCorrect, holderMaximum,
    capsules.maximum_seed_supplies_safe_resume_target
      holderCorrect holderMaximum⟩

end FixedCorrectAvailableCapsules

end Mysticeti
