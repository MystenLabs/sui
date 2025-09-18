// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod layout;
pub mod lockfile;
pub mod manifest;
mod package_impl;
mod package_lock;
pub mod paths;
pub mod root_package;
pub use package_impl::*;
pub use root_package::RootPackage;

use sha2::{Digest, Sha256};

/// Computes the SHA-256 digest of the input string
fn compute_digest(input: &str) -> String {
    format!("{:X}", Sha256::digest(input.as_bytes()))
}
