/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.KnownReferenceReplayBridge

namespace Mysticeti

/-!
Propagation of one already installed exact commit reference.

`KnownReferenceReplayBridge` derives propagation from exact install safety and
the explicit local replay-manifest path. This file keeps the small logical
counterexample that explains why one installed witness alone is insufficient.
-/

/-- The counterexample has one commit identifier and two validators. -/
def witnessOnlyInstalled (validator index : Nat) : Option Nat :=
  if index = 0 then some 0
  else if validator = 0 ∧ index = 1 then some 7
  else none

/-- The counterexample is stable at every time. -/
def witnessOnlyTrace : Time → Nat → Nat → Option Nat :=
  fun _time => witnessOnlyInstalled

theorem witness_only_trace_has_next_reference :
    witnessOnlyTrace 0 0 1 = some 7 := by
  simp [witnessOnlyTrace, witnessOnlyInstalled]

theorem witness_only_trace_is_exact_at_next_index :
    ∀ time validator commitId,
      witnessOnlyTrace time validator 1 = some commitId → commitId = 7 := by
  intro time validator commitId installed
  simp [witnessOnlyTrace, witnessOnlyInstalled] at installed
  rcases installed with ⟨rfl, rfl⟩
  rfl

theorem witness_only_trace_never_reaches_second_validator :
    ∀ time, witnessOnlyTrace time 1 1 = none := by
  intro time
  simp [witnessOnlyTrace, witnessOnlyInstalled]

/-- With unit stake and threshold two, installed witnesses have weight one. -/
theorem witness_only_trace_has_no_quorum_of_installed_witnesses :
    (2 : Nat) >
      weight 2 (fun _validator => 1)
        (fun validator => (witnessOnlyTrace 0 validator 1).isSome) := by
  decide

/-- One exact installed witness and same-index safety do not imply that every
validator installs the reference. The missing fact is a propagation path. -/
theorem installed_witness_and_exact_safety_do_not_imply_propagation :
    witnessOnlyTrace 0 0 1 = some 7 ∧
      (∀ time validator commitId,
        witnessOnlyTrace time validator 1 = some commitId → commitId = 7) ∧
      ¬(∀ validator, validator < 2 →
        ∃ finish, witnessOnlyTrace finish validator 1 = some 7) := by
  refine ⟨witness_only_trace_has_next_reference,
    witness_only_trace_is_exact_at_next_index, ?_⟩
  intro propagated
  rcases propagated 1 (by omega) with ⟨finish, installed⟩
  rw [witness_only_trace_never_reaches_second_validator finish] at installed
  contradiction

end Mysticeti
