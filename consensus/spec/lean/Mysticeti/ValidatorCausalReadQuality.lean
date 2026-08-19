/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCausalExactNext

namespace Mysticeti

/-! Weighted honest-author quality in a Mysticeti causal read.

Narwhal states chain quality as a fraction of all blocks in a causal read. The
uncertified Mysticeti DAG can contain Byzantine equivocations, so an unrestricted
raw block-count fraction is not sound. The valid parent layer of each block still
uses distinct authors and quorum stake. These results count each such author once.
-/

/-- A threshold-sized selected set contains all but the bounded bad stake outside
the bad set. This subtraction-free form avoids truncated natural subtraction. -/
private theorem threshold_le_bad_bound_plus_outside_weight
    {authorityCount : Nat} {stake : Nat -> Nat}
    {selected bad : VoterSet} {threshold badBound : Nat}
    (selectedWeight : threshold <= weight authorityCount stake selected)
    (badWeight : weight authorityCount stake bad <= badBound) :
    threshold <= badBound +
      weight authorityCount stake (VoterSet.diff selected bad) := by
  have badPartAtMostBad :
      weight authorityCount stake (VoterSet.inter selected bad) <=
        weight authorityCount stake bad :=
    weight_mono stake (VoterSet.inter_subset_right authorityCount selected bad)
  have partition :=
    weight_diff_add_inter authorityCount stake selected bad
  omega

namespace ValidatorBlock

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}

/-- Every valid parent layer has quorum stake after adding the Byzantine budget
to its distinct non-Byzantine parent-author stake. -/
theorem quorum_parent_layer_has_non_byzantine_stake
    (faults : FixedFaultInterval config)
    {block : ValidatorBlock BlockId}
    (valid : block.HasQuorumImmediateParents config) :
    config.thresholds.quorum <= config.thresholds.fault +
      weight config.authorityCount config.stake
        (VoterSet.diff block.parentAuthors faults.byzantine) := by
  exact threshold_le_bad_bound_plus_outside_weight valid.2.2
    faults.byzantineStakeBounded

/-- User-facing subtraction form of the non-Byzantine parent-stake bound. -/
theorem non_byzantine_parent_stake_at_least_quorum_minus_fault
    (faults : FixedFaultInterval config)
    {block : ValidatorBlock BlockId}
    (valid : block.HasQuorumImmediateParents config) :
    config.thresholds.quorum - config.thresholds.fault <=
      weight config.authorityCount config.stake
        (VoterSet.diff block.parentAuthors faults.byzantine) := by
  have quality := quorum_parent_layer_has_non_byzantine_stake faults valid
  omega

/-- The same parent layer has quorum stake after adding both fault budgets to
its distinct correct-and-available parent-author stake. -/
theorem quorum_parent_layer_has_correct_available_stake
    (faults : FixedFaultInterval config)
    {block : ValidatorBlock BlockId}
    (valid : block.HasQuorumImmediateParents config) :
    config.thresholds.quorum <=
      config.thresholds.fault + faults.unavailableStakeBound +
        weight config.authorityCount config.stake
          (VoterSet.diff block.parentAuthors faults.nonProgress) := by
  exact threshold_le_bad_bound_plus_outside_weight valid.2.2
    faults.non_progress_stake_bounded

/-- User-facing subtraction form of the correct-and-available parent-stake
bound. -/
theorem correct_available_parent_stake_at_least_quorum_minus_faults
    (faults : FixedFaultInterval config)
    {block : ValidatorBlock BlockId}
    (valid : block.HasQuorumImmediateParents config) :
    config.thresholds.quorum -
        (config.thresholds.fault + faults.unavailableStakeBound) <=
      weight config.authorityCount config.stake
        (VoterSet.diff block.parentAuthors faults.nonProgress) := by
  have quality := quorum_parent_layer_has_correct_available_stake faults valid
  omega

/-- Membership in the non-Byzantine parent-author set identifies one exact
parent reference and proves that its author is not Byzantine. -/
theorem non_byzantine_parent_author_has_reference
    (faults : FixedFaultInterval config)
    {block : ValidatorBlock BlockId} {author : Nat}
    (selected :
      VoterSet.diff block.parentAuthors faults.byzantine author = true) :
    exists parent,
      parent ∈ block.parents /\
        parent.author = author /\ faults.byzantine author = false := by
  have selectedFacts :
      block.parentAuthors author = true /\ faults.byzantine author = false := by
    simpa [VoterSet.diff] using selected
  have parentAuthor := selectedFacts.1
  simp only [ValidatorBlock.parentAuthors, List.any_eq_true, beq_iff_eq] at parentAuthor
  rcases parentAuthor with ⟨parent, parentMember, parentAuthor⟩
  exact ⟨parent, parentMember, parentAuthor, selectedFacts.2⟩

end ValidatorBlock

namespace CausalRecoveryCapsule

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}

/-- For each causal-read block above round one, the read contains the exact
parent bodies for distinct non-Byzantine authors whose stake, together with the
Byzantine budget, reaches quorum.

This is the weighted Mysticeti counterpart of Narwhal's per-round honest-block
argument. It does not claim a fraction of all bodies in the complete read. -/
theorem history_block_has_non_byzantine_parent_layer
    (faults : FixedFaultInterval config)
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {block : ValidatorBlock BlockId}
    (blockMember : block ∈ capsule.history)
    (blockAfterFirstRound : 1 < block.reference.round) :
    config.thresholds.quorum <= config.thresholds.fault +
        weight config.authorityCount config.stake
          (VoterSet.diff block.parentAuthors faults.byzantine) /\
      forall author,
        VoterSet.diff block.parentAuthors faults.byzantine author = true ->
          exists parentBlock,
            parentBlock ∈ capsule.history /\
              parentBlock.reference.author = author /\
              parentBlock.reference.round + 1 = block.reference.round /\
              faults.byzantine author = false := by
  have blockPositive := capsule.historyBlocksPositive block blockMember
  have valid :=
    capsule.positiveHistoryBlocksValid block blockMember blockPositive
  refine ⟨ValidatorBlock.quorum_parent_layer_has_non_byzantine_stake faults
    valid, ?_⟩
  intro author authorSelected
  rcases ValidatorBlock.non_byzantine_parent_author_has_reference faults
      authorSelected with
    ⟨parent, parentMember, parentAuthor, authorNotByzantine⟩
  have parentRound := valid.2.1 parent parentMember
  have parentPositive : 0 < parent.round := by omega
  rcases capsule.historyClosed block blockMember parent parentMember with
    parentGenesis | ⟨parentBlock, parentBlockMember, parentExact⟩
  · have parentRoundZero := capsule.genesisParentsAreRoundZero parent
      parentGenesis
    omega
  · exact ⟨parentBlock, parentBlockMember, by
      rw [parentExact]
      exact parentAuthor,
      by rw [parentExact]; exact parentRound,
      authorNotByzantine⟩

/-- At each positive round strictly below the causal-read target, one child in
the next round exposes a complete distinct-author parent layer in the read. Its
non-Byzantine parent stake is at least the quorum after the fault budget is
removed. -/
theorem has_non_byzantine_parent_layer_at_round
    (faults : FixedFaultInterval config)
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    {round : Nat}
    (roundPositive : 0 < round)
    (successorAtMostTarget : round + 1 <= capsule.targetRound) :
    exists child,
      child ∈ capsule.history /\
        child.reference.round = round + 1 /\
        config.thresholds.quorum <= config.thresholds.fault +
          weight config.authorityCount config.stake
            (VoterSet.diff child.parentAuthors faults.byzantine) /\
        forall author,
          VoterSet.diff child.parentAuthors faults.byzantine author = true ->
            exists parentBlock,
              parentBlock ∈ capsule.history /\
                parentBlock.reference.author = author /\
                parentBlock.reference.round = round /\
                faults.byzantine author = false := by
  rcases causal_history_has_valid_block_at_positive_round capsule
      (round := round + 1) (by omega) successorAtMostTarget with
    ⟨child, childMember, childRound, _childValid⟩
  rcases history_block_has_non_byzantine_parent_layer faults capsule
      childMember (by omega) with
    ⟨quality, parentBodies⟩
  refine ⟨child, childMember, childRound, quality, ?_⟩
  intro author selected
  rcases parentBodies author selected with
    ⟨parentBlock, parentMember, parentAuthor, parentRound, parentHonest⟩
  exact ⟨parentBlock, parentMember, parentAuthor, by omega, parentHonest⟩

end CausalRecoveryCapsule

end Mysticeti
