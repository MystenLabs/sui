/-
Copyright (c) Mysten Labs, Inc.
SPDX-License-Identifier: Apache-2.0
-/

import Mysticeti.LeaderOrderProbability

namespace Mysticeti

/-!
An axiom-free countable-intersection boundary for leader-order probability.

`LeaderOrderProbability.lean` proves an exact finite geometric failure bound. It
then proves an operational probability-one result for each fixed start round.
This module closes the countable gap. It uses an inductive proof system with the
three rules that probability-one events satisfy:

1. an event with the proved finite-prefix bound has probability one;
2. a larger event also has probability one;
3. a countable intersection of probability-one events has probability one.

The inductive definition has no axiom and does not accept a favorable trace. A
later measure-space model can interpret this proof system. That interpretation
needs only the three rules in `ProbabilityOneInterpretation` below.
-/

/-- Events on the ideal independent uniform first-slot sample space. -/
abbrev UniformFirstSlotEvent (law : IndependentUniformFirstSlotLaw) :=
  UniformFirstSlotTrace law → Prop

/-- Proof terms for events that the finite uniform model justifies as having
probability one. -/
inductive UniformLeaderOrderProbabilityOne
    (law : IndependentUniformFirstSlotLaw) (depth : Nat) :
    UniformFirstSlotEvent law → Prop where
  | ofPrefixBound {start : Nat} {event : UniformFirstSlotEvent law} :
      UniformPrefixProbabilityOne law depth start event →
        UniformLeaderOrderProbabilityOne law depth event
  | mono {left right : UniformFirstSlotEvent law} :
      UniformLeaderOrderProbabilityOne law depth left →
      (∀ trace, left trace → right trace) →
        UniformLeaderOrderProbabilityOne law depth right
  | countableInter (events : Nat → UniformFirstSlotEvent law) :
      (∀ index, UniformLeaderOrderProbabilityOne law depth (events index)) →
        UniformLeaderOrderProbabilityOne law depth
          (fun trace => ∀ index, events index trace)

/-- One trace has a favorable `depth + 1` first-slot window after every round. -/
def UniformFavorableWindowsAfterEveryRound
    (law : IndependentUniformFirstSlotLaw) (depth : Nat) :
    UniformFirstSlotEvent law :=
  fun trace => ∀ start,
    EventuallyUniformFavorableWindowAfter law depth start trace

/-- Independent uniform first-slot sampling gives one probability-one tail event
that contains a favorable window after every start round. -/
theorem uniform_favorable_windows_after_every_round_probability_one
    (law : IndependentUniformFirstSlotLaw) (depth : Nat) :
    UniformLeaderOrderProbabilityOne law depth
      (UniformFavorableWindowsAfterEveryRound law depth) := by
  apply UniformLeaderOrderProbabilityOne.countableInter
    (events := fun start =>
      EventuallyUniformFavorableWindowAfter law depth start)
  intro start
  exact UniformLeaderOrderProbabilityOne.ofPrefixBound
    (eventual_uniform_favorable_window_probability_one law depth start)

/-- A proved deterministic implication transfers the one tail event to a
probability-one liveness event. The theorem does not take a favorable trace. -/
theorem uniform_favorable_tail_transfers_to_liveness
    (law : IndependentUniformFirstSlotLaw) (depth : Nat)
    (liveness : UniformFirstSlotEvent law)
    (favorableTailImpliesLiveness : ∀ trace,
      UniformFavorableWindowsAfterEveryRound law depth trace →
        liveness trace) :
    UniformLeaderOrderProbabilityOne law depth liveness := by
  exact UniformLeaderOrderProbabilityOne.mono
    (uniform_favorable_windows_after_every_round_probability_one law depth)
    favorableTailImpliesLiveness

/-! ### Minimal real-probability interpretation

A real probability space must supply only these rules. In a measure library,
`holds event` can mean that the event holds almost everywhere. The first rule is
the bridge from the exact finite cylinder bound. The other two rules are standard
almost-everywhere monotonicity and countable intersection.
-/

/-- The exact rules that interpret the axiom-free proof terms in a real
probability space. -/
structure ProbabilityOneInterpretation
    (law : IndependentUniformFirstSlotLaw) (depth : Nat) where
  holds : UniformFirstSlotEvent law → Prop
  prefixBound : ∀ start event,
    UniformPrefixProbabilityOne law depth start event → holds event
  mono : ∀ {left right},
    holds left → (∀ trace, left trace → right trace) → holds right
  countableInter : ∀ events : Nat → UniformFirstSlotEvent law,
    (∀ index, holds (events index)) →
      holds (fun trace => ∀ index, events index trace)

/-- Every axiom-free probability-one proof is valid in any interpretation that
supplies the three probability rules. -/
theorem UniformLeaderOrderProbabilityOne.sound
    {law : IndependentUniformFirstSlotLaw} {depth : Nat}
    (interpretation : ProbabilityOneInterpretation law depth)
    {event : UniformFirstSlotEvent law}
    (proof : UniformLeaderOrderProbabilityOne law depth event) :
    interpretation.holds event := by
  induction proof with
  | ofPrefixBound prefixBound =>
      exact interpretation.prefixBound _ _ prefixBound
  | mono leftProof implication inductionHypothesis =>
      exact interpretation.mono inductionHypothesis implication
  | countableInter events _ inductionHypotheses =>
      exact interpretation.countableInter events inductionHypotheses

/-- The all-start favorable-window event is valid in every real-probability
interpretation of the finite uniform bounds. -/
theorem interpreted_uniform_favorable_windows_after_every_round
    (law : IndependentUniformFirstSlotLaw) (depth : Nat)
    (interpretation : ProbabilityOneInterpretation law depth) :
    interpretation.holds
      (UniformFavorableWindowsAfterEveryRound law depth) := by
  exact UniformLeaderOrderProbabilityOne.sound interpretation
    (uniform_favorable_windows_after_every_round_probability_one law depth)

end Mysticeti
