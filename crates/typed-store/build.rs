// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    // This build script enables tidehunter config used to gate tidehunter in sui codebase
    println!("cargo::rerun-if-env-changed=USE_TIDEHUNTER");
    println!("cargo::rustc-check-cfg=cfg(tidehunter)");
    if std::env::var("USE_TIDEHUNTER").is_ok() {
        println!("cargo::rustc-cfg=tidehunter");
    }
}
