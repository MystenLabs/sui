use std::{
    fmt::{Debug, Display},
    io::stdout,
    time::Duration,
};

use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use serde::{Deserialize, Serialize};

use self::{client::VultrClient, error::TestbedResult, testbed::Testbed};

pub mod client;
pub mod config;
pub mod error;
pub mod metrics;
pub mod settings;
pub mod ssh;
pub mod state;
pub mod testbed;

#[derive(Serialize, Deserialize, Clone)]
pub struct BenchmarkParameters {
    /// The committee size.
    pub nodes: usize,
    /// The number of (crash-)faults.
    pub faults: usize,
    /// The total load (tx/s) to submit to the system.
    pub load: usize,
    /// The duration of the benchmark.
    pub duration: Duration,
}

impl Default for BenchmarkParameters {
    fn default() -> Self {
        Self {
            nodes: 4,
            faults: 0,
            load: 500,
            duration: Duration::from_secs(60),
        }
    }
}

impl Debug for BenchmarkParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}-{}-{}-{}",
            self.nodes,
            self.faults,
            self.load,
            self.duration.as_secs()
        )
    }
}

impl Display for BenchmarkParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} nodes ({} faulty)", self.nodes, self.faults)
    }
}

pub struct Benchmark {
    parameters: BenchmarkParameters,
    skip_update: bool,
    skip_configure: bool,
    download_logs: bool,
}

impl Benchmark {
    pub fn new(parameters: BenchmarkParameters) -> Self {
        Self {
            parameters,
            skip_update: false,
            skip_configure: false,
            download_logs: false,
        }
    }

    pub fn skip_update(mut self) -> Self {
        self.skip_update = true;
        self
    }

    pub fn skip_configure(mut self) -> Self {
        self.skip_configure = true;
        self
    }

    pub fn download_logs(mut self) -> Self {
        self.download_logs = true;
        self
    }
}

pub struct Orchestrator {
    testbed: Testbed<VultrClient>,
}

impl Orchestrator {
    pub fn new(testbed: Testbed<VultrClient>) -> Self {
        Self { testbed }
    }

    pub async fn deploy_testbed(&mut self, instances: usize) -> TestbedResult<()> {
        self.testbed.populate(instances).await?;
        self.testbed.install().await?;
        self.testbed.update().await?;
        self.testbed.info();

        crossterm::execute!(
            stdout(),
            SetForegroundColor(Color::Green),
            SetAttribute(Attribute::Bold),
            Print("\nTestbed ready for use\n"),
            ResetColor
        )
        .unwrap();

        Ok(())
    }

    pub async fn destroy_testbed(&mut self) -> TestbedResult<()> {
        self.testbed.destroy().await
    }

    pub async fn start_testbed(&mut self, instances: usize) -> TestbedResult<()> {
        self.testbed.start(instances).await
    }

    pub async fn stop_testbed(&mut self) -> TestbedResult<()> {
        self.testbed.stop().await
    }

    pub fn print_testbed_status(&mut self) {
        self.testbed.info();
    }

    pub async fn run_benchmarks(&self, benchmarks: Vec<Benchmark>) -> TestbedResult<()> {
        // Cleanup the testbed (in case the previous run was not completed).
        self.testbed.cleanup(true).await?;

        // Update the software on all instances.
        self.testbed.update().await?;

        // Run all benchmarks.
        let mut latest_comittee_status = (0, 0);
        for benchmark in benchmarks {
            let parameters = &benchmark.parameters;
            crossterm::execute!(
                stdout(),
                SetForegroundColor(Color::Green),
                SetAttribute(Attribute::Bold),
                Print("\nStarting benchmark\n"),
                ResetColor
            )
            .unwrap();

            // Cleanup the testbed (in case the previous run was not completed).
            self.testbed.cleanup(true).await?;

            // Configure all instances (if needed).
            if latest_comittee_status != (parameters.nodes, parameters.faults) {
                self.testbed.configure(parameters).await?;
                latest_comittee_status = (parameters.nodes, parameters.faults);
            }

            // Deploy the validators.
            self.testbed.run_nodes(parameters).await?;

            // Deploy the load generators.
            self.testbed.run_clients(parameters).await?;

            // Wait for the benchmark to terminate. Then save the results and print a summary.
            let aggregator = self.testbed.collect_metrics(parameters).await?;
            aggregator.save();
            aggregator.print_summary(parameters);

            // Kill the nodes and clients (without deleting the log files).
            self.testbed.cleanup(false).await?;

            // Download the log files.
            let error_counter = self.testbed.download_logs(parameters).await?;
            error_counter.print_summary();
        }
        Ok(())
    }
}
