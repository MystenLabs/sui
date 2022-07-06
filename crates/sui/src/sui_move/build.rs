// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_package::BuildConfig;
use std::path::Path;

pub fn execute(
    path: &Path,
    dump_bytecode_as_base64: bool,
    build_config: BuildConfig,
) -> anyhow::Result<()> {
    if dump_bytecode_as_base64 {
        let compiled_modules = sui_framework::build_move_package_to_base64(path, build_config)?;
        println!("{:?}", compiled_modules);
    } else {
        sui_framework::build_and_verify_package(path, build_config)?;
    }
    Ok(())
}
