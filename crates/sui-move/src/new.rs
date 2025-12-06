// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use indoc::formatdoc;
use move_cli::base::new;
use move_package_alt::package::layout::SourcePackageLayout;
use std::{
    fs::create_dir_all,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[group(id = "sui-move-new")]
pub struct New {
    #[clap(flatten)]
    pub new: new::New,
}

impl New {
    pub fn execute(self, path: Option<&Path>) -> anyhow::Result<()> {
        let name = self.new.name_var()?;
        self.new.execute(path)?;
        std::fs::write(
            self.new.source_file_path(&path)?,
            formatdoc!(
                r#"
                /*
                /// Module: {name}
                module {name}::{name};
                */

                // For Move coding conventions, see
                // https://docs.sui.io/concepts/sui-move-concepts/conventions

                "#,
            ),
        )?;

        std::fs::write(
            self.test_file_path(&path)?,
            formatdoc!(
                r#"
                /*
                #[test_only]
                module {name}::{name}_tests;
                // uncomment this line to import the module
                // use {name}::{name};

                #[error(code = 0)]
                const ENotImplemented: vector<u8> = b"Not Implemented";

                #[test]
                fun test_{name}() {{
                    // pass
                }}

                #[test, expected_failure(abort_code = ::{name}::{name}_tests::ENotImplemented)]
                fun test_{name}_fail() {{
                    abort ENotImplemented
                }}
                */
                "#,
            ),
        )?;

        Ok(())
    }

    pub fn test_file_path(&self, path: &Option<&Path>) -> anyhow::Result<PathBuf> {
        let dir = self
            .new
            .root_dir(path)?
            .join(SourcePackageLayout::Tests.path());

        create_dir_all(&dir)?;

        Ok(dir.join(format!("{}_tests.move", self.new.name_var()?)))
    }
}
