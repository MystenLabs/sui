// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The core Move VM logic.
//!
//! It is a design goal for the Move VM to be independent of the Diem blockchain, so that
//! other blockchains can use it as well. The VM isn't there yet, but hopefully will be there
//! soon.

#[forbid(unsafe_code)]
pub mod data_cache;
#[forbid(unsafe_code)]
mod interpreter;
mod loader;
#[forbid(unsafe_code)]
pub mod logging;
#[forbid(unsafe_code)]
pub mod move_vm;
#[forbid(unsafe_code)]
pub mod native_extensions;
#[forbid(unsafe_code)]
pub mod native_functions;
#[forbid(unsafe_code)]
pub mod runtime;
#[forbid(unsafe_code)]
pub mod session;
#[macro_use]
#[forbid(unsafe_code)]
mod tracing;

// Only include debugging functionality in debug builds
#[cfg(any(debug_assertions, feature = "debugging"))]
#[forbid(unsafe_code)]
mod debug;

#[cfg(test)]
#[forbid(unsafe_code)]
mod unit_tests;
