// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Abstract access paths for identifying memory cells in Move programs.
//!
//! An access path is a root (formal parameter, local variable, or return value)
//! followed by zero or more offsets (field, vector index, or dynamic field).
//! Some examples:
//!
//! * `Formal(0)` — the first parameter
//! * `Formal(0).x` — field `x` of the first parameter
//! * `Local(3).value` — the `value` field of local variable 3 (works for
//!   both structs and enum variants since variant field access in Move
//!   bytecode is still a field access)
//! * `Formal(1)/dyn(U64)` — a dynamic field with key type `u64` on the
//!   second parameter
//!
//! This is a Sui-specific simplification of the access path abstraction from
//! `move-stackless-bytecode`. Key differences:
//! * No global storage roots (`move_to`/`move_from` do not exist in Sui)
//! * Fields are identified by name (`Symbol`) rather than positional index
//! * Enum variant fields are accessed through the same `Field` offset as
//!   struct fields (the variant is context, not a navigation step)
//! * Sui dynamic field offsets are supported

use move_binary_format::normalized;
use move_symbol_pool::Symbol;
use std::fmt;

type Type = normalized::Type<Symbol>;

/// Index of a local variable or function parameter.
pub type TempIndex = usize;

// =================================================================================================
// Data Model

/// Root of an access path: a formal parameter, local variable, or return value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Root {
    /// A formal parameter of the current function.
    Formal(TempIndex),
    /// A local variable in the current function.
    Local(TempIndex),
    /// A return value of the current function.
    Return(usize),
}

/// Offset of an access path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Offset {
    /// Access a field by name. Works for both struct fields and enum variant
    /// fields — in Move bytecode, variant field access
    /// (`ImmBorrowVariantField` / `MutBorrowVariantField`) is still a field
    /// access; the variant is the context, not a separate navigation step.
    Field(Symbol),

    /// Unknown index into a vector.
    VectorIndex,

    /// Access through a Sui dynamic field. The key describes the kind of
    /// dynamic field and the type used to look it up.
    DynamicField(DynamicFieldKey),
}

/// Describes the kind and key type of a Sui dynamic field access.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DynamicFieldKey {
    /// Whether this is a dynamic field (value inline) or dynamic object field
    /// (value is a separate object).
    pub kind: DynamicFieldKind,
    /// The Move type of the dynamic field key.
    pub key_type: Type,
}

/// The kind of a Sui dynamic field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DynamicFieldKind {
    /// `sui::dynamic_field::Field<K, V>` — value stored inline.
    DynamicField,
    /// `sui::dynamic_object_field::Wrapper<K>` — value stored as a separate
    /// object.
    DynamicObject,
}

/// A unique identifier for a memory cell: a root followed by zero or more
/// offsets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccessPath {
    root: Root,
    offsets: Vec<Offset>,
}

// =================================================================================================
// Root

impl Root {
    /// Create a `Root` for a formal parameter.
    pub fn formal(index: TempIndex) -> Self {
        Root::Formal(index)
    }

    /// Create a `Root` for a local variable.
    pub fn local(index: TempIndex) -> Self {
        Root::Local(index)
    }

    /// Create a `Root` for a return value.
    pub fn ret(index: usize) -> Self {
        Root::Return(index)
    }

    /// Return `true` if this root is a formal parameter.
    pub fn is_formal(&self) -> bool {
        matches!(self, Self::Formal(_))
    }

    /// Return `true` if this root is a local variable.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }

    /// Return `true` if this root is a return value.
    pub fn is_return(&self) -> bool {
        matches!(self, Self::Return(_))
    }
}

// =================================================================================================
// Offset

impl Offset {
    /// Create a field offset.
    pub fn field(name: Symbol) -> Self {
        Self::Field(name)
    }

    /// Create a vector index offset.
    pub fn vector_index() -> Self {
        Self::VectorIndex
    }

    /// Create a dynamic field offset.
    pub fn dynamic_field(kind: DynamicFieldKind, key_type: Type) -> Self {
        Self::DynamicField(DynamicFieldKey { kind, key_type })
    }

    /// Return `true` if this offset is the same in all concrete executions.
    ///
    /// Field offsets are statically known (they select a fixed member).
    /// Vector indices and dynamic fields are not.
    pub fn is_statically_known(&self) -> bool {
        match self {
            Self::Field(_) => true,
            Self::VectorIndex | Self::DynamicField(_) => false,
        }
    }
}

// =================================================================================================
// AccessPath

impl AccessPath {
    /// Create an access path from a root and a sequence of offsets.
    pub fn new(root: Root, offsets: Vec<Offset>) -> Self {
        AccessPath { root, offsets }
    }

    /// Create an access path with no offsets.
    pub fn new_root(root: Root) -> Self {
        AccessPath {
            root,
            offsets: vec![],
        }
    }

    /// Unpack into root and offsets.
    pub fn into_parts(self) -> (Root, Vec<Offset>) {
        (self.root, self.offsets)
    }

    /// Return a reference to the root.
    pub fn root(&self) -> &Root {
        &self.root
    }

    /// Return a reference to the offsets.
    pub fn offsets(&self) -> &[Offset] {
        &self.offsets
    }

    /// Extend this access path by appending offset `o`.
    pub fn add_offset(&mut self, o: Offset) {
        self.offsets.push(o)
    }

    /// Prepend `prefix` to this path: replace the root with `prefix`'s root
    /// and prepend `prefix`'s offsets before this path's offsets.
    pub fn prepend(&mut self, prefix: Self) {
        self.root = prefix.root;
        let mut new_offsets = prefix.offsets;
        new_offsets.append(&mut self.offsets);
        self.offsets = new_offsets;
    }

    /// Return `true` if every offset in the path is statically known.
    pub fn is_statically_known(&self) -> bool {
        self.offsets.iter().all(|o| o.is_statically_known())
    }
}

// =================================================================================================
// Display

impl fmt::Display for Root {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Formal(i) => write!(f, "Formal({})", i),
            Self::Local(i) => write!(f, "Local({})", i),
            Self::Return(i) => write!(f, "Ret({})", i),
        }
    }
}

impl fmt::Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Field(name) => write!(f, ".{}", name),
            Self::VectorIndex => f.write_str("[_]"),
            Self::DynamicField(key) => write!(f, "{}", key),
        }
    }
}

impl fmt::Display for DynamicFieldKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            DynamicFieldKind::DynamicField => "dyn",
            DynamicFieldKind::DynamicObject => "dyn_obj",
        };
        write!(f, "/{}({:?})", kind, self.key_type)
    }
}

impl fmt::Display for AccessPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root)?;
        for offset in &self.offsets {
            write!(f, "{}", offset)?;
        }
        Ok(())
    }
}

// =================================================================================================
// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(s: &str) -> Symbol {
        Symbol::from(s)
    }

    #[test]
    fn test_root_constructors_and_predicates() {
        let formal = Root::formal(0);
        assert!(formal.is_formal());
        assert!(!formal.is_local());
        assert!(!formal.is_return());

        let local = Root::local(3);
        assert!(!local.is_formal());
        assert!(local.is_local());
        assert!(!local.is_return());

        let ret = Root::ret(1);
        assert!(!ret.is_formal());
        assert!(!ret.is_local());
        assert!(ret.is_return());
    }

    #[test]
    fn test_offset_constructors() {
        let f = Offset::field(sym("balance"));
        assert!(matches!(f, Offset::Field(s) if s.as_str() == "balance"));

        let vi = Offset::vector_index();
        assert!(matches!(vi, Offset::VectorIndex));
    }

    #[test]
    fn test_offset_is_statically_known() {
        assert!(Offset::field(sym("x")).is_statically_known());
        assert!(!Offset::vector_index().is_statically_known());
        assert!(
            !Offset::dynamic_field(DynamicFieldKind::DynamicField, Type::U64).is_statically_known()
        );
        assert!(
            !Offset::dynamic_field(DynamicFieldKind::DynamicObject, Type::Address)
                .is_statically_known()
        );
    }

    #[test]
    fn test_access_path_construction() {
        let ap = AccessPath::new_root(Root::formal(0));
        assert!(ap.root().is_formal());
        assert!(ap.offsets().is_empty());

        let ap2 = AccessPath::new(Root::local(1), vec![Offset::field(sym("value"))]);
        assert!(ap2.root().is_local());
        assert_eq!(ap2.offsets().len(), 1);
    }

    #[test]
    fn test_access_path_add_offset() {
        let mut ap = AccessPath::new_root(Root::formal(0));
        ap.add_offset(Offset::field(sym("inner")));
        ap.add_offset(Offset::field(sym("value")));
        assert_eq!(ap.offsets().len(), 2);
    }

    #[test]
    fn test_access_path_prepend() {
        let mut ap = AccessPath::new(Root::formal(0), vec![Offset::field(sym("b"))]);
        let prefix = AccessPath::new(Root::formal(1), vec![Offset::field(sym("a"))]);
        ap.prepend(prefix);
        assert!(matches!(ap.root(), Root::Formal(1)));
        assert_eq!(ap.offsets().len(), 2);
        assert!(matches!(&ap.offsets()[0], Offset::Field(s) if s.as_str() == "a"));
        assert!(matches!(&ap.offsets()[1], Offset::Field(s) if s.as_str() == "b"));
    }

    #[test]
    fn test_access_path_into_parts() {
        let ap = AccessPath::new(
            Root::formal(2),
            vec![Offset::field(sym("x")), Offset::vector_index()],
        );
        let (root, offsets) = ap.into_parts();
        assert!(matches!(root, Root::Formal(2)));
        assert_eq!(offsets.len(), 2);
    }

    #[test]
    fn test_is_statically_known() {
        // All fields → statically known
        let ap = AccessPath::new(
            Root::formal(0),
            vec![Offset::field(sym("a")), Offset::field(sym("b"))],
        );
        assert!(ap.is_statically_known());

        // Contains vector index → not statically known
        let ap2 = AccessPath::new(
            Root::formal(0),
            vec![Offset::field(sym("items")), Offset::vector_index()],
        );
        assert!(!ap2.is_statically_known());

        // Contains dynamic field → not statically known
        let ap3 = AccessPath::new(
            Root::formal(0),
            vec![Offset::dynamic_field(
                DynamicFieldKind::DynamicField,
                Type::U64,
            )],
        );
        assert!(!ap3.is_statically_known());
    }

    // Demonstrate that enum variant field access uses the same Field offset
    // as struct field access. The variant is context, not a separate step.

    /// ```move
    /// struct Coin { balance: u64 }
    /// fun get_balance(c: Coin): u64 { c.balance }
    /// ```
    /// Access path for `c.balance`: Formal(0).balance
    #[test]
    fn test_struct_field_access() {
        let ap = AccessPath::new(Root::formal(0), vec![Offset::field(sym("balance"))]);
        assert_eq!(format!("{}", ap), "Formal(0).balance");
        assert!(ap.is_statically_known());
    }

    /// ```move
    /// enum Option<T> { Some { value: T }, None {} }
    /// fun unwrap<T>(opt: Option<T>): T {
    ///     match (opt) { Some { value } => value, None => abort 0 }
    /// }
    /// ```
    /// Access path for `opt.value` (in the Some branch): Formal(0).value
    /// Same representation as a struct field — the variant is implicit context.
    #[test]
    fn test_enum_variant_field_access() {
        let ap = AccessPath::new(Root::formal(0), vec![Offset::field(sym("value"))]);
        assert_eq!(format!("{}", ap), "Formal(0).value");
        assert!(ap.is_statically_known());
    }

    /// ```move
    /// enum Result<T, E> { Ok { value: T }, Err { value: E } }
    /// ```
    /// Both Ok.value and Err.value produce the same access path:
    /// Formal(0).value — the path doesn't need to distinguish variants.
    #[test]
    fn test_enum_shared_field_name() {
        let ok_path = AccessPath::new(Root::formal(0), vec![Offset::field(sym("value"))]);
        let err_path = AccessPath::new(Root::formal(0), vec![Offset::field(sym("value"))]);
        assert_eq!(ok_path, err_path);
    }

    /// Nested struct inside an enum variant.
    /// ```move
    /// struct Inner { x: u64 }
    /// enum Wrapper { Wrap { inner: Inner } }
    /// ```
    /// Accessing `w.inner.x`: Formal(0).inner.x
    #[test]
    fn test_nested_struct_in_enum() {
        let ap = AccessPath::new(
            Root::formal(0),
            vec![Offset::field(sym("inner")), Offset::field(sym("x"))],
        );
        assert_eq!(format!("{}", ap), "Formal(0).inner.x");
    }

    #[test]
    fn test_display_return_path() {
        let ap = AccessPath::new(Root::ret(0), vec![Offset::vector_index()]);
        assert_eq!(format!("{}", ap), "Ret(0)[_]");
    }

    #[test]
    fn test_root_ordering() {
        let formal0 = Root::formal(0);
        let formal1 = Root::formal(1);
        assert!(formal0 < formal1);
    }

    #[test]
    fn test_offset_ordering() {
        let f1 = Offset::field(sym("a"));
        let f2 = Offset::field(sym("b"));
        assert!(f1 != f2);
    }

    #[test]
    fn test_access_path_equality() {
        let ap1 = AccessPath::new(Root::formal(0), vec![Offset::field(sym("x"))]);
        let ap2 = AccessPath::new(Root::formal(0), vec![Offset::field(sym("x"))]);
        assert_eq!(ap1, ap2);
    }

    #[test]
    fn test_access_path_empty_offsets_is_statically_known() {
        let ap = AccessPath::new_root(Root::formal(0));
        assert!(ap.is_statically_known());
    }

    #[test]
    fn test_dynamic_field_display() {
        let ap = AccessPath::new(
            Root::formal(0),
            vec![Offset::dynamic_field(
                DynamicFieldKind::DynamicField,
                Type::U64,
            )],
        );
        let s = format!("{}", ap);
        assert!(s.contains("dyn"));
        assert!(s.contains("U64"));
    }

    #[test]
    fn test_dynamic_object_field_display() {
        let ap = AccessPath::new(
            Root::formal(0),
            vec![Offset::dynamic_field(
                DynamicFieldKind::DynamicObject,
                Type::Address,
            )],
        );
        let s = format!("{}", ap);
        assert!(s.contains("dyn_obj"));
        assert!(s.contains("Address"));
    }

    /// Dynamic field after struct field: accessing a dynamic child of a
    /// struct's field.
    /// ```move
    /// struct Table { id: UID }
    /// // table.id has dynamic field with key type u64
    /// ```
    /// Path: Formal(0).id/dyn(U64)
    #[test]
    fn test_struct_field_then_dynamic_field() {
        let ap = AccessPath::new(
            Root::formal(0),
            vec![
                Offset::field(sym("id")),
                Offset::dynamic_field(DynamicFieldKind::DynamicField, Type::U64),
            ],
        );
        assert_eq!(format!("{}", ap), "Formal(0).id/dyn(U64)");
        assert!(!ap.is_statically_known());
    }
}
