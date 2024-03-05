// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Result;
fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
