// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The core Move VM logic.
//!
//! It is a design goal for the Move VM to be independent of the Diem blockchain, so that
//! other blockchains can use it as well. The VM isn't there yet, but hopefully will be there
//! soon.

#![deny(unsafe_code)]

// #[macro_use]
// mod tracing;
// mod tracing2;

mod jit;
pub mod shared;

pub mod cache;
pub mod dev_utils;
pub mod execution;
pub mod natives;
pub mod runtime;
pub mod validation;

#[cfg(test)]
mod unit_tests;

// #[macro_use]
// mod tracing;

// Only include debugging functionality in debug or tracing builds
// #[cfg(any(debug_assertions, feature = "tracing"))]
// mod debug;

use cache::identifier_interner::IdentifierInterner;
use once_cell::sync::Lazy;
use std::sync::Arc;

/// IDENTIFIER INTERNER
/// The Ientifier Interner is global across Move Runtimes and defined here. This is for two reasons:
/// 1. The interner is _always_ a win compared to non-interned identifiers, which hold their
///    strings in boxes. This is always a strict memory win, in all cases. The overall size of the
///    interner plus its definitions is always going to be smaller than holding those individual
///    identifiers.
/// 2. Different runs will benefit from intern reuse: even if the runtime is discarded, interning
///    is a near-constant cost when spinning up a new runtime. Moreover, the interner can be set to
///    have a maximum memory it will refuse to exceed.
///    TODO: Set up this; `lasso` supports it but we need to expose that interface.
/// 3. If absolutely necessary, the execution layer _can_ dump the interner.
static STRING_INTERNER: Lazy<Arc<IdentifierInterner>> =
    Lazy::new(|| Arc::new(IdentifierInterner::default()));

/// Function to access the global StringInterner
fn string_interner() -> Arc<IdentifierInterner> {
    Arc::clone(&STRING_INTERNER)
}
