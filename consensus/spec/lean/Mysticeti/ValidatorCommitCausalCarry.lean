/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorAnchorBridge
import Mysticeti.ValidatorCausalRecoveryCapsule
import Mysticeti.ValidatorExecutionLemmas
import Mysticeti.ExactCommitPrefixSafety
import Mysticeti.ReferenceFlexCommitter
import Mysticeti.ReferenceFlexIndexedListBridge
import Mysticeti.ReachableAnchorFlexAgreement
import Mysticeti.ValidatorFlexCommitter

namespace Mysticeti

/-! Causal carry of committed leader blocks.

The relation in this file is the transitive closure of exact parent references
in one local block catalog. The main theorem does not assume that every later
proposal carries a committed frontier. It derives one carry edge from two
actual quorum sets: the direct voters for one leader and the immediate parents
of one later block.
-/

/-- One exact block reference is in the causal history of another exact block
reference in one local catalog. -/
inductive ValidatorCausalAncestor
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId) :
    ValidatorBlockRef BlockId → ValidatorBlockRef BlockId → Prop
  | same (reference) : ValidatorCausalAncestor world reference reference
  | parent {parent child : ValidatorBlockRef BlockId}
      {block : ValidatorBlock BlockId} :
      world.blockCatalog child.id = some block →
      block.reference = child →
      parent ∈ block.parents →
      ValidatorCausalAncestor world parent child
  | trans {ancestor middle descendant : ValidatorBlockRef BlockId} :
      ValidatorCausalAncestor world ancestor middle →
      ValidatorCausalAncestor world middle descendant →
      ValidatorCausalAncestor world ancestor descendant

namespace ValidatorCausalAncestor

variable {BlockId CommitId PacketId : Type}
variable {world : ValidatorWorldState BlockId CommitId PacketId}

/-- An exact immediate parent is a causal ancestor. -/
theorem of_parent
    {parent : ValidatorBlockRef BlockId} {child : ValidatorBlock BlockId}
    (catalog : world.blockCatalog child.reference.id = some child)
    (included : parent ∈ child.parents) :
    ValidatorCausalAncestor world parent child.reference := by
  exact .parent catalog rfl included

/-- Causal ancestry remains true when the durable block catalog gains entries. -/
theorem of_catalog_monotone
    {BlockId CommitId PacketId : Type}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    (catalogMonotone : OptionMapMonotone before.blockCatalog after.blockCatalog)
    {ancestor descendant : ValidatorBlockRef BlockId}
    (causal : ValidatorCausalAncestor before ancestor descendant) :
    ValidatorCausalAncestor after ancestor descendant := by
  induction causal with
  | same reference => exact .same reference
  | parent catalog blockReference included =>
      exact .parent
        (catalogMonotone _ _ catalog) blockReference included
  | trans _ _ left right => exact .trans left right

/-- Accepted catalog blocks use only immediate-round parent references. -/
def CatalogParentsAreImmediate
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId) : Prop :=
  ∀ reference block,
    world.blockCatalog reference.id = some block →
    block.reference = reference →
    block.ParentsAreImmediate

/-- Causal ancestry cannot decrease the block round when catalog parents are
immediate. -/
theorem round_le
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    (catalogValid : CatalogParentsAreImmediate world)
    {ancestor descendant : ValidatorBlockRef BlockId}
    (causal : ValidatorCausalAncestor world ancestor descendant) :
    ancestor.round ≤ descendant.round := by
  induction causal with
  | same _ => exact Nat.le_refl _
  | @parent parent child block catalog blockReference included =>
      have immediate := catalogValid child block catalog blockReference
        parent included
      rw [blockReference] at immediate
      omega
  | trans _ _ left right => exact Nat.le_trans left right

/-- A causal ancestor of a round-zero reference is that exact reference. -/
theorem eq_of_descendant_round_zero
    {BlockId CommitId PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    (catalogValid : CatalogParentsAreImmediate world)
    {ancestor descendant : ValidatorBlockRef BlockId}
    (causal : ValidatorCausalAncestor world ancestor descendant)
    (roundZero : descendant.round = 0) :
    ancestor = descendant := by
  induction causal with
  | same _ => rfl
  | @parent parent child block catalog blockReference included =>
      have immediate := catalogValid child block catalog blockReference
        parent included
      rw [blockReference] at immediate
      omega
  | @trans ancestor middle descendant leftCausal rightCausal leftIH rightIH =>
      have middleEquals : middle = descendant := rightIH roundZero
      subst middle
      exact leftIH roundZero

end ValidatorCausalAncestor

/-- Any causal ancestor of a capsule history block is either one configured
genesis root or another exact block body in the capsule history. -/
theorem causal_ancestor_of_capsule_history_block_is_listed_or_genesis
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (catalogValid : ValidatorCausalAncestor.CatalogParentsAreImmediate world)
    (historyCatalog : ∀ block, block ∈ capsule.history →
      world.blockCatalog block.reference.id = some block)
    {ancestor descendant : ValidatorBlockRef BlockId}
    (causal : ValidatorCausalAncestor world ancestor descendant)
    (descendantBody : ∃ block,
      block ∈ capsule.history ∧ block.reference = descendant) :
    ancestor ∈ capsule.genesisParents ∨
      ∃ block, block ∈ capsule.history ∧ block.reference = ancestor := by
  induction causal with
  | same reference => exact Or.inr descendantBody
  | @parent parent child block catalog blockReference included =>
      rcases descendantBody with
        ⟨historyChild, historyChildMember, historyChildReference⟩
      have sameCatalog : world.blockCatalog child.id = some historyChild := by
        rw [← historyChildReference]
        exact historyCatalog historyChild historyChildMember
      have sameBlock : block = historyChild := by
        rw [catalog] at sameCatalog
        exact Option.some.inj sameCatalog
      subst block
      rcases capsule.historyClosed historyChild historyChildMember parent
          (by simpa [blockReference] using included) with
        parentGenesis | ⟨parentBlock, parentMember, parentReference⟩
      · exact Or.inl parentGenesis
      · exact Or.inr ⟨parentBlock, parentMember, parentReference⟩
  | @trans ancestor middle descendant leftCausal rightCausal leftIH rightIH =>
      rcases rightIH descendantBody with
        middleGenesis | ⟨middleBlock, middleMember, middleReference⟩
      · have middleRoundZero :=
          capsule.genesisParentsAreRoundZero middle middleGenesis
        have ancestorEqualsMiddle :=
          ValidatorCausalAncestor.eq_of_descendant_round_zero catalogValid
            leftCausal middleRoundZero
        rw [ancestorEqualsMiddle]
        exact Or.inl middleGenesis
      · exact leftIH ⟨middleBlock, middleMember, middleReference⟩

/-- Every positive-round causal ancestor of the capsule target has one exact
body in the concrete finite capsule history. -/
theorem positive_causal_ancestor_of_capsule_target_is_in_history
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    (capsule : CausalRecoveryCapsule (BlockId := BlockId) config)
    (catalogValid : ValidatorCausalAncestor.CatalogParentsAreImmediate world)
    (historyCatalog : ∀ block, block ∈ capsule.history →
      world.blockCatalog block.reference.id = some block)
    {ancestor : ValidatorBlockRef BlockId}
    (ancestorPositive : 0 < ancestor.round)
    (causal : ValidatorCausalAncestor world ancestor
      capsule.targetBlock.reference) :
    ∃ block, block ∈ capsule.history ∧ block.reference = ancestor := by
  have targetBody : ∃ block,
      block ∈ capsule.history ∧
        block.reference = capsule.targetBlock.reference :=
    ⟨capsule.targetBlock, capsule.target_and_parents_in_history.1, rfl⟩
  rcases causal_ancestor_of_capsule_history_block_is_listed_or_genesis capsule
      catalogValid historyCatalog causal targetBody with
    ancestorGenesis | ancestorBody
  · have roundZero := capsule.genesisParentsAreRoundZero ancestor
      ancestorGenesis
    omega
  · exact ancestorBody

/-- Two quorum sets contain a correct, available authority in their
intersection.

This uses only the configured threshold equations and the Byzantine and
unavailable stake bounds. -/
theorem two_quorums_have_correct_available_intersection
    {CommitId : Type} {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    {left right : VoterSet}
    (leftQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake left)
    (rightQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake right) :
    ∃ authority,
      authority < config.authorityCount ∧
        faults.correctAvailable authority = true ∧
        left authority = true ∧ right authority = true := by
  have overlapLower := intersection_lower_bound config.authorityCount
    config.stake leftQuorum rightQuorum
  have overlapNonProgressBound :
      weight config.authorityCount config.stake
          (VoterSet.inter (VoterSet.inter left right) faults.nonProgress) ≤
        config.thresholds.fault + faults.unavailableStakeBound := by
    exact Nat.le_trans
      (weight_mono config.stake
        (VoterSet.inter_subset_right config.authorityCount
          (VoterSet.inter left right)
          faults.nonProgress))
      faults.non_progress_stake_bounded
  have partition := weight_diff_add_inter config.authorityCount config.stake
    (VoterSet.inter left right) faults.nonProgress
  have totalShape :
      totalWeight config.authorityCount config.stake =
        config.thresholds.quorum +
          (config.thresholds.fault + faults.unavailableStakeBound) := by
    rw [faults.quorumDefinition]
    exact (Nat.sub_add_cancel faults.faultBudgetsFit).symm
  have overlapAboveBudget :
      config.thresholds.fault + faults.unavailableStakeBound <
        weight config.authorityCount config.stake
          (VoterSet.inter left right) := by
    have preserves := config.thresholds.quorum_preserves_certificate
    have intersects := config.thresholds.quorum_certificate_intersection
    rw [totalShape] at overlapLower preserves intersects
    omega
  have usefulPositive :
      0 < weight config.authorityCount config.stake
        (VoterSet.diff (VoterSet.inter left right) faults.nonProgress) := by
    omega
  rcases positive_weight_has_member usefulPositive with
    ⟨authority, authorityInRange, selected, _positiveStake⟩
  have selectedFactsRaw :
      (left authority = true ∧ right authority = true) ∧
        faults.nonProgress authority = false := by
    simpa [VoterSet.diff, VoterSet.inter] using selected
  have selectedFacts :
      left authority = true ∧ right authority = true ∧
        faults.nonProgress authority = false :=
    ⟨selectedFactsRaw.1.1, selectedFactsRaw.1.2, selectedFactsRaw.2⟩
  have correctAvailable : faults.correctAvailable authority = true := by
    simp [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full,
      selectedFacts.2.2]
  exact ⟨authority, authorityInRange, correctAvailable,
    selectedFacts.1, selectedFacts.2.1⟩

/-- A later quorum-parent block carries one directly committed leader.

The later block is two rounds after the leader. Its parent set and the leader's
direct-voter set are both quorums. Their intersection contains one correct,
available author. Correct-author non-equivocation makes the exact parent branch
equal to that author's direct-vote block.
-/
theorem direct_quorum_and_later_quorum_block_carries_leader
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
    {time observer : Nat}
    {leader : ValidatorBlockRef BlockId}
    {carrier : ValidatorBlock BlockId}
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (carrierCatalog :
      (execution.trace time).blockCatalog carrier.reference.id = some carrier)
    (carrierRound : carrier.reference.round = leader.round + 2)
    (carrierParents : carrier.HasQuorumImmediateParents config)
    (carrierParentsAccepted : ∀ parent, parent ∈ carrier.parents →
      ((execution.trace time).validatorState observer).accepted parent = true)
    (directQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters (execution.trace time) observer leader)) :
    ValidatorCausalAncestor (execution.trace time) leader carrier.reference := by
  rcases two_quorums_have_correct_available_intersection faults directQuorum
      carrierParents.2.2 with
    ⟨author, authorInRange, authorCorrect, directVote, parentAuthor⟩
  have parentExists : ∃ parent,
      parent ∈ carrier.parents ∧ parent.author = author := by
    simpa [ValidatorBlock.parentAuthors] using parentAuthor
  rcases parentExists with ⟨parent, parentIncluded, parentAuthorExact⟩
  have parentRound : parent.round = leader.round + 1 := by
    have immediate := carrierParents.2.1 parent parentIncluded
    omega
  have parentAccepted := carrierParentsAccepted parent parentIncluded
  have authorNotByzantine : faults.byzantine author = false := by
    have notNonProgress : faults.nonProgress author = false := by
      simpa [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full]
        using authorCorrect
    have separate : faults.byzantine author = false ∧
        faults.unavailable author = false := by
      simpa [FixedFaultInterval.nonProgress, VoterSet.union] using notNonProgress
    exact separate.1
  have parentRecorded := representatives.acceptedCorrectReferenceIsRecorded
    time observer parent observerInRange observerCorrect
      (by simpa [parentAuthorExact])
      (by simpa [parentAuthorExact] using authorNotByzantine) parentAccepted
  have parentRecordedAtVoteRound :
      ((execution.trace time).validatorState observer).acceptedRepresentative
          (leader.round + 1) author = some parent := by
    simpa [parentRound, parentAuthorExact] using parentRecorded
  unfold traceDirectVoters at directVote
  rw [parentRecordedAtVoteRound] at directVote
  cases parentCatalog : (execution.trace time).blockCatalog parent.id with
  | none => simp [parentCatalog] at directVote
  | some voteBlock =>
      have directFacts :
          voteBlock.reference = parent ∧
            parent.author = author ∧
            parent.round = leader.round + 1 ∧
            leader ∈ voteBlock.parents := by
        simpa [parentCatalog] using directVote
      have voteBlockIsCarrierParent :
          ValidatorCausalAncestor (execution.trace time) leader parent := by
        exact .parent parentCatalog directFacts.1 directFacts.2.2.2
      have parentIsCarrierAncestor :
          ValidatorCausalAncestor (execution.trace time) parent
            carrier.reference :=
        ValidatorCausalAncestor.of_parent carrierCatalog parentIncluded
      exact .trans voteBlockIsCarrierParent parentIsCarrierAncestor

/-- One later quorum-parent block carries every directly committed leader in a
multi-leader candidate. Different leaders can use different correct authorities
from their two quorum intersections. -/
theorem later_quorum_block_carries_all_direct_candidate_leaders
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
    {time observer : Nat}
    {candidate : ReferenceFlexCandidate BlockId}
    {carrier : ValidatorBlock BlockId}
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (candidateRounds : ∀ leader,
      leader ∈ candidate.orderedCommittedLeaders →
        leader.round = candidate.leaderRound)
    (carrierCatalog :
      (execution.trace time).blockCatalog carrier.reference.id = some carrier)
    (carrierRound : carrier.reference.round = candidate.leaderRound + 2)
    (carrierParents : carrier.HasQuorumImmediateParents config)
    (carrierParentsAccepted : ∀ parent, parent ∈ carrier.parents →
      ((execution.trace time).validatorState observer).accepted parent = true)
    (allDirectQuorums : ∀ leader,
      leader ∈ candidate.orderedCommittedLeaders →
        config.thresholds.quorum ≤
          weight config.authorityCount config.stake
            (traceDirectVoters (execution.trace time) observer
              (referenceLeaderBlockToValidatorBlockRef leader))) :
    ∀ leader, leader ∈ candidate.orderedCommittedLeaders →
      ValidatorCausalAncestor (execution.trace time)
        (referenceLeaderBlockToValidatorBlockRef leader)
        carrier.reference := by
  intro leader leaderInCandidate
  apply direct_quorum_and_later_quorum_block_carries_leader execution
    representatives observerInRange observerCorrect carrierCatalog
  · simp [carrierRound, referenceLeaderBlockToValidatorBlockRef,
      candidateRounds leader leaderInCandidate]
  · exact carrierParents
  · exact carrierParentsAccepted
  · exact allDirectQuorums leader leaderInCandidate

/-! ### Causal closure through the indirect scan -/

/-- One-host refinement for an indirect commit decision.

If the local indirect decider commits a leader by using an anchor, the exact
leader block must be in that anchor block's causal history. This is a local DAG
lookup rule. It does not assume a future block or a future successful run. -/
structure ValidatorIndirectDecisionCausalRules
    {BlockId CommitId History PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (rule : ReferenceIndirectRule BlockId History) : Prop where
  committedLeaderIsAncestorOfAnchor : ∀ anchor slot leader,
    rule.decide anchor slot = .commit leader →
      ValidatorCausalAncestor world
        (referenceLeaderBlockToValidatorBlockRef leader)
        (referenceLeaderBlockToValidatorBlockRef anchor)

/-- Causal reachability to one final anchor is closed under every exact
indirect commit decision. -/
theorem indirect_decision_causal_rules_are_anchor_closed
    {BlockId CommitId History PacketId : Type}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {rule : ReferenceIndirectRule BlockId History}
    (causalRule : ValidatorIndirectDecisionCausalRules world rule)
    (finalAnchor : LeaderBlockRef BlockId) :
    rule.CommitAnchorClosed (fun block =>
      ValidatorCausalAncestor world
        (referenceLeaderBlockToValidatorBlockRef block)
        (referenceLeaderBlockToValidatorBlockRef finalAnchor)) := by
  intro anchor slot leader anchorReachesFinal decided
  exact .trans
    (causalRule.committedLeaderIsAncestorOfAnchor anchor slot leader decided)
    anchorReachesFinal

/-- If a final selected-slot list has only causally valid committed statuses,
each exact committed leader extracted from its decision list is valid. -/
private theorem committed_leaders_from_final_decisions_are_valid
    {Digest : Type}
    {anchorOK : LeaderBlockRef Digest → Prop}
    {slots : List (ReferenceSelectedSlotView Digest)}
    {decisions :
      List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
    (valid : ReferenceSelectedSlotsCommittedAnchorsValid anchorOK slots)
    (final : finalReferenceDecisions? slots = some decisions) :
    ∀ leader, leader ∈ committedLeaderRefsFromDecisions decisions →
      anchorOK leader := by
  induction slots generalizing decisions with
  | nil =>
      have decisionsEmpty : decisions = [] := by
        simpa [finalReferenceDecisions?] using Option.some.inj final.symm
      subst decisions
      simp [committedLeaderRefsFromDecisions]
  | cons slot tail inductionHypothesis =>
      rcases valid with ⟨headValid, tailValid⟩
      rcases final_reference_decisions_cons_parts final with
        ⟨headDecision, tailDecisions, headFinal, tailFinal, decisionsShape⟩
      subst decisions
      rcases slot with ⟨selected, status⟩
      cases status with
      | undecided =>
          simp [ReferenceSelectedSlotView.finalDecision?] at headFinal
      | commit block =>
          have headDecisionShape : headDecision =
              { slot := selected, decision := .commit block } := by
            exact Option.some.inj (by simpa
              [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm)
          subst headDecision
          intro leader member
          simp only [committedLeaderRefsFromDecisions, List.filterMap_cons,
            List.mem_cons] at member
          rcases member with same | inTail
          · subst leader
            simpa [ReferenceSelectedSlotView.CommittedAnchorValid] using
              headValid
          · exact inductionHypothesis tailValid tailFinal leader inTail
      | skip =>
          have headDecisionShape : headDecision =
              { slot := selected, decision := .skip } := by
            exact Option.some.inj (by simpa
              [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm)
          subst headDecision
          intro leader member
          simp only [committedLeaderRefsFromDecisions, List.filterMap_cons]
            at member
          exact inductionHypothesis tailValid tailFinal leader member

/-- A successful candidate scan can return only committed statuses that satisfy
the invariant of its input rounds. -/
theorem found_reference_candidate_committed_leaders_are_valid
    {Digest : Type}
    {anchorOK : LeaderBlockRef Digest → Prop}
    {rounds : List (ReferenceFlexRoundView Digest)}
    {candidate : ReferenceFlexCandidate Digest}
    (valid : ReferenceFlexRoundsCommittedAnchorsValid anchorOK rounds)
    (found : findReferenceFlexCommitCandidate rounds = some candidate) :
    ∀ leader, leader ∈ candidate.orderedCommittedLeaders → anchorOK leader := by
  induction rounds generalizing candidate with
  | nil => simp [findReferenceFlexCommitCandidate] at found
  | cons round tail inductionHypothesis =>
      rcases valid with ⟨roundValid, tailValid⟩
      cases final : finalReferenceDecisions? round.selectedSlots with
      | none => simp [findReferenceFlexCommitCandidate, final] at found
      | some decisions =>
          cases hasCommit : orderedDecisionsHaveCommit decisions with
          | false =>
              have tailFound : findReferenceFlexCommitCandidate tail =
                  some candidate := by
                simpa [findReferenceFlexCommitCandidate, final, hasCommit]
                  using found
              exact inductionHypothesis tailValid tailFound
          | true =>
              have candidateShape : candidate =
                  { leaderRound := round.round
                    orderedCommittedLeaders :=
                      committedLeaderRefsFromDecisions decisions } := by
                exact Option.some.inj (by simpa
                  [findReferenceFlexCommitCandidate, final, hasCommit]
                  using found.symm)
              subst candidate
              exact committed_leaders_from_final_decisions_are_valid
                roundValid final

/-- Every candidate leader returned by the complete direct-and-indirect scan is
in the causal history of one final admissible anchor.

The premise about the post-direct list is not a liveness assumption. A trace
consumer must derive it from the concrete direct-vote blocks in the finite
favorable window. -/
theorem successful_flex_scan_candidate_leaders_reach_final_anchor
    {BlockId CommitId History Encoding PacketId : Type}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ReferenceFlexCommitterContext BlockId History}
    {input : ReferenceFlexTryCommitInput BlockId CommitId}
    {world : ValidatorWorldState BlockId CommitId PacketId}
    {output : LocalFlexCommitOutput BlockId CommitId}
    (stateValid : ReferenceFlexTryCommitStateValid context input)
    (finalAnchor : LeaderBlockRef BlockId)
    (directAnchorsReachFinal : ReferenceFlexRoundsCommittedAnchorsValid
      (fun block => ValidatorCausalAncestor world
        (referenceLeaderBlockToValidatorBlockRef block)
        (referenceLeaderBlockToValidatorBlockRef finalAnchor))
      (runReferenceDirectPass context.directRule input.pending).toRoundList)
    (indirectCausal : ValidatorIndirectDecisionCausalRules world
      context.indirectRule)
    (found : tryReferenceFlexCommitWithContext functions context input =
      some output) :
    ∀ leader, leader ∈ output.candidate.orderedCommittedLeaders →
      ValidatorCausalAncestor world
        (referenceLeaderBlockToValidatorBlockRef leader)
        (referenceLeaderBlockToValidatorBlockRef finalAnchor) := by
  let directState := runReferenceDirectPass context.directRule input.pending
  have finishedValid :=
    finish_reference_flex_rounds_at_depth_preserves_valid_anchors
      context.indirectRule
      (fun block => ValidatorCausalAncestor world
        (referenceLeaderBlockToValidatorBlockRef block)
        (referenceLeaderBlockToValidatorBlockRef finalAnchor))
      (indirect_decision_causal_rules_are_anchor_closed indirectCausal
        finalAnchor)
      context.depth directAnchorsReachFinal
  have candidateFound :=
    successful_try_reference_flex_commit_candidate_in_finished_list
      functions context input stateValid found
  exact found_reference_candidate_committed_leaders_are_valid
    finishedValid candidateFound

/-- One concrete quorum-parent block after the final anchor's voter layer
carries every leader returned by the complete direct-and-indirect scan.

The trace-level recovery proof must construct `carrier`; this theorem only
composes the local scan evidence with that concrete block. -/
theorem successful_flex_scan_candidate_leaders_reach_later_carrier
    {BlockId CommitId History Encoding PacketId : Type}
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
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ReferenceFlexCommitterContext BlockId History}
    {input : ReferenceFlexTryCommitInput BlockId CommitId}
    {time observer : Nat}
    {output : LocalFlexCommitOutput BlockId CommitId}
    {finalAnchor : LeaderBlockRef BlockId}
    {carrier : ValidatorBlock BlockId}
    (observerInRange : observer < config.authorityCount)
    (observerCorrect : faults.correctAvailable observer = true)
    (stateValid : ReferenceFlexTryCommitStateValid context input)
    (directAnchorsReachFinal : ReferenceFlexRoundsCommittedAnchorsValid
      (fun block => ValidatorCausalAncestor (execution.trace time)
        (referenceLeaderBlockToValidatorBlockRef block)
        (referenceLeaderBlockToValidatorBlockRef finalAnchor))
      (runReferenceDirectPass context.directRule input.pending).toRoundList)
    (indirectCausal : ValidatorIndirectDecisionCausalRules
      (execution.trace time) context.indirectRule)
    (found : tryReferenceFlexCommitWithContext functions context input =
      some output)
    (carrierCatalog :
      (execution.trace time).blockCatalog carrier.reference.id = some carrier)
    (carrierRound : carrier.reference.round = finalAnchor.round + 2)
    (carrierParents : carrier.HasQuorumImmediateParents config)
    (carrierParentsAccepted : ∀ parent, parent ∈ carrier.parents →
      ((execution.trace time).validatorState observer).accepted parent = true)
    (finalAnchorDirectQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters (execution.trace time) observer
          (referenceLeaderBlockToValidatorBlockRef finalAnchor))) :
    ∀ leader, leader ∈ output.candidate.orderedCommittedLeaders →
      ValidatorCausalAncestor (execution.trace time)
        (referenceLeaderBlockToValidatorBlockRef leader)
        carrier.reference := by
  have finalAnchorCarried :=
    direct_quorum_and_later_quorum_block_carries_leader execution
      representatives observerInRange observerCorrect carrierCatalog
      (by simpa [referenceLeaderBlockToValidatorBlockRef] using carrierRound)
      carrierParents carrierParentsAccepted finalAnchorDirectQuorum
  intro leader leaderInCandidate
  exact .trans
    (successful_flex_scan_candidate_leaders_reach_final_anchor stateValid
      finalAnchor directAnchorsReachFinal indirectCausal found leader
      leaderInCandidate)
    finalAnchorCarried

/-- The blocks emitted by one local commit materialization are in the causal
closure of its ordered candidate leaders.

Prior-commit and genesis roots stop the local traversal. They are not newly
committed blocks and do not occur in `sortedCommittedBlocks`. -/
def ValidatorMaterializedCandidateCausalClosure
    {BlockId CommitId PacketId : Type}
    (world : ValidatorWorldState BlockId CommitId PacketId)
    (candidate : ReferenceFlexCandidate BlockId)
    (material : ExactCommitBuildMaterial (LeaderBlockRef BlockId)) : Prop :=
  material.namedLeader ∈ material.sortedCommittedBlocks ∧
    ∀ block, block ∈ material.sortedCommittedBlocks →
      ∃ leader,
        leader ∈ candidate.orderedCommittedLeaders ∧
          ValidatorCausalAncestor world
            (referenceLeaderBlockToValidatorBlockRef block)
            (referenceLeaderBlockToValidatorBlockRef leader)

/-- One-host refinement from the local DAG materializer to exact causal
closure. This record does not contain a future carrier or a future commit. -/
structure ValidatorCommitMaterialCausalClosureSourceMap
    {BlockId CommitId History Encoding PacketId : Type}
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
    (source : LocalFlexCommitterSourceMap config functions context program) where
  returnedCandidateMaterialIsCausal : ∀ time validator candidate,
    validator < config.authorityCount →
    faults.correctAvailable validator = true →
    ReferenceFlexCandidateReturned
        (context validator
          ((timed.execution.trace time).validatorState validator))
        (source.snapshot validator
          ((timed.execution.trace time).validatorState validator)) candidate →
      ValidatorMaterializedCandidateCausalClosure
        (timed.execution.trace time) candidate
        ((source.snapshot validator
          ((timed.execution.trace time).validatorState validator)).materialize
            candidate)

/-- If one carrier contains every ordered candidate leader, it also contains
every newly committed block in that successful run's exact materialized
frontier. -/
theorem successful_flex_output_candidate_carry_gives_full_material_carry
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
      ValidatorCausalAncestor world
        (referenceLeaderBlockToValidatorBlockRef block) carrier := by
  have exactOutput := successful_try_reference_flex_commit_exact_output
    functions context input found
  intro block blockInOutput
  have blockInMaterial :
      block ∈ (input.materialize output.candidate).sortedCommittedBlocks := by
    rw [exactOutput.1] at blockInOutput
    exact blockInOutput
  rcases materialCausal.2 block blockInMaterial with
    ⟨leader, leaderInCandidate, blockBeforeLeader⟩
  exact .trans blockBeforeLeader
    (candidateCarried leader leaderInCandidate)

/-- The exact named leader is also in the carrier's causal history. -/
theorem successful_flex_output_candidate_carry_gives_named_leader_carry
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
    ValidatorCausalAncestor world
      (referenceLeaderBlockToValidatorBlockRef output.builderInput.namedLeader)
      carrier := by
  have exactOutput := successful_try_reference_flex_commit_exact_output
    functions context input found
  have namedInOutput : output.builderInput.namedLeader ∈
      output.builderInput.sortedCommittedBlocks := by
    rw [exactOutput.1]
    exact materialCausal.1
  exact successful_flex_output_candidate_carry_gives_full_material_carry
    found materialCausal candidateCarried output.builderInput.namedLeader
      namedInOutput

/-- One actual successful local FlexCommitter run and one concrete later
quorum-parent block carry the complete new commit output.

The later block is current execution evidence. This theorem does not assume
that a future block is produced. The local scan rules prove that each ordered
committed leader reaches the final anchor. Quorum intersection then connects
that anchor to the later block. The local materializer rule extends the same
path to every new materialized block and to the named leader. -/
theorem correct_exact_flex_run_and_later_quorum_block_carry_commit_output
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
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (run : CorrectExactFlexRun runtime)
    {time : Time}
    {finalAnchor : LeaderBlockRef BlockId}
    {carrier : ValidatorBlock BlockId}
    (directAnchorsReachFinal : ReferenceFlexRoundsCommittedAnchorsValid
      (fun block => ValidatorCausalAncestor (timed.execution.trace time)
        (referenceLeaderBlockToValidatorBlockRef block)
        (referenceLeaderBlockToValidatorBlockRef finalAnchor))
      (runReferenceDirectPass
        (context run.observation.validator run.observation.input).directRule
        (source.snapshot run.observation.validator
          run.observation.input).pending).toRoundList)
    (indirectCausal : ValidatorIndirectDecisionCausalRules
      (timed.execution.trace time)
      (context run.observation.validator run.observation.input).indirectRule)
    (materialCausal : ValidatorMaterializedCandidateCausalClosure
      (timed.execution.trace time) run.output.candidate
      ((source.snapshot run.observation.validator
        run.observation.input).materialize run.output.candidate))
    (carrierCatalog :
      (timed.execution.trace time).blockCatalog carrier.reference.id =
        some carrier)
    (carrierRound : carrier.reference.round = finalAnchor.round + 2)
    (carrierParents : carrier.HasQuorumImmediateParents config)
    (carrierParentsAccepted : ∀ parent, parent ∈ carrier.parents →
      ((timed.execution.trace time).validatorState
        run.observation.validator).accepted parent = true)
    (finalAnchorDirectQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters (timed.execution.trace time)
          run.observation.validator
          (referenceLeaderBlockToValidatorBlockRef finalAnchor))) :
    (∀ leader, leader ∈ run.output.candidate.orderedCommittedLeaders →
      ValidatorCausalAncestor (timed.execution.trace time)
        (referenceLeaderBlockToValidatorBlockRef leader)
        carrier.reference) ∧
    (∀ block, block ∈ run.output.builderInput.sortedCommittedBlocks →
      ValidatorCausalAncestor (timed.execution.trace time)
        (referenceLeaderBlockToValidatorBlockRef block)
        carrier.reference) ∧
    ValidatorCausalAncestor (timed.execution.trace time)
      (referenceLeaderBlockToValidatorBlockRef
        run.output.builderInput.namedLeader)
      carrier.reference := by
  have candidateCarried :=
    successful_flex_scan_candidate_leaders_reach_later_carrier
      timed.execution representatives run.validatorInRange
        run.validatorCorrect
        (source.tryCommitStateValid run.observation.validator
          run.observation.input)
        directAnchorsReachFinal indirectCausal run.exactResult carrierCatalog
        carrierRound carrierParents carrierParentsAccepted
        finalAnchorDirectQuorum
  have materialCarried :=
    successful_flex_output_candidate_carry_gives_full_material_carry
      run.exactResult materialCausal candidateCarried
  have namedLeaderCarried :=
    successful_flex_output_candidate_carry_gives_named_leader_carry
      run.exactResult materialCausal candidateCarried
  exact ⟨candidateCarried, materialCarried, namedLeaderCarried⟩

/-- The concrete round-after-voters carrier of one successful local run
remains a carrier after that run's exact local commit is installed.

The carrier exists in the run's input state. The protected record work only
advances local commit storage. Catalog monotonicity preserves the already
proved leader and material paths through that install. This theorem does not
create a post-install proposal and does not use commit synchronization. -/
theorem correct_exact_flex_run_carrier_survives_local_install
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
    {source : LocalFlexCommitterSourceMap config functions context program}
    (runtime : LocalFlexCommitterRuntime timed source)
    (representatives : ValidatorAcceptedRepresentativeRules timed.execution)
    (run : CorrectExactFlexRun runtime)
    {finalAnchor : LeaderBlockRef BlockId}
    {carrier : ValidatorBlock BlockId}
    (directAnchorsReachFinal : ReferenceFlexRoundsCommittedAnchorsValid
      (fun block => ValidatorCausalAncestor
        (timed.execution.trace run.observation.time)
        (referenceLeaderBlockToValidatorBlockRef block)
        (referenceLeaderBlockToValidatorBlockRef finalAnchor))
      (runReferenceDirectPass
        (context run.observation.validator run.observation.input).directRule
        (source.snapshot run.observation.validator
          run.observation.input).pending).toRoundList)
    (indirectCausal : ValidatorIndirectDecisionCausalRules
      (timed.execution.trace run.observation.time)
      (context run.observation.validator run.observation.input).indirectRule)
    (materialCausal : ValidatorMaterializedCandidateCausalClosure
      (timed.execution.trace run.observation.time) run.output.candidate
      ((source.snapshot run.observation.validator
        run.observation.input).materialize run.output.candidate))
    (carrierCatalog :
      (timed.execution.trace run.observation.time).blockCatalog
          carrier.reference.id = some carrier)
    (carrierRound : carrier.reference.round = finalAnchor.round + 2)
    (carrierParents : carrier.HasQuorumImmediateParents config)
    (carrierParentsAccepted : ∀ parent, parent ∈ carrier.parents →
      ((timed.execution.trace run.observation.time).validatorState
        run.observation.validator).accepted parent = true)
    (finalAnchorDirectQuorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake
        (traceDirectVoters (timed.execution.trace run.observation.time)
          run.observation.validator
          (referenceLeaderBlockToValidatorBlockRef finalAnchor))) :
    ∃ recordAt,
      run.observation.time + 1 ≤ recordAt ∧
        recordAt ≤ run.observation.time + 1 + timed.localActionBound ∧
        ((timed.execution.trace (recordAt + 1)).validatorState
          run.observation.validator).installedCommitAt
            run.output.reference.index = some run.output.reference.digest ∧
        ((timed.execution.trace (recordAt + 1)).validatorState
          run.observation.validator).commitInstallSourceAt
            run.output.reference.index = some .localExecution ∧
        (timed.execution.trace (recordAt + 1)).blockCatalog
            carrier.reference.id = some carrier ∧
        (∀ leader, leader ∈ run.output.candidate.orderedCommittedLeaders →
          ValidatorCausalAncestor (timed.execution.trace (recordAt + 1))
            (referenceLeaderBlockToValidatorBlockRef leader)
            carrier.reference) ∧
        (∀ block, block ∈ run.output.builderInput.sortedCommittedBlocks →
          ValidatorCausalAncestor (timed.execution.trace (recordAt + 1))
            (referenceLeaderBlockToValidatorBlockRef block)
            carrier.reference) ∧
        ValidatorCausalAncestor (timed.execution.trace (recordAt + 1))
          (referenceLeaderBlockToValidatorBlockRef
            run.output.builderInput.namedLeader)
          carrier.reference := by
  have carried :=
    correct_exact_flex_run_and_later_quorum_block_carry_commit_output
      representatives run directAnchorsReachFinal indirectCausal materialCausal
        carrierCatalog carrierRound carrierParents carrierParentsAccepted
          finalAnchorDirectQuorum
  rcases successful_local_flex_run_completes_and_persists_exact runtime
      run.observation run.returned run.successful run.validatorInRange
        run.validatorCorrect with
    ⟨recordAt, _exactResult, runBeforeRecord, recordWithinBound, installed,
      localSource⟩
  have observationBeforeInstalled : run.observation.time ≤ recordAt + 1 := by
    exact Nat.le_trans (Nat.le_add_right _ 1)
      (Nat.le_trans runBeforeRecord (Nat.le_add_right _ 1))
  have catalogMonotone := timed.execution.blockCatalogMonotone
    run.observation.time (recordAt + 1) observationBeforeInstalled
  refine ⟨recordAt, runBeforeRecord, recordWithinBound, installed, localSource,
    catalogMonotone _ _ carrierCatalog, ?_, ?_, ?_⟩
  · intro leader leaderMember
    exact ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
      (carried.1 leader leaderMember)
  · intro block blockMember
    exact ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
      (carried.2.1 block blockMember)
  · exact ValidatorCausalAncestor.of_catalog_monotone catalogMonotone
      carried.2.2

end Mysticeti
