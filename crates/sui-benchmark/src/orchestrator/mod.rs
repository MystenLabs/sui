use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time;

use crate::orchestrator::metrics::MetricsCollector;

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

pub struct Benchmark {
    parameters: BenchmarkParameters,
    skip_update: bool,
    skip_configure: bool,
}

impl Benchmark {
    pub fn new(parameters: BenchmarkParameters) -> Self {
        Self {
            parameters,
            skip_update: false,
            skip_configure: false,
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
        for benchmark in benchmarks {
            let parameters = &benchmark.parameters;

            // Cleanup the testbed (in case the previous run was not completed).
            self.testbed.cleanup(true).await?;

            // Update the software on all instances.
            // self.testbed.update().await?;

            // Configure all instances.
            self.testbed.configure(parameters).await?;

            // Deploy the validators.
            self.testbed.run_nodes(parameters).await?;

            // Deploy the load generators.
            self.testbed.run_clients(parameters).await?;

            // Wait for the benchmark to terminate.
            println!("Waiting for about {}s...", parameters.duration.as_secs());

            let mut aggregator = MetricsCollector::new(parameters.clone());
            let mut interval = time::interval(Duration::from_secs(30));
            interval.tick().await; // The first tick returns immediately.
            loop {
                interval.tick().await;
                match self.testbed.scrape(&mut aggregator, parameters).await {
                    Ok(duration) if duration >= parameters.duration => break,
                    _ => (),
                }
            }
            aggregator.save();
            aggregator.print_summary(parameters);

            // Kill the nodes and clients (without deleting the log files).
            self.testbed.cleanup(false).await?;

            // Download the log files.
            self.testbed.logs(parameters).await?;
        }
        Ok(())
    }
}
