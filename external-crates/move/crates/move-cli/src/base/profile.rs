// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use clap::Parser;
use move_trace_format::format::MoveTraceReader;
use move_vm_profiler::trace_converter::{GasProfiler, ProfilerConfig};

/// Generate a gas profile from the Move trace file at `input`. If `output` is provided, the
/// profile will be saved under that directory, otherwise it will be saved in the current
/// directory.
#[derive(Parser)]
pub struct Profile {
    /// The name of the directory to output the profile under. If not provided the profile will be
    /// saved in the current directory.
    #[clap(short = 'o', long = "output")]
    pub output: Option<PathBuf>,
    /// The path to the trace file
    #[clap(short = 'i', long = "input")]
    pub input: PathBuf,
    /// Whether to use the full path for function names in the profile
    #[clap(
        long = "long-function-name",
        help = "Use the full path for function names in the profile",
        default_value = "false"
    )]
    pub long_function_name: bool,
}

impl Profile {
    pub fn execute(&self) -> anyhow::Result<()> {
        let fh = std::fs::File::open(&self.input)?;
        let reader = MoveTraceReader::new(fh)?;
        let mut profiler = GasProfiler::init(
            ProfilerConfig {
                output_dir: self.output.clone(),
                use_long_function_name: self.long_function_name,
            },
            self.input.to_string_lossy().to_string(),
        );

        profiler.generate_from_trace(reader);
        profiler.save_profile();
        Ok(())
    }
}
