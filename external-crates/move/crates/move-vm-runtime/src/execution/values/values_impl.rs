// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::arena::{Arena, ArenaVec},
    jit::execution::ast::Type,
    shared::{
        views::{ValueView, ValueVisitor},
        vm_pointer::VMPointer,
    },
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
    fmt::{self, Debug, Display},
    ops::{Add, Index, IndexMut},
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
pub enum Value {
    Invalid,
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(Box<u128>),
    U256(Box<u256::U256>),
    Bool(bool),
    Address(Box<AccountAddress>),
    Container(Box<Container>),
    Reference(Box<Reference>),
}

#[derive(Debug)]
pub enum Container {
    // NB: The boxes here give stable (heap) addresses to the values inside.
    Vec(Vec<Box<Value>>),
    Struct(FixedSizeVec),
    // TODO: PinVec in the limit
    VecU8(Vec<u8>),
    VecU16(Vec<u16>),
    VecU32(Vec<u32>),
    VecU64(Vec<u64>),
    VecU128(Vec<u128>),
    VecU256(Vec<u256::U256>),
    VecBool(Vec<bool>),
    VecAddress(Vec<AccountAddress>),
    Variant(Box<(VariantTag, FixedSizeVec)>),
}

/// Runtime representation of a Move value.
#[derive(Debug)]
pub enum Reference {
    U8(VMPointer<u8>),
    U16(VMPointer<u16>),
    U32(VMPointer<u32>),
    U64(VMPointer<u64>),
    U128(VMPointer<u128>),
    U256(VMPointer<u256::U256>),
    Bool(VMPointer<bool>),
    Address(VMPointer<AccountAddress>),
    Container(VMPointer<Container>),
}

#[derive(Debug)]
pub struct FixedSizeVec(Box<[Value]>);

// XXX/TODO(vm-rewrite): Remove this and replace with proper value dirtying.
#[deprecated(
    note = "This is a temporary shim for the new VM. It _MUST_ be removed before final rollout."
)]
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

/// A Move struct for creating containers.
#[derive(Debug)]
pub struct Struct {
    fields: Vec<Value>,
}

// A vector. This is an alias for a Container for now but we may change
// it once Containers are restructured.
// It's used from vector native functions to get a vector and operate on that.
// There is an impl for Vector which implements the API private to this module.
#[derive(Debug)]
pub struct Vector(Container);

/// A reference to a Move struct that allows you to take a reference to one of its fields.
#[derive(Debug)]
pub struct StructRef(VMPointer<Container>);

// A reference to a signer. Clients can attempt a cast to this struct if they are
// expecting a Signer on the stack or as an argument.
#[derive(Debug)]
pub struct SignerRef(VMPointer<Container>);

// A reference to a vector. This is an alias for a ContainerRef for now but we may change
// it once Containers are restructured.
// It's used from vector native functions to get a reference to a vector and operate on that.
// There is an impl for VectorRef which implements the API private to this module.
#[derive(Debug)]
pub struct VectorRef(VMPointer<Container>);

/// A special "slot" in global storage that can hold a resource. It also keeps track of the status
/// of the resource relative to the global state, which is necessary to compute the effects to emit
/// at the end of transaction execution.
#[derive(Debug)]
enum GlobalValueImpl {
    /// No resource resides in this slot or in storage.
    None,
    /// A resource has been published to this slot and it did not previously exist in storage.
    Fresh { container: Box<Container> },
    /// A resource resides in this slot and also in storage. The status flag indicates whether
    /// it has potentially been altered.
    Cached {
        fingerprint: GlobalFingerprint,
        container: Box<Container>,
    },
    /// A resource used to exist in storage but has been deleted by the current transaction.
    Deleted,
}

/// A wrapper around `GlobalValueImpl`, representing a "slot" in global storage that can
/// hold a resource.
#[derive(Debug)]
pub struct GlobalValue(GlobalValueImpl);

/// A Move enum value (aka a variant).
#[derive(Debug)]
pub struct Variant {
    tag: VariantTag,
    fields: Vec<Value>,
}

#[derive(Debug)]
pub struct VariantRef(VMPointer<Container>);

/// Constant representation of a Move value.
#[derive(Debug)]
pub enum ConstantValue {
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
pub enum ConstantContainer {
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
// Helper Functions
// -------------------------------------------------------------------------------------------------
// Types visible from outside the module. They are almost exclusively wrappers around the internal

impl Value {
    pub fn invalid() -> Value {
        Value::Invalid
    }
}

impl Container {
    fn len(&self) -> usize {
        match self {
            Self::Vec(r) => r.len(),
            Self::Struct(r) => r.len(),
            Self::VecU8(r) => r.len(),
            Self::VecU16(r) => r.len(),
            Self::VecU32(r) => r.len(),
            Self::VecU64(r) => r.len(),
            Self::VecU128(r) => r.len(),
            Self::VecU256(r) => r.len(),
            Self::VecBool(r) => r.len(),
            Self::VecAddress(r) => r.len(),
            Self::Variant(r) => r.as_ref().1.len(),
        }
    }

    // Create a Container for a Signer of the provided account address.
    fn signer(x: AccountAddress) -> Self {
        Container::Struct(FixedSizeVec(Box::new([Value::Address(Box::new(x))])))
    }
}

impl FixedSizeVec {
    /// Returns the length of the fixed-size vector.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Creates a `FixedSizeVec` from a `Vec<ValueImpl>`.
    pub fn from_vec(input: Vec<Value>) -> Self {
        FixedSizeVec(input.into_boxed_slice())
    }

    /// Returns an iterator over the `FixedSizeVec`.
    pub fn iter(&self) -> std::slice::Iter<'_, Value> {
        self.0.iter()
    }

    pub fn as_slice(&self) -> &[Value] {
        &self.0
    }
}

impl std::iter::IntoIterator for FixedSizeVec {
    type Item = Value;
    type IntoIter = std::vec::IntoIter<Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_vec().into_iter()
    }
}

// Implement the `Index` trait to allow immutable indexing.
impl Index<usize> for FixedSizeVec {
    type Output = Value;

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
    pub fn fingerprint(container: &Container) -> Self {
        // XXX/TODO(vm-rewrite): Implement proper fingerprinting.
        Self(Some(format!("{:?}", container)))
    }

    pub fn dirty() -> Self {
        Self(None)
    }

    pub fn same_value(&self, other: &Container) -> bool {
        self.0 == Self::fingerprint(other).0
    }
}

macro_rules! match_reference_impls {
    (
        $self_:ident; $other:ident;
        container $ref_1:ident, $ref_2:ident => $container_expr:expr;
        global $g_ref_1:ident, $g_ref_2:ident => $global_expr:expr;
        prim $prim_ref_1:ident, $prim_ref_2:ident => $prim_expr:expr;
    ) => {
        match ($self_, $other) {
            (Reference::Container($ref_1), Reference::Container($ref_2)) => $container_expr,
            (Reference::U8($prim_ref_1), Reference::U8($prim_ref_2)) => $prim_expr,
            (Reference::U16($prim_ref_1), Reference::U16($prim_ref_2)) => $prim_expr,
            (Reference::U32($prim_ref_1), Reference::U32($prim_ref_2)) => $prim_expr,
            (Reference::U64($prim_ref_1), Reference::U64($prim_ref_2)) => $prim_expr,
            (Reference::U128($prim_ref_1), Reference::U128($prim_ref_2)) => $prim_expr,
            (Reference::U256($prim_ref_1), Reference::U256($prim_ref_2)) => $prim_expr,
            (Reference::Bool($prim_ref_1), Reference::Bool($prim_ref_2)) => $prim_expr,
            (Reference::Address($prim_ref_1), Reference::Address($prim_ref_2)) => $prim_expr,
            _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message("Type mismatch during reference comparison".to_string())),
        }
    };
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
            Self::Container(c) => Self::Container(Box::new(c.as_ref().copy_value())),
            Self::Reference(ref_) => Self::Reference(Box::new(ref_.copy_value())),
        }
    }
}

impl Container {
    fn copy_value(&self) -> Self {
        match self {
            // Deep copy of a vector of `ValueImpl`
            Self::Vec(values) => {
                let copied_values: Vec<Box<Value>> =
                    values.iter().map(|v| Box::new(v.copy_value())).collect();
                Self::Vec(copied_values)
            }

            // Deep copy of `FixedSizeVec`
            Self::Struct(fixed_size_vec) => {
                let copied_values: Vec<Value> =
                    fixed_size_vec.iter().map(|v| v.copy_value()).collect();
                Self::Struct(FixedSizeVec::from_vec(copied_values))
            }

            // Deep copy of a `Variant`
            Container::Variant(variant) => {
                let (variant_tag, fixed_size_vec) = &**variant;
                let copied_values: Vec<Value> =
                    fixed_size_vec.iter().map(|v| v.copy_value()).collect();
                Container::Variant(Box::new((
                    *variant_tag,
                    FixedSizeVec::from_vec(copied_values),
                )))
            }

            // TODO: auto-gen this?
            Self::VecU8(r) => Self::VecU8(r.clone()),
            Self::VecU16(r) => Self::VecU16(r.clone()),
            Self::VecU32(r) => Self::VecU32(r.clone()),
            Self::VecU64(r) => Self::VecU64(r.clone()),
            Self::VecU128(r) => Self::VecU128(r.clone()),
            Self::VecU256(r) => Self::VecU256(r.clone()),
            Self::VecBool(r) => Self::VecBool(r.clone()),
            Self::VecAddress(r) => Self::VecAddress(r.clone()),
        }
    }
}

impl Reference {
    pub fn copy_value(&self) -> Self {
        // TODO: auto-gen this?
        match self {
            Reference::U8(ref_) => Reference::U8(ref_.ptr_clone()),
            Reference::U16(ref_) => Reference::U16(ref_.ptr_clone()),
            Reference::U32(ref_) => Reference::U32(ref_.ptr_clone()),
            Reference::U64(ref_) => Reference::U64(ref_.ptr_clone()),
            Reference::U128(ref_) => Reference::U128(ref_.ptr_clone()),
            Reference::U256(ref_) => Reference::U256(ref_.ptr_clone()),
            Reference::Bool(ref_) => Reference::Bool(ref_.ptr_clone()),
            Reference::Address(ref_) => Reference::Address(ref_.ptr_clone()),
            Reference::Container(ref_) => Reference::Container(ref_.ptr_clone()),
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
    pub fn to_constant_value(self, arena: &Arena) -> PartialVMResult<ConstantValue> {
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
            Value::Container(container) => container.to_constant_value(arena),
        }
    }
}

impl Container {
    /// Allocates the constant in the provided arena
    pub fn to_constant_value(self, arena: &Arena) -> PartialVMResult<ConstantValue> {
        macro_rules! alloc_vec {
            ($values:expr) => {
                arena.alloc_vec($values.into_iter())?
            };
        }

        let constant_container = match self {
            Container::Vec(values) => {
                let constants = values
                    .into_iter()
                    .map(|v| v.to_constant_value(arena))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let constants = arena.alloc_vec(constants.into_iter())?;
                ConstantContainer::Vec(constants)
            }
            Container::Struct(values) => {
                let constants = values
                    .into_iter()
                    .map(|v| v.to_constant_value(arena))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let constants = arena.alloc_vec(constants.into_iter())?;
                ConstantContainer::Struct(constants)
            }
            // TODO: auto-gen this?
            Container::VecU8(values) => ConstantContainer::VecU8(alloc_vec!(values)),
            Container::VecU64(values) => ConstantContainer::VecU64(alloc_vec!(values)),
            Container::VecU128(values) => ConstantContainer::VecU128(alloc_vec!(values)),
            Container::VecBool(values) => ConstantContainer::VecBool(alloc_vec!(values)),
            Container::VecAddress(values) => ConstantContainer::VecAddress(alloc_vec!(values)),
            Container::VecU16(values) => ConstantContainer::VecU16(alloc_vec!(values)),
            Container::VecU32(values) => ConstantContainer::VecU32(alloc_vec!(values)),
            Container::VecU256(values) => ConstantContainer::VecU256(alloc_vec!(values)),
            Container::Variant(variant) => {
                let (tag, values) = *variant;
                let constants = values
                    .into_iter()
                    .map(|v| v.to_constant_value(arena))
                    .collect::<PartialVMResult<Vec<_>>>()?;
                let constants = arena.alloc_vec(constants.into_iter())?;
                ConstantContainer::Variant(tag, constants)
            }
        };
        Ok(ConstantValue::Container(constant_container))
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
            ConstantValue::Container(container) => {
                Value::Container(Box::new(ConstantContainer::to_container(container)))
            }
        }
    }
}

impl ConstantContainer {
    /// Performs a deep copy of the constant value
    pub fn to_container(&self) -> Container {
        macro_rules! clone_vec {
            ($vec:expr) => {
                $vec.iter().map(|v| *v).collect()
            };
        }

        match self {
            ConstantContainer::Vec(values) => {
                let container_values = values
                    .iter()
                    .map(ConstantValue::to_value)
                    .map(Box::new)
                    .collect::<Vec<_>>();
                Container::Vec(container_values)
            }
            ConstantContainer::Struct(values) => {
                let container_values = values
                    .iter()
                    .map(ConstantValue::to_value)
                    .collect::<Vec<_>>();
                let struct_ = FixedSizeVec::from_vec(container_values);
                Container::Struct(struct_)
            }
            // TODO: auto-gen this?
            ConstantContainer::VecU8(values) => Container::VecU8(clone_vec!(values)),
            ConstantContainer::VecU64(values) => Container::VecU64(clone_vec!(values)),
            ConstantContainer::VecU128(values) => Container::VecU128(clone_vec!(values)),
            ConstantContainer::VecBool(values) => Container::VecBool(clone_vec!(values)),
            ConstantContainer::VecAddress(values) => Container::VecAddress(clone_vec!(values)),
            ConstantContainer::VecU16(values) => Container::VecU16(clone_vec!(values)),
            ConstantContainer::VecU32(values) => Container::VecU32(clone_vec!(values)),
            ConstantContainer::VecU256(values) => Container::VecU256(clone_vec!(values)),
            ConstantContainer::Variant(tag, values) => {
                let container_values = values
                    .iter()
                    .map(ConstantValue::to_value)
                    .collect::<Vec<_>>();
                let variant_ = FixedSizeVec::from_vec(container_values);
                Container::Variant(Box::new((*tag, variant_)))
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
            (Self::Container(v1), Self::Container(v2)) => v1.equals(v2),
            (Self::Reference(v1), Self::Reference(v2)) => v1.equals(v2),
            (Self::U8(v1), Self::U8(v2)) => Ok(v1 == v2),
            (Self::U16(v1), Self::U16(v2)) => Ok(v1 == v2),
            (Self::U32(v1), Self::U32(v2)) => Ok(v1 == v2),
            (Self::U64(v1), Self::U64(v2)) => Ok(v1 == v2),
            (Self::U128(v1), Self::U128(v2)) => Ok(v1 == v2),
            (Self::U256(v1), Self::U256(v2)) => Ok(v1 == v2),
            (Self::Bool(v1), Self::Bool(v2)) => Ok(v1 == v2),
            (Self::Address(v1), Self::Address(v2)) => Ok(v1 == v2),
            _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot compare values: {:?}, {:?}", self, other))),
        }
    }
}

impl Container {
    pub fn equals(&self, other: &Container) -> PartialVMResult<bool> {
        // TODO: auto-gen this?
        match (self, other) {
            (Self::Vec(v1), Self::Vec(v2)) => {
                for (a, b) in v1.iter().zip(v2) {
                    if !a.equals(b)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Self::Variant(v1), Self::Variant(v2)) => Ok(v1.0 == v2.0 && v1.1.equals(&v2.1)?),
            (Self::Struct(s1), Self::Struct(s2)) => s1.equals(s2),
            (Self::VecU8(v1), Self::VecU8(v2)) => Ok(v1 == v2),
            (Self::VecU16(v1), Self::VecU16(v2)) => Ok(v1 == v2),
            (Self::VecU32(v1), Self::VecU32(v2)) => Ok(v1 == v2),
            (Self::VecU64(v1), Self::VecU64(v2)) => Ok(v1 == v2),
            (Self::VecU128(v1), Self::VecU128(v2)) => Ok(v1 == v2),
            (Self::VecU256(v1), Self::VecU256(v2)) => Ok(v1 == v2),
            (Self::VecBool(v1), Self::VecBool(v2)) => Ok(v1 == v2),
            (Self::VecAddress(v1), Self::VecAddress(v2)) => Ok(v1 == v2),
            _ => Err(
                PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR).with_message(format!(
                    "cannot compare container values: {:?}, {:?}",
                    self, other
                )),
            ),
        }
    }
}

impl Reference {
    pub fn equals(&self, other: &Reference) -> PartialVMResult<bool> {
        // TODO: auto-gen this?
        match_reference_impls!(self; other;
            container ref_1, ref_2 => {
                Ok(VMPointer::ptr_eq(ref_1, ref_2) || ref_1.to_ref().equals(ref_2.to_ref())?)
            };
            global g_ref, ctor => {
                Reference::Container(g_ref.value.ptr_clone()).equals(ctor)
            };
            prim prim_ref_1, prim_ref_2 => {
                Ok(VMPointer::ptr_eq(prim_ref_1, prim_ref_2) || prim_ref_1.to_ref() == prim_ref_2.to_ref())
            };
        )
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
            if !a.equals(b)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

// -------------------------------------------------------------------------------------------------
// Read Ref
// -------------------------------------------------------------------------------------------------
// Implementation for the Move operation `read_ref`

impl Reference {
    pub fn read_ref(self) -> PartialVMResult<Value> {
        let value = match self {
            Reference::U8(ref_) => Value::U8(*ref_.to_ref()),
            Reference::U16(ref_) => Value::U16(*ref_.to_ref()),
            Reference::U32(ref_) => Value::U32(*ref_.to_ref()),
            Reference::U64(ref_) => Value::U64(*ref_.to_ref()),
            Reference::U128(ref_) => Value::U128(Box::new(*ref_.to_ref())),
            Reference::U256(ref_) => Value::U256(Box::new(*ref_.to_ref())),
            Reference::Bool(ref_) => Value::Bool(*ref_.to_ref()),
            Reference::Address(ref_) => Value::Address(Box::new(*ref_.to_ref())),
            Reference::Container(ref_) => Value::Container(Box::new(ref_.to_ref().copy_value())),
        };
        Ok(value)
    }
}

impl StructRef {
    pub fn read_ref(self) -> PartialVMResult<Value> {
        Ok(Value::Container(Box::new(self.0.to_ref().copy_value())))
    }
}

// -------------------------------------------------------------------------------------------------
// Write Ref
// -------------------------------------------------------------------------------------------------
// Implementation for the Move operation `write_ref`

impl Reference {
    pub fn write_ref(self, value: Value) -> PartialVMResult<()> {
        // TODO: auto-gen this?
        match (self, value) {
            (Reference::U8(ref_), Value::U8(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), new_value);
            }
            (Reference::U16(ref_), Value::U16(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), new_value);
            }
            (Reference::U32(ref_), Value::U32(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), new_value);
            }
            (Reference::U64(ref_), Value::U64(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), new_value);
            }
            (Reference::U128(ref_), Value::U128(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), *new_value);
            }
            (Reference::U256(ref_), Value::U256(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), *new_value);
            }
            (Reference::Bool(ref_), Value::Bool(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), new_value);
            }
            (Reference::Address(ref_), Value::Address(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), *new_value);
            }
            (Reference::Container(ref_), Value::Container(new_value)) => {
                let _ = std::mem::replace(ref_.to_mut_ref(), *new_value);
            }
            _ => {
                return Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message("Type mismatch during reference update".to_string()))
            }
        };
        Ok(())
    }
}

// -------------------------------------------------------------------------------------------------
// Take Ref
// -------------------------------------------------------------------------------------------------
// Implementation of borrowing in Move: convert a value to a reference, borrow field, and
// an element from a vector.

impl Value {
    fn to_ref(&self) -> PartialVMResult<Reference> {
        // TODO: auto-gen part of this?
        match self {
            // Primitive types are converted to corresponding primitive references.
            Value::U8(val) => Ok(Reference::U8(VMPointer::from_ref(val))),
            Value::U16(val) => Ok(Reference::U16(VMPointer::from_ref(val))),
            Value::U32(val) => Ok(Reference::U32(VMPointer::from_ref(val))),
            Value::U64(val) => Ok(Reference::U64(VMPointer::from_ref(val))),
            Value::U128(val) => Ok(Reference::U128(VMPointer::from_ref(val))),
            Value::U256(val) => Ok(Reference::U256(VMPointer::from_ref(val))),
            Value::Bool(val) => Ok(Reference::Bool(VMPointer::from_ref(val))),
            Value::Address(val) => Ok(Reference::Address(VMPointer::from_ref(val))),

            // Containers are converted to `ContainerReference`.
            Value::Container(val) => Ok(Reference::Container(VMPointer::from_ref(val))),

            // If the value is already a reference, return it directly.
            Value::Reference(_) => Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message("Cannot create a reference to a reference value".to_string())),

            // Return an error if the value is invalid.
            Value::Invalid => Err(PartialVMError::new(
                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
            )
            .with_message("Cannot create a reference to an invalid value".to_string())),
        }
    }

    /// Converts a reference to a `ValueImpl` into a `ReferenceImpl`.
    /// This function inspects the value and constructs the corresponding `ReferenceImpl`.
    pub fn take_ref(&self) -> PartialVMResult<Value> {
        self.to_ref().map(Box::new).map(Value::Reference)
    }
}

// -------------------------------------------------------------------------------------------------
// Borrowing
// -------------------------------------------------------------------------------------------------
// Implementation of borrowing in Move: convert a value to a reference, borrow field, and
// an element from a vector.

impl StructRef {
    /// Borrows a field from the struct by index. Returns a reference to the field
    /// wrapped in `ValueImpl`, or an error if the index is out of bounds or the
    /// container is not a struct.
    pub fn borrow_field(&self, index: usize) -> PartialVMResult<Value> {
        // Dereference the VMPointer to access the container.
        let container: &Container = self.0.to_ref();

        // Ensure the container is a struct and return the field at the specified index.
        match container {
            Container::Struct(container) => container[index].take_ref(),

            // If the container is not a struct, return an error.
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Container is not a struct".to_string()),
            ),
        }
    }
}

impl VariantRef {
    pub fn get_tag(&self) -> PartialVMResult<VariantTag> {
        // Dereference the VMPointer to access the container.
        let container: &Container = self.0.to_ref();

        // Ensure the container is a variant and return the tag.
        match container {
            Container::Variant(r) => Ok(r.0),
            _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("expected variant container, got {:?}", self.0))),
        }
    }

    pub fn check_tag(&self, expected_tag: VariantTag) -> PartialVMResult<()> {
        let tag = self.get_tag()?;
        if tag != expected_tag {
            Err(
                PartialVMError::new(StatusCode::VARIANT_TAG_MISMATCH).with_message(format!(
                    "Variant tag mismatch: expected {}, got {}",
                    expected_tag, tag
                )),
            )
        } else {
            Ok(())
        }
    }

    /// Unpacks a variant into a set of references
    pub fn unpack_variant(&self) -> PartialVMResult<Vec<Value>> {
        // Dereference the VMPointer to access the container.
        let container: &Container = self.0.to_ref();

        match container {
            Container::Variant(r) => {
                let values = &r.1;
                let mut res = vec![];
                for v in values.iter() {
                    let ref_ = match v {
                        value @ (Value::Container(_)
                        | Value::U8(_)
                        | Value::U16(_)
                        | Value::U32(_)
                        | Value::U64(_)
                        | Value::U128(_)
                        | Value::U256(_)
                        | Value::Bool(_)
                        | Value::Address(_)) => value.take_ref()?,
                        x @ (Value::Reference(_) | Value::Invalid) => {
                            return Err(PartialVMError::new(
                                StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
                            )
                            .with_message(format!(
                            "cannot unpack a reference value {:?} held inside a variant ref {:?}",
                            x, self
                        )))
                        }
                    };
                    res.push(ref_);
                }
                Ok(res)
            }

            _ => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("expected variant container, got {:?}", self.0))),
        }
    }
}

impl VectorRef {
    /// Borrows an element from the container, returning it as a reference wrapped in `ValueImpl::Reference`.
    /// The result is a `PartialVmResult<ValueImpl>` containing the element as a `Reference`.
    pub fn borrow_elem(&self, index: usize, type_param: &Type) -> PartialVMResult<Value> {
        let container = self.0.to_ref();
        check_elem_layout(type_param, container)?;

        macro_rules! prim_vec_case {
            ($vec:ident, $ty:ty, $ctor:ident) => {{
                if index >= $vec.len() {
                    return Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                        .with_sub_status(INDEX_OUT_OF_BOUNDS)
                        .with_message("Vector index out of bounds".to_string()));
                }
                let elem_ref: VMPointer<$ty> = VMPointer::from_ref(&$vec[index]);
                Ok(Value::Reference(Box::new(Reference::$ctor(elem_ref))))
            }};
        }

        match container {
            // For a vector of `ValueImpl`, borrow the element at `index`.
            Container::Vec(values) => {
                if index >= values.len() {
                    return Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                        .with_sub_status(INDEX_OUT_OF_BOUNDS)
                        .with_message("Vector container index out of bounds".to_string()));
                }
                let elem = &values[index];
                elem.as_ref().take_ref()
            }

            // For primitive-typed vectors, borrow the element at `index`.
            Container::VecU8(vec) => prim_vec_case!(vec, u8, U8),
            Container::VecU16(vec) => prim_vec_case!(vec, u16, U16),
            Container::VecU32(vec) => prim_vec_case!(vec, u32, U32),
            Container::VecU64(vec) => prim_vec_case!(vec, u64, U64),
            Container::VecU128(vec) => prim_vec_case!(vec, u128, U128),
            Container::VecU256(vec) => prim_vec_case!(vec, u256::U256, U256),
            Container::VecBool(vec) => prim_vec_case!(vec, bool, Bool),
            Container::VecAddress(vec) => prim_vec_case!(vec, AccountAddress, Address),

            // Return an error for unsupported container types.
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Unsupported container type for borrowing".to_string()),
            ),
        }
    }
}

impl SignerRef {
    pub fn borrow_signer(&self) -> PartialVMResult<Value> {
        match self.0.to_ref() {
            Container::Struct(values) => values[0].take_ref(),
            _ => Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("Unsupported container type for borrowing".to_string()),
            ),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Public Value Constructios
// -------------------------------------------------------------------------------------------------
// Constructors to allow the creation of values outside of this module.

macro_rules! make_value_prim_vec {
    ($input:ident, $ty:ident) => {{
        let vec = $input.into_iter().collect();
        Value::Container(Box::new(Container::$ty(vec)))
    }};
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

    pub fn signer(x: AccountAddress) -> Self {
        Value::Container(Box::new(Container::signer(x)))
    }

    pub fn struct_(s: Struct) -> Self {
        Value::Container(Box::new(Container::Struct(FixedSizeVec::from_vec(
            s.fields,
        ))))
    }

    pub fn variant(s: Variant) -> Self {
        let tag = s.tag;
        let fields = FixedSizeVec::from_vec(s.fields);
        Value::Container(Box::new(Container::Variant(Box::new((tag, fields)))))
    }

    // TODO: consider whether we want to replace these with fn vector(v: Vec<Value>).
    pub fn vector_u8(it: impl IntoIterator<Item = u8>) -> Self {
        make_value_prim_vec!(it, VecU8)
    }

    pub fn vector_u16(it: impl IntoIterator<Item = u16>) -> Self {
        make_value_prim_vec!(it, VecU16)
    }

    pub fn vector_u32(it: impl IntoIterator<Item = u32>) -> Self {
        make_value_prim_vec!(it, VecU32)
    }

    pub fn vector_u64(it: impl IntoIterator<Item = u64>) -> Self {
        make_value_prim_vec!(it, VecU64)
    }

    pub fn vector_u128(it: impl IntoIterator<Item = u128>) -> Self {
        make_value_prim_vec!(it, VecU128)
    }

    pub fn vector_u256(it: impl IntoIterator<Item = u256::U256>) -> Self {
        make_value_prim_vec!(it, VecU256)
    }

    pub fn vector_bool(it: impl IntoIterator<Item = bool>) -> Self {
        make_value_prim_vec!(it, VecBool)
    }

    pub fn vector_address(it: impl IntoIterator<Item = AccountAddress>) -> Self {
        make_value_prim_vec!(it, VecAddress)
    }

    // REVIEW: This API can break
    pub fn vector_for_testing_only(it: impl IntoIterator<Item = Value>) -> Self {
        Value::Container(Box::new(Container::Vec(
            it.into_iter().map(Box::new).collect(),
        )))
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

macro_rules! impl_prim_vm_value_cast {
    // Special case for boxed types.
    (Box<$ty:ty>, $tc:ident) => {
        impl VMValueCast<$ty> for Value {
            fn cast(self) -> PartialVMResult<$ty> {
                match self {
                    Value::$tc(x) => Ok(*x), // Dereference the boxed value
                    v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!("cannot cast {:?} to {}", v, stringify!($ty)))),
                }
            }
        }
    };
    // Case for types that are directly stored (not boxed).
    ($ty:ty, $tc:ident) => {
        impl VMValueCast<$ty> for Value {
            fn cast(self) -> PartialVMResult<$ty> {
                match self {
                    Value::$tc(x) => Ok(x),
                    v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message(format!("cannot cast {:?} to {}", v, stringify!($ty)))),
                }
            }
        }
    };
}

// Implementations for unboxed types.
impl_prim_vm_value_cast!(u8, U8);
impl_prim_vm_value_cast!(u16, U16);
impl_prim_vm_value_cast!(u32, U32);
impl_prim_vm_value_cast!(u64, U64);
impl_prim_vm_value_cast!(bool, Bool);

// Implementations for boxed types.
impl_prim_vm_value_cast!(Box<u128>, U128);
impl_prim_vm_value_cast!(Box<u256::U256>, U256);
impl_prim_vm_value_cast!(Box<AccountAddress>, Address);

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

impl VMValueCast<Reference> for Value {
    fn cast(mut self) -> PartialVMResult<Reference> {
        let value = std::mem::replace(&mut self, Value::Invalid);
        match value {
            Value::Reference(r) => Ok(*r),
            v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot cast {:?} to reference", v))),
        }
    }
}

impl VMValueCast<Container> for Value {
    fn cast(mut self) -> PartialVMResult<Container> {
        let value = std::mem::replace(&mut self, Value::Invalid);
        match value {
            Value::Container(container) => Ok(*container),
            v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot cast {:?} to container", v))),
        }
    }
}

impl VMValueCast<Struct> for Value {
    fn cast(mut self) -> PartialVMResult<Struct> {
        // This cose used to take unique ownership. To ensure we do something similar, we replace
        // the current one with `Invalid`.
        let value = std::mem::replace(&mut self, Value::Invalid);
        match value {
            Value::Container(container) => match *container {
                Container::Struct(fields) => Ok(Struct {
                    fields: fields.into_iter().collect(),
                }),
                v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("cannot cast {:?} to struct", v,))),
            },
            v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot cast {:?} to struct", v,))),
        }
    }
}

impl VMValueCast<Variant> for Value {
    fn cast(mut self) -> PartialVMResult<Variant> {
        let value = std::mem::replace(&mut self, Value::Invalid);
        match value {
            Value::Container(container) => match *container {
                Container::Variant(entry) => {
                    let (tag, fields) = *entry;
                    let fields = fields.into_iter().collect();
                    Ok(Variant { tag, fields })
                }
                v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                    .with_message(format!("cannot cast {:?} to enum variant", v))),
            },
            v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot cast {:?} to enum variant", v))),
        }
    }
}

macro_rules! make_value_cast_ref_error {
    ($msg:expr, $value:expr) => {
        Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
            .with_message(format!("cannot cast {:?} to {}", $value, $msg)))
    };
}

impl VMValueCast<StructRef> for Value {
    fn cast(self) -> PartialVMResult<StructRef> {
        let Value::Reference(ref_) = self else {
            return make_value_cast_ref_error!("StructRef", self);
        };
        let Reference::Container(container) = *ref_ else {
            return make_value_cast_ref_error!("StructRef", *ref_);
        };
        if matches!(container.to_ref(), Container::Struct(_)) {
            Ok(StructRef(container.ptr_clone()))
        } else {
            make_value_cast_ref_error!("StructRef", container.to_ref())
        }
    }
}

impl VMValueCast<VariantRef> for Value {
    fn cast(self) -> PartialVMResult<VariantRef> {
        let Value::Reference(ref_) = self else {
            return make_value_cast_ref_error!("VariantRef", self);
        };
        let Reference::Container(container) = *ref_ else {
            return make_value_cast_ref_error!("VariantRef", *ref_);
        };
        if matches!(container.to_ref(), Container::Variant(_)) {
            Ok(VariantRef(container.ptr_clone()))
        } else {
            make_value_cast_ref_error!("VariantRef", container.to_ref())
        }
    }
}

macro_rules! impl_vec_vm_value_cast {
    ($vec_type:ty, $container_variant:ident, $error_msg:expr) => {
        impl VMValueCast<Vec<$vec_type>> for Value {
            fn cast(mut self) -> PartialVMResult<Vec<$vec_type>> {
                let value = std::mem::replace(&mut self, Value::Invalid);
                match value {
                    Value::Container(c) => match *c {
                        Container::$container_variant(container) => {
                            Ok(container.into_iter().collect::<Vec<_>>())
                        }
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

impl VMValueCast<Vec<Value>> for Value {
    fn cast(mut self) -> PartialVMResult<Vec<Value>> {
        // Take ownership of the value by replacing it with `Invalid`
        let value = std::mem::replace(&mut self, Value::Invalid);

        // Match the container and handle `Vec<Box<ValueImpl>>`
        match value {
            Value::Container(container) => {
                if let Container::Vec(vec) = *container {
                    // Convert each `Box<ValueImpl>` into `Value`
                    let values = vec.into_iter().map(|boxed_impl| *boxed_impl).collect();
                    Ok(values)
                } else {
                    // Return error if the container is not a vector
                    Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                        .with_message("Expected vector container".to_string()))
                }
            }
            v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot cast {:?} to Vec<Value>", v))),
        }
    }
}

impl VMValueCast<SignerRef> for Value {
    fn cast(self) -> PartialVMResult<SignerRef> {
        let Value::Reference(ref_) = self else {
            return make_value_cast_ref_error!("SignerRef", self);
        };
        let Reference::Container(container) = *ref_ else {
            return make_value_cast_ref_error!("SignerRef", *ref_);
        };
        if matches!(container.to_ref(), Container::Struct(_)) {
            Ok(SignerRef(container.ptr_clone()))
        } else {
            make_value_cast_ref_error!("SignerRef", container.to_ref())
        }
    }
}

impl VMValueCast<VectorRef> for Value {
    fn cast(self) -> PartialVMResult<VectorRef> {
        let Value::Reference(ref_) = self else {
            return make_value_cast_ref_error!("VectorRef", self);
        };
        let Reference::Container(container) = *ref_ else {
            return make_value_cast_ref_error!("VectorRef", *ref_);
        };
        if matches!(
            container.to_ref(),
            Container::Vec(_)
                | Container::VecU8(_)
                | Container::VecU16(_)
                | Container::VecU32(_)
                | Container::VecU64(_)
                | Container::VecU128(_)
                | Container::VecU256(_)
                | Container::VecBool(_)
                | Container::VecAddress(_)
        ) {
            Ok(VectorRef(container.ptr_clone()))
        } else {
            make_value_cast_ref_error!("VectorRef", container.to_ref())
        }
    }
}

impl VMValueCast<Vector> for Value {
    fn cast(self) -> PartialVMResult<Vector> {
        match self {
            Value::Container(c) => Ok(Vector(*c)),
            v => Err(PartialVMError::new(StatusCode::INTERNAL_TYPE_ERROR)
                .with_message(format!("cannot cast {:?} to vector", v,))),
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
// Vector
// -------------------------------------------------------------------------------------------------
// Implemented as a built-in data type.

pub const INDEX_OUT_OF_BOUNDS: u64 = NFE_VECTOR_ERROR_BASE + 1;
pub const POP_EMPTY_VEC: u64 = NFE_VECTOR_ERROR_BASE + 2;
pub const VEC_UNPACK_PARITY_MISMATCH: u64 = NFE_VECTOR_ERROR_BASE + 3;
pub const VEC_SIZE_LIMIT_REACHED: u64 = NFE_VECTOR_ERROR_BASE + 4;

fn check_elem_layout(ty: &Type, v: &Container) -> PartialVMResult<()> {
    match (ty, v) {
        (Type::U8, Container::VecU8(_))
        | (Type::U64, Container::VecU64(_))
        | (Type::U16, Container::VecU16(_))
        | (Type::U32, Container::VecU32(_))
        | (Type::U128, Container::VecU128(_))
        | (Type::U256, Container::VecU256(_))
        | (Type::Bool, Container::VecBool(_))
        | (Type::Address, Container::VecAddress(_))
        | (Type::Signer, Container::Struct(_)) => Ok(()),

        (Type::Vector(_), Container::Vec(_)) => Ok(()),

        (Type::Datatype(_), Container::Vec(_))
        | (Type::Signer, Container::Vec(_))
        | (Type::DatatypeInstantiation(_), Container::Vec(_)) => Ok(()),

        (Type::Reference(_), _) | (Type::MutableReference(_), _) | (Type::TyParam(_), _) => Err(
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message(format!("invalid type param for vector: {:?}", ty)),
        ),

        (Type::U8, _)
        | (Type::U64, _)
        | (Type::U16, _)
        | (Type::U32, _)
        | (Type::U128, _)
        | (Type::U256, _)
        | (Type::Bool, _)
        | (Type::Address, _)
        | (Type::Signer, _)
        | (Type::Vector(_), _)
        | (Type::Datatype(_), _)
        | (Type::DatatypeInstantiation(_), _) => Err(PartialVMError::new(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        )
        .with_message(format!(
            "vector elem layout mismatch, expected {:?}, got {:?}",
            ty, v
        ))),
    }
}

macro_rules! match_vec_ref_container {
    (
        ($c:ident)
        prim $r:ident => $prim_expr:expr;
        vec $r_vec:ident => $vec_expr:expr;
    ) => {
        match $c {
            Container::VecU8($r) => $prim_expr,
            Container::VecU16($r) => $prim_expr,
            Container::VecU32($r) => $prim_expr,
            Container::VecU64($r) => $prim_expr,
            Container::VecU128($r) => $prim_expr,
            Container::VecU256($r) => $prim_expr,
            Container::VecBool($r) => $prim_expr,
            Container::VecAddress($r) => $prim_expr,
            Container::Vec($r_vec) => $vec_expr,
            Container::Struct(_) | Container::Variant { .. } => {
                unreachable!()
            }
        }
    };
}

impl VectorRef {
    pub fn len(&self, type_param: &Type) -> PartialVMResult<Value> {
        let c = self.0.to_ref();
        check_elem_layout(type_param, c)?;

        assert!(!matches!(c, Container::Struct(_) | Container::Variant(_)));
        let size = c.len();
        Ok(Value::u64(size as u64))
    }

    pub fn push_back(&self, e: Value, type_param: &Type, capacity: u64) -> PartialVMResult<()> {
        let c = self.0.to_mut_ref();
        check_elem_layout(type_param, c)?;

        assert!(!matches!(c, Container::Struct(_) | Container::Variant(_)));
        let size = c.len();

        if size >= (capacity as usize) {
            return Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                .with_sub_status(VEC_SIZE_LIMIT_REACHED)
                .with_message(format!("vector size limit is {capacity}",)));
        }

        match_vec_ref_container!(
            (c)
            prim r => r.push(VMValueCast::cast(e)?);
            vec r => r.push(Box::new(e));
        );
        Ok(())
    }

    /// Returns a RefCell reference to the underlying vector of a `&vector<u8>` value.
    pub fn as_bytes_ref(&self) -> &Vec<u8> {
        let c = self.0.to_ref();
        match c {
            Container::VecU8(r) => r,
            _ => panic!("can only be called on vector<u8>"),
        }
    }

    pub fn pop(&self, type_param: &Type) -> PartialVMResult<Value> {
        let c = self.0.to_mut_ref();
        check_elem_layout(type_param, c)?;

        macro_rules! err_pop_empty_vec {
            () => {
                return Err(PartialVMError::new(StatusCode::VECTOR_OPERATION_ERROR)
                    .with_sub_status(POP_EMPTY_VEC))
            };
        }

        let res = match c {
            Container::VecU8(r) => match r.pop() {
                Some(x) => Value::u8(x),
                None => err_pop_empty_vec!(),
            },
            Container::VecU16(r) => match r.pop() {
                Some(x) => Value::u16(x),
                None => err_pop_empty_vec!(),
            },
            Container::VecU32(r) => match r.pop() {
                Some(x) => Value::u32(x),
                None => err_pop_empty_vec!(),
            },
            Container::VecU64(r) => match r.pop() {
                Some(x) => Value::u64(x),
                None => err_pop_empty_vec!(),
            },
            Container::VecU128(r) => match r.pop() {
                Some(x) => Value::u128(x),
                None => err_pop_empty_vec!(),
            },
            Container::VecU256(r) => match r.pop() {
                Some(x) => Value::u256(x),
                None => err_pop_empty_vec!(),
            },
            Container::VecBool(r) => match r.pop() {
                Some(x) => Value::bool(x),
                None => err_pop_empty_vec!(),
            },
            Container::VecAddress(r) => match r.pop() {
                Some(x) => Value::address(x),
                None => err_pop_empty_vec!(),
            },
            Container::Vec(r) => match r.pop() {
                Some(x) => *x,
                None => err_pop_empty_vec!(),
            },
            Container::Struct(_) | Container::Variant { .. } => unreachable!(),
        };
        Ok(res)
    }

    pub fn swap(&self, idx1: usize, idx2: usize, type_param: &Type) -> PartialVMResult<()> {
        let c = self.0.to_mut_ref();
        check_elem_layout(type_param, c)?;

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

        match c {
            Container::VecU8(r) => swap!(r),
            Container::VecU16(r) => swap!(r),
            Container::VecU32(r) => swap!(r),
            Container::VecU64(r) => swap!(r),
            Container::VecU128(r) => swap!(r),
            Container::VecU256(r) => swap!(r),
            Container::VecBool(r) => swap!(r),
            Container::VecAddress(r) => swap!(r),
            Container::Vec(r) => swap!(r),
            Container::Struct(_) | Container::Variant { .. } => {
                unreachable!()
            }
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
                Value::Container(Box::new(Container::Vec(
                    elements.into_iter().map(Box::new).collect(),
                )))
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
        let elements: Vec<Value> = match self.0 {
            Container::VecU8(r) => take_and_map!(r, Value::U8),
            Container::VecU16(r) => take_and_map!(r, Value::U16),
            Container::VecU32(r) => take_and_map!(r, Value::U32),
            Container::VecU64(r) => take_and_map!(r, Value::U64),
            Container::VecU128(r) => take_and_map!(r, |n| Value::U128(Box::new(n))),
            Container::VecU256(r) => take_and_map!(r, |n| Value::U256(Box::new(n))),
            Container::VecBool(r) => take_and_map!(r, Value::Bool),
            Container::VecAddress(r) => take_and_map!(r, |a| Value::Address(Box::new(a))),
            Container::Vec(r) => take_and_map!(r, |v| *v),
            Container::Struct(_) | Container::Variant { .. } => unreachable!(),
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

    pub fn to_vec_u8(self) -> PartialVMResult<Vec<u8>> {
        check_elem_layout(&Type::U8, &self.0)?;
        if let Container::VecU8(r) = self.0 {
            Ok(r.into_iter().collect())
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

impl Container {
    #[allow(dead_code)]
    pub(crate) fn legacy_size(&self) -> AbstractMemorySize {
        match self {
            Self::Vec(r) => Vector::legacy_size_impl(r.as_ref()),
            Self::Struct(r) => Struct::legacy_size_impl(r.as_slice()),
            Self::Variant(r) => Variant::legacy_size_impl(r.1.as_slice()),
            Self::VecU8(r) => AbstractMemorySize::new((r.len() * std::mem::size_of::<u8>()) as u64),
            Self::VecU16(r) => {
                AbstractMemorySize::new((r.len() * std::mem::size_of::<u16>()) as u64)
            }
            Self::VecU32(r) => {
                AbstractMemorySize::new((r.len() * std::mem::size_of::<u32>()) as u64)
            }
            Self::VecU64(r) => {
                AbstractMemorySize::new((r.len() * std::mem::size_of::<u64>()) as u64)
            }
            Self::VecU128(r) => {
                AbstractMemorySize::new((r.len() * std::mem::size_of::<u128>()) as u64)
            }
            Self::VecU256(r) => {
                AbstractMemorySize::new((r.len() * std::mem::size_of::<u256::U256>()) as u64)
            }
            Self::VecBool(r) => {
                AbstractMemorySize::new((r.len() * std::mem::size_of::<bool>()) as u64)
            }
            Self::VecAddress(r) => {
                AbstractMemorySize::new((r.len() * std::mem::size_of::<AccountAddress>()) as u64)
            }
        }
    }
}

impl Value {
    #[deprecated(note = "Update this to not use the legacy size")]
    pub fn legacy_size(&self) -> AbstractMemorySize {
        use Value::*;

        match self {
            Invalid | U8(_) | U16(_) | U32(_) | U64(_) | U128(_) | U256(_) | Bool(_) => {
                LEGACY_CONST_SIZE
            }
            Address(_) => AbstractMemorySize::new(AccountAddress::LENGTH as u64),
            // TODO: in case the borrow fails the VM will panic.
            Container(c) => c.legacy_size(),
            Reference(_ref) => LEGACY_REFERENCE_SIZE,
        }
    }
}

impl Variant {
    #[allow(dead_code)]
    const TAG_SIZE: AbstractMemorySize = AbstractMemorySize::new(std::mem::size_of::<u16>() as u64);

    #[allow(dead_code)]
    fn legacy_size_impl(fields: &[Value]) -> AbstractMemorySize {
        fields
            .iter()
            .fold(LEGACY_STRUCT_SIZE.add(Self::TAG_SIZE), |acc, v| {
                acc + v.legacy_size()
            })
    }
}

impl Vector {
    #[allow(dead_code)]
    fn legacy_size_impl(fields: &[Box<Value>]) -> AbstractMemorySize {
        fields
            .iter()
            .fold(LEGACY_STRUCT_SIZE, |acc, v| acc + v.legacy_size())
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn legacy_size(&self) -> AbstractMemorySize {
        self.0.legacy_size()
    }
}

impl Struct {
    #[allow(dead_code)]
    fn legacy_size_impl(fields: &[Value]) -> AbstractMemorySize {
        fields
            .iter()
            .fold(LEGACY_STRUCT_SIZE, |acc, v| acc + v.legacy_size())
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn legacy_size(&self) -> AbstractMemorySize {
        Self::legacy_size_impl(&self.fields)
    }
}

#[cfg(test)]
impl Reference {
    #[allow(dead_code)]
    pub(crate) fn legacy_size(&self) -> AbstractMemorySize {
        LEGACY_REFERENCE_SIZE
    }
}

// -------------------------------------------------------------------------------------------------
// Struct Operations
// -------------------------------------------------------------------------------------------------
// Public API for Structs.

impl Struct {
    pub fn pack<I: IntoIterator<Item = Value>>(vals: I) -> Self {
        Self {
            fields: vals.into_iter().collect(),
        }
    }

    pub fn unpack(self) -> PartialVMResult<impl Iterator<Item = Value>> {
        Ok(self.fields.into_iter())
    }
}

// -------------------------------------------------------------------------------------------------
// Variant Operations
// -------------------------------------------------------------------------------------------------
// Public API for Enums.

impl Variant {
    pub fn pack<I: IntoIterator<Item = Value>>(tag: VariantTag, vals: I) -> Self {
        Self {
            tag,
            fields: vals.into_iter().collect(),
        }
    }

    pub fn unpack(self) -> PartialVMResult<impl Iterator<Item = Value>> {
        Ok(self.fields.into_iter())
    }

    pub fn check_tag(&self, tag: VariantTag) -> PartialVMResult<()> {
        if tag != self.tag {
            Err(PartialVMError::new(StatusCode::VARIANT_TAG_MISMATCH)
                .with_message(format!("tag mismatch: expected {}, got {}", tag, self.tag)))
        } else {
            Ok(())
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Global Value Operations
// -------------------------------------------------------------------------------------------------
// Public APIs for GlobalValue. They allow global values to be created from external source (a.k.a.
// storage), and references to be taken from them. At the end of the transaction execution the
// dirty ones can be identified and wrote back to storage.

// -------------------------------------------------------------------------------------------------
// FIXME FIXME FIXME
// Ask Tim for HELP
// FIXME FIXME FIXME
// -------------------------------------------------------------------------------------------------

#[allow(clippy::unnecessary_wraps)]
impl GlobalValueImpl {
    fn cached(
        val: Value,
        existing_fingerprint: Option<GlobalFingerprint>,
    ) -> Result<Self, (PartialVMError, Value)> {
        match val {
            Value::Container(container) if matches!(*container, Container::Struct(_)) => {
                let fingerprint = existing_fingerprint
                    .unwrap_or_else(|| GlobalFingerprint::fingerprint(&container));
                Ok(Self::Cached {
                    container,
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
            Value::Container(container) if matches!(*container, Container::Struct(_)) => {
                Ok(Self::Fresh { container })
            }
            val => Err((
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("failed to publish fresh: not a resource".to_string()),
                val,
            )),
        }
    }

    fn move_from(&mut self) -> PartialVMResult<Value> {
        let value = std::mem::replace(self, Self::None);
        let container = match value {
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
        Ok(Value::Container(container))
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
                let container_ref = VMPointer::from_ref(container.as_ref());
                Ok(Value::Reference(Box::new(Reference::Container(
                    container_ref,
                ))))
            }
            GlobalValueImpl::Cached { container, .. } => Ok(Value::Reference(Box::new(
                Reference::Container(VMPointer::from_ref(container.as_ref())),
            ))),
        }
    }

    fn into_effect(self) -> Option<Op<Value>> {
        match self {
            Self::None => None,
            Self::Deleted => Some(Op::Delete),
            Self::Fresh { container } => {
                let struct_ @ Container::Struct(_) = *container else {
                    unreachable!()
                };
                Some(Op::New(Value::Container(Box::new(struct_))))
            }
            Self::Cached {
                container,
                fingerprint,
            } => {
                if fingerprint.same_value(&container) {
                    None
                } else {
                    let struct_ @ Container::Struct(_) = *container else {
                        unreachable!()
                    };
                    Some(Op::New(Value::Container(Box::new(struct_))))
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
            } => !fingerprint.same_value(container),
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
}

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------
// VM Value Displays for easier reading

impl Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Invalid => write!(f, "Invalid"),

            Self::U8(x) => write!(f, "U8({})", x),
            Self::U16(x) => write!(f, "U16({})", x),
            Self::U32(x) => write!(f, "U32({})", x),
            Self::U64(x) => write!(f, "U64({})", x),
            Self::U128(x) => write!(f, "U128({})", x),
            Self::U256(x) => write!(f, "U256({})", x),
            Self::Bool(x) => write!(f, "{}", x),
            Self::Address(addr) => write!(f, "Address({})", addr.short_str_lossless()),

            Self::Container(r) => write!(f, "Container({})", r),
            Self::Reference(r) => write!(f, "(&){}", r),
        }
    }
}

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

impl Container {
    fn raw_address(&self) -> usize {
        use Container::*;

        match self {
            Vec(r) => r.as_ptr() as usize,
            Struct(r) => r.0.as_ptr() as usize,
            VecU8(r) => r.as_ptr() as usize,
            VecU16(r) => r.as_ptr() as usize,
            VecU32(r) => r.as_ptr() as usize,
            VecU64(r) => r.as_ptr() as usize,
            VecU128(r) => r.as_ptr() as usize,
            VecU256(r) => r.as_ptr() as usize,
            VecBool(r) => r.as_ptr() as usize,
            VecAddress(r) => r.as_ptr() as usize,
            Variant(r) => {
                let (_tag, fields) = &**r;
                fields.0.as_ptr() as usize
            }
        }
    }
}

impl Display for Container {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(container {:x}: ", self.raw_address())?;

        match self {
            Self::Vec(r) => display_list_of_items(r.iter(), f),
            Self::Struct(r) => display_list_of_items(r.iter(), f),
            Self::Variant(r) => {
                let (tag, values) = &**r;
                write!(f, "|tag: {}|", tag)?;
                display_list_of_items(values.iter(), f)
            }
            Self::VecU8(r) => display_list_of_items(r.iter(), f),
            Self::VecU16(r) => display_list_of_items(r.iter(), f),
            Self::VecU32(r) => display_list_of_items(r.iter(), f),
            Self::VecU64(r) => display_list_of_items(r.iter(), f),
            Self::VecU128(r) => display_list_of_items(r.iter(), f),
            Self::VecU256(r) => display_list_of_items(r.iter(), f),
            Self::VecBool(r) => display_list_of_items(r.iter(), f),
            Self::VecAddress(r) => display_list_of_items(r.iter(), f),
        }?;

        write!(f, ")")
    }
}

impl Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reference::U8(ptr) => write!(f, "U8({})", ptr.to_ref()),
            Reference::U16(ptr) => write!(f, "U16({})", ptr.to_ref()),
            Reference::U32(ptr) => write!(f, "U32({})", ptr.to_ref()),
            Reference::U64(ptr) => write!(f, "U64({})", ptr.to_ref()),
            Reference::U128(ptr) => write!(f, "U128({})", ptr.to_ref()),
            Reference::U256(ptr) => write!(f, "U256({})", ptr.to_ref()),
            Reference::Bool(ptr) => write!(f, "Bool({})", ptr.to_ref()),
            Reference::Address(ptr) => write!(f, "Address({})", ptr.to_ref()),
            Reference::Container(ptr) => write!(f, "Container({:?})", ptr.to_ref()),
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

            Value::Container(c) => print_container(buf, c),
            Value::Reference(r) => print_reference(buf, r),
        }
    }

    #[allow(clippy::borrowed_box)]
    fn print_box_value_impl<B: Write>(buf: &mut B, val: &Box<Value>) -> PartialVMResult<()> {
        print_value_impl(buf, val.as_ref())
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

    fn print_container<B: Write>(buf: &mut B, c: &Container) -> PartialVMResult<()> {
        match c {
            Container::Vec(r) => print_list(buf, "[", r.iter(), print_box_value_impl, "]"),

            Container::Struct(r) => print_list(buf, "{ ", r.iter(), print_value_impl, " }"),

            Container::Variant(r) => {
                let (tag, values) = r.as_ref();
                print_list(
                    buf,
                    &format!("|{}|{{ ", tag),
                    values.iter(),
                    print_value_impl,
                    " }",
                )
            }

            Container::VecU8(r) => print_list(buf, "[", r.iter(), print_u8, "]"),
            Container::VecU16(r) => print_list(buf, "[", r.iter(), print_u16, "]"),
            Container::VecU32(r) => print_list(buf, "[", r.iter(), print_u32, "]"),
            Container::VecU64(r) => print_list(buf, "[", r.iter(), print_u64, "]"),
            Container::VecU128(r) => print_list(buf, "[", r.iter(), print_u128, "]"),
            Container::VecU256(r) => print_list(buf, "[", r.iter(), print_u256, "]"),
            Container::VecBool(r) => print_list(buf, "[", r.iter(), print_bool, "]"),
            Container::VecAddress(r) => print_list(buf, "[", r.iter(), print_address, "]"),
        }
    }

    // TODO: This function was used in an old implementation of std::debug::print, and can probably be removed.
    pub fn print_reference<B: Write>(buf: &mut B, r: &Reference) -> PartialVMResult<()> {
        debug_write!(buf, "(&) ")?;
        match r {
            Reference::U8(x) => print_u8(buf, x.to_ref()),
            Reference::U16(x) => print_u16(buf, x.to_ref()),
            Reference::U32(x) => print_u32(buf, x.to_ref()),
            Reference::U64(x) => print_u64(buf, x.to_ref()),
            Reference::U128(x) => print_u128(buf, x.to_ref()),
            Reference::U256(x) => print_u256(buf, x.to_ref()),
            Reference::Bool(x) => print_bool(buf, x.to_ref()),
            Reference::Address(x) => print_address(buf, x.to_ref()),

            Reference::Container(c) => print_container(buf, c.to_ref()),
        }
    }

    pub fn print_stack_frame<B: Write>(
        buf: &mut B,
        stack_frame: &StackFrame,
    ) -> PartialVMResult<()> {
        // REVIEW: The number of spaces in the indent is currently hard coded.
        for (idx, val) in stack_frame.iter().enumerate() {
            debug_write!(buf, "            [{}] ", idx)?;
            print_value_impl(buf, val)?;
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

// -------------------------------------------------------------------------------------------------
// FIXME FIXME FIXME
// Ask Tim for HELP
// FIXME FIXME FIXME
// -------------------------------------------------------------------------------------------------

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

impl<'a, 'b> serde::Serialize for AnnotatedValue<'a, 'b, MoveTypeLayout, Value> {
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

            (MoveTypeLayout::Struct(struct_layout), Value::Container(c))
                if matches!(**c, Container::Struct(_)) =>
            {
                let Container::Struct(r) = &**c else {
                    unreachable!()
                };
                (AnnotatedValue {
                    layout: &**struct_layout,
                    val: r,
                })
                .serialize(serializer)
            }

            (MoveTypeLayout::Enum(enum_layout), Value::Container(c))
                if matches!(**c, Container::Variant(_)) =>
            {
                let Container::Variant(r) = &**c else {
                    unreachable!()
                };
                (AnnotatedValue {
                    layout: &**enum_layout,
                    val: &**r,
                })
                .serialize(serializer)
            }

            (MoveTypeLayout::Vector(layout), Value::Container(c)) => {
                let layout = &**layout;
                match (layout, &**c) {
                    (MoveTypeLayout::U8, Container::VecU8(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U16, Container::VecU16(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U32, Container::VecU32(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U64, Container::VecU64(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U128, Container::VecU128(r)) => r.serialize(serializer),
                    (MoveTypeLayout::U256, Container::VecU256(r)) => r.serialize(serializer),
                    (MoveTypeLayout::Bool, Container::VecBool(r)) => r.serialize(serializer),
                    (MoveTypeLayout::Address, Container::VecAddress(r)) => r.serialize(serializer),

                    (_, Container::Vec(r)) => {
                        let v = r;
                        let mut t = serializer.serialize_seq(Some(v.len()))?;
                        for val in v.iter() {
                            t.serialize_element(&AnnotatedValue {
                                layout,
                                val: &**val,
                            })?;
                        }
                        t.end()
                    }

                    (layout, container) => Err(invariant_violation::<S>(format!(
                        "cannot serialize container {:?} as {:?}",
                        container, layout
                    ))),
                }
            }

            (MoveTypeLayout::Signer, Value::Container(c))
                if matches!(**c, Container::Struct(_)) =>
            {
                let Container::Struct(r) = &**c else {
                    unreachable!()
                };
                let v = r;
                if v.len() != 1 {
                    return Err(invariant_violation::<S>(format!(
                        "cannot serialize container as a signer -- expected 1 field got {}",
                        v.len()
                    )));
                }
                (AnnotatedValue {
                    layout: &MoveTypeLayout::Address,
                    val: &v[0],
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

impl<'a, 'b> serde::Serialize for AnnotatedValue<'a, 'b, MoveStructLayout, FixedSizeVec> {
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
            t.serialize_element(&AnnotatedValue {
                layout: field_layout,
                val,
            })?;
        }
        t.end()
    }
}

impl<'a, 'b> serde::Serialize
    for AnnotatedValue<'a, 'b, MoveEnumLayout, (VariantTag, FixedSizeVec)>
{
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

impl<'a, 'b> serde::Serialize for AnnotatedValue<'a, 'b, VariantFields<'a>, FixedSizeVec> {
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
                layout: &**struct_layout,
            }
            .deserialize(deserializer)?),

            L::Enum(enum_layout) => Ok(SeedWrapper {
                layout: &**enum_layout,
            }
            .deserialize(deserializer)?),

            L::Vector(layout) => {
                let container = match &**layout {
                    L::U8 => Container::VecU8(Vec::deserialize(deserializer)?),
                    L::U16 => Container::VecU16(Vec::deserialize(deserializer)?),
                    L::U32 => Container::VecU32(Vec::deserialize(deserializer)?),
                    L::U64 => Container::VecU64(Vec::deserialize(deserializer)?),
                    L::U128 => Container::VecU128(Vec::deserialize(deserializer)?),
                    L::U256 => Container::VecU256(Vec::deserialize(deserializer)?),
                    L::Bool => Container::VecBool(Vec::deserialize(deserializer)?),
                    L::Address => Container::VecAddress(Vec::deserialize(deserializer)?),
                    layout => {
                        // TODO: Box this as part of deserialization to avoid the second iteration?
                        let v = deserializer
                            .deserialize_seq(VectorElementVisitor(SeedWrapper { layout }))?
                            .into_iter()
                            .map(Box::new)
                            .collect();
                        Container::Vec(v)
                    }
                };
                Ok(Value::Container(Box::new(container)))
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
        Ok(Value::struct_(Struct::pack(fields)))
    }
}

impl<'d> serde::de::DeserializeSeed<'d> for SeedWrapper<&MoveEnumLayout> {
    type Value = Value;

    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        let variant = deserializer.deserialize_tuple(2, EnumFieldVisitor(&self.layout.0))?;
        Ok(Value::variant(variant))
    }
}

struct VectorElementVisitor<'a>(SeedWrapper<&'a MoveTypeLayout>);

impl<'d, 'a> serde::de::Visitor<'d> for VectorElementVisitor<'a> {
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

impl<'d, 'a> serde::de::Visitor<'d> for StructFieldVisitor<'a> {
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

impl<'d, 'a> serde::de::Visitor<'d> for EnumFieldVisitor<'a> {
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

impl<'d, 'a> serde::de::DeserializeSeed<'d> for &MoveRuntimeVariantFieldLayout<'a> {
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

impl Container {
    fn visit_impl(&self, visitor: &mut impl ValueVisitor, depth: usize) {
        use Container::*;

        match self {
            Vec(r) => {
                if visitor.visit_vec(depth, r.len()) {
                    for val in r.iter() {
                        val.visit_impl(visitor, depth + 1);
                    }
                }
            }
            Struct(r) => {
                if visitor.visit_struct(depth, r.len()) {
                    for val in r.iter() {
                        val.visit_impl(visitor, depth + 1);
                    }
                }
            }
            Variant(r) => {
                if visitor.visit_variant(depth, r.1.len()) {
                    for val in r.as_ref().1.iter() {
                        val.visit_impl(visitor, depth + 1);
                    }
                }
            }
            VecU8(r) => visitor.visit_vec_u8(depth, r),
            VecU16(r) => visitor.visit_vec_u16(depth, r),
            VecU32(r) => visitor.visit_vec_u32(depth, r),
            VecU64(r) => visitor.visit_vec_u64(depth, r),
            VecU128(r) => visitor.visit_vec_u128(depth, r),
            VecU256(r) => visitor.visit_vec_u256(depth, r),
            VecBool(r) => visitor.visit_vec_bool(depth, r),
            VecAddress(r) => visitor.visit_vec_address(depth, r),
        }
    }

    fn visit_indexed(&self, visitor: &mut impl ValueVisitor, depth: usize, idx: usize) {
        use Container::*;

        match self {
            Vec(r) => r[idx].visit_impl(visitor, depth + 1),
            Struct(r) => r[idx].visit_impl(visitor, depth + 1),
            Variant(r) => r.1[idx].visit_impl(visitor, depth + 1),
            VecU8(vals) => visitor.visit_u8(depth + 1, vals[idx]),
            VecU16(vals) => visitor.visit_u16(depth + 1, vals[idx]),
            VecU32(vals) => visitor.visit_u32(depth + 1, vals[idx]),
            VecU64(vals) => visitor.visit_u64(depth + 1, vals[idx]),
            VecU128(vals) => visitor.visit_u128(depth + 1, vals[idx]),
            VecU256(vals) => visitor.visit_u256(depth + 1, vals[idx]),
            VecBool(vals) => visitor.visit_bool(depth + 1, vals[idx]),
            VecAddress(vals) => visitor.visit_address(depth + 1, vals[idx]),
        }
    }
}

impl Reference {
    fn visit_impl(&self, visitor: &mut impl ValueVisitor, depth: usize) {
        if visitor.visit_ref(depth) {
            match self {
                Reference::U8(val) => visitor.visit_u8(depth + 1, *val.to_ref()),
                Reference::U16(val) => visitor.visit_u16(depth + 1, *val.to_ref()),
                Reference::U32(val) => visitor.visit_u32(depth + 1, *val.to_ref()),
                Reference::U64(val) => visitor.visit_u64(depth + 1, *val.to_ref()),
                Reference::U128(val) => visitor.visit_u128(depth + 1, *val.to_ref()),
                Reference::U256(val) => visitor.visit_u256(depth + 1, *val.to_ref()),
                Reference::Bool(val) => visitor.visit_bool(depth + 1, *val.to_ref()),
                Reference::Address(val) => visitor.visit_address(depth + 1, *val.to_ref()),

                Reference::Container(c) => c.to_ref().visit_impl(visitor, depth + 1),
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

            Value::Container(c) => c.visit_impl(visitor, depth),
            Value::Reference(r) => r.visit_impl(visitor, depth),
        }
    }
}

impl ValueView for Value {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        self.visit_impl(visitor, 0)
    }
}

impl ValueView for Struct {
    fn visit(&self, visitor: &mut impl ValueVisitor) {
        if visitor.visit_struct(0, self.fields.len()) {
            for val in self.fields.iter() {
                val.visit_impl(visitor, 1);
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
                    self.0.to_ref().visit_impl(visitor, 0)
                }
            }
        )+
    };
}

impl_container_ref_views!(VectorRef, StructRef, SignerRef, VariantRef);

// Note: We may want to add more helpers to retrieve value views behind references here.

impl Struct {
    #[allow(clippy::needless_lifetimes)]
    pub fn field_views<'a>(&'a self) -> impl ExactSizeIterator<Item = impl ValueView + 'a> {
        self.fields.iter()
    }
}

impl Variant {
    #[allow(clippy::needless_lifetimes)]
    pub fn field_views<'a>(&'a self) -> impl ExactSizeIterator<Item = impl ValueView + 'a> {
        self.fields.iter()
    }
}

impl Vector {
    pub fn elem_len(&self) -> usize {
        self.0.len()
    }

    #[allow(clippy::needless_lifetimes)]
    pub fn elem_views<'a>(&'a self) -> impl ExactSizeIterator<Item = impl ValueView + 'a> {
        struct ElemView<'b> {
            container: &'b Container,
            idx: usize,
        }

        impl<'b> ValueView for ElemView<'b> {
            fn visit(&self, visitor: &mut impl ValueVisitor) {
                self.container.visit_indexed(visitor, 0, self.idx)
            }
        }

        let len = self.0.len();

        (0..len).map(|idx| ElemView {
            container: &self.0,
            idx,
        })
    }
}

impl Reference {
    #[allow(clippy::needless_lifetimes)]
    pub fn value_view<'a>(&'a self) -> impl ValueView + 'a {
        struct ValueBehindRef<'b>(&'b Reference);

        impl<'b> ValueView for ValueBehindRef<'b> {
            fn visit(&self, visitor: &mut impl ValueVisitor) {
                match self.0 {
                    Reference::U8(val) => visitor.visit_u8(0, *val.to_ref()),
                    Reference::U16(val) => visitor.visit_u16(0, *val.to_ref()),
                    Reference::U32(val) => visitor.visit_u32(0, *val.to_ref()),
                    Reference::U64(val) => visitor.visit_u64(0, *val.to_ref()),
                    Reference::U128(val) => visitor.visit_u128(0, *val.to_ref()),
                    Reference::U256(val) => visitor.visit_u256(0, *val.to_ref()),
                    Reference::Bool(val) => visitor.visit_bool(0, *val.to_ref()),
                    Reference::Address(val) => visitor.visit_address(0, *val.to_ref()),

                    Reference::Container(c) => c.to_ref().visit_impl(visitor, 0),
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

        struct Wrapper<'b>(&'b Container);

        impl<'b> ValueView for Wrapper<'b> {
            fn visit(&self, visitor: &mut impl ValueVisitor) {
                let r = self.0;
                if visitor.visit_struct(0, r.len()) {
                    let Container::Struct(fields) = r else {
                        unreachable!()
                    };
                    for val in fields.iter() {
                        val.visit_impl(visitor, 1);
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

// -------------------------------------------------------------------------------------------------
// FIXME FIXME FIXME
// Ask Tim for HELP
// FIXME FIXME FIXME
// -------------------------------------------------------------------------------------------------

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
            (L::Enum(enum_layout), Value::Container(container)) => {
                let MoveEnumLayout(variants) = &**enum_layout;
                if let Container::Variant(r) = &**container {
                    let (tag, values) = &**r; // Dereference the Box to get the variant data
                    let tag = *tag; // Simply copy the u16 value, no need for dereferencing
                    let field_layouts = &variants[tag as usize];
                    let mut fields = vec![];
                    for (v, field_layout) in values.iter().zip(field_layouts) {
                        fields.push(v.as_move_value(field_layout));
                    }
                    RuntimeValue::Variant(RuntimeVariant { tag, fields })
                } else {
                    panic!("Expected Enum, got non-variant container");
                }
            }

            // Struct case with direct access to Box
            (L::Struct(struct_layout), Value::Container(container)) => {
                if let Container::Struct(r) = &**container {
                    let mut fields = vec![];
                    for (v, field_layout) in r.iter().zip(struct_layout.fields().iter()) {
                        fields.push(v.as_move_value(field_layout));
                    }
                    RuntimeValue::Struct(RuntimeStruct::new(fields))
                } else {
                    panic!("Expected Struct, got non-struct container");
                }
            }

            // Vector case with handling different container types
            (L::Vector(inner_layout), Value::Container(container)) => {
                RuntimeValue::Vector(match &**container {
                    Container::VecU8(r) => r.iter().map(|u| RuntimeValue::U8(*u)).collect(),
                    Container::VecU16(r) => r.iter().map(|u| RuntimeValue::U16(*u)).collect(),
                    Container::VecU32(r) => r.iter().map(|u| RuntimeValue::U32(*u)).collect(),
                    Container::VecU64(r) => r.iter().map(|u| RuntimeValue::U64(*u)).collect(),
                    Container::VecU128(r) => r.iter().map(|u| RuntimeValue::U128(*u)).collect(),
                    Container::VecU256(r) => r.iter().map(|u| RuntimeValue::U256(*u)).collect(),
                    Container::VecBool(r) => r.iter().map(|u| RuntimeValue::Bool(*u)).collect(),
                    Container::VecAddress(r) => {
                        r.iter().map(|u| RuntimeValue::Address(*u)).collect()
                    }
                    Container::Vec(r) => r
                        .iter()
                        .map(|v| v.as_move_value(inner_layout.as_ref()))
                        .collect(),
                    Container::Struct(_) => panic!("Got struct container when converting vec"),
                    Container::Variant { .. } => {
                        panic!("Got variant container when converting vec")
                    }
                })
            }

            // Signer case: just dereferencing the box and checking for address
            (L::Signer, Value::Container(container)) => {
                if let Container::Struct(r) = &**container {
                    if r.len() != 1 {
                        panic!("Unexpected signer layout: {:?}", r);
                    }
                    match &r[0] {
                        Value::Address(a) => RuntimeValue::Signer(**a),
                        v => panic!("Unexpected non-address while converting signer: {:?}", v),
                    }
                } else {
                    panic!("Expected Struct for Signer, got non-struct container");
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
        match (layout, self) {
            (L::U8, Value::U8(x)) => Some(AnnValue::U8(*x)),
            (L::U16, Value::U16(x)) => Some(AnnValue::U16(*x)),
            (L::U32, Value::U32(x)) => Some(AnnValue::U32(*x)),
            (L::U64, Value::U64(x)) => Some(AnnValue::U64(*x)),
            (L::U128, Value::U128(x)) => Some(AnnValue::U128(**x)),
            (L::U256, Value::U256(x)) => Some(AnnValue::U256(**x)),
            (L::Bool, Value::Bool(x)) => Some(AnnValue::Bool(*x)),
            (L::Address, Value::Address(x)) => Some(AnnValue::Address(**x)),
            (layout, Value::Container(container)) => container.as_annotated_move_value(layout),
            (layout, Value::Reference(ref_)) => ref_.as_annotated_move_value(layout),
            (_, _) => None,
        }
    }
}

impl Container {
    /// Converts the value to an annotated move value. This is only needed for tracing and care
    /// should be taken when using this function as it can possibly inflate the size of the value.
    pub(crate) fn as_annotated_move_value(&self, layout: &AnnTypeLayout) -> Option<AnnValue> {
        use AnnTypeLayout as L;
        match (layout, self) {
            (L::Vector(elem_layout), Container::Vec(entries)) => Some(AnnValue::Vector(
                entries
                    .iter()
                    .map(|v| v.as_annotated_move_value(elem_layout))
                    .collect::<Option<_>>()?,
            )),
            (L::Vector(_inner), Container::VecU8(entries)) => Some(AnnValue::Vector(
                entries.iter().map(|n| AnnValue::U8(*n)).collect(),
            )),
            (L::Vector(_inner), Container::VecU16(entries)) => Some(AnnValue::Vector(
                entries.iter().map(|n| AnnValue::U16(*n)).collect(),
            )),
            (L::Vector(_inner), Container::VecU32(entries)) => Some(AnnValue::Vector(
                entries.iter().map(|n| AnnValue::U32(*n)).collect(),
            )),
            (L::Vector(_inner), Container::VecU64(entries)) => Some(AnnValue::Vector(
                entries.iter().map(|n| AnnValue::U64(*n)).collect(),
            )),
            (L::Vector(_inner), Container::VecU128(entries)) => Some(AnnValue::Vector(
                entries.iter().map(|n| AnnValue::U128(*n)).collect(),
            )),
            (L::Vector(_inner), Container::VecU256(entries)) => Some(AnnValue::Vector(
                entries.iter().map(|n| AnnValue::U256(*n)).collect(),
            )),
            (L::Vector(_inner), Container::VecBool(entries)) => Some(AnnValue::Vector(
                entries.iter().map(|n| AnnValue::Bool(*n)).collect(),
            )),
            (L::Vector(_inner), Container::VecAddress(entries)) => Some(AnnValue::Vector(
                entries.iter().map(|n| AnnValue::Address(*n)).collect(),
            )),
            (L::Signer, Container::Struct(struct_)) => {
                let values = &struct_.0;
                let [Value::Address(ref addr)] = **values else {
                    return None;
                };
                Some(AnnValue::Signer(**addr))
            }
            (L::Struct(struct_layout), Container::Struct(struct_)) => {
                let fields = struct_
                    .0
                    .iter()
                    .zip(struct_layout.fields.iter())
                    .map(|(value, layout)| {
                        let value = value.as_annotated_move_value(&layout.layout)?;
                        let field_name = layout.name.clone();
                        Some((field_name, value))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let struct_ = AnnStruct::new(struct_layout.type_.clone(), fields);
                Some(AnnValue::Struct(struct_))
            }
            (L::Enum(enum_layout), Container::Variant(variant)) => {
                let AnnEnumLayout { type_, variants } = enum_layout.as_ref();
                let (tag, values) = variant.as_ref();
                let ((variant_name, _), field_layouts) =
                    variants.iter().find(|((_, vtag), _)| vtag == tag)?;

                let fields = values
                    .0
                    .iter()
                    .zip(field_layouts.iter())
                    .map(|(value, layout)| {
                        let value = value.as_annotated_move_value(&layout.layout)?;
                        let field_name = layout.name.clone();
                        Some((field_name, value))
                    })
                    .collect::<Option<Vec<_>>>()?;

                let tag = *tag;
                let type_ = type_.clone();
                let variant_name = variant_name.clone();
                let variant = AnnVariant {
                    tag,
                    fields,
                    type_,
                    variant_name,
                };
                Some(AnnValue::Variant(variant))
            }
            (_, _) => None,
        }
    }
}

impl Reference {
    /// Converts the value to an annotated move value. This is only needed for tracing and care
    /// should be taken when using this function as it can possibly inflate the size of the value.
    pub(crate) fn as_annotated_move_value(&self, layout: &AnnTypeLayout) -> Option<AnnValue> {
        use AnnTypeLayout as L;
        match (layout, self) {
            (L::U8, Reference::U8(value)) => Some(AnnValue::U8(*value.to_ref())),
            (L::U16, Reference::U16(value)) => Some(AnnValue::U16(*value.to_ref())),
            (L::U32, Reference::U32(value)) => Some(AnnValue::U32(*value.to_ref())),
            (L::U64, Reference::U64(value)) => Some(AnnValue::U64(*value.to_ref())),
            (L::U128, Reference::U128(value)) => Some(AnnValue::U128(*value.to_ref())),
            (L::U256, Reference::U256(value)) => Some(AnnValue::U256(*value.to_ref())),
            (L::Address, Reference::Address(value)) => Some(AnnValue::Address(*value.to_ref())),
            (L::Bool, Reference::Bool(value)) => Some(AnnValue::Bool(*value.to_ref())),
            (layout, Reference::Container(container)) => {
                container.to_ref().as_annotated_move_value(layout)
            }
            (_, _) => None,
        }
    }
}
