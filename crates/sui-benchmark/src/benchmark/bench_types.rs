// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::{Config, NetworkConfig};

use super::load_generator::calculate_throughput;
use clap::*;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeMap;
use std::default::Default;
use std::path::PathBuf;
use strum_macros::EnumString;
use sui_types::base_types::ObjectID;
use sui_types::crypto::{KeyPair, PublicKeyBytes};

#[derive(Debug, Clone, Parser)]
#[clap(
    name = "Sui Benchmark",
    about = "Local test and benchmark of the Sui authorities"
)]
pub struct Benchmark {
    /// Size of the Sui committee.
    #[clap(long, default_value = "30", global = true)]
    pub committee_size: usize,
    /// Timeout for sending queries (us)
    #[clap(long, default_value = "400000000", global = true)]
    pub send_timeout_us: u64,
    /// Timeout for receiving responses (us)
    #[clap(long, default_value = "400000000", global = true)]
    pub recv_timeout_us: u64,
    /// Maximum size of datagrams received and sent (bytes)
    #[clap(long, default_value = "65000", global = true)]
    pub buffer_size: usize,
    /// Number of connections to the server
    #[clap(long, default_value = "0", global = true)]
    pub tcp_connections: usize,
    /// Number of database cpus
    #[clap(long, default_value = "1", global = true)]
    pub db_cpus: usize,
    /// Use Move orders
    #[clap(long, global = true)]
    pub use_native: bool,
    #[clap(long, default_value = "2", global = true)]
    pub batch_size: usize,

    #[clap(
        arg_enum,
        default_value = "single-validator-thread",
        global = true,
        ignore_case = true
    )]
    pub running_mode: RunningMode,

    #[clap(long, global = true)]
    pub working_dir: Option<PathBuf>,

    /// Type of benchmark to run
    #[clap(subcommand)]
    pub bench_type: BenchmarkType,
}

#[derive(Parser, Debug, Clone, PartialEq, EnumString, Eq)]
#[clap(rename_all = "kebab-case")]
pub enum BenchmarkType {
    #[clap(name = "microbench")]
    MicroBenchmark {
        /// Hostname
        #[clap(long, default_value = "127.0.0.1")]
        host: String,
        /// Base port number
        #[clap(long, default_value = "9555")]
        port: u16,
        #[clap(subcommand)]
        type_: MicroBenchmarkType,
    },
    // ... more benchmark types here
}

#[derive(Debug, Parser, Clone, Copy, ArgEnum, EnumString)]
#[clap(rename_all = "kebab-case")]
pub enum RunningMode {
    SingleValidatorThread,
    SingleValidatorProcess,
    RemoteValidator,
}

#[derive(Debug, Clone, Parser, Eq, PartialEq, EnumString)]
#[clap(rename_all = "kebab-case")]
pub enum MicroBenchmarkType {
    Throughput {
        /// Number of transactions to be sent in the benchmark
        #[clap(long, default_value = "1000000")]
        num_transactions: usize,
    },
    Latency {
        /// Number of chunks to send
        #[clap(long, default_value = "100")]
        num_chunks: usize,
        /// Size of chunks per tick
        #[clap(long, default_value = "1000")]
        chunk_size: usize,
        /// The time between each tick. Default 10ms
        #[clap(long, default_value = "10000")]
        period_us: u64,
    },
}

impl std::fmt::Display for BenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::fmt::Display for MicroBenchmarkType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Default for MicroBenchmarkType {
    fn default() -> Self {
        MicroBenchmarkType::Throughput {
            num_transactions: 100_000,
        }
    }
}

#[derive(Debug, Clone)]
pub enum BenchmarkResult {
    MicroBenchmark(MicroBenchmarkResult),
}
impl std::fmt::Display for BenchmarkResult {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BenchmarkResult::MicroBenchmark(m) => write!(f, "{}", m),
        }
    }
}

#[derive(Debug, Clone)]
pub enum MicroBenchmarkResult {
    Throughput {
        chunk_throughput: f64,
    },
    CombinedLatency {
        load_chunk_size: usize,
        tick_period_us: usize,
        load_latencies: Vec<u128>,
        chunk_latencies: Vec<u128>,
    },
    Latency {
        load_chunk_size: usize,
        tick_period_us: usize,
        latencies: Vec<u128>,
    },
}

impl std::fmt::Display for MicroBenchmarkResult {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MicroBenchmarkResult::Throughput { chunk_throughput } => {
                write!(f, "Throughout: {} tps", chunk_throughput)
            }
            MicroBenchmarkResult::CombinedLatency {
                chunk_latencies,
                load_chunk_size,
                tick_period_us: tick_period,
                ..
            } => {
                // Average the latency. Probably not the best idea since they vary
                // Should probably do better stats on these numbers
                let tracer_avg =
                    chunk_latencies.iter().sum::<u128>() as f64 / chunk_latencies.len() as f64;

                write!(
                    f,
                    "Average Latency {} us @ {} tps ({} samples)",
                    tracer_avg,
                    calculate_throughput(*load_chunk_size, *tick_period as u128),
                    chunk_latencies.len()
                )
            }
            MicroBenchmarkResult::Latency {
                load_chunk_size,
                tick_period_us,
                latencies,
            } => {
                let tracer_avg = latencies.iter().sum::<u128>() as f64 / latencies.len() as f64;

                write!(
                    f,
                    "Average Latency {} us @ {} tps ({} samples)",
                    tracer_avg,
                    calculate_throughput(*load_chunk_size, *tick_period_us as u128),
                    latencies.len()
                )
            }
        }
    }
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct RemoteLoadGenConfig {
    /// Keypairs of all the validators
    /// Ideally we wouldnt need this, but sometime we pre-sign certs
    pub validator_keypairs: BTreeMap<PublicKeyBytes, KeyPair>,
    /// Account keypair to use for transactions
    pub account_keypair: KeyPair,
    /// ObjectID offset for transaction objects
    pub object_id_offset: ObjectID,
    /// Network config for accessing validators
    pub network_config: NetworkConfig,
}

impl Config for RemoteLoadGenConfig {}
