// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The core Move VM logic.
//!
//! It is a design goal for the Move VM to be independent of the Diem blockchain, so that
//! other blockchains can use it as well. The VM isn't there yet, but hopefully will be there
//! soon.

#![deny(
    clippy::cast_possible_truncation,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    unsafe_code,
)]
#![cfg_attr(
    test,
    allow(clippy::indexing_slicing, clippy::cast_possible_truncation)
)]

#[cfg(not(target_pointer_width = "64"))]
compile_error!("This code requires a 64-bit target");

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
