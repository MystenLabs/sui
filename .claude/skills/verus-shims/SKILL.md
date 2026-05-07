# Verus: Register External Types and Functions

Set up the Verus infrastructure for types and functions defined outside `verus!{}`.

## Why registration is needed

Verus only reasons about types defined inside `verus!{}`. Everything else is opaque. Registration tells Verus a type or function exists and attaches spec behavior to it.

## Registering a type

Types defined outside `verus!{}` — even in the same file — need a wrapper tuple struct:

```rust
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExCommittee(pub Committee);
```

The wrapper exists solely as a syntactic anchor for the attribute. Rust can't attach attributes to items you don't own, so the tuple struct provides an owned item that Verus can act on.

## Accessing fields on registered types

Direct field access on `external_body` types is disallowed ("opaque datatype"). Instead:

1. Add a public getter method to the type (in its crate):
   ```rust
   pub fn get_epoch(&self) -> EpochId { self.epoch }
   ```
2. In `verus!{}`, declare an uninterpreted spec function and connect it:
   ```rust
   pub uninterp spec fn auth_sig_epoch_spec(sig: &AuthoritySignInfo) -> u64;
   pub assume_specification[ AuthoritySignInfo::get_epoch ](sig: &AuthoritySignInfo) -> (e: u64)
       ensures e == auth_sig_epoch_spec(sig),
   ;
   ```

## Registering a trait

When a type bound like `T: Message` causes a Verus error, register the trait:

```rust
#[verifier::external_trait_specification]
pub trait ExMessage {
    type ExternalTraitSpecificationFor: Message;
    type DigestType: Clone + std::fmt::Debug;  // declare associated types the compiler uses
}
```

Only declare the associated types that appear in struct fields or method signatures that Verus needs to see.

## Attaching spec to existing functions (`assume_specification`)

For a method you didn't write but want to reason about:

```rust
pub assume_specification[ Committee::epoch ](c: &Committee) -> (e: u64)
    ensures e == committee_epoch_spec(c),
;
```

The bracket syntax `[ Type::method ]` identifies the function. The signature must match the real function exactly — generic parameters, ownership, and return type.

## Moving a type to the verified crate

When you need to implement a Verus trait (like `View`) for a type, you hit the orphan rule: you can't `impl ForeignTrait for ForeignType`. The fix is to move the type definition into the verified sister crate (`crates/<name>/verified/`), so the crate owns both the type and the impl. The original crate re-exports the type so existing imports are unchanged.

Move a type when:
- You need `impl View for T` and `T` is defined in a crate that doesn't have `verify = true`
- You need to attach spec functions as methods rather than as external stubs

## Common errors

| Error | Cause | Fix |
|---|---|---|
| `field expression for opaque datatype` | Direct field access on `external_body` type | Add getter + `assume_specification` |
| `cannot find associated type A67_DigestType` | Trait registered without declaring its associated types | Add `type DigestType: ...` to the `ExTrait` declaration |
| `assume_specification requires function type signature to match exactly` | Generic parameters differ between spec and real function | Match the full signature including all type parameters |
| Unresolved import of a spec fn in stable build | Spec function defined inside `verus!{}` imported at top level | Wrap the import in `#[cfg(verus_only)]` |
