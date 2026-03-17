// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object::rpc_visitor::Meter;
use crate::object::rpc_visitor::MeterError;

/// A trait for serializing Move values into some nested structured representation that supports
/// `null`, `bool`, numbers, strings, vectors, and maps (e.g. JSON or Protobuf).
///
/// Formats decide both the output shape and how each output operation is charged against a meter.
pub trait Format: Sized {
    type Vec: Default;
    type Map: Default;

    fn is_null(&self) -> bool;
    fn is_bool(&self) -> bool;
    fn is_number(&self) -> bool;
    fn is_string(&self) -> bool;
    fn is_array(&self) -> bool;
    fn is_object(&self) -> bool;

    fn as_bool(&self) -> Option<bool>;
    fn as_string(&self) -> Option<&str>;
    fn as_array(&self) -> Option<&Self::Vec>;
    fn as_object(&self) -> Option<&Self::Map>;

    /// Write a `null` value.
    fn null<M: Meter>(meter: &mut M) -> Result<Self, MeterError>;

    /// Write a `true` or `false` value.
    fn bool<M: Meter>(meter: &mut M, value: bool) -> Result<Self, MeterError>;

    /// Write a numeric value that fits in a `u32`.
    fn number<M: Meter>(meter: &mut M, value: u32) -> Result<Self, MeterError>;

    /// Write a string value.
    fn string<M: Meter>(meter: &mut M, value: String) -> Result<Self, MeterError>;

    /// Write a completed vector.
    fn vec<M: Meter>(meter: &mut M, value: Self::Vec) -> Result<Self, MeterError>;

    /// Write a completed key-value map.
    fn map<M: Meter>(meter: &mut M, value: Self::Map) -> Result<Self, MeterError>;

    /// Add an element to a vector.
    fn vec_push_element<M: Meter>(
        meter: &mut M,
        vec: &mut Self::Vec,
        val: Self,
    ) -> Result<(), MeterError>;

    /// Add a key-value pair to a map.
    fn map_push_field<M: Meter>(
        meter: &mut M,
        map: &mut Self::Map,
        key: String,
        val: Self,
    ) -> Result<(), MeterError>;
}
