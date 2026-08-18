/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ReferenceFlexCommitter
import Mysticeti.RecursiveFlexAgreement

namespace Mysticeti

/-!
Exact commit agreement for one favorable recovery window.

This is a restricted single-target result. It applies when the target round is
the first unresolved pending round, or when all earlier pending rounds are
already final. It does not prove that one target at `R` and one anchor at
`R + d` can close an arbitrary earlier undecided prefix.

The worst-case Rust scan needs `d + 1` consecutive usable anchor rounds. The
last anchor also needs its next-round voting layer. `CommonCommitStep` uses the
separate full-scan result for that case.

This module does not assume general agreement for all selected leader slots.
Instead, both validators select one exact directly committed first-slot anchor.
Each validator has the complete causal data of that anchor. The indirect rule
uses only that causal data. Extra local blocks cannot change its result.

The final composition keeps any earlier direct result only when the local
commit-rule safety proof shows that it matches the result from the common
anchor. `ExactAnchorEvidenceMap.safeDirectResults` derives this fact from the
direct and indirect quorum rules. Rust source mapping remains separate.
-/

/-- The decision data obtained by walking the complete causal closure of one
exact anchor. `certifiedAt slot` contains exactly the target blocks that receive
certificate stake in that causal closure. -/
structure ExactAnchorCausalData (Digest : Type) where
  anchor : LeaderBlockRef Digest
  certifiedAt : ExactSelectedLeaderSlot → List (LeaderBlockRef Digest)
  certifiedNodup : ∀ slot, (certifiedAt slot).Nodup
  certifiedMatchesSlot : ∀ slot block,
    block ∈ certifiedAt slot →
      block.round = slot.round ∧ block.author = slot.validator

/-- Convert the exact certified block set for one slot to the Rust indirect
result. Exactly one certified block commits. Zero or multiple blocks skip. -/
def exactIndirectDecisionFromCertificates
    {Digest : Type} [DecidableEq Digest]
    (certified : List (LeaderBlockRef Digest)) :
    ExactSelectedSlotDecision (LeaderBlockRef Digest) :=
  match certified with
  | [block] => .commit block
  | _ => .skip

/-- The indirect result determined by complete causal data. -/
def canonicalExactIndirectDecision
    {Digest : Type} [DecidableEq Digest]
    (data : ExactAnchorCausalData Digest)
    (slot : ExactSelectedLeaderSlot) :
    ExactSelectedSlotDecision (LeaderBlockRef Digest) :=
  exactIndirectDecisionFromCertificates (data.certifiedAt slot)

/-- The target blocks that one local view will count after it walks the anchor.
The filter models the Rust intersection between locally known target blocks and
blocks that receive certificate stake from the anchor's causal closure. -/
def localCertifiedBlocks
    {Digest : Type} [DecidableEq Digest]
    (data : ExactAnchorCausalData Digest)
    (availableAt :
      ExactSelectedLeaderSlot → LeaderBlockRef Digest → Bool)
    (slot : ExactSelectedLeaderSlot) : List (LeaderBlockRef Digest) :=
  (data.certifiedAt slot).filter fun block => availableAt slot block

/-- The local indirect result. Local storage can contain more blocks than the
anchor causal closure. -/
def localExactIndirectDecision
    {Digest : Type} [DecidableEq Digest]
    (data : ExactAnchorCausalData Digest)
    (availableAt :
      ExactSelectedLeaderSlot → LeaderBlockRef Digest → Bool)
    (slot : ExactSelectedLeaderSlot) :
    ExactSelectedSlotDecision (LeaderBlockRef Digest) :=
  exactIndirectDecisionFromCertificates
    (localCertifiedBlocks data availableAt slot)

/-- A complete local view contains every certified target block reached from
the exact anchor. It can contain any additional blocks. -/
def CompleteExactAnchorView
    {Digest : Type} [DecidableEq Digest]
    (data : ExactAnchorCausalData Digest)
    (availableAt :
      ExactSelectedLeaderSlot → LeaderBlockRef Digest → Bool) :
    Prop :=
  ∀ slot block, block ∈ data.certifiedAt slot → availableAt slot block = true

/-- Extra local blocks do not change the set counted by the indirect rule when
the local view contains the complete exact-anchor causal data. -/
theorem local_certified_blocks_equal_canonical
    {Digest : Type} [DecidableEq Digest]
    {data : ExactAnchorCausalData Digest}
    {availableAt :
      ExactSelectedLeaderSlot → LeaderBlockRef Digest → Bool}
    (complete : CompleteExactAnchorView data availableAt)
    (slot : ExactSelectedLeaderSlot) :
    localCertifiedBlocks data availableAt slot = data.certifiedAt slot := by
  unfold localCertifiedBlocks
  apply List.filter_eq_self.2
  intro block included
  exact complete slot block included

/-- Complete local views of the same exact anchor produce the canonical
indirect result, even when their extra local blocks differ. -/
theorem complete_exact_anchor_view_has_canonical_indirect_result
    {Digest : Type} [DecidableEq Digest]
    {data : ExactAnchorCausalData Digest}
    {availableAt :
      ExactSelectedLeaderSlot → LeaderBlockRef Digest → Bool}
    (complete : CompleteExactAnchorView data availableAt)
    (slot : ExactSelectedLeaderSlot) :
    localExactIndirectDecision data availableAt slot =
      canonicalExactIndirectDecision data slot := by
  unfold localExactIndirectDecision canonicalExactIndirectDecision
  rw [local_certified_blocks_equal_canonical complete slot]

/-- A one-validator mapping from a complete local anchor closure to canonical
causal data. No field compares two validators. -/
structure ExactAnchorCausalDataMap
    (LocalView Digest : Type) [DecidableEq Digest] where
  localData : LocalView → LeaderBlockRef Digest → ExactAnchorCausalData Digest
  canonicalData : LeaderBlockRef Digest → ExactAnchorCausalData Digest
  availableAt :
    LocalView → ExactSelectedLeaderSlot → LeaderBlockRef Digest → Bool
  complete : LocalView → LeaderBlockRef Digest → Prop
  canonicalDataNamesAnchor : ∀ anchor,
    (canonicalData anchor).anchor = anchor
  completeMapsToCanonical : ∀ view anchor,
    complete view anchor → localData view anchor = canonicalData anchor
  completeContainsCertifiedBlocks : ∀ view anchor,
    complete view anchor →
      CompleteExactAnchorView (canonicalData anchor) (availableAt view)

namespace ExactAnchorCausalDataMap

/-- A complete local closure for one exact anchor produces its canonical
indirect result. -/
theorem complete_view_has_canonical_indirect_result
    {LocalView Digest : Type} [DecidableEq Digest]
    (mapping : ExactAnchorCausalDataMap LocalView Digest)
    {view : LocalView} {anchor : LeaderBlockRef Digest}
    (complete : mapping.complete view anchor)
    (slot : ExactSelectedLeaderSlot) :
    localExactIndirectDecision (mapping.localData view anchor)
        (mapping.availableAt view) slot =
      canonicalExactIndirectDecision (mapping.canonicalData anchor) slot := by
  rw [mapping.completeMapsToCanonical view anchor complete]
  exact complete_exact_anchor_view_has_canonical_indirect_result
    (mapping.completeContainsCertifiedBlocks view anchor complete) slot

/-- Two complete local closures for one exact anchor give identical indirect
results. Their additional local blocks can differ. -/
theorem two_complete_views_have_same_indirect_result
    {LocalView Digest : Type} [DecidableEq Digest]
    (mapping : ExactAnchorCausalDataMap LocalView Digest)
    {left right : LocalView} {anchor : LeaderBlockRef Digest}
    (leftComplete : mapping.complete left anchor)
    (rightComplete : mapping.complete right anchor)
    (slot : ExactSelectedLeaderSlot) :
    localExactIndirectDecision (mapping.localData left anchor)
        (mapping.availableAt left) slot =
      localExactIndirectDecision (mapping.localData right anchor)
        (mapping.availableAt right) slot := by
  rw [mapping.complete_view_has_canonical_indirect_result leftComplete slot]
  rw [mapping.complete_view_has_canonical_indirect_result rightComplete slot]

end ExactAnchorCausalDataMap

/-- Build selected-slot views from one common ordered slot list and local
statuses. -/
def selectedSlotViewsForOrder
    {Digest : Type}
    (order : List ExactSelectedLeaderSlot)
    (statusAt : ExactSelectedLeaderSlot → ReferenceSlotStatus Digest) :
    List (ReferenceSelectedSlotView Digest) :=
  order.map fun slot => { slot, status := statusAt slot }

/-- One directly committed first slot followed by any local statuses. Both
validators use the same `tailOrder`, but their tail statuses can differ. -/
def favorableAnchorSelectedSlots
    {Digest : Type}
    (firstSlot : ExactSelectedLeaderSlot)
    (tailOrder : List ExactSelectedLeaderSlot)
    (anchor : LeaderBlockRef Digest)
    (tailStatus : ExactSelectedLeaderSlot → ReferenceSlotStatus Digest) :
    List (ReferenceSelectedSlotView Digest) :=
  { slot := firstSlot, status := .commit anchor } ::
    selectedSlotViewsForOrder tailOrder tailStatus

/-- The exact selected-slot order of a favorable anchor round does not depend
on local tail statuses. -/
theorem favorable_anchor_selected_slot_order
    {Digest : Type}
    (firstSlot : ExactSelectedLeaderSlot)
    (tailOrder : List ExactSelectedLeaderSlot)
    (anchor : LeaderBlockRef Digest)
    (tailStatus : ExactSelectedLeaderSlot → ReferenceSlotStatus Digest) :
    (favorableAnchorSelectedSlots firstSlot tailOrder anchor tailStatus).map
        ReferenceSelectedSlotView.slot =
      firstSlot :: tailOrder := by
  simp only [favorableAnchorSelectedSlots, selectedSlotViewsForOrder,
    List.map_cons, List.map_map]
  congr 1
  induction tailOrder with
  | nil => rfl
  | cons slot tail ih =>
      simp only [List.map_cons, Function.comp_apply]
      rw [ih]

/-- A directly committed first selected slot makes the exact anchor independent
of all later local statuses. -/
theorem favorable_first_slot_selects_exact_anchor
    {Digest : Type}
    (firstSlot : ExactSelectedLeaderSlot)
    (tailOrder : List ExactSelectedLeaderSlot)
    (anchor : LeaderBlockRef Digest)
    (tailStatus : ExactSelectedLeaderSlot → ReferenceSlotStatus Digest) :
    scanReferenceSelectedSlots
        (favorableAnchorSelectedSlots firstSlot tailOrder anchor tailStatus) =
      .found anchor := by
  rfl

/-- A cached direct result for one slot. `none` means that the indirect rule
must supply the final result. -/
abbrev OptionalExactDirectResult (Digest : Type) :=
  Option (ExactSelectedSlotDecision (LeaderBlockRef Digest))

/-- Local direct results that are safe to keep when the common exact anchor is
used. This is a one-validator commit-rule safety condition. It does not compare
validators or assert a future quorum. -/
structure AnchorCompatibleDirectResults
    {Digest : Type}
    (slots : List ExactSelectedLeaderSlot)
    (anchorDecisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest)) where
  directAt : ExactSelectedLeaderSlot → OptionalExactDirectResult Digest
  decidedMatchesAnchor : ∀ slot,
    slot ∈ slots →
      ∀ decision, directAt slot = some decision →
        decision = anchorDecisionAt slot

/-- Keep a direct result when present. Otherwise, use the indirect result from
the exact anchor. -/
def finalDecisionWithDirectResult
    {Digest : Type}
    {slots : List ExactSelectedLeaderSlot}
    {anchorDecisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest)}
    (direct : AnchorCompatibleDirectResults slots anchorDecisionAt)
    (slot : ExactSelectedLeaderSlot) :
    ExactSelectedSlotDecision (LeaderBlockRef Digest) :=
  (direct.directAt slot).getD (anchorDecisionAt slot)

/-- Every retained direct result equals the exact-anchor result. -/
theorem final_decision_with_direct_result_eq_anchor
    {Digest : Type}
    {slots : List ExactSelectedLeaderSlot}
    {anchorDecisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest)}
    (direct : AnchorCompatibleDirectResults slots anchorDecisionAt)
    {slot : ExactSelectedLeaderSlot}
    (included : slot ∈ slots) :
    finalDecisionWithDirectResult direct slot = anchorDecisionAt slot := by
  cases result : direct.directAt slot with
  | none => simp [finalDecisionWithDirectResult, result]
  | some decision =>
      simpa [finalDecisionWithDirectResult, result] using
        direct.decidedMatchesAnchor slot included decision result

/-- Convert one exact final decision to the reference-carrying slot status. -/
def referenceStatusOfExactDecision
    {Digest : Type} :
    ExactSelectedSlotDecision (LeaderBlockRef Digest) →
      ReferenceSlotStatus Digest
  | .commit block => .commit block
  | .skip => .skip

/-- Converting one exact decision to a status and back preserves the ordered
decision. -/
theorem reference_status_final_decision
    {Digest : Type}
    (slot : ExactSelectedLeaderSlot)
    (decision : ExactSelectedSlotDecision (LeaderBlockRef Digest)) :
    (ReferenceSelectedSlotView.finalDecision?
      { slot
        status := referenceStatusOfExactDecision decision }) =
      some { slot, decision } := by
  cases decision <;> rfl

/-- Final selected-slot views after local direct results and the exact-anchor
indirect result are combined. -/
def finalSelectedSlotsWithDirectResults
    {Digest : Type}
    (slots : List ExactSelectedLeaderSlot)
    (anchorDecisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest))
    (direct : AnchorCompatibleDirectResults slots anchorDecisionAt) :
    List (ReferenceSelectedSlotView Digest) :=
  slots.map fun slot =>
    { slot
      status := referenceStatusOfExactDecision
        (finalDecisionWithDirectResult direct slot) }

/-- Final selected-slot views made only from the common exact-anchor result. -/
def canonicalFinalSelectedSlots
    {Digest : Type}
    (slots : List ExactSelectedLeaderSlot)
    (anchorDecisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest)) :
    List (ReferenceSelectedSlotView Digest) :=
  slots.map fun slot =>
    { slot
      status := referenceStatusOfExactDecision (anchorDecisionAt slot) }

/-- Safe local direct results cannot change the complete final selected-slot
list produced by the common exact anchor. -/
theorem final_selected_slots_with_direct_results_eq_canonical
    {Digest : Type}
    (slots : List ExactSelectedLeaderSlot)
    (anchorDecisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest))
    (direct : AnchorCompatibleDirectResults slots anchorDecisionAt) :
    finalSelectedSlotsWithDirectResults slots anchorDecisionAt direct =
      canonicalFinalSelectedSlots slots anchorDecisionAt := by
  apply List.map_congr_left
  intro slot included
  rw [final_decision_with_direct_result_eq_anchor direct included]

/-- The exact ordered decisions made by one decision function. -/
def orderedExactDecisions
    {Digest : Type}
    (slots : List ExactSelectedLeaderSlot)
    (decisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest)) :
    List (OrderedSelectedSlotDecision (LeaderBlockRef Digest)) :=
  slots.map fun slot => { slot, decision := decisionAt slot }

/-- Converting final canonical statuses recovers all exact ordered decisions. -/
theorem canonical_final_reference_decisions
    {Digest : Type}
    (slots : List ExactSelectedLeaderSlot)
    (decisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest)) :
    finalReferenceDecisions?
        (canonicalFinalSelectedSlots slots decisionAt) =
      some (orderedExactDecisions slots decisionAt) := by
  induction slots with
  | nil => rfl
  | cons slot tail ih =>
      rw [show canonicalFinalSelectedSlots (slot :: tail) decisionAt =
          { slot
            status := referenceStatusOfExactDecision (decisionAt slot) } ::
            canonicalFinalSelectedSlots tail decisionAt by rfl]
      rw [show orderedExactDecisions (slot :: tail) decisionAt =
          { slot, decision := decisionAt slot } ::
            orderedExactDecisions tail decisionAt by rfl]
      rw [finalReferenceDecisions?,
        reference_status_final_decision slot (decisionAt slot), ih]

/-- A commit in the first selected slot makes the ordered final decision list a
commit round. -/
theorem ordered_exact_decisions_have_first_commit
    {Digest : Type}
    (firstSlot : ExactSelectedLeaderSlot)
    (tail : List ExactSelectedLeaderSlot)
    (decisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest))
    (block : LeaderBlockRef Digest)
    (firstCommitted : decisionAt firstSlot = .commit block) :
    orderedDecisionsHaveCommit
        (orderedExactDecisions (firstSlot :: tail) decisionAt) = true := by
  simp [orderedExactDecisions, orderedDecisionsHaveCommit, firstCommitted]

/-- The exact candidate for one final round. -/
def canonicalReferenceFlexCandidate
    {Digest : Type}
    (round : Nat)
    (slots : List ExactSelectedLeaderSlot)
    (decisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest)) :
  ReferenceFlexCandidate Digest :=
  { leaderRound := round
    orderedCommittedLeaders := committedLeaderRefsFromDecisions
      (orderedExactDecisions slots decisionAt) }

/-- A final round whose first selected slot commits is selected as the exact
local commit candidate. -/
theorem canonical_favorable_round_finds_exact_candidate
    {Digest : Type}
    (round : Nat)
    (firstSlot : ExactSelectedLeaderSlot)
    (tail : List ExactSelectedLeaderSlot)
    (decisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest))
    (block : LeaderBlockRef Digest)
    (firstCommitted : decisionAt firstSlot = .commit block) :
    findReferenceFlexCommitCandidate
        [{ round
           selectedSlots :=
             canonicalFinalSelectedSlots (firstSlot :: tail) decisionAt }] =
      some (canonicalReferenceFlexCandidate round (firstSlot :: tail)
        decisionAt) := by
  rw [findReferenceFlexCommitCandidate,
    canonical_final_reference_decisions]
  simp [ordered_exact_decisions_have_first_commit firstSlot tail decisionAt block
    firstCommitted, canonicalReferenceFlexCandidate]

/-- Local final statuses that are safe for the common anchor select the same
exact candidate as the canonical statuses. -/
theorem direct_compatible_favorable_round_finds_exact_candidate
    {Digest : Type}
    (round : Nat)
    (firstSlot : ExactSelectedLeaderSlot)
    (tail : List ExactSelectedLeaderSlot)
    (decisionAt :
      ExactSelectedLeaderSlot →
        ExactSelectedSlotDecision (LeaderBlockRef Digest))
    (direct : AnchorCompatibleDirectResults (firstSlot :: tail) decisionAt)
    (block : LeaderBlockRef Digest)
    (firstCommitted : decisionAt firstSlot = .commit block) :
    findReferenceFlexCommitCandidate
        [{ round
           selectedSlots := finalSelectedSlotsWithDirectResults
             (firstSlot :: tail) decisionAt direct }] =
      some (canonicalReferenceFlexCandidate round (firstSlot :: tail)
        decisionAt) := by
  rw [final_selected_slots_with_direct_results_eq_canonical]
  exact canonical_favorable_round_finds_exact_candidate round firstSlot tail
    decisionAt block firstCommitted

/-- The exact round and slot identities used by one favorable recovery window.
All correct validators use these two ordered lists. -/
structure ExactFavorableRecoveryWindow
    (Digest : Type) (Correct : Nat → Prop) where
  targetRound : Nat
  depth : Nat
  targetFirstSlot : ExactSelectedLeaderSlot
  targetTailOrder : List ExactSelectedLeaderSlot
  anchorFirstSlot : ExactSelectedLeaderSlot
  anchorTailOrder : List ExactSelectedLeaderSlot
  targetFirstBlock : LeaderBlockRef Digest
  anchorBlock : LeaderBlockRef Digest
  targetOrderAtRound : ∀ slot,
    slot ∈ targetFirstSlot :: targetTailOrder → slot.round = targetRound
  anchorOrderAtRound : ∀ slot,
    slot ∈ anchorFirstSlot :: anchorTailOrder →
      slot.round = targetRound + depth
  targetBlockMatchesFirstSlot :
    targetFirstBlock.round = targetFirstSlot.round ∧
      targetFirstBlock.author = targetFirstSlot.validator
  anchorBlockMatchesFirstSlot :
    anchorBlock.round = anchorFirstSlot.round ∧
      anchorBlock.author = anchorFirstSlot.validator
  anchorAuthorCorrect : Correct anchorBlock.author

namespace ExactFavorableRecoveryWindow

def targetOrder
    {Digest : Type} {Correct : Nat → Prop}
    (window : ExactFavorableRecoveryWindow Digest Correct) :
    List ExactSelectedLeaderSlot :=
  window.targetFirstSlot :: window.targetTailOrder

def anchorOrder
    {Digest : Type} {Correct : Nat → Prop}
    (window : ExactFavorableRecoveryWindow Digest Correct) :
    List ExactSelectedLeaderSlot :=
  window.anchorFirstSlot :: window.anchorTailOrder

/-- The directly committed anchor names the configured first selected slot at
round `R + d`. -/
theorem anchor_block_at_target_plus_depth
    {Digest : Type} {Correct : Nat → Prop}
    (window : ExactFavorableRecoveryWindow Digest Correct) :
    window.anchorBlock.round = window.targetRound + window.depth := by
  rw [window.anchorBlockMatchesFirstSlot.1]
  exact window.anchorOrderAtRound window.anchorFirstSlot (by simp)

end ExactFavorableRecoveryWindow

/-- Local exclusion facts for one direct decision and one exact-anchor
certificate set. These facts follow from the direct-versus-indirect safety
lemmas. They do not refer to another validator. -/
structure ExactAnchorSlotRuleSafety
    {Digest : Type}
    (certified : List (LeaderBlockRef Digest)) where
  DirectCommit : LeaderBlockRef Digest → Prop
  DirectSkip : Prop
  directCommitIsCertified : ∀ block,
    DirectCommit block → block ∈ certified
  directCommitExcludesOtherCertificate : ∀ block other,
    DirectCommit block → other ∈ certified → other = block
  directSkipExcludesCertificate : ∀ block,
    DirectSkip → block ∈ certified → False

namespace ExactAnchorSlotRuleSafety

/-- One cached direct decision has the local evidence described by the safety
interface. -/
def ValidDirectDecision
    {Digest : Type}
    {certified : List (LeaderBlockRef Digest)}
    (safety : ExactAnchorSlotRuleSafety certified) :
    ExactSelectedSlotDecision (LeaderBlockRef Digest) → Prop
  | .commit block => safety.DirectCommit block
  | .skip => safety.DirectSkip

/-- The local direct-versus-indirect safety facts make a cached direct result
equal to the deterministic exact-anchor result. -/
theorem valid_direct_decision_matches_anchor
    {Digest : Type} [DecidableEq Digest]
    {certified : List (LeaderBlockRef Digest)}
    (nodup : certified.Nodup)
    (safety : ExactAnchorSlotRuleSafety certified)
    {decision : ExactSelectedSlotDecision (LeaderBlockRef Digest)}
    (valid : safety.ValidDirectDecision decision) :
    decision = exactIndirectDecisionFromCertificates certified := by
  cases decision with
  | commit block =>
      change safety.DirectCommit block at valid
      cases certified with
      | nil =>
          have included := safety.directCommitIsCertified block valid
          simp at included
      | cons first tail =>
          cases tail with
          | nil =>
              have same := safety.directCommitExcludesOtherCertificate
                block first valid (by simp)
              subst first
              rfl
          | cons second rest =>
              have firstSame := safety.directCommitExcludesOtherCertificate
                block first valid (by simp)
              have secondSame := safety.directCommitExcludesOtherCertificate
                block second valid (by simp)
              subst first
              subst second
              simp at nodup
  | skip =>
      change safety.DirectSkip at valid
      cases certified with
      | nil => rfl
      | cons first tail =>
          cases tail with
          | nil =>
              exact False.elim
                (safety.directSkipExcludesCertificate first valid (by simp))
          | cons second rest => rfl

end ExactAnchorSlotRuleSafety

/-- All cached direct results in one local target round, with local safety
evidence against the common exact-anchor certificate data. -/
structure SafeDirectResultsForAnchor
    {Digest : Type} [DecidableEq Digest]
    (data : ExactAnchorCausalData Digest)
    (slots : List ExactSelectedLeaderSlot) where
  directAt : ExactSelectedLeaderSlot → OptionalExactDirectResult Digest
  safetyAt : ∀ slot, ExactAnchorSlotRuleSafety (data.certifiedAt slot)
  directValid : ∀ slot,
    slot ∈ slots →
      ∀ decision, directAt slot = some decision →
        (safetyAt slot).ValidDirectDecision decision

namespace SafeDirectResultsForAnchor

/-- Local quorum-safety evidence supplies the compatibility interface used by
the ordered FlexCommitter proof. -/
def toAnchorCompatible
    {Digest : Type} [DecidableEq Digest]
    {data : ExactAnchorCausalData Digest}
    {slots : List ExactSelectedLeaderSlot}
    (direct : SafeDirectResultsForAnchor data slots) :
    AnchorCompatibleDirectResults slots
      (canonicalExactIndirectDecision data) :=
  { directAt := direct.directAt
    decidedMatchesAnchor := by
      intro slot included decision found
      exact ExactAnchorSlotRuleSafety.valid_direct_decision_matches_anchor
        (data.certifiedNodup slot) (direct.safetyAt slot)
        (direct.directValid slot included decision found) }

end SafeDirectResultsForAnchor

/-- A one-validator bridge from exact anchor history and vote evidence to the
certificate list used by this module. -/
structure ExactAnchorEvidenceMap
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    (data : ExactAnchorCausalData Digest)
    (commitVotes skipVotes : LeaderBlockRef Digest → VoterSet) where
  history : LeaderAnchorHistory Digest authorityCount stake thresholds
  historyNamesAnchor : history.anchorRef = data.anchor
  certifiedIff : ∀ slot block,
    block.AtSelectedSlot slot →
      (block ∈ data.certifiedAt slot ↔ history.HasCertificate block)
  directAnchorEvidence :
    DirectAnchorEvidence thresholds commitVotes skipVotes history

namespace ExactAnchorEvidenceMap

/-- Exact quorum and non-equivocation evidence proves the three local exclusion
facts used by the favorable-window composition. -/
def slotRuleSafety
    {Digest : Type} {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {data : ExactAnchorCausalData Digest}
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    (source : @ExactAnchorEvidenceMap Digest authorityCount stake thresholds
      data commitVotes skipVotes)
    (slot : ExactSelectedLeaderSlot) :
    ExactAnchorSlotRuleSafety (data.certifiedAt slot) :=
  { DirectCommit := fun block =>
      block.AtSelectedSlot slot ∧
        thresholds.quorum ≤ weight authorityCount stake (commitVotes block)
    DirectSkip := ∀ block, block.AtSelectedSlot slot →
      thresholds.quorum ≤ weight authorityCount stake (skipVotes block)
    directCommitIsCertified := by
      intro block committed
      have certified := source.directAnchorEvidence.direct_commit_is_certified
        committed.2
      exact (source.certifiedIff slot block committed.1).2 certified
    directCommitExcludesOtherCertificate := by
      intro block other committed otherIncluded
      have otherAt : other.AtSelectedSlot slot :=
        data.certifiedMatchesSlot slot other otherIncluded
      have otherCertified : source.history.HasCertificate other :=
        (source.certifiedIff slot other otherAt).1 otherIncluded
      have sameSlot := LeaderBlockRef.same_selected_slot_of_at_slot
        otherAt committed.1
      exact source.directAnchorEvidence.direct_commit_excludes_other_certificate
        committed.2 sameSlot otherCertified
    directSkipExcludesCertificate := by
      intro block skipped included
      have blockAt : block.AtSelectedSlot slot :=
        data.certifiedMatchesSlot slot block included
      have certified : source.history.HasCertificate block :=
        (source.certifiedIff slot block blockAt).1 included
      exact source.directAnchorEvidence.direct_skip_excludes_certificate
        (skipped block blockAt) certified }

/-- Build the local direct-result safety proof from the exact Rust-style direct
status evidence and the exact-anchor evidence map. -/
def safeDirectResults
    {Digest : Type} [DecidableEq Digest]
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {data : ExactAnchorCausalData Digest}
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    (source : @ExactAnchorEvidenceMap Digest authorityCount stake thresholds
      data commitVotes skipVotes)
    (slots : List ExactSelectedLeaderSlot)
    (directAt : ExactSelectedLeaderSlot → OptionalExactDirectResult Digest)
    (directValid : ∀ slot,
      slot ∈ slots →
        ∀ decision, directAt slot = some decision →
          ExactDirectStatusValid thresholds commitVotes skipVotes slot
            (referenceStatusOfExactDecision decision)) :
    SafeDirectResultsForAnchor data slots :=
  { directAt
    safetyAt := source.slotRuleSafety
    directValid := by
      intro slot included decision found
      have valid := directValid slot included decision found
      cases decision <;>
        simpa [ExactAnchorSlotRuleSafety.ValidDirectDecision,
          ExactDirectStatusValid, slotRuleSafety,
          referenceStatusOfExactDecision] using valid }

end ExactAnchorEvidenceMap

/-- The canonical indirect decision for one target slot in the favorable
window. -/
def favorableRecoveryAnchorDecisionAt
    {LocalView Digest : Type} [DecidableEq Digest]
    {Correct : Nat → Prop}
    (mapping : ExactAnchorCausalDataMap LocalView Digest)
    (window : ExactFavorableRecoveryWindow Digest Correct)
    (slot : ExactSelectedLeaderSlot) :
    ExactSelectedSlotDecision (LeaderBlockRef Digest) :=
  canonicalExactIndirectDecision
    (mapping.canonicalData window.anchorBlock) slot

/-- One validator's local input for the restricted favorable-window proof.

The input contains only local facts. The first anchor slot and first target slot
are directly committed. The target round's other direct results have local
safety proofs against the exact-anchor result. -/
structure FavorableRecoveryLocalInput
    {LocalView Digest : Type} [DecidableEq Digest]
    {Correct : Nat → Prop}
    (mapping : ExactAnchorCausalDataMap LocalView Digest)
    (window : ExactFavorableRecoveryWindow Digest Correct) where
  view : LocalView
  completeAnchor : mapping.complete view window.anchorBlock
  anchorTailStatus :
    ExactSelectedLeaderSlot → ReferenceSlotStatus Digest
  targetDirect : SafeDirectResultsForAnchor
    (mapping.canonicalData window.anchorBlock) window.targetOrder
  targetFirstDirect :
    targetDirect.directAt window.targetFirstSlot =
      some (.commit window.targetFirstBlock)

namespace FavorableRecoveryLocalInput

/-- Build one local favorable-window input from exact direct-vote and anchor
history evidence. -/
def ofExactEvidence
    {LocalView Digest : Type} [DecidableEq Digest]
    {Correct : Nat → Prop}
    {authorityCount : Nat} {stake : Nat → Nat}
    {thresholds : Thresholds authorityCount stake}
    {mapping : ExactAnchorCausalDataMap LocalView Digest}
    {window : ExactFavorableRecoveryWindow Digest Correct}
    (view : LocalView)
    (completeAnchor : mapping.complete view window.anchorBlock)
    (anchorTailStatus :
      ExactSelectedLeaderSlot → ReferenceSlotStatus Digest)
    {commitVotes skipVotes : LeaderBlockRef Digest → VoterSet}
    (evidence : @ExactAnchorEvidenceMap Digest authorityCount stake thresholds
      (mapping.canonicalData window.anchorBlock) commitVotes skipVotes)
    (directAt : ExactSelectedLeaderSlot → OptionalExactDirectResult Digest)
    (directValid : ∀ slot,
      slot ∈ window.targetOrder →
        ∀ decision, directAt slot = some decision →
          ExactDirectStatusValid thresholds commitVotes skipVotes slot
            (referenceStatusOfExactDecision decision))
    (targetFirstDirect :
      directAt window.targetFirstSlot =
        some (.commit window.targetFirstBlock)) :
    FavorableRecoveryLocalInput mapping window :=
  { view
    completeAnchor
    anchorTailStatus
    targetDirect := evidence.safeDirectResults window.targetOrder directAt
      directValid
    targetFirstDirect }

def targetCompatibleDirect
    {LocalView Digest : Type} [DecidableEq Digest]
    {Correct : Nat → Prop}
    {mapping : ExactAnchorCausalDataMap LocalView Digest}
    {window : ExactFavorableRecoveryWindow Digest Correct}
    (input : FavorableRecoveryLocalInput mapping window) :
    AnchorCompatibleDirectResults window.targetOrder
      (favorableRecoveryAnchorDecisionAt mapping window) := by
  unfold favorableRecoveryAnchorDecisionAt
  exact input.targetDirect.toAnchorCompatible

def anchorSelectedSlots
    {LocalView Digest : Type} [DecidableEq Digest]
    {Correct : Nat → Prop}
    {mapping : ExactAnchorCausalDataMap LocalView Digest}
    {window : ExactFavorableRecoveryWindow Digest Correct}
    (input : FavorableRecoveryLocalInput mapping window) :
    List (ReferenceSelectedSlotView Digest) :=
  favorableAnchorSelectedSlots window.anchorFirstSlot window.anchorTailOrder
    window.anchorBlock input.anchorTailStatus

def targetSelectedSlots
    {LocalView Digest : Type} [DecidableEq Digest]
    {Correct : Nat → Prop}
    {mapping : ExactAnchorCausalDataMap LocalView Digest}
    {window : ExactFavorableRecoveryWindow Digest Correct}
    (input : FavorableRecoveryLocalInput mapping window) :
    List (ReferenceSelectedSlotView Digest) :=
  finalSelectedSlotsWithDirectResults window.targetOrder
    (favorableRecoveryAnchorDecisionAt mapping window)
    input.targetCompatibleDirect

def targetRounds
    {LocalView Digest : Type} [DecidableEq Digest]
    {Correct : Nat → Prop}
    {mapping : ExactAnchorCausalDataMap LocalView Digest}
    {window : ExactFavorableRecoveryWindow Digest Correct}
    (input : FavorableRecoveryLocalInput mapping window) :
    List (ReferenceFlexRoundView Digest) :=
  [{ round := window.targetRound
     selectedSlots := input.targetSelectedSlots }]

end FavorableRecoveryLocalInput

/-- The exact candidate derived from the common anchor in the target round. -/
def favorableRecoveryCandidate
    {LocalView Digest : Type} [DecidableEq Digest]
    {Correct : Nat → Prop}
    (mapping : ExactAnchorCausalDataMap LocalView Digest)
    (window : ExactFavorableRecoveryWindow Digest Correct) :
    ReferenceFlexCandidate Digest :=
  canonicalReferenceFlexCandidate window.targetRound window.targetOrder
    (favorableRecoveryAnchorDecisionAt mapping window)

/-- The complete result of comparing two local favorable-window executions.
The two local block stores can contain different extra blocks. -/
structure FavorableRecoveryPairAgreement
    {LocalView BlockDigest CommitId Encoding : Type}
    [DecidableEq BlockDigest]
    {Correct : Nat → Prop}
    (mapping : ExactAnchorCausalDataMap LocalView BlockDigest)
    (buildMapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockDigest CommitId)
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockDigest) Encoding)
    (window : ExactFavorableRecoveryWindow BlockDigest Correct)
    (left right : FavorableRecoveryLocalInput mapping window)
    (leftPrior rightPrior : ExactCommitReference CommitId) : Prop where
  leftSelectsExactAnchor :
    scanReferenceSelectedSlots left.anchorSelectedSlots =
      .found window.anchorBlock
  rightSelectsExactAnchor :
    scanReferenceSelectedSlots right.anchorSelectedSlots =
      .found window.anchorBlock
  leftCausalDataIsCanonical :
    mapping.localData left.view window.anchorBlock =
      mapping.canonicalData window.anchorBlock
  rightCausalDataIsCanonical :
    mapping.localData right.view window.anchorBlock =
      mapping.canonicalData window.anchorBlock
  indirectResultsAgree : ∀ slot,
    slot ∈ window.targetOrder →
      localExactIndirectDecision
          (mapping.localData left.view window.anchorBlock)
          (mapping.availableAt left.view) slot =
        localExactIndirectDecision
          (mapping.localData right.view window.anchorBlock)
          (mapping.availableAt right.view) slot
  leftDirectResultsMatchAnchor : ∀ slot,
    slot ∈ window.targetOrder →
      finalDecisionWithDirectResult left.targetCompatibleDirect slot =
        localExactIndirectDecision
          (mapping.localData left.view window.anchorBlock)
          (mapping.availableAt left.view) slot
  rightDirectResultsMatchAnchor : ∀ slot,
    slot ∈ window.targetOrder →
      finalDecisionWithDirectResult right.targetCompatibleDirect slot =
        localExactIndirectDecision
          (mapping.localData right.view window.anchorBlock)
          (mapping.availableAt right.view) slot
  leftFindsCandidate :
    findReferenceFlexCommitCandidate left.targetRounds =
      some (favorableRecoveryCandidate mapping window)
  rightFindsCandidate :
    findReferenceFlexCommitCandidate right.targetRounds =
      some (favorableRecoveryCandidate mapping window)
  candidateResultsAgree :
    findReferenceFlexCommitCandidate left.targetRounds =
      findReferenceFlexCommitCandidate right.targetRounds
  committedLeadersAgree :
    (findReferenceFlexCommitCandidate left.targetRounds).map
        ReferenceFlexCandidate.committedLeaderRefs =
      (findReferenceFlexCommitCandidate right.targetRounds).map
        ReferenceFlexCandidate.committedLeaderRefs
  builderInputsAgree :
    buildMapping.localInput left.view leftPrior
        (favorableRecoveryCandidate mapping window) =
      buildMapping.localInput right.view rightPrior
        (favorableRecoveryCandidate mapping window)
  builtReferencesAgree :
    constructExactCommitReferenceFromInput functions
        (buildMapping.localInput left.view leftPrior
          (favorableRecoveryCandidate mapping window)) =
      constructExactCommitReferenceFromInput functions
        (buildMapping.localInput right.view rightPrior
          (favorableRecoveryCandidate mapping window))

/-- Two local executions in one favorable recovery window derive one exact
commit candidate and one deterministic builder input.

The theorem does not use `CrossViewExactSlotAgreement`. The two validators are
compared only through a common exact anchor. Each completeness premise is local.
`ExactAnchorCausalDataMap` maps the decision closure. The separate one-validator
`ReferenceCommitMaterializerSourceMap` maps the Rust DFS, sort, named leader,
and timestamp calculation. -/
theorem favorable_recovery_window_pair_agrees
    {LocalView BlockDigest CommitId Encoding : Type}
    [DecidableEq BlockDigest]
    {Correct : Nat → Prop}
    (mapping : ExactAnchorCausalDataMap LocalView BlockDigest)
    (buildMapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockDigest CommitId)
    (functions : CommitReferenceFunctions CommitId
      (LeaderBlockRef BlockDigest) Encoding)
    (window : ExactFavorableRecoveryWindow BlockDigest Correct)
    (left right : FavorableRecoveryLocalInput mapping window)
    {leftPrior rightPrior : ExactCommitReference CommitId}
    (samePrior : leftPrior = rightPrior)
    (leftBuildComplete : buildMapping.complete left.view leftPrior
      (favorableRecoveryCandidate mapping window))
    (rightBuildComplete : buildMapping.complete right.view rightPrior
      (favorableRecoveryCandidate mapping window)) :
    FavorableRecoveryPairAgreement mapping buildMapping functions window left right
      leftPrior rightPrior := by
  have leftAnchor :
      scanReferenceSelectedSlots left.anchorSelectedSlots =
        .found window.anchorBlock := by
    exact favorable_first_slot_selects_exact_anchor window.anchorFirstSlot
      window.anchorTailOrder window.anchorBlock left.anchorTailStatus
  have rightAnchor :
      scanReferenceSelectedSlots right.anchorSelectedSlots =
        .found window.anchorBlock := by
    exact favorable_first_slot_selects_exact_anchor window.anchorFirstSlot
      window.anchorTailOrder window.anchorBlock right.anchorTailStatus
  have leftData :
      mapping.localData left.view window.anchorBlock =
        mapping.canonicalData window.anchorBlock :=
    mapping.completeMapsToCanonical left.view window.anchorBlock
      left.completeAnchor
  have rightData :
      mapping.localData right.view window.anchorBlock =
        mapping.canonicalData window.anchorBlock :=
    mapping.completeMapsToCanonical right.view window.anchorBlock
      right.completeAnchor
  have indirectEqual : ∀ slot,
      slot ∈ window.targetOrder →
        localExactIndirectDecision
            (mapping.localData left.view window.anchorBlock)
            (mapping.availableAt left.view) slot =
          localExactIndirectDecision
            (mapping.localData right.view window.anchorBlock)
            (mapping.availableAt right.view) slot := by
    intro slot _included
    exact mapping.two_complete_views_have_same_indirect_result
      left.completeAnchor right.completeAnchor slot
  have leftDirectMatches : ∀ slot,
      slot ∈ window.targetOrder →
        finalDecisionWithDirectResult left.targetCompatibleDirect slot =
          localExactIndirectDecision
            (mapping.localData left.view window.anchorBlock)
            (mapping.availableAt left.view) slot := by
    intro slot included
    calc
      finalDecisionWithDirectResult left.targetCompatibleDirect slot =
          favorableRecoveryAnchorDecisionAt mapping window slot :=
        final_decision_with_direct_result_eq_anchor
          left.targetCompatibleDirect included
      _ = localExactIndirectDecision
          (mapping.localData left.view window.anchorBlock)
          (mapping.availableAt left.view) slot := by
        symm
        exact mapping.complete_view_has_canonical_indirect_result
          left.completeAnchor slot
  have rightDirectMatches : ∀ slot,
      slot ∈ window.targetOrder →
        finalDecisionWithDirectResult right.targetCompatibleDirect slot =
          localExactIndirectDecision
            (mapping.localData right.view window.anchorBlock)
            (mapping.availableAt right.view) slot := by
    intro slot included
    calc
      finalDecisionWithDirectResult right.targetCompatibleDirect slot =
          favorableRecoveryAnchorDecisionAt mapping window slot :=
        final_decision_with_direct_result_eq_anchor
          right.targetCompatibleDirect included
      _ = localExactIndirectDecision
          (mapping.localData right.view window.anchorBlock)
          (mapping.availableAt right.view) slot := by
        symm
        exact mapping.complete_view_has_canonical_indirect_result
          right.completeAnchor slot
  have targetFirstIncluded :
      window.targetFirstSlot ∈ window.targetOrder := by
    simp [ExactFavorableRecoveryWindow.targetOrder]
  have firstCanonical :
      favorableRecoveryAnchorDecisionAt mapping window
          window.targetFirstSlot =
        .commit window.targetFirstBlock := by
    have targetFirstDirectCompatible :
        left.targetCompatibleDirect.directAt window.targetFirstSlot =
          some (.commit window.targetFirstBlock) := by
      change left.targetDirect.directAt window.targetFirstSlot =
        some (.commit window.targetFirstBlock)
      exact left.targetFirstDirect
    exact (left.targetCompatibleDirect.decidedMatchesAnchor
      window.targetFirstSlot
      targetFirstIncluded (.commit window.targetFirstBlock)
      targetFirstDirectCompatible).symm
  have leftFound :
      findReferenceFlexCommitCandidate left.targetRounds =
        some (favorableRecoveryCandidate mapping window) := by
    simpa [FavorableRecoveryLocalInput.targetRounds,
      FavorableRecoveryLocalInput.targetSelectedSlots,
      ExactFavorableRecoveryWindow.targetOrder,
      favorableRecoveryCandidate] using
      direct_compatible_favorable_round_finds_exact_candidate
        window.targetRound window.targetFirstSlot window.targetTailOrder
        (favorableRecoveryAnchorDecisionAt mapping window)
        left.targetCompatibleDirect
        window.targetFirstBlock firstCanonical
  have rightFound :
      findReferenceFlexCommitCandidate right.targetRounds =
        some (favorableRecoveryCandidate mapping window) := by
    simpa [FavorableRecoveryLocalInput.targetRounds,
      FavorableRecoveryLocalInput.targetSelectedSlots,
      ExactFavorableRecoveryWindow.targetOrder,
      favorableRecoveryCandidate] using
      direct_compatible_favorable_round_finds_exact_candidate
        window.targetRound window.targetFirstSlot window.targetTailOrder
        (favorableRecoveryAnchorDecisionAt mapping window)
        right.targetCompatibleDirect
        window.targetFirstBlock firstCanonical
  have candidateEqual :
      findReferenceFlexCommitCandidate left.targetRounds =
        findReferenceFlexCommitCandidate right.targetRounds := by
    rw [leftFound, rightFound]
  have leadersEqual :
      (findReferenceFlexCommitCandidate left.targetRounds).map
          ReferenceFlexCandidate.committedLeaderRefs =
        (findReferenceFlexCommitCandidate right.targetRounds).map
          ReferenceFlexCandidate.committedLeaderRefs := by
    rw [candidateEqual]
  have inputsEqual :
      buildMapping.localInput left.view leftPrior
          (favorableRecoveryCandidate mapping window) =
        buildMapping.localInput right.view rightPrior
          (favorableRecoveryCandidate mapping window) := by
    exact buildMapping.complete_local_inputs_agree samePrior rfl
      leftBuildComplete rightBuildComplete
  have referencesEqual :
      constructExactCommitReferenceFromInput functions
          (buildMapping.localInput left.view leftPrior
            (favorableRecoveryCandidate mapping window)) =
        constructExactCommitReferenceFromInput functions
          (buildMapping.localInput right.view rightPrior
            (favorableRecoveryCandidate mapping window)) := by
    rw [inputsEqual]
  exact
    { leftSelectsExactAnchor := leftAnchor
      rightSelectsExactAnchor := rightAnchor
      leftCausalDataIsCanonical := leftData
      rightCausalDataIsCanonical := rightData
      indirectResultsAgree := indirectEqual
      leftDirectResultsMatchAnchor := leftDirectMatches
      rightDirectResultsMatchAnchor := rightDirectMatches
      leftFindsCandidate := leftFound
      rightFindsCandidate := rightFound
      candidateResultsAgree := candidateEqual
      committedLeadersAgree := leadersEqual
      builderInputsAgree := inputsEqual
      builtReferencesAgree := referencesEqual }

end Mysticeti
