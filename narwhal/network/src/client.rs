// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use anemo::{Network, PeerId, Request};
use async_trait::async_trait;
use crypto::{traits::KeyPair, NetworkKeyPair, NetworkPublicKey};
use mysten_common::sync::notify_once::NotifyOnce;
use parking_lot::RwLock;
use tokio::{select, time::sleep};
use tracing::error;
use types::{
    error::LocalClientError, FetchBatchesRequest, FetchBatchesResponse, PrimaryToWorker,
    WorkerOthersBatchMessage, WorkerOwnBatchMessage, WorkerSynchronizeMessage, WorkerToPrimary,
};

use crate::traits::{PrimaryToWorkerClient, WorkerToPrimaryClient};

/// NetworkClient provides the interface to send requests to other nodes, and call other components
/// directly if they live in the same process. It is used by both primary and worker(s).
///
/// Currently this only supports local direct calls, and it will be extended to support remote
/// network calls.
///
/// TODO: investigate splitting this into Primary and Worker specific clients.
#[derive(Clone)]
pub struct NetworkClient {
    inner: Arc<RwLock<Inner>>,
    shutdown_notify: Arc<NotifyOnce>,
}

struct Inner {
    primary_peer_id: PeerId,
    primary_network: Option<Network>,
    worker_network: BTreeMap<u32, Network>,
    worker_to_primary_handler: Option<Arc<dyn WorkerToPrimary>>,
    primary_to_worker_handler: BTreeMap<PeerId, Arc<dyn PrimaryToWorker>>,
    shutdown: bool,
}

impl NetworkClient {
    const GET_CLIENT_RETRIES: usize = 50;
    const GET_CLIENT_INTERVAL: Duration = Duration::from_millis(100);

    pub fn new(primary_peer_id: PeerId) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                primary_peer_id,
                primary_network: None,
                worker_network: BTreeMap::new(),
                worker_to_primary_handler: None,
                primary_to_worker_handler: BTreeMap::new(),
                shutdown: false,
            })),
            shutdown_notify: Arc::new(NotifyOnce::new()),
        }
    }

    pub fn new_from_keypair(primary_network_keypair: &NetworkKeyPair) -> Self {
        Self::new(PeerId(primary_network_keypair.public().0.into()))
    }

    pub fn new_with_empty_id() -> Self {
        // ED25519_PUBLIC_KEY_LENGTH is 32 bytes.
        Self::new(PeerId([0u8; 32]))
    }

    pub fn set_primary_network(&self, network: Network) {
        let mut inner = self.inner.write();
        if inner.primary_network.is_some() {
            error!("Primary network is already set");
        }
        inner.primary_network = Some(network);
    }

    pub fn set_worker_network(&self, worker_id: u32, network: Network) {
        let mut inner = self.inner.write();
        if inner.worker_network.insert(worker_id, network).is_some() {
            error!("Worker {} network is already set", worker_id);
        }
    }

    pub async fn get_primary_network(&self) -> Result<Network, anemo::rpc::Status> {
        for _ in 0..Self::GET_CLIENT_RETRIES {
            {
                let inner = self.inner.read();
                if inner.shutdown {
                    return Err(anemo::rpc::Status::internal("This node has shutdown"));
                }
                if let Some(network) = &inner.primary_network {
                    return Ok(network.clone());
                }
            }
            sleep(Self::GET_CLIENT_INTERVAL).await;
        }
        Err(anemo::rpc::Status::internal("Primary has not started"))
    }

    pub async fn get_worker_network(&self, worker_id: u32) -> Result<Network, anemo::rpc::Status> {
        for _ in 0..Self::GET_CLIENT_RETRIES {
            {
                let inner = self.inner.read();
                if inner.shutdown {
                    return Err(anemo::rpc::Status::internal("This node has shutdown"));
                }
                if let Some(network) = inner.worker_network.get(&worker_id) {
                    return Ok(network.clone());
                }
            }
            sleep(Self::GET_CLIENT_INTERVAL).await;
        }
        Err(anemo::rpc::Status::internal(format!(
            "The worker {} has not started",
            worker_id
        )))
    }

    pub fn set_worker_to_primary_local_handler(&self, handler: Arc<dyn WorkerToPrimary>) {
        let mut inner = self.inner.write();
        inner.worker_to_primary_handler = Some(handler);
    }

    pub fn set_primary_to_worker_local_handler(
        &self,
        worker_id: PeerId,
        handler: Arc<dyn PrimaryToWorker>,
    ) {
        let mut inner = self.inner.write();
        inner.primary_to_worker_handler.insert(worker_id, handler);
    }

    pub fn shutdown(&self) {
        let mut inner = self.inner.write();
        if inner.shutdown {
            return;
        }
        inner.worker_to_primary_handler = None;
        inner.primary_to_worker_handler = BTreeMap::new();
        inner.shutdown = true;
        let _ = self.shutdown_notify.notify();
    }

    async fn get_primary_to_worker_handler(
        &self,
        peer_id: PeerId,
    ) -> Result<Arc<dyn PrimaryToWorker>, LocalClientError> {
        for _ in 0..Self::GET_CLIENT_RETRIES {
            {
                let inner = self.inner.read();
                if inner.shutdown {
                    return Err(LocalClientError::ShuttingDown);
                }
                if let Some(handler) = inner.primary_to_worker_handler.get(&peer_id) {
                    return Ok(handler.clone());
                }
            }
            sleep(Self::GET_CLIENT_INTERVAL).await;
        }
        Err(LocalClientError::WorkerNotStarted(peer_id))
    }

    async fn get_worker_to_primary_handler(
        &self,
    ) -> Result<Arc<dyn WorkerToPrimary>, LocalClientError> {
        for _ in 0..Self::GET_CLIENT_RETRIES {
            {
                let inner = self.inner.read();
                if inner.shutdown {
                    return Err(LocalClientError::ShuttingDown);
                }
                if let Some(handler) = &inner.worker_to_primary_handler {
                    return Ok(handler.clone());
                }
            }
            sleep(Self::GET_CLIENT_INTERVAL).await;
        }
        Err(LocalClientError::PrimaryNotStarted(
            self.inner.read().primary_peer_id,
        ))
    }
}

// TODO: extract common logic for cancelling on shutdown.

#[async_trait]
impl PrimaryToWorkerClient for NetworkClient {
    async fn synchronize(
        &self,
        worker_name: NetworkPublicKey,
        request: WorkerSynchronizeMessage,
    ) -> Result<(), LocalClientError> {
        let c = self
            .get_primary_to_worker_handler(PeerId(worker_name.0.into()))
            .await?;
        select! {
            resp = c.synchronize(Request::new(request)) => {
                resp.map_err(|e| LocalClientError::Internal(format!("{e:?}")))?;
                Ok(())
            },
            () = self.shutdown_notify.wait() => {
                Err(LocalClientError::ShuttingDown)
            },
        }
    }

    async fn fetch_batches(
        &self,
        worker_name: NetworkPublicKey,
        request: FetchBatchesRequest,
    ) -> Result<FetchBatchesResponse, LocalClientError> {
        let c = self
            .get_primary_to_worker_handler(PeerId(worker_name.0.into()))
            .await?;
        select! {
            resp = c.fetch_batches(Request::new(request)) => {
                Ok(resp.map_err(|e| LocalClientError::Internal(format!("{e:?}")))?.into_inner())
            },
            () = self.shutdown_notify.wait() => {
                Err(LocalClientError::ShuttingDown)
            },
        }
    }
}

#[async_trait]
impl WorkerToPrimaryClient for NetworkClient {
    async fn report_own_batch(
        &self,
        request: WorkerOwnBatchMessage,
    ) -> Result<(), LocalClientError> {
        let c = self.get_worker_to_primary_handler().await?;
        select! {
            resp = c.report_own_batch(Request::new(request)) => {
                resp.map_err(|e| LocalClientError::Internal(format!("{e:?}")))?;
                Ok(())
            },
            () = self.shutdown_notify.wait() => {
                Err(LocalClientError::ShuttingDown)
            },
        }
    }

    async fn report_others_batch(
        &self,
        request: WorkerOthersBatchMessage,
    ) -> Result<(), LocalClientError> {
        let c = self.get_worker_to_primary_handler().await?;
        select! {
            resp = c.report_others_batch(Request::new(request)) => {
                resp.map_err(|e| LocalClientError::Internal(format!("{e:?}")))?;
                Ok(())
            },
            () = self.shutdown_notify.wait() => {
                Err(LocalClientError::ShuttingDown)
            },
        }
    }
}
