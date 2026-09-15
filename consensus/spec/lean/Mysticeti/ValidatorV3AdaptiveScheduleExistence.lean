/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.ValidatorV3AdaptiveScheduleFixpoint

namespace Mysticeti

/-! The v3 adaptive schedule rule has one consistent run.

The rule for one round can read verdicts from earlier rounds. It still needs a
total verdict function as input. `adaptiveRun` gives the rule the verdicts that
strong recursion has already constructed. It uses one fixed default verdict at
the current round and at later rounds. The stratification condition makes these
default values irrelevant.
-/

namespace V3AdaptiveScheduleRule

variable {Verdict : Type}

/-- Construct the adaptive run by strong recursion on the round.

The `defaultVerdict` value completes the partial run to a total function. The
schedule at the current round reads only values below its gate. The gate is not
above the current round. Thus, the schedule cannot read the default value. -/
noncomputable def adaptiveRun (rule : V3AdaptiveScheduleRule Verdict)
    (defaultVerdict : Verdict) : Nat → Verdict :=
  fun round =>
    Nat.strongRecOn round fun current earlier =>
      rule.decide current
        (rule.scheduleFrom
          (fun queried =>
            if below : queried < current then earlier queried below
            else defaultVerdict)
          (rule.gateOf current))

/-- The constructed run agrees with the adaptive rule at every round. -/
theorem adaptiveRun_consistent (rule : V3AdaptiveScheduleRule Verdict)
    (defaultVerdict : Verdict) :
    rule.ConsistentRun (rule.adaptiveRun defaultVerdict) := by
  intro round
  unfold adaptiveRun Nat.strongRecOn
  rw [WellFounded.fix_eq]
  apply congrArg (rule.decide round)
  apply rule.readsBelowGate
  intro earlier belowGate
  rw [dif_pos (Nat.lt_of_lt_of_le belowGate (rule.gateNotAbove round))]

/-- The rule has a consistent run.

The rule itself supplies the fixed default verdict. Its value is irrelevant,
because the schedule cannot read it. Thus, this theorem needs no `Inhabited`
instance for `Verdict`. -/
theorem adaptive_run_exists (rule : V3AdaptiveScheduleRule Verdict) :
    ∃ run : Nat → Verdict, rule.ConsistentRun run :=
  ⟨rule.adaptiveRun (rule.decide 0 []), rule.adaptiveRun_consistent _⟩

/-- Exactly one run agrees with the adaptive rule at every round. -/
theorem adaptive_run_exists_unique (rule : V3AdaptiveScheduleRule Verdict) :
    ∃! run : Nat → Verdict, rule.ConsistentRun run := by
  refine ⟨rule.adaptiveRun (rule.decide 0 []),
    rule.adaptiveRun_consistent _, ?_⟩
  intro run runConsistent
  exact rule.consistent_runs_are_equal runConsistent
    (rule.adaptiveRun_consistent _)

end V3AdaptiveScheduleRule

end Mysticeti
