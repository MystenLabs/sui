// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "sui-single-node-benchmark",
    about = "Benchmark a single validator node",
    rename_all = "kebab-case",
    author,
    version
)]
pub enum Command {
    #[command(name = "no-move")]
    NoMove {
        #[arg(
            long,
            default_value_t = 1000000,
            help = "Number of transactions to submit"
        )]
        tx_count: u64,
        #[arg(
            long,
            default_value_t = false,
            help = "Whether to include cert verification and tx manager in the benchmark"
        )]
        end_to_end: bool,
    },
    #[command(name = "move")]
    Move {
        #[arg(
            long,
            default_value_t = 1000000,
            help = "Number of transactions to submit"
        )]
        tx_count: u64,
        #[arg(
            long,
            default_value_t = false,
            help = "Whether to include cert verification and tx manager in the benchmark"
        )]
        end_to_end: bool,
        #[arg(
            long,
            default_value_t = 2,
            help = "Number of address owned input objects per transaction.\
            This represents the amount of DB reads per transaction prior to execution."
        )]
        num_input_objects: u8,
        #[arg(
            long,
            default_value_t = 0,
            help = "Number of dynamic fields read per transaction.\
            This represents the amount of runtime DB reads per transaction during execution."
        )]
        num_dynamic_fields: u64,
        #[arg(
            long,
            default_value_t = 0,
            help = "Computation intensity per transaction.\
            The transaction computes the n-th Fibonacci number \
            specified by this parameter * 100."
        )]
        computation: u8,
    },
}
