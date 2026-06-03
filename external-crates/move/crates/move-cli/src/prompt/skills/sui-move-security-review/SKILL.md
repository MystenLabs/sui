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

> **Self-bootstrap (any AI agent):** this skill is bundled inside the `move` binary. **For
> comprehensive audits, enumerate every reference file in this bundle (`move prompt skill
> sui-move-security-review --list`) and read each one before applying the routing table
> below.** The detection heuristics, severity ratings, exploit sketches, and bytecode
> signals live in those per-category files; the routing table is a summary only. Read a
> specific reference file with `move prompt skill sui-move-security-review --file <ref>`.
> This skill belongs to one or more categories — run `move prompt categories` to see them
> and `move prompt category <name>` to read the category's workflow. No filesystem install
> is required — the binary is self-contained.

> Offensive counterpart to the constructive Move skills. Each constructive "must / never /
> always" rule implies a vulnerability when violated. This skill is the violation catalog: every
> rule has a **detection heuristic**, a **severity**, and an **exploit sketch** so findings are
> actionable against concrete code.

> **Sources.** Rules are grounded in the MystenLabs Sui skills (cited per rule as
> `MystenLabs/skills → <file>`). Rules marked **[+domain]** come from established Sui/Move auditing
> practice and are NOT in those skills — they are high-yield and easy to miss (e.g.
> capability–resource binding SM-A3, type-confusion SM-B4). When verifying on-chain facts, prefer
> [docs.sui.io](https://docs.sui.io), [move-book.com](https://move-book.com), and the Sui framework
> source.

> **Auditing on-chain bytecode?** If the target is a deployed package (not source): first stand up
> tools with the `sui-and-move-tools` skill (clone Sui, fetch the package, **disassemble** every
> module — and build `move decompile` for the explanation step). **Apply the `SM-*` rules to the
> disassembly** (`move disassemble` output): it is faithful, 1:1 with the executed bytecode.
> Once a finding is confirmed on the assembly, render the matching decompiled `.move` snippet
> alongside as a *Human view* — but never derive a finding from decompiled source. See
> `auditing-bytecode.md` for the workflow + per-rule disassembly signals, and
> `move-bytecode-comprehension` for what survives compilation.

## How to audit with this skill

1. **Map the attack surface first.** List every `public` and `entry` function (these are the only
   externally reachable entrypoints), every `*Cap`/witness/OTW type, every shared object, and
   every struct's ability set. PTB callers control argument order and supply arbitrary inputs —
   treat all entrypoints as adversarial.
2. **Walk the catalog by category** (A–M). For each rule, run the `Detect` heuristic against the
   code. A grep hit is a *candidate*, not a finding — confirm the invariant is actually broken.
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
| `auditing-bytecode.md` | all (on-chain) | applying the rules to **disassembly** (analysis substrate); per-rule disassembly signals; rendering decompiled snippets as *Human view* for confirmed findings |

## Known system addresses (sanity references)

`0x1` std · `0x2` Sui framework · `0x6` `Clock` · `0x8` `Random` · `0x403` coin `DenyList`.
