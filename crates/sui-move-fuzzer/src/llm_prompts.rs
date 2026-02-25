// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Prompt engineering for LLM-guided seed generation.
//!
//! Provides:
//! - `build_seed_gen_prompt()` — constructs a gap-specific prompt
//! - `BYTECODE_REFERENCE` — Move bytecode instruction reference for the LLM
//! - `MODULE_SPEC_REFERENCE` — ModuleSpec JSON format documentation
//! - `VERIFIER_ARCHITECTURE` — Summary of the Sui Move verification pipeline

use crate::coverage_gaps::CoverageGap;

// ─── Reference constants ──────────────────────────────────────────────────────

pub const VERIFIER_ARCHITECTURE: &str = r#"
# Sui Move VM Verification Pipeline

A CompiledModule passes through these layers in order. Each layer fully
rejects the module before the next one runs.

1. **BoundsChecker** — validates that all table indices (handles, signatures,
   identifiers) are within bounds. ~99% of random bytes fail here.

2. **SignatureChecker** — validates that all Signature entries contain valid
   token sequences (no cycles, phantom constraints satisfied, etc.)

3. **InstructionConsistency** — validates that deprecated global-storage opcodes
   are not used when `deprecate_global_storage_ops` is enabled (Sui mainnet).

4. **CodeUnitVerifier** — runs multiple sub-passes on each function body:
   - ControlFlow: checks CFG reducibility (no irreducible loops)
   - StackUsageSafety: balanced stack across all paths, no underflow
   - TypeSafety: type-checks each instruction using an abstract interpreter
   - LocalsSafety: checks that locals are initialized before use
   - ReferenceSafety: borrow-check using graph or regex analysis; checks that
     references don't escape scopes, loops converge with consistent borrow states

5. **InstantiationLoops** — detects cycles in the generic instantiation graph
   (e.g., f<T> calls f<vector<T>> → infinite instantiation depth)

6. **Sui-specific passes** (applied after Move verification passes):
   - struct_with_key_verifier: key structs must have UID as first field
   - global_storage_access_verifier: no global storage operations (Move global ops
     like move_to/move_from are banned in Sui)
   - id_leak_verifier: UID values must not be duplicated or laundered through
     pack/unpack sequences; tracks Fresh/Bound/Other states
   - entry_points_verifier: entry functions must have compatible signatures
   - one_time_witness_verifier: OTW structs must follow naming conventions
"#;

pub const MODULE_SPEC_REFERENCE: &str = r#"
# ModuleSpec JSON Format

Output a JSON object with these fields:

```json
{
  "module_name": "string",          // valid Move identifier, no spaces
  "imports": [                       // optional list of framework imports
    {
      "address": "0x2",             // hex address (short form like "0x2" is OK)
      "module": "object",           // module name
      "types": ["UID"],             // types from this module you reference in field types
      "functions": ["new"]          // functions you want to Call
    }
  ],
  "structs": [                       // optional list of struct definitions
    {
      "name": "MyStruct",
      "abilities": ["copy", "drop"], // any of: copy, drop, store, key
      "fields": [
        {"name": "x", "type": "u64"},
        {"name": "id", "type": "UID"}  // use plain name if imported above
      ]
    }
  ],
  "functions": [                     // at least one function required
    {
      "name": "attack",
      "visibility": "public",        // public | private | friend
      "is_entry": true,              // optional, default false
      "parameters": ["u64", "&mut TxContext"],  // type strings
      "returns": [],                 // type strings
      "locals": ["u64"],             // additional locals beyond parameters
      "code": [                      // bytecode instruction strings
        "CopyLoc(0)",
        "LdU64(1)",
        "Add",
        "Pop",
        "Ret"
      ]
    }
  ]
}
```

## Type strings

Primitives: `bool`, `u8`, `u16`, `u32`, `u64`, `u128`, `u256`, `address`, `signer`
References: `&T` (immutable), `&mut T` (mutable)
Vectors: `vector<T>`
Imported types: use the type name directly after listing it in `imports.types`
Shorthands: `TxContext` = `0x2::tx_context::TxContext`, `UID` = `0x2::object::UID`
Local structs: use the struct name directly (e.g., `"MyStruct"`)

## Bytecode instructions

Local variable operations:
- `CopyLoc(N)` — copy local N onto stack (N is 0-indexed, params come first)
- `MoveLoc(N)` — move local N onto stack (local becomes unavailable)
- `StLoc(N)` — pop stack and store into local N
- `ImmBorrowLoc(N)` — borrow local N immutably → &T on stack
- `MutBorrowLoc(N)` — borrow local N mutably → &mut T on stack
- `FreezeRef` — convert &mut T to &T
- `ReadRef` — dereference &T → T
- `WriteRef` — pop T and &mut T, write T through reference

Integer constants:
- `LdTrue`, `LdFalse` — push bool
- `LdU8(N)`, `LdU16(N)`, `LdU32(N)`, `LdU64(N)`, `LdU128(N)`, `LdU256(N)` — push integer

Arithmetic (pop 2 same-type integers, push 1):
- `Add`, `Sub`, `Mul`, `Div`, `Mod`
- `BitAnd`, `BitOr`, `Xor`
- `Shl`, `Shr` — shift (second operand must be u8)

Comparison (pop 2 same-type integers, push bool):
- `Lt`, `Gt`, `Le`, `Ge`, `Eq`, `Neq`

Logic (on bools):
- `And`, `Or`, `Not`

Control flow:
- `Branch(N)` — unconditional jump to instruction index N (0-based)
- `BrTrue(N)` — pop bool, jump if true
- `BrFalse(N)` — pop bool, jump if false
- `Ret` — return (must match function return type signature)

Stack:
- `Pop` — discard top of stack
- `Abort` — abort with u64 error code from stack

Casts:
- `CastU8`, `CastU16`, `CastU32`, `CastU64`, `CastU128`, `CastU256`

Struct operations:
- `Pack(StructName)` — pop field values from stack, push struct value
- `Unpack(StructName)` — pop struct value, push field values onto stack
- `ImmBorrowField(N)` — borrow field N of struct ref (N is field_handle index)
- `MutBorrowField(N)` — mutable borrow of field N

Function calls:
- `Call(module::function)` — call imported function (must be in imports.functions)

## Key constraints

1. Every function MUST end with `Ret`
2. The stack must be empty at `Ret` (unless returning values matching the `returns` list)
3. Branch targets are instruction indices (0-based) within the function body
4. `CopyLoc`/`MoveLoc` can only access initialized locals (params are pre-initialized)
5. Stack depth must be identical on all paths that merge at a branch target
6. For `key` structs: the FIRST field must be of type `UID` (from 0x2::object)
"#;

pub const BYTECODE_REFERENCE: &str = r#"
Complete Move bytecode instruction reference for LLM use:

STACK EFFECTS (→ = result type pushed):
  LdTrue → bool        LdFalse → bool
  LdU8(n) → u8        LdU16(n) → u16      LdU32(n) → u32
  LdU64(n) → u64      LdU128(n) → u128    LdU256(n) → u256
  CopyLoc(i) → T      (T = type of local i)
  MoveLoc(i) → T      (moves, local becomes unavailable)
  StLoc(i): T →       (pops T, stores to local i, local must be type T)
  ImmBorrowLoc(i) → &T
  MutBorrowLoc(i) → &mut T
  FreezeRef: &mut T → &T
  ReadRef: &T → T
  WriteRef: T, &mut T →    (pops value then reference, writes)
  Pack(S): f0, f1, ... → S (fields in declaration order)
  Unpack(S): S → f0, f1, ... (fields in declaration order)
  ImmBorrowField(fhi): &S → &T  (field handle index → field type)
  MutBorrowField(fhi): &mut S → &mut T
  Call(fhi): args → rets
  Add/Sub/Mul/Div/Mod: T, T → T   (same integer type)
  BitAnd/BitOr/Xor: T, T → T
  Shl/Shr: T, u8 → T  (first arg is any int, second must be u8)
  Lt/Gt/Le/Ge: T, T → bool
  Eq/Neq: T, T → bool  (any copyable type)
  And/Or: bool, bool → bool
  Not: bool → bool
  CastU8..CastU256: (any int) → (target type)
  Branch(n): (no stack effect)
  BrTrue(n): bool →    (jumps if true)
  BrFalse(n): bool →   (jumps if false)
  Pop: T →
  Ret: (return values) →
  Abort: u64 →
"#;

// ─── Prompt builder ───────────────────────────────────────────────────────────

/// Build a prompt asking Claude to generate a `ModuleSpec` JSON that exercises
/// the given coverage gap.
///
/// `source_context` is the relevant verifier source code (a snippet or the
/// full file content) that the LLM should reason about.
pub fn build_seed_gen_prompt(gap: &CoverageGap, source_context: &str) -> String {
    format!(
        r#"You are an expert in the Move bytecode format and the Sui Move VM verifier.
Your task is to generate a `ModuleSpec` JSON that exercises a specific, hard-to-reach
verifier path. This will be used as a fuzzing seed.

{VERIFIER_ARCHITECTURE}

{MODULE_SPEC_REFERENCE}

# Target coverage gap

ID: {gap_id}
Description: {gap_description}
Verifier pass: {gap_pass:?}
Error expected: {gap_error}

# LLM hint

{gap_hint}

# Relevant verifier source code

```rust
{source_context}
```

# Your task

Generate a `ModuleSpec` JSON that:
1. Is structurally valid (passes BoundsChecker and SignatureChecker)
2. Specifically targets the path described above
3. Is as minimal as possible — include only what's needed to reach the target path

Respond with ONLY the JSON, no explanation. Wrap it in ```json ... ``` fences.
The JSON must be complete and valid — it will be parsed directly.

Example of the expected format:
```json
{{
  "module_name": "fuzz_target",
  "imports": [],
  "structs": [],
  "functions": [{{
    "name": "f",
    "visibility": "public",
    "parameters": [],
    "returns": [],
    "locals": [],
    "code": ["LdU64(42)", "Pop", "Ret"]
  }}]
}}
```

Now generate the ModuleSpec for the target gap:
"#,
        gap_id = gap.path_id,
        gap_description = gap.description,
        gap_pass = gap.pass,
        gap_error = gap
            .error_code
            .map(|c| format!("{:?}", c))
            .unwrap_or_else(|| "varies (check Sui pass)".to_string()),
        gap_hint = gap.llm_hint,
    )
}

/// Build a retry prompt when the previous attempt failed.
///
/// `previous_spec` is the JSON the LLM produced last time.
/// `error` is the error we got when trying to build/verify with it.
pub fn build_retry_prompt(gap: &CoverageGap, previous_spec: &str, error: &str) -> String {
    format!(
        r#"Your previous attempt to generate a ModuleSpec for gap `{gap_id}` failed.

**Error:** {error}

**Your previous ModuleSpec:**
```json
{previous_spec}
```

Please fix the ModuleSpec to avoid this error while still targeting the same gap:
- {gap_description}
- Hint: {gap_hint}

{MODULE_SPEC_REFERENCE}

{BYTECODE_REFERENCE}

Respond with ONLY the corrected JSON in ```json ... ``` fences.
"#,
        gap_id = gap.path_id,
        gap_description = gap.description,
        gap_hint = gap.llm_hint,
    )
}

/// Extract the first ```json ... ``` block from an LLM response string.
pub fn extract_json(response: &str) -> Option<&str> {
    let start = response.find("```json")?.checked_add(7)?;
    let rest = &response[start..];
    // Skip optional leading newline.
    let content_start = if rest.starts_with('\n') { 1 } else { 0 };
    let end = rest.find("```")?;
    Some(rest[content_start..end].trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_basic() {
        let resp = "Here is the result:\n```json\n{\"a\": 1}\n```\nDone.";
        assert_eq!(extract_json(resp), Some("{\"a\": 1}"));
    }

    #[test]
    fn extract_json_no_newline() {
        let resp = "```json{\"x\":2}```";
        assert_eq!(extract_json(resp), Some("{\"x\":2}"));
    }

    #[test]
    fn extract_json_none_without_fence() {
        let resp = "no json here";
        assert_eq!(extract_json(resp), None);
    }
}
