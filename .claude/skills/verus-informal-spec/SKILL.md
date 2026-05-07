# Verus: Write the Informal Spec

Write the algebraic model and behavioral contract before touching any Verus syntax.

## Why this comes first

A proof is only as useful as its spec. A weak or wrong spec produces a proof that says nothing meaningful. Getting the spec right first saves far more time than it costs.

## Step 1: Define the abstract state

Describe the type's state as a mathematical object, ignoring implementation details.

- What does this type *represent* (not what data structure it uses)?
- Express it as a set, map, sequence, or simple predicate over those.
- Example: `StakeAggregator` is `{ voted: Set<Authority>, committee: Committee }` — not "a HashMap plus a counter."

The abstract state should be expressible as a Verus `spec fn` on `self`. Name it something like `has_voted`, `invariant_holds`, or `all_sigs_valid`.

## Step 2: Describe each operation as a state transition

For the function you're verifying, write:

- **Pre-state**: what must be true before the call (preconditions)
- **Post-state**: what is true after the call (postconditions)
- **Return value**: what each variant of the return value means about the state

Write this as pure math first. Example:

```
insert(agg, authority, sig):
  pre:  agg.invariant_holds()
  post: agg.voted == old(agg.voted) ∪ {authority}
        agg.total_votes == Σ weight(a) for a ∈ agg.voted
  returns:
    Failed      ⟺  authority ∈ old(agg.voted)  ∨  weight(authority) = 0
    QuorumReached ⟺  authority ∉ old(agg.voted)  ∧  weight > 0  ∧  new_sum ≥ threshold
    NotEnoughVotes is the remaining case
```

## Step 3: State the key invariants

What properties must hold before and after every public operation?

- These become `requires invariant_holds()` and `ensures invariant_holds()`.
- Example: `total_votes == Σ weight(a) for a ∈ data.dom()` is a sum invariant.

## Step 4: Decompose into safety and liveness

Every behavioral property is either *safety* ("nothing bad ever happens") or *liveness* ("something good eventually happens"). A spec that addresses only one class is almost certainly incomplete.

- **Safety**: What states are forbidden? What invariants must always hold? What can never be returned?
- **Liveness**: What must eventually become true? (In a synchronous setting this often means: given a valid sequence of inputs, is there a path to each outcome?)

A common incompleteness pattern is a spec that rules out all bad behaviors but accidentally permits a system that does nothing — which vacuously satisfies every safety property. Explicitly asking "what must eventually be *possible*?" catches this.

## Step 5: State semantic guarantees beyond state transitions

These are the properties callers actually care about:

- **Monotonicity**: does the function ever lose previously-established facts? If not, say so.
- **Commutativity**: does order of operations matter? If not, prove it.
- **Value preservation**: are stored values ever silently overwritten? If not, say so.

If you can state these now, they become the most valuable part of the spec.

## Step 6: Prefer declarative statements over operational ones

A spec is easier to trust when a reader can see immediately that it says the right thing, without simulating execution. Prefer stating *what is true* over *how the system proceeds*:

- "The lock is held by at most one process" is obviously correct.
- "The acquire/release protocol executes steps A, B, C" requires the reader to verify the protocol is correct — which is what you're trying to avoid.

If your spec clause reads like pseudocode, it is probably at the wrong level of abstraction. Step back and ask: what *property* does that pseudocode achieve?

## Checklist before moving on

- [ ] The abstract state is named and defined as a spec predicate, not in terms of the concrete data structure
- [ ] The state transition is written as a biconditional (⟺), not just one direction
- [ ] The invariant is identified and will appear in both `requires` and `ensures`
- [ ] Both safety properties (what is forbidden) and liveness properties (what must be possible) are addressed
- [ ] At least one of monotonicity / commutativity / value-preservation is stated
- [ ] Spec clauses state what is true, not how the code achieves it
- [ ] The spec describes what callers need to know, not what the implementation does

Only proceed to `/verus-check-spec` once this checklist is complete.
