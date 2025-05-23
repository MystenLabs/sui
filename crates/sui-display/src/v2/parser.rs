// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::borrow::Cow;

use move_core_types::{
    account_address::AccountAddress,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};

/// A single Display string template is a sequence of strands.
#[derive(Debug)]
pub enum Strand<'s> {
    /// Text strands are ported literally to the output.
    Text(Cow<'s, str>),

    /// Expr strands are interpreted to some value whose string representation is included in the
    /// output. They are surrounded by curly braces in the syntax, to differentiate them from text.
    Expr(Expr<'s>),
}

/// Expressions are composed of a number of alternates and an optional transform. During
/// evaluation, each alternate is evaluated in turn until the first one succeeds, and if a
/// transform is provided, it is applied to the result to convert it to a string.
#[derive(Debug)]
pub struct Expr<'s> {
    alternates: Vec<Chain<'s>>,
    transform: Option<&'s str>,
}

/// Chains are a sequence of nested field accesses.
#[derive(Debug)]
pub struct Chain<'s> {
    /// An optional root expression. If not provided, the object being displayed is the root.
    root: Option<Literal<'s>>,

    /// A sequence of field accessors that go successively deeper into the object.
    accessors: Vec<Accessor<'s>>,
}

/// Different ways to nest deeply into an object.
#[derive(Debug)]
pub enum Accessor<'s> {
    /// Access a named field.
    Field(&'s str),

    /// Index into a vector, VecMap, or dynamic field.
    Index(Chain<'s>),

    /// Index into a dynamic object field.
    IIndex(Chain<'s>),
}

/// Literal forms are elements whose syntax determines their (outer) type.
#[derive(Debug)]
pub enum Literal<'s> {
    // Primitives
    Address(AccountAddress),
    Bool(bool),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(U256),

    // Aggregates
    ByteArray(Vec<u8>),
    String(Cow<'s, str>),
    Vector(Box<Vector<'s>>),

    // Datatypes
    Struct(Box<Struct<'s>>),
    Enum(Box<Enum<'s>>),
}

/// Contents of a vector literal.
#[derive(Debug)]
pub struct Vector<'s> {
    /// Element type, optional for non-empty vectors.
    type_: Option<TypeTag>,
    elements: Vec<Chain<'s>>,
}

/// Contents of a struct literal.
#[derive(Debug)]
pub struct Struct<'s> {
    type_: StructTag,
    fields: Fields<'s>,
}

/// Contents of an enum literal.
#[derive(Debug)]
pub struct Enum<'s> {
    type_: StructTag,
    variant_name: Option<&'s str>,
    variant_index: u16,
    fields: Fields<'s>,
}

#[derive(Debug)]
pub enum Fields<'s> {
    Positional(Vec<Chain<'s>>),
    Named(Vec<(&'s str, Chain<'s>)>),
}
