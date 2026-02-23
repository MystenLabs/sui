// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use sui_forking::{ForkingClient, ForkingNetwork, ForkingNode, ForkingNodeConfig, StartupSeeding};
use tokio::process::{Child, Command};
use tokio::time::sleep;
use url::Url;

use crate::harness::logging;
use crate::harness::ports;
use crate::harness::source_network::SourceNetworkHarness;
use crate::harness::{OPERATION_TIMEOUT_SECS, TEST_TIMEOUT_SECS};

pub struct ForkingHarness {
    base_url: Url,
    rpc_address: SocketAddr,
    data_dir: PathBuf,
    runtime: Option<Runtime>,
}

enum Runtime {
    Programmatic(ForkingNode),
    Cli(CliProcess),
}

struct CliProcess {
    child: Child,
    log_path: PathBuf,
}

impl ForkingHarness {
    pub async fn start_programmatic(
        source: &SourceNetworkHarness,
        checkpoint: u64,
        startup_seeding: StartupSeeding,
        data_dir: PathBuf,
    ) -> Result<Self> {
        let mut ports = ports::allocate_ports(2).context("failed to allocate forking ports")?;
        let rpc_port = ports
            .pop()
            .ok_or_else(|| anyhow::anyhow!("missing allocated rpc port"))?;
        let server_port = ports
            .pop()
            .ok_or_else(|| anyhow::anyhow!("missing allocated server port"))?;

        let config = ForkingNodeConfig::builder()
            .network(ForkingNetwork::Custom(source.graphql_url().clone()))
            .fullnode_url(source.fullnode_url().clone())
            .checkpoint(checkpoint)
            .startup_seeding(startup_seeding)
            .host(IpAddr::V4(Ipv4Addr::LOCALHOST))
            .rpc_port(rpc_port)
            .server_port(server_port)
            .data_dir(data_dir.clone())
            .build()
            .context("failed to build forking node config")?;

        let node = ForkingNode::start(config)
            .await
            .context("failed to start forking node programmatically")?;

        Ok(Self {
            base_url: node.http_base_url().clone(),
            rpc_address: node.rpc_address(),
            data_dir: node.data_dir().to_path_buf(),
            runtime: Some(Runtime::Programmatic(node)),
        })
    }

    pub async fn start_cli(
        source: &SourceNetworkHarness,
        checkpoint: u64,
        startup_seeding: StartupSeeding,
        data_dir: PathBuf,
    ) -> Result<Self> {
        let mut ports = ports::allocate_ports(2).context("failed to allocate forking ports")?;
        let rpc_port = ports
            .pop()
            .ok_or_else(|| anyhow::anyhow!("missing allocated rpc port"))?;
        let server_port = ports
            .pop()
            .ok_or_else(|| anyhow::anyhow!("missing allocated server port"))?;

        let base_url = Url::parse(&format!("http://127.0.0.1:{server_port}"))
            .context("failed to build forking HTTP base URL")?;
        let rpc_address = SocketAddr::from((Ipv4Addr::LOCALHOST, rpc_port));

        let binary = binary_path()?;
        let log_dir = data_dir.join("logs");
        let (log_path, stdout, stderr) = logging::create_process_log_file(&log_dir, "forking")
            .with_context(|| {
                format!("failed to initialize log file under {}", log_dir.display())
            })?;

        let mut command = Command::new(&binary);
        command
            .arg("start")
            .arg("--rpc-port")
            .arg(rpc_port.to_string())
            .arg("--server-port")
            .arg(server_port.to_string())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--checkpoint")
            .arg(checkpoint.to_string())
            .arg("--network")
            .arg(source.graphql_url().as_str())
            .arg("--fullnode-url")
            .arg(source.fullnode_url().as_str())
            .arg("--data-dir")
            .arg(data_dir.as_os_str())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));

        append_startup_seeding_args(&mut command, &startup_seeding);

        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn '{}'", binary.to_string_lossy()))?;

        wait_for_http_readiness(&base_url, &mut child, &log_path).await?;

        Ok(Self {
            base_url,
            rpc_address,
            data_dir,
            runtime: Some(Runtime::Cli(CliProcess { child, log_path })),
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn rpc_address(&self) -> SocketAddr {
        self.rpc_address
    }

    pub fn grpc_endpoint(&self) -> String {
        format!("http://{}", self.rpc_address)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn client(&self) -> ForkingClient {
        ForkingClient::new(self.base_url.clone())
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let runtime = self
            .runtime
            .take()
            .ok_or_else(|| anyhow::anyhow!("forking runtime already shut down"))?;

        match runtime {
            Runtime::Programmatic(node) => {
                node.shutdown()
                    .await
                    .context("failed to shutdown forking node")?;
                Ok(())
            }
            Runtime::Cli(mut process) => {
                match process
                    .child
                    .try_wait()
                    .context("failed to inspect forking cli process")?
                {
                    Some(status) => {
                        if !status.success() {
                            let logs = logging::read_process_log(&process.log_path);
                            bail!("forking CLI process exited with status {status}\n{}", logs);
                        }
                        Ok(())
                    }
                    None => {
                        process
                            .child
                            .kill()
                            .await
                            .context("failed to terminate forking cli process")?;
                        let _ = process
                            .child
                            .wait()
                            .await
                            .context("failed waiting for forking cli process termination")?;
                        Ok(())
                    }
                }
            }
        }
    }
}

impl Drop for ForkingHarness {
    fn drop(&mut self) {
        if let Some(Runtime::Cli(process)) = &mut self.runtime {
            let _ = process.child.start_kill();
        }
    }
}

pub fn binary_path() -> Result<PathBuf> {
    if let Ok(binary) = std::env::var("CARGO_BIN_EXE_sui_forking") {
        return Ok(PathBuf::from(binary));
    }

    if let Ok(binary) = std::env::var("CARGO_BIN_EXE_sui-forking") {
        return Ok(PathBuf::from(binary));
    }

    let current_executable =
        std::env::current_exe().context("failed to resolve current test executable path")?;
    let Some(target_debug_dir) = current_executable
        .parent()
        .and_then(std::path::Path::parent)
    else {
        bail!(
            "failed to derive target/debug from current executable '{}'",
            current_executable.display()
        );
    };

    let candidate = target_debug_dir.join("sui-forking");
    if candidate.exists() {
        return Ok(candidate);
    }

    bail!(
        "neither CARGO_BIN_EXE_sui_forking nor CARGO_BIN_EXE_sui-forking is set, and fallback binary '{}' does not exist",
        candidate.display()
    )
}

fn append_startup_seeding_args(command: &mut Command, startup_seeding: &StartupSeeding) {
    match startup_seeding {
        StartupSeeding::None => {}
        StartupSeeding::Accounts(accounts) => {
            if !accounts.is_empty() {
                let accounts = accounts
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                command.arg("--accounts").arg(accounts);
            }
        }
        StartupSeeding::Objects(objects) => {
            if !objects.is_empty() {
                let objects = objects
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                command.arg("--objects").arg(objects);
            }
        }
    }
}

async fn wait_for_http_readiness(base_url: &Url, child: &mut Child, log_path: &Path) -> Result<()> {
    let health_url = base_url
        .join("health")
        .context("failed to construct health endpoint url")?;
    let http = reqwest::Client::new();

    let deadline = Instant::now() + Duration::from_secs(TEST_TIMEOUT_SECS);
    loop {
        if let Some(status) = child
            .try_wait()
            .context("failed while waiting for forking cli startup")?
        {
            let logs = logging::read_process_log(log_path);
            bail!(
                "forking CLI exited before readiness with status {status}\n{}",
                logs
            );
        }

        if let Ok(response) = http.get(health_url.clone()).send().await
            && response.status().is_success()
        {
            return Ok(());
        }

        if Instant::now() >= deadline {
            let logs = logging::read_process_log(log_path);
            bail!(
                "timed out waiting for forking CLI readiness after {}s ({}). Logs:\n{}",
                TEST_TIMEOUT_SECS,
                health_url,
                logs
            );
        }

        sleep(Duration::from_secs(1)).await;
    }
}

pub async fn wait_for_subscription_message<T>(
    future: impl std::future::Future<Output = Result<T, tonic::Status>>,
) -> Result<T> {
    let timeout = Duration::from_secs(OPERATION_TIMEOUT_SECS);
    tokio::time::timeout(timeout, future)
        .await
        .with_context(|| {
            format!(
                "subscription wait timed out after {}s",
                OPERATION_TIMEOUT_SECS
            )
        })?
        .context("subscription stream returned an error")
}
