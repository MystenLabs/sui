// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use codespan_reporting::term::termcolor::Buffer;
use log::debug;
use std::path::Path;
use std::path::PathBuf;
use std::{fs::File, io::Read};
use tempfile::TempDir;

pub fn test_move(path: &Path) -> anyhow::Result<()> {
    let out_path = TempDir::new()?;
    let config = BuildConfig {
        dev_mode: true,
        test_mode: false,
        generate_docs: true,
        install_dir: Some(out_path),
        force_recompilation: false,
        ..Default::default()
    };
    let resolved_package =
        config.resolution_graph_for_package(self.toml_path, None, &mut progress)?;
    model_builder::build(resolved_package)?;
    Ok(())
}

datatest_stable::harness!(test_move, "tests/move", r".*\.toml",);
