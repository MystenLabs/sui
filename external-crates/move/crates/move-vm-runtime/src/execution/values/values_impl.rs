// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// THIS IS TO SUPRESS CI ERRORS -- DO NOT MERGE IT
#![allow(deprecated)]

use crate::{
    cache::arena::{Arena, ArenaVec},
    jit::execution::ast::Type,
    shared::views::{ValueView, ValueVisitor},
};
use move_binary_format::{
    errors::*,
    file_format::{Constant, SignatureToken, VariantTag},
};
use move_core_types::{
    account_address::AccountAddress,
    effects::Op,
    gas_algebra::AbstractMemorySize,
    runtime_value::{MoveEnumLayout, MoveStructLayout, MoveTypeLayout},
    u256,
    vm_status::{sub_status::NFE_VECTOR_ERROR_BASE, StatusCode},
    VARIANT_COUNT_MAX,
};
use std::{
    fmt::{self, Debug, Display, Formatter},
    ops::{Index, IndexMut},
};

macro_rules! debug_write {
    ($($toks: tt)*) => {
        write!($($toks)*).map_err(|_|
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("failed to write to buffer".to_string())
        )
    };
}

macro_rules! debug_writeln {
    ($($toks: tt)*) => {
        writeln!($($toks)*).map_err(|_|
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("failed to write to buffer".to_string())
        )
    };
}

// -------------------------------------------------------------------------------------------------
// Value Types
// -------------------------------------------------------------------------------------------------
//  Internal representation of the Move value calculus. These types are abstractions over the
//  concrete Move concepts and may carry additional information that is not defined by the
//  language, but required by the implementation.

#[derive(Debug)]
pub struct MemBox<T: Sized>(std::rc::Rc<std::cell::RefCell<T>>);

#[derive(Debug)]
pub enum Value {
    Invalid,

    // Primitives
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(Box<u128>),
    U256(Box<u256::U256>),
    Bool(bool),
    Address(Box<AccountAddress>),

    // Containers
    Vec(Vec<MemBox<Value>>),
    PrimVec(PrimVec),
    Struct(Struct),
    Variant(Variant),

    // References
    Reference(Reference),
}

#[derive(Debug)]
pub enum PrimVec {
    VecU8(Vec<u8>),
    VecU16(Vec<u16>),
    VecU32(Vec<u32>),
    VecU64(Vec<u64>),
    VecU128(Vec<u128>),
    VecU256(Vec<u256::U256>),
    VecBool(Vec<bool>),
    VecAddress(Vec<AccountAddress>),
}

#[derive(Debug)]
pub struct FixedSizeVec(Box<[MemBox<Value>]>);

/// Runtime representation of a Move value.
#[derive(Debug)]
pub enum Reference {
    Value(MemBox<Value>),
    Indexed(Box<(MemBox<Value>, usize)>),
}

// XXX/TODO(vm-rewrite): Remove this and replace with proper value dirtying.
// This is a temporary shim for the new VM. It _MUST_ be removed before final rollout.
#[derive(Debug)]
pub struct GlobalFingerprint(Option<String>);

// -------------------------------------------------------------------------------------------------
// Alias Types
// -------------------------------------------------------------------------------------------------
// Types visible from outside the module, representing more-precise views of the Value type
// (structs, vectors, etc). They are almost exclusively wrappers around the internal
// representation, acting as public interfaces. The methods they provide closely resemble the Move
// concepts their names suggest: move_local, borrow_field, pack, unpack, etc.
//
/// An integer value in Move.

#[derive(Debug)]
pub enum IntegerValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(u256::U256),
}

#[derive(Debug)]
pub struct Struct(FixedSizeVec);

#[derive(Debug)]
pub struct Variant(Box<(VariantTag, FixedSizeVec)>);

// A vector. This is an alias for a Container for now but we may change
// it once Containers are restructured.
// It's used from vector native functions to get a vector and operate on that.
// There is an impl for Vector which implements the API private to this module.
#[derive(Debug)]
pub struct Vector(Value);

/// A reference to a Move struct that allows you to take a reference to one of its fields.
#[derive(Debug)]
pub struct StructRef(MemBox<Value>);

// A reference to a signer. Clients can attempt a cast to this struct if they are
// expecting a Signer on the stack or as an argument.
#[derive(Debug)]
pub struct SignerRef(MemBox<Value>);

// A reference to a vector. This is an alias for a ContainerRef for now but we may change
// it once Containers are restructured.
// It's used from vector native functions to get a reference to a vector and operate on that.
// There is an impl for VectorRef which implements the API private to this module.
#[derive(Debug)]
pub struct VectorRef(MemBox<Value>);

#[derive(Debug)]
pub struct VariantRef(MemBox<Value>);

// Internal type to ease writing vector operations.
enum VectorMatch<Vec, PrimVec> {
    Vec(Vec),
    PrimVec(PrimVec),
}

#[repr(transparent)]
struct VectorMatchRef<'v>(VectorMatch<&'v Vec<MemBox<Value>>, &'v PrimVec>);
#[repr(transparent)]
struct VectorMatchRefMut<'v>(VectorMatch<&'v mut Vec<MemBox<Value>>, &'v mut PrimVec>);

/// A special "slot" in global storage that can hold a resource. It also keeps track of the status
/// of the resource relative to the global state, which is necessary to compute the effects to emit
/// at the end of transaction execution.
#[derive(Debug)]
pub enum GlobalValueImpl {
    /// No resource resides in this slot or in storage.
    None,
    /// A resource has been published to this slot and it did not previously exist in storage.
    Fresh { container: MemBox<Value> },
    /// A resource resides in this slot and also in storage. The status flag indicates whether
    /// it has potentially been altered.
    Cached {
        fingerprint: GlobalFingerprint,
        container: MemBox<Value>,
    },
    /// A resource used to exist in storage but has been deleted by the current transaction.
    Deleted,
}

/// A wrapper around `GlobalValueImpl`, representing a "slot" in global storage that can
/// hold a resource.
#[derive(Debug)]
pub struct GlobalValue(GlobalValueImpl);

/// Constant representation of a Move value.
#[derive(Debug)]
pub(crate) enum ConstantValue {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(u256::U256),
    Bool(bool),
    Address(AccountAddress),
    Container(ConstantContainer),
}

/// A container is a collection of constant values. It is used to represent data structures like a
/// Move vector or struct.
#[derive(Debug)]
pub(crate) enum ConstantContainer {
    Vec(ArenaVec<ConstantValue>),
    Struct(ArenaVec<ConstantValue>),
    VecU8(ArenaVec<u8>),
    VecU64(ArenaVec<u64>),
    VecU128(ArenaVec<u128>),
    VecBool(ArenaVec<bool>),
    VecAddress(ArenaVec<AccountAddress>),
    VecU16(ArenaVec<u16>),
    VecU32(ArenaVec<u32>),
    VecU256(ArenaVec<u256::U256>),
    Variant(VariantTag, ArenaVec<ConstantValue>),
}

// -------------------------------------------------------------------------------------------------
// Helper Macros
// -------------------------------------------------------------------------------------------------
// Macros to ease writing code later. These appear here due to file ordering requirements.

macro_rules! match_prim_vec {
    ($prim_vec:expr, $items:ident, $rhs:expr) => {
        match $prim_vec {
            PrimVec::VecU8($items) => $rhs,
            PrimVec::VecU16($items) => $rhs,
            PrimVec::VecU32($items) => $rhs,
            PrimVec::VecU64($items) => $rhs,
            PrimVec::VecU128($items) => $rhs,
            PrimVec::VecU256($items) => $rhs,
            PrimVec::VecBool($items) => $rhs,
            PrimVec::VecAddress($items) => $rhs,
        }
    };
}

macro_rules! match_prim_vec_pair {
    ($prim_vec_1:expr, $prim_vec_2:expr, $items_1:ident, $items_2:ident, $rhs:expr, $err:expr) => {
        match ($prim_vec_1, $prim_vec_2) {
            (PrimVec::VecU8($items_1), PrimVec::VecU8($items_2)) => Ok($rhs),
            (PrimVec::VecU16($items_1), PrimVec::VecU16($items_2)) => Ok($rhs),
            (PrimVec::VecU32($items_1), PrimVec::VecU32($items_2)) => Ok($rhs),
            (PrimVec::VecU64($items_1), PrimVec::VecU64($items_2)) => Ok($rhs),
            (PrimVec::VecU128($items_1), PrimVec::VecU128($items_2)) => Ok($rhs),
            (PrimVec::VecU256($items_1), PrimVec::VecU256($items_2)) => Ok($rhs),
            (PrimVec::VecBool($items_1), PrimVec::VecBool($items_2)) => Ok($rhs),
            (PrimVec::VecAddress($items_1), PrimVec::VecAddress($items_2)) => Ok($rhs),
            _ => Err($err),
        }
    };
}

macro_rules! map_prim_vec {
    ($prim_vec:expr, $items:ident, $rhs:expr) => {
        match $prim_vec {
            PrimVec::VecU8($items) => PrimVec::VecU8($rhs),
            PrimVec::VecU16($items) => PrimVec::VecU16($rhs),
            PrimVec::VecU32($items) => PrimVec::VecU32($rhs),
            PrimVec::VecU64($items) => PrimVec::VecU64($rhs),
            PrimVec::VecU128($items) => PrimVec::VecU128($rhs),
            PrimVec::VecU256($items) => PrimVec::VecU256($rhs),
            PrimVec::VecBool($items) => PrimVec::VecBool($rhs),
            PrimVec::VecAddress($items) => PrimVec::VecAddress($rhs),
        }
    };
}

// -------------------------------------------------------------------------------------------------
// Helper Functions
// -------------------------------------------------------------------------------------------------
// Types visible from outside the module. They are almost exclusively wrappers around the internal

impl Value {
    pub fn invalid() -> Value {
        Value::Invalid
    }

    fn variant_ref(&self) -> PartialVMResult<&Variant> {
        if let Value::Variant(variant) = self {
            Ok(variant)
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("{:?} is not a variant", self)),
            )
        }
    }

    fn prim_vec_ref(&self) -> PartialVMResult<&PrimVec> {
        if let Value::PrimVec(prim_vec) = self {
            Ok(prim_vec)
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("{:?} is not a primitive vector", self)),
            )
        }
    }

    fn prim_vec_mut_ref(&mut self) -> PartialVMResult<&mut PrimVec> {
        if let Value::PrimVec(prim_vec) = self {
            Ok(prim_vec)
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("{:?} is not a primitive vector", self)),
            )
        }
    }

    fn vector_ref(&self) -> PartialVMResult<VectorMatchRef<'_>> {
        match self {
            Value::Vec(vec) => Ok(VectorMatchRef(VectorMatch::Vec(vec))),
            Value::PrimVec(mem_box) => Ok(VectorMatchRef(VectorMatch::PrimVec(mem_box))),
            Value::Invalid
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::U256(_)
            | Value::Bool(_)
            | Value::Address(_)
            | Value::Struct(_)
            | Value::Variant(_)
            | Value::Reference(_) => Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message(format!("{:?} is not a vector", self))),
        }
    }

    fn vector_mut_ref(&mut self) -> PartialVMResult<VectorMatchRefMut<'_>> {
        match self {
            Value::Vec(vec) => Ok(VectorMatchRefMut(VectorMatch::Vec(vec))),
            Value::PrimVec(mem_box) => Ok(VectorMatchRefMut(VectorMatch::PrimVec(mem_box))),
            Value::Invalid
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::U256(_)
            | Value::Bool(_)
            | Value::Address(_)
            | Value::Struct(_)
            | Value::Variant(_)
            | Value::Reference(_) => Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message(format!("{:?} is not a vector", self))),
        }
    }
}

impl<T: Debug> MemBox<T> {
    pub fn new(t: T) -> MemBox<T> {
        Self(std::rc::Rc::new(std::cell::RefCell::new(t)))
    }

    pub fn borrow(&self) -> std::cell::Ref<'_, T> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, T> {
        self.0.borrow_mut()
    }

    pub fn take(self) -> PartialVMResult<T> {
        match std::rc::Rc::try_unwrap(self.0) {
            Ok(refcell) => Ok(refcell.into_inner()),
            Err(val) => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    format!("Tried to take value {:?} with dangline references", val),
                ),
            ),
        }
    }

    pub fn replace(&mut self, t: T) -> T {
        self.0.replace(t)
    }

    fn ptr_clone(&self) -> Self {
        MemBox(std::rc::Rc::clone(&self.0))
    }
}

impl MemBox<Value> {
    pub fn as_ref_value(&self) -> Value {
        Value::Reference(Reference::Value(self.ptr_clone()))
    }
}

impl PrimVec {
    /// Returns the length of the vector.
    pub fn len(&self) -> usize {
        match_prim_vec!(self, items, items.len())
    }

    /// Indicate if the vector is emtpy.
    pub fn is_empty(&self) -> bool {
        match_prim_vec!(self, items, items.is_empty())
    }
}

impl FixedSizeVec {
    /// Returns the length of the fixed-size vector.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Creates a `FixedSizeVec` from a `Vec<ValueImpl>`.
    pub fn from_vec(input: Vec<MemBox<Value>>) -> Self {
        FixedSizeVec(input.into_boxed_slice())
    }

    /// Returns an iterator over the `FixedSizeVec`.
    pub fn iter(&self) -> std::slice::Iter<'_, MemBox<Value>> {
        self.0.iter()
    }

    pub fn as_slice(&self) -> &[MemBox<Value>] {
        &self.0
    }
}

impl std::iter::IntoIterator for FixedSizeVec {
    type Item = MemBox<Value>;
    type IntoIter = std::vec::IntoIter<MemBox<Value>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_vec().into_iter()
    }
}

// Implement the `Index` trait to allow immutable indexing.
impl Index<usize> for FixedSizeVec {
    type Output = MemBox<Value>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

// Implement the `IndexMut` trait to allow mutable indexing.
impl IndexMut<usize> for FixedSizeVec {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl GlobalFingerprint {
    pub fn fingerprint(container: &Struct) -> Self {
        // XXX/TODO(vm-rewrite): Implement proper fingerprinting.
        Self(Some(format!("{:?}", container)))
    }

    pub fn dirty() -> Self {
        Self(None)
    }

    pub fn same_value(&self, other: &Struct) -> bool {
        self.0 == Self::fingerprint(other).0
    }
}

trait IndexRef {
    fn copy_element(&self) -> PartialVMResult<Value>;
}

impl IndexRef for Box<(MemBox<Value>, usize)> {
    fn copy_element(&self) -> PartialVMResult<Value> {
        let (vec, ndx) = self.as_ref();
        let opt_ = match vec.borrow().prim_vec_ref()? {
            PrimVec::VecU8(items) => items.get(*ndx).copied().map(Value::U8),
            PrimVec::VecU16(items) => items.get(*ndx).copied().map(Value::U16),
            PrimVec::VecU32(items) => items.get(*ndx).copied().map(Value::U32),
            PrimVec::VecU64(items) => items.get(*ndx).copied().map(Value::U64),
            PrimVec::VecU128(items) => items.get(*ndx).copied().map(|v| Value::U128(Box::new(v))),
            PrimVec::VecU256(items) => items.get(*ndx).cloned().map(|v| Value::U256(Box::new(v))),
            PrimVec::VecBool(items) => items.get(*ndx).copied().map(Value::Bool),
            PrimVec::VecAddress(items) => items
                .get(*ndx)
                .cloned()
                .map(|v| Value::Address(Box::new(v))),
        };
        opt_.ok_or_else(|| PartialVMError::new(StatusCode::INDEX_OUT_OF_BOUNDS))
    }
}

// -------------------------------------------------------------------------------------------------
// Reference Conversions
// -------------------------------------------------------------------------------------------------
// Helpers to obtain a Rust reference to a value via a VM reference. Required for equalities.
// Implementation of Move copy. It is intentional we avoid implementing the standard library trait
// Clone, to prevent surprising behaviors from happening.

impl Value {
    pub fn copy_value(&self) -> Self {
        match self {
            Self::Invalid => Self::Invalid,

            Self::U8(x) => Self::U8(*x),
            Self::U16(x) => Self::U16(*x),
            Self::U32(x) => Self::U32(*x),
            Self::U64(x) => Self::U64(*x),
            Self::U128(v) => Self::U128(Box::new(**v)),
            Self::U256(v) => Self::U256(Box::new(**v)),
            Self::Bool(x) => Self::Bool(*x),
            Self::Address(x) => Self::Address(Box::new(*x.clone())),

            // When cloning a container, we need to make sure we make a deep
            // copy of the data instead of a shallow copy of the Rc.
            Self::Struct(struct_) => Self::Struct(Struct::pack_boxed(
                struct_.0.iter().map(|value| value.copy_value()).collect(),
            )),
            Self::Variant(variant_) => {
                let (tag, entries) = variant_.as_ref();
                let new_entries = entries
                    .iter()
                    .map(|value| value.copy_value())
                    .collect::<Vec<_>>();
                let variant = Variant::pack_boxed(*tag, new_entries);
                Self::Variant(variant)
            }
            Self::Vec(vec) => Self::Vec(vec.iter().map(|value| value.copy_value()).collect()),
            Self::PrimVec(prim_vec) => Self::PrimVec(map_prim_vec!(prim_vec, items, items.clone())),

            Self::Reference(ref_) => Self::Reference(ref_.copy_value()),
        }
    }
}

impl MemBox<Value> {
    pub fn copy_value(&self) -> Self {
        Self::new(self.borrow().copy_value())
    }
}

impl Reference {
    pub fn copy_value(&self) -> Self {
        match self {
            Reference::Value(mem_box) => Reference::Value(mem_box.ptr_clone()),
            Reference::Indexed(entry) => {
                let (vec, ndx) = entry.as_ref();
                Reference::Indexed(Box::new((vec.ptr_clone(), *ndx)))
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Constant Value Conversions
// -------------------------------------------------------------------------------------------------
// Helpers to convert to and from Constant Values, which are what the execution AST holds for
// Constants.

impl Value {
    /// Allocates the constant in the provided arena
    pub(crate) fn into_constant_value(self, arena: &Arena) -> PartialVMResult<ConstantValue> {
        macro_rules! alloc_vec {
            ($values:expr) => {
                arena.alloc_vec($values.into_iter())?
            };
        }
        match self {
            Value::Invalid => Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message("invalid value in constant".to_string())),
            Value::Reference(_) => Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message("invalid reference in constant".to_string())),
            // TODO: auto-gen this?
            Value::U8(value) => Ok(ConstantValue::U8(value)),
            Value::U16(value) => Ok(ConstantValue::U16(value)),
            Value::U32(value) => Ok(ConstantValue::U32(value)),
            Value::U64(value) => Ok(ConstantValue::U64(value)),
            Value::U128(value) => Ok(ConstantValue::U128(*value)),
            Value::U256(value) => Ok(ConstantValue::U256(*value)),
            Value::Bool(value) => Ok(ConstantValue::Bool(value)),
            Value::Address(value) => Ok(ConstantValue::Address(*value)),

            Value::Vec(values) => {
                let constants = values
                    .into_iter()
                    .map(|v| {
                        v.take()
                            .expect("Could not take a value during constant creation")
                            .into_constant_value(arena)
                    })
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let constants = alloc_vec!(constants);
                Ok(ConstantValue::Container(ConstantContainer::Vec(constants)))
            }
            Value::PrimVec(prim_vec) => Ok(ConstantValue::Container(match prim_vec {
                PrimVec::VecU8(values) => ConstantContainer::VecU8(alloc_vec!(values)),
                PrimVec::VecU16(values) => ConstantContainer::VecU16(alloc_vec!(values)),
                PrimVec::VecU32(values) => ConstantContainer::VecU32(alloc_vec!(values)),
                PrimVec::VecU64(values) => ConstantContainer::VecU64(alloc_vec!(values)),
                PrimVec::VecU128(values) => ConstantContainer::VecU128(alloc_vec!(values)),
                PrimVec::VecU256(values) => ConstantContainer::VecU256(alloc_vec!(values)),
                PrimVec::VecBool(values) => ConstantContainer::VecBool(alloc_vec!(values)),
                PrimVec::VecAddress(values) => ConstantContainer::VecAddress(alloc_vec!(values)),
            })),
            Value::Struct(values) => {
                let constants = values
                    .0
                    .into_iter()
                    .map(|v| {
                        v.take()
                            .expect("Could not take a value during constant creation")
                            .into_constant_value(arena)
                    })
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let constants = alloc_vec!(constants);
                Ok(ConstantValue::Container(ConstantContainer::Struct(
                    constants,
                )))
            }
            Value::Variant(variant) => {
                let (tag, values) = *variant.0;
                let constants = values
                    .into_iter()
                    .map(|v| {
                        v.take()
                            .expect("Could not take a value during constant creation")
                            .into_constant_value(arena)
                    })
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let constants = alloc_vec!(constants);
                Ok(ConstantValue::Container(ConstantContainer::Variant(
                    tag, constants,
                )))
            }
        }
    }
}

impl ConstantValue {
    /// Performs a deep copy of the constant value
    pub fn to_value(&self) -> Value {
        match self {
            // TODO: auto-gen this?
            ConstantValue::U8(value) => Value::U8(*value),
            ConstantValue::U16(value) => Value::U16(*value),
            ConstantValue::U32(value) => Value::U32(*value),
            ConstantValue::U64(value) => Value::U64(*value),
            ConstantValue::U128(value) => Value::U128(Box::new(*value)),
            ConstantValue::U256(value) => Value::U256(Box::new(*value)),
            ConstantValue::Bool(value) => Value::Bool(*value),
            ConstantValue::Address(value) => Value::Address(Box::new(*value)),
            ConstantValue::Container(container) => container.to_value(),
        }
    }
}

impl ConstantContainer {
    /// Performs a deep copy of the constant value
    pub fn to_value(&self) -> Value {
        match self {
            ConstantContainer::Vec(const_values) => {
                let values = const_values
                    .iter()
                    .map(ConstantValue::to_value)
                    .map(MemBox::new)
                    .collect::<Vec<_>>();
                Value::Vec(values)
            }
            ConstantContainer::Struct(const_values) => {
                let values = const_values
                    .iter()
                    .map(ConstantValue::to_value)
                    .map(MemBox::new)
                    .collect::<Vec<_>>();
                Value::Struct(Struct::pack_boxed(values))
            }
            // TODO: auto-gen this?
            ConstantContainer::VecU8(const_values) => {
                Value::PrimVec(PrimVec::VecU8(const_values.to_vec()))
            }
            ConstantContainer::VecU64(const_values) => {
                Value::PrimVec(PrimVec::VecU64(const_values.to_vec()))
            }
            ConstantContainer::VecU128(const_values) => {
                Value::PrimVec(PrimVec::VecU128(const_values.to_vec()))
            }
            ConstantContainer::VecBool(const_values) => {
                Value::PrimVec(PrimVec::VecBool(const_values.to_vec()))
            }
            ConstantContainer::VecAddress(const_values) => {
                Value::PrimVec(PrimVec::VecAddress(const_values.to_vec()))
            }
            ConstantContainer::VecU16(const_values) => {
                Value::PrimVec(PrimVec::VecU16(const_values.to_vec()))
            }
            ConstantContainer::VecU32(const_values) => {
                Value::PrimVec(PrimVec::VecU32(const_values.to_vec()))
            }
            ConstantContainer::VecU256(const_values) => {
                Value::PrimVec(PrimVec::VecU256(const_values.to_vec()))
            }
            ConstantContainer::Variant(tag, const_values) => {
                let values = const_values
                    .iter()
                    .map(ConstantValue::to_value)
                    .map(MemBox::new)
                    .collect::<Vec<_>>();
                Value::Variant(Variant::pack_boxed(*tag, values))
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Equality
// -------------------------------------------------------------------------------------------------
// Equality tests of Move values. Errors are raised when types mismatch.
//
// It is intended to NOT use or even implement the standard library traits Eq and Partial Eq due
// to:
// 1. They do not allow errors to be returned.
// 2. They can be invoked without the user being noticed thanks to operator overloading.
//
// Eq and Partial Eq must also NOT be derived for the reasons above plus that the
// derived implementation differs from the semantics we want.

impl Value {
    pub fn equals(&self, other: &Value) -> PartialVMResult<bool> {
        // TODO: auto-gen this?
        match (self, other) {
            (Self::Reference(v1), Self::Reference(v2)) => v1.equals(v2),
            (Self::U8(v1), Self::U8(v2)) => Ok(v1 == v2),
            (Self::U16(v1), Self::U16(v2)) => Ok(v1 == v2),
            (Self::U32(v1), Self::U32(v2)) => Ok(v1 == v2),
            (Self::U64(v1), Self::U64(v2)) => Ok(v1 == v2),
            (Self::U128(v1), Self::U128(v2)) => Ok(v1 == v2),
            (Self::U256(v1), Self::U256(v2)) => Ok(v1 == v2),
            (Self::Bool(v1), Self::Bool(v2)) => Ok(v1 == v2),
            (Self::Address(v1), Self::Address(v2)) => Ok(v1 == v2),
            (Self::PrimVec(v1), Self::PrimVec(v2)) => Ok(v1.len() == v2.len()
                && match_prim_vec_pair!(
                    v1,
                    v2,
                    lhs,
                    rhs,
                    lhs == rhs,
                    PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!("cannot compare values: {:?}, {:?}", v1, v2))
                )?),
            (Self::Vec(v1), Self::Vec(v2)) => Ok(v1.len() == v2.len()
                && v1.iter().zip(v2.iter()).try_fold(true, |acc, (a, b)| {
                    a.borrow().equals(&b.borrow()).map(|eq| acc && eq)
                })?),
            (Self::Struct(v1), Self::Struct(v2)) => Ok(v1.len() == v2.len()
                && v1.iter().zip(v2.iter()).try_fold(true, |acc, (a, b)| {
                    a.borrow().equals(&b.borrow()).map(|eq| acc && eq)
                })?),
            (Self::Variant(tv1), Self::Variant(tv2)) => {
                let (tag1, v1) = tv1.as_ref();
                let (tag2, v2) = tv2.as_ref();
                Ok(tag1 == tag2
                    && v1.len() == v2.len()
                    && v1.iter().zip(v2.iter()).try_fold(true, |acc, (a, b)| {
                        a.borrow().equals(&b.borrow()).map(|eq| acc && eq)
                    })?)
            }
            _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot compare values: {:?}, {:?}", self, other))),
        }
    }
}

impl Reference {
    pub fn equals(&self, other: &Reference) -> PartialVMResult<bool> {
        match (self, other) {
            (Reference::Value(mem_box_1), Reference::Value(mem_box_2)) => {
                mem_box_1.borrow().equals(&mem_box_2.borrow())
            }
            (Reference::Indexed(ref_1), Reference::Indexed(ref_2)) => {
                let (vec_1, ndx_1) = ref_1.as_ref();
                let (vec_2, ndx_2) = ref_2.as_ref();
                match_prim_vec_pair!(
                    vec_1.borrow().prim_vec_ref()?,
                    vec_2.borrow().prim_vec_ref()?,
                    items1,
                    items2,
                    items1[*ndx_1] == items2[*ndx_2],
                    PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!("cannot compare values: {:?}, {:?}", self, other))
                )
            }
            (Reference::Value(mem_box), Reference::Indexed(entry))
            | (Reference::Indexed(entry), Reference::Value(mem_box)) => {
                let box_value = &*mem_box.borrow();
                let (vec, ndx) = entry.as_ref();
                let Value::PrimVec(vec) = &*vec.borrow() else {
                    return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!("invalid indexed reference: {:?}", vec)));
                };
                match (vec, box_value) {
                    (PrimVec::VecU8(lhs), Value::U8(rhs)) => Ok(lhs[*ndx] == *rhs),
                    (PrimVec::VecU16(lhs), Value::U16(rhs)) => Ok(lhs[*ndx] == *rhs),
                    (PrimVec::VecU32(lhs), Value::U32(rhs)) => Ok(lhs[*ndx] == *rhs),
                    (PrimVec::VecU64(lhs), Value::U64(rhs)) => Ok(lhs[*ndx] == *rhs),
                    (PrimVec::VecU128(lhs), Value::U128(rhs)) => Ok(lhs[*ndx] == **rhs),
                    (PrimVec::VecU256(lhs), Value::U256(rhs)) => Ok(lhs[*ndx] == **rhs),
                    (PrimVec::VecBool(lhs), Value::Bool(rhs)) => Ok(lhs[*ndx] == *rhs),
                    (PrimVec::VecAddress(lhs), Value::Address(rhs)) => Ok(lhs[*ndx] == **rhs),
                    _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!("cannot compare values: {:?}, {:?}", self, other))),
                }
            }
        }
    }
}

impl FixedSizeVec {
    pub fn equals(&self, other: &FixedSizeVec) -> PartialVMResult<bool> {
        if self.len() != other.len() {
            return Err(
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(format!(
                    "cannot compare fixed size vectors of different lengths: {:?}, {:?}",
                    self, other
                )),
            );
        }
        for (a, b) in self.iter().zip(other.iter()) {
            if !a.borrow().equals(&b.borrow())? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

// -------------------------------------------------------------------------------------------------
// Read Ref
// -------------------------------------------------------------------------------------------------
// Implementation for the Move operation `read_ref` -- copies the value.

impl Reference {
    pub fn read_ref(self) -> PartialVMResult<Value> {
        match self {
            Reference::Value(mem_box) => Ok(mem_box.borrow().copy_value()),
            Reference::Indexed(index_ref) => index_ref.copy_element(),
        }
    }
}

impl StructRef {
    #[allow(dead_code)]
    pub fn read_ref(self) -> PartialVMResult<Value> {
        Ok(self.0.borrow().copy_value())
    }
}

// -------------------------------------------------------------------------------------------------
// Write Ref
// -------------------------------------------------------------------------------------------------
// Implementation for the Move operation `write_ref`

impl Reference {
    pub fn write_ref(self, value: Value) -> PartialVMResult<()> {
        match self {
            // In this case, we assume a well-typed program, so just write it in
            Reference::Value(mut mem_box) => drop(mem_box.replace(value)),
            Reference::Indexed(index_ref) => {
                macro_rules! assign {
                    // Pattern for boxed assignment: dereference the boxed value.
                    (Box, $vec:expr, $ndx:expr, $variant:ident, $value:expr) => {{
                        let Some(target) = $vec.get_mut($ndx) else {
                            return Err(PartialVMError::new(StatusCode::INDEX_OUT_OF_BOUNDS)
                                .with_message(
                                    "failed in write_ref: index lookup failure".to_string(),
                                ));
                        };
                        let Value::$variant(val_box) = $value else {
                            return Err(PartialVMError::new(
                                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            )
                            .with_message("failed in write_ref: type mismatch".to_string()));
                        };
                        *target = *val_box;
                    }};
                    // Pattern for direct assignment: assign the value directly.
                    ($vec:expr, $ndx:expr, $variant:ident, $value:expr) => {{
                        let Some(target) = $vec.get_mut($ndx) else {
                            return Err(PartialVMError::new(StatusCode::INDEX_OUT_OF_BOUNDS)
                                .with_message(
                                    "failed in write_ref: index lookup failure".to_string(),
                                ));
                        };
                        let Value::$variant(inner_val) = $value else {
                            return Err(PartialVMError::new(
                                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            )
                            .with_message("failed in write_ref: type mismatch".to_string()));
                        };
                        *target = inner_val;
                    }};
                }
                let (vec, ndx) = &*index_ref;
                match &mut *vec.borrow_mut().prim_vec_mut_ref()? {
                    PrimVec::VecU8(items) => assign!(items, *ndx, U8, value),
                    PrimVec::VecU16(items) => assign!(items, *ndx, U16, value),
                    PrimVec::VecU32(items) => assign!(items, *ndx, U32, value),
                    PrimVec::VecU64(items) => assign!(items, *ndx, U64, value),
                    PrimVec::VecU128(items) => assign!(Box, items, *ndx, U128, value),
                    PrimVec::VecU256(items) => assign!(Box, items, *ndx, U256, value),
                    PrimVec::VecBool(items) => assign!(items, *ndx, Bool, value),
                    PrimVec::VecAddress(items) => assign!(Box, items, *ndx, Address, value),
                };
            }
        }
        Ok(())
    }
}

// -------------------------------------------------------------------------------------------------
// Borrowing
// -------------------------------------------------------------------------------------------------
// Implementation of borrowing in Move: convert a value to a reference, borrow field, and
// an element from a vector.

impl StructRef {
    pub fn borrow_field(&self, index: usize) -> PartialVMResult<Value> {
        // Borrow the inner Value from the MemBox.
        let container = self.0.borrow();
        match &*container {
            // If the contained value is a Struct (i.e. a FixedSizeVec),
            // index into it to obtain the desired field.
            Value::Struct(fixed_vec) => {
                // fixed_vec is a FixedSizeVec, so we can use the Index impl.
                let field: &MemBox<Value> = &fixed_vec[index];
                // Return a Value::Reference wrapping a clone of the field.
                Ok(Value::Reference(Reference::Value(field.ptr_clone())))
            }
            // If not a struct, return an error.
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Container is not a struct".to_string()),
            ),
        }
    }
}

impl VariantRef {
    /// Returns the variant tag if the contained value is a variant.
    pub fn get_tag(&self) -> PartialVMResult<VariantTag> {
        Ok(*self.0.borrow().variant_ref()?.as_ref().0)
    }

    /// Checks that the variant tag matches the expected tag.
    pub fn check_tag(&self, expected_tag: VariantTag) -> PartialVMResult<()> {
        let tag = *self.0.borrow().variant_ref()?.as_ref().0;
        println!("borrowed");
        if tag == expected_tag {
            println!("tag checked");
            Ok(())
        } else {
            println!("error");
            println!("formatted");
            Err(
                PartialVMError::new(StatusCode::VARIANT_TAG_MISMATCH).with_message(format!(
                    "Variant tag mismatch: expected {:?}, found {:?}",
                    expected_tag, tag
                )),
            )
        }
    }

    /// Unpacks the variant and returns a Vec of field references (in order).
    /// Each field is returned as a Value::Reference wrapping a cloned pointer.
    pub fn unpack_variant(&self) -> PartialVMResult<Vec<Value>> {
        println!("Unpacking variant: {:?}", self.0);
        let value_ref = self.0.borrow();
        if let Value::Variant(boxed_variant) = &*value_ref {
            let (_tag, fixed_vec) = boxed_variant.as_ref();
            // fixed_vec is a FixedSizeVec; we iterate over its fields.
            let mut result = Vec::new();
            for field in fixed_vec.iter() {
                // Use ptr_clone() to create a new pointer to the field.
                result.push(Value::Reference(Reference::Value(field.ptr_clone())));
            }
            Ok(result)
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Value is not a variant".to_string()),
            )
        }
    }
}

impl VectorRef {
    /// Borrows an element from the container, returning it as a reference wrapped in `ValueImpl::Reference`.
    /// The result is a `PartialVmResult<ValueImpl>` containing the element as a `Reference`.
    pub fn borrow_elem(&self, index: usize, type_param: &Type) -> PartialVMResult<Value> {
        // Borrow the container inside the MemBox.
        let value = &*self.0.borrow();
        check_elem_layout(type_param, value)?;
        match value {
            // For a Vec container, extract the element.
            Value::Vec(vec) => {
                if index >= vec.len() {
                    return Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                        .with_sub_status(INDEX_OUT_OF_BOUNDS)
                        .with_message("Index out of bounds in Vec".to_string()));
                }
                let elem = &vec[index];
                // Return a reference value to the element.
                Ok(Value::Reference(Reference::Value(elem.ptr_clone())))
            }
            // For a primitive vector, return an Indexed reference.
            Value::PrimVec(prim_vec) => {
                // Determine the length of the inner vector.
                let len = match prim_vec {
                    PrimVec::VecU8(items) => items.len(),
                    PrimVec::VecU16(items) => items.len(),
                    PrimVec::VecU32(items) => items.len(),
                    PrimVec::VecU64(items) => items.len(),
                    PrimVec::VecU128(items) => items.len(),
                    PrimVec::VecU256(items) => items.len(),
                    PrimVec::VecBool(items) => items.len(),
                    PrimVec::VecAddress(items) => items.len(),
                };
                if index >= len {
                    return Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                        .with_sub_status(INDEX_OUT_OF_BOUNDS)
                        .with_message("Index out of bounds in PrimVec".to_string()));
                }
                // Return an indexed reference.
                Ok(Value::Reference(Reference::Indexed(Box::new((
                    self.0.ptr_clone(),
                    index,
                )))))
            }
            // If the container is neither a Vec nor a PrimVec, signal an error.
            _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message("Container is not a vector".to_string())),
        }
    }
}

impl SignerRef {
    /// Borrows the signer’s field (at index 0) as a reference.
    pub fn borrow_signer(&self) -> PartialVMResult<Value> {
        // Borrow the inner value from the MemBox.
        let container = self.0.borrow();
        match &*container {
            // Expect a struct, i.e. a FixedSizeVec of fields.
            Value::Struct(fixed_vec) => {
                // Ensure that the struct has exactly one field.
                if fixed_vec.len() != 1 {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(
                                "Signer struct must contain exactly one field".to_string(),
                            ),
                    );
                }
                // Retrieve the 0th element.
                let field = &fixed_vec[0];
                // Return it as a reference by cloning its pointer.
                Ok(Value::Reference(Reference::Value(field.ptr_clone())))
            }
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Container is not a signer".to_string()),
            ),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Public Value Constructios
// -------------------------------------------------------------------------------------------------
// Constructors to allow the creation of values outside of this module.

// Define a macro to generate primitive vector constructors.
macro_rules! impl_vector_fn {
    ($fn_name:ident, $variant:ident, $item_ty:ty) => {
        pub fn $fn_name(it: impl IntoIterator<Item = $item_ty>) -> Self {
            let vec: Vec<$item_ty> = it.into_iter().collect();
            Value::PrimVec(PrimVec::$variant(vec))
        }
    };
}

impl Value {
    pub fn u8(x: u8) -> Self {
        Value::U8(x)
    }

    pub fn u16(x: u16) -> Self {
        Value::U16(x)
    }

    pub fn u32(x: u32) -> Self {
        Value::U32(x)
    }

    pub fn u64(x: u64) -> Self {
        Value::U64(x)
    }

    pub fn u128(x: u128) -> Self {
        Value::U128(Box::new(x))
    }

    pub fn u256(x: u256::U256) -> Self {
        Value::U256(Box::new(x))
    }

    pub fn bool(x: bool) -> Self {
        Value::Bool(x)
    }

    pub fn address(x: AccountAddress) -> Self {
        Value::Address(Box::new(x))
    }

    /// A signer is a special struct containing exactly one field.
    /// Here we represent it as a struct (FixedSizeVec) with one element,
    /// which is the address wrapped as a Value.
    pub fn signer(x: AccountAddress) -> Self {
        let fields = vec![MemBox::new(Value::address(x))];
        Value::Struct(Struct::pack_boxed(fields))
    }

    pub fn struct_(struct_: Struct) -> Self {
        Value::Struct(struct_)
    }

    pub fn make_struct<I: IntoIterator<Item = Value>>(values: I) -> Self {
        Value::Struct(Struct::pack(values))
    }

    pub fn variant(variant: Variant) -> Self {
        Value::Variant(variant)
    }

    pub fn make_variant_boxed<I: IntoIterator<Item = MemBox<Value>>>(
        tag: VariantTag,
        values: I,
    ) -> Self {
        Value::Variant(Variant::pack_boxed(tag, values))
    }

    pub fn make_variant<I: IntoIterator<Item = Value>>(tag: VariantTag, values: I) -> Self {
        Value::Variant(Variant::pack(tag, values))
    }

    impl_vector_fn!(vector_u8, VecU8, u8);
    impl_vector_fn!(vector_u16, VecU16, u16);
    impl_vector_fn!(vector_u32, VecU32, u32);
    impl_vector_fn!(vector_u64, VecU64, u64);
    impl_vector_fn!(vector_u128, VecU128, u128);
    impl_vector_fn!(vector_u256, VecU256, u256::U256);
    impl_vector_fn!(vector_bool, VecBool, bool);
    impl_vector_fn!(vector_address, VecAddress, AccountAddress);

    /// For testing only, construct a vector from an iterator of Values.
    pub fn vector_for_testing_only(it: impl IntoIterator<Item = Value>) -> Self {
        let vec: Vec<MemBox<Value>> = it.into_iter().map(MemBox::new).collect();
        Value::Vec(vec)
    }
}

// -------------------------------------------------------------------------------------------------
// Casting
// -------------------------------------------------------------------------------------------------
// Constructors to allow the creation of values outside of this module. Due to the public value
// types being opaque to an external user, the following public APIs are required to enable
// conversion between types in order to gain access to specific operations certain more refined
// types offer. For example, one must convert a `Value` to a `Struct` before unpack can be called.
//
// It is expected that the caller will keep track of the invariants and guarantee the conversion
// will succeed. An error will be raised in case of a violation.

pub trait VMValueCast<T> {
    fn cast(self) -> PartialVMResult<T>;
}

// Macro to implement casting for primitive Value variants.
macro_rules! impl_vm_value_cast_primitive {
    ($target_ty:ty, $variant:ident, $transform:expr) => {
        impl VMValueCast<$target_ty> for Value {
            fn cast(self) -> PartialVMResult<$target_ty> {
                match self {
                    Value::$variant(x) => Ok($transform(x)),
                    other => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!(
                            "Expected Value::{} but found {:?}",
                            stringify!($variant),
                            other
                        ))),
                }
            }
        }
    };
}

impl_vm_value_cast_primitive!(u8, U8, |x| x);
impl_vm_value_cast_primitive!(u16, U16, |x| x);
impl_vm_value_cast_primitive!(u32, U32, |x| x);
impl_vm_value_cast_primitive!(u64, U64, |x| x);
impl_vm_value_cast_primitive!(u128, U128, |x: Box<u128>| *x);
impl_vm_value_cast_primitive!(u256::U256, U256, |x: Box<u256::U256>| *x);
impl_vm_value_cast_primitive!(bool, Bool, |x| x);
impl_vm_value_cast_primitive!(AccountAddress, Address, |x: Box<AccountAddress>| *x);
impl_vm_value_cast_primitive!(Reference, Reference, |x| x);

impl VMValueCast<StructRef> for Value {
    fn cast(self) -> PartialVMResult<StructRef> {
        match self {
            Value::Reference(r) => {
                match r {
                    // For a direct reference, check the inner value.
                    Reference::Value(mem_box) => {
                        let inner = mem_box.borrow();
                        if let Value::Struct(_) = &*inner {
                            // The reference holds a struct; return a StructRef by cloning the pointer.
                            Ok(StructRef(mem_box.ptr_clone()))
                        } else {
                            Err(
                                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(
                                    format!("Expected a Struct in reference, found {:?}", inner),
                                ),
                            )
                        }
                    }
                    // We do not support indexed references for StructRef conversion.
                    Reference::Indexed(_) => Err(PartialVMError::new(
                        StatusCode::INTERNAL_TYPE_ERROR,
                    )
                    .with_message(
                        "Expected a Struct reference, got an Indexed reference".to_string(),
                    )),
                }
            }
            // Otherwise, it's not a struct.
            other => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Expected a Struct, found {:?}", other))),
        }
    }
}

impl VMValueCast<VariantRef> for Value {
    fn cast(self) -> PartialVMResult<VariantRef> {
        match self {
            Value::Reference(r) => {
                match r {
                    // For a direct reference, check the inner value.
                    Reference::Value(mem_box) => {
                        let inner = mem_box.borrow();
                        if let Value::Variant(_) = &*inner {
                            // The reference holds a struct; return a StructRef by cloning the pointer.
                            Ok(VariantRef(mem_box.ptr_clone()))
                        } else {
                            Err(
                                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(
                                    format!("Expected a Variant in reference, found {:?}", inner),
                                ),
                            )
                        }
                    }
                    // We do not support indexed references for StructRef conversion.
                    Reference::Indexed(_) => Err(PartialVMError::new(
                        StatusCode::INTERNAL_TYPE_ERROR,
                    )
                    .with_message(
                        "Expected a Variant reference, got an Indexed reference".to_string(),
                    )),
                }
            }
            // Otherwise, it's not a struct.
            other => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Expected a Variant, found {:?}", other))),
        }
    }
}

impl VMValueCast<SignerRef> for Value {
    fn cast(self) -> PartialVMResult<SignerRef> {
        match self {
            Value::Reference(r) => {
                match r {
                    // For a direct reference, check the inner value.
                    Reference::Value(mem_box) => {
                        let inner = mem_box.borrow();
                        if let Value::Struct(struct_) = &*inner {
                            if struct_.len() != 1 {
                                return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                                    .with_message(format!(
                                        "Expected signer struct with one field, found {} fields",
                                        struct_.len()
                                    )));
                            };
                            // The reference holds a struct; return a StructRef by cloning the pointer.
                            Ok(SignerRef(mem_box.ptr_clone()))
                        } else {
                            Err(
                                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(
                                    format!("Expected a Struct in reference, found {:?}", inner),
                                ),
                            )
                        }
                    }
                    // We do not support indexed references for StructRef conversion.
                    Reference::Indexed(_) => Err(PartialVMError::new(
                        StatusCode::INTERNAL_TYPE_ERROR,
                    )
                    .with_message(
                        "Expected a Struct reference, got an Indexed reference".to_string(),
                    )),
                }
            }
            // Otherwise, it's not a struct.
            other => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Expected a Struct, found {:?}", other))),
        }
    }
}

impl VMValueCast<VectorRef> for Value {
    fn cast(self) -> PartialVMResult<VectorRef> {
        match self {
            // Direct container case.
            Value::Vec(_) | Value::PrimVec(_) => Ok(VectorRef(MemBox::new(self))),
            // A reference may also wrap a vector-like value.
            Value::Reference(r) => match r {
                Reference::Value(mem_box) => {
                    let inner = mem_box.borrow();
                    match &*inner {
                        Value::Vec(_) | Value::PrimVec(_) => Ok(VectorRef(mem_box.ptr_clone())),
                        _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                            .with_message(format!(
                                "Expected a vector container in reference, found {:?}",
                                inner
                            ))),
                    }
                }
                Reference::Indexed(_) => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(
                        "Expected a vector container, got an Indexed reference".to_string(),
                    )),
            },
            // Otherwise, the value isn't a vector-like container.
            other => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Expected a vector container, found {:?}", other))),
        }
    }
}

impl VMValueCast<Vector> for Value {
    fn cast(self) -> PartialVMResult<Vector> {
        match self {
            // Accept both container forms.
            Value::Vec(_) | Value::PrimVec(_) => Ok(Vector(self)),
            other => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Expected a Vector, found {:?}", other))),
        }
    }
}

impl VMValueCast<Struct> for Value {
    fn cast(self) -> PartialVMResult<Struct> {
        match self {
            // Accept both container forms.
            Value::Struct(struct_) => Ok(struct_),
            other => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Expected a Struct, found {:?}", other))),
        }
    }
}

impl VMValueCast<Variant> for Value {
    fn cast(self) -> PartialVMResult<Variant> {
        match self {
            // Accept both container forms.
            Value::Variant(struct_) => Ok(struct_),
            other => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("Expected a Variant, found {:?}", other))),
        }
    }
}

impl VMValueCast<IntegerValue> for Value {
    fn cast(mut self) -> PartialVMResult<IntegerValue> {
        let value = std::mem::replace(&mut self, Value::Invalid);
        match value {
            Value::U8(x) => Ok(IntegerValue::U8(x)),
            Value::U16(x) => Ok(IntegerValue::U16(x)),
            Value::U32(x) => Ok(IntegerValue::U32(x)),
            Value::U64(x) => Ok(IntegerValue::U64(x)),
            Value::U128(x) => Ok(IntegerValue::U128(*x)),
            Value::U256(x) => Ok(IntegerValue::U256(*x)),
            v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot cast {:?} to integer", v))),
        }
    }
}

macro_rules! impl_vec_vm_value_cast {
    ($vec_type:ty, $prim_vec_type:ident, $error_msg:expr) => {
        impl VMValueCast<Vec<$vec_type>> for Value {
            fn cast(self) -> PartialVMResult<Vec<$vec_type>> {
                match self {
                    Value::PrimVec(prim_vec) => match prim_vec {
                        PrimVec::$prim_vec_type(vec) => Ok(vec),
                        v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                            .with_message(format!($error_msg, v))),
                    },
                    v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!($error_msg, v))),
                }
            }
        }
    };
}

impl_vec_vm_value_cast!(u8, VecU8, "cannot cast {:?} to vector<u8>");
impl_vec_vm_value_cast!(u64, VecU64, "cannot cast {:?} to vector<u64>");
impl_vec_vm_value_cast!(
    AccountAddress,
    VecAddress,
    "cannot cast {:?} to vector<address>"
);

impl VMValueCast<Vec<Value>> for Value {
    fn cast(self) -> PartialVMResult<Vec<Value>> {
        match self {
            Value::Vec(entries) => entries
                .into_iter()
                .map(|entry| entry.take())
                .collect::<PartialVMResult<Vec<_>>>(),
            v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot cast {:?} to Vec<Value>", v))),
        }
    }
}

macro_rules! impl_vm_value_cast_integer {
    ($target_type:ty, $variant:ident, $error_msg:expr) => {
        impl VMValueCast<$target_type> for IntegerValue {
            fn cast(self) -> PartialVMResult<$target_type> {
                match self {
                    Self::$variant(x) => Ok(x),
                    v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!($error_msg, v))),
                }
            }
        }
    };
}

impl_vm_value_cast_integer!(u8, U8, "cannot cast {:?} to u8");
impl_vm_value_cast_integer!(u16, U16, "cannot cast {:?} to u16");
impl_vm_value_cast_integer!(u32, U32, "cannot cast {:?} to u32");
impl_vm_value_cast_integer!(u64, U64, "cannot cast {:?} to u64");
impl_vm_value_cast_integer!(u128, U128, "cannot cast {:?} to u128");
impl_vm_value_cast_integer!(u256::U256, U256, "cannot cast {:?} to u256");

impl IntegerValue {
    pub fn value_as<T>(self) -> PartialVMResult<T>
    where
        Self: VMValueCast<T>,
    {
        VMValueCast::cast(self)
    }
}

impl Value {
    pub fn value_as<T>(self) -> PartialVMResult<T>
    where
        Self: VMValueCast<T>,
    {
        VMValueCast::cast(self)
    }
}

// -------------------------------------------------------------------------------------------------
// Integer Operations
// -------------------------------------------------------------------------------------------------
// Arithmetic operations and conversions for integer values.

macro_rules! checked_arithmetic_op {
    ($func_name:ident, $op:ident, $error_msg:expr) => {
        pub fn $func_name(self, other: Self) -> PartialVMResult<Self> {
            use IntegerValue::*;
            let res = match (self, other) {
                (U8(l), U8(r)) => u8::$op(l, r).map(IntegerValue::U8),
                (U16(l), U16(r)) => u16::$op(l, r).map(IntegerValue::U16),
                (U32(l), U32(r)) => u32::$op(l, r).map(IntegerValue::U32),
                (U64(l), U64(r)) => u64::$op(l, r).map(IntegerValue::U64),
                (U128(l), U128(r)) => u128::$op(l, r).map(IntegerValue::U128),
                (U256(l), U256(r)) => u256::U256::$op(l, r).map(IntegerValue::U256),
                (l, r) => {
                    let msg = format!($error_msg, l, r);
                    return Err(
                        PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(msg)
                    );
                }
            };
            res.ok_or_else(|| PartialVMError::new(StatusCode::ARITHMETIC_ERROR))
        }
    };
}

macro_rules! simple_bitwise_op {
    ($func_name:ident, $op:tt, $error_msg:expr) => {
        pub fn $func_name(self, other: Self) -> PartialVMResult<Self> {
            use IntegerValue::*;
            Ok(match (self, other) {
                (U8(l), U8(r)) => IntegerValue::U8(l $op r),
                (U16(l), U16(r)) => IntegerValue::U16(l $op r),
                (U32(l), U32(r)) => IntegerValue::U32(l $op r),
                (U64(l), U64(r)) => IntegerValue::U64(l $op r),
                (U128(l), U128(r)) => IntegerValue::U128(l $op r),
                (U256(l), U256(r)) => IntegerValue::U256(l $op r),
                (l, r) => {
                    let msg = format!($error_msg, l, r);
                    return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(msg));
                }
            })
        }
    };
}

macro_rules! shift_op {
    ($func_name:ident, $op:tt, $error_msg:expr) => {
        pub fn $func_name(self, n_bits: u8) -> PartialVMResult<Self> {
            use IntegerValue::*;
            Ok(match self {
                U8(x) => {
                    if n_bits >= 8 {
                        return Err(PartialVMError::new(StatusCode::ARITHMETIC_ERROR));
                    }
                    IntegerValue::U8(x $op n_bits)
                }
                U16(x) => {
                    if n_bits >= 16 {
                        return Err(PartialVMError::new(StatusCode::ARITHMETIC_ERROR));
                    }
                    IntegerValue::U16(x $op n_bits)
                }
                U32(x) => {
                    if n_bits >= 32 {
                        return Err(PartialVMError::new(StatusCode::ARITHMETIC_ERROR));
                    }
                    IntegerValue::U32(x $op n_bits)
                }
                U64(x) => {
                    if n_bits >= 64 {
                        return Err(PartialVMError::new(StatusCode::ARITHMETIC_ERROR));
                    }
                    IntegerValue::U64(x $op n_bits)
                }
                U128(x) => {
                    if n_bits >= 128 {
                        return Err(PartialVMError::new(StatusCode::ARITHMETIC_ERROR));
                    }
                    IntegerValue::U128(x $op n_bits)
                }
                U256(x) => IntegerValue::U256(x $op n_bits),
            })
        }
    };
}

macro_rules! comparison_op {
    ($func_name:ident, $op:tt, $error_msg:expr) => {
        pub fn $func_name(self, other: Self) -> PartialVMResult<bool> {
            use IntegerValue::*;
            Ok(match (self, other) {
                (U8(l), U8(r)) => l $op r,
                (U16(l), U16(r)) => l $op r,
                (U32(l), U32(r)) => l $op r,
                (U64(l), U64(r)) => l $op r,
                (U128(l), U128(r)) => l $op r,
                (U256(l), U256(r)) => l $op r,
                (l, r) => {
                    let msg = format!($error_msg, l, r);
                    return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(msg));
                }
            })
        }
    };
}

macro_rules! cast_integer {
    ($func_name:ident, $target_type:ty, $max_value:expr, $unchecked_cast_method:ident) => {
        pub fn $func_name(self) -> PartialVMResult<$target_type> {
            use IntegerValue::*;

            match self {
                U8(x) => Ok(x as $target_type),
                U16(x) => {
                    if x > ($max_value as u16) {
                        Err(
                            PartialVMError::new(StatusCode::ARITHMETIC_ERROR).with_message(
                                format!("Cannot cast u16({}) to {}", x, stringify!($target_type)),
                            ),
                        )
                    } else {
                        Ok(x as $target_type)
                    }
                }
                U32(x) => {
                    if x > ($max_value as u32) {
                        Err(
                            PartialVMError::new(StatusCode::ARITHMETIC_ERROR).with_message(
                                format!("Cannot cast u32({}) to {}", x, stringify!($target_type)),
                            ),
                        )
                    } else {
                        Ok(x as $target_type)
                    }
                }
                U64(x) => {
                    if x > ($max_value as u64) {
                        Err(
                            PartialVMError::new(StatusCode::ARITHMETIC_ERROR).with_message(
                                format!("Cannot cast u64({}) to {}", x, stringify!($target_type)),
                            ),
                        )
                    } else {
                        Ok(x as $target_type)
                    }
                }
                U128(x) => {
                    if x > ($max_value as u128) {
                        Err(
                            PartialVMError::new(StatusCode::ARITHMETIC_ERROR).with_message(
                                format!("Cannot cast u128({}) to {}", x, stringify!($target_type)),
                            ),
                        )
                    } else {
                        Ok(x as $target_type)
                    }
                }
                U256(x) => {
                    if x > u256::U256::from($max_value) {
                        Err(
                            PartialVMError::new(StatusCode::ARITHMETIC_ERROR).with_message(
                                format!("Cannot cast u256({}) to {}", x, stringify!($target_type)),
                            ),
                        )
                    } else {
                        Ok(x.$unchecked_cast_method())
                    }
                }
            }
        }
    };
}

impl IntegerValue {
    // Define arithmetic operations using the checked_arithmetic_op! macro
    checked_arithmetic_op!(add_checked, checked_add, "Cannot add {:?} and {:?}");
    checked_arithmetic_op!(sub_checked, checked_sub, "Cannot sub {:?} from {:?}");
    checked_arithmetic_op!(mul_checked, checked_mul, "Cannot mul {:?} and {:?}");
    checked_arithmetic_op!(div_checked, checked_div, "Cannot div {:?} by {:?}");
    checked_arithmetic_op!(rem_checked, checked_rem, "Cannot rem {:?} by {:?}");

    // Define the bitwise operations using the simple_bitwise_op! macro
    simple_bitwise_op!(bit_or, |, "Cannot bit_or {:?} and {:?}");
    simple_bitwise_op!(bit_and, &, "Cannot bit_and {:?} and {:?}");
    simple_bitwise_op!(bit_xor, ^, "Cannot bit_xor {:?} and {:?}");

    // Define the shift operations using the shift_op! macro
    shift_op!(shl_checked, <<, "Cannot left shift {:?} by {:?}");
    shift_op!(shr_checked, >>, "Cannot right shift {:?} by {:?}");

    // Define the comparison operations using the comparison_op! macro
    comparison_op!(lt, <, "Cannot compare {:?} and {:?}: incompatible integer types");
    comparison_op!(le, <=, "Cannot compare {:?} and {:?}: incompatible integer types");
    comparison_op!(gt, >, "Cannot compare {:?} and {:?}: incompatible integer types");
    comparison_op!(ge, >=, "Cannot compare {:?} and {:?}: incompatible integer types");

    // Generate cast functions for all types up to u256
    cast_integer!(cast_u8, u8, u8::MAX, unchecked_as_u8);
    cast_integer!(cast_u16, u16, u16::MAX, unchecked_as_u16);
    cast_integer!(cast_u32, u32, u32::MAX, unchecked_as_u32);
    cast_integer!(cast_u64, u64, u64::MAX, unchecked_as_u64);
    cast_integer!(cast_u128, u128, u128::MAX, unchecked_as_u128);

    pub fn cast_u256(self) -> PartialVMResult<u256::U256> {
        use IntegerValue::*;
        match self {
            U8(x) => Ok(u256::U256::from(x)),
            U16(x) => Ok(u256::U256::from(x)),
            U32(x) => Ok(u256::U256::from(x)),
            U64(x) => Ok(u256::U256::from(x)),
            U128(x) => Ok(u256::U256::from(x)),
            U256(x) => Ok(x),
        }
    }

    pub fn into_value(self) -> Value {
        use IntegerValue::*;
        match self {
            U8(x) => Value::u8(x),
            U16(x) => Value::u16(x),
            U32(x) => Value::u32(x),
            U64(x) => Value::u64(x),
            U128(x) => Value::u128(x),
            U256(x) => Value::u256(x),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Struct Operations
// -------------------------------------------------------------------------------------------------

impl Struct {
    /// Returns the length of the fixed-size vector.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn pack<I: IntoIterator<Item = Value>>(input: I) -> Self {
        let values = input.into_iter().map(MemBox::new).collect::<Vec<_>>();
        Self::pack_boxed(values)
    }

    pub fn pack_boxed(input: Vec<MemBox<Value>>) -> Self {
        let values = FixedSizeVec(input.into_boxed_slice());
        Struct(values)
    }

    pub fn unpack(self) -> PartialVMResult<impl Iterator<Item = Value>> {
        Ok(self
            .0
            .into_iter()
            .map(|value| value.take())
            .collect::<PartialVMResult<Vec<_>>>()?
            .into_iter())
    }

    /// Returns an iterator over the fields
    pub fn iter(&self) -> std::slice::Iter<'_, MemBox<Value>> {
        self.0.iter()
    }
}

// Implement the `Index` trait to allow immutable indexing.
impl Index<usize> for Struct {
    type Output = MemBox<Value>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

// -------------------------------------------------------------------------------------------------
// Variant Operations
// -------------------------------------------------------------------------------------------------

impl Variant {
    /// Returns the length of the fixed-size vector.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.0.as_ref().1.len()
    }

    pub fn pack<I: IntoIterator<Item = Value>>(tag: VariantTag, input: I) -> Self {
        let values = input.into_iter().map(MemBox::new).collect::<Vec<_>>();
        Self::pack_boxed(tag, values)
    }

    pub fn pack_boxed<I: IntoIterator<Item = MemBox<Value>>>(tag: VariantTag, input: I) -> Self {
        let values = FixedSizeVec(input.into_iter().collect::<Vec<_>>().into_boxed_slice());
        Variant(Box::new((tag, values)))
    }

    pub fn as_ref(&self) -> (&VariantTag, &FixedSizeVec) {
        let (tag, fields) = self.0.as_ref();
        (tag, fields)
    }

    /// Returns an iterator over the fields
    pub fn iter(&self) -> std::slice::Iter<'_, MemBox<Value>> {
        self.0.as_ref().1.iter()
    }

    pub fn unpack(self) -> PartialVMResult<impl Iterator<Item = Value>> {
        let (_tag, fields) = *self.0;
        Ok(fields
            .into_iter()
            .map(|value| value.take())
            .collect::<PartialVMResult<Vec<_>>>()?
            .into_iter())
    }

    pub fn check_tag(&self, expected_tag: VariantTag) -> PartialVMResult<()> {
        let variant_tag = self.0.as_ref().0;
        if expected_tag != variant_tag {
            Err(
                PartialVMError::new(StatusCode::VARIANT_TAG_MISMATCH).with_message(format!(
                    "tag mismatch: expected {}, got {}",
                    expected_tag, variant_tag
                )),
            )
        } else {
            Ok(())
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Vector Operaitons
// -------------------------------------------------------------------------------------------------
// Implemented as a built-in data type.

pub const INDEX_OUT_OF_BOUNDS: u64 = NFE_VECTOR_ERROR_BASE + 1;
pub const POP_EMPTY_VEC: u64 = NFE_VECTOR_ERROR_BASE + 2;
pub const VEC_UNPACK_PARITY_MISMATCH: u64 = NFE_VECTOR_ERROR_BASE + 3;
pub const VEC_SIZE_LIMIT_REACHED: u64 = NFE_VECTOR_ERROR_BASE + 4;

fn check_elem_layout(ty: &Type, v: &Value) -> PartialVMResult<()> {
    macro_rules! allowed_types {
        ($ty:expr; $v:expr; $($allowed:pat),+ $(,)?) => {
            match $ty {
                Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("invalid type param for vector: {:?}", ty)),
                ),
                $(
                    $allowed => Ok(()),
                )+
                _ => Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("vector elem layout mismatch, expected {:?}, got {:?}", $ty, $v))
                ),
            }
        };
    }
    match v {
        Value::Vec(_) => {
            allowed_types!(ty; v; Type::Vector(_), Type::Datatype(_), Type::Signer, Type::DatatypeInstantiation(_))
        }
        Value::PrimVec(prim_vec) => match prim_vec {
            PrimVec::VecU8(_) => allowed_types!(ty; v; Type::U8),
            PrimVec::VecU16(_) => allowed_types!(ty; v; Type::U16),
            PrimVec::VecU32(_) => allowed_types!(ty; v; Type::U32),
            PrimVec::VecU64(_) => allowed_types!(ty; v; Type::U64),
            PrimVec::VecU128(_) => allowed_types!(ty; v; Type::U128),
            PrimVec::VecU256(_) => allowed_types!(ty; v; Type::U256),
            PrimVec::VecBool(_) => allowed_types!(ty; v; Type::Bool),
            PrimVec::VecAddress(_) => allowed_types!(ty; v; Type::Address),
        },
        Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_)
        | Value::U256(_)
        | Value::Bool(_)
        | Value::Address(_)
        | Value::Struct(_)
        | Value::Variant(_)
        | Value::Invalid
        | Value::Reference(_) => Err(PartialVMError::new(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        )
        .with_message(format!("value {:?} is not a vector", v))),
    }
}

macro_rules! match_vec_ref_container {
    (
        (mut $vec:ident)
        prim $r:ident => $prim_expr:expr;
        vec $r_vec:ident => $vec_expr:expr;
    ) => {
        match $vec.0 {
            VectorMatch::PrimVec(prim_vec) => match_prim_vec!(&mut *prim_vec, $r, $prim_expr),
            VectorMatch::Vec($r_vec) => $vec_expr,
        }
    };

    (
        ($vec:ident)
        prim $r:ident => $prim_expr:expr;
        vec $r_vec:ident => $vec_expr:expr;
    ) => {
        match $vec.0 {
            VectorMatch::PrimVec(prim_vec) => match_prim_vec!(&*prim_vec, $r, $prim_expr),
            VectorMatch::Vec($r_vec) => $vec_expr,
        }
    };
}

impl VectorMatchRef<'_> {
    fn len(&self) -> usize {
        match &self.0 {
            VectorMatch::Vec(vec) => vec.len(),
            VectorMatch::PrimVec(mem_box) => mem_box.len(),
        }
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl VectorMatchRefMut<'_> {
    fn len(&self) -> usize {
        match &self.0 {
            VectorMatch::Vec(vec) => vec.len(),
            VectorMatch::PrimVec(mem_box) => mem_box.len(),
        }
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// A custom type that holds both the outer borrow and the inner borrow.
pub struct VecU8Ref<'a> {
    // The outer borrow (of the Value) must be kept alive.
    _outer: std::cell::Ref<'a, Value>,
    // The inner borrow (of the PrimVec) is mapped to a &Vec<u8>.
    inner: std::cell::Ref<'a, Vec<u8>>,
}

impl std::ops::Deref for VecU8Ref<'_> {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl VectorRef {
    pub fn len(&self, type_param: &Type) -> PartialVMResult<Value> {
        let value = &*self.0.borrow();
        check_elem_layout(type_param, value)?;
        value
            .vector_ref()
            .map(|vec| vec.len() as u64)
            .map(Value::U64)
    }

    pub fn push_back(&self, e: Value, type_param: &Type, capacity: u64) -> PartialVMResult<()> {
        let value = &mut *self.0.borrow_mut();
        check_elem_layout(type_param, value)?;
        let vec = value.vector_mut_ref()?;
        let size = vec.len();

        if size >= (capacity as usize) {
            return Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                .with_sub_status(VEC_SIZE_LIMIT_REACHED)
                .with_message(format!("vector size limit is {capacity}",)));
        }

        match_vec_ref_container!(
            (mut vec)
            prim r => r.push(VMValueCast::cast(e)?);
            vec r => r.push(MemBox::new(e));
        );
        Ok(())
    }

    pub fn as_bytes_ref(&self) -> std::cell::Ref<'_, Vec<u8>> {
        std::cell::Ref::map(self.0.borrow(), |value| match value {
            Value::PrimVec(PrimVec::VecU8(vec)) => vec,
            Value::PrimVec(_)
            | Value::Invalid
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::U256(_)
            | Value::Bool(_)
            | Value::Address(_)
            | Value::Vec(_)
            | Value::Struct(_)
            | Value::Variant(_)
            | Value::Reference(_) => panic!("can only be called on vector<u8>"),
        })
    }

    pub fn pop(&self, type_param: &Type) -> PartialVMResult<Value> {
        let value = &mut *self.0.borrow_mut();
        check_elem_layout(type_param, value)?;

        macro_rules! pop_vec_item {
            ($items:expr, $value:ident, $rhs:expr) => {
                match $items.pop() {
                    Some($value) => Ok($rhs),
                    None => Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                        .with_sub_status(POP_EMPTY_VEC)),
                }
            };
        }

        use PrimVec as PV;
        use VectorMatch as V;

        match value.vector_mut_ref()?.0 {
            V::PrimVec(PV::VecU8(xs)) => pop_vec_item!(xs, x, Value::U8(x)),
            V::PrimVec(PV::VecU16(xs)) => pop_vec_item!(xs, x, Value::U16(x)),
            V::PrimVec(PV::VecU32(xs)) => pop_vec_item!(xs, x, Value::U32(x)),
            V::PrimVec(PV::VecU64(xs)) => pop_vec_item!(xs, x, Value::U64(x)),
            V::PrimVec(PV::VecU128(xs)) => pop_vec_item!(xs, x, Value::U128(Box::new(x))),
            V::PrimVec(PV::VecU256(xs)) => pop_vec_item!(xs, x, Value::U256(Box::new(x))),
            V::PrimVec(PV::VecBool(xs)) => pop_vec_item!(xs, x, Value::Bool(x)),
            V::PrimVec(PV::VecAddress(xs)) => pop_vec_item!(xs, x, Value::Address(Box::new(x))),
            V::Vec(items) => pop_vec_item!(items, value, value.take()?),
        }
    }

    pub fn swap(&self, idx1: usize, idx2: usize, type_param: &Type) -> PartialVMResult<()> {
        let value = &mut *self.0.borrow_mut();
        check_elem_layout(type_param, value)?;

        macro_rules! swap {
            ($v: expr) => {{
                let v = $v;
                if idx1 >= v.len() || idx2 >= v.len() {
                    return Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                        .with_sub_status(INDEX_OUT_OF_BOUNDS));
                }
                v.swap(idx1, idx2);
            }};
        }

        use PrimVec as PV;
        use VectorMatch as V;

        match value.vector_mut_ref()?.0 {
            V::PrimVec(PV::VecU8(xs)) => swap!(xs),
            V::PrimVec(PV::VecU16(xs)) => swap!(xs),
            V::PrimVec(PV::VecU32(xs)) => swap!(xs),
            V::PrimVec(PV::VecU64(xs)) => swap!(xs),
            V::PrimVec(PV::VecU128(xs)) => swap!(xs),
            V::PrimVec(PV::VecU256(xs)) => swap!(xs),
            V::PrimVec(PV::VecBool(xs)) => swap!(xs),
            V::PrimVec(PV::VecAddress(xs)) => swap!(xs),
            V::Vec(items) => swap!(items),
        }
        Ok(())
    }
}

macro_rules! pack_vector {
    ($elements:expr, $vector_fn:expr) => {
        $vector_fn(
            $elements
                .into_iter()
                .map(|v| VMValueCast::cast(v))
                .collect::<PartialVMResult<Vec<_>>>()?,
        )
    };
}

macro_rules! take_and_map {
    ($container:expr, $map_fn:expr) => {
        $container.into_iter().map($map_fn).collect::<Vec<_>>()
    };
}

impl Vector {
    pub fn pack(type_param: &Type, elements: Vec<Value>) -> PartialVMResult<Value> {
        let container = match type_param {
            Type::U8 => pack_vector!(elements, Value::vector_u8),
            Type::U16 => pack_vector!(elements, Value::vector_u16),
            Type::U32 => pack_vector!(elements, Value::vector_u32),
            Type::U64 => pack_vector!(elements, Value::vector_u64),
            Type::U128 => pack_vector!(elements, Value::vector_u128),
            Type::U256 => pack_vector!(elements, Value::vector_u256),
            Type::Bool => pack_vector!(elements, Value::vector_bool),
            Type::Address => pack_vector!(elements, Value::vector_address),

            Type::Signer | Type::Vector(_) | Type::Datatype(_) | Type::DatatypeInstantiation(_) => {
                Value::Vec(elements.into_iter().map(MemBox::new).collect())
            }

            Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("invalid type param for vector: {:?}", type_param)),
                )
            }
        };

        Ok(container)
    }

    pub fn empty(type_param: &Type) -> PartialVMResult<Value> {
        Self::pack(type_param, vec![])
    }

    pub fn unpack(self, type_param: &Type, expected_num: u64) -> PartialVMResult<Vec<Value>> {
        check_elem_layout(type_param, &self.0)?;

        use PrimVec as PV;
        use Value as V;

        let elements: Vec<Value> = match self.0 {
            V::PrimVec(PV::VecU8(xs)) => take_and_map!(xs, Value::U8),
            V::PrimVec(PV::VecU16(xs)) => take_and_map!(xs, Value::U16),
            V::PrimVec(PV::VecU32(xs)) => take_and_map!(xs, Value::U32),
            V::PrimVec(PV::VecU64(xs)) => take_and_map!(xs, Value::U64),
            V::PrimVec(PV::VecU128(xs)) => take_and_map!(xs, |x| Value::U128(Box::new(x))),
            V::PrimVec(PV::VecU256(xs)) => take_and_map!(xs, |x| Value::U256(Box::new(x))),
            V::PrimVec(PV::VecBool(xs)) => take_and_map!(xs, Value::Bool),
            V::PrimVec(PV::VecAddress(xs)) => take_and_map!(xs, |x| Value::Address(Box::new(x))),
            V::Vec(items) => items
                .into_iter()
                .map(|v| v.take())
                .collect::<PartialVMResult<Vec<_>>>()?,
            value => {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(format!("{:?} is not a vector", value)),
                )
            }
        };
        if expected_num as usize == elements.len() {
            Ok(elements)
        } else {
            Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                .with_sub_status(VEC_UNPACK_PARITY_MISMATCH))
        }
    }

    pub fn destroy_empty(self, type_param: &Type) -> PartialVMResult<()> {
        self.unpack(type_param, 0)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn to_vec_u8(self) -> PartialVMResult<Vec<u8>> {
        check_elem_layout(&Type::U8, &self.0)?;
        if let Value::PrimVec(PrimVec::VecU8(xs)) = self.0 {
            Ok(xs.into_iter().collect())
        } else {
            Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("expected vector<u8>".to_string()),
            )
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Abstract Memory Size
// -------------------------------------------------------------------------------------------------
// TODO(gas): This is the oldest implementation of abstract memory size. It is now kept only as a
// reference impl, which is used to ensure the new implementation is fully backward compatible. We
// should be able to get this removed after we use the new impl for a while and gain enough
// confidence in that.

/// The size in bytes for a non-string or address constant on the stack
pub(crate) const LEGACY_CONST_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);

/// The size in bytes for a reference on the stack
pub(crate) const LEGACY_REFERENCE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);

/// The size of a struct in bytes
pub(crate) const LEGACY_STRUCT_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);

impl Value {
    // TODO(vm-rewrite): Rename this
    // We assume Variant is defined as: type Variant = (VariantTag, FixedSizeVec);
    // and that VariantTag is Copy.
    const TAG_SIZE: AbstractMemorySize = AbstractMemorySize::new(std::mem::size_of::<u16>() as u64);

    #[deprecated(note = "Update this to not use the legacy size")]
    pub fn legacy_size(&self) -> AbstractMemorySize {
        use Value::*;
        match self {
            // All scalar primitives use the legacy constant size.
            Invalid | U8(_) | U16(_) | U32(_) | U64(_) | U128(_) | U256(_) | Bool(_) => {
                LEGACY_CONST_SIZE
            }
            // AccountAddress uses its fixed length.
            Address(_) => AbstractMemorySize::new(AccountAddress::LENGTH as u64),
            // A vector container: delegate to its legacy size implementation.
            Vec(vec) => vec.iter().fold(LEGACY_STRUCT_SIZE, |acc, field| {
                acc + field.borrow().legacy_size()
            }),
            // A primitive vector: borrow its inner PrimVec and compute its legacy size.
            PrimVec(prim_vec) => prim_vec.legacy_size(),
            // A struct is a FixedSizeVec.
            Struct(s) => s.0.legacy_size(),
            // A variant is a boxed tuple (VariantTag, FixedSizeVec).
            Variant(var_box) => {
                let (_tag, fixed_vec) = var_box.as_ref();
                fixed_vec.legacy_size() + Self::TAG_SIZE
            }
            // References have a fixed legacy size.
            Reference(_) => LEGACY_REFERENCE_SIZE,
        }
    }
}

impl FixedSizeVec {
    /// Computes the legacy size of a struct by folding over its fields.
    pub fn legacy_size(&self) -> AbstractMemorySize {
        // Start with a base overhead for a struct.
        self.0.iter().fold(LEGACY_STRUCT_SIZE, |acc, field| {
            acc + field.borrow().legacy_size()
        })
    }
}

impl Reference {
    #[cfg(test)]
    /// For testing purposes, the legacy size of any reference is fixed.
    pub fn legacy_size(&self) -> AbstractMemorySize {
        LEGACY_REFERENCE_SIZE
    }
}

impl PrimVec {
    /// Computes the legacy size of a primitive vector.
    ///
    /// Here we assume that each element contributes `LEGACY_CONST_SIZE` bytes,
    /// and that the overhead is zero (or could be adjusted if needed).
    pub fn legacy_size(&self) -> AbstractMemorySize {
        let overhead = AbstractMemorySize::new(0);
        fn size_count<T>(items: &[T]) -> AbstractMemorySize {
            AbstractMemorySize::new(items.len() as u64 * std::mem::size_of::<T>() as u64)
        }
        overhead
            + match self {
                PrimVec::VecU8(items) => size_count(items),
                PrimVec::VecU16(items) => size_count(items),
                PrimVec::VecU32(items) => size_count(items),
                PrimVec::VecU64(items) => size_count(items),
                PrimVec::VecU128(items) => size_count(items),
                PrimVec::VecU256(items) => size_count(items),
                PrimVec::VecBool(items) => size_count(items),
                PrimVec::VecAddress(items) => size_count(items),
            }
    }
}

impl Struct {
    #[cfg(test)]
    pub(crate) fn legacy_size(&self) -> AbstractMemorySize {
        self.0.legacy_size()
    }
}

// -------------------------------------------------------------------------------------------------
// Global Value Operations
// -------------------------------------------------------------------------------------------------
// Public APIs for GlobalValue. They allow global values to be created from external source (a.k.a.
// storage), and references to be taken from them. At the end of the transaction execution the
// dirty ones can be identified and wrote back to storage.

#[allow(clippy::unnecessary_wraps)]
impl GlobalValueImpl {
    fn cached(
        val: Value,
        existing_fingerprint: Option<GlobalFingerprint>,
    ) -> Result<Self, (PartialVMError, Value)> {
        match val {
            Value::Struct(struct_) => {
                let fingerprint = existing_fingerprint
                    .unwrap_or_else(|| GlobalFingerprint::fingerprint(&struct_));
                Ok(Self::Cached {
                    container: MemBox::new(Value::Struct(struct_)),
                    fingerprint,
                })
            }
            val => Err((
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("failed to publish cached: not a resource".to_string()),
                val,
            )),
        }
    }

    fn fresh(val: Value) -> Result<Self, (PartialVMError, Value)> {
        match val {
            container @ Value::Struct(_) => Ok(Self::Fresh {
                container: MemBox::new(container),
            }),
            val => Err((
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("failed to publish fresh: not a resource".to_string()),
                val,
            )),
        }
    }

    fn move_from(&mut self) -> PartialVMResult<Value> {
        let value = std::mem::replace(self, Self::None);
        let value_box = match value {
            Self::None | Self::Deleted => {
                return Err(PartialVMError::new(StatusCode::MISSING_DATA))
            }
            Self::Fresh { container } => {
                let previous = std::mem::replace(self, Self::None);
                assert!(matches!(previous, Self::None));
                container
            }
            Self::Cached {
                fingerprint: _,
                container,
            } => {
                let previous = std::mem::replace(self, Self::Deleted);
                assert!(matches!(previous, Self::None));
                container
            }
        };
        value_box.take()
    }

    fn move_to(&mut self, val: Value) -> Result<(), (PartialVMError, Value)> {
        match self {
            Self::Fresh { .. } | Self::Cached { .. } => {
                return Err((
                    PartialVMError::new(StatusCode::RESOURCE_ALREADY_EXISTS),
                    val,
                ))
            }
            Self::None => *self = Self::fresh(val)?,
            Self::Deleted => {
                let Self::Deleted = std::mem::replace(self, Self::None) else {
                    unreachable!()
                };
                *self = Self::cached(val, Some(GlobalFingerprint::dirty()))?
            }
        }
        Ok(())
    }

    fn exists(&self) -> PartialVMResult<bool> {
        match self {
            Self::Fresh { .. } | Self::Cached { .. } => Ok(true),
            Self::None | Self::Deleted => Ok(false),
        }
    }

    fn borrow_global(&self) -> PartialVMResult<Value> {
        match self {
            Self::None | Self::Deleted => Err(PartialVMError::new(StatusCode::MISSING_DATA)),
            GlobalValueImpl::Fresh { container } => {
                Ok(Value::Reference(Reference::Value(container.ptr_clone())))
            }
            GlobalValueImpl::Cached { container, .. } => {
                Ok(Value::Reference(Reference::Value(container.ptr_clone())))
            }
        }
    }

    fn into_effect(self) -> Option<Op<Value>> {
        match self {
            Self::None => None,
            Self::Deleted => Some(Op::Delete),
            Self::Fresh { container } => {
                let value @ Value::Struct(_) = container
                    .take()
                    .expect("Tried to take a global value in use")
                else {
                    unreachable!()
                };
                Some(Op::New(value))
            }
            Self::Cached {
                container,
                fingerprint,
            } => {
                let Value::Struct(struct_) = container
                    .take()
                    .expect("Tried to take a global value in use")
                else {
                    unreachable!()
                };
                if fingerprint.same_value(&struct_) {
                    None
                } else {
                    Some(Op::New(Value::Struct(struct_)))
                }
            }
        }
    }

    fn is_mutated(&self) -> bool {
        match self {
            Self::None => false,
            Self::Deleted => true,
            Self::Fresh { .. } => true,
            Self::Cached {
                fingerprint,
                container,
            } => {
                let Value::Struct(struct_) = &*container.borrow() else {
                    unreachable!()
                };
                !fingerprint.same_value(struct_)
            }
        }
    }

    fn into_value(self) -> Option<Value> {
        match self {
            Self::None | Self::Deleted => None,
            Self::Fresh { container } | Self::Cached { container, .. } => {
                let struct_ @ Value::Struct(_)
                    = container.take().expect("Could not take global value ") else {
                    unreachable!()
                };
                Some(struct_)
            }
        }
    }
}

impl GlobalValue {
    pub fn none() -> Self {
        Self(GlobalValueImpl::None)
    }

    pub fn cached(val: Value) -> PartialVMResult<Self> {
        Ok(Self(
            GlobalValueImpl::cached(val, None).map_err(|(err, _val)| err)?,
        ))
    }

    pub fn move_from(&mut self) -> PartialVMResult<Value> {
        self.0.move_from()
    }

    pub fn move_to(&mut self, val: Value) -> Result<(), (PartialVMError, Value)> {
        self.0.move_to(val)
    }

    pub fn borrow_global(&self) -> PartialVMResult<Value> {
        self.0.borrow_global()
    }

    pub fn exists(&self) -> PartialVMResult<bool> {
        self.0.exists()
    }

    pub fn into_effect(self) -> Option<Op<Value>> {
        self.0.into_effect()
    }

    pub fn is_mutated(&self) -> bool {
        self.0.is_mutated()
    }

    pub fn into_value(self) -> Option<Value> {
        self.0.into_value()
    }
}

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------
// VM Value Displays for easier reading

fn display_list_of_items<T, I>(items: I, f: &mut fmt::Formatter) -> fmt::Result
where
    T: Display,
    I: IntoIterator<Item = T>,
{
    write!(f, "[")?;
    let mut items = items.into_iter();
    if let Some(x) = items.next() {
        write!(f, "{}", x)?;
        for x in items {
            write!(f, ", {}", x)?;
        }
    }
    write!(f, "]")
}

impl Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Invalid => write!(f, "Invalid"),
            // Primitives
            Value::U8(x) => write!(f, "U8({})", x),
            Value::U16(x) => write!(f, "U16({})", x),
            Value::U32(x) => write!(f, "U32({})", x),
            Value::U64(x) => write!(f, "U64({})", x),
            Value::U128(x) => write!(f, "U128({})", x),
            Value::U256(x) => write!(f, "U256({})", x),
            Value::Bool(x) => write!(f, "Bool({})", x),
            Value::Address(addr) => write!(f, "Address({})", addr.short_str_lossless()),

            // Containers
            Value::Vec(vec) => {
                write!(f, "Vec(")?;
                display_list_of_items(vec.iter().map(|v| v.borrow()), f)?;
                write!(f, "])")
            }
            Value::PrimVec(prim_vec) => write!(f, "PrimVec({})", prim_vec),
            Value::Struct(s) => write!(f, "Struct({})", s.0),
            Value::Variant(var) => {
                let (tag, fields) = var.as_ref();
                write!(f, "Variant(tag: {}, fields: {})", tag, fields)
            }

            // References
            Value::Reference(r) => write!(f, "Reference({})", r),
        }
    }
}

impl Display for PrimVec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        macro_rules! display_items {
            ($name:expr, $items:expr) => {{
                write!(f, "{}(", $name)?;
                display_list_of_items($items, f)?;
                write!(f, ")")
            }};
        }
        match self {
            PrimVec::VecU8(items) => display_items!("VecU8", items),
            PrimVec::VecU16(items) => display_items!("VecU16", items),
            PrimVec::VecU32(items) => display_items!("VecU32", items),
            PrimVec::VecU64(items) => display_items!("VecU64", items),
            PrimVec::VecU128(items) => display_items!("VecU128", items),
            PrimVec::VecU256(items) => display_items!("VecU256", items),
            PrimVec::VecBool(items) => display_items!("VecBool", items),
            PrimVec::VecAddress(items) => display_items!("VecAddress", items),
        }
    }
}

impl Display for FixedSizeVec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        // Display the struct as a list of its fields.
        // Each field is a MemBox<Value>; we borrow the inner value to display it.
        write!(f, "Struct(")?;
        display_list_of_items(self.0.iter().map(|mb| mb.borrow()), f)?;
        write!(f, ")")
    }
}

impl Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reference::Value(mem_box) => write!(f, "{}", mem_box.borrow()),
            Reference::Indexed(index_ref) => {
                let (vec, ndx) = index_ref.as_ref();
                let Value::PrimVec(vec) = &*vec.borrow() else {
                    unreachable!()
                };
                match_prim_vec!(vec, vec, write!(f, "{}", vec[*ndx]))
            }
        }
    }
}

impl fmt::Display for ConstantValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstantValue::U8(x) => write!(f, "{}", x),
            ConstantValue::U16(x) => write!(f, "{}", x),
            ConstantValue::U32(x) => write!(f, "{}", x),
            ConstantValue::U64(x) => write!(f, "{}", x),
            ConstantValue::U128(x) => write!(f, "{}", x),
            ConstantValue::U256(x) => write!(f, "{}", x),
            ConstantValue::Bool(b) => write!(f, "{}", b),
            ConstantValue::Address(addr) => write!(f, "{}", addr),
            ConstantValue::Container(c) => write!(f, "{}", c),
        }
    }
}

impl fmt::Display for ConstantContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstantContainer::Vec(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::Struct(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::VecU8(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::VecU64(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::VecU128(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::VecBool(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::VecAddress(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::VecU16(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::VecU32(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::VecU256(vec) => display_list_of_items(vec.iter(), f),
            ConstantContainer::Variant(tag, vec) => {
                write!(f, "|tag: {}|", tag)?;
                display_list_of_items(vec.iter(), f)
            }
        }
    }
}

#[allow(dead_code)]
pub mod debug {
    use crate::execution::interpreter::locals::StackFrame;

    use super::*;
    use std::fmt::Write;

    fn print_invalid<B: Write>(buf: &mut B) -> PartialVMResult<()> {
        debug_write!(buf, "-")
    }

    fn print_u8<B: Write>(buf: &mut B, x: &u8) -> PartialVMResult<()> {
        debug_write!(buf, "{}", x)
    }

    fn print_u16<B: Write>(buf: &mut B, x: &u16) -> PartialVMResult<()> {
        debug_write!(buf, "{}", x)
    }

    fn print_u32<B: Write>(buf: &mut B, x: &u32) -> PartialVMResult<()> {
        debug_write!(buf, "{}", x)
    }

    fn print_u64<B: Write>(buf: &mut B, x: &u64) -> PartialVMResult<()> {
        debug_write!(buf, "{}", x)
    }

    fn print_u128<B: Write>(buf: &mut B, x: &u128) -> PartialVMResult<()> {
        debug_write!(buf, "{}", x)
    }

    fn print_u256<B: Write>(buf: &mut B, x: &u256::U256) -> PartialVMResult<()> {
        debug_write!(buf, "{}", x)
    }

    fn print_bool<B: Write>(buf: &mut B, x: &bool) -> PartialVMResult<()> {
        debug_write!(buf, "{}", x)
    }

    fn print_address<B: Write>(buf: &mut B, x: &AccountAddress) -> PartialVMResult<()> {
        debug_write!(buf, "{}", x)
    }

    fn print_value_impl<B: Write>(buf: &mut B, val: &Value) -> PartialVMResult<()> {
        match val {
            Value::Invalid => print_invalid(buf),
            Value::U8(x) => print_u8(buf, x),
            Value::U16(x) => print_u16(buf, x),
            Value::U32(x) => print_u32(buf, x),
            Value::U64(x) => print_u64(buf, x),
            Value::U128(x) => print_u128(buf, x),
            Value::U256(x) => print_u256(buf, x),
            Value::Bool(x) => print_bool(buf, x),
            Value::Address(x) => print_address(buf, x),
            Value::Reference(r) => print_reference(buf, r),
            Value::Vec(items) => print_list(buf, "[", items.iter(), print_box_value_impl, "]"),
            Value::PrimVec(prim_vec) => match prim_vec {
                PrimVec::VecU8(items) => print_list(buf, "[", items.iter(), print_u8, "]"),
                PrimVec::VecU16(items) => print_list(buf, "[", items.iter(), print_u16, "]"),
                PrimVec::VecU32(items) => print_list(buf, "[", items.iter(), print_u32, "]"),
                PrimVec::VecU64(items) => print_list(buf, "[", items.iter(), print_u64, "]"),
                PrimVec::VecU128(items) => print_list(buf, "[", items.iter(), print_u128, "]"),
                PrimVec::VecU256(u256s) => print_list(buf, "[", u256s.iter(), print_u256, "]"),
                PrimVec::VecBool(items) => print_list(buf, "[", items.iter(), print_bool, "]"),
                PrimVec::VecAddress(items) => {
                    print_list(buf, "[", items.iter(), print_address, "]")
                }
            },
            Value::Struct(fields) => {
                print_list(buf, "{ ", fields.iter(), print_box_value_impl, " }")
            }
            Value::Variant(variant_) => {
                let (tag, fields) = variant_.as_ref();
                print_list(
                    buf,
                    &format!("|{}|{{ ", tag),
                    fields.iter(),
                    print_box_value_impl,
                    " }",
                )
            }
        }
    }

    #[allow(clippy::borrowed_box)]
    fn print_box_value_impl<B: Write>(buf: &mut B, val: &MemBox<Value>) -> PartialVMResult<()> {
        print_value_impl(buf, &val.borrow())
    }

    fn print_list<'a, B, I, X, F>(
        buf: &mut B,
        begin: &str,
        items: I,
        print: F,
        end: &str,
    ) -> PartialVMResult<()>
    where
        B: Write,
        X: 'a,
        I: IntoIterator<Item = &'a X>,
        F: Fn(&mut B, &X) -> PartialVMResult<()>,
    {
        debug_write!(buf, "{}", begin)?;
        let mut it = items.into_iter();
        if let Some(x) = it.next() {
            print(buf, x)?;
            for x in it {
                debug_write!(buf, ", ")?;
                print(buf, x)?;
            }
        }
        debug_write!(buf, "{}", end)?;
        Ok(())
    }

    // TODO: This function was used in an old implementation of std::debug::print, and can probably be removed.
    pub fn print_reference<B: Write>(buf: &mut B, r: &Reference) -> PartialVMResult<()> {
        debug_write!(buf, "(&) ")?;
        match r {
            Reference::Value(mem_box) => print_box_value_impl(buf, mem_box),
            Reference::Indexed(entry) => print_value_impl(buf, &entry.copy_element()?),
        }
    }

    pub fn print_stack_frame<B: Write>(
        buf: &mut B,
        stack_frame: &StackFrame,
    ) -> PartialVMResult<()> {
        // REVIEW: The number of spaces in the indent is currently hard coded.
        for (idx, val) in stack_frame.iter().enumerate() {
            debug_write!(buf, "            [{}] ", idx)?;
            print_value_impl(buf, &val.borrow())?;
            debug_writeln!(buf)?;
        }
        Ok(())
    }

    pub fn print_value<B: Write>(buf: &mut B, val: &Value) -> PartialVMResult<()> {
        print_value_impl(buf, val)
    }
}

/***************************************************************************************
 *
 * Serialization & Deserialization
 *
 *   BCS implementation for VM values. Note although values are represented as Rust
 *   enums that carry type info in the tags, we should NOT rely on them for
 *   serialization:
 *     1) Depending on the specific internal representation, it may be impossible to
 *        reconstruct the layout from a value. For example, one cannot tell if a general
 *        container is a struct or a value.
 *     2) Even if 1) is not a problem at a certain time, we may change to a different
 *        internal representation that breaks the 1-1 mapping. Extremely speaking, if
 *        we switch to untagged unions one day, none of the type info will be carried
 *        by the value.
 *
 *   Therefore the appropriate & robust way to implement serialization & deserialization
 *   is to involve an explicit representation of the type layout.
 *
 **************************************************************************************/

use serde::{
    de::Error as DeError,
    ser::{Error as SerError, SerializeSeq, SerializeTuple},
    Deserialize,
};

impl Value {
    pub fn simple_deserialize(blob: &[u8], layout: &MoveTypeLayout) -> Option<Value> {
        bcs::from_bytes_seed(SeedWrapper { layout }, blob).ok()
    }

    pub fn simple_serialize(&self, layout: &MoveTypeLayout) -> Option<Vec<u8>> {
        Some(bcs::to_bytes(&AnnotatedValue { layout, val: self }).expect("BCS failed"))
    }
}

struct AnnotatedValue<'a, 'b, T1, T2> {
    layout: &'a T1,
    val: &'b T2,
}

fn invariant_violation<S: serde::Serializer>(message: String) -> S::Error {
    S::Error::custom(
        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(message),
    )
}

impl serde::Serialize for AnnotatedValue<'_, '_, MoveTypeLayout, Value> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match (self.layout, self.val) {
            (MoveTypeLayout::U8, Value::U8(x)) => serializer.serialize_u8(*x),
            (MoveTypeLayout::U16, Value::U16(x)) => serializer.serialize_u16(*x),
            (MoveTypeLayout::U32, Value::U32(x)) => serializer.serialize_u32(*x),
            (MoveTypeLayout::U64, Value::U64(x)) => serializer.serialize_u64(*x),
            (MoveTypeLayout::U128, Value::U128(x)) => serializer.serialize_u128(**x),
            (MoveTypeLayout::U256, Value::U256(x)) => x.serialize(serializer),
            (MoveTypeLayout::Bool, Value::Bool(x)) => serializer.serialize_bool(*x),
            (MoveTypeLayout::Address, Value::Address(x)) => x.serialize(serializer),

            (MoveTypeLayout::Struct(struct_layout), Value::Struct(struct_)) => (AnnotatedValue {
                layout: struct_layout.as_ref(),
                val: &struct_.0,
            })
            .serialize(serializer),

            (MoveTypeLayout::Enum(enum_layout), Value::Variant(entry)) => (AnnotatedValue {
                layout: enum_layout.as_ref(),
                val: entry.0.as_ref(),
            })
            .serialize(serializer),

            (MoveTypeLayout::Vector(layout), Value::PrimVec(prim_vec)) => {
                let layout = layout.as_ref();
                match (layout, prim_vec) {
                    (MoveTypeLayout::U8, PrimVec::VecU8(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U16, PrimVec::VecU16(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U32, PrimVec::VecU32(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U64, PrimVec::VecU64(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U128, PrimVec::VecU128(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U256, PrimVec::VecU256(r)) => r.serialize(serializer),
                    (MoveTypeLayout::Bool, PrimVec::VecBool(r)) => r.serialize(serializer),
                    (MoveTypeLayout::Address, PrimVec::VecAddress(r)) => r.serialize(serializer),
                    (layout, container) => Err(invariant_violation::<S>(format!(
                        "cannot serialize container {:?} as {:?}",
                        container, layout
                    ))),
                }
            }
            (MoveTypeLayout::Vector(layout), Value::Vec(r)) => {
                let layout = layout.as_ref();
                let v = r;
                let mut t = serializer.serialize_seq(Some(v.len()))?;
                for val in v.iter() {
                    let val = &*val.borrow();
                    t.serialize_element(&AnnotatedValue { layout, val })?;
                }
                t.end()
            }

            (MoveTypeLayout::Signer, Value::Struct(struct_)) => {
                if struct_.len() != 1 {
                    return Err(invariant_violation::<S>(format!(
                        "cannot serialize container as a signer -- expected 1 field got {}",
                        struct_.len()
                    )));
                }
                (AnnotatedValue {
                    layout: &MoveTypeLayout::Address,
                    val: &*struct_[0].borrow(),
                })
                .serialize(serializer)
            }

            (ty, val) => Err(invariant_violation::<S>(format!(
                "cannot serialize value {:?} as {:?}",
                val, ty
            ))),
        }
    }
}

impl serde::Serialize for AnnotatedValue<'_, '_, MoveStructLayout, FixedSizeVec> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let values = &self.val;
        let fields = self.layout.fields();
        if fields.len() != values.len() {
            return Err(invariant_violation::<S>(format!(
                "cannot serialize struct value {:?} as {:?} -- number of fields mismatch",
                self.val, self.layout
            )));
        }
        let mut t = serializer.serialize_tuple(values.len())?;
        for (field_layout, val) in fields.iter().zip(values.iter()) {
            let val = &*val.borrow();
            t.serialize_element(&AnnotatedValue {
                layout: field_layout,
                val,
            })?;
        }
        t.end()
    }
}

impl serde::Serialize for AnnotatedValue<'_, '_, MoveEnumLayout, (VariantTag, FixedSizeVec)> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (tag, values) = &self.val;
        let tag = if *tag as u64 > VARIANT_COUNT_MAX {
            return Err(serde::ser::Error::custom(format!(
                "Variant tag {} is greater than the maximum allowed value of {}",
                tag, VARIANT_COUNT_MAX
            )));
        } else {
            *tag as u8
        };

        let fields = &self.layout.0[tag as usize];
        if fields.len() != values.len() {
            return Err(invariant_violation::<S>(format!(
                "cannot serialize variant value {:?} as {:?} -- number of fields mismatch",
                self.val, self.layout
            )));
        }

        let mut t = serializer.serialize_tuple(2)?;
        t.serialize_element(&tag)?;

        t.serialize_element(&AnnotatedValue {
            layout: &VariantFields(fields),
            val: values,
        })?;

        t.end()
    }
}

struct VariantFields<'a>(&'a [MoveTypeLayout]);

impl<'a> serde::Serialize for AnnotatedValue<'a, '_, VariantFields<'a>, FixedSizeVec> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let values = self.val;
        let types = self.layout.0;
        if types.len() != values.len() {
            return Err(invariant_violation::<S>(format!(
                "cannot serialize variant value {:?} as {:?} -- number of fields mismatch",
                self.val, self.layout.0
            )));
        }
        let mut t = serializer.serialize_tuple(values.len())?;
        for (field_layout, val) in types.iter().zip(values.iter()) {
            let val = &*val.borrow();
            t.serialize_element(&AnnotatedValue {
                layout: field_layout,
                val,
            })?;
        }
        t.end()
    }
}

#[derive(Clone)]
struct SeedWrapper<L> {
    layout: L,
}

impl<'d> serde::de::DeserializeSeed<'d> for SeedWrapper<&MoveTypeLayout> {
    type Value = Value;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        use MoveTypeLayout as L;
        use PrimVec as PV;
        use Value as V;

        match self.layout {
            L::Bool => bool::deserialize(deserializer).map(Value::bool),
            L::U8 => u8::deserialize(deserializer).map(Value::u8),
            L::U16 => u16::deserialize(deserializer).map(Value::u16),
            L::U32 => u32::deserialize(deserializer).map(Value::u32),
            L::U64 => u64::deserialize(deserializer).map(Value::u64),
            L::U128 => u128::deserialize(deserializer).map(Value::u128),
            L::U256 => u256::U256::deserialize(deserializer).map(Value::u256),
            L::Address => AccountAddress::deserialize(deserializer).map(Value::address),
            L::Signer => AccountAddress::deserialize(deserializer).map(Value::signer),

            L::Struct(struct_layout) => Ok(SeedWrapper {
                layout: struct_layout.as_ref(),
            }
            .deserialize(deserializer)?),

            L::Enum(enum_layout) => Ok(SeedWrapper {
                layout: enum_layout.as_ref(),
            }
            .deserialize(deserializer)?),

            L::Vector(layout) => {
                let value = match layout.as_ref() {
                    L::U8 => V::PrimVec(PV::VecU8(Vec::deserialize(deserializer)?)),
                    L::U16 => V::PrimVec(PV::VecU16(Vec::deserialize(deserializer)?)),
                    L::U32 => V::PrimVec(PV::VecU32(Vec::deserialize(deserializer)?)),
                    L::U64 => V::PrimVec(PV::VecU64(Vec::deserialize(deserializer)?)),
                    L::U128 => V::PrimVec(PV::VecU128(Vec::deserialize(deserializer)?)),
                    L::U256 => V::PrimVec(PV::VecU256(Vec::deserialize(deserializer)?)),
                    L::Bool => V::PrimVec(PV::VecBool(Vec::deserialize(deserializer)?)),
                    L::Address => V::PrimVec(PV::VecAddress(Vec::deserialize(deserializer)?)),
                    layout => {
                        // TODO: Box this as part of deserialization to avoid the second iteration?
                        let v = deserializer
                            .deserialize_seq(VectorElementVisitor(SeedWrapper { layout }))?
                            .into_iter()
                            .map(MemBox::new)
                            .collect();
                        Value::Vec(v)
                    }
                };
                Ok(value)
            }
        }
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for SeedWrapper<&MoveStructLayout> {
    type Value = Value;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        let fields = deserializer
            .deserialize_tuple(self.layout.0.len(), StructFieldVisitor(&self.layout.0))?;
        Ok(Value::make_struct(fields))
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for SeedWrapper<&MoveEnumLayout> {
    type Value = Value;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        let variant = deserializer.deserialize_tuple(2, EnumFieldVisitor(&self.layout.0))?;
        Ok(Value::Variant(variant))
    }
}

struct VectorElementVisitor<'a>(SeedWrapper<&'a MoveTypeLayout>);

impl<'d> serde::de::Visitor<'d> for VectorElementVisitor<'_> {
    type Value = Vec<Value>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Vector")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut vals = Vec::new();
        while let Some(elem) = seq.next_element_seed(self.0.clone())? {
            vals.push(elem)
        }
        Ok(vals)
    }
}

struct StructFieldVisitor<'a>(&'a [MoveTypeLayout]);

impl<'d> serde::de::Visitor<'d> for StructFieldVisitor<'_> {
    type Value = Vec<Value>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let mut val = Vec::new();
        for (i, field_layout) in self.0.iter().enumerate() {
            if let Some(elem) = seq.next_element_seed(SeedWrapper {
                layout: field_layout,
            })? {
                val.push(elem)
            } else {
                return Err(A::Error::invalid_length(i, &self));
            }
        }
        Ok(val)
    }
}

struct EnumFieldVisitor<'a>(&'a Vec<Vec<MoveTypeLayout>>);

impl<'d> serde::de::Visitor<'d> for EnumFieldVisitor<'_> {
    type Value = Variant;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Enum")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        let tag = match seq.next_element_seed(&MoveTypeLayout::U8)? {
            Some(RuntimeValue::U8(tag)) if tag as u64 <= VARIANT_COUNT_MAX => tag as u16,
            Some(RuntimeValue::U8(tag)) => {
                return Err(A::Error::invalid_length(tag as usize, &self))
            }
            Some(val) => {
                return Err(A::Error::invalid_type(
                    serde::de::Unexpected::Other(&format!("{val:?}")),
                    &self,
                ))
            }
            None => return Err(A::Error::invalid_length(0, &self)),
        };

        let Some(variant_layout) = self.0.get(tag as usize) else {
            return Err(A::Error::invalid_length(tag as usize, &self));
        };

        let Some(fields) = seq.next_element_seed(&MoveRuntimeVariantFieldLayout(variant_layout))?
        else {
            return Err(A::Error::invalid_length(1, &self));
        };

        Ok(Variant::pack(tag, fields))
    }
}

struct MoveRuntimeVariantFieldLayout<'a>(&'a Vec<MoveTypeLayout>);

impl<'d> serde::de::DeserializeSeed<'d> for &MoveRuntimeVariantFieldLayout<'_> {
    type Value = Vec<Value>;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_tuple(self.0.len(), StructFieldVisitor(self.0))
    }
}

/***************************************************************************************
*
* Constants
*
*   Implementation of deserialization of constant data into a runtime value
*
**************************************************************************************/

impl Value {
    fn constant_sig_token_to_layout(constant_signature: &SignatureToken) -> Option<MoveTypeLayout> {
        use MoveTypeLayout as L;
        use SignatureToken as S;

        Some(match constant_signature {
            S::Bool => L::Bool,
            S::U8 => L::U8,
            S::U16 => L::U16,
            S::U32 => L::U32,
            S::U64 => L::U64,
            S::U128 => L::U128,
            S::U256 => L::U256,
            S::Address => L::Address,
            S::Signer => return None,
            S::Vector(inner) => L::Vector(Box::new(Self::constant_sig_token_to_layout(inner)?)),
            // Not yet supported
            S::Datatype(_) | S::DatatypeInstantiation(_) => return None,
            // Not allowed/Not meaningful
            S::TypeParameter(_) | S::Reference(_) | S::MutableReference(_) => return None,
        })
    }

    pub fn deserialize_constant(constant: &Constant) -> Option<Value> {
        let layout = Self::constant_sig_token_to_layout(&constant.type_)?;
        Value::simple_deserialize(&constant.data, &layout)
    }
}

// -------------------------------------------------------------------------------------------------
// Views and Visitors
// -------------------------------------------------------------------------------------------------
// Visitors and Views allow for walking and inspecting values, including a depth bound.

impl PrimVec {
    /// Visit the indexed element, using the provided visitor and depth (or 0 is no depth is
    /// provided).
    fn visit_indexed(&self, ndx: usize, visitor: &mut impl ValueVisitor, depth: usize) {
        match self {
            PrimVec::VecU8(xs) => visitor.visit_u8(depth, xs[ndx]),
            PrimVec::VecU16(xs) => {
                visitor.visit_u16(depth, xs[ndx]);
            }
            PrimVec::VecU32(xs) => {
                visitor.visit_u32(depth, xs[ndx]);
            }
            PrimVec::VecU64(xs) => {
                visitor.visit_u64(depth, xs[ndx]);
            }
            PrimVec::VecU128(xs) => {
                visitor.visit_u128(depth, xs[ndx]);
            }
            PrimVec::VecU256(xs) => {
                visitor.visit_u256(depth, xs[ndx]);
            }
            PrimVec::VecBool(xs) => {
                visitor.visit_bool(depth, xs[ndx]);
            }
            PrimVec::VecAddress(xs) => {
                visitor.visit_address(depth, xs[ndx]);
            }
        }
    }
}

impl Reference {
    fn visit_impl(&self, visitor: &mut impl ValueVisitor, depth: usize) {
        if visitor.visit_ref(depth) {
            match self {
                Reference::Value(mem_box) => mem_box.borrow().visit_impl(visitor, depth),
                Reference::Indexed(entry) => {
                    let (vec, ndx) = entry.as_ref();
                    vec.borrow()
                        .prim_vec_ref()
                        .unwrap_or_else(|_| panic!("Indexed ref that is not a prim vec: {:?}", vec))
                        .visit_indexed(*ndx, visitor, depth + 1);
                }
            }
        }
    }
}

impl Value {
    fn visit_impl(&self, visitor: &mut impl ValueVisitor, depth: usize) {
        match self {
            Value::Invalid => unreachable!("Should not be able to visit an invalid value"),
            Value::U8(val) => visitor.visit_u8(depth, *val),
            Value::U16(val) => visitor.visit_u16(depth, *val),
            Value::U32(val) => visitor.visit_u32(depth, *val),
            Value::U64(val) => visitor.visit_u64(depth, *val),
            Value::U128(val) => visitor.visit_u128(depth, *val.as_ref()),
            Value::U256(val) => visitor.visit_u256(depth, *val.as_ref()),
            Value::Bool(val) => visitor.visit_bool(depth, *val),
            Value::Address(val) => visitor.visit_address(depth, **val),
            Value::Reference(r) => r.visit_impl(visitor, depth),
            Value::Vec(items) => {
                if visitor.visit_vec(depth, items.len()) {
                    for item in items {
                        item.borrow().visit_impl(visitor, depth + 1);
                    }
                }
            }
            Value::PrimVec(prim_vec) => match prim_vec {
                PrimVec::VecU8(r) => visitor.visit_vec_u8(depth, r),
                PrimVec::VecU16(r) => visitor.visit_vec_u16(depth, r),
                PrimVec::VecU32(r) => visitor.visit_vec_u32(depth, r),
                PrimVec::VecU64(r) => visitor.visit_vec_u64(depth, r),
                PrimVec::VecU128(r) => visitor.visit_vec_u128(depth, r),
                PrimVec::VecU256(r) => visitor.visit_vec_u256(depth, r),
                PrimVec::VecBool(r) => visitor.visit_vec_bool(depth, r),
                PrimVec::VecAddress(r) => visitor.visit_vec_address(depth, r),
            },
            Value::Struct(struct_) => {
                if visitor.visit_struct(depth, struct_.len()) {
                    for item in struct_.iter() {
                        item.borrow().visit_impl(visitor, depth + 1);
                    }
                }
            }
            Value::Variant(entry) => {
                let (_tag, fields) = entry.as_ref();
                if visitor.visit_struct(depth, fields.len()) {
                    for item in fields.iter() {
                        item.borrow().visit_impl(visitor, depth + 1);
                    }
                }
            }
        }
    }
}

impl ValueView for Value {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        self.visit_impl(visitor, 0)
    }
}

impl ValueView for MemBox<Value> {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        self.0.borrow().visit_impl(visitor, 0)
    }
}

impl ValueView for Struct {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        if visitor.visit_struct(0, self.0.len()) {
            for val in self.0.iter() {
                val.borrow().visit_impl(visitor, 1);
            }
        }
    }
}

impl ValueView for Vector {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        self.0.visit_impl(visitor, 0)
    }
}

impl ValueView for IntegerValue {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        use IntegerValue::*;

        match self {
            U8(val) => visitor.visit_u8(0, *val),
            U16(val) => visitor.visit_u16(0, *val),
            U32(val) => visitor.visit_u32(0, *val),
            U64(val) => visitor.visit_u64(0, *val),
            U128(val) => visitor.visit_u128(0, *val),
            U256(val) => visitor.visit_u256(0, *val),
        }
    }
}

impl ValueView for Reference {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        self.visit_impl(visitor, 0)
    }
}

macro_rules! impl_container_ref_views {
    ($($type_name:ty),+) => {
        $(
            impl ValueView for $type_name {
                fn visit(&self, visitor: &mut impl ValueVisitor) {
                    self.0.borrow().visit_impl(visitor, 0)
                }
            }
        )+
    };
}

impl_container_ref_views!(VectorRef, StructRef, SignerRef, VariantRef);

impl Struct {
    #[allow(clippy::needless_lifetimes)]
    pub fn field_views<'a>(&'a self) -> impl ExactSizeIterator<Item = impl ValueView + 'a> {
        self.0.iter()
    }
}

impl Variant {
    #[allow(clippy::needless_lifetimes)]
    pub fn field_views<'a>(&'a self) -> impl ExactSizeIterator<Item = impl ValueView + 'a> {
        self.0.as_ref().1.iter()
    }
}

impl Vector {
    pub fn elem_len(&self) -> usize {
        self.0
            .vector_ref()
            .unwrap_or_else(|_| panic!("Expected a vector, got {:?}", self))
            .len()
    }

    #[allow(clippy::needless_lifetimes)]
    pub fn elem_views<'a>(&'a self) -> impl ExactSizeIterator<Item = impl ValueView + 'a> {
        struct ElemView<'b> {
            container: &'b Value,
            ndx: usize,
        }

        impl ValueView for ElemView<'_> {
            fn visit(&self, visitor: &mut impl ValueVisitor) {
                match &self.container {
                    Value::Vec(v) => {
                        v[self.ndx].borrow().visit(visitor);
                    }
                    Value::PrimVec(v) => {
                        v.visit_indexed(self.ndx, visitor, 0);
                    }
                    _ => unreachable!(),
                }
            }
        }

        let container = self
            .0
            .vector_ref()
            .unwrap_or_else(|_| panic!("Expected a vector, got {:?}", self));
        let len = container.len();
        (0..len).map(move |ndx| ElemView {
            container: &self.0,
            ndx,
        })
    }
}

impl Reference {
    #[allow(clippy::needless_lifetimes)]
    pub fn value_view<'a>(&'a self) -> impl ValueView + 'a {
        struct ValueBehindRef<'b>(&'b Reference);

        /// Returns a `value` behind a reference; visiting it visits the underlying vaouel.
        impl<'b> ValueView for ValueBehindRef<'b> {
            fn visit(&self, visitor: &mut impl ValueVisitor) {
                match self.0 {
                    Reference::Value(mem_box) => mem_box.borrow().visit_impl(visitor, 0),
                    Reference::Indexed(entry) => {
                        let (vec, ndx) = entry.as_ref();
                        let Value::PrimVec(prim_vec) = &*vec.borrow() else {
                            panic!("Expected prim vec for indexed reference, got {:?}", vec);
                        };
                        prim_vec.visit_indexed(*ndx, visitor, 0);
                    }
                }
            }
        }

        ValueBehindRef(self)
    }
}

impl GlobalValue {
    #[allow(clippy::needless_lifetimes)]
    pub fn view<'a>(&'a self) -> Option<impl ValueView + 'a> {
        use GlobalValueImpl as G;

        struct Wrapper<'b>(&'b MemBox<Value>);

        impl<'b> ValueView for Wrapper<'b> {
            fn visit(&self, visitor: &mut impl ValueVisitor) {
                let Value::Struct(struct_) = &*self.0.borrow() else {
                    unreachable!()
                };
                if visitor.visit_struct(0, struct_.len()) {
                    for val in struct_.iter() {
                        val.borrow().visit_impl(visitor, 1);
                    }
                }
            }
        }

        match &self.0 {
            G::None | G::Deleted => None,
            G::Cached { container, .. } | G::Fresh { container } => Some(Wrapper(container)),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Prop Testing
// -------------------------------------------------------------------------------------------------
// Random generation of values that fit into a given layout.

#[cfg(feature = "fuzzing")]
pub mod prop {
    use super::*;
    use proptest::{collection::vec, prelude::*};

    pub fn value_strategy_with_layout(layout: &MoveTypeLayout) -> impl Strategy<Value = Value> {
        use MoveTypeLayout as L;

        match layout {
            L::U8 => any::<u8>().prop_map(Value::u8).boxed(),
            L::U16 => any::<u16>().prop_map(Value::u16).boxed(),
            L::U32 => any::<u32>().prop_map(Value::u32).boxed(),
            L::U64 => any::<u64>().prop_map(Value::u64).boxed(),
            L::U128 => any::<u128>().prop_map(Value::u128).boxed(),
            L::U256 => any::<u256::U256>().prop_map(Value::u256).boxed(),
            L::Bool => any::<bool>().prop_map(Value::bool).boxed(),
            L::Address => any::<AccountAddress>().prop_map(Value::address).boxed(),
            L::Signer => any::<AccountAddress>().prop_map(Value::signer).boxed(),

            L::Vector(layout) => match &**layout {
                L::U8 => vec(any::<u8>(), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::VecU8(Rc::new(RefCell::new(
                            vals,
                        )))))
                    })
                    .boxed(),
                L::U16 => vec(any::<u16>(), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::VecU16(Rc::new(RefCell::new(
                            vals,
                        )))))
                    })
                    .boxed(),
                L::U32 => vec(any::<u32>(), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::VecU32(Rc::new(RefCell::new(
                            vals,
                        )))))
                    })
                    .boxed(),
                L::U64 => vec(any::<u64>(), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::VecU64(Rc::new(RefCell::new(
                            vals,
                        )))))
                    })
                    .boxed(),
                L::U128 => vec(any::<u128>(), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::VecU128(Rc::new(RefCell::new(
                            vals,
                        )))))
                    })
                    .boxed(),
                L::U256 => vec(any::<u256::U256>(), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::VecU256(Rc::new(RefCell::new(
                            vals,
                        )))))
                    })
                    .boxed(),
                L::Bool => vec(any::<bool>(), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::VecBool(Rc::new(RefCell::new(
                            vals,
                        )))))
                    })
                    .boxed(),
                L::Address => vec(any::<AccountAddress>(), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::VecAddress(Rc::new(
                            RefCell::new(vals),
                        ))))
                    })
                    .boxed(),
                layout => vec(value_strategy_with_layout(layout), 0..10)
                    .prop_map(|vals| {
                        Value(Value::Container(Container::Vec(Rc::new(RefCell::new(
                            vals.into_iter().map(|val| val.0).collect(),
                        )))))
                    })
                    .boxed(),
            },

            L::Struct(struct_layout) => struct_layout
                .fields()
                .iter()
                .map(value_strategy_with_layout)
                .collect::<Vec<_>>()
                .prop_map(move |vals| Value::struct_(Struct::pack(vals)))
                .boxed(),
        }
    }

    pub fn layout_strategy() -> impl Strategy<Value = MoveTypeLayout> {
        use MoveTypeLayout as L;

        let leaf = prop_oneof![
            1 => Just(L::U8),
            1 => Just(L::U16),
            1 => Just(L::U32),
            1 => Just(L::U64),
            1 => Just(L::U128),
            1 => Just(L::U256),
            1 => Just(L::Bool),
            1 => Just(L::Address),
            1 => Just(L::Signer),
        ];

        leaf.prop_recursive(8, 32, 2, |inner| {
            prop_oneof![
                1 => inner.clone().prop_map(|layout| L::Vector(Box::new(layout))),
                1 => vec(inner, 0..1).prop_map(|f_layouts| {
                     L::Struct(MoveStructLayout::new(f_layouts))}),
            ]
        })
    }

    pub fn layout_and_value_strategy() -> impl Strategy<Value = (MoveTypeLayout, Value)> {
        layout_strategy().no_shrink().prop_flat_map(|layout| {
            let value_strategy = value_strategy_with_layout(&layout);
            (Just(layout), value_strategy)
        })
    }
}

use move_core_types::runtime_value::{
    MoveStruct as RuntimeStruct, MoveValue as RuntimeValue, MoveVariant as RuntimeVariant,
};

impl Value {
    pub fn as_move_value(&self, layout: &MoveTypeLayout) -> RuntimeValue {
        use MoveTypeLayout as L;
        use PrimVec as PV;

        match (layout, self) {
            (L::U8, Value::U8(x)) => RuntimeValue::U8(*x),
            (L::U16, Value::U16(x)) => RuntimeValue::U16(*x),
            (L::U32, Value::U32(x)) => RuntimeValue::U32(*x),
            (L::U64, Value::U64(x)) => RuntimeValue::U64(*x),
            (L::U128, Value::U128(x)) => RuntimeValue::U128(**x),
            (L::U256, Value::U256(x)) => RuntimeValue::U256(**x),
            (L::Bool, Value::Bool(x)) => RuntimeValue::Bool(*x),
            (L::Address, Value::Address(x)) => RuntimeValue::Address(**x),

            // Enum variant case with dereferencing the Box.
            (L::Enum(enum_layout), Value::Variant(entry)) => {
                let MoveEnumLayout(variants) = &**enum_layout;
                let (tag, values) = entry.as_ref();
                let tag = *tag; // Simply copy the u16 value, no need for dereferencing
                let field_layouts = &variants[tag as usize];
                let mut fields = vec![];
                for (v, field_layout) in values.iter().zip(field_layouts) {
                    fields.push(v.borrow().as_move_value(field_layout));
                }
                RuntimeValue::Variant(RuntimeVariant { tag, fields })
            }

            // Struct case with direct access to Box
            (L::Struct(struct_layout), Value::Struct(values)) => {
                let mut fields = vec![];
                for (v, field_layout) in values.iter().zip(struct_layout.fields().iter()) {
                    fields.push(v.borrow().as_move_value(field_layout));
                }
                RuntimeValue::Struct(RuntimeStruct::new(fields))
            }

            // Vector case with handling different container types
            (L::Vector(inner_layout), Value::Vec(values)) => RuntimeValue::Vector(
                values
                    .iter()
                    .map(|v| v.borrow().as_move_value(inner_layout.as_ref()))
                    .collect(),
            ),
            (L::Vector(inner_layout), Value::PrimVec(values)) => {
                use RuntimeValue as MV;
                macro_rules! make_vec {
                    ($xs:expr, $ctor:ident) => {
                        MV::Vector($xs.iter().map(|x| MV::$ctor(*x)).collect())
                    };
                }
                match (inner_layout.as_ref(), values) {
                    (L::U8, PV::VecU8(xs)) => make_vec!(xs, U8),
                    (L::U16, PV::VecU16(xs)) => make_vec!(xs, U16),
                    (L::U32, PV::VecU32(xs)) => make_vec!(xs, U32),
                    (L::U64, PV::VecU64(xs)) => make_vec!(xs, U64),
                    (L::U128, PV::VecU128(xs)) => make_vec!(xs, U128),
                    (L::U256, PV::VecU256(xs)) => make_vec!(xs, U256),
                    (L::Bool, PV::VecBool(xs)) => make_vec!(xs, Bool),
                    (L::Address, PV::VecAddress(xs)) => make_vec!(xs, Address),
                    (
                        ty @ (L::Bool
                        | L::U8
                        | L::U64
                        | L::U128
                        | L::Address
                        | L::U16
                        | L::U32
                        | L::U256),
                        vec,
                    ) => {
                        panic!("Mismatched type {:?} for primitive vector {:?}", ty, vec);
                    }
                    (L::Signer | L::Vector(_) | L::Struct(_) | L::Enum(_), _) => {
                        panic!(
                            "Expected a primitive type for the primitive vector, got {:?}",
                            inner_layout.as_ref()
                        );
                    }
                }
            }

            // Signer case: just dereferencing the box and checking for address
            (L::Signer, Value::Struct(values)) => {
                if values.len() != 1 {
                    panic!("Unexpected signer layout: {:?}", values);
                }
                match &*values[0].borrow() {
                    Value::Address(a) => RuntimeValue::Signer(**a),
                    v => panic!("Unexpected non-address while converting signer: {:?}", v),
                }
            }

            (layout, val) => panic!("Cannot convert value {:?} as {:?}", val, layout),
        }
    }
}

use move_core_types::annotated_value::{
    MoveEnumLayout as AnnEnumLayout, MoveStruct as AnnStruct, MoveTypeLayout as AnnTypeLayout,
    MoveValue as AnnValue, MoveVariant as AnnVariant,
};

impl Value {
    /// Converts the value to an annotated move value. This is only needed for tracing and care
    /// should be taken when using this function as it can possibly inflate the size of the value.
    pub(crate) fn as_annotated_move_value(&self, layout: &AnnTypeLayout) -> Option<AnnValue> {
        use AnnTypeLayout as L;
        use AnnValue as AV;
        match (layout, self) {
            (L::U8, Value::U8(x)) => Some(AnnValue::U8(*x)),
            (L::U16, Value::U16(x)) => Some(AnnValue::U16(*x)),
            (L::U32, Value::U32(x)) => Some(AnnValue::U32(*x)),
            (L::U64, Value::U64(x)) => Some(AnnValue::U64(*x)),
            (L::U128, Value::U128(x)) => Some(AnnValue::U128(**x)),
            (L::U256, Value::U256(x)) => Some(AnnValue::U256(**x)),
            (L::Bool, Value::Bool(x)) => Some(AnnValue::Bool(*x)),
            (L::Address, Value::Address(x)) => Some(AnnValue::Address(**x)),
            (L::Enum(e_layout), Value::Variant(var_box)) => {
                let AnnEnumLayout { type_, variants } = e_layout.as_ref();
                let (tag, values) = var_box.as_ref();
                let tag = *tag;
                let ((name, _), field_layouts) = variants.iter().find(|((_, t), _)| *t == tag)?;
                let mut fields = vec![];
                for (v, field_layout) in values.iter().zip(field_layouts) {
                    fields.push((
                        field_layout.name.clone(),
                        v.borrow().as_annotated_move_value(&field_layout.layout)?,
                    ));
                }
                Some(AV::Variant(AnnVariant {
                    tag,
                    fields,
                    type_: type_.clone(),
                    variant_name: name.clone(),
                }))
            }
            (L::Struct(struct_layout), Value::Struct(values)) => {
                let mut fields = vec![];
                for (v, field_layout) in values.iter().zip(struct_layout.fields.iter()) {
                    fields.push((
                        field_layout.name.clone(),
                        v.borrow().as_annotated_move_value(&field_layout.layout)?,
                    ));
                }
                Some(AV::Struct(AnnStruct::new(
                    struct_layout.type_.clone(),
                    fields,
                )))
            }
            (L::Vector(inner_layout), Value::Vec(vec)) => {
                let result: Option<Vec<_>> = vec
                    .iter()
                    .map(|mb| mb.borrow().as_annotated_move_value(inner_layout))
                    .collect();
                Some(AV::Vector(result?))
            }
            (L::Vector(inner_layout), Value::PrimVec(values)) => {
                macro_rules! make_vec {
                    ($xs:expr, $ctor:ident) => {
                        Some(AV::Vector($xs.iter().map(|x| AV::$ctor(*x)).collect()))
                    };
                }
                match (inner_layout.as_ref(), values) {
                    (L::U8, PrimVec::VecU8(xs)) => make_vec!(xs, U8),
                    (L::U16, PrimVec::VecU16(xs)) => make_vec!(xs, U16),
                    (L::U32, PrimVec::VecU32(xs)) => make_vec!(xs, U32),
                    (L::U64, PrimVec::VecU64(xs)) => make_vec!(xs, U64),
                    (L::U128, PrimVec::VecU128(xs)) => make_vec!(xs, U128),
                    (L::U256, PrimVec::VecU256(xs)) => make_vec!(xs, U256),
                    (L::Bool, PrimVec::VecBool(xs)) => make_vec!(xs, Bool),
                    (L::Address, PrimVec::VecAddress(xs)) => make_vec!(xs, Address),
                    (_, _) => None,
                }
            }
            (L::Signer, Value::Struct(values)) => {
                if values.len() != 1 {
                    return None;
                }
                match &*values[0].borrow() {
                    Value::Address(a) => Some(AV::Signer(**a)),
                    _ => None,
                }
            }
            (layout, Value::Reference(ref_)) => ref_.as_annotated_move_value(layout),
            (_, _) => None,
        }
    }
}

impl Reference {
    pub fn as_annotated_move_value(&self, layout: &AnnTypeLayout) -> Option<AnnValue> {
        use move_core_types::annotated_value::MoveTypeLayout as L;
        use AnnValue as AV;
        match self {
            // If the reference is a direct reference, delegate to the inner Value.
            Reference::Value(mem_box) => mem_box.borrow().as_annotated_move_value(layout),
            // If it is an indexed reference, we need to extract the element.
            Reference::Indexed(entry) => {
                let (container, ndx) = &**entry;
                match &*container.borrow() {
                    // For a vector container, expect a Value::Vec.
                    Value::Vec(vec) => {
                        // Get the element at the index and recursively convert.
                        let field = vec.get(*ndx)?;
                        field.borrow().as_annotated_move_value(layout)
                    }
                    // For a primitive vector container, expect Value::PrimVec.
                    Value::PrimVec(prim_vec) => {
                        // We require that the layout is for a vector; then we inspect
                        // the inner layout and the PrimVec variant.
                        match layout {
                            L::Vector(inner_layout) => match (inner_layout.as_ref(), prim_vec) {
                                (L::U8, PrimVec::VecU8(xs)) => {
                                    Some(AV::Vector(xs.iter().map(|u| AV::U8(*u)).collect()))
                                }
                                (L::U16, PrimVec::VecU16(xs)) => {
                                    Some(AV::Vector(xs.iter().map(|u| AV::U16(*u)).collect()))
                                }
                                (L::U32, PrimVec::VecU32(xs)) => {
                                    Some(AV::Vector(xs.iter().map(|u| AV::U32(*u)).collect()))
                                }
                                (L::U64, PrimVec::VecU64(xs)) => {
                                    Some(AV::Vector(xs.iter().map(|u| AV::U64(*u)).collect()))
                                }
                                (L::U128, PrimVec::VecU128(xs)) => {
                                    Some(AV::Vector(xs.iter().map(|u| AV::U128(*u)).collect()))
                                }
                                (L::U256, PrimVec::VecU256(xs)) => {
                                    Some(AV::Vector(xs.iter().map(|u| AV::U256(*u)).collect()))
                                }
                                (L::Bool, PrimVec::VecBool(xs)) => {
                                    Some(AV::Vector(xs.iter().map(|b| AV::Bool(*b)).collect()))
                                }
                                (L::Address, PrimVec::VecAddress(xs)) => {
                                    Some(AV::Vector(xs.iter().map(|a| AV::Address(*a)).collect()))
                                }
                                (ty, vec) => {
                                    panic!(
                                        "Mismatched type {:?} for primitive vector {:?}",
                                        ty, vec
                                    )
                                }
                            },
                            _ => None,
                        }
                    }
                    // Otherwise, the container is not a supported vector-like type.
                    _ => None,
                }
            }
        }
    }
}
