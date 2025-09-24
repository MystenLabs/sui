// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::base::reroot_path;

use anyhow::anyhow;
use move_package::BuildConfig;

use clap::*;
use std::path::{Path, PathBuf};

/// Disassemble the Move bytecode pointed to
#[derive(Parser)]
#[clap(
    name = "decompile",
    about = "Decompile Move bytecode into Move source code"
)]
pub struct Decompile {
    #[clap(long = "input")]
    /// The path to the directory or file to decompile.
    pub input: String,
    #[clap(long = "output")]
    /// The path to write the output
    pub output_path: String,
}

enum Input {
    File(PathBuf),
    Directory(PathBuf),
}

impl Decompile {
    pub fn execute(self, path: Option<&Path>, _config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        let Self { input, output_path } = self;
        // Ensure the input file exists
        let input_path = Path::new(&input);
        if !input_path.exists() {
            anyhow::bail!("Input path '{}' does not exist", input);
        }
        // Determine if the input is a file or directory
        let input = if input_path.is_file() {
            Input::File(input_path.to_path_buf())
        } else if input_path.is_dir() {
            Input::Directory(input_path.to_path_buf())
        } else {
            anyhow::bail!("Input path '{}' is neither a file nor a directory", input);
        };

        // Process the input accordingly
        let files_to_process = match input {
            Input::File(file_path) => vec![file_path],
            Input::Directory(dir_path) => {
                let mut files = Vec::new();
                let mut paths_to_check = vec![dir_path];
                while let Some(new_paths) = paths_to_check.pop() {
                    for entry in std::fs::read_dir(&new_paths)
                        .map_err(|_| anyhow!("Directory path invalid"))?
                    {
                        let entry = entry?;
                        let path = entry.path();
                        if path.is_dir() {
                            paths_to_check.push(path);
                        } else if path.is_file()
                            && path.extension().and_then(|s| s.to_str()) == Some("mv")
                        {
                            files.push(path);
                        }
                    }
                }
                files
            }
        };

        // Ensure the output path exists
        let output_path = if Path::new(&output_path).is_absolute() {
            output_path
        } else {
            rerooted_path
                .join(output_path)
                .to_string_lossy()
                .to_string()
        };
        let output_path = PathBuf::from(output_path);
        std::fs::create_dir_all(&output_path).map_err(|_| anyhow!("Failed to create directory"))?;
        let _paths = move_decompiler::generate_from_files(&files_to_process, &output_path)?;
        Ok(())
    }
}
