// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The core Move VM logic.
//!
//! It is a design goal for the Move VM to be independent of the Diem blockchain, so that
//! other blockchains can use it as well. The VM isn't there yet, but hopefully will be there
//! soon.

#![deny(unsafe_code)]

mod jit;
pub mod shared;

pub mod cache;
pub mod natives;
pub mod on_chain;
pub mod vm;

// #[macro_use]
// mod tracing;

// Only include debugging functionality in debug builds
// #[cfg(any(debug_assertions, feature = "debugging"))]
// mod debug;
