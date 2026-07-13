// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    // `optional_include_str!` expands to `None` when an explanation file is absent, in which case
    // rustc records no dependency on it -- watch the whole directory so adding a new explanation
    // file triggers a rebuild.
    println!("cargo::rerun-if-changed=src/diagnostics/explanations");
}
