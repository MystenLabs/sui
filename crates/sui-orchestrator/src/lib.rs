use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io::{stdout, Read},
    path::PathBuf,
    time::Duration,
};

use benchmark::BenchmarkParameters;
use client::Instance;
use crossterm::{
    cursor::MoveToColumn,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor, Stylize},
};
use futures::future::try_join_all;
use logs::LogsParser;
use measurement::MeasurementsCollection;
use settings::Settings;
use ssh::{SshCommand, SshConnectionManager};
use tokio::time::{self, sleep, Instant};

use crate::{
    benchmark::BenchmarkParametersGenerator, config::Config, error::TestbedError,
    error::TestbedResult,
};

pub mod benchmark;
pub mod client;
mod config;
mod error;
mod logs;
mod measurement;
pub mod plot;
pub mod settings;
pub mod ssh;
pub mod testbed;

pub struct Orchestrator {
    /// The testbed's settings.
    settings: Settings,
    /// The state of the testbed (reflecting accurately the state of the machines).
    instances: Vec<Instance>,
    /// Handle ssh connections to instances.
    ssh_manager: SshConnectionManager,
    skip_testbed_update: bool,
    skip_testbed_reconfiguration: bool,
    skip_logs_analysis: bool,
}

impl Orchestrator {
    const CLIENT_METRIC_PORT: u16 = 8081;
    const SCRAPE_INTERVAL: Duration = Duration::from_secs(30);

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
            skip_testbed_reconfiguration: false,
            skip_logs_analysis: false,
        }
    }

    pub fn skip_testbed_updates(mut self, skip_testbed_update: bool) -> Self {
        self.skip_testbed_update = skip_testbed_update;
        self
    }

    pub fn skip_testbed_reconfiguration(mut self, skip_testbed_reconfiguration: bool) -> Self {
        self.skip_testbed_reconfiguration = skip_testbed_reconfiguration;
        self
    }

    pub fn skip_logs_analysis(mut self, skip_logs_analysis: bool) -> Self {
        self.skip_logs_analysis = skip_logs_analysis;
        self
    }

    pub fn select_instances(
        &self,
        parameters: &BenchmarkParameters,
    ) -> TestbedResult<Vec<Instance>> {
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

        ensure!(
            instances.len() == parameters.nodes,
            TestbedError::InsufficientCapacity(format!("{}", parameters.nodes - instances.len()))
        );
        Ok(instances)
    }

    pub async fn wait_for_command<'a, I>(&self, instances: I, command_id: &str) -> TestbedResult<()>
    where
        I: Iterator<Item = &'a Instance> + Clone,
    {
        loop {
            sleep(Duration::from_secs(5)).await;

            let ssh_command = SshCommand::new(move |_| "(tmux ls || true)".into());
            let result = self
                .ssh_manager
                .execute(instances.clone(), ssh_command)
                .await?;

            if result
                .iter()
                .all(|(stdout, _)| !stdout.contains(command_id))
            {
                break;
            }
        }
        Ok(())
    }

    pub async fn wait_until_command<'a, I>(
        &self,
        instances: I,
        command_id: &str,
    ) -> TestbedResult<()>
    where
        I: Iterator<Item = &'a Instance> + Clone,
    {
        loop {
            sleep(Duration::from_secs(5)).await;

            let ssh_command = SshCommand::new(move |_| "(tmux ls || true)".into());
            let result = self
                .ssh_manager
                .execute(instances.clone(), ssh_command)
                .await?;

            if result.iter().all(|(stdout, _)| stdout.contains(command_id)) {
                break;
            }
        }
        Ok(())
    }

    pub async fn install(&self) -> TestbedResult<()> {
        crossterm::execute!(
            stdout(),
            Print("Installing dependencies on all machines...")
        )
        .unwrap();

        let url = self.settings.repository.url.clone();
        let name = self.settings.repository_name();
        let command = [
            "sudo apt-get update",
            "sudo apt-get -y upgrade",
            "sudo apt-get -y autoremove",
            // Disable "pending kernel upgrade" message.
            "sudo apt -y remove needrestart",
            // Install typical dependencies
            "sudo apt-get -y install curl git-all clang cmake gcc libssl-dev pkg-config libclang-dev",
            // The following dependencies prevent the error: [error: linker `cc` not found].
            "sudo apt-get -y install build-essential",
            // This dependency is missing from the Sui docs.
            "sudo apt-get -y install libpq-dev",
            // Install rust (non-interactive).
            "curl --proto \"=https\" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
            "source $HOME/.cargo/env",
            "rustup default stable",
            // Disable UFW.
            "sudo ufw disable",
            // Clone the repo.
            &format!("(git clone {url} || (cd {name} ; git pull))"),
        ]
        .join(" && ");

        let instances = self.instances.iter().filter(|x| x.is_active());
        let ssh_command = SshCommand::new(move |_| command.clone());
        self.ssh_manager.execute(instances, ssh_command).await?;

        println!(" [{}]", "Ok".green());
        Ok(())
    }

    pub async fn update(&self) -> TestbedResult<()> {
        let branch = self.settings.repository.branch.clone();
        crossterm::execute!(
            stdout(),
            Print(format!(
                "Updating {} instances (branch '{branch}')...",
                self.instances.len()
            ))
        )
        .unwrap();

        let command = [
            &format!("git fetch -f"),
            &format!("git checkout -f {branch}"),
            &format!("git pull -f"),
            "source $HOME/.cargo/env",
            &format!("cargo build --release"),
        ]
        .join(" && ");

        let instances = self.instances.iter().filter(|x| x.is_active());
        let id = "update";
        let repo_name = self.settings.repository_name();
        let ssh_command = SshCommand::new(move |_| command.clone())
            .run_background(id.into())
            .with_execute_from_path(repo_name.into());
        self.ssh_manager
            .execute(instances.clone(), ssh_command)
            .await?;

        self.wait_for_command(instances, "update").await?;

        println!(" [{}]", "Ok".green());
        Ok(())
    }

    pub async fn configure(&self, parameters: &BenchmarkParameters) -> TestbedResult<()> {
        crossterm::execute!(stdout(), Print("Generating configuration files...")).unwrap();

        // Select instances to configure.
        let instances = self.select_instances(parameters)?;

        // Generate the genesis configuration file and the keystore allowing access to gas objects.
        // TODO: There should be no need to generate these files locally; we can generate them
        // directly on the remote machines.
        let mut config = Config::new(&instances);
        config.print_files();

        let handles = instances
            .iter()
            .cloned()
            .map(|instance| {
                let repo_name = self.settings.repository_name();
                let config_files = config.files();
                let genesis_command = config.genesis_command();
                let ssh_manager = self.ssh_manager.clone();

                tokio::spawn(async move {
                    // Connect to the instance.
                    let connection = ssh_manager
                        .connect(instance.ssh_address())
                        .await?
                        .with_timeout(&Some(Duration::from_secs(180)));

                    // Upload all configuration files.
                    for source in config_files {
                        let destination = source.file_name().expect("Config file is directory");
                        let mut file = File::open(&source).expect("Cannot open config file");
                        let mut buf = Vec::new();
                        file.read_to_end(&mut buf).expect("Cannot read config file");
                        connection.upload(destination, &buf)?;
                    }

                    // Generate the genesis files.
                    let command = ["source $HOME/.cargo/env", &genesis_command].join(" && ");
                    connection
                        .execute_from_path(command, repo_name.clone())
                        .map(|_| ())
                        .map_err(TestbedError::from)
                })
            })
            .collect::<Vec<_>>();

        try_join_all(handles)
            .await
            .unwrap()
            .into_iter()
            .collect::<TestbedResult<_>>()?;

        println!(" [{}]", "Ok".green());
        Ok(())
    }

    pub async fn cleanup(&self, cleanup: bool) -> TestbedResult<()> {
        crossterm::execute!(stdout(), Print("Cleaning up testbed...")).unwrap();

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
        self.ssh_manager.execute(instances, ssh_command).await?;

        println!(" [{}]", "Ok".green());
        Ok(())
    }

    pub async fn run_nodes(&self, parameters: &BenchmarkParameters) -> TestbedResult<()> {
        crossterm::execute!(stdout(), Print("Deploying validators...")).unwrap();

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
            .execute(instances.iter(), ssh_command)
            .await?;

        println!(" [{}]", "Ok".green());
        Ok(())
    }

    pub async fn run_clients(&self, parameters: &BenchmarkParameters) -> TestbedResult<()> {
        crossterm::execute!(stdout(), Print("Setting up load generators...")).unwrap();

        // Select the instances to run.
        let instances = self.select_instances(parameters)?;

        // For the nodes to boot.
        self.wait_until_command(instances.iter(), "node").await?;

        // Deploy the load generators.
        let committee_size = instances.len();
        let load_share = parameters.load.clone() / committee_size;
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
                &format!("--num-workers 100 --target-qps {load_share}"),
                "--shared-counter 0 --in-flight-ratio 50 --transfer-object 100",
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
            .execute(instances.iter(), ssh_command)
            .await?;

        println!(" [{}]", "Ok".green());
        Ok(())
    }

    pub async fn collect_metrics(
        &self,
        parameters: &BenchmarkParameters,
    ) -> TestbedResult<MeasurementsCollection> {
        crossterm::execute!(
            stdout(),
            Print(format!(
                "Scrape metrics for {}s...\n",
                parameters.duration.as_secs()
            ))
        )
        .unwrap();

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
                .execute(instances.iter(), ssh_command.clone())
                .await
            {
                Ok(stdio) => {
                    crossterm::execute!(
                        stdout(),
                        MoveToColumn(0),
                        Print(format!(
                            "[{:?}] Scraping metrics...",
                            now.duration_since(start)
                        ))
                    )
                    .unwrap();
                    for (i, (stdout, _stderr)) in stdio.iter().enumerate() {
                        aggregator.add(i, stdout);
                    }
                }
                Err(e) => crossterm::execute!(
                    stdout(),
                    SetAttribute(Attribute::Bold),
                    MoveToColumn(0),
                    Print(format!("Failed to scrape metrics: {e}")),
                    SetAttribute(Attribute::NormalIntensity),
                )
                .unwrap(),
            }
            if aggregator.benchmark_duration() >= parameters.duration {
                break;
            }
        }
        aggregator.save(&self.settings.results_directory);

        println!();
        Ok(aggregator)
    }

    pub async fn download_logs(
        &self,
        parameters: &BenchmarkParameters,
    ) -> TestbedResult<LogsParser> {
        crossterm::execute!(stdout(), Print("Downloading logs...")).unwrap();

        let instances = self.select_instances(parameters)?;

        let handles = instances
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, instance)| {
                let ssh_manager = self.ssh_manager.clone();
                let log_directory = self.settings.logs_directory.clone();
                let parameters = parameters.clone();

                tokio::spawn(async move {
                    let mut error_counter = LogsParser::default();

                    // Connect to the instance.
                    let connection = ssh_manager.connect(instance.ssh_address()).await?;

                    // Create a log sub-directory for this run.
                    let path: PathBuf = [&log_directory, &format!("logs-{parameters:?}").into()]
                        .iter()
                        .collect();
                    fs::create_dir_all(&path).expect("Failed to create log directory");

                    // Download the node log files.
                    let node_log_content = connection.download("node.log")?;
                    error_counter.set_node_errors(&node_log_content);

                    let node_log_file = [path.clone(), format!("node-{i}.log").into()]
                        .iter()
                        .collect::<PathBuf>();
                    fs::write(&node_log_file, node_log_content.as_bytes())
                        .expect("Cannot write log file");

                    // Download the clients log files.
                    let client_log_content = connection.download("client.log")?;
                    error_counter.set_client_errors(&client_log_content);

                    let client_log_file = [path, format!("client-{i}.log").into()]
                        .iter()
                        .collect::<PathBuf>();
                    fs::write(&client_log_file, client_log_content.as_bytes())
                        .expect("Cannot write log file");

                    Ok(error_counter)
                })
            })
            .collect::<Vec<_>>();

        let error_counters: Vec<LogsParser> = try_join_all(handles)
            .await
            .unwrap()
            .into_iter()
            .collect::<TestbedResult<_>>()?;

        println!(" [{}]", "Ok".green());
        Ok(LogsParser::aggregate(error_counters))
    }

    pub async fn run_benchmarks(
        &mut self,
        mut generator: BenchmarkParametersGenerator,
    ) -> TestbedResult<()> {
        // Cleanup the testbed (in case the previous run was not completed).
        self.cleanup(true).await?;

        // Update the software on all instances.
        if !self.skip_testbed_update {
            self.install().await?;
            self.update().await?;
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
            self.cleanup(true).await?;

            // Configure all instances (if needed).
            if latest_comittee_status != (parameters.nodes, parameters.faults) {
                self.configure(&parameters).await?;
                latest_comittee_status = (parameters.nodes, parameters.faults);
            }

            // Deploy the validators.
            self.run_nodes(&parameters).await?;

            // Deploy the load generators.
            self.run_clients(&parameters).await?;

            // Wait for the benchmark to terminate. Then save the results and print a summary.
            let aggregator = self.collect_metrics(&parameters).await?;
            aggregator.print_summary(&parameters);
            generator.register_result(aggregator);

            // Kill the nodes and clients (without deleting the log files).
            self.cleanup(false).await?;

            // Download the log files.
            if !self.skip_logs_analysis {
                let error_counter = self.download_logs(&parameters).await?;
                error_counter.print_summary();
            }

            println!();
            i += 1;
        }
        Ok(())
    }
}
