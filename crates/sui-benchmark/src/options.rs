// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;

use strum_macros::EnumString;

use crate::drivers::Interval;

#[derive(Parser)]
#[clap(name = "Stress Testing Framework")]
pub struct Opts {
    /// Si&ze of the Sui committee.
    #[clap(long, default_value = "4", global = true)]
    pub committee_size: u64,
    /// Num of accounts to use for transfer objects
    #[clap(long, default_value = "5", global = true)]
    pub num_transfer_accounts: u64,
    /// Num server threads
    #[clap(long, default_value = "24", global = true)]
    pub num_server_threads: u64,
    /// Num client threads
    /// ideally same as number of workers
    #[clap(long, default_value = "3", global = true)]
    pub num_client_threads: u64,
    #[clap(long, default_value = "", global = true)]
    pub log_path: String,
    /// [Required for remote benchmark]
    /// Path where genesis.blob is stored when running remote benchmark
    #[clap(long, default_value = "/tmp/genesis.blob", global = true)]
    pub genesis_blob_path: String,
    /// [Required for remote benchmark]
    /// Path where keypair for primary gas account is stored. The format of
    /// this file is same as what `sui keytool generate` outputs
    #[clap(long, default_value = "", global = true)]
    pub keystore_path: String,
    /// [Required for remote benchmark]
    /// Object id of the primary gas coin used for benchmark
    /// NOTE: THe remote network should have this coin in its genesis config
    /// with large enough gas.
    #[clap(long, default_value = "", global = true)]
    pub primary_gas_owner_id: String,
    #[clap(long, default_value = "500", global = true)]
    pub gas_request_chunk_size: u64,
    /// Whether to run local or remote benchmark
    /// NOTE: For running remote benchmark we must have the following
    /// genesis_blob_path, keypair_path and primary_gas_id
    #[clap(long, parse(try_from_str), default_value = "true", global = true)]
    pub local: bool,
    /// Required in remote benchmark, namely when local = false
    /// Multiple fullnodes can be specified.
    #[clap(
        long,
        multiple_occurrences = false,
        multiple_values = true,
        value_delimiter = ',',
        global = true
    )]
    pub fullnode_rpc_addresses: Vec<String>,
    /// Whether to submit transactions to a fullnode.
    /// If true, use FullNodeProxy.
    /// Otherwise, use LocalValidatorAggregatorProxy.
    /// This param only matters when local = false, namely local runs always
    /// use a LocalValidatorAggregatorProxy.
    #[clap(long, parse(try_from_str), default_value = "false", global = true)]
    pub use_fullnode_for_execution: bool,
    /// True to use FullNodeReconfigObserver,
    /// Otherwise use EmbeddedReconfigObserver,
    #[clap(long, parse(try_from_str), default_value = "false", global = true)]
    pub use_fullnode_for_reconfig: bool,
    /// Default workload is 100% transfer object
    #[clap(subcommand)]
    pub run_spec: RunSpec,
    #[clap(long, default_value = "9091", global = true)]
    pub server_metric_port: u16,
    #[clap(long, default_value = "127.0.0.1", global = true)]
    pub client_metric_host: String,
    #[clap(long, default_value = "8081", global = true)]
    pub client_metric_port: u16,
    /// Whether or no to download TXes during follow
    #[clap(long, global = true)]
    pub download_txes: bool,
    /// Number of transactions or duration to
    /// run the benchmark for. Default set to
    /// "unbounded" i.e. benchmark runs forever
    /// until terminated with a ctrl-c. However,
    /// if we wanted to run the test for
    /// 60 seconds, this could be set as "60s".
    /// And if we wanted to run the test for
    /// 10,000 transactions we could set it to
    /// "10000"
    #[clap(long, global = true, default_value = "unbounded")]
    pub run_duration: Interval,
    /// Path where benchmark stats is stored
    #[clap(long, default_value = "/tmp/bench_result", global = true)]
    pub benchmark_stats_path: String,
    /// Path where previous benchmark stats is stored to use for comparison
    #[clap(long, default_value = "", global = true)]
    pub compare_with: String,
    // Stat collection interval seconds
    #[clap(long, default_value = "10", global = true)]
    pub stat_collection_interval: u64,
    // Enable stress stat collection. When enabled the sysinfo crate will be used
    // to gather system information. For example cpu usage will be polled every
    // 1 second and the P50/P99 usage statistics will be outputted either at
    // the end of the benchmark or periodically during a continuous run.
    #[clap(long, action, global = true)]
    pub stress_stat_collection: bool,
    // When starting multiple stress clients, stagger the start time by a random multiplier
    // between 0 and this value, times initialization time which is 1min. This helps to avoid
    // transaction conflicts between clients.
    #[clap(long, default_value = "0", global = true)]
    pub staggered_start_max_multiplier: u32,

    /// Start the stress test at a given protocol version. (Usually unnecessary if stress test is
    /// built at the same commit as the validators.
    #[clap(long, global = true)]
    pub protocol_version: Option<u64>,
}

#[derive(Debug, Clone, Parser, Eq, PartialEq, EnumString)]
#[non_exhaustive]
#[clap(rename_all = "kebab-case")]
pub enum RunSpec {
    // Allow the ability to mix shared object and
    // single owner transactions in the benchmarking
    // framework. Currently, only shared counter
    // and transfer object transaction types are
    // supported but there will be more in future. Also
    // there is no dependency between individual
    // transactions such that they can all be executed
    // and make progress in parallel. But this too
    // will likely change in future to support
    // more representative workloads.
    Bench {
        // ----- workloads ----
        // relative weight of shared counter
        // transaction in the benchmark workload
        #[clap(long, default_value = "0")]
        shared_counter: u32,
        // relative weight of transfer object
        // transactions in the benchmark workload
        #[clap(long, default_value = "1")]
        transfer_object: u32,
        // relative weight of delegation transactions in the benchmark workload
        #[clap(long, default_value = "0")]
        delegation: u32,
        // relative weight of batch payment transactions in the benchmark workload
        #[clap(long, default_value = "0")]
        batch_payment: u32,
        // relative weight of adversarial transactions in the benchmark workload
        #[clap(long, default_value = "0")]
        adversarial: u32,

        // --- workload-specific options --- (TODO: use subcommands or similar)
        // 100 for max hotness i.e all requests target
        // just the same shared counter, 0 for no hotness
        // i.e. all requests target a different shared
        // counter. The way total number of counters to
        // create is computed roughly as:
        // total_shared_counters = max(1, qps * (1.0 - hotness/100.0))
        #[clap(long, default_value = "50")]
        shared_counter_hotness_factor: u32,
        // batch size use for batch payment workload
        #[clap(long, default_value = "15")]
        batch_payment_size: u32,
        // type and load % of adversarial transactions in the benchmark workload.
        // Format is "{adversarial_type}-{load_factor}".
        // `load_factor` is a number between 0.0 and 1.0 which dictates how much load per tx
        // Default is (0-0.5) implying random load at 50% load. See `AdversarialPayloadType` enum for `adversarial_type`
        #[clap(long, default_value = "0-1.0")]
        adversarial_cfg: String,
        // Used to specify health check mode for certain workloads. i.e.
        // Transfer Workload: Send coin to the same account as the primary gas
        // owner account.
        // Shared Workload: TBD
        #[clap(long, parse(try_from_str), default_value = "false", global = true)]
        health_check: bool,

        // --- generic options ---
        // Target qps
        #[clap(long, default_value = "1000", global = true)]
        target_qps: u64,
        // Number of workers
        #[clap(long, default_value = "12", global = true)]
        num_workers: u64,
        // Max in-flight ratio
        #[clap(long, default_value = "5", global = true)]
        in_flight_ratio: u64,
    },
}
