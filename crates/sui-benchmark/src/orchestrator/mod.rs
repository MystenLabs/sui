use std::io::stdout;

use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor, Stylize,
};

use self::{
    client::Client, error::TestbedResult, parameters::BenchmarkParametersGenerator,
    testbed::Testbed,
};

pub mod client;
pub mod config;
pub mod error;
pub mod metrics;
pub mod parameters;
pub mod settings;
pub mod ssh;
pub mod testbed;

pub struct Orchestrator<C: Client> {
    testbed: Testbed<C>,
    skip_testbed_update: bool,
    skip_testbed_reconfiguration: bool,
    ignore_logs: bool,
}

impl<C: Client> Orchestrator<C> {
    pub fn new(testbed: Testbed<C>) -> Self {
        Self {
            testbed,
            skip_testbed_update: false,
            skip_testbed_reconfiguration: false,
            ignore_logs: false,
        }
    }

    pub fn with_testbed_update(mut self, skip_testbed_update: bool) -> Self {
        self.skip_testbed_update = skip_testbed_update;
        self
    }

    pub fn with_testbed_reconfiguration(mut self, skip_testbed_reconfiguration: bool) -> Self {
        self.skip_testbed_reconfiguration = skip_testbed_reconfiguration;
        self
    }

    pub fn with_logs_analysis(mut self, ignore_logs: bool) -> Self {
        self.ignore_logs = ignore_logs;
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

    pub async fn run_benchmarks(
        &mut self,
        mut generator: BenchmarkParametersGenerator<usize>,
    ) -> TestbedResult<()> {
        // Cleanup the testbed (in case the previous run was not completed).
        self.testbed.cleanup(true).await?;

        // Update the software on all instances.
        if !self.skip_testbed_update {
            self.testbed.update().await?;
        }

        // Check whether to reconfigure the testbed before the first run.
        let mut latest_comittee_status = if self.skip_testbed_reconfiguration {
            (generator.nodes, generator.faults)
        } else {
            (0, 0)
        };

        // Run all benchmarks.
        let mut i = 1;
        while let Some(parameters) = generator.next_parameters() {
            crossterm::execute!(
                stdout(),
                SetForegroundColor(Color::Green),
                SetAttribute(Attribute::Bold),
                Print(format!("\nStarting benchmark {i}\n")),
                ResetColor
            )
            .unwrap();
            crossterm::execute!(
                stdout(),
                Print(format!("{}: {parameters}\n\n", "Parameters".bold())),
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
            aggregator.print_summary(&parameters);
            generator.register_result(aggregator);

            // Kill the nodes and clients (without deleting the log files).
            self.testbed.cleanup(false).await?;

            // Download the log files.
            if !self.ignore_logs {
                let error_counter = self.testbed.download_logs(&parameters).await?;
                error_counter.print_summary();
            }

            println!();
            i += 1;
        }
        Ok(())
    }
}
