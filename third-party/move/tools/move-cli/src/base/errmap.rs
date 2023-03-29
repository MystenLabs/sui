// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_package::{BuildConfig, ModelConfig};
use std::path::PathBuf;

/// Generate error map for the package and its dependencies at `path` for use by the Move
/// explanation tool.
#[derive(Parser)]
#[clap(name = "errmap")]
pub struct Errmap {
    /// The prefix that all error reasons within modules will be prefixed with, e.g., "E" if
    /// all error reasons are "E_CANNOT_PERFORM_OPERATION", "E_CANNOT_ACCESS", etc.
    #[clap(long)]
    pub error_prefix: Option<String>,
    /// The file to serialize the generated error map to.
    #[clap(long, default_value = "error_map", parse(from_os_str))]
    pub output_file: PathBuf,
}

impl Errmap {
    pub fn execute(self, path: Option<PathBuf>, config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let Self {
            error_prefix,
            output_file,
        } = self;
        let mut errmap_options = move_errmapgen::ErrmapOptions::default();
        if let Some(err_prefix) = error_prefix {
            errmap_options.error_prefix = err_prefix;
        }
        errmap_options.output_file = output_file
            .with_extension(move_command_line_common::files::MOVE_ERROR_DESC_EXTENSION)
            .to_string_lossy()
            .to_string();
        let model = config.move_model_for_package(
            &rerooted_path,
            ModelConfig {
                all_files_as_targets: true,
                target_filter: None,
            },
        )?;
        let mut errmap_gen = move_errmapgen::ErrmapGen::new(&model, &errmap_options);
        errmap_gen.gen();
        errmap_gen.save_result();
        Ok(())
    }
}
