/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorV3AdaptiveScheduleRustMap

namespace Mysticeti

/-! The v3 adaptive leader schedule is a well-defined fixpoint.

The v3 schedule is adaptive: `LeaderScheduleV3` recomputes the allowed-leader
vector from committed scores. Leader identity therefore flows upward from the
decisions below, while a decision flows downward from its anchor. Composed
without a restriction, the verdict of a slot could depend on the leader of its
own anchor, whose identity depends on that verdict. Two different schedules
could then each justify themselves, and the model would not say which one a
correct host runs.

Rust already prevents this, and this module states the reason.
`NextCommitLeaderSchedule` carries `min_next_leader_round`, which is one round
above the leader round of the last commit in the pending window.
`FlexCommitter::maybe_refresh_pending_commit_state` asserts that the value is
identical at one commit index, that it moves strictly forward with the index,
and that a schedule change drops every pending round below the new gate. A
schedule therefore governs only rounds at or above its gate, and every commit
that produced it has a leader round below that gate.

The decision at one round then reads a schedule built from strictly earlier
rounds. That stratification makes the recursion well founded, so at most one
run of decisions is consistent with the rule. `adaptive_run_unique` is that
result.

The model here covers the schedule-to-decision cycle only. It does not repeat
the exact-prefix agreement of `ValidatorV3AdaptiveScheduleSafety`, which fixes
the replayed state for one installed commit index.
-/

/-- The stratified schedule rule that `LeaderScheduleV3` and `FlexCommitter`
run together.

`scheduleFrom` is the allowed-leader vector that governs from one gate round
onward. `readsBelowGate` is the stratification: the vector depends only on
decisions at rounds below the gate, which is what `min_next_leader_round`
enforces. `gateOf` selects the gate that governs one round, and `gateNotAbove`
records that a gate never governs a round below itself. -/
structure V3AdaptiveScheduleRule (Verdict : Type) where
  scheduleFrom : (Nat → Verdict) → Nat → List Nat
  readsBelowGate : ∀ (left right : Nat → Verdict) (gate : Nat),
    (∀ round, round < gate → left round = right round) →
      scheduleFrom left gate = scheduleFrom right gate
  gateOf : Nat → Nat
  gateNotAbove : ∀ round, gateOf round ≤ round
  decide : Nat → List Nat → Verdict

namespace V3AdaptiveScheduleRule

variable {Verdict : Type}

/-- One assignment of verdicts to rounds that agrees with the rule everywhere.
Each round uses the schedule that its own gate produces. -/
def ConsistentRun (rule : V3AdaptiveScheduleRule Verdict)
    (run : Nat → Verdict) : Prop :=
  ∀ round, run round =
    rule.decide round (rule.scheduleFrom run (rule.gateOf round))

/-- Two consistent runs agree at every round.

This is the safety half of the fixpoint. No two schedules can each justify
themselves, because the schedule that governs one round reads only rounds below
its gate, and a gate is never above the round it governs. -/
theorem adaptive_run_unique (rule : V3AdaptiveScheduleRule Verdict)
    {left right : Nat → Verdict}
    (leftConsistent : rule.ConsistentRun left)
    (rightConsistent : rule.ConsistentRun right) :
    ∀ round, left round = right round := by
  intro round
  induction round using Nat.strongRecOn with
  | _ round below =>
      have sameSchedule :
          rule.scheduleFrom left (rule.gateOf round) =
            rule.scheduleFrom right (rule.gateOf round) :=
        rule.readsBelowGate left right (rule.gateOf round) fun earlier lower =>
          below earlier (Nat.lt_of_lt_of_le lower (rule.gateNotAbove round))
      rw [leftConsistent round, rightConsistent round, sameSchedule]

/-- Two consistent runs are the same function.

Everything a host derives from its run therefore agrees: the ordered committed
leaders, the installed sequence, the ledger, and the schedule at every round.
The common commit chain of `ASM-SAFE-COMMIT-CHAIN` is a consequence of the
stratified rule and the per-slot decision result. It is not a separate
condition. What remains for that assumption is the refinement obligation that
each correct host's actual behavior is a consistent run of one common rule. -/
theorem consistent_runs_are_equal (rule : V3AdaptiveScheduleRule Verdict)
    {left right : Nat → Verdict}
    (leftConsistent : rule.ConsistentRun left)
    (rightConsistent : rule.ConsistentRun right) :
    left = right :=
  funext (rule.adaptive_run_unique leftConsistent rightConsistent)

/-- Any reading of the run agrees between two consistent runs. The committed
sequence is one such reading. -/
theorem consistent_run_readings_agree {Output : Type}
    (rule : V3AdaptiveScheduleRule Verdict)
    (reading : (Nat → Verdict) → Output)
    {left right : Nat → Verdict}
    (leftConsistent : rule.ConsistentRun left)
    (rightConsistent : rule.ConsistentRun right) :
    reading left = reading right := by
  rw [rule.consistent_runs_are_equal leftConsistent rightConsistent]

/-- The ordered committed leaders that one run produces through a round bound.

`commitAt` reads one round's verdict and returns the committed leader when that
round commits. This is the shape of the sequence a host installs. -/
def committedPrefix {Leader : Type} (commitAt : Verdict → Option Leader)
    (run : Nat → Verdict) (bound : Nat) : List Leader :=
  (List.range bound).filterMap fun round => commitAt (run round)

/-- Sequence agreement. Two consistent runs commit the same ordered leaders
through every bound.

The argument is induction on the round, not an assumption that the prefixes
match. Agreement below a round gives the same schedule at that round, because
the schedule reads only rounds below its gate and the gate never sits above the
round. The same schedule then gives the same verdict. Prefix agreement is the
conclusion at each step, not a hypothesis. -/
theorem committed_prefix_agrees {Leader : Type}
    (rule : V3AdaptiveScheduleRule Verdict)
    (commitAt : Verdict → Option Leader)
    {left right : Nat → Verdict}
    (leftConsistent : rule.ConsistentRun left)
    (rightConsistent : rule.ConsistentRun right)
    (bound : Nat) :
    committedPrefix commitAt left bound =
      committedPrefix commitAt right bound :=
  rule.consistent_run_readings_agree
    (fun run => committedPrefix commitAt run bound) leftConsistent
    rightConsistent

end V3AdaptiveScheduleRule

/-! ### The rule at the model's own commit types

The results above hold for any verdict type. This section fixes the verdict to
the exact commit candidate that a FlexCommitter round scan returns, so sequence
agreement is stated about commits rather than about an abstract reading.
-/

/-- One round's verdict: the exact commit candidate that the round produces, or
none when the round does not commit. -/
abbrev V3RoundVerdict (BlockId : Type) := Option (ReferenceFlexCandidate BlockId)

/-- The exact commit candidates that one run installs through a round bound. -/
def v3CommittedCandidates {BlockId : Type}
    (run : Nat → V3RoundVerdict BlockId) (bound : Nat) :
    List (ReferenceFlexCandidate BlockId) :=
  V3AdaptiveScheduleRule.committedPrefix (fun verdict => verdict) run bound

/-- Sequence agreement at the model's commit types.

Two consistent runs of one stratified rule install the same ordered exact commit
candidates through every bound. Each candidate carries its leader round and its
ordered committed leaders, so the two hosts agree on the commit sequence and on
the leaders inside each commit. -/
theorem v3_committed_candidates_agree {BlockId : Type}
    (rule : V3AdaptiveScheduleRule (V3RoundVerdict BlockId))
    {left right : Nat → V3RoundVerdict BlockId}
    (leftConsistent : rule.ConsistentRun left)
    (rightConsistent : rule.ConsistentRun right)
    (bound : Nat) :
    v3CommittedCandidates left bound = v3CommittedCandidates right bound :=
  rule.committed_prefix_agrees _ leftConsistent rightConsistent bound

/-- The ordered committed leaders of every commit also agree.

`ReferenceFlexCandidate.committedLeaderRefs` is the list that the commit builder
reads, so this is the leader-level form of the same result. -/
theorem v3_committed_leaders_agree {BlockId : Type}
    (rule : V3AdaptiveScheduleRule (V3RoundVerdict BlockId))
    {left right : Nat → V3RoundVerdict BlockId}
    (leftConsistent : rule.ConsistentRun left)
    (rightConsistent : rule.ConsistentRun right)
    (bound : Nat) :
    (v3CommittedCandidates left bound).map
        ReferenceFlexCandidate.committedLeaderRefs =
      (v3CommittedCandidates right bound).map
        ReferenceFlexCandidate.committedLeaderRefs := by
  rw [v3_committed_candidates_agree rule leftConsistent rightConsistent bound]

/-! ### The gate sequence that Rust runs

The abstract rule above takes its stratification as fields. This section builds
those fields from the modeled `LeaderScheduleV3` state, so the gate is the Rust
`min_next_leader_round` and not an assumption of the fixpoint argument.
-/

/-- The schedule sequence along one commit chain, with the gate rules that
`FlexCommitter::maybe_refresh_pending_commit_state` asserts.

`stateAt` is a function of the commit index, which is the assertion that the
schedule and the gate cannot differ at one index. `gateStrictlyIncreases` is the
assertion that the gate only moves forward. `pendingLeaderRoundsBelowGate` is
`min_next_leader_round() = last pending leader round + 1` together with the
strictly increasing leader rounds that `add_commit` asserts. -/
structure V3ScheduleGateSequence (BlockId CommitId : Type) where
  stateAt : Nat → V3ScheduleState BlockId CommitId
  gateStrictlyIncreases : ∀ index,
    (stateAt index).minNextLeaderRound <
      (stateAt (index + 1)).minNextLeaderRound
  pendingLeaderRoundsBelowGate : ∀ index material,
    material ∈ (stateAt index).pendingCommits →
      material.leader.round < (stateAt index).minNextLeaderRound
  /-- The commit index whose schedule governs one round. -/
  governingIndex : Nat → Nat
  /-- A governing schedule never starts above the round that it governs. -/
  governingGateNotAbove : ∀ round,
    (stateAt (governingIndex round)).minNextLeaderRound ≤ round

namespace V3ScheduleGateSequence

variable {BlockId CommitId : Type}

/-- The gate never moves backward. -/
theorem gate_monotone (seq : V3ScheduleGateSequence BlockId CommitId)
    {earlier later : Nat} (ordered : earlier ≤ later) :
    (seq.stateAt earlier).minNextLeaderRound ≤
      (seq.stateAt later).minNextLeaderRound := by
  induction later with
  | zero =>
      have isZero : earlier = 0 := Nat.le_zero.mp ordered
      subst isZero
      exact Nat.le_refl _
  | succ later ih =>
      rcases Nat.eq_or_lt_of_le ordered with same | below
      · subst same
        exact Nat.le_refl _
      · exact Nat.le_trans (ih (by omega))
          (Nat.le_of_lt (seq.gateStrictlyIncreases later))

/-- The stratification, on the modeled Rust state: every commit that feeds the
schedule governing one round sits at a leader round below that round.

This is why the schedule-to-decision recursion has no cycle. A decision at a
round can depend on the schedule, and that schedule depends only on commits
whose leaders are strictly earlier. -/
theorem governing_commits_are_below_round
    (seq : V3ScheduleGateSequence BlockId CommitId)
    {round index : Nat} (earlier : index ≤ seq.governingIndex round)
    {material : V3CommitMaterial BlockId CommitId}
    (pending : material ∈ (seq.stateAt index).pendingCommits) :
    material.leader.round < round := by
  have belowGate := seq.pendingLeaderRoundsBelowGate index material pending
  have gateLe := seq.gate_monotone earlier
  have governs := seq.governingGateNotAbove round
  omega

/-- Build the stratified rule from the gate sequence. `gateNotAbove` is now the
Rust gate rule rather than a field of the fixpoint argument. -/
def toRule {Verdict : Type}
    (seq : V3ScheduleGateSequence BlockId CommitId)
    (scheduleFrom : (Nat → Verdict) → Nat → List Nat)
    (readsBelowGate : ∀ (left right : Nat → Verdict) (gate : Nat),
      (∀ round, round < gate → left round = right round) →
        scheduleFrom left gate = scheduleFrom right gate)
    (decide : Nat → List Nat → Verdict) :
    V3AdaptiveScheduleRule Verdict :=
  { scheduleFrom
    readsBelowGate
    gateOf := fun round =>
      (seq.stateAt (seq.governingIndex round)).minNextLeaderRound
    gateNotAbove := seq.governingGateNotAbove
    decide }

/-- The run that `FlexCommitter` produces under one Rust gate sequence is the
only consistent one. -/
theorem adaptive_run_unique_under_rust_gate {Verdict : Type}
    (seq : V3ScheduleGateSequence BlockId CommitId)
    {scheduleFrom : (Nat → Verdict) → Nat → List Nat}
    {readsBelowGate : ∀ (left right : Nat → Verdict) (gate : Nat),
      (∀ round, round < gate → left round = right round) →
        scheduleFrom left gate = scheduleFrom right gate}
    {decide : Nat → List Nat → Verdict}
    {left right : Nat → Verdict}
    (leftConsistent :
      (seq.toRule scheduleFrom readsBelowGate decide).ConsistentRun left)
    (rightConsistent :
      (seq.toRule scheduleFrom readsBelowGate decide).ConsistentRun right) :
    ∀ round, left round = right round :=
  V3AdaptiveScheduleRule.adaptive_run_unique _ leftConsistent rightConsistent

end V3ScheduleGateSequence

end Mysticeti
