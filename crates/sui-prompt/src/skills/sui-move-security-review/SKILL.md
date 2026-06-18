---
name: sui-move-security-review
description: >
  Use when auditing, reviewing, or hunting for vulnerabilities in Move code on Sui — including
  deployed/decompiled packages. A checklist of invariants whose VIOLATION causes exploitable
  bugs: access control & capabilities, struct abilities & type safety, object lifecycle &
  ownership, shared-object and PTB attack surface, dynamic fields & collections, arithmetic &
  coins, init/OTW/package upgrades, hot-potato composability, time & on-chain randomness, and
  test-only code leakage. Trigger on "audit this Move code", "find vulnerabilities in this Sui
  contract", "security review", "is this package safe?", "I suspect there's a bug in X",
  "something is wrong with this contract", or when reasoning about whether a Move function can
  be abused.
---

# Move Security Review (on Sui)

> **Self-bootstrap (any AI agent):** this skill is bundled inside the `sui` binary. **For
> comprehensive audits, read every reference file in this bundle before applying the
> routing table below** — default to `sui prompt skill sui-move-security-review --all`
> for a one-shot load of all of them; if you need to budget context tighter, enumerate
> with `sui prompt skill sui-move-security-review --list` then read each via `--file
> <ref>`. The detection heuristics, severity ratings, exploit sketches, and bytecode
> signals live in those per-category files; the routing table is a summary only.
> This skill belongs to one or more categories — run `sui prompt categories` to see them
> and `sui prompt category <name>` to read the category's workflow. No filesystem install
> is required — the binary is self-contained.

> Offensive counterpart to the constructive Move skills — each rule = violated invariant
> with detection heuristic, severity, and exploit sketch.

> **Sources.** Per-rule citation `MystenLabs/skills → <file>`. **[+domain]** = established
> auditing practice not in upstream skills (high-yield, easy to miss — e.g. SM-A3, SM-B4).
> Verify on-chain facts against [docs.sui.io](https://docs.sui.io),
> [move-book.com](https://move-book.com), or framework source.

> **Auditing on-chain bytecode?** Use `sui-and-move-tools` to fetch + decompile; apply
> `SM-*` rules to the decompiled `.move` files. See `auditing-bytecode.md` for workflow +
> per-rule signals. Drop to disassembly only for abort-code values, failed decompilation,
> or ambiguous excerpts.

## How to audit with this skill

1. **Map the attack surface first.** List every `public` and `entry` function (these are the only
   externally reachable entrypoints), every `*Cap`/witness/OTW type, every shared object, and
   every struct's ability set. PTB callers control argument order and supply arbitrary inputs —
   treat all entrypoints as adversarial.
2. **Walk the catalog by category** (A–M). For each rule, run the `Detect` heuristic
   against the code.
   - **A grep hit is a *candidate*, not a finding.** Confirm the invariant is actually
     broken before reporting.
   - **A grep miss is NOT proof of absence — walk the candidate set explicitly.** Many
     rules detect the *absence* of a guard, check, or invariant — either purely (SM-A2,
     A3, A6, B4, D1, E4, G2) or as confirmation on top of a candidate-presence check
     (SM-C1, C3, C4, F2, G1, M1). For these the bug shape is "X is missing where it
     should be"; an empty grep often means *"X is missing everywhere"*. Identify the
     rule's candidate set (privileged call sites for SM-A6; `&mut SharedT`-mutating
     `public`/`entry` fns for SM-A2; cap-gated fns for SM-A3; `dynamic_field::borrow*` /
     `bag::borrow*` / `table::borrow*` sites for SM-E4; `object::delete` sites for SM-C1;
     etc.) and for each candidate check whether the required guard is present — reason
     about dataflow, not the textual presence of `assert!` somewhere in the file.
3. **Report findings keyed to the rule ID** (e.g. `SM-A3`) with: severity, the offending
   location (`file:line`), why the invariant is violated, and the concrete exploit. Distinguish
   *exploitable* from *defense-in-depth*.
4. **Re-derive, don't trust naming.** A struct named `AdminCap` may be safe; an unnamed struct
   may be the real authority. Reason from abilities and from who can construct/obtain a value.

## Severity legend

- **Critical** — direct loss/theft of funds or objects, unlimited mint, total authority seizure, or permanent asset lock.
- **High** — privilege escalation, invariant bypass, or DoS of core functionality under realistic conditions.
- **Medium** — conditional/limited-impact issues, information disclosure, griefing, or contention DoS.

## Routing table (load the reference file for the category in scope)

| File | Categories | Covers |
|------|-----------|--------|
| `access-control.md` | A, G(custody) | capability abilities, missing auth, cap↔resource binding, admin handoff, witness forging, treasury/deny custody |
| `abilities-and-types.md` | B | object shape, irreversible `store`/soulbound, event/Display exposure, type-confusion / fake-object injection |
| `object-lifecycle.md` | C, D | UID deletion & orphaned dynamic fields, accidental share, ungated shared-object delete, `Receiving<T>`, on-chain invariant enforcement, contention/equivocation |
| `dynamic-fields.md` | E | DF key collision/predictability, DF vs DOF addressability, unbounded inline collections, cleanup |
| `arithmetic-and-coins.md` | F, G | integer truncation via `as`, rounding/zero-amount, mint/burn gating, deny-list enforcement |
| `init-otw-upgrades.md` | H, I | OTW well-formedness, unsafe `init`, `UpgradeCap` custody, versioning/migration gaps |
| `composability-and-ptb.md` | J, K | hot-potato weakening, internal-transfer/leaky `_mut`, attacker-orchestrated PTB, gas-coin/sponsor |
| `time-and-randomness.md` | L | `Clock` vs epoch time, **randomness test-and-abort** |
| `test-and-offchain.md` | M + appendix | `#[test_only]` leakage; off-chain appendix (non-blocking) |
| `auditing-bytecode.md` | all (on-chain) | applying the rules to the decompiled view (working substrate); per-rule signals in decompiled-source terms; disassembly fetched per-module on demand for specific verification cases |

## Known system addresses (sanity references)

`0x1` std · `0x2` Sui framework · `0x6` `Clock` · `0x8` `Random` · `0x403` coin `DenyList`.
