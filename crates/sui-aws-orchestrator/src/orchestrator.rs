// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, VecDeque},
    fs::{self},
    marker::PhantomData,
    path::PathBuf,
    time::Duration,
};

use tokio::time::{self, Instant};

use crate::monitor::Monitor;
use crate::{
    benchmark::{BenchmarkParameters, BenchmarkParametersGenerator, BenchmarkType},
    client::Instance,
    display, ensure,
    error::{TestbedError, TestbedResult},
    faults::CrashRecoverySchedule,
    logs::LogsAnalyzer,
    measurement::{Measurement, MeasurementsCollection},
    protocol::{ProtocolCommands, ProtocolMetrics},
    settings::Settings,
    ssh::{CommandContext, CommandStatus, SshConnectionManager},
};

/// An orchestrator to run benchmarks on a testbed.
pub struct Orchestrator<P, T> {
    /// The testbed's settings.
    settings: Settings,
    /// The state of the testbed (reflecting accurately the state of the machines).
    instances: Vec<Instance>,
    /// The type of the benchmark parameters.
    benchmark_type: PhantomData<T>,
    /// Provider-specific commands to install on the instance.
    instance_setup_commands: Vec<String>,
    /// Protocol-specific commands generator to generate the protocol configuration files,
    /// boot clients and nodes, etc.
    protocol_commands: P,
    /// The interval between measurements collection.
    scrape_interval: Duration,
    /// The interval to crash nodes.
    crash_interval: Duration,
    /// Handle ssh connections to instances.
    ssh_manager: SshConnectionManager,
    /// Whether to skip testbed updates before running benchmarks.
    skip_testbed_update: bool,
    /// Whether to skip testbed configuration before running benchmarks.
    skip_testbed_configuration: bool,
    /// Whether to downloading and analyze the client and node log files.
    log_processing: bool,
    /// Number of instances running only load generators (not nodes). If this value is set
    /// to zero, the orchestrator runs a load generate collocated with each node.
    dedicated_clients: usize,
    /// Whether to forgo a grafana and prometheus instance and leave the testbed unmonitored.
    skip_monitoring: bool,
}

impl<P, T> Orchestrator<P, T> {
    /// The default interval between measurements collection.
    const DEFAULT_SCRAPE_INTERVAL: Duration = Duration::from_secs(15);
    /// The default interval to crash nodes.
    const DEFAULT_CRASH_INTERVAL: Duration = Duration::from_secs(60);

    /// Make a new orchestrator.
    pub fn new(
        settings: Settings,
        instances: Vec<Instance>,
        instance_setup_commands: Vec<String>,
        protocol_commands: P,
        ssh_manager: SshConnectionManager,
    ) -> Self {
        Self {
            settings,
            instances,
            benchmark_type: PhantomData,
            instance_setup_commands,
            protocol_commands,
            ssh_manager,
            scrape_interval: Self::DEFAULT_SCRAPE_INTERVAL,
            crash_interval: Self::DEFAULT_CRASH_INTERVAL,
            skip_testbed_update: false,
            skip_testbed_configuration: false,
            log_processing: false,
            dedicated_clients: 0,
            skip_monitoring: false,
        }
    }

    /// Set interval between measurements collection.
    pub fn with_scrape_interval(mut self, scrape_interval: Duration) -> Self {
        self.scrape_interval = scrape_interval;
        self
    }

    /// Set interval with which to crash nodes.
    pub fn with_crash_interval(mut self, crash_interval: Duration) -> Self {
        self.crash_interval = crash_interval;
        self
    }

    /// Set whether to skip testbed updates before running benchmarks.
    pub fn skip_testbed_updates(mut self, skip_testbed_update: bool) -> Self {
        self.skip_testbed_update = skip_testbed_update;
        self
    }

    /// Whether to skip testbed configuration before running benchmarks.
    pub fn skip_testbed_configuration(mut self, skip_testbed_configuration: bool) -> Self {
        self.skip_testbed_configuration = skip_testbed_configuration;
        self
    }

    /// Set whether to download and analyze the client and node log files.
    pub fn with_log_processing(mut self, log_processing: bool) -> Self {
        self.log_processing = log_processing;
        self
    }

    /// Set the number of instances running exclusively load generators.
    pub fn with_dedicated_clients(mut self, dedicated_clients: usize) -> Self {
        self.dedicated_clients = dedicated_clients;
        self
    }

    /// Set whether to boot grafana on the local machine to monitor the nodes.
    pub fn skip_monitoring(mut self, skip_monitoring: bool) -> Self {
        self.skip_monitoring = skip_monitoring;
        self
    }

    /// Select on which instances of the testbed to run the benchmarks. This function returns two vector
    /// of instances; the first contains the instances on which to run the load generators and the second
    /// contains the instances on which to run the nodes.
    pub fn select_instances(
        &self,
        parameters: &BenchmarkParameters<T>,
    ) -> TestbedResult<(Vec<Instance>, Vec<Instance>, Option<Instance>)> {
        // Ensure there are enough active instances.
        let available_instances: Vec<_> = self.instances.iter().filter(|x| x.is_active()).collect();
        let minimum_instances = if self.skip_monitoring {
            parameters.nodes + self.dedicated_clients
        } else {
            parameters.nodes + self.dedicated_clients + 1
        };
        ensure!(
            available_instances.len() >= minimum_instances,
            TestbedError::InsufficientCapacity(minimum_instances - available_instances.len())
        );

        // Sort the instances by region.
        let mut instances_by_regions = HashMap::new();
        for instance in available_instances {
            instances_by_regions
                .entry(&instance.region)
                .or_insert_with(VecDeque::new)
                .push_back(instance);
        }

        // Select the instance to host the monitoring stack.
        let mut monitoring_instance = None;
        if !self.skip_monitoring {
            for region in &self.settings.regions {
                if let Some(regional_instances) = instances_by_regions.get_mut(region) {
                    if let Some(instance) = regional_instances.pop_front() {
                        monitoring_instance = Some(instance.clone());
                    }
                    break;
                }
            }
        }

        // Select the instances to host exclusively load generators.
        let mut client_instances = Vec::new();
        for region in self.settings.regions.iter().cycle() {
            if client_instances.len() == self.dedicated_clients {
                break;
            }
            if let Some(regional_instances) = instances_by_regions.get_mut(region) {
                if let Some(instance) = regional_instances.pop_front() {
                    client_instances.push(instance.clone());
                }
            }
        }

        // Select the instances to host the nodes.
        let mut nodes_instances = Vec::new();
        for region in self.settings.regions.iter().cycle() {
            if nodes_instances.len() == parameters.nodes {
                break;
            }
            if let Some(regional_instances) = instances_by_regions.get_mut(region) {
                if let Some(instance) = regional_instances.pop_front() {
                    nodes_instances.push(instance.clone());
                }
            }
        }

        // Spawn a load generate collocated with each node if there are no instances dedicated
        // to excursively run load generators.
        if client_instances.is_empty() {
            client_instances = nodes_instances.clone();
        }

        Ok((client_instances, nodes_instances, monitoring_instance))
    }
}

impl<P: ProtocolCommands<T> + ProtocolMetrics, T: BenchmarkType> Orchestrator<P, T> {
    /// Boot one node per instance.
    async fn boot_nodes(
        &self,
        instances: Vec<Instance>,
        parameters: &BenchmarkParameters<T>,
    ) -> TestbedResult<()> {
        // Run one node per instance.
        let targets = self
            .protocol_commands
            .node_command(instances.clone(), parameters);

        let repo = self.settings.repository_name();
        let context = CommandContext::new()
            .run_background("node".into())
            .with_log_file("~/node.log".into())
            .with_execute_from_path(repo.into());
        self.ssh_manager
            .execute_per_instance(targets, context)
            .await?;

        // Wait until all nodes are reachable.
        let commands = self
            .protocol_commands
            .nodes_metrics_command(instances.clone());
        self.ssh_manager.wait_for_success(commands).await;

        Ok(())
    }

    /// Install the codebase and its dependencies on the testbed.
    pub async fn install(&self) -> TestbedResult<()> {
        display::action("Installing dependencies on all machines");

        let working_dir = self.settings.working_dir.display();
        let url = &self.settings.repository.url;
        let basic_commands = [
            "sudo apt-get update",
            "sudo apt-get -y upgrade",
            "sudo apt-get -y autoremove",
            // Disable "pending kernel upgrade" message.
            "sudo apt-get -y remove needrestart",
            // The following dependencies:
            // * build-essential: prevent the error: [error: linker `cc` not found].
            // * libssl-dev - Required to compile the orchestrator, todo remove this dependency
            "sudo apt-get -y install build-essential libssl-dev",
            // Install rust (non-interactive).
            "curl --proto \"=https\" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
            "echo \"source $HOME/.cargo/env\" | tee -a ~/.bashrc",
            "source $HOME/.cargo/env",
            "rustup default stable",
            // Create the working directory.
            &format!("mkdir -p {working_dir}"),
            // Clone the repo.
            &format!("(git clone {url} || true)"),
        ];

        let cloud_provider_specific_dependencies: Vec<_> = self
            .instance_setup_commands
            .iter()
            .map(|x| x.as_str())
            .collect();

        let protocol_dependencies = self.protocol_commands.protocol_dependencies();

        let command = [
            &basic_commands[..],
            &Monitor::dependencies()[..],
            &cloud_provider_specific_dependencies[..],
            &protocol_dependencies[..],
        ]
        .concat()
        .join(" && ");

        let active = self.instances.iter().filter(|x| x.is_active()).cloned();
        let context = CommandContext::default();
        self.ssh_manager.execute(active, command, context).await?;

        display::done();
        Ok(())
    }

    /// Reload prometheus on all instances.
    pub async fn start_monitoring(&self, parameters: &BenchmarkParameters<T>) -> TestbedResult<()> {
        let (clients, nodes, instance) = self.select_instances(parameters)?;
        if let Some(instance) = instance {
            display::action("Configuring monitoring instance");

            let monitor = Monitor::new(instance, clients, nodes, self.ssh_manager.clone());
            monitor.start_prometheus(&self.protocol_commands).await?;
            monitor.start_grafana().await?;

            display::done();
            display::config("Grafana address", monitor.grafana_address());
            display::newline();
        }

        Ok(())
    }

    /// Update all instances to use the version of the codebase specified in the setting file.
    pub async fn update(&self) -> TestbedResult<()> {
        display::action("Updating all instances");

        // Update all active instances. This requires compiling the codebase in release (which
        // may take a long time) so we run the command in the background to avoid keeping alive
        // many ssh connections for too long.
        let commit = &self.settings.repository.commit;
        let command = [
            "git fetch -f",
            &format!("(git checkout -b {commit} {commit} || git checkout -f {commit})"),
            "(git pull -f || true)",
            "source $HOME/.cargo/env",
            "cargo build --release",
        ]
        .join(" && ");

        let active = self.instances.iter().filter(|x| x.is_active()).cloned();

        let id = "update";
        let repo_name = self.settings.repository_name();
        let context = CommandContext::new()
            .run_background(id.into())
            .with_execute_from_path(repo_name.into());
        self.ssh_manager
            .execute(active.clone(), command, context)
            .await?;

        // Wait until the command finished running.
        self.ssh_manager
            .wait_for_command(active, id, CommandStatus::Terminated)
            .await?;

        display::done();
        Ok(())
    }

    /// Configure the instances with the appropriate configuration files.
    pub async fn configure(&self, parameters: &BenchmarkParameters<T>) -> TestbedResult<()> {
        display::action("Configuring instances");

        // Select instances to configure.
        let (clients, nodes, _) = self.select_instances(parameters)?;

        // Generate the genesis configuration file and the keystore allowing access to gas objects.
        let command = self.protocol_commands.genesis_command(nodes.iter());
        let repo_name = self.settings.repository_name();
        let context = CommandContext::new().with_execute_from_path(repo_name.into());
        let all = clients.into_iter().chain(nodes);
        self.ssh_manager.execute(all, command, context).await?;

        display::done();
        Ok(())
    }

    /// Cleanup all instances and optionally delete their log files.
    pub async fn cleanup(&self, cleanup: bool) -> TestbedResult<()> {
        display::action("Cleaning up testbed");

        // Kill all tmux servers and delete the nodes dbs. Optionally clear logs.
        let mut command = vec!["(tmux kill-server || true)".into()];
        for path in self.protocol_commands.db_directories() {
            command.push(format!("(rm -rf {} || true)", path.display()));
        }
        if cleanup {
            command.push("(rm -rf ~/*log* || true)".into());
        }
        let command = command.join(" ; ");

        // Execute the deletion on all machines.
        let active = self.instances.iter().filter(|x| x.is_active()).cloned();
        let context = CommandContext::default();
        self.ssh_manager.execute(active, command, context).await?;

        display::done();
        Ok(())
    }

    /// Deploy the nodes.
    pub async fn run_nodes(&self, parameters: &BenchmarkParameters<T>) -> TestbedResult<()> {
        display::action("Deploying validators");

        // Select the instances to run.
        let (_, nodes, _) = self.select_instances(parameters)?;

        // Boot one node per instance.
        self.boot_nodes(nodes, parameters).await?;

        display::done();
        Ok(())
    }

    /// Deploy the load generators.
    pub async fn run_clients(&self, parameters: &BenchmarkParameters<T>) -> TestbedResult<()> {
        display::action("Setting up load generators");

        // Select the instances to run.
        let (clients, _, _) = self.select_instances(parameters)?;

        // Deploy the load generators.
        let targets = self
            .protocol_commands
            .client_command(clients.clone(), parameters);

        let repo = self.settings.repository_name();
        let context = CommandContext::new()
            .run_background("client".into())
            .with_log_file("~/client.log".into())
            .with_execute_from_path(repo.into());
        self.ssh_manager
            .execute_per_instance(targets, context)
            .await?;

        // Wait until all load generators are reachable.
        let commands = self.protocol_commands.clients_metrics_command(clients);
        self.ssh_manager.wait_for_success(commands).await;

        display::done();
        Ok(())
    }

    /// Collect metrics from the load generators.
    pub async fn run(
        &self,
        parameters: &BenchmarkParameters<T>,
    ) -> TestbedResult<MeasurementsCollection<T>> {
        display::action(format!(
            "Scraping metrics (at least {}s)",
            parameters.duration.as_secs()
        ));

        // Select the instances to run.
        let (clients, nodes, _) = self.select_instances(parameters)?;

        // Regularly scrape the client
        let mut metrics_commands = self.protocol_commands.clients_metrics_command(clients);

        // TODO: Remove this when narwhal client latency metrics are available.
        // We will be getting latency metrics directly from narwhal nodes instead from the nw client
        metrics_commands.append(&mut self.protocol_commands.nodes_metrics_command(nodes.clone()));

        let mut aggregator = MeasurementsCollection::new(&self.settings, parameters.clone());
        let mut metrics_interval = time::interval(self.scrape_interval);
        metrics_interval.tick().await; // The first tick returns immediately.

        let faults_type = parameters.faults.clone();
        let mut faults_schedule = CrashRecoverySchedule::new(faults_type, nodes.clone());
        let mut faults_interval = time::interval(self.crash_interval);
        faults_interval.tick().await; // The first tick returns immediately.

        let start = Instant::now();
        loop {
            tokio::select! {
                // Scrape metrics.
                now = metrics_interval.tick() => {
                    let elapsed = now.duration_since(start).as_secs_f64().ceil() as u64;
                    display::status(format!("{elapsed}s"));

                    let stdio = self
                        .ssh_manager
                        .execute_per_instance(metrics_commands.clone(), CommandContext::default())
                        .await?;
                    for (i, (stdout, _stderr)) in stdio.iter().enumerate() {
                        let measurement = Measurement::from_prometheus::<P>(stdout);
                        aggregator.add(i, measurement);
                    }

                    if elapsed > parameters.duration .as_secs() {
                        break;
                    }
                },

                // Kill and recover nodes according to the input schedule.
                _ = faults_interval.tick() => {
                    let  action = faults_schedule.update();
                    if !action.kill.is_empty() {
                        self.ssh_manager.kill(action.kill.clone(), "node").await?;
                    }
                    if !action.boot.is_empty() {
                        self.boot_nodes(action.boot.clone(), parameters).await?;
                    }
                    if !action.kill.is_empty() || !action.boot.is_empty() {
                        display::newline();
                        display::config("Testbed update", action);
                    }
                }
            }
        }

        let results_directory = &self.settings.results_dir;
        let commit = &self.settings.repository.commit;
        let path: PathBuf = [results_directory, &format!("results-{commit}").into()]
            .iter()
            .collect();
        fs::create_dir_all(&path).expect("Failed to create log directory");
        aggregator.save(path);

        display::done();
        Ok(aggregator)
    }

    /// Download the log files from the nodes and clients.
    pub async fn download_logs(
        &self,
        parameters: &BenchmarkParameters<T>,
    ) -> TestbedResult<LogsAnalyzer> {
        // Select the instances to run.
        let (clients, nodes, _) = self.select_instances(parameters)?;

        // Create a log sub-directory for this run.
        let commit = &self.settings.repository.commit;
        let path: PathBuf = [
            &self.settings.logs_dir,
            &format!("logs-{commit}").into(),
            &format!("logs-{parameters:?}").into(),
        ]
        .iter()
        .collect();
        fs::create_dir_all(&path).expect("Failed to create log directory");

        // NOTE: Our ssh library does not seem to be able to transfers files in parallel reliably.
        let mut log_parsers = Vec::new();

        // Download the clients log files.
        display::action("Downloading clients logs");
        for (i, instance) in clients.iter().enumerate() {
            display::status(format!("{}/{}", i + 1, clients.len()));

            let connection = self.ssh_manager.connect(instance.ssh_address()).await?;
            let client_log_content = connection.download("client.log").await?;

            let client_log_file = [path.clone(), format!("client-{i}.log").into()]
                .iter()
                .collect::<PathBuf>();
            fs::write(&client_log_file, client_log_content.as_bytes())
                .expect("Cannot write log file");

            let mut log_parser = LogsAnalyzer::default();
            log_parser.set_client_errors(&client_log_content);
            log_parsers.push(log_parser)
        }
        display::done();

        display::action("Downloading nodes logs");
        for (i, instance) in nodes.iter().enumerate() {
            display::status(format!("{}/{}", i + 1, nodes.len()));

            let connection = self.ssh_manager.connect(instance.ssh_address()).await?;
            let node_log_content = connection.download("node.log").await?;

            let node_log_file = [path.clone(), format!("node-{i}.log").into()]
                .iter()
                .collect::<PathBuf>();
            fs::write(&node_log_file, node_log_content.as_bytes()).expect("Cannot write log file");

            let mut log_parser = LogsAnalyzer::default();
            log_parser.set_node_errors(&node_log_content);
            log_parsers.push(log_parser)
        }
        display::done();

        Ok(LogsAnalyzer::aggregate(log_parsers))
    }

    /// Run all the benchmarks specified by the benchmark generator.
    pub async fn run_benchmarks(
        &mut self,
        mut generator: BenchmarkParametersGenerator<T>,
    ) -> TestbedResult<()> {
        display::header("Preparing testbed");
        display::config("Commit", format!("'{}'", &self.settings.repository.commit));
        display::newline();

        // Cleanup the testbed (in case the previous run was not completed).
        self.cleanup(true).await?;

        // Update the software on all instances.
        if !self.skip_testbed_update {
            self.install().await?;
            self.update().await?;
        }

        // Run all benchmarks.
        let mut i = 1;
        let mut latest_committee_size = 0;
        while let Some(parameters) = generator.next() {
            display::header(format!("Starting benchmark {i}"));
            display::config("Benchmark type", &parameters.benchmark_type);
            display::config("Parameters", &parameters);
            display::newline();

            // Cleanup the testbed (in case the previous run was not completed).
            self.cleanup(true).await?;
            // Start the instance monitoring tools.
            self.start_monitoring(&parameters).await?;

            // Configure all instances (if needed).
            if !self.skip_testbed_configuration && latest_committee_size != parameters.nodes {
                self.configure(&parameters).await?;
                latest_committee_size = parameters.nodes;
            }

            // Deploy the validators.
            self.run_nodes(&parameters).await?;

            // Deploy the load generators.
            self.run_clients(&parameters).await?;

            // Wait for the benchmark to terminate. Then save the results and print a summary.
            let aggregator = self.run(&parameters).await?;
            aggregator.display_summary();
            generator.register_result(aggregator);
            //drop(monitor);

            // Kill the nodes and clients (without deleting the log files).
            self.cleanup(false).await?;

            // Download the log files.
            if self.log_processing {
                let error_counter = self.download_logs(&parameters).await?;
                error_counter.print_summary();
            }

            i += 1;
        }

        display::header("Benchmark completed");
        Ok(())
    }
}
