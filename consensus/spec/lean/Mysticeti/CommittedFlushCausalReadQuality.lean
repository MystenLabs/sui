/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ReferenceCommitMaterializerDfs

namespace Mysticeti

/-! Weighted causal-read quality for every actual committed flush.

`ValidatorCausalReadQuality` proves the weighted honest-parent bound for a
recovery capsule. This module carries the same bound to the block set that the
Rust materializer actually commits, and it locates each exact honest parent
body.

A committed flush is one `FlexCommitter::build_commit` result. For each block in
the flush and each distinct non-Byzantine parent author counted by the bound,
the exact parent body is in one of three places:

* the same flush;
* the committed mark that an earlier commit already set, by this walk for a
  local commit or by `FlexCommitter::handle_certified_commit` for a
  synchronized one; or
* at or below the local GC round. The walk stops there. Such a reference is an
  old root of the committed history, but this result does not by itself say that
  the body entered a durable commit body.

The third case is only reachable one round above the GC boundary, because valid
parents are immediate. Above that boundary the honest layer is inside the
installed committed prefix.
-/

namespace CommitMaterializerView

variable {BlockId CommitId : Type}
variable {config : ValidatorEpochConfig CommitId}
variable {view : CommitMaterializerView BlockId}

/-- The local-view facts that the Rust walk relies on.

`catalogValid` and `leaderValid` are block-verification results. `eligibleBody`
is the `get_blocks(..).expect(..)` obligation inside the walk: every above-GC
ancestor that no earlier commit took has its body in the local `DagState`. -/
structure MaterializerValidity
    (config : ValidatorEpochConfig CommitId)
    (view : CommitMaterializerView BlockId)
    (leaders : List (ValidatorBlock BlockId)) : Prop where
  leadersValid : ∀ leader, leader ∈ leaders →
    leader.HasQuorumImmediateParents config
  /-- `DagState::get_blocks` also returns genesis blocks, and a genesis block
  has no ancestors. The parent-quorum rule therefore applies only above round
  zero. Every block that the walk follows is above the GC round, so this is
  enough. -/
  catalogValid : ∀ (reference : ValidatorBlockRef BlockId) block,
    view.catalog reference.id = some block →
      0 < block.reference.round →
      block.HasQuorumImmediateParents config
  eligibleBody : ∀ block, view.Materialized leaders block →
    ∀ (reference : ValidatorBlockRef BlockId),
      reference ∈ view.ancestorsOf block →
      view.EligibleAncestor reference →
      ∃ body, view.catalog reference.id = some body

/-- Where one exact parent reference of a flush block stands.

Only `inFlush` supplies a body. `inEarlierCommit` supplies the committed mark
that an earlier commit set, and `committedRoot` supplies only that the reference
is at or below the local GC boundary. A reference at or below that boundary can
also have been pruned without entering any commit, so this branch is a boundary
result and not a durability claim. -/
inductive ParentLocation (view : CommitMaterializerView BlockId)
    (leaders : List (ValidatorBlock BlockId))
    (reference : ValidatorBlockRef BlockId) : Prop where
  | inFlush (body : ValidatorBlock BlockId) :
      view.catalog reference.id = some body →
      view.Materialized leaders body →
      ParentLocation view leaders reference
  | inEarlierCommit : reference.id ∈ view.committedBefore →
      ParentLocation view leaders reference
  | committedRoot : reference.round ≤ view.gcRound →
      ParentLocation view leaders reference

variable {leaders : List (ValidatorBlock BlockId)}

/-- Every block that the run commits passes the parent-quorum check. -/
theorem materialized_valid
    (validity : MaterializerValidity config view leaders)
    {block : ValidatorBlock BlockId}
    (materialized : view.Materialized leaders block) :
    block.HasQuorumImmediateParents config := by
  induction materialized with
  | leader member => exact validity.leadersValid _ member
  | @ancestor _ parent reference _ _ aboveGc _ body _ =>
      refine validity.catalogValid reference parent body ?_
      rw [view.catalogNamesBlock reference parent body]
      omega

/-- Each exact parent of a flush block is in the flush, in an earlier flush of
the same prefix, or at or below the GC boundary. -/
theorem materialized_parent_is_located
    (validity : MaterializerValidity config view leaders)
    {block : ValidatorBlock BlockId}
    (materialized : view.Materialized leaders block)
    {reference : ValidatorBlockRef BlockId}
    (referenceMember : reference ∈ block.parents) :
    ParentLocation view leaders reference := by
  have ancestorMember := view.parentsAreAncestors block reference referenceMember
  by_cases aboveGc : view.gcRound < reference.round
  · by_cases fresh : reference.id ∈ view.committedBefore
    · exact .inEarlierCommit fresh
    · rcases validity.eligibleBody block materialized reference ancestorMember
        ⟨aboveGc, fresh⟩ with ⟨body, catalogued⟩
      exact .inFlush body catalogued
        (materialized_contains_eligible_ancestor materialized ancestorMember
          ⟨aboveGc, fresh⟩ catalogued)
  · exact .committedRoot (Nat.le_of_not_lt aboveGc)

/-- Weighted causal-read quality for one committed flush block.

The distinct non-Byzantine parent authors reach the quorum after the Byzantine
budget is removed, and each of them supplies one located exact parent body. This
is the weighted Mysticeti counterpart of the per-round honest-block argument. It
does not claim a fraction of all bodies in the flush. -/
theorem committed_flush_block_has_non_byzantine_parent_layer
    (faults : FixedFaultInterval config)
    (validity : MaterializerValidity config view leaders)
    {block : ValidatorBlock BlockId}
    (materialized : view.Materialized leaders block) :
    config.thresholds.quorum ≤ config.thresholds.fault +
        weight config.authorityCount config.stake
          (VoterSet.diff block.parentAuthors faults.byzantine) ∧
      ∀ author,
        VoterSet.diff block.parentAuthors faults.byzantine author = true →
          ∃ reference,
            reference ∈ block.parents ∧
              reference.author = author ∧
              reference.round + 1 = block.reference.round ∧
              faults.byzantine author = false ∧
              ParentLocation view leaders reference := by
  have valid := materialized_valid validity materialized
  refine ⟨ValidatorBlock.quorum_parent_layer_has_non_byzantine_stake faults
    valid, ?_⟩
  intro author selected
  rcases ValidatorBlock.non_byzantine_parent_author_has_reference faults
      selected with ⟨reference, referenceMember, referenceAuthor, honest⟩
  exact ⟨reference, referenceMember, referenceAuthor,
    valid.2.1 reference referenceMember, honest,
    materialized_parent_is_located validity materialized referenceMember⟩

/-- Above the GC boundary the committed prefix holds the complete honest parent
layer. A block more than one round above the boundary has no parent that a
committed root can supply, so each honest parent is in this flush or in an
earlier flush of the same durable prefix. -/
theorem committed_flush_block_covers_non_byzantine_parent_layer
    (faults : FixedFaultInterval config)
    (validity : MaterializerValidity config view leaders)
    {block : ValidatorBlock BlockId}
    (materialized : view.Materialized leaders block)
    (aboveBoundary : view.gcRound + 1 < block.reference.round) :
    config.thresholds.quorum ≤ config.thresholds.fault +
        weight config.authorityCount config.stake
          (VoterSet.diff block.parentAuthors faults.byzantine) ∧
      ∀ author,
        VoterSet.diff block.parentAuthors faults.byzantine author = true →
          ∃ reference,
            reference ∈ block.parents ∧
              reference.author = author ∧
              reference.round + 1 = block.reference.round ∧
              faults.byzantine author = false ∧
              ((∃ body, view.catalog reference.id = some body ∧
                  view.Materialized leaders body) ∨
                reference.id ∈ view.committedBefore) := by
  rcases committed_flush_block_has_non_byzantine_parent_layer faults validity
      materialized with ⟨quality, layer⟩
  refine ⟨quality, ?_⟩
  intro author selected
  rcases layer author selected with
    ⟨reference, referenceMember, referenceAuthor, referenceRound, honest,
      location⟩
  refine ⟨reference, referenceMember, referenceAuthor, referenceRound, honest,
    ?_⟩
  cases location with
  | inFlush body catalogued bodyMaterialized =>
      exact Or.inl ⟨body, catalogued, bodyMaterialized⟩
  | inEarlierCommit earlier => exact Or.inr earlier
  | committedRoot atBoundary => omega

/-! ### The sorted committed vector

`build_commit` sorts the walk result and puts that vector in the
`CommittedSubDag` and in the serialized commit body. The next result carries the
weighted bound to every block of that vector, so the property holds for the
flush that the product actually commits and votes for.
-/

/-- Every block in one sorted committed vector has the weighted honest parent
layer, and each honest parent reference is located. -/
theorem sorted_commit_block_has_non_byzantine_parent_layer
    [DecidableEq BlockId]
    (faults : FixedFaultInterval config)
    (validity : MaterializerValidity config view leaders)
    (blockSort : CommitBlockSort BlockId)
    {fuel : Nat} {out : List (ValidatorBlock BlockId)}
    {finalCommitted : List BlockId}
    (run : view.buildCommit fuel leaders = some (out, finalCommitted))
    (leadersCatalogued : ∀ leader, leader ∈ leaders →
      view.catalog leader.reference.id = some leader)
    {block : ValidatorBlock BlockId}
    (committedMember : block ∈ blockSort.sort leaders out) :
    config.thresholds.quorum ≤ config.thresholds.fault +
        weight config.authorityCount config.stake
          (VoterSet.diff block.parentAuthors faults.byzantine) ∧
      ∀ author,
        VoterSet.diff block.parentAuthors faults.byzantine author = true →
          ∃ reference,
            reference ∈ block.parents ∧
              reference.author = author ∧
              reference.round + 1 = block.reference.round ∧
              faults.byzantine author = false ∧
              ParentLocation view leaders reference := by
  have materialized :=
    (buildCommit_materializes_exactly run leadersCatalogued block).mp
      ((blockSort.keepsBlocks leaders out block).mp committedMember)
  exact committed_flush_block_has_non_byzantine_parent_layer faults validity
    materialized

end CommitMaterializerView

end Mysticeti
