# Verus: Stress Test the Spec

Actively attack the spec to find vacuity, under-specification, and mis-specification. Run this after the proof verifies — a green Verus run only confirms the implementation satisfies the spec; it says nothing about whether the spec is strong enough to matter.

## 1. Vacuity detection

A property is *vacuously satisfied* when it holds for a trivial reason — the antecedent is never triggered. In industrial hardware verification, roughly 20% of specs pass vacuously on first run, and vacuous passes always indicate a real problem.

**How to detect it in Verus:** For each conditional postcondition `P ==> Q`, ask: is `P` ever true? Try to construct a concrete scenario (a `proof` block or test) where `P` holds. If you cannot, the clause is vacuous — it constrains nothing.

A fast mechanical check: temporarily replace the antecedent with `false` and confirm that Verus now rejects the spec. If it still passes, the antecedent was dead to begin with.

Example: `sig_is_valid(&sig, committee) ==> self.has_voted(authority)` is vacuous if `sig_is_valid` is never true in any execution that reaches this point.

## 2. Spec mutation: weaken postconditions and check that Verus objects

Deliberately introduce weaknesses into the spec and verify that the prover detects them. If a weakened spec still passes, the original spec was not strong enough to rule out that weakness.

Mutations to try:
- Change `<=>` to `==>` in a state-transition biconditional — does verification still pass? (It should fail; if it doesn't, the reverse direction was vacuous.)
- Replace a specific postcondition with `true` — does the proof still go through? (It should; check that removing it actually weakened the guarantee.)
- Weaken `out is Failed <==> P` to `out is Failed ==> P` — does the prover object? (If not, the `<==` direction was never needed, which suggests it is not being verified at all.)
- Remove a `forall` monotonicity clause and check that a challenge theorem you derived earlier now fails to prove.

Each surviving mutant — a weakened spec that still verifies — points to a region the proof is not actually checking.

## 3. Model mutation: break the implementation and check that the spec catches it

Introduce deliberate faults into the *implementation* and verify that the *spec* rejects them. If a broken implementation still satisfies your spec, the spec is too weak to catch that class of error.

Faults to try:
- Remove the epoch check in `insert` — does the spec now fail? (It should, via the `envelope_epoch != committee_epoch ==> out is Failed` clause.)
- Make `insert_generic` unconditionally return `Failed` — does the invariant postcondition catch it?
- Corrupt the `total_votes` update (e.g. add 1 instead of `weight`) — does `invariant_holds()` in `ensures` catch it?
- Skip the `insert_new` call — does the state-transition biconditional catch it?

After each fault, run `bash scripts/verus-check.sh`. A fault that does not cause a verification failure is a fault your spec cannot detect.

## 4. Animate against concrete scenarios

Before trusting a spec, verify it against executions you fully understand.

Write `proof` blocks (or Rust unit tests for the exec behavior) that:
- Construct a clearly correct execution and assert the postconditions hold
- Construct a clearly *incorrect* execution and assert that at least one postcondition is violated

In Verus, concrete animation looks like:

```rust
proof fn check_insert_epoch_mismatch() {
    // Construct a scenario where epochs differ and confirm Failed is the only possible outcome.
    // If this proof goes through vacuously (no contradiction), the spec is not ruling out the bad state.
}
```

If you cannot write the "incorrect execution" proof block because Verus cannot even express the scenario, that is a sign the spec is under-constrained at the type level.

## 5. Treat proof failures as spec validators

When Verus reports a postcondition not satisfied, do not immediately fix the implementation. First ask: **is this counterexample actually a bad execution?**

If the failing execution represents something your spec forbids but which is actually correct behavior, the bug is in the spec, not the system. Spurious proof failures are the most reliable way to find mis-specifications — the SMT solver is implicitly testing whether the spec matches your intent every time it finds a violating trace.

## Checklist

- [ ] Every conditional postcondition (`P ==> Q`) has been checked for vacuity — there exists a reachable execution where `P` holds
- [ ] At least three postcondition mutations were tried; each caused a verification failure
- [ ] At least three implementation faults were tried; each caused a verification failure
- [ ] At least one correct and one incorrect concrete execution have been animated
- [ ] Any proof failure encountered during development was examined as a potential spec error before being treated as an implementation bug
