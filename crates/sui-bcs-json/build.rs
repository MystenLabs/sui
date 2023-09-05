// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate cbindgen;
use cbindgen::Language;
use std::env;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let config = cbindgen::Config {
        usize_is_size_t: true,
        ..Default::default()
    };
    cbindgen::Builder::new()
        .with_config(config)
        .with_language(Language::C)
        .with_crate(crate_dir)
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file("sui_bcs_json_bindings.h");
}
