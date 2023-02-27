use std::{
    fmt::{Debug, Display},
    hash::Hash,
    io::stdout,
    time::Duration,
};

use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use serde::{Deserialize, Serialize};

use self::{
    client::VultrClient, error::TestbedResult, metrics::MetricsCollector, testbed::Testbed,
};

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

impl BenchmarkParameters {
    pub fn new(nodes: usize, faults: usize, load: usize, duration: Duration) -> Self {
        Self {
            nodes,
            faults,
            load,
            duration,
        }
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

pub enum LoadType {
    Fixed(Vec<usize>),
    Search {
        starting_load: usize,
        latency_increase_tolerance: usize,
        max_iterations: usize,
    },
}

pub struct BenchmarkParametersGenerator<ScraperId: Serialize + Clone> {
    nodes: usize,
    load_type: LoadType,
    faults: usize,
    duration: Duration,
    next_load: Option<usize>,

    lower_bound_result: Option<MetricsCollector<ScraperId>>,
    upper_bound_result: Option<MetricsCollector<ScraperId>>,
    iterations: usize,
}

impl<ScraperId> BenchmarkParametersGenerator<ScraperId>
where
    ScraperId: Serialize + Eq + Hash + Clone,
{
    const DEFAULT_DURATION: Duration = Duration::from_secs(180);

    pub fn new(nodes: usize, mut load_type: LoadType) -> Self {
        let next_load = match &mut load_type {
            LoadType::Fixed(loads) => {
                if loads.is_empty() {
                    None
                } else {
                    Some(loads.remove(0))
                }
            }
            LoadType::Search { starting_load, .. } => Some(*starting_load),
        };
        Self {
            nodes,
            load_type,
            faults: 0,
            duration: Self::DEFAULT_DURATION,
            next_load,
            lower_bound_result: None,
            upper_bound_result: None,
            iterations: 0,
        }
    }

    pub fn with_faults(mut self, faults: usize) -> Self {
        self.faults = faults;
        self
    }

    pub fn with_custom_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn register_result(&mut self, result: MetricsCollector<ScraperId>) {
        self.next_load = match &mut self.load_type {
            LoadType::Fixed(loads) => {
                if loads.is_empty() {
                    None
                } else {
                    Some(loads.remove(0))
                }
            }
            LoadType::Search {
                latency_increase_tolerance,
                max_iterations,
                ..
            } => {
                if self.iterations >= *max_iterations {
                    None
                } else {
                    self.iterations += 1;
                    match (&mut self.lower_bound_result, &mut self.upper_bound_result) {
                        (None, None) => {
                            let next = result.load() * 2;
                            self.lower_bound_result = Some(result);
                            Some(next)
                        }
                        (Some(lower), None) => {
                            let threshold = lower.load() * (*latency_increase_tolerance);
                            if result.load() > threshold {
                                let next = (lower.load() + result.load()) / 2;
                                self.upper_bound_result = Some(result);
                                Some(next)
                            } else {
                                let next = result.load() * 2;
                                *lower = result;
                                Some(next)
                            }
                        }
                        (Some(lower), Some(upper)) => {
                            let threshold = lower.load() * (*latency_increase_tolerance);
                            if result.load() > threshold {
                                *upper = result;
                            } else {
                                *lower = result;
                            }
                            Some((lower.load() + upper.load()) / 2)
                        }
                        _ => panic!("Benchmark parameters builder is in an incoherent state"),
                    }
                }
            }
        };
    }

    pub fn next_parameters(&mut self) -> Option<BenchmarkParameters> {
        self.next_load.map(|load| {
            BenchmarkParameters::new(self.nodes, self.faults, load, self.duration.clone())
        })
    }
}

pub struct Orchestrator {
    testbed: Testbed<VultrClient>,
    parameters_generator: BenchmarkParametersGenerator<usize>,
    skip_update: bool,
    skip_configure: bool,
    ignore_logs: bool,
}

impl Orchestrator {
    pub fn new(
        testbed: Testbed<VultrClient>,
        parameters_generator: BenchmarkParametersGenerator<usize>,
    ) -> Self {
        Self {
            testbed,
            parameters_generator,
            skip_update: false,
            skip_configure: false,
            ignore_logs: false,
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

    pub fn ignore_logs(mut self) -> Self {
        self.ignore_logs = true;
        self
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

    pub async fn run_benchmarks(&self, benchmarks: Vec<BenchmarkParameters>) -> TestbedResult<()> {
        // Cleanup the testbed (in case the previous run was not completed).
        self.testbed.cleanup(true).await?;

        // Update the software on all instances.
        self.testbed.update().await?;

        // Run all benchmarks.
        let mut latest_comittee_status = (0, 0);
        for parameters in benchmarks {
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
                self.testbed.configure(&parameters).await?;
                latest_comittee_status = (parameters.nodes, parameters.faults);
            }

            // Deploy the validators.
            self.testbed.run_nodes(&parameters).await?;

            // Deploy the load generators.
            self.testbed.run_clients(&parameters).await?;

            // Wait for the benchmark to terminate. Then save the results and print a summary.
            let aggregator = self.testbed.collect_metrics(&parameters).await?;
            aggregator.save();
            aggregator.print_summary(&parameters);

            // Kill the nodes and clients (without deleting the log files).
            self.testbed.cleanup(false).await?;

            // Download the log files.
            let error_counter = self.testbed.download_logs(&parameters).await?;
            error_counter.print_summary();
        }
        Ok(())
    }
}
