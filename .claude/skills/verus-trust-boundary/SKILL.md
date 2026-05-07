# Verus: Identify the Trust Boundary

Decide what to prove directly and what to axiomatize with `external_body`.

## The principle

Every `external_body` is a gap in the proof. The goal is to minimize those gaps while keeping the verification tractable. Verify the logic; axiomatize the physics.

## What to prove directly

- Control flow and branching (which branch is taken, what is returned)
- State invariants (sum invariants, domain membership, value equality)
- Set-algebraic properties (commutativity, monotonicity)
- Anything that can be expressed as a formula over spec types without opening a crypto primitive

## What to axiomatize with `external_body`

- **Cryptographic operations**: BLS signature verification, hash functions, key recovery. These are correct-by-construction in the library; re-proving them in Verus adds no safety.
- **Complex loops with external side effects**: eviction loops that walk a HashMap, log messages, call external libraries.
- **Opaque library behavior**: aggregation of BLS sigs, serialization, network I/O.

The test: *can you state what the function does without opening its body?* If yes, `external_body` with strong postconditions. If no, you need to understand and prove the body.

## Structuring external_body helpers

When you decide a piece of logic is `external_body`, extract it into a named helper rather than marking the entire public function as trusted. This way the public function is proven correct *using* the trusted helper.

Example: `insert` is proven; `try_aggregate_and_verify` (the BLS part) is `external_body`. The proof of `insert` is real — it verifies the epoch check, the delegation to `insert_generic`, and the state changes — and relies only on the postconditions of the trusted helper.

**Write strong postconditions on every `external_body` helper.** The caller can only rely on what those postconditions say. Every property you omit becomes a gap that propagates outward. At minimum:
- Invariant preserved
- Committee/context reference unchanged
- State changes bounded (e.g. "eviction only removes entries, never adds")
- Key semantic guarantees (e.g. "valid sigs are never evicted")

## Checklist

- [ ] Every `external_body` function has postconditions that are as strong as the caller needs
- [ ] No function is `external_body` just because it was hard to think about — only because its body is genuinely opaque (crypto, I/O, complex library)
- [ ] The trust boundary is a thin layer around the opaque parts, not the whole module
- [ ] The proven functions call into `external_body` helpers; the public API is proven

Once the boundary is clear, proceed to `/verus-shims` to set up type registrations, then `/verus-proof` to write the spec and proof.
