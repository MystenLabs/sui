// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::*;
use move_trace_format::format::MoveTraceReader;
use move_vm_profiler::trace_converter::{GasProfiler, ProfilerConfig};
use std::path::PathBuf;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum AnalyzeTraceCommand {
    /// Generate a gas profile for the trace file compatible with the `speedscope.app` profiler.
    GasProfile {
        /// Whether function names should be fully qualified with their module and package
        /// addresses or if only the function name should be used.
        #[arg(long, short)]
        use_long_function_name: bool,
    },
}

impl AnalyzeTraceCommand {
    pub async fn execute(
        self,
        path: PathBuf,
        output_dir: Option<PathBuf>,
    ) -> Result<(), anyhow::Error> {
        let trace_file = std::fs::File::open(&path).map_err(|e| {
            anyhow!(
                "Failed to open trace file at {}: {e}",
                path.to_string_lossy()
            )
        })?;
        let trace_reader = MoveTraceReader::new(trace_file)
            .map_err(|e| anyhow!("Failed to read trace file: {e}"))?;

        match self {
            AnalyzeTraceCommand::GasProfile {
                use_long_function_name,
            } => {
                let mut profiler = GasProfiler::init(
                    ProfilerConfig {
                        output_dir,
                        use_long_function_name,
                    },
                    path.to_string_lossy().to_string(),
                );
                profiler.generate_from_trace(trace_reader);
                profiler.save_profile();
            }
        }

        Ok(())
    }
}
