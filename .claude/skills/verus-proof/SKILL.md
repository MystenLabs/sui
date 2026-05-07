# Verus: Write the Spec and Proof

Translate the informal spec into Verus syntax and iterate until zero errors.

## Writing the spec

### Model the formal spec after the informal algebraic spec

The `requires`/`ensures` clauses are a direct translation of the informal algebraic spec, not a description of the implementation.

Take the algebraic model — abstract state, state transitions, invariants, semantic guarantees — and mechanically render each piece into Verus syntax:

- The abstract state predicate becomes an `open spec fn` (e.g. `has_voted`, `invariant_holds`)
- The state transition "voted = old(voted) ∪ {authority}" becomes a `forall` biconditional in `ensures`
- The invariant becomes both a `requires` and an `ensures`
- Monotonicity, commutativity, and value-preservation become additional `ensures` clauses

**Never read the implementation and transcribe it into `requires`/`ensures`.** That produces a spec trivially satisfied by the current code but useless for catching bugs or constraining future refactors. A spec that mirrors the implementation is a tautology, not a specification.

Only consult the implementation to understand *how* to express something in Verus syntax — never to decide *what* to say.

If you find yourself writing a postcondition that starts "the function calls X, so ensures X ran," stop. That is describing the implementation, not specifying behavior.

**Name your predicates.** Define `open spec fn has_voted`, `invariant_holds`, `all_sigs_valid`, etc. rather than inlining complex expressions in `requires`/`ensures`. Named predicates are reusable across lemmas and make specs readable.

**Use biconditionals.** The state-transition postcondition should be `<=>` not `==>`:
```rust
forall|a: AuthorityName|
    self.has_voted(a) <==> (#[trigger] old(self).has_voted(a) || a == authority),
```

**Cover all variants.** For each return variant, state exactly when it occurs — necessary and sufficient conditions.

**Recursive spec functions** need `decreases` and a matching inductive lemma with explicit base case and inductive step.

## Writing the proof

**Start with `external_body`, then fill in.** First write the spec on a bare `external_body` function and confirm it type-checks. Then remove `external_body`, add the body, and add proof hints until Verus accepts it.

**Proof hints go immediately after the code they justify:**
```rust
self.data.insert_new(authority, s);
proof {
    lemma_voted_weight_insert(&old(self).committee, before_dom, authority);
    assert(!old(self).data@.contains_key(authority));
    assert(self.data@.dom() =~= old(self).data@.dom().insert(authority));
}
```

**Ghost variables** save spec values before they're consumed by exec:
```rust
let ghost pre_sig = envelope_sig_spec(&envelope);
let (data, sig) = envelope.into_data_and_sig();
// now: sig == pre_sig
```

**Triggers** tell Verus when to instantiate a `forall`. If a quantifier isn't firing, add `#[trigger]` to the term that appears in the goal:
```rust
forall|a: AuthorityName| agg.has_voted(a) ==> #[trigger] old(agg).has_voted(a),
```

**Commutativity lemmas** take intermediate `Set<AuthorityName>` values as parameters and the postconditions as hypotheses — don't try to reason about mutable-reference calls directly:
```rust
pub proof fn lemma_insert_commutes(
    agg_voted: Set<AuthorityName>, auth_a: AuthorityName, auth_b: AuthorityName,
    after_a: Set<AuthorityName>, after_ab: Set<AuthorityName>,
    after_b: Set<AuthorityName>, after_ba: Set<AuthorityName>,
)
    requires
        auth_a != auth_b,
        forall|c| after_a.contains(c) <==> (agg_voted.contains(c) || c == auth_a),
        // ... etc
```

## Common errors and fixes

| Error | Fix |
|---|---|
| `precondition not satisfied` at a call site | Check all `requires` of the callee; add the missing condition to the caller's `requires` or prove it holds at that point |
| `postcondition not satisfied` | Add a `proof { assert(...); }` block at the relevant point to show Verus the intermediate fact |
| `field expression for opaque datatype` | Use the getter method + `assume_specification` pattern (see `/verus-shims`) |
| Forall not instantiated | Add `#[trigger]` to a term that appears in the goal |
| `assertion failed` in a proof block | The intermediate fact is not derivable from what Verus knows; add a lemma call or more specific assertion |
| `decreases` check fails | Use a numeric bound that provably decreases, or restructure the recursion |

## Iteration loop

1. Run `bash scripts/verus-check.sh`
2. For each error, add the minimal proof hint that resolves it
3. Do not add hints speculatively — only add what a specific error requires
4. Repeat until zero errors

When Verus reports zero errors, also run `cargo check -p <crate>` to confirm the stable build is clean (spec functions inside `verus!{}` are erased; make sure nothing leaks out with a missing `#[cfg(verus_only)]`).
