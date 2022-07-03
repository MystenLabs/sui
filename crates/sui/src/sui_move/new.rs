// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_cli::package::cli::create_move_package;
use std::path::Path;

pub fn execute(path: &Path, name: &String) -> anyhow::Result<()> {
    create_move_package(path,
                        name,
                        "0.0.1",
                        "Sui",
                        "{ git = \"https://github.com/MystenLabs/sui.git\", subdir = \"crates/sui-framework\", rev = \"main\" }",
                        &name.to_lowercase(),
                        "0x0")?;
    Ok(())
}
