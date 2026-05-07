# Verus: Check for Under-Specification

Actively audit the informal spec for gaps before writing any Verus code.

## Why this step exists

Under-specified code is dangerous: Verus will happily verify it, but the proof says nothing useful. A spec that only captures part of the contract gives false confidence.

## Spawn an independent reviewer

Before self-reviewing, spawn a sub-agent and ask it to:
1. Read the informal spec you wrote
2. Read the implementation
3. Report any properties the spec does not capture that a correct implementation would satisfy

The sub-agent should not try to fix anything — only report. Review its findings before continuing.

## Self-review checklist

Work through each item. If the answer is "no" or "unclear," the spec is under-specified.

### State transitions

- [ ] **Biconditional, not one-directional.** If you wrote `result is QuorumReached ==> total >= threshold`, does the converse also hold? If so, use `<==>`  instead of `==>`. One-directional implications leave half the behavior unspecified.

- [ ] **All variants covered.** For each return variant, is there a clause that says *exactly* when it occurs? Not just sufficient conditions — necessary AND sufficient.

- [ ] **"Nothing else changes" is stated.** If only one entry is added, say `has_voted(a) <==> old(has_voted(a)) || a == authority` — the biconditional captures that no OTHER entries were added or removed. A one-directional implies you can also add arbitrary other entries.

### Invariants

- [ ] **Invariant in both requires and ensures.** If the invariant must hold before a call, it must be in `requires`. If it must hold after, it must be in `ensures`. Don't assume it carries over implicitly.

- [ ] **Committee/context reference unchanged.** If a function should not mutate the committee, say `self.committee == old(self).committee`.

### Semantic properties

- [ ] **Monotonicity.** If an entry with a valid sig was present before, is it still present after? If yes, add: `old(self).has_voted(a) && sig_is_valid(&old(self).data@[a], ...) ==> self.has_voted(a)`.

- [ ] **Value preservation.** Are stored values for existing entries ever overwritten? If not: `old(self).has_voted(a) ==> self.data@[a] == old(self).data@[a]`.

- [ ] **Commutativity.** If two operations commute, add a lemma. The proof is usually just set-algebra commutativity applied to the state-transition biconditionals.

### Axioms (external_body)

- [ ] **Every external_body postcondition is as strong as possible.** The caller can only rely on what the external_body postconditions say. Weak postconditions here produce weak guarantees everywhere the helper is used.

- [ ] **The trust boundary is correct.** Is the function really too hard to prove? Or is it just unfamiliar? Functions with complex loops or cryptographic bodies are good external_body candidates. Pure logic is not.

## After this step

Fix any gaps found before proceeding to `/verus-trust-boundary`. An under-specified formal proof is worse than no proof — it gives false assurance.
