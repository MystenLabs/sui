# Verus Formal Verification

Guides you through formally verifying a Rust function or data structure using Verus.

## Usage

```
/verus-verify <target>
```

## Phases

Work through these phases in order. Each has its own sub-skill with detailed guidance.

1. **Write the informal spec** (`/verus-informal-spec`)
   Articulate the algebraic model and behavioral contract in plain language before writing any Verus syntax. This is the most important step — a bad spec produces a useless proof.

2. **Check for under-specification** (`/verus-check-spec`)
   Actively hunt for missing biconditionals, monotonicity, commutativity, and other gaps. Spawn a sub-agent to review independently.

3. **Identify the trust boundary** (`/verus-trust-boundary`)
   Decide what to prove and what to axiomatize with `external_body`. Write strong postconditions on every `external_body` helper.

4. **Register external types** (`/verus-shims`)
   Set up `external_type_specification`, `external_trait_specification`, and `assume_specification` for types and functions from outside `verus!{}`.

5. **Write the spec and proof** (`/verus-proof`)
   Translate the informal spec into Verus `requires`/`ensures`, add proof hints, and iterate until Verus reports zero errors.

6. **Verify and commit**
   Run `bash scripts/verus-check.sh` (zero errors across all verified crates) and `cargo check -p <crate>` (clean stable build). Commit verified code and shims together; commit debug scaffolding separately so it can be discarded.
