// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::{
    benchmark::{BenchmarkParametersGenerator, LoadType},
    client::{aws::AwsClient, vultr::VultrClient, ServerProviderClient},
    measurement::MeasurementsCollection,
    orchestrator::Orchestrator,
    plot::Plotter,
    settings::{CloudProvider, Settings},
    ssh::SshConnectionManager,
    testbed::Testbed,
};
use clap::Parser;
use color_eyre::eyre::{Context, Result};

mod benchmark;
mod client;
mod config;
mod error;
mod logs;
mod measurement;
mod orchestrator;
mod plot;
mod settings;
mod ssh;
mod testbed;

#[derive(Parser)]
#[clap(name = "Testbed orchestrator")]
pub struct Opts {
    /// The path to the settings file. This file contains basic information to deploy testbeds
    /// and run benchmarks such as the url of the git repo, the commit to deploy, etc.
    #[clap(
        long,
        value_name = "FILE",
        default_value = "crates/sui-orchestrator/assets/settings.json",
        global = true
    )]
    settings_path: String,

    /// The type of operation to run.
    #[clap(subcommand)]
    operation: Operation,
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
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
        #[clap(long, value_name = "INT", default_value = "0")]
        shared_objects_ratio: u16,

        /// The committee size to deploy.
        #[clap(long, value_name = "INT")]
        committee: usize,

        /// The fixed loads (in tx/s) to submit to the nodes.
        #[clap(
            long,
            value_name = "INT",
            multiple_occurrences = false,
            multiple_values = true,
            value_delimiter = ','
        )]
        loads: Vec<usize>,

        /// Number of faulty nodes.
        #[clap(long, value_name = "INT", default_value = "0")]
        faults: usize,

        /// The minimum duration of the benchmark in seconds.
        #[clap(long, value_parser = parse_duration, default_value = "180")]
        duration: Duration,

        /// Whether to skip testbed updates before running benchmarks.
        #[clap(long, action, default_value = "false")]
        skip_testbed_update: bool,

        /// Whether to skip testbed configuration before running benchmarks.
        #[clap(long, action, default_value = "false")]
        skip_testbed_configuration: bool,

        /// Whether to skip downloading and analyzing log files.
        #[clap(long, action, default_value = "false")]
        skip_logs_processing: bool,

        /// The timeout duration for ssh commands.
        #[clap(long, action, value_parser = parse_duration, default_value = "30")]
        timeout: Duration,

        /// The number of times the orchestrator should retry an ssh command.
        #[clap(long, value_name = "INT", default_value = "5")]
        retries: usize,
    },

    /// Print L-graphs from the collected data. These plots are very basic, their purpose
    /// is to get a quick lock at the data.
    Plot {
        /// The limit of the x-axis.
        #[clap(long, value_name = "FLOAT")]
        x_lim: Option<f32>,

        /// The limit of the y-axis.
        #[clap(long, value_name = "FLOAT")]
        y_lim: Option<f32>,
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
        CloudProvider::Vultr => {
            // Create the client for the cloud provider.
            let token = settings
                .load_token()
                .wrap_err("Failed to load cloud provider's token")?;
            let client = VultrClient::new(token, settings.clone());

            // Execute the command.
            run(settings, client, opts).await
        }
    }
}

async fn run<C: ServerProviderClient>(settings: Settings, client: C, opts: Opts) -> Result<()> {
    // Create a new testbed.
    let mut testbed = Testbed::new(settings.clone(), client)
        .await
        .wrap_err("Failed to crate testbed")?;

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
            shared_objects_ratio,
            committee,
            faults,
            duration,
            loads,
            skip_testbed_update,
            skip_testbed_configuration,
            skip_logs_processing,
            timeout,
            retries,
        } => {
            // TODO: Fix the load generator to support benchmarks with faults.
            if faults != 0 {
                unimplemented!("Benchmarks with faults are yet not supported")
            }

            // Create a new orchestrator to instruct the testbed.
            let username = testbed.username();
            let private_key_file = settings.ssh_private_key_file.clone();
            let ssh_manager = SshConnectionManager::new(username.into(), private_key_file)
                .with_timeout(timeout)
                .with_retries(retries);

            let instances = testbed.instances();
            let orchestrator = Orchestrator::new(settings, instances, ssh_manager);

            let shared_objects_ratio = shared_objects_ratio.min(100);
            let loads = if loads.is_empty() { vec![200] } else { loads };

            let generator = BenchmarkParametersGenerator::new(
                shared_objects_ratio,
                committee,
                LoadType::Fixed(loads),
            )
            .with_custom_duration(duration)
            .with_faults(faults);

            orchestrator
                .skip_testbed_updates(skip_testbed_update)
                .skip_testbed_configuration(skip_testbed_configuration)
                .skip_logs_processing(skip_logs_processing)
                .run_benchmarks(generator)
                .await
                .wrap_err("Failed to run benchmarks")?;
        }

        // Plot (basic) L-graphs from the collected data.
        Operation::Plot { x_lim, y_lim } => Plotter::new(settings)
            .with_x_lim(x_lim)
            .with_y_lim(y_lim)
            .load_measurements()
            .plot_latency_throughput()
            .wrap_err("Failed to plot data")?,

        // Print a summary of the specified measurements collection.
        Operation::Summarize { path } => MeasurementsCollection::load(path)?.display_summary(),
    }
    Ok(())
}

fn parse_duration(arg: &str) -> Result<Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}
