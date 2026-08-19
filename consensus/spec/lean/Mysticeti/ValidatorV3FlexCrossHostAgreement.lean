/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorV3FlexScheduleRun

namespace Mysticeti

/-! Cross-host agreement on the adaptive FlexCommitter run.

`V3AdaptiveScheduleRule.consistent_runs_are_equal` compares two consistent runs
of one rule. It does not compare two hosts, because a host supplies its own
model functions. This module states the cross-host step at the level that the
protocol actually supports.

Full verdict equality is the wrong target. The Flex round scan stops at the
first undecided selected slot, so a host with less evidence returns no candidate
where a better informed host returns one. Their verdicts differ at that moment,
and a theorem that claimed otherwise would be false of a real execution.

The right statement is agreement where both hosts have decided. The per-slot
safety result gives exactly that: two valid views cannot hold conflicting final
results for one selected slot. `final_reference_slot_decision_agrees` is the
slot form. This module lifts it to the round verdict and then to the run.

The remaining condition is `CrossViewExactSlotAgreement` for each selected slot,
which is the named cross-view safety goal of `ReferenceFlexCommitter`. It is not
discharged here.
-/

variable {BlockId : Type}

/-- Two hosts that agree on a round's selected slots produce the same commit
candidate, whenever both of them produce one.

The round scan reads one round view, so this is the one-round case of
`reference_flex_commit_candidates_agree`. -/
theorem round_candidates_agree {round : Nat}
    {leftSlots rightSlots : List (ReferenceSelectedSlotView BlockId)}
    (slots : ExactListAgreement CrossViewExactSlotAgreement leftSlots rightSlots)
    {leftCandidate rightCandidate : ReferenceFlexCandidate BlockId}
    (leftFound :
      findReferenceFlexRoundCandidate round leftSlots = some leftCandidate)
    (rightFound :
      findReferenceFlexRoundCandidate round rightSlots = some rightCandidate) :
    leftCandidate = rightCandidate :=
  reference_flex_commit_candidates_agree
    (left := [{ round := round, selectedSlots := leftSlots }])
    (right := [{ round := round, selectedSlots := rightSlots }])
    (ExactPrefixAgreement.cons ⟨rfl, slots⟩ ExactPrefixAgreement.leftNil)
    leftFound rightFound

/-- Two hosts agree on every round verdict when their selected slots agree and
neither host is ahead of the other in deciding.

`bothDecide` is the honest extra condition. It says the two hosts have reached
the same decision state at the round. It does not say that they hold the same
blocks, and it does not compare their schedules. A lagging host simply fails
this condition until it decides. -/
theorem round_verdicts_agree
    {leftSlots rightSlots : Nat → List (ReferenceSelectedSlotView BlockId)}
    (slots : ∀ round,
      ExactListAgreement CrossViewExactSlotAgreement (leftSlots round)
        (rightSlots round))
    (bothDecide : ∀ round,
      (findReferenceFlexRoundCandidate round (leftSlots round)).isSome =
        (findReferenceFlexRoundCandidate round (rightSlots round)).isSome)
    (round : Nat) :
    findReferenceFlexRoundCandidate round (leftSlots round) =
      findReferenceFlexRoundCandidate round (rightSlots round) := by
  have decided := bothDecide round
  cases leftFound :
      findReferenceFlexRoundCandidate round (leftSlots round) with
  | none =>
      cases rightFound :
          findReferenceFlexRoundCandidate round (rightSlots round) with
      | none => rfl
      | some _ => rw [leftFound, rightFound] at decided; exact absurd decided (by simp)
  | some leftCandidate =>
      cases rightFound :
          findReferenceFlexRoundCandidate round (rightSlots round) with
      | none => rw [leftFound, rightFound] at decided; exact absurd decided (by simp)
      | some rightCandidate =>
          rw [round_candidates_agree (slots round) leftFound rightFound]

/-- Two hosts install the same ordered commit candidates.

This is the cross-host form of `v3_committed_candidates_agree`. It needs no
statement about the two hosts' schedules, because agreeing verdicts give
agreeing schedules through the stratified rule. -/
theorem committed_candidates_agree_across_hosts
    {leftSlots rightSlots : Nat → List (ReferenceSelectedSlotView BlockId)}
    (slots : ∀ round,
      ExactListAgreement CrossViewExactSlotAgreement (leftSlots round)
        (rightSlots round))
    (bothDecide : ∀ round,
      (findReferenceFlexRoundCandidate round (leftSlots round)).isSome =
        (findReferenceFlexRoundCandidate round (rightSlots round)).isSome)
    (bound : Nat) :
    v3CommittedCandidates
        (fun round => findReferenceFlexRoundCandidate round (leftSlots round))
        bound =
      v3CommittedCandidates
        (fun round => findReferenceFlexRoundCandidate round (rightSlots round))
        bound := by
  have sameRun :
      (fun round => findReferenceFlexRoundCandidate round (leftSlots round)) =
        fun round => findReferenceFlexRoundCandidate round (rightSlots round) :=
    funext (round_verdicts_agree slots bothDecide)
  rw [sameRun]

end Mysticeti
