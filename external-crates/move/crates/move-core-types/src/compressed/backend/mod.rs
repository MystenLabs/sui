// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Backing-store implementations for compressed layouts.
//!
//! Each storage submodule ([`arc_pool`], [`box_pool`]) provides a concrete way
//! of holding layout node data and implements the per-flavor `TypeLayout`
//! traits on its storage type. The infrastructure shared between node-table
//! backends is split three ways:
//!
//! - [`encoding`] — flavor-agnostic primitives: `LayoutRef`, `LeafType`,
//!   `PoolBuilder<Node>`.
//! - [`runtime_nodes`] — runtime-flavor node types, pool-builder alias, and
//!   view-construction helpers.
//! - [`annotated_nodes`] — annotated-flavor equivalent.

pub mod annotated_nodes;
pub mod arc_pool;
pub mod box_pool;
pub mod encoding;
pub mod runtime_nodes;

pub use arc_pool::ArcPool;
pub use box_pool::BoxPool;
pub use encoding::{LayoutRef, LeafType};

/// The default backend used by [`crate::compressed::annotated::MoveTypeLayout`]
/// and related types when no explicit backend type parameter is supplied.
pub type DefaultAnnotated = arc_pool::AnnotatedArcPool;

/// The default backend used by [`crate::compressed::runtime::MoveTypeLayout`]
/// and related types when no explicit backend type parameter is supplied.
pub type DefaultRuntime = arc_pool::RuntimeArcPool;

/// The default backend-builder used by
/// [`crate::compressed::annotated::MoveTypeLayoutBuilder`].
pub type DefaultAnnotatedBuilder = arc_pool::AnnotatedArcPoolBuilder;

/// The default backend-builder used by
/// [`crate::compressed::runtime::MoveTypeLayoutBuilder`].
pub type DefaultRuntimeBuilder = arc_pool::RuntimeArcPoolBuilder;
