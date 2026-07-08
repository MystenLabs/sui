// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Human-facing documentation for each lint, surfaced via `move lint --explain <CODE>` and
//! referenced by a per-diagnostic hint. This module is the single source of truth for lint prose;
//! the numeric `(category, code)` ids are only used to render the `Wxxyyy` code and to match
//! emitted diagnostics so the hint can name the right lint.

use crate::linters::LinterDiagnosticCategory;
use std::fmt;

/// The category name for a diagnostic `category` byte, exactly as defined by
/// [`LinterDiagnosticCategory`]. Referencing the enum discriminants keeps this in lockstep with the
/// linter — the docs never invent categories of their own.
fn category_label(category: u8) -> &'static str {
    use LinterDiagnosticCategory::*;
    match category {
        c if c == Correctness as u8 => "Correctness",
        c if c == Complexity as u8 => "Complexity",
        c if c == Suspicious as u8 => "Suspicious",
        c if c == Deprecated as u8 => "Deprecated",
        c if c == Style as u8 => "Style",
        c if c == Sui as u8 => "Sui",
        _ => "Unknown",
    }
}

/// Where a lint originates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LintOrigin {
    /// Generic Move linter (`move_compiler::linters`).
    Core,
    /// Sui-specific linter (`move_compiler::sui_mode::linters`).
    Sui,
}

impl LintOrigin {
    fn label(self) -> &'static str {
        match self {
            LintOrigin::Core => "Core",
            LintOrigin::Sui => "Sui",
        }
    }
}

/// A worked bad/good pair illustrating a lint.
pub struct LintExample {
    /// A snippet that triggers the lint.
    pub bad: &'static str,
    /// The corrected snippet that does not.
    pub good: &'static str,
}

/// Human-facing documentation for a single lint.
pub struct LintDoc {
    /// Filter name: the canonical `--explain` key and the `#[allow(lint(<name>))]` key.
    pub name: &'static str,
    pub origin: LintOrigin,
    /// `true` if the lint runs at the default level; `false` if it requires `--lint`.
    pub default: bool,
    /// Diagnostic category and code. Used to render the `Wxxyyy` code and to match emitted
    /// diagnostics for the `--explain` hint. These follow the current (origin-based) numbering.
    pub category: u8,
    pub code: u8,
    /// One-line description of what the lint flags (matches the diagnostic message).
    pub summary: &'static str,
    /// Why the flagged pattern matters.
    pub rationale: &'static str,
    /// When firing is acceptable, or how to knowingly opt out. `None` omits the section.
    pub when_ok: Option<&'static str>,
    pub example: LintExample,
}

impl LintDoc {
    /// The rendered diagnostic code, e.g. `W99000`. Lints are always warnings, so the severity
    /// prefix is always `W`.
    pub fn rendered_code(&self) -> String {
        format!("W{:02}{:03}", self.category, self.code)
    }
}

/// Look up a lint doc by its filter name (`share_owned`) or by its rendered code, accepting
/// `W99000`, `w99000`, `99000`, or a `Lint `-prefixed form.
pub fn find_lint_doc(query: &str) -> Option<&'static LintDoc> {
    let q = query.trim();
    if let Some(doc) = LINT_DOCS.iter().find(|d| d.name == q) {
        return Some(doc);
    }
    let norm = q
        .strip_prefix("Lint ")
        .unwrap_or(q)
        .trim()
        .trim_start_matches(['W', 'w']);
    LINT_DOCS
        .iter()
        .find(|d| d.rendered_code().trim_start_matches('W') == norm)
}

/// Look up a lint doc by its diagnostic `(category, code)`. Used to attach the `--explain` hint to
/// emitted lint diagnostics.
pub fn lint_doc_by_id(category: u8, code: u8) -> Option<&'static LintDoc> {
    LINT_DOCS
        .iter()
        .find(|d| d.category == category && d.code == code)
}

fn indent(s: &str, pad: &str) -> String {
    s.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `--explain <name>` output for a single lint.
impl fmt::Display for LintDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = if self.default {
            "on by default"
        } else {
            "requires `--lint`"
        };
        writeln!(f, "{}", self.name)?;
        writeln!(f, "{}", "=".repeat(self.name.len()))?;
        writeln!(f)?;
        writeln!(f, "category: {}", category_label(self.category))?;
        writeln!(f, "origin:   {}", self.origin.label())?;
        writeln!(f, "level:    {level}")?;
        writeln!(f, "code:     {}", self.rendered_code())?;
        writeln!(f)?;
        writeln!(f, "{}", self.summary)?;
        writeln!(f)?;
        writeln!(f, "{}", self.rationale)?;
        if let Some(when_ok) = self.when_ok {
            writeln!(f)?;
            writeln!(f, "When it's OK:")?;
            writeln!(f, "{}", indent(when_ok, "  "))?;
        }
        writeln!(f)?;
        writeln!(f, "Example:")?;
        writeln!(f)?;
        writeln!(f, "  // flagged")?;
        writeln!(f, "{}", indent(self.example.bad, "  "))?;
        writeln!(f)?;
        writeln!(f, "  // suggested")?;
        writeln!(f, "{}", indent(self.example.good, "  "))?;
        writeln!(f)?;
        writeln!(
            f,
            "Suppress a specific case with `#[allow(lint({}))]`.",
            self.name
        )
    }
}

/// The catalog listing printed by `move lint --list`.
pub struct LintIndex;

impl fmt::Display for LintIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Move lint index. Run `move lint --explain <name>` for details on any lint.\n"
        )?;
        let width = LINT_DOCS.iter().map(|d| d.name.len()).max().unwrap_or(0);
        // Categories in `LinterDiagnosticCategory` order; empty ones are skipped.
        const CATEGORIES: &[&str] = &[
            "Correctness",
            "Complexity",
            "Suspicious",
            "Deprecated",
            "Style",
            "Sui",
        ];
        for category in CATEGORIES {
            let mut any = false;
            for doc in LINT_DOCS
                .iter()
                .filter(|d| &category_label(d.category) == category)
            {
                if !any {
                    writeln!(f, "{category}")?;
                    any = true;
                }
                writeln!(f, "    {:width$}  {}", doc.name, doc.summary, width = width)?;
            }
            if any {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

//**************************************************************************************************
// Registry
//**************************************************************************************************

// A flat registry; `render_index` groups by category. The `(category, code)` pairs are the lints'
// actual diagnostic ids (see `LinterDiagnosticCategory`), used to render `Wxxyyy` and match
// emitted diagnostics.
pub const LINT_DOCS: &[LintDoc] = &[
    LintDoc {
        name: "share_owned",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 0,
        summary: "possible owned object share",
        rationale: "\
Sharing is irreversible and makes an object world-writable forever. Calling `share_object` on an \
object that is not freshly created in this function (it arrives as a parameter or from an unpack) \
can silently hand every account write access to something a user believed was theirs. It fires only \
when the type is also transferable — it has `store`, or is transferred with `transfer::transfer` \
elsewhere; a `key`-only type that is never transferred is not flagged.",
        when_ok: Some(
            "The shared object really is fresh, but was produced through a helper call the local \
analysis can't see through — a conservative false positive.",
        ),
        example: LintExample {
            bad: "// `o` is a parameter — the checker can't prove it is fresh\npublic fun share(o: Obj) {\n    transfer::public_share_object(o)\n}",
            good: "// packed here, so provably a fresh object\npublic fun share(ctx: &mut TxContext) {\n    transfer::share_object(Obj { id: object::new(ctx) })\n}",
        },
    },
    LintDoc {
        name: "custom_state_change",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 2,
        summary: "potentially unenforceable custom transfer/share/freeze policy",
        rationale: "\
A by-value struct defined in this module that has the `store` ability can already be transferred, \
shared, or frozen by anyone through the `public_*` API. Routing it through the private \
`transfer`/`share_object`/`freeze_object` therefore cannot enforce any custom policy — callers just \
bypass it.",
        when_ok: Some(
            "If the type keeps `store`, call the honest `public_*` variant. If the policy must \
actually hold, remove `store` so the private variant is the only path.",
        ),
        example: LintExample {
            bad: "// `S1` has `key` + `store`, so callers can already freeze it via `public_freeze_object`\npublic fun custom_freeze(o: S1) {\n    transfer::freeze_object(o)\n}",
            good: "public fun custom_freeze(o: S1) {\n    transfer::public_freeze_object(o)\n}",
        },
    },
    LintDoc {
        name: "freeze_wrapped",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 4,
        summary: "attempting to freeze wrapped objects",
        rationale: "\
Freezing makes an object immutable forever. If the frozen object wraps another object — a field \
whose type has `key`, directly or transitively — that inner object is frozen and made unrecoverable \
too, which is almost never intended.",
        when_ok: None,
        example: LintExample {
            bad: "public struct Inner has key, store { id: UID }\npublic struct Wrapper has key, store { id: UID, inner: Inner }\n\npublic fun freeze_wrapper(w: Wrapper) {\n    transfer::public_freeze_object(w)\n}",
            good: "public struct Config has key, store { id: UID, fee: u64 }\n\npublic fun freeze_config(c: Config) {\n    transfer::public_freeze_object(c)\n}",
        },
    },
    LintDoc {
        name: "public_random",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 6,
        summary: "Risky use of 'sui::random'",
        rationale: "\
A `public` function that takes `Random` or `RandomGenerator` can be called by other Move code, \
letting an attacker draw randomness and react to the outcome within the same transaction. Randomness \
should only be reachable from a transaction, not composed by another contract.",
        when_ok: Some(
            "Reduce the visibility below `public` — a non-public `entry` function, `public(package)`, \
or a private function — so it can't be composed by another contract. Adding `entry` to a function \
that stays `public` does not help.",
        ),
        example: LintExample {
            bad: "public fun not_allowed(_r: &Random) {}",
            good: "entry fun basic_random(_r: &Random) {}",
        },
    },
    LintDoc {
        name: "freezing_capability",
        origin: LintOrigin::Sui,
        default: false,
        category: 99,
        code: 8,
        summary: "freezing potential capability",
        rationale: "\
Capabilities gate privileged actions. Freezing one turns it into a permanent immutable object that \
anyone can reference and that can never be revoked — usually the opposite of the intended access \
control. The lint matches by struct name (a capitalized `Cap`).",
        when_ok: Some(
            "The match is by name — a `Cap` at the end of the name, or followed by an uppercase \
letter, digit, or `_`. So a non-capability like `NoCap` is a false positive, while a real \
capability with an off-pattern name (`AdminRights`, `Capv0`) is missed.",
        ),
        example: LintExample {
            bad: "public struct AdminCap has key { id: UID }\n\npublic fun freeze_cap(cap: AdminCap) {\n    transfer::public_freeze_object(cap)\n}",
            good: "// keep the capability owned instead of freezing it\npublic fun keep_cap(cap: AdminCap, owner: address) {\n    transfer::transfer(cap, owner)\n}",
        },
    },
    LintDoc {
        name: "missing_key",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 7,
        summary: "struct with id but missing key ability",
        rationale: "\
A struct whose first field is `id: UID` looks like a Sui object, but without the `key` ability it \
can never be stored, transferred, or shared as one. This is almost always a forgotten `has key`.",
        when_ok: None,
        example: LintExample {
            bad: "public struct MissingKeyAbility {\n    id: UID,\n}",
            good: "public struct HasKeyAbility has key {\n    id: UID,\n}",
        },
    },
    LintDoc {
        name: "collection_equality",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 5,
        summary: "possibly useless collections compare",
        rationale: "\
Sui collections — `Bag`, `ObjectBag`, `Table`, `ObjectTable`, `LinkedTable`, `TableVec`, `VecMap`, \
and `VecSet` — have no meaningful `==`. The UID-backed ones (`Bag`, `Table`, …) compare by their \
internal handle, not contents; `VecMap`/`VecSet` compare their backing vector, so equal contents in \
a different insertion order compare unequal. Either way `==`/`!=` rarely means what it looks like.",
        when_ok: None,
        example: LintExample {
            bad: "public fun bag_eq(bag1: &Bag, bag2: &Bag): bool {\n    bag1 == bag2\n}",
            good: "// compare the state you care about, not the collection handle\npublic fun bag_eq(bag1: &Bag, bag2: &Bag): bool {\n    bag1.length() == bag2.length()\n}",
        },
    },
    LintDoc {
        name: "uncallable_function",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 11,
        summary: "it will not be possible to call this function",
        rationale: "\
The transaction runtime can only supply certain argument shapes. Taking `TxContext` by value, a \
`&mut TxContext` alongside any other `TxContext` parameter (it must be the only one), or a `&mut` to \
`Clock`/`Random` makes the function impossible to call from a transaction.",
        when_ok: Some(
            "A single `&mut TxContext` is fine. Use `&Clock` and `&Random` rather than `&mut`.",
        ),
        example: LintExample {
            bad: "// `Clock` must be passed by immutable reference\nfun uses_clock(_c: &mut Clock) {}",
            good: "fun uses_clock(_c: &Clock) {}",
        },
    },
    LintDoc {
        name: "unused_object_with_fields",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 12,
        summary: "unused object with fields",
        rationale: "\
A reference to an object that carries data beyond its `id`, but whose fields are never read, is a \
likely mistake: the function takes the object yet never looks at the values it holds.",
        when_ok: Some(
            "Objects with no field beyond `id` (pure marker capabilities), by-value params, and \
generic object types are out of scope. Otherwise the function should assert on or otherwise read a \
field — or drop the parameter if it truly isn't needed.",
        ),
        example: LintExample {
            bad: "public struct OwnerCap has key { id: UID, owns: address }\n\npublic fun unused(_c: &OwnerCap) {}",
            good: "public fun owner(c: &OwnerCap): address {\n    c.owns\n}",
        },
    },
    LintDoc {
        name: "loop_without_exit",
        origin: LintOrigin::Core,
        default: false,
        category: 2,
        code: 6,
        summary: "'loop' without 'break' or 'return'",
        rationale: "\
A `loop` whose body contains neither `break` nor `return` has no normal exit — it runs until it \
aborts, if ever. This is almost always a missing exit condition. (`while` is covered separately by \
`while_true`.)",
        when_ok: Some(
            "An `abort` inside the loop is not treated as an exit, so a deliberately divergent loop \
is a false positive.",
        ),
        example: LintExample {
            bad: "let i = 0;\nloop {\n    i = i + 1;\n}",
            good: "let i = 0;\nloop {\n    if (i >= 10) break;\n    i = i + 1;\n}",
        },
    },
    LintDoc {
        name: "self_assignment",
        origin: LintOrigin::Core,
        default: false,
        category: 2,
        code: 8,
        summary: "assignment preserves the same value",
        rationale: "\
Assigning a location to itself (`x = x`, `*r = *r`, `s.f = s.f`) has no effect. It usually signals a \
typo or an unfinished edit.",
        when_ok: None,
        example: LintExample {
            bad: "p = p;",
            good: "// remove the redundant statement",
        },
    },
    LintDoc {
        name: "always_equal_operands",
        origin: LintOrigin::Core,
        default: false,
        category: 2,
        code: 11,
        summary: "redundant, always-equal operands for binary operation",
        rationale: "\
A binary operation whose two operands are the same expression has an already-known result: `x == x` \
is always `true`, `x - x` is `0`, `x / x` is `1`, `x & x` is just `x`. The operation is dead weight \
and often a copy-paste bug where one side should differ.",
        when_ok: None,
        example: LintExample {
            bad: "let b = x == x;",
            good: "let b = true;",
        },
    },
    LintDoc {
        name: "unused_return_value",
        origin: LintOrigin::Core,
        default: true,
        category: 2,
        code: 13,
        summary: "return value of a non-mutating call is discarded",
        rationale: "\
Discarding the result of a call that has no `&mut` arguments means the call did nothing observable. \
Either use the value or make the intent explicit with `let _ = ...`. In Sui mode `&mut TxContext` is \
treated as non-mutating, so such calls are still flagged.",
        when_ok: Some(
            "Calls that take a `&mut` argument (a real side effect) are not flagged. Bind to `let _` \
to acknowledge an intentionally discarded value.",
        ),
        example: LintExample {
            bad: "fun price(x: u64): u64 { x + 1 }\n// ...\nprice(10);",
            good: "let _ = price(10);",
        },
    },
    LintDoc {
        name: "self_transfer",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 1,
        summary: "non-composable transfer to sender",
        rationale: "\
Transferring an object (one with `key` + `store`) to `ctx.sender()` inside the function makes it \
non-composable: a caller (or a programmable transaction block) cannot use the object because the \
function gives it away internally. Returning the object lets the caller decide what to do with it.",
        when_ok: Some(
            "Rare — the composable pattern is to return the object and let the caller place it. \
`entry` functions and `init` are already exempt.",
        ),
        example: LintExample {
            bad: "// `S1` has `key` + `store`\npublic fun mint(ctx: &mut TxContext) {\n    transfer::public_transfer(S1 { id: object::new(ctx) }, ctx.sender())\n}",
            good: "public fun mint(ctx: &mut TxContext): S1 {\n    S1 { id: object::new(ctx) }\n}",
        },
    },
    LintDoc {
        name: "coin_field",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 3,
        summary: "sub-optimal 'sui::coin::Coin' field type",
        rationale: "\
`Coin<T>` is itself a full object (it has an `id: UID`) meant for transfers between accounts. Held \
as a field it adds needless object plumbing; `Balance<T>` is the storage-oriented type for keeping \
value inside another object.",
        when_ok: Some(
            "Keep `Coin` only if the field genuinely needs to be an independent object. An alias does \
not avoid the lint — the resolved type is what's matched.",
        ),
        example: LintExample {
            bad: "public struct S2 has key, store {\n    id: UID,\n    c: Coin<S1>,\n}",
            good: "public struct S2 has key, store {\n    id: UID,\n    c: Balance<S1>,\n}",
        },
    },
    LintDoc {
        name: "public_entry",
        origin: LintOrigin::Sui,
        default: true,
        category: 99,
        code: 10,
        summary: "unnecessary `entry` on a `public` function",
        rationale: "\
`entry` on a `public` function is redundant: a `public` function is already callable from a \
programmable transaction block. `entry` is only meaningful on a non-`public` function, where it is \
what makes the function callable as a transaction command.",
        when_ok: None,
        example: LintExample {
            bad: "public entry fun mint() {}",
            good: "public fun mint() {}",
        },
    },
    LintDoc {
        name: "prefer_mut_tx_context",
        origin: LintOrigin::Sui,
        default: false,
        category: 99,
        code: 9,
        summary: "prefer '&mut TxContext' over '&TxContext'",
        rationale: "\
A public function that takes `&TxContext` cannot later create objects — which needs \
`&mut TxContext` — without a breaking signature change. Taking `&mut TxContext` up front keeps the \
API upgrade-compatible.",
        when_ok: Some(
            "Only `public` functions are checked; any non-`public` visibility (private, \
`public(package)`, `public(friend)`) is exempt, since those signatures can change freely.",
        ),
        example: LintExample {
            bad: "public fun incorrect_mint(_ctx: &TxContext) {}",
            good: "public fun correct_mint(_ctx: &mut TxContext) {}",
        },
    },
    LintDoc {
        name: "constant_naming",
        origin: LintOrigin::Core,
        default: false,
        category: 4,
        code: 1,
        summary: "constant should follow naming convention",
        rationale: "\
Constants are expected to be `UPPER_SNAKE_CASE` or PascalCase (either is accepted for any constant). \
A lowercase or mixed name (`max_supply`, `JSON_Max_Size`) reads like a variable and breaks module \
consistency.",
        when_ok: Some(
            "PascalCase is deliberately allowed — including the `E`-prefixed PascalCase used for \
error constants, such as `ENotAuthorized`.",
        ),
        example: LintExample {
            bad: "const Another_BadName: u64 = 42;",
            good: "const MAX_LIMIT: u64 = 1000;\nconst ENotAuthorized: u64 = 0;",
        },
    },
    LintDoc {
        name: "abort_without_constant",
        origin: LintOrigin::Core,
        default: false,
        category: 4,
        code: 5,
        summary: "'abort' or 'assert' without named constant",
        rationale: "\
A bare numeric abort code carries no meaning at the call site or in error output. A named constant \
documents the failure and keeps codes consistent across the module.",
        when_ok: Some(
            "The whole argument must be a single named constant — `abort A + B` still fires. \
`assert!(cond, ECode)` is the idiomatic form.",
        ),
        example: LintExample {
            bad: "abort 100",
            good: "const ERR_INVALID_ARGUMENT: u64 = 1;\n// ...\nabort ERR_INVALID_ARGUMENT",
        },
    },
    LintDoc {
        name: "unnecessary_math",
        origin: LintOrigin::Core,
        default: false,
        category: 1,
        code: 3,
        summary: "math operator can be simplified",
        rationale: "\
An operation with an identity operand (`* 1`, `/ 1`, `+ 0`, `- 0`, `<< 0`, `>> 0`) does nothing; one \
with an absorbing operand (`* 0`, `0 / x`, `x % 1`, `0 % x`) is `0`; and `1 % x` is `1`. Only literal \
`0`/`1` operands are detected.",
        when_ok: None,
        example: LintExample {
            bad: "let y = x * 1;",
            good: "let y = x;",
        },
    },
    LintDoc {
        name: "unnecessary_conditional",
        origin: LintOrigin::Core,
        default: false,
        category: 1,
        code: 7,
        summary: "'if' expression can be removed",
        rationale: "\
An `if` with one branch `true` and the other `false` collapses to the condition itself (or its \
negation), and one whose branches are the same literal value collapses to that value. The \
conditional only adds noise.",
        when_ok: None,
        example: LintExample {
            bad: "let b = if (!condition) true else false;",
            good: "let b = !condition;",
        },
    },
    LintDoc {
        name: "redundant_ref_deref",
        origin: LintOrigin::Core,
        default: false,
        category: 1,
        code: 9,
        summary: "redundant reference/dereference",
        rationale: "\
Taking a reference and immediately dereferencing it (`&*(&x)`), or dereferencing a fresh field \
borrow (`*(&s.f)`), is a no-op the compiler already performs for you.",
        when_ok: Some(
            "Dereferencing an existing reference (`*r`) is fine — only dereferencing a fresh borrow \
(`*(&x)` or `&*(&x)`) is redundant.",
        ),
        example: LintExample {
            bad: "let _ref = &*(&resource);",
            good: "let _ref = &resource;",
        },
    },
    LintDoc {
        name: "combinable_comparisons",
        origin: LintOrigin::Core,
        default: false,
        category: 1,
        code: 12,
        summary: "comparison operations condition can be simplified",
        rationale: "\
Two comparisons over the same operand pair joined by `&&`/`||` often collapse to a single comparison \
(`x == y && x >= y` is just `x == y`), or to a constant `true`/`false` (`x >= y || x <= y`).",
        when_ok: Some("Negated comparisons are not handled and won't fire."),
        example: LintExample {
            bad: "let b = x == y && x >= y;",
            good: "let b = x == y;",
        },
    },
    LintDoc {
        name: "while_true",
        origin: LintOrigin::Core,
        default: false,
        category: 4,
        code: 2,
        summary: "unnecessary 'while (true)', replace with 'loop'",
        rationale: "\
`while (true)` is an infinite loop written the long way. `loop` states the intent directly and can \
`break` with a value. Only the literal `true` condition is detected.",
        when_ok: None,
        example: LintExample {
            bad: "while (true) {\n    // ...\n}",
            good: "loop {\n    // ...\n}",
        },
    },
    LintDoc {
        name: "unneeded_return",
        origin: LintOrigin::Core,
        default: false,
        category: 4,
        code: 4,
        summary: "unneeded return",
        rationale: "\
In tail position the `return` keyword is redundant — the trailing expression is already the \
function's value.",
        when_ok: Some(
            "Only a tail-position `return` is flagged — of any value-yielding expression (a call, \
`S { .. }`, arithmetic, a cast, `loop`, even `()`). A `return` used as an early exit, or whose \
operand doesn't yield a value (e.g. `return abort E`), is left alone.",
        ),
        example: LintExample {
            bad: "fun price(): u64 {\n    return 5\n}",
            good: "fun price(): u64 {\n    5\n}",
        },
    },
    LintDoc {
        name: "unnecessary_unit",
        origin: LintOrigin::Core,
        default: false,
        category: 4,
        code: 10,
        summary: "unit `()` expression can be removed or simplified",
        rationale: "\
A `()` unit expression as a non-final statement, or as a branch of an `if`, adds nothing. Remove it, \
or invert the condition to drop the empty branch.",
        when_ok: None,
        example: LintExample {
            bad: "if (b) () else { x = 1 };",
            good: "if (!b) { x = 1 };",
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Route `--explain` snapshots into their own folder so they don't mix with the `.move`/`.snap`
    // linter fixtures under `tests/`.
    fn explain_settings() -> insta::Settings {
        let mut settings = insta::Settings::clone_current();
        settings.set_snapshot_path("snapshots/explain");
        settings.set_prepend_module_to_snapshot(false);
        settings
    }

    #[test]
    fn explain_output_per_lint() {
        explain_settings().bind(|| {
            for doc in LINT_DOCS {
                insta::assert_snapshot!(doc.name, doc.to_string());
            }
        });
    }

    #[test]
    fn explain_index() {
        explain_settings().bind(|| {
            insta::assert_snapshot!("index", LintIndex.to_string());
        });
    }

    #[test]
    fn ids_and_names_are_unique() {
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for doc in LINT_DOCS {
            assert!(names.insert(doc.name), "duplicate lint name {}", doc.name);
            assert!(
                ids.insert((doc.category, doc.code)),
                "duplicate lint id for {}",
                doc.name
            );
        }
    }

    #[test]
    fn every_registered_lint_has_a_doc() {
        use crate::diagnostics::filter::FILTER_ALL;
        let documented: HashSet<&str> = LINT_DOCS.iter().map(|d| d.name).collect();
        let mut registered: HashSet<String> = HashSet::new();
        for (_prefix, filters) in [
            crate::linters::known_filters(),
            crate::sui_mode::linters::known_filters(),
        ] {
            for (name, _ids) in filters {
                if name.as_str() != FILTER_ALL {
                    registered.insert(name.to_string());
                }
            }
        }
        let missing: Vec<_> = registered
            .iter()
            .filter(|n| !documented.contains(n.as_str()))
            .collect();
        assert!(missing.is_empty(), "lints missing an --explain doc: {missing:?}");
        // Every doc must correspond to a real registered lint (no stale entries).
        let extra: Vec<_> = documented
            .iter()
            .filter(|n| !registered.contains(**n))
            .collect();
        assert!(extra.is_empty(), "docs for unregistered lints: {extra:?}");
    }

    #[test]
    fn doc_ids_match_registered_filters() {
        use crate::diagnostics::filter::FILTER_ALL;
        // name -> (category, code) as the linter actually registers it.
        let mut registered: std::collections::HashMap<String, (u8, u8)> =
            std::collections::HashMap::new();
        for (_prefix, filters) in [
            crate::linters::known_filters(),
            crate::sui_mode::linters::known_filters(),
        ] {
            for (name, ids) in filters {
                if name.as_str() == FILTER_ALL {
                    continue;
                }
                let id = ids.first().expect("lint filter has an id");
                registered.insert(name.to_string(), (id.category, id.code));
            }
        }
        for doc in LINT_DOCS {
            let (category, code) = registered
                .get(doc.name)
                .unwrap_or_else(|| panic!("{} is not a registered lint", doc.name));
            assert_eq!(
                (doc.category, doc.code),
                (*category, *code),
                "doc for `{}` claims id ({}, {}) but the lint registers ({}, {}) — \
                 category/code must match the linter, not be hand-set",
                doc.name,
                doc.category,
                doc.code,
                category,
                code
            );
        }
    }

    #[test]
    fn explain_hint_injected_when_command_set() {
        use crate::diagnostics::codes::{Severity, custom};
        use crate::diagnostics::{
            Diagnostic, Diagnostics, report_diagnostics_to_buffer, set_explain_command,
        };
        use crate::linters::LINT_WARNING_PREFIX;
        use crate::shared::files::MappedFiles;
        use move_ir_types::location::Loc;

        // Runs in its own process under nextest, so the process-global command name is isolated.
        set_explain_command("test move lint");
        let info = custom(LINT_WARNING_PREFIX, Severity::Warning, 99, 0, "possible owned object share");
        let diag = Diagnostic::new(
            info,
            (Loc::invalid(), "here"),
            Vec::<(Loc, String)>::new(),
            Vec::<String>::new(),
        );
        let mut diags = Diagnostics::new();
        diags.add(diag);
        let rendered =
            String::from_utf8(report_diagnostics_to_buffer(&MappedFiles::empty(), diags, false))
                .unwrap();
        assert!(
            rendered.contains("test move lint --explain share_owned"),
            "expected `--explain` hint in output:\n{rendered}"
        );
    }

    #[test]
    fn find_by_name_and_code() {
        let doc = find_lint_doc("share_owned").expect("by name");
        assert_eq!(doc.name, "share_owned");
        assert_eq!(find_lint_doc("W99000").unwrap().name, "share_owned");
        assert_eq!(find_lint_doc("Lint W99000").unwrap().name, "share_owned");
        assert_eq!(find_lint_doc("99000").unwrap().name, "share_owned");
        assert!(find_lint_doc("nonsense").is_none());
    }
}
