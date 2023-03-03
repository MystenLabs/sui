use std::{
    cmp::max,
    fs::{self, File},
    io::{stdout, Read},
    path::PathBuf,
    time::Duration,
};

use crossterm::{
    cursor::MoveToColumn,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor, Stylize},
};
use futures::future::try_join_all;
use prettytable::{format, row, Table};
use tokio::time::{self, sleep, Instant};

use crate::{
    ensure,
    orchestrator::{
        client::Client,
        error::{TestbedError, TestbedResult},
        settings::Settings,
        ssh::SshConnection,
    },
};

use super::{
    client::Instance,
    config::Config,
    metrics::MetricsCollector,
    parameters::BenchmarkParameters,
    ssh::{SshCommand, SshConnectionManager},
};

#[derive(Default)]
pub struct ErrorCounter {
    pub node_errors: usize,
    pub node_panic: bool,
    pub client_errors: usize,
    pub client_panic: bool,
}

impl ErrorCounter {
    pub fn set_node_errors(&mut self, log: &str) {
        self.node_errors = log.matches(" ERROR").count();
        self.node_panic = log.contains("panic");
    }

    pub fn set_client_errors(&mut self, log: &str) {
        self.client_errors = max(self.client_errors, log.matches(" ERROR").count());
        self.client_panic = log.contains("panic");
    }

    pub fn aggregate(counters: Vec<Self>) -> Self {
        let mut highest = Self::default();
        for counter in counters {
            if counter.node_panic {
                return counter;
            } else if counter.client_panic {
                return counter;
            } else if counter.client_errors > highest.client_errors {
                highest = counter;
            } else if counter.node_errors > highest.node_errors {
                highest = counter;
            }
        }
        highest
    }

    pub fn print_summary(&self) {
        if self.node_panic {
            crossterm::execute!(
                stdout(),
                SetForegroundColor(Color::Red),
                SetAttribute(Attribute::Bold),
                Print("\nNode(s) panicked!\n"),
                ResetColor
            )
            .unwrap();
        } else if self.client_panic {
            crossterm::execute!(
                stdout(),
                SetForegroundColor(Color::Red),
                SetAttribute(Attribute::Bold),
                Print("\nClient(s) panicked!\n"),
                ResetColor
            )
            .unwrap();
        } else if self.node_errors != 0 || self.client_errors != 0 {
            crossterm::execute!(
                stdout(),
                SetAttribute(Attribute::Bold),
                Print(format!(
                    "\nLogs contain errors (node: {}, client: {})\n",
                    self.node_errors, self.client_errors
                )),
            )
            .unwrap();
        }
    }
}

pub struct Testbed<C> {
    /// The testbed's settings.
    settings: Settings,
    /// The client interfacing with the cloud provider.
    client: C,
    /// The state of the testbed (reflecting accurately the state of the machines).
    instances: Vec<Instance>,
    /// Handle ssh connections to instances.
    ssh_manager: SshConnectionManager,
}

impl<C: Client> Testbed<C> {
    /// Create a new testbed instance with the specified settings and client.
    pub async fn new(settings: Settings, client: C) -> TestbedResult<Self> {
        let public_key = settings.load_ssh_public_key()?;
        client.register_ssh_public_key(public_key).await?;

        let instances = client.list_instances().await?;
        let private_key_file = settings.ssh_private_key_file.clone().into();
        let ssh_manager = SshConnectionManager::new(C::USERNAME.into(), private_key_file);
        Ok(Self {
            settings,
            client,
            instances,
            ssh_manager,
        })
    }

    /// Print the current state of the testbed.
    pub fn info(&self) {
        let sorted: Vec<(_, Vec<_>)> = self
            .settings
            .regions
            .iter()
            .map(|region| {
                (
                    region,
                    self.instances
                        .iter()
                        .filter(|instance| &instance.region == region)
                        .collect(),
                )
            })
            .collect();

        println!();
        println!("{} {}", "Client:".bold(), self.client);
        println!(
            "{} {} ({})",
            "Repo:".bold(),
            self.settings.repository.url,
            self.settings.repository.branch
        );

        let mut table = Table::new();
        let format = format::FormatBuilder::new()
            .separators(
                &[
                    format::LinePosition::Top,
                    format::LinePosition::Bottom,
                    format::LinePosition::Title,
                ],
                format::LineSeparator::new('-', '-', '-', '-'),
            )
            .padding(1, 1)
            .build();
        table.set_format(format);

        println!();
        table.set_titles(row![bH2->format!("Instances ({})",self.instances.len())]);
        for (i, (region, instances)) in sorted.iter().enumerate() {
            table.add_row(row![bH2->region.to_uppercase()]);
            let mut j = 0;
            for instance in instances {
                if (j + 1) % 5 == 0 {
                    table.add_row(row![]);
                }
                let private_key_file = self.settings.ssh_private_key_file.display();
                let username = C::USERNAME;
                let ip = instance.main_ip;
                let connect = format!("ssh -i {private_key_file} {username}@{ip}");
                if !instance.is_terminated() {
                    if instance.is_active() {
                        table.add_row(row![bFg->format!("{j}"), connect]);
                    } else {
                        table.add_row(row![bFr->format!("{j}"), connect]);
                    }
                    j += 1;
                }
            }
            if i != sorted.len() - 1 {
                table.add_row(row![]);
            }
        }
        table.printstd();
        println!();
    }

    /// Populate the testbed by creating the specified amount of instances per region. The total
    /// number of instances created is thus the specified amount x the number of regions.
    pub async fn populate(&mut self, quantity: usize) -> TestbedResult<()> {
        crossterm::execute!(
            stdout(),
            Print(format!(
                "Populating testbed with {quantity} instances per region..."
            ))
        )
        .unwrap();

        try_join_all(
            self.settings
                .regions
                .iter()
                .map(|region| (0..quantity).map(|_| self.client.create_instance(region.clone())))
                .flatten()
                .collect::<Vec<_>>(),
        )
        .await?;

        // Wait until the instances are booted.
        self.ready().await?;
        self.instances = self.client.list_instances().await?;
        Ok(())
    }

    /// Destroy all instances of the testbed.
    pub async fn destroy(&mut self) -> TestbedResult<()> {
        try_join_all(
            self.instances
                .drain(..)
                .map(|instance| self.client.delete_instance(instance))
                .collect::<Vec<_>>(),
        )
        .await
        .map_err(TestbedError::from)
        .map(|_| ())
    }

    /// Start the specified number of instances in each region. Returns an error if there are not
    /// enough available instances.
    pub async fn start(&mut self, quantity: usize) -> TestbedResult<()> {
        // Gather available instances.
        let mut available = Vec::new();
        let mut missing = Vec::new();

        for region in &self.settings.regions {
            let filtered: Vec<_> = self
                .instances
                .iter()
                .filter(|x| x.is_inactive() && &x.region == region)
                .take(quantity)
                .collect();
            if filtered.len() < quantity {
                missing.push((region.clone(), quantity - filtered.len()))
            } else {
                available.extend(filtered);
            }
        }

        ensure!(
            missing.is_empty(),
            TestbedError::InsufficientCapacity(format!("{missing:?}"))
        );

        // Start instances.
        self.client.start_instances(available.into_iter()).await?;

        // Wait until the instances are started.
        self.ready().await?;
        self.instances = self.client.list_instances().await?;
        Ok(())
    }

    /// Stop all instances of the testbed.
    pub async fn stop(&mut self) -> TestbedResult<()> {
        // Stop all instances.
        self.client.stop_instances(self.instances.iter()).await?;

        // Wait until the instances are stopped.
        loop {
            let instances = self.client.list_instances().await?;
            if instances.iter().all(|x| x.is_inactive()) {
                self.instances = instances;
                break;
            }
        }
        Ok(())
    }

    pub async fn ready(&self) -> TestbedResult<()> {
        let mut waiting = 0;
        loop {
            let duration = Duration::from_secs(5);
            sleep(duration).await;

            waiting += duration.as_secs();
            crossterm::execute!(
                stdout(),
                MoveToColumn(0),
                Print(format!("Waiting for machines to boot ({waiting}s)..."))
            )
            .unwrap();

            let instances = self.client.list_instances().await?;
            if try_join_all(instances.iter().map(|instance| {
                let private_key_file = self.settings.ssh_private_key_file.clone();
                SshConnection::new(instance.ssh_address(), C::USERNAME, private_key_file)
            }))
            .await
            .is_ok()
            {
                break;
            }
        }

        println!(" [{}]", "Ok".green());
        Ok(())
    }
}

impl<C> Testbed<C> {
    const CLIENT_METRIC_PORT: u16 = 8081;
    const SCRAPE_INTERVAL: Duration = Duration::from_secs(30);

    pub fn select_instances(
        &self,
        parameters: &BenchmarkParameters,
    ) -> TestbedResult<Vec<Instance>> {
        // TODO: Select an equal number of instances per region.
        let instances: Vec<_> = self
            .instances
            .iter()
            .filter(|x| x.is_active())
            .cloned()
            .take(parameters.nodes)
            .collect();

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
        let ssh_command = SshCommand::new(move |_| command.clone());
        self.ssh_manager
            .execute(self.instances.iter(), ssh_command)
            .await?;

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
    ) -> TestbedResult<MetricsCollector<usize>> {
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

        let mut aggregator = MetricsCollector::new(parameters.clone());
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
                        aggregator.collect(i, stdout);
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
    ) -> TestbedResult<ErrorCounter> {
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
                    let mut error_counter = ErrorCounter::default();

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

        let error_counters: Vec<ErrorCounter> = try_join_all(handles)
            .await
            .unwrap()
            .into_iter()
            .collect::<TestbedResult<_>>()?;

        println!(" [{}]", "Ok".green());
        Ok(ErrorCounter::aggregate(error_counters))
    }
}

#[cfg(test)]
mod test {
    use crate::orchestrator::{
        client::test_client::TestClient, settings::Settings, testbed::Testbed,
    };

    #[tokio::test]
    async fn populate() {
        let settings = Settings::new_for_test();
        let client = TestClient::default();
        let mut testbed = Testbed::new(settings, client).await.unwrap();

        testbed.populate(5).await.unwrap();

        assert_eq!(
            testbed.instances.len(),
            5 * testbed.settings.number_of_regions()
        );
        for (i, instance) in testbed.instances.iter().enumerate() {
            assert_eq!(i.to_string(), instance.id);
        }
    }

    #[tokio::test]
    async fn destroy() {
        let settings = Settings::new_for_test();
        let client = TestClient::default();
        let mut testbed = Testbed::new(settings, client).await.unwrap();

        testbed.destroy().await.unwrap();

        assert_eq!(testbed.instances.len(), 0);
    }

    #[tokio::test]
    async fn start() {
        let settings = Settings::new_for_test();
        let client = TestClient::default();
        let mut testbed = Testbed::new(settings, client).await.unwrap();
        testbed.populate(5).await.unwrap();

        let result = testbed.start(2).await;

        assert!(result.is_ok());
        for region in &testbed.settings.regions {
            let active = testbed
                .instances
                .iter()
                .filter(|x| x.is_active() && &x.region == region)
                .count();
            assert_eq!(active, 2);

            let inactive = testbed
                .instances
                .iter()
                .filter(|x| x.is_inactive() && &x.region == region)
                .count();
            assert_eq!(inactive, 3);
        }
    }

    #[tokio::test]
    async fn start_insufficient_capacity() {
        let settings = Settings::new_for_test();
        let client = TestClient::default();
        let mut testbed = Testbed::new(settings, client).await.unwrap();
        testbed.populate(1).await.unwrap();

        let result = testbed.start(2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn stop() {
        let settings = Settings::new_for_test();
        let client = TestClient::default();
        let mut testbed = Testbed::new(settings, client).await.unwrap();
        testbed.populate(5).await.unwrap();
        testbed.start(2).await.unwrap();

        testbed.stop().await.unwrap();

        assert!(testbed
            .instances
            .iter()
            .all(|x| x.power_status == "inactive"))
    }
}
