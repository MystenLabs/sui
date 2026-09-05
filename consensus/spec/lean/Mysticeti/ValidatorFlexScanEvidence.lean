/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ExactCommitPrefixSafety
import Mysticeti.ValidatorFlexPendingRefresh

namespace Mysticeti

/-!
Exact vote evidence reconstructed for one successful FlexCommitter result.

This module is a local, past-state bridge. It does not state that a later run,
proposal, packet, delivery, or commit occurs.
-/

/-- One indirect commit made by the deterministic high-to-low reconstruction.

The witness names only a history that the exact reconstructed scan uses. The
`here`
constructor records the concrete undecided slot, the first usable higher
anchor, and the exact indirect decision. The `later` constructor moves the
witness through lower rounds. Thus, this relation cannot name an arbitrary
anchor history. -/
inductive ValidatorFlexUsedIndirectCommit
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) (depth : Nat) :
    List (ReferenceFlexRoundView Digest) →
      History → LeaderBlockRef Digest → Prop where
  | here {round tail slot anchor leader} :
      slot ∈ round.selectedSlots →
      slot.status = .undecided →
      scanReferenceAnchorAtOrAbove (round.round + depth)
          (finishReferenceFlexRoundsAtDepth rule depth tail) = .found anchor →
      rule.decide anchor slot.slot = .commit leader →
      ValidatorFlexUsedIndirectCommit rule depth (round :: tail)
        (rule.historyOf anchor) leader
  | later {round tail history leader} :
      ValidatorFlexUsedIndirectCommit rule depth tail history leader →
      ValidatorFlexUsedIndirectCommit rule depth (round :: tail) history leader

/-- One committed leader has either one direct quorum or one exact indirect
certificate from an anchor history that the exact scan used. -/
def ValidatorFlexCommittedLeaderOrigin
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (commitVotes : LeaderBlockRef Digest → VoterSet)
    (exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds)
    (usedIndirect : History → LeaderBlockRef Digest → Prop)
    (leader : LeaderBlockRef Digest) : Prop :=
  thresholds.quorum ≤
      weight authorityCount stake (commitVotes leader) ∨
    ∃ history,
      usedIndirect history leader ∧
        thresholds.certificate ≤ weight authorityCount stake
          ((exactAnchorHistory history).certificateVotes leader)

/-- One exact voter in the direct quorum or in the indirect certificate that
supports a committed leader. -/
inductive ValidatorFlexCommittedLeaderVoterOrigin
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (commitVotes : LeaderBlockRef Digest → VoterSet)
    (exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds)
    (usedIndirect : History → LeaderBlockRef Digest → Prop)
    (leader : LeaderBlockRef Digest) (voter : Nat) : Prop where
  | direct
      (quorum : thresholds.quorum ≤
        weight authorityCount stake (commitVotes leader))
      (selected : commitVotes leader voter = true) :
      ValidatorFlexCommittedLeaderVoterOrigin thresholds commitVotes
        exactAnchorHistory usedIndirect leader voter
  | indirect (history : History)
      (used : usedIndirect history leader)
      (certificate : thresholds.certificate ≤
        weight authorityCount stake
          ((exactAnchorHistory history).certificateVotes leader))
      (selected :
        (exactAnchorHistory history).certificateVotes leader voter = true) :
      ValidatorFlexCommittedLeaderVoterOrigin thresholds commitVotes
        exactAnchorHistory usedIndirect leader voter

/-- Enlarging the exact set of used indirect histories preserves one origin. -/
private theorem committed_leader_origin_mono
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {leftUsed rightUsed : History → LeaderBlockRef Digest → Prop}
    (included : ∀ history leader, leftUsed history leader →
      rightUsed history leader)
    {leader : LeaderBlockRef Digest}
    (origin : ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
      exactAnchorHistory leftUsed leader) :
    ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
      exactAnchorHistory rightUsed leader := by
  rcases origin with direct | ⟨history, used, certificate⟩
  · exact Or.inl direct
  · exact Or.inr ⟨history, included history leader used, certificate⟩

/-- Every committed status in one selected-slot list has an exact vote origin. -/
def ReferenceSelectedSlotsCommittedOrigins
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (commitVotes : LeaderBlockRef Digest → VoterSet)
    (exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds)
    (usedIndirect : History → LeaderBlockRef Digest → Prop)
    (slots : List (ReferenceSelectedSlotView Digest)) : Prop :=
  ∀ slot, slot ∈ slots → ∀ leader,
    slot.status = .commit leader →
      ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
        exactAnchorHistory usedIndirect leader

/-- Every committed status in one ordered round list has an exact vote origin. -/
def ReferenceFlexRoundsCommittedOrigins
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    (thresholds : Thresholds authorityCount stake)
    (commitVotes : LeaderBlockRef Digest → VoterSet)
    (exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds)
    (usedIndirect : History → LeaderBlockRef Digest → Prop)
    (rounds : List (ReferenceFlexRoundView Digest)) : Prop :=
  ∀ round, round ∈ rounds →
    ReferenceSelectedSlotsCommittedOrigins thresholds commitVotes
      exactAnchorHistory usedIndirect round.selectedSlots

/-- Exact direct-status validity supplies the direct branch of the origin. -/
private theorem direct_status_valid_gives_committed_origin
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {usedIndirect : History → LeaderBlockRef Digest → Prop}
    {slot : ExactSelectedLeaderSlot}
    {status : ReferenceSlotStatus Digest}
    (valid : ExactDirectStatusValid thresholds commitVotes skipVotes slot
      status) :
    ∀ leader, status = .commit leader →
      ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
        exactAnchorHistory usedIndirect leader := by
  intro leader committed
  cases status with
  | undecided => simp at committed
  | commit block =>
      have same : block = leader := by
        exact ReferenceSlotStatus.commit.inj committed
      subst block
      exact Or.inl valid.2
  | skip => simp at committed

/-- One valid indirect commit supplies the certificate branch of the origin. -/
private theorem indirect_status_valid_gives_committed_origin
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {usedIndirect : History → LeaderBlockRef Digest → Prop}
    {history : History} {slot : ExactSelectedLeaderSlot}
    {status : ReferenceSlotStatus Digest}
    (valid : ExactIndirectStatusValid (exactAnchorHistory history) slot
      status) :
    ∀ leader, usedIndirect history leader →
      status = .commit leader →
      ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
        exactAnchorHistory usedIndirect leader := by
  intro leader used committed
  cases status with
  | undecided => exact False.elim valid
  | commit block =>
      have same : block = leader := by
        exact ReferenceSlotStatus.commit.inj committed
      subst block
      exact Or.inr ⟨history, used, valid.2.selectedCertified⟩
  | skip => simp at committed

/-- Direct first-decision evidence gives every committed basis status an
origin. -/
private theorem first_decision_basis_has_committed_origins
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {anchorOK : LeaderBlockRef Digest → Prop}
    {rule : ReferenceIndirectRule Digest History}
    {depth : Nat}
    {directBasis current : List (ReferenceFlexRoundView Digest)}
    (provenance : ExactFlexFirstDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule depth directBasis current) :
    ReferenceFlexRoundsCommittedOrigins thresholds commitVotes
      exactAnchorHistory (fun _ _ => False) directBasis := by
  intro round roundMember slot slotMember leader committed
  have valid := provenance.directStatusValid round roundMember slot slotMember
  exact direct_status_valid_gives_committed_origin valid leader committed

/-- Finishing one selected slot preserves its direct origin or creates the
exact certificate origin returned by the indirect rule. -/
private theorem finish_selected_slot_preserves_committed_origin
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    {anchor : ReferenceAnchorScanResult Digest}
    (anchorValid : ∀ block, anchor = .found block → anchorOK block)
    (indirectValid : ∀ block, anchorOK block → ∀ slot,
      ExactIndirectStatusValid
        (exactAnchorHistory (rule.historyOf block)) slot
        (rule.decide block slot))
    {usedIndirect : History → LeaderBlockRef Digest → Prop}
    {view : ReferenceSelectedSlotView Digest}
    (indirectUsed : ∀ block leader,
      anchor = .found block →
      view.status = .undecided →
      rule.decide block view.slot = .commit leader →
      usedIndirect (rule.historyOf block) leader)
    (origin : ∀ leader, view.status = .commit leader →
      ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
        exactAnchorHistory usedIndirect leader) :
    ∀ leader,
      (finishReferenceSelectedSlot rule anchor view).status = .commit leader →
        ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
          exactAnchorHistory usedIndirect leader := by
  rcases view with ⟨slot, status⟩
  cases status with
  | undecided =>
      cases anchor with
      | blocked => simp [finishReferenceSelectedSlot]
      | noAnchor => simp [finishReferenceSelectedSlot]
      | found block =>
          intro leader committed
          have blockValid := anchorValid block rfl
          have valid := indirectValid block blockValid slot
          have decided : rule.decide block slot = .commit leader := by
            simpa [finishReferenceSelectedSlot] using committed
          have used := indirectUsed block leader rfl rfl decided
          exact indirect_status_valid_gives_committed_origin valid leader used
            decided
  | commit block =>
      intro leader committed
      exact origin leader (by simpa [finishReferenceSelectedSlot] using committed)
  | skip => simp [finishReferenceSelectedSlot]

/-- Finishing one complete selected-slot list preserves exact origins. -/
private theorem finish_selected_slots_preserves_committed_origins
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    {anchor : ReferenceAnchorScanResult Digest}
    (anchorValid : ∀ block, anchor = .found block → anchorOK block)
    (indirectValid : ∀ block, anchorOK block → ∀ slot,
      ExactIndirectStatusValid
        (exactAnchorHistory (rule.historyOf block)) slot
        (rule.decide block slot))
    {usedIndirect : History → LeaderBlockRef Digest → Prop}
    {slots : List (ReferenceSelectedSlotView Digest)}
    (indirectUsed : ∀ view, view ∈ slots → ∀ block leader,
      anchor = .found block →
      view.status = .undecided →
      rule.decide block view.slot = .commit leader →
      usedIndirect (rule.historyOf block) leader)
    (origins : ReferenceSelectedSlotsCommittedOrigins thresholds commitVotes
      exactAnchorHistory usedIndirect slots) :
    ReferenceSelectedSlotsCommittedOrigins thresholds commitVotes
      exactAnchorHistory usedIndirect
        (finishReferenceSelectedSlots rule anchor slots) := by
  intro finishedSlot finishedMember leader committed
  simp only [finishReferenceSelectedSlots, List.mem_map] at finishedMember
  rcases finishedMember with ⟨sourceSlot, sourceMember, finishedShape⟩
  subst finishedSlot
  exact finish_selected_slot_preserves_committed_origin rule anchorOK
    anchorValid indirectValid
      (indirectUsed sourceSlot sourceMember)
      (origins sourceSlot sourceMember) leader committed

/-- The high-to-low indirect finisher preserves a direct origin or records the
exact certificate origin for every newly committed status. -/
private theorem finish_rounds_preserves_committed_origins
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    (rule : ReferenceIndirectRule Digest History)
    (anchorOK : LeaderBlockRef Digest → Prop)
    (closed : rule.CommitAnchorClosed anchorOK)
    (indirectValid : ∀ anchor, anchorOK anchor → ∀ slot,
      ExactIndirectStatusValid
        (exactAnchorHistory (rule.historyOf anchor)) slot
        (rule.decide anchor slot))
    (depth : Nat)
    {rounds : List (ReferenceFlexRoundView Digest)}
    (anchors : ReferenceFlexRoundsCommittedAnchorsValid anchorOK rounds)
    (origins : ReferenceFlexRoundsCommittedOrigins thresholds commitVotes
      exactAnchorHistory (fun _ _ => False) rounds) :
    ReferenceFlexRoundsCommittedOrigins thresholds commitVotes
      exactAnchorHistory (ValidatorFlexUsedIndirectCommit rule depth rounds)
        (finishReferenceFlexRoundsAtDepth rule depth rounds) := by
  induction rounds with
  | nil =>
      intro round member
      simp [finishReferenceFlexRoundsAtDepth] at member
  | cons round tail inductionHypothesis =>
      rcases anchors with ⟨roundAnchors, tailAnchors⟩
      have tailOrigins : ReferenceFlexRoundsCommittedOrigins thresholds
          commitVotes exactAnchorHistory (fun _ _ => False) tail := by
        intro tailRound tailMember
        exact origins tailRound (List.mem_cons_of_mem round tailMember)
      have finishedTailOrigins := inductionHypothesis tailAnchors tailOrigins
      have liftedFinishedTailOrigins : ReferenceFlexRoundsCommittedOrigins
          thresholds commitVotes exactAnchorHistory
          (ValidatorFlexUsedIndirectCommit rule depth (round :: tail))
          (finishReferenceFlexRoundsAtDepth rule depth tail) := by
        intro tailRound tailMember slot slotMember leader committed
        exact committed_leader_origin_mono
          (leftUsed := ValidatorFlexUsedIndirectCommit rule depth tail)
          (rightUsed := ValidatorFlexUsedIndirectCommit rule depth
            (round :: tail))
          (fun _ _ used => ValidatorFlexUsedIndirectCommit.later used)
          (finishedTailOrigins tailRound tailMember slot slotMember leader
            committed)
      have finishedTailAnchors :=
        finish_reference_flex_rounds_at_depth_preserves_valid_anchors rule
          anchorOK closed depth tailAnchors
      let finishedTail := finishReferenceFlexRoundsAtDepth rule depth tail
      let selectedAnchor := scanReferenceAnchorAtOrAbove
        (round.round + depth) finishedTail
      have selectedAnchorValid : ∀ block,
          selectedAnchor = .found block → anchorOK block := by
        intro block found
        exact scan_reference_anchor_at_or_above_found_anchor_valid
          finishedTailAnchors found
      have roundOrigins : ReferenceSelectedSlotsCommittedOrigins thresholds
          commitVotes exactAnchorHistory
          (ValidatorFlexUsedIndirectCommit rule depth (round :: tail))
          round.selectedSlots := by
        intro slot slotMember leader committed
        exact committed_leader_origin_mono
          (leftUsed := fun _ _ => False)
          (rightUsed := ValidatorFlexUsedIndirectCommit rule depth
            (round :: tail))
          (fun _ _ impossible => False.elim impossible)
          (origins round (by simp) slot slotMember leader committed)
      have finishedRoundOrigins :=
        finish_selected_slots_preserves_committed_origins rule anchorOK
          selectedAnchorValid indirectValid
          (fun view viewMember block leader found undecided decided => by
            apply ValidatorFlexUsedIndirectCommit.here viewMember undecided
            · simpa [selectedAnchor, finishedTail] using found
            · exact decided)
          roundOrigins
      intro finishedRound member
      simp only [finishReferenceFlexRoundsAtDepth, List.mem_cons] at member
      rcases member with same | inTail
      · subst finishedRound
        simpa [finishReferenceFlexRoundAtDepth, finishedTail, selectedAnchor]
          using finishedRoundOrigins
      · exact liftedFinishedTailOrigins finishedRound inTail

/-- Every exact committed reference extracted from one final decision list has
the origin carried by its source selected-slot status. -/
private theorem committed_references_from_final_decisions_have_origins
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {usedIndirect : History → LeaderBlockRef Digest → Prop}
    {slots : List (ReferenceSelectedSlotView Digest)}
    {decisions : List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
    (origins : ReferenceSelectedSlotsCommittedOrigins thresholds commitVotes
      exactAnchorHistory usedIndirect slots)
    (final : finalReferenceDecisions? slots = some decisions) :
    ∀ leader, leader ∈ committedLeaderRefsFromDecisions decisions →
      ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
        exactAnchorHistory usedIndirect leader := by
  induction slots generalizing decisions with
  | nil =>
      have decisionsEmpty : decisions = [] := by
        simpa [finalReferenceDecisions?] using Option.some.inj final.symm
      subst decisions
      simp [committedLeaderRefsFromDecisions]
  | cons slot tail inductionHypothesis =>
      rcases final_reference_decisions_cons_parts final with
        ⟨headDecision, tailDecisions, headFinal, tailFinal, decisionsShape⟩
      subst decisions
      rcases slot with ⟨selected, status⟩
      cases status with
      | undecided =>
          simp [ReferenceSelectedSlotView.finalDecision?] at headFinal
      | commit block =>
          have headShape : headDecision =
              { slot := selected, decision := .commit block } := by
            exact Option.some.inj (by simpa
              [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm)
          subst headDecision
          intro leader member
          simp only [committedLeaderRefsFromDecisions, List.filterMap_cons,
            List.mem_cons] at member
          rcases member with same | tailMember
          · subst leader
            exact origins
              { slot := selected, status := .commit block } (by simp) block rfl
          · apply inductionHypothesis
              (fun tailSlot tailMember' leader' committed =>
                origins tailSlot (by simp [tailMember']) leader' committed)
              tailFinal leader tailMember
      | skip =>
          have headShape : headDecision =
              { slot := selected, decision := .skip } := by
            exact Option.some.inj (by simpa
              [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm)
          subst headDecision
          intro leader member
          simp only [committedLeaderRefsFromDecisions, List.filterMap_cons]
            at member
          apply inductionHypothesis
              (fun tailSlot tailMember leader' committed =>
                origins tailSlot (by simp [tailMember]) leader' committed)
            tailFinal leader member

/-- A candidate found in a list contains one committed leader whose status has
the supplied per-round origin property. -/
private theorem found_candidate_has_committed_origin
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {usedIndirect : History → LeaderBlockRef Digest → Prop}
    {rounds : List (ReferenceFlexRoundView Digest)}
    {candidate : ReferenceFlexCandidate Digest}
    (origins : ReferenceFlexRoundsCommittedOrigins thresholds commitVotes
      exactAnchorHistory usedIndirect rounds)
    (found : findReferenceFlexCommitCandidate rounds = some candidate) :
    ∃ leader,
      leader ∈ candidate.orderedCommittedLeaders ∧
        ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
          exactAnchorHistory usedIndirect leader := by
  induction rounds generalizing candidate with
  | nil => simp [findReferenceFlexCommitCandidate] at found
  | cons round tail inductionHypothesis =>
      cases final : finalReferenceDecisions? round.selectedSlots with
      | none => simp [findReferenceFlexCommitCandidate, final] at found
      | some decisions =>
          cases hasCommit : orderedDecisionsHaveCommit decisions with
          | false =>
              have tailFound : findReferenceFlexCommitCandidate tail =
                  some candidate := by
                simpa [findReferenceFlexCommitCandidate, final, hasCommit]
                  using found
              have tailOrigins : ReferenceFlexRoundsCommittedOrigins
                  thresholds commitVotes exactAnchorHistory usedIndirect tail := by
                intro tailRound tailMember
                exact origins tailRound (List.mem_cons_of_mem round tailMember)
              exact inductionHypothesis tailOrigins tailFound
          | true =>
              have candidateShape : candidate =
                  { leaderRound := round.round
                    orderedCommittedLeaders :=
                      committedLeaderRefsFromDecisions decisions } := by
                exact Option.some.inj (by simpa
                  [findReferenceFlexCommitCandidate, final, hasCommit]
                  using found.symm)
              subst candidate
              have nonempty : committedLeaderRefsFromDecisions decisions ≠ [] := by
                clear final found origins inductionHypothesis
                induction decisions with
                | nil => simp [orderedDecisionsHaveCommit] at hasCommit
                | cons decision tail ih =>
                    cases decision with
                    | mk selected decision =>
                        cases decision with
                        | commit block =>
                            simp [committedLeaderRefsFromDecisions]
                        | skip =>
                            simp only [orderedDecisionsHaveCommit] at hasCommit
                            simp only [committedLeaderRefsFromDecisions,
                              List.filterMap_cons]
                            exact ih hasCommit
              rcases List.exists_mem_of_ne_nil _ nonempty with ⟨leader, member⟩
              have roundOrigins := origins round (by simp)
              have leaderOrigin :=
                committed_references_from_final_decisions_have_origins
                  roundOrigins final leader member
              exact ⟨leader, member, leaderOrigin⟩

/-- A successful scan with sticky first-decision provenance exposes one
committed leader and the exact direct-quorum or indirect-certificate origin of
that status. -/
theorem successful_first_decision_scan_has_committed_leader_origin
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {anchorOK : LeaderBlockRef Digest → Prop}
    {rule : ReferenceIndirectRule Digest History}
    {depth : Nat}
    {directBasis current : List (ReferenceFlexRoundView Digest)}
    {candidate : ReferenceFlexCandidate Digest}
    (provenance : ExactFlexFirstDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule depth directBasis current)
    (closed : rule.CommitAnchorClosed anchorOK)
    (indirectValid : ∀ anchor, anchorOK anchor → ∀ slot,
      ExactIndirectStatusValid
        (exactAnchorHistory (rule.historyOf anchor)) slot
        (rule.decide anchor slot))
    (found : findReferenceFlexCommitCandidate
      (finishReferenceFlexRoundsAtDepth rule depth current) = some candidate) :
    ∃ leader,
      leader ∈ candidate.orderedCommittedLeaders ∧
        ValidatorFlexCommittedLeaderOrigin thresholds commitVotes
          exactAnchorHistory
            (ValidatorFlexUsedIndirectCommit rule depth directBasis) leader := by
  have basisOrigins := first_decision_basis_has_committed_origins provenance
  have basisAnchors := provenance.directAnchorsValid
  have finishedBasisOrigins := finish_rounds_preserves_committed_origins rule
    anchorOK closed indirectValid depth basisAnchors basisOrigins
  have replay := provenance.replay
  rw [replay] at finishedBasisOrigins
  exact found_candidate_has_committed_origin finishedBasisOrigins found

/-- A voter set whose threshold exceeds all Byzantine and unavailable stake
contains one correct, available selected voter. -/
theorem threshold_voter_set_has_correct_available_member
    {CommitId : Type} {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config)
    (threshold : Nat) (voters : VoterSet)
    (aboveNonProgress :
      config.thresholds.fault + faults.unavailableStakeBound < threshold)
    (reached : threshold ≤
      weight config.authorityCount config.stake voters) :
    ∃ author,
      author < config.authorityCount ∧
        faults.correctAvailable author = true ∧ voters author = true := by
  have overlapBound :
      weight config.authorityCount config.stake
          (VoterSet.inter voters faults.nonProgress) ≤
        config.thresholds.fault + faults.unavailableStakeBound := by
    exact Nat.le_trans
      (weight_mono config.stake
        (VoterSet.inter_subset_right config.authorityCount voters
          faults.nonProgress))
      faults.non_progress_stake_bounded
  have partition := weight_diff_add_inter config.authorityCount config.stake
    voters faults.nonProgress
  have usefulPositive : 0 <
      weight config.authorityCount config.stake
        (VoterSet.diff voters faults.nonProgress) := by
    omega
  rcases positive_weight_has_member usefulPositive with
    ⟨author, authorInRange, selected, _positiveStake⟩
  have selectedAndCorrect : voters author = true ∧
      faults.nonProgress author = false := by
    simpa [VoterSet.diff] using selected
  have correctAvailable : faults.correctAvailable author = true := by
    simp [FixedFaultInterval.correctAvailable, VoterSet.diff, VoterSet.full,
      selectedAndCorrect.2]
  exact ⟨author, authorInRange, correctAvailable, selectedAndCorrect.1⟩

/-- The configured direct quorum exceeds all non-progress stake. -/
theorem direct_quorum_exceeds_non_progress_stake
    {CommitId : Type} {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config) :
    config.thresholds.fault + faults.unavailableStakeBound <
      config.thresholds.quorum := by
  have preserves := config.thresholds.quorum_preserves_certificate
  have certificatePositive := config.thresholds.certificate_positive
  have budgetsFit := faults.faultBudgetsFit
  have quorumDefinition := faults.quorumDefinition
  omega

/-- The configured indirect certificate also exceeds all non-progress stake. -/
theorem indirect_certificate_exceeds_non_progress_stake
    {CommitId : Type} {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config) :
    config.thresholds.fault + faults.unavailableStakeBound <
      config.thresholds.certificate := by
  have totalShape : totalWeight config.authorityCount config.stake =
      config.thresholds.quorum +
        (config.thresholds.fault + faults.unavailableStakeBound) := by
    rw [faults.quorumDefinition]
    exact (Nat.sub_add_cancel faults.faultBudgetsFit).symm
  have intersects := config.thresholds.quorum_certificate_intersection
  rw [totalShape] at intersects
  omega

/-- A direct quorum contains one correct, available voter. -/
theorem direct_quorum_has_correct_available_member
    {CommitId : Type} {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config) (voters : VoterSet)
    (quorum : config.thresholds.quorum ≤
      weight config.authorityCount config.stake voters) :
    ∃ author,
      author < config.authorityCount ∧
        faults.correctAvailable author = true ∧ voters author = true := by
  exact threshold_voter_set_has_correct_available_member faults
    config.thresholds.quorum voters
      (direct_quorum_exceeds_non_progress_stake faults) quorum

/-- An indirect certificate contains one correct, available voter. -/
theorem indirect_certificate_has_correct_available_member
    {CommitId : Type} {config : ValidatorEpochConfig CommitId}
    (faults : FixedFaultInterval config) (voters : VoterSet)
    (certificate : config.thresholds.certificate ≤
      weight config.authorityCount config.stake voters) :
    ∃ author,
      author < config.authorityCount ∧
        faults.correctAvailable author = true ∧ voters author = true := by
  exact threshold_voter_set_has_correct_available_member faults
    config.thresholds.certificate voters
      (indirect_certificate_exceeds_non_progress_stake faults) certificate

/-! ### Exact candidate-round shape -/

/-- One current status with exact first-decision provenance names its selected
slot. -/
private theorem flex_slot_provenance_current_commit_matches_slot
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {anchorOK : LeaderBlockRef Digest → Prop}
    {rule : ReferenceIndirectRule Digest History}
    {minimumRound : Nat}
    {higher : List (ReferenceFlexRoundView Digest)}
    {directBasis current : ReferenceSelectedSlotView Digest}
    (provenance : ExactFlexSlotDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule minimumRound higher
      directBasis current) :
    ∀ leader, current.status = .commit leader →
      leader.AtSelectedSlot current.slot := by
  intro leader committed
  cases provenance with
  | direct sameSlot sameStatus directValid _ =>
      have basisCommitted : directBasis.status = .commit leader :=
        sameStatus.trans committed
      rw [basisCommitted] at directValid
      simpa only [sameSlot] using directValid.1
  | indirect _ _ _ _ _ _ currentStatus _ indirectValid =>
      have originCommitted : _ = ReferenceSlotStatus.commit leader :=
        currentStatus.symm.trans committed
      rw [originCommitted] at indirectValid
      exact indirectValid.1

/-- Every current committed status in one exact provenance list names its
selected slot. -/
private theorem flex_slot_provenance_list_current_commits_match_slots
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {anchorOK : LeaderBlockRef Digest → Prop}
    {rule : ReferenceIndirectRule Digest History}
    {minimumRound : Nat}
    {higher : List (ReferenceFlexRoundView Digest)}
    {directBasis current : List (ReferenceSelectedSlotView Digest)}
    (provenance : ExactListAgreement
      (ExactFlexSlotDecisionProvenance thresholds commitVotes skipVotes
        exactAnchorHistory anchorOK rule minimumRound higher)
      directBasis current) :
    ∀ slot, slot ∈ current → ∀ leader,
      slot.status = .commit leader → leader.AtSelectedSlot slot.slot := by
  induction provenance with
  | nil => simp
  | @cons directHead currentHead directTail currentTail head tail ih =>
      intro slot member leader committed
      rcases List.mem_cons.mp member with same | tailMember
      · subst slot
        exact flex_slot_provenance_current_commit_matches_slot head leader
          committed
      · exact ih slot tailMember leader committed

/-- Every committed status in the current list of a first-decision provenance
names its selected slot. -/
private theorem first_decision_current_commits_match_slots
    {Digest History : Type}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    {exactAnchorHistory : History →
      LeaderAnchorHistory Digest authorityCount stake thresholds}
    {anchorOK : LeaderBlockRef Digest → Prop}
    {rule : ReferenceIndirectRule Digest History}
    {depth : Nat}
    {directBasis current : List (ReferenceFlexRoundView Digest)}
    (provenance : ExactFlexFirstDecisionProvenance thresholds commitVotes
      skipVotes exactAnchorHistory anchorOK rule depth directBasis current) :
    ∀ round, round ∈ current → ∀ slot,
      slot ∈ round.selectedSlots → ∀ leader,
        slot.status = .commit leader → leader.AtSelectedSlot slot.slot := by
  induction provenance with
  | nil => simp
  | cons tail _ slots ih =>
      intro round roundMember
      rcases List.mem_cons.mp roundMember with same | tailMember
      · subst round
        exact flex_slot_provenance_list_current_commits_match_slots slots
      · exact ih round tailMember

/-- The direct pass preserves the exact selected-slot round keys. -/
private theorem direct_pass_preserves_selected_slot_rounds
    {Digest : Type} (rule : ReferenceDirectRule Digest)
    {state : IndexedReferenceFlexState Digest}
    (roundKeys : state.SelectedSlotRoundsMatch) :
    (runReferenceDirectPass rule state).SelectedSlotRoundsMatch := by
  intro index indexInRange slot slotMember
  change index < state.roundCount at indexInRange
  change slot ∈
    (runReferenceDirectRound rule (state.rounds index)).selectedSlots at slotMember
  change slot.slot.round = (state.rounds index).round
  simp only [runReferenceDirectRound, List.mem_map] at slotMember
  rcases slotMember with ⟨sourceSlot, sourceMember, slotShape⟩
  subst slot
  rcases sourceSlot with ⟨selected, status⟩
  have sourceRoundKey := roundKeys index indexInRange
    { slot := selected, status := status } sourceMember
  cases status <;> simpa [applyReferenceDirectDecision] using sourceRoundKey

/-- Selected-slot round keys in one indexed state also hold in its exact
finite list projection. -/
private theorem indexed_state_list_selected_slot_rounds_match
    {Digest : Type} {state : IndexedReferenceFlexState Digest}
    (roundKeys : state.SelectedSlotRoundsMatch) :
    ∀ round, round ∈ state.toRoundList → ∀ slot,
      slot ∈ round.selectedSlots → slot.slot.round = round.round := by
  have fromRange : ∀ start fuel,
      start + fuel ≤ state.roundCount →
      ∀ round,
        round ∈ indexedReferenceRoundsFrom state.rounds start fuel →
        ∀ slot, slot ∈ round.selectedSlots →
          slot.slot.round = round.round := by
    intro start fuel
    induction fuel generalizing start with
    | zero => simp [indexedReferenceRoundsFrom]
    | succ remaining inductionHypothesis =>
        intro inRange round roundMember slot slotMember
        simp only [indexedReferenceRoundsFrom, List.mem_cons] at roundMember
        rcases roundMember with same | tailMember
        · subst round
          exact roundKeys start (by omega) slot slotMember
        · exact inductionHypothesis (start + 1) (by omega) round tailMember
            slot slotMember
  exact fromRange 0 state.roundCount (by omega)

/-- Finishing the exact high-to-low indirect pass preserves the fact that each
committed leader names the round that contains its selected slot. -/
private theorem finish_flex_rounds_commits_match_own_round
    {Digest History : Type}
    (rule : ReferenceIndirectRule Digest History) (depth : Nat)
    (commitMatchesSlot : ∀ anchor slot block,
      rule.decide anchor slot = .commit block → block.AtSelectedSlot slot)
    {rounds : List (ReferenceFlexRoundView Digest)}
    (existingCommits : ∀ round, round ∈ rounds → ∀ slot,
      slot ∈ round.selectedSlots → ∀ leader,
        slot.status = .commit leader → leader.round = round.round)
    (slotRounds : ∀ round, round ∈ rounds → ∀ slot,
      slot ∈ round.selectedSlots → slot.slot.round = round.round) :
    ∀ round, round ∈ finishReferenceFlexRoundsAtDepth rule depth rounds →
      ∀ slot, slot ∈ round.selectedSlots → ∀ leader,
        slot.status = .commit leader → leader.round = round.round := by
  induction rounds with
  | nil => simp [finishReferenceFlexRoundsAtDepth]
  | cons sourceRound tail inductionHypothesis =>
      let finishedTail := finishReferenceFlexRoundsAtDepth rule depth tail
      let anchor := scanReferenceAnchorAtOrAbove
        (sourceRound.round + depth) finishedTail
      have tailCommits := inductionHypothesis
        (fun round member => existingCommits round (by simp [member]))
        (fun round member => slotRounds round (by simp [member]))
      intro round roundMember
      simp only [finishReferenceFlexRoundsAtDepth, List.mem_cons] at roundMember
      rcases roundMember with same | tailMember
      · subst round
        intro slot slotMember leader committed
        change leader.round = sourceRound.round
        change slot ∈ finishReferenceSelectedSlots rule anchor
          sourceRound.selectedSlots at slotMember
        simp only [finishReferenceSelectedSlots, List.mem_map] at slotMember
        rcases slotMember with ⟨sourceSlot, sourceMember, slotShape⟩
        subst slot
        rcases sourceSlot with ⟨selected, status⟩
        cases status with
        | undecided =>
            cases anchorShape : anchor with
            | blocked =>
                simp [finishReferenceSelectedSlot, anchorShape] at committed
            | noAnchor =>
                simp [finishReferenceSelectedSlot, anchorShape] at committed
            | found anchorBlock =>
                have decided : rule.decide anchorBlock selected =
                    .commit leader := by
                  simpa [finishReferenceSelectedSlot, anchorShape] using committed
                exact (commitMatchesSlot anchorBlock selected leader decided).1.trans
                  (slotRounds sourceRound (by simp)
                    { slot := selected, status := .undecided } sourceMember)
        | commit block =>
            have sameBlock : block = leader := by
              exact ReferenceSlotStatus.commit.inj (by
                simpa [finishReferenceSelectedSlot] using committed)
            subst block
            exact existingCommits sourceRound (by simp)
              { slot := selected, status := .commit leader } sourceMember leader rfl
        | skip => simp [finishReferenceSelectedSlot] at committed
      · exact tailCommits round tailMember

/-- A candidate found in a round list contains only leaders at its exact
candidate round. -/
private theorem found_candidate_committed_leaders_match_round
    {Digest : Type}
    {rounds : List (ReferenceFlexRoundView Digest)}
    {candidate : ReferenceFlexCandidate Digest}
    (valid : ∀ round, round ∈ rounds → ∀ slot,
      slot ∈ round.selectedSlots → ∀ leader,
        slot.status = .commit leader → leader.round = round.round)
    (found : findReferenceFlexCommitCandidate rounds = some candidate) :
    ∀ leader, leader ∈ candidate.orderedCommittedLeaders →
      leader.round = candidate.leaderRound := by
  have decisionsMatch : ∀ {slots : List (ReferenceSelectedSlotView Digest)}
      {decisions : List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
      {targetRound : Nat},
      (∀ slot, slot ∈ slots → ∀ leader,
        slot.status = .commit leader → leader.round = targetRound) →
      finalReferenceDecisions? slots = some decisions →
      ∀ leader, leader ∈ committedLeaderRefsFromDecisions decisions →
        leader.round = targetRound := by
    intro slots decisions targetRound roundValid final
    induction slots generalizing decisions with
    | nil =>
        have decisionsEmpty : decisions = [] := by
          simpa [finalReferenceDecisions?] using Option.some.inj final.symm
        subst decisions
        simp [committedLeaderRefsFromDecisions]
    | cons slot tail inductionHypothesis =>
        rcases final_reference_decisions_cons_parts final with
          ⟨headDecision, tailDecisions, headFinal, tailFinal, decisionsShape⟩
        subst decisions
        rcases slot with ⟨selected, status⟩
        cases status with
        | undecided =>
            simp [ReferenceSelectedSlotView.finalDecision?] at headFinal
        | commit block =>
            have headShape : headDecision =
                { slot := selected, decision := .commit block } := by
              exact Option.some.inj (by simpa
                [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm)
            subst headDecision
            intro leader member
            simp only [committedLeaderRefsFromDecisions, List.filterMap_cons,
              List.mem_cons] at member
            rcases member with same | tailMember
            · subst leader
              exact roundValid
                { slot := selected, status := .commit block } (by simp) block rfl
            · exact inductionHypothesis
                (fun tailSlot tailMember' leader' committed =>
                  roundValid tailSlot (by simp [tailMember']) leader' committed)
                tailFinal leader tailMember
        | skip =>
            have headShape : headDecision =
                { slot := selected, decision := .skip } := by
              exact Option.some.inj (by simpa
                [ReferenceSelectedSlotView.finalDecision?] using headFinal.symm)
            subst headDecision
            intro leader member
            simp only [committedLeaderRefsFromDecisions, List.filterMap_cons]
              at member
            exact inductionHypothesis
              (fun tailSlot tailMember leader' committed =>
                roundValid tailSlot (by simp [tailMember]) leader' committed)
              tailFinal leader member
  induction rounds generalizing candidate with
  | nil => simp [findReferenceFlexCommitCandidate] at found
  | cons round tail inductionHypothesis =>
      cases final : finalReferenceDecisions? round.selectedSlots with
      | none => simp [findReferenceFlexCommitCandidate, final] at found
      | some decisions =>
          cases hasCommit : orderedDecisionsHaveCommit decisions with
          | false =>
              have tailFound : findReferenceFlexCommitCandidate tail =
                  some candidate := by
                simpa [findReferenceFlexCommitCandidate, final, hasCommit]
                  using found
              exact inductionHypothesis
                (fun tailRound tailMember => valid tailRound (by simp [tailMember]))
                tailFound
          | true =>
              have candidateShape : candidate =
                  { leaderRound := round.round
                    orderedCommittedLeaders :=
                      committedLeaderRefsFromDecisions decisions } := by
                exact Option.some.inj (by simpa
                  [findReferenceFlexCommitCandidate, final, hasCommit]
                  using found.symm)
              subst candidate
              exact decisionsMatch (valid round (by simp)) final

/-- Every committed leader in one actual successful exact result names the
candidate round selected by that result. -/
theorem correct_exact_flex_run_committed_leaders_match_candidate_round
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (run : CorrectExactFlexRun runtime) :
    ∀ leader, leader ∈ run.output.candidate.orderedCommittedLeaders →
      leader.round = run.output.candidate.leaderRound := by
  let validator := run.observation.validator
  let state := run.observation.input
  let localContext := context validator state
  let input := source.snapshot validator state
  let directState := runReferenceDirectPass localContext.directRule input.pending
  let current := directState.toRoundList
  have provenance := authenticated.firstDecisionProvenance validator state
    run.validatorInRange run.validatorCorrect
  have directSlotRounds : directState.SelectedSlotRoundsMatch :=
    direct_pass_preserves_selected_slot_rounds localContext.directRule
      (source.tryCommitStateValid validator state).selectedSlotRoundsMatch
  have currentSlotRounds : ∀ round, round ∈ current → ∀ slot,
      slot ∈ round.selectedSlots → slot.slot.round = round.round :=
    indexed_state_list_selected_slot_rounds_match directSlotRounds
  have currentCommitsAtSlots : ∀ round, round ∈ current → ∀ slot,
      slot ∈ round.selectedSlots → ∀ leader,
        slot.status = .commit leader → leader.AtSelectedSlot slot.slot :=
    first_decision_current_commits_match_slots provenance
  have currentCommitsMatchRound : ∀ round, round ∈ current → ∀ slot,
      slot ∈ round.selectedSlots → ∀ leader,
        slot.status = .commit leader → leader.round = round.round := by
    intro round roundMember slot slotMember leader committed
    exact (currentCommitsAtSlots round roundMember slot slotMember leader
      committed).1.trans (currentSlotRounds round roundMember slot slotMember)
  have finishedCommitsMatchRound :=
    finish_flex_rounds_commits_match_own_round localContext.indirectRule
      localContext.depth localContext.indirectCommitMatchesSlot
      currentCommitsMatchRound currentSlotRounds
  have found := successful_try_reference_flex_commit_candidate_in_finished_list
    functions localContext input (source.tryCommitStateValid validator state)
      run.exactResult
  exact found_candidate_committed_leaders_match_round
    finishedCommitsMatchRound found

/-- One actual successful correct run exposes a committed leader and the exact
direct-quorum or indirect-certificate origin in the reconstructed scan that
returns the same result. -/
theorem correct_exact_flex_run_has_committed_leader_origin
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (run : CorrectExactFlexRun runtime) :
    ∃ leader,
      leader ∈ run.output.candidate.orderedCommittedLeaders ∧
        ValidatorFlexCommittedLeaderOrigin config.thresholds
          (authenticated.commitVotes run.observation.validator
            run.observation.input)
          authenticated.exactAnchorHistory
          (ValidatorFlexUsedIndirectCommit
            (context run.observation.validator
              run.observation.input).indirectRule
            (context run.observation.validator run.observation.input).depth
            (authenticated.firstDecisionBase run.observation.validator
              run.observation.input)) leader := by
  let validator := run.observation.validator
  let state := run.observation.input
  let localContext := context validator state
  let input := source.snapshot validator state
  have provenance := authenticated.firstDecisionProvenance validator state
    run.validatorInRange run.validatorCorrect
  have closed : localContext.indirectRule.CommitAnchorClosed
      authenticated.admissibleAnchor := by
    intro anchor slot leader anchorValid decided
    exact authenticated.indirectCommitAnchorIsAdmissible validator state
      run.validatorInRange run.validatorCorrect anchor anchorValid slot leader
        decided
  have indirectValid : ∀ anchor, authenticated.admissibleAnchor anchor →
      ∀ slot,
        ExactIndirectStatusValid
          (authenticated.exactAnchorHistory
            (localContext.indirectRule.historyOf anchor)) slot
          (localContext.indirectRule.decide anchor slot) := by
    intro anchor anchorValid slot
    exact authenticated.admissibleIndirectStatusValid validator state
      run.validatorInRange run.validatorCorrect anchor anchorValid slot
  have found := successful_try_reference_flex_commit_candidate_in_finished_list
    functions localContext input (source.tryCommitStateValid validator state)
      run.exactResult
  exact successful_first_decision_scan_has_committed_leader_origin provenance
    closed indirectValid found

/-- One actual successful correct run exposes one exact correct, available
voter from the direct quorum or indirect certificate that supports a returned
committed leader. -/
theorem correct_exact_flex_run_has_correct_available_voter_origin
    {BlockId CommitId History Encoding PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (run : CorrectExactFlexRun runtime) :
    ∃ leader voter,
      leader ∈ run.output.candidate.orderedCommittedLeaders ∧
        voter < config.authorityCount ∧
        faults.correctAvailable voter = true ∧
        ValidatorFlexCommittedLeaderVoterOrigin config.thresholds
          (authenticated.commitVotes run.observation.validator
            run.observation.input)
          authenticated.exactAnchorHistory
          (ValidatorFlexUsedIndirectCommit
            (context run.observation.validator
              run.observation.input).indirectRule
            (context run.observation.validator run.observation.input).depth
            (authenticated.firstDecisionBase run.observation.validator
              run.observation.input)) leader voter := by
  rcases correct_exact_flex_run_has_committed_leader_origin authenticated run
      with ⟨leader, leaderMember, direct | indirect⟩
  · rcases direct_quorum_has_correct_available_member faults _ direct with
      ⟨voter, voterInRange, voterCorrect, selected⟩
    exact ⟨leader, voter, leaderMember, voterInRange, voterCorrect,
      .direct direct selected⟩
  · rcases indirect with ⟨history, used, certificate⟩
    rcases indirect_certificate_has_correct_available_member faults _
        certificate with
      ⟨voter, voterInRange, voterCorrect, selected⟩
    exact ⟨leader, voter, leaderMember, voterInRange, voterCorrect,
      .indirect history used certificate selected⟩

/-- One exact accepted child body realizes one counted vote for a leader. -/
structure ValidatorFlexExactVoteChild
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {initial : ValidatorFlexInitialDagSupport timed}
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    {highestAcceptedRound : Nat}
    (support : ValidatorFlexRunDagSupport initial observation
      highestAcceptedRound)
    (leader : LeaderBlockRef BlockId) (voter : Nat)
    (reference : ValidatorBlockRef BlockId) : Prop where
  member : reference ∈ support.references
  author : reference.author = voter
  round : reference.round = leader.round + 1
  leaderIsParent : referenceLeaderBlockToValidatorBlockRef leader ∈
    (support.body reference).parents

/-- Catalog entries persist through one finite atomic-event prefix. -/
private theorem validator_world_step_catalog_persists_for_flex_evidence
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {time : Time}
    {before after : ValidatorWorldState BlockId CommitId PacketId}
    {events : List (ValidatorAtomicEvent BlockId CommitId PacketId)}
    (step : ValidatorWorldStep config faults protocolPacket program time before
      events after)
    {blockId : BlockId} {block : ValidatorBlock BlockId}
    (present : before.blockCatalog blockId = some block) :
    after.blockCatalog blockId = some block := by
  induction step with
  | nil => exact present
  | cons firstStep remainingSteps inductionHypothesis =>
      exact inductionHypothesis
        ((validator_atomic_step_history_monotone firstStep).1 blockId block
          present)

/-- Past-state mapping from the exact vote sets that reconstruct one actual
successful result to concrete accepted child bodies in that run's finite DAG
support.

The indirect field is restricted by `ValidatorFlexUsedIndirectCommit`. It does
not range over arbitrary histories or anchors. Neither field states that a
future packet, delivery, run, or commit occurs. -/
structure ValidatorFlexScanEvidenceSourceMap
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source) where
  directCommitVoteHasExactChild : ∀ (run : CorrectExactFlexRun runtime)
      leader voter,
    voter < config.authorityCount →
    authenticated.commitVotes run.observation.validator run.observation.input
        leader voter = true →
    ∃ reference,
      ValidatorFlexExactVoteChild
        (mapping.dagSupport run.observation run.occurs)
        leader voter reference
  usedIndirectCertificateVoteHasExactChild :
    ∀ (run : CorrectExactFlexRun runtime) history leader voter,
    ValidatorFlexUsedIndirectCommit
        (context run.observation.validator
          run.observation.input).indirectRule
        (context run.observation.validator run.observation.input).depth
        (authenticated.firstDecisionBase run.observation.validator
          run.observation.input) history leader →
    voter < config.authorityCount →
    (authenticated.exactAnchorHistory history).certificateVotes leader voter =
        true →
    ∃ reference,
      ValidatorFlexExactVoteChild
        (mapping.dagSupport run.observation run.occurs)
        leader voter reference

/-- One actual successful run exposes a correct, available concrete vote child
for one committed leader in its reconstructed returned candidate. -/
theorem correct_exact_flex_run_has_correct_available_vote_child
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (authenticated : AuthenticatedFlexVoteSourceMap faults functions context
      source)
    (evidence : ValidatorFlexScanEvidenceSourceMap mapping authenticated)
    (run : CorrectExactFlexRun runtime) :
    ∃ leader voter reference,
      leader ∈ run.output.candidate.orderedCommittedLeaders ∧
        voter < config.authorityCount ∧
        faults.correctAvailable voter = true ∧
        ValidatorFlexExactVoteChild
          (mapping.dagSupport run.observation run.occurs)
          leader voter reference := by
  rcases correct_exact_flex_run_has_correct_available_voter_origin
      authenticated run with
    ⟨leader, voter, leaderMember, voterInRange, voterCorrect, origin⟩
  cases origin with
  | direct _ selected =>
      rcases evidence.directCommitVoteHasExactChild run leader voter
          voterInRange selected with ⟨reference, child⟩
      exact ⟨leader, voter, reference, leaderMember, voterInRange,
        voterCorrect, child⟩
  | indirect history used _ selected =>
      rcases evidence.usedIndirectCertificateVoteHasExactChild run history
          leader voter used voterInRange selected with ⟨reference, child⟩
      exact ⟨leader, voter, reference, leaderMember, voterInRange,
        voterCorrect, child⟩

/-- The concrete correct vote child has a bounded initial author origin or one
exact earlier, possibly same-batch, author persistence action. -/
theorem correct_exact_flex_run_vote_child_has_past_author_origin
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (authenticatedVotes : AuthenticatedFlexVoteSourceMap faults functions
      context source)
    (authenticatedBodies : ValidatorFlexAuthenticatedBodySourceMap mapping)
    (evidence : ValidatorFlexScanEvidenceSourceMap mapping authenticatedVotes)
    (run : CorrectExactFlexRun runtime) :
    ∃ leader voter reference,
      leader ∈ run.output.candidate.orderedCommittedLeaders ∧
        voter < config.authorityCount ∧
        faults.correctAvailable voter = true ∧
        ValidatorFlexExactVoteChild
          (mapping.dagSupport run.observation run.occurs)
          leader voter reference ∧
        ValidatorFlexCorrectAuthorOrigin timed initial run.observation
          reference := by
  rcases correct_exact_flex_run_has_correct_available_vote_child mapping
      authenticatedVotes evidence run with
    ⟨leader, voter, reference, leaderMember, voterInRange, voterCorrect,
      child⟩
  let occurs := run.occurs
  rcases occurs with
    ⟨beforeEvents, afterEvents, actionBefore, actionAfter, runPrefix⟩
  have referenceAuthorInRange : reference.author < config.authorityCount := by
    rw [child.author]
    exact voterInRange
  have referenceAuthorCorrect :
      faults.correctAvailable reference.author = true := by
    rw [child.author]
    exact voterCorrect
  have origin :=
    ValidatorFlexPendingRefreshSourceMap.correct_support_body_has_past_author_origin
      mapping
    authenticatedBodies run.occurs runPrefix child.member
      referenceAuthorInRange referenceAuthorCorrect
  exact ⟨leader, voter, reference, leaderMember, voterInRange, voterCorrect,
    child, origin⟩

/-- Above the finite initial author bound, one reconstructed correct vote child
is the exact body of an earlier, possibly same-batch, author persistence.

The proof compares both bodies at the run's next trace state. Existing catalog
entries cannot be replaced, so equal authenticated references select one exact
body. -/
theorem correct_flex_vote_child_above_initial_bound_has_exact_persistence
    {BlockId CommitId History Encoding PacketId ScheduleKey : Type}
    [DecidableEq ScheduleKey]
    {config : ValidatorEpochConfig CommitId}
    {functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding}
    {context : ValidatorFlexContextAt BlockId CommitId History}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {source : LocalFlexCommitterSourceMap config functions context program}
    {runtime : LocalFlexCommitterRuntime timed source}
    {initial : ValidatorFlexInitialDagSupport timed}
    {schedule : ValidatorFlexPendingSchedule CommitId ScheduleKey}
    (effects : ValidatorExactExecutionEffects faults protocolPacket network
      program timed.execution)
    (mapping : ValidatorFlexPendingRefreshSourceMap source runtime initial
      schedule)
    (authenticatedBodies : ValidatorFlexAuthenticatedBodySourceMap mapping)
    {run : CorrectExactFlexRun runtime}
    {leader : LeaderBlockRef BlockId} {voter : Nat}
    {reference : ValidatorBlockRef BlockId}
    (voterInRange : voter < config.authorityCount)
    (voterCorrect : faults.correctAvailable voter = true)
    (child : ValidatorFlexExactVoteChild
      (mapping.dagSupport run.observation run.occurs)
      leader voter reference)
    (aboveInitial : initial.roundBound voter < reference.round) :
    ValidatorActionBeforeFlexRun timed run.observation reference.author
      (.persistProposal
        ((mapping.dagSupport run.observation run.occurs).body reference)) := by
  let occurs := run.occurs
  rcases occurs with
    ⟨beforeEvents, afterEvents, actionBefore, actionAfter, runPrefix⟩
  have referenceAuthorInRange : reference.author < config.authorityCount := by
    rw [child.author]
    exact voterInRange
  have referenceAuthorCorrect :
      faults.correctAvailable reference.author = true := by
    rw [child.author]
    exact voterCorrect
  have origin :=
    ValidatorFlexPendingRefreshSourceMap.correct_support_body_has_past_author_origin
      mapping authenticatedBodies run.occurs runPrefix child.member
        referenceAuthorInRange referenceAuthorCorrect
  rcases origin with initialOrigin | ⟨block, blockReference, persistence⟩
  · have withinBound := initial.ownReferenceWithinBound reference.author
      reference initialOrigin.1
    have withinVoterBound :
        reference.round ≤ initial.roundBound voter := by
      simpa only [child.author] using withinBound
    exact False.elim ((Nat.not_lt_of_ge withinVoterBound) aboveInitial)
  · let support := mapping.dagSupport run.observation run.occurs
    have supportCatalogAtAction :
        actionBefore.blockCatalog reference.id = some (support.body reference) :=
      support.bodyIsCataloguedBeforeRun reference beforeEvents afterEvents
        actionBefore actionAfter child.member runPrefix
    rcases runPrefix with
      ⟨eventSplit, prefixStep, runStep, suffixStep, inputIsActionBefore⟩
    have supportCatalogAfterRun :
        actionAfter.blockCatalog reference.id = some (support.body reference) :=
      (validator_atomic_step_history_monotone runStep).1 reference.id
        (support.body reference) supportCatalogAtAction
    have supportCatalogAtNext :
        (timed.execution.trace (run.observation.time + 1)).blockCatalog
            reference.id = some (support.body reference) :=
      validator_world_step_catalog_persists_for_flex_evidence suffixStep
        supportCatalogAfterRun
    have exactPersistedBody : block = support.body reference := by
      have persistedCatalogAtNext :
          (timed.execution.trace (run.observation.time + 1)).blockCatalog
              reference.id = some block := by
        rcases persistence with earlier | sameBatch
        · rcases earlier with ⟨persistTime, persistBeforeRun, persistOccurs⟩
          have stored := effects.persistedProposalStoresBlock persistTime
            reference.author block persistOccurs
          rw [blockReference] at stored
          have persistBeforeNext :
              persistTime + 1 ≤ run.observation.time + 1 :=
            Nat.succ_le_succ (Nat.le_of_lt persistBeforeRun)
          exact timed.execution.blockCatalogMonotone (persistTime + 1)
            (run.observation.time + 1) persistBeforeNext reference.id block
              stored
        · rcases sameBatch with
            ⟨sameBefore, sameAfter, sameActionBefore, sameActionAfter,
              samePrefix, persistOccurs⟩
          rcases samePrefix with
            ⟨sameEventSplit, _samePrefixStep, _sameRunStep,
              _sameSuffixStep, _sameInput⟩
          have fullOccurs : ValidatorLocalActionOccurs
              (timed.execution.events run.observation.time) reference.author
                (.persistProposal block) := by
            rcases persistOccurs with ⟨earlierEvents, laterEvents, shape⟩
            refine ⟨earlierEvents,
              laterEvents ++
                (.localAction run.observation.validator .runCommitter ::
                  sameAfter), ?_⟩
            rw [sameEventSplit, shape]
            simp [List.append_assoc]
          have stored := effects.persistedProposalStoresBlock
            run.observation.time reference.author block fullOccurs
          simpa only [blockReference] using stored
      exact Option.some.inj (persistedCatalogAtNext.symm.trans
        supportCatalogAtNext)
    subst block
    exact persistence

/-- A correct author's persistence above its signer floor at one cutoff cannot
be an earlier action. The conclusion returns the concrete main-trace action
time, including a persistence earlier in the run's own event batch. -/
theorem flex_persistence_above_time_floor_occurs_after
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    (cutoff : Time)
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    {author : Nat} {block : ValidatorBlock BlockId}
    (authorInRange : author < config.authorityCount)
    (aboveCutoffFloor :
      ((timed.execution.trace cutoff).validatorState
        author).highestSignedRound < block.reference.round)
    (persistence : ValidatorActionBeforeFlexRun timed observation author
      (.persistProposal block)) :
    ∃ persistTime,
      cutoff ≤ persistTime ∧ persistTime ≤ observation.time ∧
        ValidatorLocalActionOccurs (timed.execution.events persistTime) author
          (.persistProposal block) := by
  rcases persistence with earlier | sameBatch
  · rcases earlier with ⟨persistTime, persistBeforeRun, persistOccurs⟩
    by_cases afterCutoff : cutoff ≤ persistTime
    · exact ⟨persistTime, afterCutoff, Nat.le_of_lt persistBeforeRun,
        persistOccurs⟩
    · have persistBeforeCutoff : persistTime < cutoff :=
        Nat.lt_of_not_ge afterCutoff
      have stored := persist_proposal_occurrence_stores_own_block
        timed.execution persistOccurs
      have persistNextBeforeCutoff : persistTime + 1 ≤ cutoff := by
        exact Nat.add_one_le_iff.mpr persistBeforeCutoff
      have storedAtCutoff :=
        (timed.execution.durableStateMonotone author (persistTime + 1)
          cutoff authorInRange persistNextBeforeCutoff).own_block_persists
            stored
      have roundAtMostFloor :=
        (timed.execution.statesWellFormed cutoff author authorInRange)
          |>.ownBlockDoesNotExceedSignerFloor block.reference.round
            block.reference storedAtCutoff
      exact False.elim (by omega)
  · rcases sameBatch with
      ⟨beforeEvents, afterEvents, actionBefore, actionAfter, runPrefix,
        persistOccurs⟩
    rcases runPrefix with
      ⟨eventSplit, _prefixStep, _runStep, _suffixStep, _input⟩
    rcases persistOccurs with ⟨earlierEvents, laterEvents, shape⟩
    have fullOccurs : ValidatorLocalActionOccurs
        (timed.execution.events observation.time) author
          (.persistProposal block) := by
      refine ⟨earlierEvents,
        laterEvents ++ (.localAction observation.validator .runCommitter ::
          afterEvents), ?_⟩
      rw [eventSplit, shape]
      simp [List.append_assoc]
    have runAfterCutoff : cutoff ≤ observation.time := by
      by_cases afterCutoff : cutoff ≤ observation.time
      · exact afterCutoff
      · have runBeforeCutoff : observation.time < cutoff :=
          Nat.lt_of_not_ge afterCutoff
        have stored := persist_proposal_occurrence_stores_own_block
          timed.execution fullOccurs
        have runNextBeforeCutoff : observation.time + 1 ≤ cutoff := by
          exact Nat.add_one_le_iff.mpr runBeforeCutoff
        have storedAtCutoff :=
          (timed.execution.durableStateMonotone author (observation.time + 1)
            cutoff authorInRange runNextBeforeCutoff).own_block_persists stored
        have roundAtMostFloor :=
          (timed.execution.statesWellFormed cutoff author authorInRange)
            |>.ownBlockDoesNotExceedSignerFloor block.reference.round
              block.reference storedAtCutoff
        exact False.elim (by omega)
    exact ⟨observation.time, runAfterCutoff, Nat.le_refl _, fullOccurs⟩

/-- The GST specialization of the signer-floor persistence cutoff. -/
theorem flex_persistence_above_gst_floor_occurs_after_gst
    {BlockId CommitId PacketId : Type}
    {config : ValidatorEpochConfig CommitId}
    {faults : FixedFaultInterval config}
    {protocolPacket :
      AddressedPacket (ValidatorMessage BlockId CommitId) → Prop}
    {network : AddressedPartialSynchrony config faults protocolPacket}
    {program : ValidatorExecutionProgram BlockId CommitId}
    {timed : ValidatorBoundedExecution (PacketId := PacketId) config faults
      protocolPacket network program}
    {observation : LocalFlexCommitterRunObservation BlockId CommitId}
    {author : Nat} {block : ValidatorBlock BlockId}
    (authorInRange : author < config.authorityCount)
    (aboveGstFloor :
      ((timed.execution.trace network.gst).validatorState
        author).highestSignedRound < block.reference.round)
    (persistence : ValidatorActionBeforeFlexRun timed observation author
      (.persistProposal block)) :
    ∃ persistTime,
      network.gst ≤ persistTime ∧ persistTime ≤ observation.time ∧
        ValidatorLocalActionOccurs (timed.execution.events persistTime) author
          (.persistProposal block) := by
  exact flex_persistence_above_time_floor_occurs_after network.gst
    authorInRange aboveGstFloor persistence

end Mysticeti
