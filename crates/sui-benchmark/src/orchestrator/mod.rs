use std::io::stdout;

use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor, Stylize,
};

use self::{
    client::VultrClient, error::TestbedResult, parameters::BenchmarkParametersGenerator,
    testbed::Testbed,
};

pub mod client;
pub mod config;
pub mod error;
pub mod metrics;
pub mod parameters;
pub mod settings;
pub mod ssh;
pub mod state;
pub mod testbed;

pub struct Orchestrator {
    testbed: Testbed<VultrClient>,
    parameters_generator: BenchmarkParametersGenerator<usize>,
    do_not_update_testbed: bool,
    do_not_reconfigure_testbed: bool,
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
            do_not_update_testbed: false,
            do_not_reconfigure_testbed: false,
            ignore_logs: false,
        }
    }

    pub fn do_not_update_testbed(mut self) -> Self {
        self.do_not_update_testbed = true;
        self
    }

    pub fn do_not_reconfigure_testbed(mut self) -> Self {
        self.do_not_reconfigure_testbed = true;
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

    pub async fn run_benchmarks(&mut self) -> TestbedResult<()> {
        // Cleanup the testbed (in case the previous run was not completed).
        self.testbed.cleanup(true).await?;

        // Update the software on all instances.
        if !self.do_not_update_testbed {
            self.testbed.update().await?;
        }

        // Check whether to reconfigure the testbed before the first run.
        let mut latest_comittee_status = if self.do_not_reconfigure_testbed {
            (
                self.parameters_generator.nodes,
                self.parameters_generator.faults,
            )
        } else {
            (0, 0)
        };

        // Run all benchmarks.
        let mut i = 1;
        while let Some(parameters) = self.parameters_generator.next_parameters() {
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
            aggregator.save();
            aggregator.print_summary(&parameters);
            self.parameters_generator.register_result(aggregator);

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
