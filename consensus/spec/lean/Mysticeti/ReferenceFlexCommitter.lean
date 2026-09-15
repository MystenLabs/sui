/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.CommitReferenceAgreement
import Mysticeti.LeaderBranchAgreement

namespace Mysticeti

/-!
A reference-carrying model of the ordered Mysticeti v3 FlexCommitter scans.

The status-only model in `FlexCommitter.lean` is useful for progress proofs. It
cannot state exact agreement because a commit status does not contain a block
reference. This module keeps the full selected leader block reference.

The conditional agreement theorems do not assume one common commit candidate.
Each validator runs its own scan. The scan theorems derive one exact candidate
and one ordered committed-leader list. A separate one-validator source map covers
the Rust causal traversal, block sort, named leader, and timestamp calculation.

The remaining safety goal is `CrossViewExactSlotAgreement`. The exact-reference
commit cases with at least one direct result are proved in
`LeaderBranchAgreement.lean`. Two indirect commits need comparable anchor
histories. General cross-view commit-versus-skip exclusion is also still open.
-/

/-- A selected leader slot is identified before the committer scans it. -/
structure ExactSelectedLeaderSlot where
  round : Nat
  validator : Nat

/-- A final selected-slot decision. A commit result carries its exact block ID. -/
inductive ExactSelectedSlotDecision (BlockId : Type) where
  | commit (block : BlockId)
  | skip

/-- One final decision at one position in the selected leader slot order. -/
structure OrderedSelectedSlotDecision (BlockId : Type) where
  slot : ExactSelectedLeaderSlot
  decision : ExactSelectedSlotDecision BlockId

/-- One local status for one selected leader slot. A commit status keeps the
complete block reference, including the branch digest. -/
inductive ReferenceSlotStatus (Digest : Type) where
  | undecided
  | commit (block : LeaderBlockRef Digest)
  | skip

/-- One selected leader slot at its exact position in the scan order. -/
structure ReferenceSelectedSlotView (Digest : Type) where
  slot : ExactSelectedLeaderSlot
  status : ReferenceSlotStatus Digest

namespace ReferenceSelectedSlotView

/-- A committed block must name the selected round and validator. -/
def WellFormed {Digest : Type}
    (view : ReferenceSelectedSlotView Digest) : Prop :=
  match view.status with
  | .commit block =>
      block.round = view.slot.round ∧ block.author = view.slot.validator
  | .undecided | .skip => True

/-- Convert one final local status to the builder decision type. -/
def finalDecision? {Digest : Type}
    (view : ReferenceSelectedSlotView Digest) :
    Option (OrderedSelectedSlotDecision (LeaderBlockRef Digest)) :=
  match view.status with
  | .undecided => none
  | .commit block => some { slot := view.slot, decision := .commit block }
  | .skip => some { slot := view.slot, decision := .skip }

end ReferenceSelectedSlotView

/-- Two local statuses do not conflict.

An undecided status can later become either final result. Two final statuses must
have the same result. Two commit results must contain the same exact block
reference. -/
def ReferenceSlotStatus.Compatible {Digest : Type} :
    ReferenceSlotStatus Digest → ReferenceSlotStatus Digest → Prop
  | .undecided, _ => True
  | _, .undecided => True
  | .commit left, .commit right => left = right
  | .skip, .skip => True
  | _, _ => False

/-- The named cross-view selected-slot safety goal.

This relation is not a common-candidate assumption. It compares one selected
slot at the same position in two local views. The open protocol proof must derive
it from authenticated votes, quorum intersections, and the common first-anchor
rule. It must cover two indirect commits and cross-view commit-versus-skip
results. -/
def CrossViewExactSlotAgreement {Digest : Type}
    (left right : ReferenceSelectedSlotView Digest) : Prop :=
  left.slot = right.slot ∧ left.status.Compatible right.status

/-- Pointwise agreement for two lists with the same positions. -/
inductive ExactListAgreement {Left Right : Type}
    (relation : Left → Right → Prop) : List Left → List Right → Prop where
  | nil : ExactListAgreement relation [] []
  | cons {left : Left} {right : Right} {leftTail : List Left}
      {rightTail : List Right} :
      relation left right → ExactListAgreement relation leftTail rightTail →
      ExactListAgreement relation (left :: leftTail) (right :: rightTail)

/-- Pointwise agreement for a common list prefix. Either local view can contain
more later entries. -/
inductive ExactPrefixAgreement {Left Right : Type}
    (relation : Left → Right → Prop) : List Left → List Right → Prop where
  | leftNil {right : List Right} : ExactPrefixAgreement relation [] right
  | rightNil {left : List Left} : ExactPrefixAgreement relation left []
  | cons {left : Left} {right : Right} {leftTail : List Left}
      {rightTail : List Right} :
      relation left right →
      ExactPrefixAgreement relation leftTail rightTail →
      ExactPrefixAgreement relation (left :: leftTail) (right :: rightTail)

/-- One round keeps all selected leader slots in their scan order. -/
structure ReferenceFlexRoundView (Digest : Type) where
  round : Nat
  selectedSlots : List (ReferenceSelectedSlotView Digest)

/-- Corresponding rounds use the same round number and selected-slot order. Their
final slot results contain the same exact references. -/
structure CrossViewExactRoundAgreement {Digest : Type}
    (left right : ReferenceFlexRoundView Digest) : Prop where
  sameRound : left.round = right.round
  selectedSlots : ExactListAgreement CrossViewExactSlotAgreement
    left.selectedSlots right.selectedSlots

/-- One local pending FlexCommitter state. The list order is the Rust pending
round order. Each round stores its selected leader slot order. -/
structure ReferenceFlexState (Digest : Type) where
  commitIndex : Nat
  rounds : List (ReferenceFlexRoundView Digest)

/-- Two local pending states use the same next commit index and corresponding
round and slot orders. Final corresponding decisions agree exactly. -/
structure CrossViewReferenceFlexAgreement {Digest : Type}
    (left right : ReferenceFlexState Digest) : Prop where
  sameCommitIndex : left.commitIndex = right.commitIndex
  rounds : ExactPrefixAgreement CrossViewExactRoundAgreement
    left.rounds right.rounds

/-- The result of one ordered anchor scan. -/
inductive ReferenceAnchorScanResult (Digest : Type) where
  | blocked
  | noAnchor
  | found (block : LeaderBlockRef Digest)

/-- Compatible scan results can differ only when one view is blocked. Two found
results must contain the same exact block reference. -/
def ReferenceAnchorScanResult.Compatible {Digest : Type} :
    ReferenceAnchorScanResult Digest → ReferenceAnchorScanResult Digest → Prop
  | .blocked, _ => True
  | _, .blocked => True
  | .noAnchor, .noAnchor => True
  | .found left, .found right => left = right
  | _, _ => False

/-- Scan selected slots in order. Skip continues the scan. Undecided blocks the
scan. The first commit returns its exact block reference. -/
def scanReferenceSelectedSlots {Digest : Type} :
    List (ReferenceSelectedSlotView Digest) → ReferenceAnchorScanResult Digest
  | [] => .noAnchor
  | view :: tail =>
      match view.status with
      | .undecided => .blocked
      | .commit block => .found block
      | .skip => scanReferenceSelectedSlots tail

/-- Corresponding selected-slot lists produce compatible scan results. -/
theorem reference_selected_slot_scans_compatible
    {Digest : Type}
    {left right : List (ReferenceSelectedSlotView Digest)}
    (agreement : ExactListAgreement CrossViewExactSlotAgreement left right) :
    (scanReferenceSelectedSlots left).Compatible
      (scanReferenceSelectedSlots right) := by
  induction agreement with
  | nil =>
      simp [scanReferenceSelectedSlots,
        ReferenceAnchorScanResult.Compatible]
  | @cons leftHead rightHead leftTail rightTail headAgreement tailAgreement ih =>
      rcases headAgreement with ⟨sameSlot, compatible⟩
      cases leftStatus : leftHead.status <;>
        cases rightStatus : rightHead.status <;>
        simp [ReferenceSlotStatus.Compatible,
          ReferenceAnchorScanResult.Compatible, scanReferenceSelectedSlots,
          leftStatus, rightStatus] at compatible ⊢
      case commit.commit => exact compatible
      case skip.undecided =>
        cases scanReferenceSelectedSlots leftTail <;> simp
      case skip.skip => exact ih

/-- Two successful ordered slot scans return the same exact block reference. -/
theorem reference_selected_slot_anchor_agreement
    {Digest : Type}
    {left right : List (ReferenceSelectedSlotView Digest)}
    (agreement : ExactListAgreement CrossViewExactSlotAgreement left right)
    {leftBlock rightBlock : LeaderBlockRef Digest}
    (leftFound : scanReferenceSelectedSlots left = .found leftBlock)
    (rightFound : scanReferenceSelectedSlots right = .found rightBlock) :
    leftBlock = rightBlock := by
  induction agreement with
  | nil => simp [scanReferenceSelectedSlots] at leftFound
  | @cons leftHead rightHead leftTail rightTail headAgreement tailAgreement ih =>
      rcases headAgreement with ⟨sameSlot, compatible⟩
      cases leftStatus : leftHead.status <;>
        cases rightStatus : rightHead.status <;>
        simp [ReferenceSlotStatus.Compatible, scanReferenceSelectedSlots,
          leftStatus, rightStatus] at compatible leftFound rightFound
      case commit.commit leftRef rightRef =>
        exact leftFound.symm.trans (compatible.trans rightFound)
      case skip.skip =>
        exact ih leftFound rightFound

/-- Scan all pending rounds in round order and all selected leader slots in their
stored order. -/
def scanReferenceAnchorRounds {Digest : Type} :
    List (ReferenceFlexRoundView Digest) → ReferenceAnchorScanResult Digest
  | [] => .noAnchor
  | round :: tail =>
      match scanReferenceSelectedSlots round.selectedSlots with
      | .blocked => .blocked
      | .found block => .found block
      | .noAnchor => scanReferenceAnchorRounds tail

/-- If both local ordered anchor scans find an anchor, they return the same exact
block reference. Later suffixes can differ. -/
theorem reference_anchor_round_scans_agree
    {Digest : Type}
    {left right : List (ReferenceFlexRoundView Digest)}
    (agreement : ExactPrefixAgreement CrossViewExactRoundAgreement left right)
    {leftBlock rightBlock : LeaderBlockRef Digest}
    (leftFound : scanReferenceAnchorRounds left = .found leftBlock)
    (rightFound : scanReferenceAnchorRounds right = .found rightBlock) :
    leftBlock = rightBlock := by
  induction agreement with
  | leftNil => simp [scanReferenceAnchorRounds] at leftFound
  | rightNil => simp [scanReferenceAnchorRounds] at rightFound
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      have headCompatible :=
        reference_selected_slot_scans_compatible roundAgreement.selectedSlots
      cases leftScan : scanReferenceSelectedSlots leftRound.selectedSlots <;>
        cases rightScan : scanReferenceSelectedSlots rightRound.selectedSlots <;>
        simp [ReferenceAnchorScanResult.Compatible, scanReferenceAnchorRounds,
          leftScan, rightScan] at headCompatible leftFound rightFound
      case noAnchor.noAnchor => exact ih leftFound rightFound
      case found.found leftRef rightRef =>
        exact leftFound.symm.trans (headCompatible.trans rightFound)

/-- Convert one fully decided round to its ordered exact decisions. -/
def finalReferenceDecisions? {Digest : Type} :
    List (ReferenceSelectedSlotView Digest) →
      Option (List (OrderedSelectedSlotDecision (LeaderBlockRef Digest)))
  | [] => some []
  | view :: tail =>
      match view.finalDecision?, finalReferenceDecisions? tail with
      | some decision, some decisions => some (decision :: decisions)
      | _, _ => none

/-- A successful nonempty conversion contains one final head decision and one
successful tail conversion. -/
theorem final_reference_decisions_cons_parts
    {Digest : Type}
    {head : ReferenceSelectedSlotView Digest}
    {tail : List (ReferenceSelectedSlotView Digest)}
    {decisions :
      List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
    (final : finalReferenceDecisions? (head :: tail) = some decisions) :
    ∃ headDecision tailDecisions,
      head.finalDecision? = some headDecision ∧
        finalReferenceDecisions? tail = some tailDecisions ∧
        decisions = headDecision :: tailDecisions := by
  simp only [finalReferenceDecisions?] at final
  cases headResult : head.finalDecision? with
  | none => simp [headResult] at final
  | some headDecision =>
      cases tailResult : finalReferenceDecisions? tail with
      | none => simp [headResult, tailResult] at final
      | some tailDecisions =>
          simp [headResult, tailResult] at final
          exact ⟨headDecision, tailDecisions, rfl, rfl, final.symm⟩

/-- Corresponding final selected slots have the same complete decision. -/
theorem final_reference_slot_decision_agrees
    {Digest : Type}
    {left right : ReferenceSelectedSlotView Digest}
    (agreement : CrossViewExactSlotAgreement left right)
    {leftDecision rightDecision :
      OrderedSelectedSlotDecision (LeaderBlockRef Digest)}
    (leftFinal : left.finalDecision? = some leftDecision)
    (rightFinal : right.finalDecision? = some rightDecision) :
    leftDecision = rightDecision := by
  rcases agreement with ⟨sameSlot, compatible⟩
  cases left with
  | mk leftSlot leftStatus =>
      cases right with
      | mk rightSlot rightStatus =>
          simp only at sameSlot
          subst rightSlot
          cases leftStatus <;> cases rightStatus <;>
            simp [ReferenceSlotStatus.Compatible,
              ReferenceSelectedSlotView.finalDecision?] at compatible leftFinal rightFinal
          case commit.commit leftBlock rightBlock =>
            subst rightBlock
            exact leftFinal.symm.trans rightFinal
          case skip.skip =>
            exact leftFinal.symm.trans rightFinal

/-- If two corresponding slot lists are final, their complete ordered exact
decision lists are equal. -/
theorem final_reference_decisions_agree
    {Digest : Type}
    {left right : List (ReferenceSelectedSlotView Digest)}
    (agreement : ExactListAgreement CrossViewExactSlotAgreement left right)
    {leftDecisions rightDecisions :
      List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))}
    (leftFinal : finalReferenceDecisions? left = some leftDecisions)
    (rightFinal : finalReferenceDecisions? right = some rightDecisions) :
    leftDecisions = rightDecisions := by
  induction agreement generalizing leftDecisions rightDecisions with
  | nil =>
      have leftEmpty :
          ([] : List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))) =
            leftDecisions := by
        exact Option.some.inj (by simpa [finalReferenceDecisions?] using leftFinal)
      have rightEmpty :
          ([] : List (OrderedSelectedSlotDecision (LeaderBlockRef Digest))) =
            rightDecisions := by
        exact Option.some.inj (by simpa [finalReferenceDecisions?] using rightFinal)
      exact leftEmpty.symm.trans rightEmpty
  | @cons leftHead rightHead leftTail rightTail headAgreement tailAgreement ih =>
      rcases final_reference_decisions_cons_parts leftFinal with
        ⟨leftHeadDecision, leftTailDecisions, leftHeadFinal, leftTailFinal,
          leftDecisionsShape⟩
      rcases final_reference_decisions_cons_parts rightFinal with
        ⟨rightHeadDecision, rightTailDecisions, rightHeadFinal, rightTailFinal,
          rightDecisionsShape⟩
      have headEqual := final_reference_slot_decision_agrees headAgreement
        leftHeadFinal rightHeadFinal
      have tailEqual := ih leftTailFinal rightTailFinal
      rw [leftDecisionsShape, rightDecisionsShape, headEqual, tailEqual]

/-- Test whether one final selected-slot list contains a committed leader. -/
def orderedDecisionsHaveCommit {BlockId : Type} :
    List (OrderedSelectedSlotDecision BlockId) → Bool
  | [] => false
  | decision :: tail =>
      match decision.decision with
      | .commit _ => true
      | .skip => orderedDecisionsHaveCommit tail

/-- Keep only committed leader references from one ordered final decision list. -/
def committedLeaderRefsFromDecisions {BlockId : Type}
    (decisions : List (OrderedSelectedSlotDecision BlockId)) : List BlockId :=
  decisions.filterMap fun decision =>
    match decision.decision with
    | .commit block => some block
    | .skip => none

/-- One exact commit candidate returned by a local pending-round scan.

Skip decisions are scan state. They are not part of the candidate passed to the
Rust commit builder.
-/
structure ReferenceFlexCandidate (Digest : Type) where
  leaderRound : Nat
  orderedCommittedLeaders : List (LeaderBlockRef Digest)

namespace ReferenceFlexCandidate

/-- Keep the committed leader references in selected leader slot order. -/
def committedLeaderRefs {Digest : Type}
    (candidate : ReferenceFlexCandidate Digest) : List (LeaderBlockRef Digest) :=
  candidate.orderedCommittedLeaders

/-- Combine one exact candidate with local Rust builder material.

The candidate supplies only the ordered committed leaders. The material must
come from the local DAG traversal, deterministic block sort, and timestamp
calculation. Skip decisions do not enter this input.
-/
def toBuilderInput {BlockId CommitId : Type}
    (prior : ExactCommitReference CommitId)
    (material : ExactCommitBuildMaterial (LeaderBlockRef BlockId))
    (candidate : ReferenceFlexCandidate BlockId) :
    ExactCommitBuilderInput CommitId (LeaderBlockRef BlockId) :=
  { prior
    index := prior.index + 1
    timestamp := material.timestamp
    orderedCommittedLeaders := candidate.committedLeaderRefs
    namedLeader := material.namedLeader
    sortedCommittedBlocks := material.sortedCommittedBlocks }

/-- Make the exact Rust commit body from a candidate and its local material. -/
def toCommitRecord {BlockId CommitId : Type}
    (prior : ExactCommitReference CommitId)
    (material : ExactCommitBuildMaterial (LeaderBlockRef BlockId))
    (candidate : ReferenceFlexCandidate BlockId) :
    ExactCommitRecord CommitId (LeaderBlockRef BlockId) :=
  (candidate.toBuilderInput prior material).toCommitRecord

end ReferenceFlexCandidate

/-- A one-validator source map for the Rust data that is not in selected-slot
decisions.

`localMaterial` must map one local DFS, sort, and timestamp calculation.
Completeness says that this local calculation has all required exact blocks and
the prior committed state. A complete local calculation maps to the material
that is determined by the prior reference and exact candidate. No field compares
two validators or assumes a common future commit.

The Rust refinement must prove this map. In particular, it must bind the prior
digest to one prior commit body and history, map the full ancestor closure above
GC, use the same committee, and prove deterministic timestamp and block order.
The current Rust block sort has no full-reference tie-break after its hash key.
Before `completeMapsToCanonical` can be instantiated, the Rust refinement must
prove one of these conditions:

* equal sort keys keep the same DFS input order at every correct host;
* distinct committed block references do not have equal sort keys; or
* Rust adds a deterministic full-reference tie-break.

Without one of these conditions, equal committed block sets can produce
different ordered commit bodies.
-/
structure ReferenceCommitMaterializerSourceMap
    (LocalView BlockId CommitId : Type) where
  localMaterial : LocalView → ExactCommitReference CommitId →
    ReferenceFlexCandidate BlockId →
      ExactCommitBuildMaterial (LeaderBlockRef BlockId)
  canonicalMaterial : ExactCommitReference CommitId →
    ReferenceFlexCandidate BlockId →
      ExactCommitBuildMaterial (LeaderBlockRef BlockId)
  complete : LocalView → ExactCommitReference CommitId →
    ReferenceFlexCandidate BlockId → Prop
  completeMapsToCanonical : ∀ view prior candidate,
    complete view prior candidate →
      localMaterial view prior candidate = canonicalMaterial prior candidate

namespace ReferenceCommitMaterializerSourceMap

/-- Make one local exact Rust builder input. -/
def localInput
    {LocalView BlockId CommitId : Type}
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockId CommitId)
    (view : LocalView) (prior : ExactCommitReference CommitId)
    (candidate : ReferenceFlexCandidate BlockId) :
    ExactCommitBuilderInput CommitId (LeaderBlockRef BlockId) :=
  candidate.toBuilderInput prior (mapping.localMaterial view prior candidate)

/-- Make the canonical input for one prior reference and exact candidate. -/
def canonicalInput
    {LocalView BlockId CommitId : Type}
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockId CommitId)
    (prior : ExactCommitReference CommitId)
    (candidate : ReferenceFlexCandidate BlockId) :
    ExactCommitBuilderInput CommitId (LeaderBlockRef BlockId) :=
  candidate.toBuilderInput prior (mapping.canonicalMaterial prior candidate)

/-- One complete local materialization gives the canonical builder input. -/
theorem complete_local_input_is_canonical
    {LocalView BlockId CommitId : Type}
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockId CommitId)
    {view : LocalView} {prior : ExactCommitReference CommitId}
    {candidate : ReferenceFlexCandidate BlockId}
    (complete : mapping.complete view prior candidate) :
    mapping.localInput view prior candidate =
      mapping.canonicalInput prior candidate := by
  unfold localInput canonicalInput
  rw [mapping.completeMapsToCanonical view prior candidate complete]

/-- Two complete local calculations agree after their prior references and
exact candidates have been proved equal. -/
theorem complete_local_inputs_agree
    {LocalView BlockId CommitId : Type}
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockId CommitId)
    {leftView rightView : LocalView}
    {leftPrior rightPrior : ExactCommitReference CommitId}
    {leftCandidate rightCandidate : ReferenceFlexCandidate BlockId}
    (samePrior : leftPrior = rightPrior)
    (sameCandidate : leftCandidate = rightCandidate)
    (leftComplete : mapping.complete leftView leftPrior leftCandidate)
    (rightComplete : mapping.complete rightView rightPrior rightCandidate) :
    mapping.localInput leftView leftPrior leftCandidate =
      mapping.localInput rightView rightPrior rightCandidate := by
  subst rightPrior
  subst rightCandidate
  rw [mapping.complete_local_input_is_canonical leftComplete]
  rw [mapping.complete_local_input_is_canonical rightComplete]

end ReferenceCommitMaterializerSourceMap

/-- Scan pending rounds in order. A round with an undecided slot blocks the scan.
A final all-skip round is bypassed. The first final round with a commit supplies
the exact candidate. -/
def findReferenceFlexCommitCandidate {Digest : Type} :
    List (ReferenceFlexRoundView Digest) → Option (ReferenceFlexCandidate Digest)
  | [] => none
  | round :: tail =>
      match finalReferenceDecisions? round.selectedSlots with
      | none => none
      | some decisions =>
          if orderedDecisionsHaveCommit decisions then
            some {
              leaderRound := round.round
              orderedCommittedLeaders :=
                committedLeaderRefsFromDecisions decisions }
          else
            findReferenceFlexCommitCandidate tail

/-- Two successful local FlexCommitter scans return the same exact candidate.

This is a conditional composition lemma. Its input is the named selected-slot
safety goal. It does not take a common candidate as an input. -/
theorem reference_flex_commit_candidates_agree
    {Digest : Type}
    {left right : List (ReferenceFlexRoundView Digest)}
    (agreement : ExactPrefixAgreement CrossViewExactRoundAgreement left right)
    {leftCandidate rightCandidate : ReferenceFlexCandidate Digest}
    (leftFound :
      findReferenceFlexCommitCandidate left = some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate right = some rightCandidate) :
    leftCandidate = rightCandidate := by
  induction agreement generalizing leftCandidate rightCandidate with
  | leftNil => simp [findReferenceFlexCommitCandidate] at leftFound
  | rightNil => simp [findReferenceFlexCommitCandidate] at rightFound
  | @cons leftRound rightRound leftTail rightTail roundAgreement tailAgreement ih =>
      rcases roundAgreement with ⟨sameRound, slotAgreement⟩
      cases leftFinal : finalReferenceDecisions? leftRound.selectedSlots with
      | none => simp [findReferenceFlexCommitCandidate, leftFinal] at leftFound
      | some leftDecisions =>
          cases rightFinal : finalReferenceDecisions? rightRound.selectedSlots with
          | none => simp [findReferenceFlexCommitCandidate, rightFinal] at rightFound
          | some rightDecisions =>
              have decisionsEqual := final_reference_decisions_agree slotAgreement
                leftFinal rightFinal
              subst rightDecisions
              cases hasCommit : orderedDecisionsHaveCommit leftDecisions with
              | false =>
                  simp [findReferenceFlexCommitCandidate, leftFinal, rightFinal,
                    hasCommit] at leftFound rightFound
                  exact ih leftFound rightFound
              | true =>
                  simp [findReferenceFlexCommitCandidate, leftFinal, rightFinal,
                    hasCommit] at leftFound rightFound
                  rw [← leftFound, ← rightFound]
                  rw [ReferenceFlexCandidate.mk.injEq]
                  exact ⟨sameRound, rfl⟩

/-- The exact committed leader reference lists agree when both local scans return
a candidate. -/
theorem reference_flex_committed_leader_refs_agree
    {Digest : Type}
    {left right : List (ReferenceFlexRoundView Digest)}
    (agreement : ExactPrefixAgreement CrossViewExactRoundAgreement left right)
    {leftCandidate rightCandidate : ReferenceFlexCandidate Digest}
    (leftFound :
      findReferenceFlexCommitCandidate left = some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate right = some rightCandidate) :
    leftCandidate.committedLeaderRefs = rightCandidate.committedLeaderRefs := by
  have candidatesEqual := reference_flex_commit_candidates_agree agreement
    leftFound rightFound
  rw [candidatesEqual]

/-- Separate local scans produce equal exact Rust builder inputs after each
one-validator materializer has a complete local causal view.

`samePrior` is the induction hypothesis for one commit index. The completeness
facts are local source-map facts. They do not state a common future output.
-/
theorem reference_flex_builder_inputs_agree
    {LocalView BlockId CommitId : Type}
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockId CommitId)
    {leftView rightView : LocalView}
    {leftState rightState : ReferenceFlexState BlockId}
    (agreement : CrossViewReferenceFlexAgreement leftState rightState)
    {leftCandidate rightCandidate : ReferenceFlexCandidate BlockId}
    (leftFound :
      findReferenceFlexCommitCandidate leftState.rounds = some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate rightState.rounds = some rightCandidate)
    {leftPrior rightPrior : ExactCommitReference CommitId}
    (samePrior : leftPrior = rightPrior)
    (leftComplete : mapping.complete leftView leftPrior leftCandidate)
    (rightComplete : mapping.complete rightView rightPrior rightCandidate) :
    mapping.localInput leftView leftPrior leftCandidate =
      mapping.localInput rightView rightPrior rightCandidate := by
  have candidatesEqual := reference_flex_commit_candidates_agree agreement.rounds
    leftFound rightFound
  exact mapping.complete_local_inputs_agree samePrior candidatesEqual
    leftComplete rightComplete

/-- The deterministic reference builder returns the same next exact reference
after two separate local scans satisfy selected-slot agreement.

This theorem is not the final system safety theorem. Cross-view selected-slot
agreement remains an open upstream proof goal. -/
theorem reference_flex_built_references_agree
    {LocalView BlockId CommitId Encoding : Type}
    (functions : CommitReferenceFunctions
      CommitId (LeaderBlockRef BlockId) Encoding)
    (mapping : ReferenceCommitMaterializerSourceMap
      LocalView BlockId CommitId)
    {leftView rightView : LocalView}
    {leftState rightState : ReferenceFlexState BlockId}
    (agreement : CrossViewReferenceFlexAgreement leftState rightState)
    {leftCandidate rightCandidate : ReferenceFlexCandidate BlockId}
    (leftFound :
      findReferenceFlexCommitCandidate leftState.rounds = some leftCandidate)
    (rightFound :
      findReferenceFlexCommitCandidate rightState.rounds = some rightCandidate)
    {leftPrior rightPrior : ExactCommitReference CommitId}
    (samePrior : leftPrior = rightPrior)
    (leftComplete : mapping.complete leftView leftPrior leftCandidate)
    (rightComplete : mapping.complete rightView rightPrior rightCandidate) :
    constructExactCommitReferenceFromInput functions
        (mapping.localInput leftView leftPrior leftCandidate) =
      constructExactCommitReferenceFromInput functions
        (mapping.localInput rightView rightPrior rightCandidate) := by
  have inputsEqual := reference_flex_builder_inputs_agree mapping agreement
    leftFound rightFound samePrior leftComplete rightComplete
  rw [inputsEqual]

end Mysticeti
