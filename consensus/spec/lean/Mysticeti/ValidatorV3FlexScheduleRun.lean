/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorV3AdaptiveScheduleFixpoint

namespace Mysticeti

/-!
The modeled FlexCommitter loop produces a consistent adaptive-schedule run.

This module separates the local Rust facts that are needed for the result.
It does not put `ConsistentRun` in a structure field. It also does not compare
two hosts.

The schedule state is indexed by leader round. The state at round `r` contains
the commits whose candidate rounds are less than `r`. A round without a
candidate keeps the state. A round with a candidate applies the modeled
`LeaderScheduleV3::add_commit` step. Induction therefore reconstructs the
schedule from the earlier round verdicts.

The decision side is separate. The selected order reads the
`allowed_leaders` vector at the governing gate and applies the Rust round
shuffle. The modeled direct and indirect passes produce the final selected-slot
list. The round verdict is `findReferenceFlexCommitCandidate` on that one
round.

The result is for one host and one set of modeled functions. This file does not
prove that different local DAG views use the same `commitMaterial` and
`postScanSlots` functions. The proof that all correct hosts use one common model
remains open.
-/

/-- Apply the pending-round candidate scan to one modeled round. A Rust source
map must also justify the projection from the full pending prefix to these
per-round verdicts. -/
def findReferenceFlexRoundCandidate {BlockId : Type} (round : Nat)
    (selectedSlots : List (ReferenceSelectedSlotView BlockId)) :
    V3RoundVerdict BlockId :=
  findReferenceFlexCommitCandidate [{ round, selectedSlots }]

/-- Pure functions that define one adaptive FlexCommitter rule.

`commitMaterial` is the exact material passed to the modeled
`LeaderScheduleV3::add_commit` step after a round returns a candidate.
`postScanSlots` is the final selected-slot list after the modeled direct and
indirect passes. `gateOf` is the schedule gate that governs a round. These are
functions, not claims about a host. A Rust source map must show that each local
call uses these functions. -/
structure V3FlexScheduleRunModel (BlockId CommitId : Type) where
  parameters : V3ReplayParameters BlockId CommitId
  /-- The `min_next_leader_round` gate that governs each scanned round. -/
  gateOf : Nat → Nat
  /-- Rust does not use a schedule above the round that it governs. -/
  gateNotAbove : ∀ round, gateOf round ≤ round
  commitMaterial : Nat → V3ScheduleState BlockId CommitId →
    ReferenceFlexCandidate BlockId → V3CommitMaterial BlockId CommitId
  postScanSlots : Nat → List Nat →
    List (ReferenceSelectedSlotView BlockId)

namespace V3FlexScheduleRunModel

variable {BlockId CommitId : Type} [DecidableEq BlockId]

/-- Apply the schedule effect of one round verdict. A round without a commit
does not call `LeaderScheduleV3::add_commit`. -/
def advance (model : V3FlexScheduleRunModel BlockId CommitId) (round : Nat)
    (state : V3ScheduleState BlockId CommitId) :
    V3RoundVerdict BlockId → V3ScheduleState BlockId CommitId
  | none => state
  | some candidate =>
      model.parameters.replay.addCommit state
        (model.commitMaterial round state candidate)

/-- Replay the modeled `LeaderScheduleV3` state through all rounds below one
gate. -/
def replayState (model : V3FlexScheduleRunModel BlockId CommitId)
    (run : Nat → V3RoundVerdict BlockId) :
    Nat → V3ScheduleState BlockId CommitId
  | 0 => model.parameters.replay.initialState
  | round + 1 => model.advance round (model.replayState run round) (run round)

omit [DecidableEq BlockId] in
/-- The replay state at a gate reads only verdicts below that gate. -/
theorem replayState_eq_of_eq_below
    (model : V3FlexScheduleRunModel BlockId CommitId)
    {left right : Nat → V3RoundVerdict BlockId} {gate : Nat}
    (sameBelow : ∀ round, round < gate → left round = right round) :
    model.replayState left gate = model.replayState right gate := by
  induction gate with
  | zero => rfl
  | succ gate inductionHypothesis =>
      have sameEarlier : ∀ round, round < gate → left round = right round := by
        intro round lower
        exact sameBelow round (by omega)
      have sameAtGate : left gate = right gate :=
        sameBelow gate (by omega)
      simp only [replayState]
      rw [inductionHypothesis sameEarlier, sameAtGate]

/-- The ordered allowed-leader vector after replaying all verdicts below one
gate. -/
def scheduleFrom (model : V3FlexScheduleRunModel BlockId CommitId)
    (run : Nat → V3RoundVerdict BlockId) (gate : Nat) : List Nat :=
  model.parameters.replay.allowedLeaders (model.replayState run gate)

/-- The modeled per-round FlexCommitter decision. It applies the Rust round
shuffle to the allowed-leader vector, runs the modeled decision passes, and
scans the resulting selected slots. -/
def decide (model : V3FlexScheduleRunModel BlockId CommitId) (round : Nat)
    (allowedLeaders : List Nat) : V3RoundVerdict BlockId :=
  findReferenceFlexRoundCandidate round
    (model.postScanSlots round
      (model.parameters.shuffle allowedLeaders round))

/-- The concrete adaptive rule for the modeled Rust functions. Replaying
through a Rust gate includes exactly the candidate rounds below that gate. -/
def rule (model : V3FlexScheduleRunModel BlockId CommitId) :
    V3AdaptiveScheduleRule (V3RoundVerdict BlockId) :=
  { scheduleFrom := model.scheduleFrom
    readsBelowGate := by
      intro left right gate sameBelow
      unfold scheduleFrom
      rw [model.replayState_eq_of_eq_below sameBelow]
    gateOf := model.gateOf
    gateNotAbove := model.gateNotAbove
    decide := model.decide }

end V3FlexScheduleRunModel

/-! ### One-host source facts -/

/-- Local data and local transition facts for one modeled host.

Each fact maps one small Rust action. No field states that `roundVerdicts` is a
consistent run, and no field refers to another host.

* `startsFromInitialSchedule` maps `LeaderScheduleV3::new`.
* `noCandidatePreservesSchedule` maps a round that does not call `add_commit`.
* `candidateFeedsAddCommit` maps the material from one successful commit into
  `LeaderScheduleV3::add_commit`.
* `selectedOrderReadsSchedule` maps
  `PendingCommitState::get_or_create_round_state`.
* `decisionPassProducesSlot` maps one position after the local direct and
  indirect decision passes update that position.
-/
structure V3FlexScheduleHost
    (BlockId CommitId : Type) [DecidableEq BlockId]
    (model : V3FlexScheduleRunModel BlockId CommitId) where
  scheduleState : Nat → V3ScheduleState BlockId CommitId
  selectedOrder : Nat → List Nat
  postScanSlots : Nat → List (ReferenceSelectedSlotView BlockId)
  startsFromInitialSchedule :
    scheduleState 0 = model.parameters.replay.initialState
  noCandidatePreservesSchedule : ∀ round,
    findReferenceFlexRoundCandidate round (postScanSlots round) = none →
      scheduleState (round + 1) = scheduleState round
  candidateFeedsAddCommit : ∀ round candidate,
    findReferenceFlexRoundCandidate round (postScanSlots round) =
        some candidate →
      scheduleState (round + 1) =
        model.parameters.replay.addCommit (scheduleState round)
          (model.commitMaterial round (scheduleState round) candidate)
  selectedOrderReadsSchedule : ∀ round,
    selectedOrder round =
      model.parameters.shuffle
        (model.parameters.replay.allowedLeaders
          (scheduleState (model.gateOf round))) round
  decisionPassProducesSlot : ∀ (round index : Nat),
    (postScanSlots round)[index]? =
      (model.postScanSlots round (selectedOrder round))[index]?

namespace V3FlexScheduleHost

variable {BlockId CommitId : Type} [DecidableEq BlockId]
variable {model : V3FlexScheduleRunModel BlockId CommitId}

/-- Extract the host's final verdict for one leader round from its local
post-scan selected slots. -/
def roundVerdicts (host : V3FlexScheduleHost BlockId CommitId model) :
    Nat → V3RoundVerdict BlockId :=
  fun round => findReferenceFlexRoundCandidate round (host.postScanSlots round)

/-- The two local schedule-update cases combine into the modeled round step. -/
theorem scheduleState_succ
    (host : V3FlexScheduleHost BlockId CommitId model) (round : Nat) :
    host.scheduleState (round + 1) =
      model.advance round (host.scheduleState round)
        (host.roundVerdicts round) := by
  unfold roundVerdicts
  cases found : findReferenceFlexRoundCandidate round
      (host.postScanSlots round) with
  | none =>
      rw [host.noCandidatePreservesSchedule round found]
      simp [V3FlexScheduleRunModel.advance]
  | some candidate =>
      rw [host.candidateFeedsAddCommit round candidate found]
      simp [V3FlexScheduleRunModel.advance]

/-- The host schedule state is derived from its earlier round verdicts.

This is the main composition step. It follows by induction from the initial,
no-candidate, and add-commit facts. It is not a structure field. -/
theorem scheduleState_eq_replayState
    (host : V3FlexScheduleHost BlockId CommitId model) (gate : Nat) :
    host.scheduleState gate = model.replayState host.roundVerdicts gate := by
  induction gate with
  | zero =>
      simpa [V3FlexScheduleRunModel.replayState] using
        host.startsFromInitialSchedule
  | succ round inductionHypothesis =>
      rw [host.scheduleState_succ]
      simp only [V3FlexScheduleRunModel.replayState]
      rw [inductionHypothesis]

/-- The pointwise local decision-pass fact gives the complete post-scan list.
The list equality is derived and is not a structure field. -/
theorem postScanSlots_eq
    (host : V3FlexScheduleHost BlockId CommitId model) (round : Nat) :
    host.postScanSlots round =
      model.postScanSlots round (host.selectedOrder round) := by
  apply List.ext_getElem?
  exact host.decisionPassProducesSlot round

/-- The host's extracted round verdicts form a consistent run of the concrete
adaptive FlexCommitter rule.

The proof first reconstructs the schedule state from earlier verdicts. It then
uses the local round shuffle and the derived pointwise result for the decision
pass. The candidate scan reads the resulting slots. -/
theorem consistentRun
    (host : V3FlexScheduleHost BlockId CommitId model) :
    model.rule.ConsistentRun host.roundVerdicts := by
  intro round
  change host.roundVerdicts round =
    model.decide round
      (model.scheduleFrom host.roundVerdicts (model.gateOf round))
  unfold V3FlexScheduleRunModel.decide V3FlexScheduleRunModel.scheduleFrom
  rw [← host.scheduleState_eq_replayState (model.gateOf round)]
  unfold roundVerdicts
  rw [← host.selectedOrderReadsSchedule round]
  rw [← host.postScanSlots_eq round]

end V3FlexScheduleHost

end Mysticeti
