// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{str::FromStr, time::Duration};

use benchmark::{BenchmarkParametersGenerator, LoadType};
use clap::Parser;
use client::{aws::AwsClient, ServerProviderClient};
use eyre::{Context, Result};
use faults::FaultsType;
use measurement::MeasurementsCollection;
use orchestrator::Orchestrator;
use protocol::narwhal::{NarwhalBenchmarkType, NarwhalProtocol};
use settings::{CloudProvider, Settings};
use ssh::SshConnectionManager;
use testbed::Testbed;

pub mod benchmark;
pub mod client;
pub mod display;
pub mod error;
pub mod faults;
pub mod logs;
pub mod measurement;
mod monitor;
pub mod orchestrator;
pub mod protocol;
pub mod settings;
pub mod ssh;
pub mod testbed;

/// NOTE: Link these types to the correct protocol. Either Sui or Narwhal.
// use protocol::sui::{SuiBenchmarkType, SuiProtocol};
// type Protocol = SuiProtocol;
// type BenchmarkType = SuiBenchmarkType;
type Protocol = NarwhalProtocol;
type BenchmarkType = NarwhalBenchmarkType;

#[derive(Parser)]
#[command(author, version, about = "Testbed orchestrator", long_about = None)]
pub struct Opts {
    /// The path to the settings file. This file contains basic information to deploy testbeds
    /// and run benchmarks such as the url of the git repo, the commit to deploy, etc.
    #[clap(
        long,
        value_name = "FILE",
        default_value = "crates/sui-aws-orchestrator/assets/settings.json",
        global = true
    )]
    settings_path: String,

    /// The type of operation to run.
    #[clap(subcommand)]
    operation: Operation,
}

#[derive(Parser)]
pub enum Operation {
    /// Get or modify the status of the testbed.
    Testbed {
        #[clap(subcommand)]
        action: TestbedAction,
    },

    /// Run a benchmark on the specified testbed.
    Benchmark {
        /// Percentage of shared vs owned objects; 0 means only owned objects and 100 means
        /// only shared objects.
        #[clap(long, default_value = "0", global = true)]
        benchmark_type: String,

        /// The committee size to deploy.
        #[clap(long, value_name = "INT")]
        committee: usize,

        /// Number of faulty nodes.
        #[clap(long, value_name = "INT", default_value = "0", global = true)]
        faults: usize,

        /// Whether the faulty nodes recover.
        #[clap(long, action, default_value = "false", global = true)]
        crash_recovery: bool,

        /// The interval to crash nodes in seconds.
        #[clap(long, value_parser = parse_duration, default_value = "60", global = true)]
        crash_interval: Duration,

        /// The minimum duration of the benchmark in seconds.
        #[clap(long, value_parser = parse_duration, default_value = "600", global = true)]
        duration: Duration,

        /// The interval between measurements collection in seconds.
        #[clap(long, value_parser = parse_duration, default_value = "15", global = true)]
        scrape_interval: Duration,

        /// Whether to skip testbed updates before running benchmarks.
        #[clap(long, action, default_value = "false", global = true)]
        skip_testbed_update: bool,

        /// Whether to skip testbed configuration before running benchmarks.
        #[clap(long, action, default_value = "false", global = true)]
        skip_testbed_configuration: bool,

        /// Whether to download and analyze the client and node log files.
        #[clap(long, action, default_value = "false", global = true)]
        log_processing: bool,

        /// The number of instances running exclusively load generators. If set to zero the
        /// orchestrator collocates one load generator with each node.
        #[clap(long, value_name = "INT", default_value = "0", global = true)]
        dedicated_clients: usize,

        /// Whether to forgo a grafana and prometheus instance and leave the testbed unmonitored.
        #[clap(long, action, default_value = "false", global = true)]
        skip_monitoring: bool,

        /// The timeout duration for ssh commands (in seconds).
        #[clap(long, action, value_parser = parse_duration, default_value = "30", global = true)]
        timeout: Duration,

        /// The number of times the orchestrator should retry an ssh command.
        #[clap(long, value_name = "INT", default_value = "5", global = true)]
        retries: usize,

        /// The load to submit to the system.
        #[clap(subcommand)]
        load_type: Load,
    },

    /// Print a summary of the specified measurements collection.
    Summarize {
        /// The path to the settings file.
        #[clap(long, value_name = "FILE")]
        path: String,
    },
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum TestbedAction {
    /// Display the testbed status.
    Status,

    /// Deploy the specified number of instances in all regions specified by in the setting file.
    Deploy {
        /// Number of instances to deploy.
        #[clap(long)]
        instances: usize,

        /// The region where to deploy the instances. If this parameter is not specified, the
        /// command deploys the specified number of instances in all regions listed in the
        /// setting file.
        #[clap(long)]
        region: Option<String>,
    },

    /// Start at most the specified number of instances per region on an existing testbed.
    Start {
        /// Number of instances to deploy.
        #[clap(long, default_value = "200")]
        instances: usize,
    },

    /// Stop an existing testbed (without destroying the instances).
    Stop,

    /// Destroy the testbed and terminate all instances.
    Destroy,
}

#[derive(Parser)]
pub enum Load {
    /// The fixed loads (in tx/s) to submit to the nodes.
    FixedLoad {
        /// A list of fixed load (tx/s).
        #[clap(
            long,
            value_name = "INT",
            num_args(1..),
            value_delimiter = ','
        )]
        loads: Vec<usize>,
    },

    /// Search for the maximum load that the system can sustainably handle.
    Search {
        /// The initial load (in tx/s) to test and use a baseline.
        #[clap(long, value_name = "INT", default_value = "250")]
        starting_load: usize,
        /// The maximum number of iterations before converging on a breaking point.
        #[clap(long, value_name = "INT", default_value = "5")]
        max_iterations: usize,
    },
}

fn parse_duration(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let opts: Opts = Opts::parse();

    // Load the settings files.
    let settings = Settings::load(&opts.settings_path).wrap_err("Failed to load settings")?;

    match &settings.cloud_provider {
        CloudProvider::Aws => {
            // Create the client for the cloud provider.
            let client = AwsClient::new(settings.clone()).await;

            // Execute the command.
            run(settings, client, opts).await
        }
    }
}

async fn run<C: ServerProviderClient>(settings: Settings, client: C, opts: Opts) -> Result<()> {
    // Create a new testbed.
    let mut testbed = Testbed::new(settings.clone(), client)
        .await
        .wrap_err("Failed to create testbed")?;

    match opts.operation {
        Operation::Testbed { action } => match action {
            // Display the current status of the testbed.
            TestbedAction::Status => testbed.status(),

            // Deploy the specified number of instances on the testbed.
            TestbedAction::Deploy { instances, region } => testbed
                .deploy(instances, region)
                .await
                .wrap_err("Failed to deploy testbed")?,

            // Start the specified number of instances on an existing testbed.
            TestbedAction::Start { instances } => testbed
                .start(instances)
                .await
                .wrap_err("Failed to start testbed")?,

            // Stop an existing testbed.
            TestbedAction::Stop => testbed.stop().await.wrap_err("Failed to stop testbed")?,

            // Destroy the testbed and terminal all instances.
            TestbedAction::Destroy => testbed
                .destroy()
                .await
                .wrap_err("Failed to destroy testbed")?,
        },

        // Run benchmarks.
        Operation::Benchmark {
            benchmark_type,
            committee,
            faults,
            crash_recovery,
            crash_interval,
            duration,
            scrape_interval,
            skip_testbed_update,
            skip_testbed_configuration,
            log_processing,
            dedicated_clients,
            skip_monitoring,
            timeout,
            retries,
            load_type,
        } => {
            // Create a new orchestrator to instruct the testbed.
            let username = testbed.username();
            let private_key_file = settings.ssh_private_key_file.clone();
            let ssh_manager = SshConnectionManager::new(username.into(), private_key_file)
                .with_timeout(timeout)
                .with_retries(retries);

            let instances = testbed.instances();

            let setup_commands = testbed
                .setup_commands()
                .await
                .wrap_err("Failed to load testbed setup commands")?;

            let protocol_commands = Protocol::new(&settings);
            let benchmark_type = BenchmarkType::from_str(&benchmark_type)?;

            let load = match load_type {
                Load::FixedLoad { loads } => {
                    let loads = if loads.is_empty() { vec![200] } else { loads };
                    LoadType::Fixed(loads)
                }
                Load::Search {
                    starting_load,
                    max_iterations,
                } => LoadType::Search {
                    starting_load,
                    max_iterations,
                },
            };

            let fault_type = if !crash_recovery || faults == 0 {
                FaultsType::Permanent { faults }
            } else {
                FaultsType::CrashRecovery {
                    max_faults: faults,
                    interval: crash_interval,
                }
            };

            let generator = BenchmarkParametersGenerator::new(committee, load)
                .with_benchmark_type(benchmark_type)
                .with_custom_duration(duration)
                .with_faults(fault_type);

            Orchestrator::new(
                settings,
                instances,
                setup_commands,
                protocol_commands,
                ssh_manager,
            )
            .with_scrape_interval(scrape_interval)
            .with_crash_interval(crash_interval)
            .skip_testbed_updates(skip_testbed_update)
            .skip_testbed_configuration(skip_testbed_configuration)
            .with_log_processing(log_processing)
            .with_dedicated_clients(dedicated_clients)
            .skip_monitoring(skip_monitoring)
            .run_benchmarks(generator)
            .await
            .wrap_err("Failed to run benchmarks")?;
        }

        // Print a summary of the specified measurements collection.
        Operation::Summarize { path } => {
            MeasurementsCollection::<BenchmarkType>::load(path)?.display_summary()
        }
    }
    Ok(())
}
