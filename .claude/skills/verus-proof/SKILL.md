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

## Proved HashMap iteration

`HashMap::iter()` is fully specced in vstd via `MapIterGhostIterator` and can be used in
proven exec code. Two patterns have been established.

### Setup: activate the HashMap axioms

All proven iteration requires these broadcasts at the top of the function:

```rust
broadcast use axiom_authority_name_key_model, axiom_random_state_builds_valid_hashers,
    vstd::std_specs::hash::group_hash_axioms;
```

The `group_hash_axioms` broadcast unlocks `assume_specification[HashMap::iter]`, which gives:
- `map_iter@.1.to_set() == m@.kv_pairs()` — every (key,value) pair is covered
- `map_iter@.1.no_duplicates()` — no position appears twice

**`HashMap::values()` is weaker** — it only gives `to_set() == m@.values()` (no key identity,
no `no_duplicates`). Prefer `iter()` for any proven loop.

### Key scoping rule: use `map_iter@.1`, not `VERUS_ghost_iter.kv_pairs`

`VERUS_ghost_iter` is **out of Rust scope after the for loop** and cannot be referenced in
post-loop proof blocks. `map_iter@.1` (the ghost view of the exec iterator, held in a `let`
before the loop) is accessible throughout and after the loop, and equals
`VERUS_ghost_iter.kv_pairs` via exec_invariant.

Always anchor loop invariants that need post-loop reasoning to `map_iter@.1`:

```rust
let map_iter = m.inner.iter();
for (k, v) in map_iter
    invariant
        map_iter@.1.to_set() =~= m@.kv_pairs(),   // not VERUS_ghost_iter.kv_pairs
        ...
```

### Pattern 1: collect-all (direct index invariant)

When every entry is collected unconditionally, use a positional index invariant:

```rust
let map_iter = agg.data.inner.iter();
for (k, v) in map_iter
    invariant
        map_iter@.1.to_set() =~= agg.data@.kv_pairs(),
        result@.len() == VERUS_ghost_iter.pos,
        forall|i: int| 0 <= i < VERUS_ghost_iter.pos ==>
            #[trigger] result@[i] == map_iter@.1[i].1,
{
    proof { assert(map_iter@.1[VERUS_ghost_iter.pos] == (*k, *v)); }
    result.push(clone_sig(v));
}
// After loop — map_iter@.1 is still accessible:
proof {
    assert(result@.len() == map_iter@.1.len() as int);
    // Part 1: every output element came from the map
    // Part 2: every map value is in the output
    //   → find j with map_iter@.1[j] == (k, agg.data@[k]) from coverage,
    //     then result@[j] == agg.data@[k] from index invariant
}
```

Post-loop, the index invariant fires via `result@[j]` as trigger, giving
`result@[j] == map_iter@.1[j].1` for all `j < map_iter@.1.len()`.

### Pattern 2: filtered construction (ghost set + remaining invariant)

When only entries satisfying a predicate are kept, use a ghost `processed_dom` set and a
"remaining" invariant. The remaining invariant uses `map_iter@.1` as the trigger (not
`VERUS_ghost_iter.kv_pairs`) so the pre-loop coverage proof applies at the base case.

**Pre-loop: establish coverage explicitly**

```rust
let map_iter = agg.data.inner.iter();
proof {
    // Establish that every key appears at some position in map_iter@.1.
    // This fact carries into the loop invariant's base case via exec_invariant.
    assert forall|k: AuthorityName| init_data.dom().contains(k) implies
        exists|j: int| 0 <= j < map_iter@.1.len() && map_iter@.1[j].0 == k
    by {
        let pair = (k, init_data[k]);
        assert(init_data.kv_pairs().contains(pair));
        assert(map_iter@.1.to_set().contains(pair));
        let j = choose|j: int| 0 <= j < map_iter@.1.len() && map_iter@.1[j] == pair;
    };
}
```

**Loop invariants**

```rust
let ghost mut processed_dom: Set<AuthorityName> = Set::empty();
for (k, v) in map_iter
    invariant
        map_iter@.1.to_set() =~= init_data.kv_pairs(),
        map_iter@.1.no_duplicates(),
        processed_dom.subset_of(init_data.dom()),
        // tracked set: processed_dom == {map_iter@.1[j].0 | j < pos}
        forall|a: AuthorityName|
            processed_dom.contains(a) <==>
            exists|j: int| 0 <= j < VERUS_ghost_iter.pos && #[trigger] map_iter@.1[j].0 == a,
        // remaining invariant: every unprocessed init_data key is still to come.
        // After loop (pos == len): no such j exists → processed_dom == init_data.dom().
        forall|a: AuthorityName|
            init_data.dom().contains(a) && !processed_dom.contains(a)
            ==> exists|j: int|
                VERUS_ghost_iter.pos <= j < map_iter@.1.len()
                && #[trigger] map_iter@.1[j].0 == a,
        // ... membership and sum invariants ...
```

**Key-distinctness lemma** — derive that `map_iter@.1[j1].0 != map_iter@.1[j2].0` for
`j1 != j2` from `no_duplicates` + coverage. Two entries with the same key would have the same
map value (from coverage's `kv_pairs()` definition), giving the same tuple — contradicting
`no_duplicates`:

```rust
pub proof fn lemma_kv_pairs_key_distinct<K, V>(
    kv_pairs: Seq<(K, V)>, m: Map<K, V>, j1: int, j2: int,
)
    requires
        kv_pairs.no_duplicates(),
        kv_pairs.to_set() =~= m.kv_pairs(),
        0 <= j1 < kv_pairs.len(), 0 <= j2 < kv_pairs.len(), j1 != j2,
    ensures kv_pairs[j1].0 != kv_pairs[j2].0,
{ ... }
```

**Ghost set update: capture old value before inserting**

```rust
let ghost old_pd = processed_dom;
processed_dom = processed_dom.insert(*k);
// Now use old_pd directly in lemma calls — avoids processed_dom.remove(*k) reasoning.
lemma_voted_weight_insert(&committee, old_pd, *k);
assert(old_pd =~= processed_dom.remove(*k));
```

**Post-loop proof**

After the loop, the SMT solver uses the remaining invariant + ghost_ensures internally
(without user-accessible `VERUS_ghost_iter`) to derive `processed_dom == init_data.dom()`:

```rust
proof {
    // Z3 derives: "remaining" invariant with pos==len → no valid j → processed_dom ⊇ dom
    assert(processed_dom =~= init_data.dom());
    // membership biconditional + processed_dom == dom → postcondition
}
```

### Sum invariants and overflow bounds

When tracking a running sum, add to the loop invariant:

```rust
new_total as int + bad_votes as int == voted_weight(&committee, processed_dom),
voted_weight(&committee, init_data.dom()) == old(agg).total_votes as int,
committee_unique(&agg.committee),  // required by lemma_voted_weight_insert
```

The last two are constant-valued invariants: trivially maintained, but needed to give
`lemma_voted_weight_insert` and `lemma_voted_weight_le_subset` what they need.

Before each arithmetic assignment, assert the explicit integer bound so Verus can discharge
the overflow check:

```rust
assert(new_total as int + committee_weight_of(&agg.committee, *k) <= old(agg).total_votes as int);
new_total = new_total + votes;
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
| `invariant not satisfied before loop` | The base case fails; check coverage — often the pre-loop proof block is missing or the trigger doesn't match |
| `invariant not satisfied at end of loop body` | Step case fails; often a ghost variable update order issue or missing explicit sum equality assertion |
| Arithmetic overflow in proven loop | Add explicit `assert(a as int + b as int <= MAX)` before the assignment; derive the bound from `lemma_voted_weight_le_subset` + sum invariant |
| `VERUS_ghost_iter` not found after loop | It's out of scope — anchor the invariant to `map_iter@.1` instead and use it in the post-loop proof |
| `exists` postcondition not closed | Provide an explicit witness: `assert(agg.has_voted(k) && agg.data@[k] == v)` with the concrete `k` already in scope |

## Iteration loop

1. Run `bash scripts/verus-check.sh`
2. For each error, add the minimal proof hint that resolves it
3. Do not add hints speculatively — only add what a specific error requires
4. Repeat until zero errors

When Verus reports zero errors, also run `cargo check -p <crate>` to confirm the stable build is clean (spec functions inside `verus!{}` are erased; make sure nothing leaks out with a missing `#[cfg(verus_only)]`).
