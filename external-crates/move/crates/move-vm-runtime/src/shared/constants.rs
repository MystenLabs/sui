// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Constants
// -------------------------------------------------------------------------------------------------

// TODO Determine stack size limits based on gas limit
pub const OPERAND_STACK_SIZE_LIMIT: usize = 1024;
pub const CALL_STACK_SIZE_LIMIT: usize = 1024;

/// Maximal number of locals any individual call can have.
pub const LOCALS_PER_FRAME_LIMIT: usize = 2_048;

/// Maximum type depth when applying a type substitution.
pub const TYPE_DEPTH_MAX: usize = 256;

/// Maximal depth of a value in terms of type depth.
pub const VALUE_DEPTH_MAX: u64 = 128;

/// Maximal nodes which are allowed when converting to layout. This includes the types of
/// fields for struct types.
/// Maximal nodes which are allowed when converting to layout. This includes the types of
/// fields for datatypes.
pub const MAX_TYPE_TO_LAYOUT_NODES: u64 = 256;

/// Maximal nodes which are all allowed when instantiating a generic type. This does not include
/// field types of datatypes.
pub const MAX_TYPE_INSTANTIATION_NODES: u64 = 128;

/// Maximal depth of a value in terms of type depth.
pub const VALUE_DEPTH_MAX: u64 = 128;
