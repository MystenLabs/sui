// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io::Read,
    path::PathBuf,
    time::Duration,
};

use tokio::time::{self, Instant};

use crate::{
    benchmark::{BenchmarkParameters, BenchmarkParametersGenerator},
    client::Instance,
    config::Config,
    display, ensure,
    error::{TestbedError, TestbedResult},
    logs::LogsAnalyzer,
    measurement::{Measurement, MeasurementsCollection},
    settings::Settings,
    ssh::{CommandStatus, SshCommand, SshConnectionManager},
};

/// An orchestrator to run benchmarks on a testbed.
pub struct Orchestrator {
    /// The testbed's settings.
    settings: Settings,
    /// The state of the testbed (reflecting accurately the state of the machines).
    instances: Vec<Instance>,
    /// Handle ssh connections to instances.
    ssh_manager: SshConnectionManager,
    /// Whether to skip testbed updates before running benchmarks.
    skip_testbed_update: bool,
    /// Whether to skip testbed configuration before running benchmarks.
    skip_testbed_configuration: bool,
    /// Whether to skip downloading and analyzing log files.
    skip_logs_processing: bool,
}

impl Orchestrator {
    /// The port where the client exposes prometheus metrics.
    const CLIENT_METRIC_PORT: u16 = 8081;
    /// The default interval between measurements collection.
    const SCRAPE_INTERVAL: Duration = Duration::from_secs(30);

    /// Make a new orchestrator.
    pub fn new(
        settings: Settings,
        instances: Vec<Instance>,
        ssh_manager: SshConnectionManager,
    ) -> Self {
        Self {
            settings,
            instances,
            ssh_manager,
            skip_testbed_update: false,
            skip_testbed_configuration: false,
            skip_logs_processing: false,
        }
    }

    /// Whether to skip testbed updates before running benchmarks.
    pub fn skip_testbed_updates(mut self, skip_testbed_update: bool) -> Self {
        self.skip_testbed_update = skip_testbed_update;
        self
    }

    /// Whether to skip testbed configuration before running benchmarks.
    pub fn skip_testbed_configuration(mut self, skip_testbed_configuration: bool) -> Self {
        self.skip_testbed_configuration = skip_testbed_configuration;
        self
    }

    /// Whether to skip downloading and analyzing log files.
    pub fn skip_logs_processing(mut self, skip_logs_analysis: bool) -> Self {
        self.skip_logs_processing = skip_logs_analysis;
        self
    }

    /// Select on which instances of the testbed to run the benchmarks.
    pub fn select_instances(
        &self,
        parameters: &BenchmarkParameters,
    ) -> TestbedResult<Vec<Instance>> {
        ensure!(
            self.instances.len() >= parameters.nodes,
            TestbedError::InsufficientCapacity(parameters.nodes - self.instances.len())
        );

        let mut instances_by_regions = HashMap::new();
        for instance in &self.instances {
            if instance.is_active() {
                instances_by_regions
                    .entry(&instance.region)
                    .or_insert_with(VecDeque::new)
                    .push_back(instance);
            }
        }

        let mut instances = Vec::new();
        for region in self.settings.regions.iter().cycle() {
            if instances.len() == parameters.nodes {
                break;
            }
            if let Some(regional_instances) = instances_by_regions.get_mut(region) {
                if let Some(instance) = regional_instances.pop_front() {
                    instances.push(instance.clone());
                }
            }
        }
        Ok(instances)
    }

    /// Install the codebase and its dependencies on the testbed.
    pub async fn install(&self) -> TestbedResult<()> {
        display::action("Installing dependencies on all machines");

        let url = self.settings.repository.url.clone();
        let command = [
            "sudo apt-get update",
            "sudo apt-get -y upgrade",
            "sudo apt-get -y autoremove",
            // Disable "pending kernel upgrade" message.
            "sudo apt-get -y remove needrestart",
            // The following dependencies prevent the error: [error: linker `cc` not found].
            "sudo apt-get -y install build-essential",
            // Install typical sui dependencies.
            "sudo apt-get -y install curl git-all clang cmake gcc libssl-dev pkg-config libclang-dev",
            // This dependency is missing from the Sui docs.
            "sudo apt-get -y install libpq-dev",
            // Install dependencies to compile 'plotter'.
            "sudo apt-get -y install libfontconfig libfontconfig1-dev",
            // Install rust (non-interactive).
            "curl --proto \"=https\" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
            "source $HOME/.cargo/env",
            "rustup default stable",
            // Disable UFW.
            "sudo ufw disable",
            // Install prometheus.
            "sudo apt-get -y install prometheus",
            "sudo chmod 777 -R /var/lib/prometheus/",
            // Clone the repo.
            &format!("(git clone {url} || true)"),
        ]
        .join(" && ");

        let instances = self.instances.iter().filter(|x| x.is_active());
        let ssh_command = SshCommand::new(move |_| command.clone());
        self.ssh_manager.execute(instances, &ssh_command).await?;

        display::done();
        Ok(())
    }

    /// Update all instances to use the version of the codebase specified in the setting file.
    pub async fn update(&self) -> TestbedResult<()> {
        display::action(format!("Updating all instances"));

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

        let instances = self.instances.iter().filter(|x| x.is_active());
        let id = "update";
        let repo_name = self.settings.repository_name();
        let ssh_command = SshCommand::new(move |_| command.clone())
            .run_background(id.into())
            .with_execute_from_path(repo_name.into());
        self.ssh_manager
            .execute(instances.clone(), &ssh_command)
            .await?;

        // Wait until the command finished running.
        self.ssh_manager
            .wait_for_command(instances, &ssh_command, CommandStatus::Terminated)
            .await?;

        display::done();
        Ok(())
    }

    /// Configure the instances with the appropriate configuration files.
    pub async fn configure(&self, parameters: &BenchmarkParameters) -> TestbedResult<()> {
        display::action("Configuring instances");

        // Select instances to configure.
        let instances = self.select_instances(parameters)?;

        // Generate the genesis configuration file and the keystore allowing access to gas objects.
        // TODO: There should be no need to generate these files locally; we can generate them
        // directly on the remote machines.
        let mut config = Config::new(&instances);
        config.print_files();

        // NOTE: Our ssh library does not seem to be able to transfers files in parallel reliably.
        for (i, instance) in instances.iter().enumerate() {
            display::status(format!("{}/{}", i + 1, instances.len()));

            // Connect to the instance.
            let connection = self
                .ssh_manager
                .connect(instance.ssh_address())
                .await?
                .with_timeout(&Some(Duration::from_secs(180)));

            // Upload all configuration files.
            for source in config.files() {
                let destination = source.file_name().expect("Config file is directory");
                let mut file = File::open(&source).expect("Cannot open config file");
                let mut buf = Vec::new();
                file.read_to_end(&mut buf).expect("Cannot read config file");
                connection.upload(destination, &buf)?;
            }

            // Generate the genesis files.
            let command = ["source $HOME/.cargo/env", &config.genesis_command()].join(" && ");
            let repo_name = self.settings.repository_name();
            connection.execute_from_path(command, repo_name)?;
        }

        display::done();
        Ok(())
    }

    /// Cleanup all instances and optionally delete their log files.
    pub async fn cleanup(&self, cleanup: bool) -> TestbedResult<()> {
        display::action("Cleaning up testbed");

        // Kill all tmux servers and delete the nodes dbs. Optionally clear logs.
        let mut command = vec![
            "(tmux kill-server || true)",
            "(rm -rf ~/.sui/sui_config/*_db || true)",
        ];
        if cleanup {
            command.push("(rm -rf ~/*log* || true)");
        }
        let command = command.join(" ; ");

        // Execute the deletion on all machines.
        let instances = self.instances.iter().filter(|x| x.is_active());
        let ssh_command = SshCommand::new(move |_| command.clone());
        self.ssh_manager.execute(instances, &ssh_command).await?;

        display::done();
        Ok(())
    }

    /// Deploy the nodes.
    pub async fn run_nodes(&self, parameters: &BenchmarkParameters) -> TestbedResult<()> {
        display::action("Deploying validators");

        // Select the instances to run.
        let instances = self.select_instances(parameters)?;

        // Deploy the committee.
        let listen_addresses = Config::new(&instances).listen_addresses;
        let command = move |i: usize| -> String {
            let path = format!("~/.sui/sui_config/validator-config-{i}.yaml");
            let address = listen_addresses[i].clone();
            let run = format!(
                "cargo run --release --bin sui-node -- --config-path {path} --listen-address {address}"
            );
            ["source $HOME/.cargo/env", &run].join(" && ")
        };

        let repo = self.settings.repository_name();
        let ssh_command = SshCommand::new(command)
            .run_background("node".into())
            .with_log_file("~/node.log".into())
            .with_execute_from_path(repo.into());
        self.ssh_manager
            .execute(instances.iter(), &ssh_command)
            .await?;

        // For the nodes to boot.
        self.ssh_manager
            .wait_for_command(instances.iter(), &ssh_command, CommandStatus::Running)
            .await?;

        display::done();
        Ok(())
    }

    /// Deploy the load generators.
    pub async fn run_clients(&self, parameters: &BenchmarkParameters) -> TestbedResult<()> {
        display::action("Setting up load generators");

        // Select the instances to run.
        let instances = self.select_instances(parameters)?;

        // Deploy the load generators.
        let committee_size = instances.len();
        let load_share = parameters.load / committee_size;
        let shared_counter = parameters.shared_objects_ratio;
        let transfer_objects = 100 - shared_counter;
        let command = move |i: usize| -> String {
            let gas_id = Config::gas_object_id_offsets(committee_size)[i].clone();
            let genesis = "~/.sui/sui_config/genesis.blob";
            let keystore = format!("~/{}", Config::GAS_KEYSTORE_FILE);

            let run = [
                "cargo run --release --bin stress --",
                "--local false --num-client-threads 100 --num-transfer-accounts 2 ",
                &format!("--genesis-blob-path {genesis} --keystore-path {keystore}"),
                &format!("--primary-gas-id {gas_id}"),
                "bench",
                &format!("--num-workers 100 --in-flight-ratio 50 --target-qps {load_share}"),
                &format!("--shared-counter {shared_counter} --transfer-object {transfer_objects}"),
                &format!("--client-metric-port {}", Self::CLIENT_METRIC_PORT),
            ]
            .join(" ");
            ["source $HOME/.cargo/env", &run].join(" && ")
        };

        let repo = self.settings.repository_name();
        let ssh_command = SshCommand::new(command)
            .run_background("client".into())
            .with_log_file("~/client.log".into())
            .with_execute_from_path(repo.into());
        self.ssh_manager
            .execute(instances.iter(), &ssh_command)
            .await?;

        display::done();
        Ok(())
    }

    /// Collect metrics from the load generators.
    pub async fn collect_metrics(
        &self,
        parameters: &BenchmarkParameters,
    ) -> TestbedResult<MeasurementsCollection> {
        display::action(format!(
            "Scraping metrics (at least {}s)",
            parameters.duration.as_secs()
        ));

        // Select the instances to run.
        let instances = self.select_instances(parameters)?;

        // Regularly scrape the client metrics.
        let command = format!("curl 127.0.0.1:{}/metrics", Self::CLIENT_METRIC_PORT);
        let ssh_command = SshCommand::new(move |_| command.clone());

        let mut aggregator = MeasurementsCollection::new(&self.settings, parameters.clone());
        let mut interval = time::interval(Self::SCRAPE_INTERVAL);
        interval.tick().await; // The first tick returns immediately.

        let start = Instant::now();
        loop {
            let now = interval.tick().await;
            match self
                .ssh_manager
                .execute(instances.iter(), &ssh_command)
                .await
            {
                Ok(stdio) => {
                    let elapsed = now.duration_since(start).as_secs_f64().ceil() as u64;
                    display::status(format!("{elapsed}s",));
                    for (i, (stdout, _stderr)) in stdio.iter().enumerate() {
                        let measurement = Measurement::from_prometheus(stdout);
                        aggregator.add(i, measurement);
                    }
                }
                Err(e) => display::warn(format!("Failed to scrape metrics: {e}")),
            }
            if aggregator.benchmark_duration() >= parameters.duration {
                break;
            }
        }

        let results_directory = &self.settings.results_directory;
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
        parameters: &BenchmarkParameters,
    ) -> TestbedResult<LogsAnalyzer> {
        display::action("Downloading logs");
        // Select the instances to run.
        let instances = self.select_instances(parameters)?;

        // NOTE: Our ssh library does not seem to be able to transfers files in parallel reliably.
        let mut log_parsers = Vec::new();
        for (i, instance) in instances.iter().enumerate() {
            display::status(format!("{}/{}", i + 1, instances.len()));
            let mut log_parser = LogsAnalyzer::default();

            // Connect to the instance.
            let connection = self.ssh_manager.connect(instance.ssh_address()).await?;

            // Create a log sub-directory for this run.
            let commit = &self.settings.repository.commit;
            let path: PathBuf = [
                &self.settings.logs_directory,
                &format!("logs-{commit}").into(),
                &format!("logs-{parameters:?}").into(),
            ]
            .iter()
            .collect();
            fs::create_dir_all(&path).expect("Failed to create log directory");

            // Download the node log files.
            let node_log_content = connection.download("node.log")?;
            log_parser.set_node_errors(&node_log_content);

            let node_log_file = [path.clone(), format!("node-{i}.log").into()]
                .iter()
                .collect::<PathBuf>();
            fs::write(&node_log_file, node_log_content.as_bytes()).expect("Cannot write log file");

            // Download the clients log files.
            let client_log_content = connection.download("client.log")?;
            log_parser.set_client_errors(&client_log_content);

            let client_log_file = [path, format!("client-{i}.log").into()]
                .iter()
                .collect::<PathBuf>();
            fs::write(&client_log_file, client_log_content.as_bytes())
                .expect("Cannot write log file");

            log_parsers.push(log_parser)
        }

        display::done();
        Ok(LogsAnalyzer::aggregate(log_parsers))
    }

    /// Run all the benchmarks specified by the benchmark generator.
    pub async fn run_benchmarks(
        &mut self,
        mut generator: BenchmarkParametersGenerator,
    ) -> TestbedResult<()> {
        display::header("Preparing testbed");
        display::config("Commit", format!("'{}'", &self.settings.repository.commit));

        // Cleanup the testbed (in case the previous run was not completed).
        self.cleanup(true).await?;

        // Update the software on all instances.
        if !self.skip_testbed_update {
            self.install().await?;
            self.update().await?;
        }

        // Run all benchmarks.
        let mut i = 1;
        let mut latest_comittee_size = 0;
        while let Some(parameters) = generator.next() {
            display::header(format!("Starting benchmark {i}"));
            display::config("Parameters", &parameters);

            // Cleanup the testbed (in case the previous run was not completed).
            self.cleanup(true).await?;

            // Configure all instances (if needed).
            if !self.skip_testbed_configuration && latest_comittee_size != parameters.nodes {
                self.configure(&parameters).await?;
                latest_comittee_size = parameters.nodes;
            }

            // Deploy the validators.
            self.run_nodes(&parameters).await?;

            // Deploy the load generators.
            self.run_clients(&parameters).await?;

            // Wait for the benchmark to terminate. Then save the results and print a summary.
            let aggregator = self.collect_metrics(&parameters).await?;
            aggregator.display_summary();
            generator.register_result(aggregator);

            // Kill the nodes and clients (without deleting the log files).
            self.cleanup(false).await?;

            // Download the log files.
            if !self.skip_logs_processing {
                let error_counter = self.download_logs(&parameters).await?;
                error_counter.print_summary();
            }

            i += 1;
        }

        display::header("Benchmark completed");
        Ok(())
    }
}
