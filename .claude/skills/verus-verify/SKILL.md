# Verus Formal Verification

Guides you through formally verifying a Rust function or data structure using Verus.

## Usage

```
/verus-verify <target>
```

Where `<target>` is a function, method, or data structure to verify (e.g. `StakeAggregator::insert_generic`).

## Background

Verus is a Rust formal verifier that uses an SMT solver (Z3). It operates at the source level: you annotate Rust code with `requires`/`ensures` contracts and `proof` blocks, and Verus either proves correctness or reports a counterexample.

### Key concepts

- **`verus! { }`** — macro that marks code for verification. Outside this macro, Verus erases everything to a no-op in stable builds.
- **`spec fn`** — mathematical definition. No runtime cost; can call other spec fns.
- **`proof fn`** — theorem. Has no runtime cost; can only be called from other proofs.
- **`exec fn`** — ordinary Rust function with `requires`/`ensures` annotations.
- **`open spec fn`** — spec fn whose body is visible to callers (unfoldable). Use `closed` to hide the body.
- **`external_type_specification`** — tells Verus a type defined outside `verus!{}` exists.
- **`external_body`** — marks a function body as trusted; its spec is accepted as axiom.
- **`assume_specification`** — attaches `requires`/`ensures` to an existing exec function without modifying it. Used for functions in other crates.
- **`broadcast proof`** — lemma Verus applies automatically via triggers; use for facts you want always in scope.

### Crate structure

Each crate that needs verification gets a sister crate at `crates/<name>/verified/`. The sister crate has `[package.metadata.verus] verify = true` in its `Cargo.toml`. The main crate adds the sister as a workspace dependency and re-exports any types moved into it. This avoids the orphan rule (you can only implement Verus traits like `View` for types your crate owns).

The sister crates are verified by `scripts/verus-check.sh`.

---

## Workflow

### Phase 1: Write the informal spec first

Before touching any Verus syntax, articulate the algebraic model in plain language or math.

1. **What abstract state does the type maintain?** Express it as a mathematical object (set, map, sequence) independent of implementation details (hash maps, BTreeSets, etc.).

2. **What does the function do to that state?** Write a functional description:
   - What is the state before and after?
   - What are the possible outcomes?
   - Are there preconditions on the inputs?

3. **What are the key invariants?** Identify properties that must hold before AND after every public operation (e.g. "total_votes == Σ weight(a) for a ∈ data").

4. **What should the behavioral contract be?** Ask: "What does a caller need to know?" Not "What does the implementation do?" Prefer:
   - Biconditionals (`⟺`) over one-directional implications
   - Properties about the abstract state (e.g. `has_voted`) over implementation details (e.g. "the HashMap contains key k")
   - Commutativity and monotonicity properties — these are often the most useful guarantees for callers

Write the informal spec down before writing any code.

---

### Phase 2: Check for under-specification

Once you have a draft spec, actively look for gaps. Common failure modes:

- **One-directional when biconditional is possible.** If you write `result is QuorumReached ==> total >= threshold`, ask whether the converse also holds. If it does, say so.
- **Missing monotonicity.** If a function adds to a set, does it ever remove from it? If it never removes valid entries, say so explicitly.
- **Missing commutativity.** If two insertions commute, prove it — this is a non-trivial safety property.
- **Variant determination not fully specified.** If you specify when `Failed` can occur, also specify when it CANNOT occur. The biconditional strengthens the spec.
- **Invariant not in requires/ensures.** If the invariant must hold before calling a function, put it in `requires`. If it must hold after, put it in `ensures`. Don't omit it just because it "obviously" holds.

Spawn a sub-agent to independently review the spec for under-specifications before writing proofs.

---

### Phase 3: Identify the trust boundary

Decide what to prove and what to axiomatize.

**Prove directly:**
- Control flow and branching logic
- State invariants (sum invariants, domain membership)
- Set-algebraic properties (commutativity, monotonicity)
- Anything that can be expressed as a pure formula over spec types

**Axiomatize with `external_body`:**
- Cryptographic operations (BLS signature verification, hash functions)
- Complex loops with difficult-to-express invariants that aren't central to the correctness argument
- External library behavior that is well-understood but costly to re-verify

**The rule:** if the correctness argument for the function depends on the body being correct, prove it. If it depends only on the *interface* of a sub-operation (e.g. "verify_secure returns Ok iff the sig is valid"), axiomatize the interface and prove the caller.

When using `external_body`, write strong postconditions — state everything the callers need. Weak postconditions from external_body helpers produce weak guarantees for the functions that call them.

---

### Phase 4: Register external types

For any type used in spec/proof code that is defined outside `verus!{}`:

1. Use `external_type_specification` + `external_body` to register it. This tells Verus the type exists but treats it as opaque.
2. For fields you need to access in spec: add exec getter methods to the type, then use `assume_specification` to connect the getter's return value to an `uninterp spec fn`.
3. For traits you need as bounds (e.g. `T: Message`): register the trait with `external_trait_specification`. Declare any associated types the compiler needs.
4. If a type needs `View`, `obeys_key_model`, or other Verus traits: move the type definition into the verified crate so you can `impl` the trait without an orphan error.

The wrapper tuple struct pattern (`pub struct ExFoo(pub Foo)`) is required because Rust attributes can only be attached to items you own — the wrapper is just a syntactic anchor for the `external_type_specification` attribute.

---

### Phase 5: Write the spec

Translate the informal spec from Phase 1 into Verus syntax.

**For `requires`:** include the invariant (`invariant_holds()`), uniqueness/well-formedness conditions, and any overflow guards.

**For `ensures`:** write biconditionals for variant determination, state transitions as `forall` over domain membership (biconditional `<==>`), and value-preservation postconditions for existing entries.

**Naming:** define named spec predicates (`has_voted`, `invariant_holds`, `all_sigs_valid`) rather than inlining complex expressions. This makes specs readable and allows sharing across lemmas.

**Commutativity lemmas:** take the intermediate states as parameters and the relevant postconditions as hypotheses, rather than reasoning about mutable-reference calls directly. The proof then reduces to pure set algebra.

---

### Phase 6: Write the proof

Start by running Verus on the spec with an empty `external_body` body and check that the spec itself type-checks. Then fill in the body and add proof hints as needed.

**Proof hints:** use `proof { assert(...); }` blocks immediately after the code steps they justify. Each hint should explain the "why" — not what the code does, but why it satisfies the invariant.

**Lemmas for induction:** recursive spec functions (over list prefixes, set sizes, etc.) need matching inductive lemmas. Write the lemma, prove the base case and inductive step explicitly with `decreases`.

**Triggers:** Verus uses triggers to instantiate quantifiers. If a `forall` isn't firing, add `#[trigger]` to the term Verus should watch for. Prefer terms that appear in the goal.

**Common failures:**
- "field expression for opaque datatype" → the type was registered with `external_body`; add a getter method and `assume_specification`.
- "precondition not satisfied" at a call site → check all `requires` clauses of the callee; add the missing condition to the caller's `requires` or prove it holds.
- Unused imports in `#[cfg(verus_only)]` blocks → Verus spec functions are erased in stable builds; guard their imports with `#[cfg(verus_only)]`.

---

### Phase 7: Verify and commit

Run `bash scripts/verus-check.sh` and confirm zero errors across all verified crates. Also run `cargo check -p <crate>` to confirm the stable build is clean (no warnings).

Commit verified code and supporting shims together. Commit debug scaffolding (temporary asserts, exploratory specs) separately so they can be discarded.

---

## Patterns Reference

### Abstract state predicate
```rust
pub open spec fn has_voted(&self, authority: AuthorityName) -> bool {
    self.data@.contains_key(authority)
}
```

### Sum invariant
```rust
pub open spec fn invariant_holds(&self) -> bool {
    self.data@.dom().finite()
    && self.total_votes as int == voted_weight(&self.committee, self.data@.dom())
}
```

### Biconditional state transition
```rust
ensures
    forall|a: AuthorityName|
        self.has_voted(a) <==> (#[trigger] old(self).has_voted(a) || a == authority),
```

### Value preservation
```rust
ensures
    forall|a: AuthorityName|
        old(self).has_voted(a) ==> #[trigger] self.data@[a] == old(self).data@[a],
    !old(self).has_voted(authority) ==> self.data@[authority] == s,
```

### Connecting exec getters to spec projectors
```rust
// In the type's crate:
pub fn get_epoch(&self) -> EpochId { self.epoch }

// In verus!{}:
pub uninterp spec fn auth_sig_epoch_spec(sig: &AuthoritySignInfo) -> u64;
pub assume_specification[ AuthoritySignInfo::get_epoch ](sig: &AuthoritySignInfo) -> (e: u64)
    ensures e == auth_sig_epoch_spec(sig),
;
```

### external_body helper for trusted crypto operations
```rust
#[verifier::external_body]
fn try_aggregate_and_verify<T: Message + Serialize, const STRENGTH: bool>(
    agg: &mut StakeAggregator<AuthoritySignInfo, STRENGTH>,
    data: T,
) -> (out: InsertResult<AuthorityQuorumSignInfo<STRENGTH>>)
    requires
        old(agg).invariant_holds(),
        committee_unique(&old(agg).committee),
    ensures
        agg.committee == old(agg).committee,
        agg.invariant_holds(),
        forall|a: AuthorityName| agg.has_voted(a) ==> #[trigger] old(agg).has_voted(a),
        forall|a: AuthorityName|
            old(agg).has_voted(a)
            && sig_is_valid(&old(agg).data@[a], &old(agg).committee)
                ==> #[trigger] agg.has_voted(a),
        forall|a: AuthorityName|
            agg.has_voted(a) ==> #[trigger] agg.data@[a] == old(agg).data@[a],
{ /* untrusted body */ }
```

### Commutativity lemma (set-algebra pattern)
```rust
pub proof fn lemma_insert_commutes(
    agg_voted: Set<AuthorityName>,
    auth_a: AuthorityName,
    auth_b: AuthorityName,
    after_a: Set<AuthorityName>, after_ab: Set<AuthorityName>,
    after_b: Set<AuthorityName>, after_ba: Set<AuthorityName>,
)
    requires
        auth_a != auth_b,
        forall|c| after_a.contains(c) <==> (#[trigger] agg_voted.contains(c) || c == auth_a),
        forall|c| after_ab.contains(c) <==> (#[trigger] after_a.contains(c) || c == auth_b),
        forall|c| after_b.contains(c) <==> (#[trigger] agg_voted.contains(c) || c == auth_b),
        forall|c| after_ba.contains(c) <==> (#[trigger] after_b.contains(c) || c == auth_a),
    ensures
        forall|c| #[trigger] after_ab.contains(c) <==> after_ba.contains(c)
{
    assert forall|c| #[trigger] after_ab.contains(c) <==> after_ba.contains(c) by {
        assert(after_ab.contains(c) <==> (agg_voted.contains(c) || c == auth_a || c == auth_b));
        assert(after_ba.contains(c) <==> (agg_voted.contains(c) || c == auth_b || c == auth_a));
    }
}
```

### Recursive spec function with inductive lemma
```rust
pub open spec fn voted_weight_le(c: &Committee, voted: Set<AuthorityName>, n: int) -> int
    decreases n,
{
    if n <= 0 { 0 }
    else {
        let nm = committee_authorities(c)[n - 1];
        if voted.contains(nm) {
            committee_weight_seq(c)[n - 1] as int + voted_weight_le(c, voted, n - 1)
        } else {
            voted_weight_le(c, voted, n - 1)
        }
    }
}

pub proof fn lemma_voted_weight_insert_le(c: &Committee, voted: Set<AuthorityName>, name: AuthorityName, n: int)
    requires 0 <= n <= committee_authorities(c).len(), !voted.contains(name), committee_unique(c),
    ensures voted_weight_le(c, voted.insert(name), n) == voted_weight_le(c, voted, n) + weight_of_aux(c, name, n),
    decreases n,
{ /* inductive step */ }
```
