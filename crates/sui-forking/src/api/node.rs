// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Lifecycle API for running `sui-forking` in-process.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use url::Url;

use crate::api::client::ForkingClient;
use crate::api::config::ForkingNodeConfig;
use crate::api::error::StartError;

/// Handle for a running in-process `sui-forking` node.
#[derive(Debug)]
pub struct ForkingNode {
    client: ForkingClient,
    http_base_url: Url,
    rpc_address: SocketAddr,
    data_dir: PathBuf,
    shutdown_sender: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<anyhow::Result<()>>>,
}

impl ForkingNode {
    /// Start a new local forking node.
    ///
    /// This returns only after startup has completed and listeners are ready.
    ///
    /// # Errors
    ///
    /// Returns [`StartError`] when startup fails, readiness cannot be reached, or runtime task
    /// initialization crashes.
    pub async fn start(config: ForkingNodeConfig) -> Result<Self, StartError> {
        let data_dir = prepare_data_dir(config.data_dir)?;

        let host = config.host.to_string();
        let server_port = config.server_port;
        let rpc_port = config.rpc_port;
        let fullnode_url = config.fullnode_url.map(|url| url.to_string());
        let startup_seeds = config.startup_seeding.into_internal();
        let fork_network = config.network.to_internal();
        let checkpoint = config.checkpoint;

        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (ready_sender, ready_receiver) = oneshot::channel();

        let data_dir_for_task = data_dir.clone();
        let mut task = tokio::spawn(async move {
            crate::server::start_server_with_signals(
                startup_seeds,
                fork_network,
                checkpoint,
                fullnode_url,
                host,
                server_port,
                rpc_port,
                data_dir_for_task,
                crate::VERSION,
                Some(shutdown_receiver),
                Some(ready_sender),
            )
            .await
        });

        wait_for_ready(&mut task, ready_receiver).await?;

        let http_address = SocketAddr::new(config.host, server_port);
        let http_base_url = control_url(http_address)?;
        let rpc_address = SocketAddr::from(([127, 0, 0, 1], rpc_port));

        Ok(Self {
            client: ForkingClient::new(http_base_url.clone()),
            http_base_url,
            rpc_address,
            data_dir,
            shutdown_sender: Some(shutdown_sender),
            task: Some(task),
        })
    }

    /// Return a typed control client for this node.
    pub fn client(&self) -> ForkingClient {
        self.client.clone()
    }

    /// Return the control API base URL.
    pub fn http_base_url(&self) -> &Url {
        &self.http_base_url
    }

    /// Return the gRPC address.
    pub fn rpc_address(&self) -> SocketAddr {
        self.rpc_address
    }

    /// Return the data directory root used by this node.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Trigger graceful shutdown and wait for completion.
    ///
    /// # Errors
    ///
    /// Returns [`StartError`] if shutdown fails or runtime task crashes.
    pub async fn shutdown(mut self) -> Result<(), StartError> {
        if let Some(sender) = self.shutdown_sender.take() {
            let _ = sender.send(());
        }
        wait_for_task(self.task.take()).await
    }

    /// Wait until the node exits.
    ///
    /// # Errors
    ///
    /// Returns [`StartError`] if runtime task exits with failure.
    pub async fn wait(mut self) -> Result<(), StartError> {
        wait_for_task(self.task.take()).await
    }
}

impl Drop for ForkingNode {
    fn drop(&mut self) {
        if let Some(sender) = self.shutdown_sender.take() {
            let _ = sender.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn prepare_data_dir(configured: Option<PathBuf>) -> Result<PathBuf, StartError> {
    let path = match configured {
        Some(path) => path,
        None => mysten_common::tempdir()
            .map_err(|err| StartError::CreateTempDir {
                message: err.to_string(),
            })?
            .keep(),
    };

    std::fs::create_dir_all(&path).map_err(|source| StartError::CreateDataDir {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

fn control_url(address: SocketAddr) -> Result<Url, StartError> {
    Url::parse(&format!("http://{address}"))
        .map_err(|source| StartError::InvalidControlUrl { address, source })
}

async fn wait_for_ready(
    task: &mut JoinHandle<anyhow::Result<()>>,
    ready_receiver: oneshot::Receiver<()>,
) -> Result<(), StartError> {
    tokio::select! {
        ready = ready_receiver => {
            match ready {
                Ok(()) => Ok(()),
                Err(_) => Err(StartError::ExitedBeforeReady {
                    message: "runtime exited before readiness".to_string(),
                }),
            }
        }
        runtime_result = task => {
            let runtime_result = runtime_result.map_err(|source| StartError::Join { source })?;
            match runtime_result {
                Ok(()) => Err(StartError::ExitedBeforeReady {
                    message: "runtime exited before readiness".to_string(),
                }),
                Err(err) => Err(StartError::ExitedBeforeReady {
                    message: err.to_string(),
                }),
            }
        }
    }
}

async fn wait_for_task(task: Option<JoinHandle<anyhow::Result<()>>>) -> Result<(), StartError> {
    let Some(task) = task else {
        return Ok(());
    };
    let runtime_result = task.await.map_err(|source| StartError::Join { source })?;
    runtime_result.map_err(|err| StartError::Runtime {
        message: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::Notify;
    use tokio::time::{Duration, sleep};

    use super::*;

    fn fake_node(
        task: JoinHandle<anyhow::Result<()>>,
        shutdown_sender: oneshot::Sender<()>,
    ) -> ForkingNode {
        let base_url = Url::parse("http://127.0.0.1:9001").expect("url");
        ForkingNode {
            client: ForkingClient::new(base_url.clone()),
            http_base_url: base_url,
            rpc_address: SocketAddr::from(([127, 0, 0, 1], 9000)),
            data_dir: PathBuf::from("/tmp/forking"),
            shutdown_sender: Some(shutdown_sender),
            task: Some(task),
        }
    }

    #[tokio::test]
    async fn readiness_succeeds_when_signal_arrives() {
        let mut task = tokio::spawn(async move {
            std::future::pending::<()>().await;
            Ok(())
        });
        let (ready_sender, ready_receiver) = oneshot::channel();
        ready_sender.send(()).expect("ready signal");

        wait_for_ready(&mut task, ready_receiver)
            .await
            .expect("readiness succeeds");
        task.abort();
    }

    #[tokio::test]
    async fn readiness_fails_when_runtime_exits_first() {
        let mut task = tokio::spawn(async move { anyhow::bail!("startup failed") });
        let (_ready_sender, ready_receiver) = oneshot::channel::<()>();

        let err = wait_for_ready(&mut task, ready_receiver)
            .await
            .expect_err("runtime exits before readiness");
        assert!(matches!(err, StartError::ExitedBeforeReady { .. }));
    }

    #[tokio::test]
    async fn shutdown_sends_signal_and_waits() {
        let notified = Arc::new(Notify::new());
        let notified_clone = notified.clone();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = shutdown_receiver.await;
            notified_clone.notify_one();
            Ok(())
        });
        let node = fake_node(task, shutdown_sender);

        node.shutdown().await.expect("shutdown succeeds");
        notified.notified().await;
    }

    #[tokio::test]
    async fn drop_aborts_runtime_task() {
        let (shutdown_sender, _shutdown_receiver) = oneshot::channel::<()>();
        let task = tokio::spawn(async move { std::future::pending::<anyhow::Result<()>>().await });
        let abort_handle = task.abort_handle();
        let node = fake_node(task, shutdown_sender);

        drop(node);
        sleep(Duration::from_millis(10)).await;
        assert!(abort_handle.is_finished());
    }
}
