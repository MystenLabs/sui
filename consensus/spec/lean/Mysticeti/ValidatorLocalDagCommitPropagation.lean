/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorCommitCausalCarry
import Mysticeti.ValidatorBlockSyncBridge
import Mysticeti.ExactCommitPrefixSafety
import Mysticeti.ValidatorReferenceFlexTrace
import Mysticeti.ValidatorTraceFavorableWindow

namespace Mysticeti

/-! Pure bridges between the two exact local-DAG reachability relations.

`ValidatorCausalAncestor` follows parents from an earlier reference to a later
reference. `ValidatorCausalClosureReference` starts at the later reference and
walks to its parents. These relations describe the same finite parent path.
The results in this file add no execution or network assumptions.
-/

namespace ValidatorCausalClosureReference

variable {BlockId CommitId PacketId : Type}
variable {world : ValidatorWorldState BlockId CommitId PacketId}

/-- Two parent walks compose. -/
theorem append
    {outer middle inner : ValidatorBlockRef BlockId}
    (outerToMiddle : ValidatorCausalClosureReference world outer middle)
    (middleToInner : ValidatorCausalClosureReference world middle inner) :
    ValidatorCausalClosureReference world outer inner := by
  induction middleToInner with
  | anchor => exact outerToMiddle
  | parent childInClosure catalog referenceExact parentIncluded
      inductionHypothesis =>
      exact .parent inductionHypothesis catalog referenceExact parentIncluded

end ValidatorCausalClosureReference

/-- An ancestor path is the reverse view of one causal-closure walk. -/
theorem validator_causal_ancestor_to_closure
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {ancestor descendant : ValidatorBlockRef BlockId}
    (path : ValidatorCausalAncestor world ancestor descendant) :
    ValidatorCausalClosureReference world descendant ancestor := by
  induction path with
  | same => exact .anchor
  | parent catalog referenceExact parentIncluded =>
      exact .parent .anchor catalog referenceExact parentIncluded
  | trans _ _ ancestorToMiddle middleToDescendant =>
      exact ValidatorCausalClosureReference.append middleToDescendant
        ancestorToMiddle

/-- A causal-closure walk is the reverse view of one ancestor path. -/
theorem validator_causal_closure_to_ancestor
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {ancestor descendant : ValidatorBlockRef BlockId}
    (path : ValidatorCausalClosureReference world descendant ancestor) :
    ValidatorCausalAncestor world ancestor descendant := by
  induction path with
  | anchor => exact .same _
  | parent childInClosure catalog referenceExact parentIncluded
      inductionHypothesis =>
      exact .trans (.parent catalog referenceExact parentIncluded)
        inductionHypothesis

/-- The forward and reverse local-DAG relations are equivalent. -/
theorem validator_causal_ancestor_iff_closure
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {ancestor descendant : ValidatorBlockRef BlockId} :
    ValidatorCausalAncestor world ancestor descendant ↔
      ValidatorCausalClosureReference world descendant ancestor :=
  ⟨validator_causal_ancestor_to_closure,
    validator_causal_closure_to_ancestor⟩

/-- Exact references in one carrier closure that are strictly above a local GC
round. A parent at or below the cutoff is a boundary root and is not fetched by
this relation. -/
inductive ValidatorCausalClosureReferenceAboveRound
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (cutoff : Nat) (anchor : ValidatorBlockRef BlockId) :
    ValidatorBlockRef BlockId → Prop where
  | anchor
      (above : cutoff < anchor.round) :
      ValidatorCausalClosureReferenceAboveRound world cutoff anchor anchor
  | parent {child parent : ValidatorBlockRef BlockId}
      {block : ValidatorBlock BlockId} :
      ValidatorCausalClosureReferenceAboveRound world cutoff anchor child →
      world.blockCatalog child.id = some block →
      block.reference = child →
      parent ∈ block.parents →
      cutoff < parent.round →
      ValidatorCausalClosureReferenceAboveRound world cutoff anchor parent

namespace ValidatorCausalClosureReferenceAboveRound

/-- An above-cutoff parent walk remains valid when the durable block catalog
gains entries. -/
theorem of_catalog_monotone
    {BlockId CommitId PacketId : Type}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {cutoff : Nat} {anchor reference : ValidatorBlockRef BlockId}
    (catalogMonotone : OptionMapMonotone before.blockCatalog after.blockCatalog)
    (path : ValidatorCausalClosureReferenceAboveRound before cutoff anchor
      reference) :
    ValidatorCausalClosureReferenceAboveRound after cutoff anchor reference := by
  induction path with
  | anchor above => exact .anchor above
  | parent childInClosure catalog referenceExact parentIncluded above
      inductionHypothesis =>
      exact .parent inductionHypothesis
        (catalogMonotone _ _ catalog) referenceExact parentIncluded above

end ValidatorCausalClosureReferenceAboveRound

/-- One observer accepted every carrier ancestor that is strictly above its
local GC cutoff. -/
def ValidatorAcceptedCausalClosureAboveRound
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (observer cutoff : Nat) (anchor : ValidatorBlockRef BlockId) : Prop :=
  ∀ reference,
    ValidatorCausalClosureReferenceAboveRound world cutoff anchor reference →
    (world.validatorState observer).accepted reference = true

/-- One direct parent at or below the cutoff is a stopped boundary root. -/
def ValidatorCausalClosureBoundaryAtOrBelowRound
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (cutoff : Nat) (anchor root : ValidatorBlockRef BlockId) : Prop :=
  ∃ child block,
    ValidatorCausalClosureReferenceAboveRound world cutoff anchor child ∧
      world.blockCatalog child.id = some block ∧
      block.reference = child ∧
      root ∈ block.parents ∧
      root.round ≤ cutoff

/-- Removing the cutoff proof gives an ordinary causal-closure walk. -/
theorem validator_causal_closure_above_round_to_closure
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {cutoff : Nat} {anchor reference : ValidatorBlockRef BlockId}
    (path : ValidatorCausalClosureReferenceAboveRound world cutoff anchor
      reference) :
    ValidatorCausalClosureReference world anchor reference := by
  induction path with
  | anchor => exact .anchor
  | parent childInClosure catalog referenceExact parentIncluded _above
      inductionHypothesis =>
      exact .parent inductionHypothesis catalog referenceExact parentIncluded

/-- A full parent walk to an above-cutoff reference stays above the cutoff when
all catalog edges use the immediate preceding round. -/
theorem validator_causal_closure_to_above_round
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {cutoff : Nat} {anchor reference : ValidatorBlockRef BlockId}
    (parentsImmediate : ∀
      (child parent : ValidatorBlockRef BlockId) (block : ValidatorBlock BlockId),
      world.blockCatalog child.id = some block →
      block.reference = child →
      parent ∈ block.parents →
      parent.round + 1 = child.round)
    (path : ValidatorCausalClosureReference world anchor reference)
    (above : cutoff < reference.round) :
    ValidatorCausalClosureReferenceAboveRound world cutoff anchor reference := by
  induction path with
  | anchor => exact .anchor above
  | @parent child parent block childInClosure catalog referenceExact
      parentIncluded inductionHypothesis =>
      have childAbove : cutoff < child.round := by
        have immediate := parentsImmediate child parent block catalog
          referenceExact parentIncluded
        omega
      exact .parent (inductionHypothesis childAbove) catalog referenceExact
        parentIncluded above

/-- Full accepted closure implies accepted closure above every cutoff. -/
theorem validator_accepted_causal_closure_above_round_of_full
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {observer cutoff : Nat} {anchor : ValidatorBlockRef BlockId}
    (accepted : ValidatorAcceptedCausalClosure world observer anchor) :
    ValidatorAcceptedCausalClosureAboveRound world observer cutoff anchor := by
  intro reference path
  exact accepted reference
    (validator_causal_closure_above_round_to_closure path)

/-- A pinned source capsule contains the exact body for every target-closure
reference above a cutoff. The walk uses only immediate parent references. A
reference at or below the cutoff is not in the relation and remains an opaque
committed root. -/
theorem pinned_capsule_contains_above_cutoff_target_closure
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
    {holder time cutoff : Nat}
    {reference : ValidatorBlockRef BlockId}
    (source : CausalRecoveryCapsuleExecutionSource syncRules capsule holder time)
    (path : ValidatorCausalClosureReferenceAboveRound
      (timed.execution.trace time) cutoff capsule.targetBlock.reference
        reference) :
    ∃ block,
      block ∈ capsule.history ∧ block.reference = reference := by
  induction path with
  | anchor _above =>
      exact ⟨capsule.targetBlock,
        capsule.target_and_parents_in_history.1, rfl⟩
  | @parent child parent catalogBlock childInClosure catalog
      referenceExact parentIncluded parentAbove inductionHypothesis =>
      rcases inductionHypothesis with
        ⟨capsuleChild, childMember, capsuleChildReference⟩
      have capsuleCatalogAtChild :
          (timed.execution.trace time).blockCatalog child.id =
            some capsuleChild := by
        simpa [capsuleChildReference] using source.catalog capsuleChild childMember
      have sameChildBody : catalogBlock = capsuleChild := by
        rw [capsuleCatalogAtChild] at catalog
        exact (Option.some.inj catalog).symm
      have parentInCapsuleChild : parent ∈ capsuleChild.parents := by
        simpa [sameChildBody] using parentIncluded
      rcases capsule.historyClosed capsuleChild childMember parent
          parentInCapsuleChild with
        parentIsGenesis | ⟨parentBlock, parentMember, parentReference⟩
      · have parentRoundZero :=
          capsule.genesisParentsAreRoundZero parent parentIsGenesis
        omega
      · exact ⟨parentBlock, parentMember, parentReference⟩

/-- If a correct validator's GC frontier crosses a reference after its exact
prior commit round, its durable commit index has advanced. A same-index head
cannot change, and GC cannot exceed that unchanged head's round. -/
theorem gc_crossed_future_reference_implies_commit_index_advance
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {start finish validator : Time}
    {prior : ValidatorCommitHead CommitId}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (ordered : start ≤ finish)
    (headAtStart :
      ((execution.trace start).validatorState validator).commitHead = prior)
    (referenceAfterPrior : prior.round < reference.round)
    (crossed : reference.round ≤
      ((execution.trace finish).validatorState validator).gcRound) :
    prior.index <
      ((execution.trace finish).validatorState validator).commitHead.index := by
  have durable := execution.durableStateMonotone validator start finish
    validatorInRange ordered
  have gcAtMostHead := correct_validator_gc_round_at_most_commit_round execution
    (time := finish) validatorInRange validatorCorrect
  have priorRoundBeforeFinishHead : prior.round <
      ((execution.trace finish).validatorState validator).commitHead.round := by
    omega
  by_cases advanced : prior.index <
      ((execution.trace finish).validatorState validator).commitHead.index
  · exact advanced
  · exfalso
    have indexMonotone := durable.1
    rw [headAtStart] at indexMonotone
    have sameIndex :
        ((execution.trace start).validatorState validator).commitHead.index =
          ((execution.trace finish).validatorState validator).commitHead.index := by
      rw [headAtStart]
      omega
    have sameHead := durable.2.2.1 sameIndex
    have finishHeadIsPrior :
        ((execution.trace finish).validatorState validator).commitHead = prior :=
      sameHead.symm.trans headAtStart
    rw [finishHeadIsPrior] at priorRoundBeforeFinishHead
    exact (Nat.lt_irrefl prior.round) priorRoundBeforeFinishHead

/-- One required post-prior reference is either still above the requester's GC
frontier, or the durable prefix already contains the next commit index. This is
the cutoff split used by recursive local-DAG synchronization. -/
theorem required_reference_gc_split
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (prefixMap : ValidatorCommitPrefixSourceMap faults execution.trace)
    {start finish validator : Time}
    {prior : ValidatorCommitHead CommitId}
    {reference : ValidatorBlockRef BlockId}
    (validatorInRange : validator < config.authorityCount)
    (validatorCorrect : faults.correctAvailable validator = true)
    (ordered : start ≤ finish)
    (headAtStart :
      ((execution.trace start).validatorState validator).commitHead = prior)
    (referenceAfterPrior : prior.round < reference.round) :
    ((execution.trace finish).validatorState validator).gcRound <
        reference.round ∨
      ∃ commitId,
        ((execution.trace finish).validatorState validator).installedCommitAt
          (prior.index + 1) = some commitId := by
  by_cases aboveGc :
      ((execution.trace finish).validatorState validator).gcRound <
        reference.round
  · exact Or.inl aboveGc
  · have crossed : reference.round ≤
        ((execution.trace finish).validatorState validator).gcRound := by
      omega
    have advanced := gc_crossed_future_reference_implies_commit_index_advance
      execution validatorInRange validatorCorrect ordered headAtStart
        referenceAfterPrior crossed
    exact Or.inr (prefixMap.installedAtOrBelowHead finish validator
      (prior.index + 1) validatorInRange validatorCorrect (by
        change prior.index + 1 ≤
          ((execution.trace finish).validatorState validator).commitHead.index
        omega))

/-- One concrete later target carries the exact evidence used by a successful
source Flex run. This is an internal current-DAG result, not a permitted final
liveness input. Each anchor has a full correct-available direct-vote frontier,
and each new material block reaches the same target. -/
structure ValidatorExactFlexCarrierCoverage
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    (config : ValidatorEpochConfig CommitId)
    (faults : FixedFaultInterval config)
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (target : ValidatorBlockRef BlockId)
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (count : Nat)
    (output : LocalFlexCommitOutput BlockId CommitId) : Prop where
  catalogParentsImmediate :
    ValidatorCausalAncestor.CatalogParentsAreImmediate world
  leaderReachesTarget : ∀ offset,
    offset < count →
      ValidatorCausalAncestor world (leaderAt offset) target
  directVotesReachTarget : ∀ offset,
    offset < count →
      ∀ voter,
        voter < config.authorityCount →
        faults.correctAvailable voter = true →
        ∃ voteBlock : ValidatorBlock BlockId,
          world.blockCatalog voteBlock.reference.id = some voteBlock ∧
            voteBlock.reference.author = voter ∧
            voteBlock.reference.round = (leaderAt offset).round + 1 ∧
            leaderAt offset ∈ voteBlock.parents ∧
            ValidatorCausalAncestor world voteBlock.reference target
  materialReachesTarget : ∀ block,
    block ∈ output.builderInput.sortedCommittedBlocks →
      ValidatorCausalAncestor world
        (referenceLeaderBlockToValidatorBlockRef block) target

namespace ValidatorExactFlexCarrierCoverage

/-- Full Flex evidence carried by one block remains carried by a later causal
descendant after the durable catalog grows.

This is the pure bridge used by the post-install carrier construction. The
earlier recovery block can collect the complete direct-vote frontier. A later
ordinary proposal can then occur after the exact local commit install and use
that recovery block, or one of its descendants, as an ancestor. No future
proposal, delivery, or commit is assumed by this theorem. -/
theorem of_catalog_monotone_and_descendant
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {earlierTarget laterTarget : ValidatorBlockRef BlockId}
    {leaderAt : Nat → ValidatorBlockRef BlockId}
    {count : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (coverage : ValidatorExactFlexCarrierCoverage config faults before
      earlierTarget leaderAt count output)
    (catalogMonotone : OptionMapMonotone before.blockCatalog
      after.blockCatalog)
    (laterCatalogParentsImmediate :
      ValidatorCausalAncestor.CatalogParentsAreImmediate after)
    (earlierReachesLater : ValidatorCausalAncestor after earlierTarget
      laterTarget) :
    ValidatorExactFlexCarrierCoverage config faults after laterTarget leaderAt
      count output := by
  refine {
    catalogParentsImmediate := laterCatalogParentsImmediate
    leaderReachesTarget := ?_
    directVotesReachTarget := ?_
    materialReachesTarget := ?_ }
  · intro offset offsetInRange
    exact .trans
      (ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
        (coverage.leaderReachesTarget offset offsetInRange))
      earlierReachesLater
  · intro offset offsetInRange voter voterInRange voterCorrect
    rcases coverage.directVotesReachTarget offset offsetInRange voter
        voterInRange voterCorrect with
      ⟨voteBlock, voteCatalog, voteAuthor, voteRound, leaderParent,
        voteReachesEarlier⟩
    exact ⟨voteBlock, catalogMonotone _ _ voteCatalog, voteAuthor, voteRound,
      leaderParent,
      .trans
        (ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
          voteReachesEarlier)
        earlierReachesLater⟩
  · intro block blockInMaterial
    exact .trans
      (ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
        (coverage.materialReachesTarget block blockInMaterial))
      earlierReachesLater

end ValidatorExactFlexCarrierCoverage

/-- An above-GC source ancestor of one accepted target closure is accepted at
the requester. Only catalog persistence and exact immediate-parent rounds are
used to transfer the source path to the later trace state. -/
theorem accepted_above_gc_target_closure_accepts_source_ancestor
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    {sourceTime finish requester : Time}
    {target reference : ValidatorBlockRef BlockId}
    (sourceBeforeFinish : sourceTime ≤ finish)
    (catalogValid : ValidatorCausalAncestor.CatalogParentsAreImmediate
      (execution.trace sourceTime))
    (causal : ValidatorCausalAncestor (execution.trace sourceTime) reference
      target)
    (aboveGc :
      ((execution.trace finish).validatorState requester).gcRound <
        reference.round)
    (acceptedClosure : ValidatorAcceptedCausalClosureAboveRound
      (execution.trace finish) requester
        ((execution.trace finish).validatorState requester).gcRound target) :
    ((execution.trace finish).validatorState requester).accepted reference =
      true := by
  have sourceClosure := validator_causal_ancestor_to_closure causal
  have sourceAbove := validator_causal_closure_to_above_round
    (cutoff := ((execution.trace finish).validatorState requester).gcRound)
    (parentsImmediate := by
      intro child parent block catalog referenceExact parentIncluded
      have immediate := catalogValid child block catalog referenceExact parent
        parentIncluded
      simpa [referenceExact] using immediate)
    sourceClosure aboveGc
  have laterAbove :=
    ValidatorCausalClosureReferenceAboveRound.of_catalog_monotone
      (execution.blockCatalogMonotone sourceTime finish sourceBeforeFinish)
      sourceAbove
  exact acceptedClosure reference laterAbove

/-- Accepted above-GC target closure transfers one source carrier's complete
anchor and direct-vote range to a correct requester. If any required leader is
already at the requester GC frontier, the durable prefix instead proves that
the requester installed the next commit index. -/
theorem accepted_above_gc_carrier_coverage_gives_exact_range_or_installed_next
    {BlockId CommitId PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    (execution : ValidatorExecution (PacketId := PacketId) config faults
      protocolPacket network program)
    (representatives : ValidatorAcceptedRepresentativeRules execution)
    (prefixMap : ValidatorCommitPrefixSourceMap faults execution.trace)
    {sourceTime start finish requester count baseRound : Time}
    {prior : ValidatorCommitHead CommitId}
    {target : ValidatorBlockRef BlockId}
    {leaderAt : Nat → ValidatorBlockRef BlockId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (coverage : ValidatorExactFlexCarrierCoverage config faults
      (execution.trace sourceTime) target leaderAt count output)
    (sourceBeforeFinish : sourceTime ≤ finish)
    (startBeforeFinish : start ≤ finish)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (headAtStart :
      ((execution.trace start).validatorState requester).commitHead = prior)
    (baseAfterPrior : prior.round < baseRound)
    (leaderRound : ∀ offset, offset < count →
      (leaderAt offset).round = baseRound + offset)
    (acceptedClosure : ValidatorAcceptedCausalClosureAboveRound
      (execution.trace finish) requester
        ((execution.trace finish).validatorState requester).gcRound target) :
    (∃ witnessId,
      ((execution.trace finish).validatorState requester).installedCommitAt
        (prior.index + 1) = some witnessId) ∨
      (((execution.trace finish).validatorState requester).commitHead = prior ∧
        (∀ offset, offset < count →
          ((execution.trace finish).validatorState requester).accepted
                (leaderAt offset) = true ∧
              config.thresholds.quorum ≤
                weight config.authorityCount config.stake
                  (traceDirectVoters (execution.trace finish) requester
                    (leaderAt offset))) ∧
        ∀ block, block ∈ output.builderInput.sortedCommittedBlocks →
          (referenceLeaderBlockToValidatorBlockRef block).round ≤
              ((execution.trace finish).validatorState requester).gcRound ∨
            ((execution.trace finish).validatorState requester).accepted
                (referenceLeaderBlockToValidatorBlockRef block) = true) := by
  have durable := execution.durableStateMonotone requester start finish
    requesterInRange startBeforeFinish
  by_cases indexAdvanced : prior.index <
      ((execution.trace finish).validatorState requester).commitHead.index
  · have nextAtOrBelowHead : prior.index + 1 ≤
        (execution.trace finish).localCommitIndex requester := by
      change prior.index + 1 ≤
        ((execution.trace finish).validatorState requester).commitHead.index
      omega
    exact Or.inl (prefixMap.installedAtOrBelowHead finish requester
      (prior.index + 1) requesterInRange requesterCorrect nextAtOrBelowHead)
  · have indexMonotone := durable.1
    rw [headAtStart] at indexMonotone
    have sameIndex :
        ((execution.trace start).validatorState requester).commitHead.index =
          ((execution.trace finish).validatorState requester).commitHead.index := by
      rw [headAtStart]
      omega
    have headAtFinish :
        ((execution.trace finish).validatorState requester).commitHead = prior :=
      (durable.2.2.1 sameIndex).symm.trans headAtStart
    by_cases leaderAtRoot : ∃ offset,
        offset < count ∧
          (leaderAt offset).round ≤
            ((execution.trace finish).validatorState requester).gcRound
    · rcases leaderAtRoot with ⟨offset, offsetInRange, atRoot⟩
      have leaderAfterPrior : prior.round < (leaderAt offset).round := by
        rw [leaderRound offset offsetInRange]
        omega
      rcases required_reference_gc_split execution prefixMap requesterInRange
          requesterCorrect startBeforeFinish headAtStart leaderAfterPrior with
        leaderAbove | installedNext
      · omega
      · exact Or.inl installedNext
    · have everyLeaderAbove : ∀ offset, offset < count →
          ((execution.trace finish).validatorState requester).gcRound <
            (leaderAt offset).round := by
        intro offset offsetInRange
        by_cases above :
            ((execution.trace finish).validatorState requester).gcRound <
              (leaderAt offset).round
        · exact above
        · exfalso
          exact leaderAtRoot ⟨offset, offsetInRange, by omega⟩
      refine Or.inr ⟨headAtFinish, ?_, ?_⟩
      · intro offset offsetInRange
        have leaderAccepted :=
          accepted_above_gc_target_closure_accepts_source_ancestor execution
            sourceBeforeFinish coverage.catalogParentsImmediate
            (coverage.leaderReachesTarget offset offsetInRange)
            (everyLeaderAbove offset offsetInRange) acceptedClosure
        refine ⟨leaderAccepted, ?_⟩
        apply all_correct_available_children_vote_gives_quorum faults
        intro voter voterInRange voterCorrect
        rcases coverage.directVotesReachTarget offset offsetInRange voter
            voterInRange voterCorrect with
          ⟨voteBlock, voteCatalog, voteAuthor, voteRound, leaderParent,
            voteReachesTarget⟩
        have voteAbove :
            ((execution.trace finish).validatorState requester).gcRound <
              voteBlock.reference.round := by
          rw [voteRound]
          have := everyLeaderAbove offset offsetInRange
          omega
        have voteAccepted :=
          accepted_above_gc_target_closure_accepts_source_ancestor execution
            sourceBeforeFinish coverage.catalogParentsImmediate voteReachesTarget
            voteAbove acceptedClosure
        have voteCatalogAtFinish :
            (execution.trace finish).blockCatalog voteBlock.reference.id =
              some voteBlock :=
          execution.blockCatalogMonotone sourceTime finish sourceBeforeFinish
            voteBlock.reference.id voteBlock voteCatalog
        have voterNotByzantine : faults.byzantine voter = false := by
          have notNonProgress : faults.nonProgress voter = false := by
            simpa [FixedFaultInterval.correctAvailable, VoterSet.diff,
              VoterSet.full] using voterCorrect
          have separated : faults.byzantine voter = false ∧
              faults.unavailable voter = false := by
            simpa [FixedFaultInterval.nonProgress, VoterSet.union] using
              notNonProgress
          exact separated.1
        have peerRepresentative :=
          representatives.acceptedCorrectReferenceIsRecorded finish requester
            voteBlock.reference requesterInRange requesterCorrect
            (by simpa [voteAuthor] using voterInRange)
            (by simpa [voteAuthor] using voterNotByzantine) voteAccepted
        have representativeAtVoteRound :
            ((execution.trace finish).validatorState requester).acceptedRepresentative
                ((leaderAt offset).round + 1) voter =
              some voteBlock.reference := by
          simpa [voteAuthor, voteRound] using peerRepresentative
        exact accepted_child_with_leader_parent_is_direct_voter
          representativeAtVoteRound voteCatalogAtFinish rfl voteAuthor voteRound
          leaderParent
      · intro block blockInMaterial
        by_cases atRoot :
            (referenceLeaderBlockToValidatorBlockRef block).round ≤
              ((execution.trace finish).validatorState requester).gcRound
        · exact Or.inl atRoot
        · exact Or.inr
            (accepted_above_gc_target_closure_accepts_source_ancestor execution
              sourceBeforeFinish coverage.catalogParentsImmediate
              (coverage.materialReachesTarget block blockInMaterial)
              (by omega) acceptedClosure)

/-- Candidate-leader carry puts each newly committed material block in the
carrier's exact parent closure. -/
theorem successful_flex_output_candidate_carry_gives_full_material_closure
    {BlockId CommitId History Encoding PacketId : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ReferenceFlexCommitterContext BlockId History}
    {input : ReferenceFlexTryCommitInput BlockId CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    {carrier : ValidatorBlockRef BlockId}
    (found : tryReferenceFlexCommitWithContext functions context input =
      some output)
    (materialCausal : ValidatorMaterializedCandidateCausalClosure world
      output.candidate (input.materialize output.candidate))
    (candidateCarried : ∀ leader,
      leader ∈ output.candidate.orderedCommittedLeaders →
        ValidatorCausalAncestor world
          (referenceLeaderBlockToValidatorBlockRef leader) carrier) :
    ∀ block, block ∈ output.builderInput.sortedCommittedBlocks →
      ValidatorCausalClosureReference world carrier
        (referenceLeaderBlockToValidatorBlockRef block) := by
  intro block blockInOutput
  apply validator_causal_ancestor_to_closure
  exact successful_flex_output_candidate_carry_gives_full_material_carry found
    materialCausal candidateCarried block blockInOutput

/-- If one observer accepted the carrier's full parent closure, it accepted
every newly committed material block from the carried successful output. -/
theorem accepted_carrier_candidate_carry_accepts_full_commit_material
    {BlockId CommitId History Encoding PacketId : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ReferenceFlexCommitterContext BlockId History}
    {input : ReferenceFlexTryCommitInput BlockId CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {observer : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    {carrier : ValidatorBlockRef BlockId}
    (found : tryReferenceFlexCommitWithContext functions context input =
      some output)
    (materialCausal : ValidatorMaterializedCandidateCausalClosure world
      output.candidate (input.materialize output.candidate))
    (candidateCarried : ∀ leader,
      leader ∈ output.candidate.orderedCommittedLeaders →
        ValidatorCausalAncestor world
          (referenceLeaderBlockToValidatorBlockRef leader) carrier)
    (acceptedClosure : ValidatorAcceptedCausalClosure world observer carrier) :
    ∀ block, block ∈ output.builderInput.sortedCommittedBlocks →
      (world.validatorState observer).accepted
        (referenceLeaderBlockToValidatorBlockRef block) = true := by
  intro block blockInOutput
  exact acceptedClosure _
    (successful_flex_output_candidate_carry_gives_full_material_closure found
      materialCausal candidateCarried block blockInOutput)

/-- An accepted exact anchor range at one correct requester runs and records
the same exact successor as an earlier correct source run. The only alternative
is that the requester already installed the next index before the protected
run completed. The requester run is produced by the trace theorem; it is not a
premise. -/
theorem trace_direct_quorum_range_records_same_successor_or_installed_next
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (pending : ValidatorExactPendingIngestionRules source)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (work : ValidatorSuccessfulFlexScanWorkRules (faults := faults)
      (network := network) (timed := timed) source)
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (sourceRun : CorrectExactFlexRun runtime)
    {start observer baseRound : Nat}
    {prior : ValidatorCommitHead CommitId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (sourcePrior : sourceRun.prior = prior)
    (sourceBeforeStart : sourceRun.observation.time ≤ start)
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (headAtStart :
      ((timed.execution.trace start).validatorState observer).commitHead = prior)
    (baseAfterCommitHead : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (leaderAccepted : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      ((timed.execution.trace start).validatorState observer).accepted
        (leaderAt offset) = true)
    (directQuorum : ∀ offset,
      offset < (context observer
        ((timed.execution.trace start).validatorState observer)).depth + 1 →
      config.thresholds.quorum ≤
        weight config.authorityCount config.stake
          (traceDirectVoters (timed.execution.trace start) observer
            (leaderAt offset))) :
    (∃ finish witnessId,
      start ≤ finish ∧
        ((timed.execution.trace finish).validatorState observer).installedCommitAt
          (prior.index + 1) = some witnessId) ∨
      ∃ (peerRun : CorrectExactFlexRun runtime) (installedAt : Time),
        sourceRun.observation.time ≤ peerRun.observation.time ∧
          start ≤ peerRun.observation.time ∧
          peerRun.observation.validator = observer ∧
          peerRun.prior = prior ∧
          peerRun.output = sourceRun.output ∧
          peerRun.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              observer).installedCommitAt
                sourceRun.output.reference.index =
              some sourceRun.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              observer).commitInstallSourceAt
                sourceRun.output.reference.index =
              some .localExecution := by
  rcases trace_direct_quorum_range_runs_exact_committer_or_installed_next
      source runtime prefixMap pending direct work leaderAt observerInRange
      observerCorrect headAtStart baseAfterCommitHead leaderRound firstSelected
      leaderAccepted directQuorum with
    localRun | installedNext
  · rcases localRun with
      ⟨observation, output, startBeforeRun, observationValidator, returned,
        successful, inputPrior, _outputNext⟩
    let peerRun : CorrectExactFlexRun runtime :=
      { observation := observation
        output := output
        prior := prior
        validatorInRange := by simpa [observationValidator] using observerInRange
        validatorCorrect := by simpa [observationValidator] using observerCorrect
        returned := returned
        successful := successful
        priorAtInput := inputPrior }
    have samePrior : sourceRun.prior = peerRun.prior := by
      simpa [peerRun] using sourcePrior
    have sameOutput : sourceRun.output = peerRun.output :=
      correct_local_flex_runs_same_prior_exact_output authenticated sourceRun
        peerRun samePrior
    have sameOutputAtPeer : peerRun.output = sourceRun.output := sameOutput.symm
    rcases successful_local_flex_run_completes_and_persists_exact runtime
        observation returned successful
        (by simpa [observationValidator] using observerInRange)
        (by simpa [observationValidator] using observerCorrect) with
      ⟨recordAt, _exactResult, runBeforeRecord, _recordBound, installed,
        localSource⟩
    refine Or.inr ⟨peerRun, recordAt + 1,
      Nat.le_trans sourceBeforeStart startBeforeRun, startBeforeRun,
      ?_, rfl, sameOutputAtPeer, ?_, ?_, ?_⟩
    · simpa [peerRun] using observationValidator
    · change observation.time < recordAt + 1
      exact Nat.lt_of_lt_of_le (Nat.lt_succ_self observation.time)
        (Nat.le_trans runBeforeRecord (Nat.le_add_right recordAt 1))
    · rw [sameOutput]
      simpa [observationValidator] using installed
    · rw [sameOutput]
      simpa [observationValidator] using localSource
  · exact Or.inl installedNext

/-- One accepted full carrier closure either finds an already installed next
index or runs and records the source run's exact successor at the requester.
The accepted leaders, direct-vote quorums, and cutoff material disposition are
derived internally and do not occur in the result. Local material completeness
for the returned peer output remains the one-host `LocalFlexCommitterSourceMap`
refinement used by exact Flex safety. -/
theorem accepted_carrier_coverage_records_same_successor_or_installed_next
    {BlockId CommitId History Encoding PacketId : Type}
    [DecidableEq BlockId]
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (source : LocalFlexCommitterSourceMap config functions context program)
    (runtime : LocalFlexCommitterRuntime timed source)
    (prefixMap : ValidatorCommitPrefixSourceMap faults timed.execution.trace)
    (pending : ValidatorExactPendingIngestionRules source)
    (direct : ValidatorExactDirectRuleSourceMap (PacketId := PacketId)
      (faults := faults) source)
    (work : ValidatorSuccessfulFlexScanWorkRules (faults := faults)
      (network := network) (timed := timed) source)
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (representatives : ValidatorAcceptedRepresentativeRules
      timed.execution)
    (sourceRun : CorrectExactFlexRun runtime)
    {sourceTime peerStart finish requester baseRound : Time}
    {prior : ValidatorCommitHead CommitId}
    {target : ValidatorBlockRef BlockId}
    (leaderAt : Nat → ValidatorBlockRef BlockId)
    (coverage : ValidatorExactFlexCarrierCoverage config faults
      (timed.execution.trace sourceTime) target leaderAt
        ((context requester
          ((timed.execution.trace finish).validatorState requester)).depth + 1)
        sourceRun.output)
    (sourcePrior : sourceRun.prior = prior)
    (sourceRunBeforeCarrier : sourceRun.observation.time ≤ sourceTime)
    (sourceBeforeFinish : sourceTime ≤ finish)
    (peerStartBeforeFinish : peerStart ≤ finish)
    (requesterInRange : requester < config.authorityCount)
    (requesterCorrect : faults.correctAvailable requester = true)
    (headAtPeerStart :
      ((timed.execution.trace peerStart).validatorState requester).commitHead =
        prior)
    (baseAfterPrior : prior.round < baseRound)
    (leaderRound : ∀ offset,
      offset < (context requester
        ((timed.execution.trace finish).validatorState requester)).depth + 1 →
      (leaderAt offset).round = baseRound + offset)
    (firstSelected : ∀ offset,
      offset < (context requester
        ((timed.execution.trace finish).validatorState requester)).depth + 1 →
      (config.selectedLeaderOrder prior.id (baseRound + offset)).head? =
        some (leaderAt offset).author)
    (acceptedClosure : ValidatorAcceptedCausalClosureAboveRound
      (timed.execution.trace finish) requester
        ((timed.execution.trace finish).validatorState requester).gcRound
        target) :
    (∃ later witnessId,
      peerStart ≤ later ∧
        ((timed.execution.trace later).validatorState requester).installedCommitAt
          (prior.index + 1) = some witnessId) ∨
      ∃ (peerRun : CorrectExactFlexRun runtime) (installedAt : Time),
        sourceRun.observation.time ≤ peerRun.observation.time ∧
          peerStart ≤ peerRun.observation.time ∧
          peerRun.observation.validator = requester ∧
          peerRun.prior = prior ∧
          peerRun.output = sourceRun.output ∧
          peerRun.observation.time < installedAt ∧
          ((timed.execution.trace installedAt).validatorState
              requester).installedCommitAt
                sourceRun.output.reference.index =
              some sourceRun.output.reference.digest ∧
          ((timed.execution.trace installedAt).validatorState
              requester).commitInstallSourceAt
                sourceRun.output.reference.index =
              some .localExecution := by
  rcases accepted_above_gc_carrier_coverage_gives_exact_range_or_installed_next
      timed.execution representatives prefixMap coverage sourceBeforeFinish
      peerStartBeforeFinish requesterInRange requesterCorrect headAtPeerStart
      baseAfterPrior leaderRound acceptedClosure with
    installedAtFinish | ⟨headAtFinish, rangeAtFinish, _materialAtFinish⟩
  · rcases installedAtFinish with ⟨witnessId, installed⟩
    exact Or.inl ⟨finish, witnessId, peerStartBeforeFinish, installed⟩
  · rcases trace_direct_quorum_range_records_same_successor_or_installed_next
        source runtime prefixMap pending direct work authenticated sourceRun
        leaderAt sourcePrior
        (Nat.le_trans sourceRunBeforeCarrier sourceBeforeFinish)
        requesterInRange requesterCorrect headAtFinish baseAfterPrior leaderRound
        firstSelected
        (fun offset offsetInRange =>
          (rangeAtFinish offset offsetInRange).1)
        (fun offset offsetInRange =>
          (rangeAtFinish offset offsetInRange).2) with
      installedLater | recorded
    · rcases installedLater with
        ⟨later, witnessId, finishBeforeLater, installed⟩
      exact Or.inl ⟨later, witnessId,
        Nat.le_trans peerStartBeforeFinish finishBeforeLater, installed⟩
    · rcases recorded with
        ⟨peerRun, installedAt, sourceBeforePeer, finishBeforePeer,
          peerValidator, peerPrior, sameOutput, peerBeforeInstall, installed,
          localSource⟩
      exact Or.inr ⟨peerRun, installedAt, sourceBeforePeer,
        Nat.le_trans peerStartBeforeFinish finishBeforePeer, peerValidator,
        peerPrior, sameOutput, peerBeforeInstall, installed, localSource⟩

end Mysticeti
