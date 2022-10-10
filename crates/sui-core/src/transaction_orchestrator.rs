// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
Transaction Orchestrator is a Node component that utilizes Quorum Driver to
submit transactions to validators for finality, and proactively executes
finalized transactions locally, with the help of Node Sync.
*/
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;

use crate::authority::AuthorityState;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use crate::node_sync::{NodeSyncHandle, SyncStatus};
use crate::quorum_driver::{QuorumDriver, QuorumDriverHandler, QuorumDriverMetrics};
use prometheus::Registry;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    CertifiedTransaction, CertifiedTransactionEffects, ExecuteTransactionRequest,
    ExecuteTransactionRequestType, ExecuteTransactionResponse, QuorumDriverRequest,
    QuorumDriverRequestType, QuorumDriverResponse,
};
use tap::TapFallible;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, warn};

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct TransactiondOrchestrator<A> {
    quorum_driver_handler: QuorumDriverHandler<A>,
    quorum_driver: Arc<QuorumDriver<A>>,
    node_sync_handle: NodeSyncHandle,
    validator_state: Arc<AuthorityState>,
    _local_executor_handle: JoinHandle<()>,
}

impl<A> TransactiondOrchestrator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        validators: Arc<AuthorityAggregator<A>>,
        validator_state: Arc<AuthorityState>,
        node_sync_handle: NodeSyncHandle,
        prometheus_registry: &Registry,
    ) -> Self {
        let quorum_driver_handler =
            QuorumDriverHandler::new(validators, QuorumDriverMetrics::new(prometheus_registry));
        let quorum_driver = quorum_driver_handler.clone_quorum_driver();
        let effects_receiver = quorum_driver_handler.subscribe();
        let state_clone = validator_state.clone();
        let handle_clone = node_sync_handle.clone();
        let _local_executor_handle = {
            tokio::task::spawn(async move {
                Self::loop_execute_finalized_tx_locally(
                    state_clone,
                    handle_clone,
                    effects_receiver,
                )
                .await;
            })
        };
        Self {
            quorum_driver_handler,
            quorum_driver,
            validator_state,
            node_sync_handle,
            _local_executor_handle,
        }
    }

    pub async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequest,
    ) -> SuiResult<ExecuteTransactionResponse> {
        // TODO check if tx is already executed on this node.
        // Note: since EffectsCert is not stored today, we need to gather that from validators
        // (and maybe store it for caching purposes)
        let wait_for_local_execution = matches!(
            request.request_type,
            ExecuteTransactionRequestType::WaitForLocalExecution
        );
        let transaction = request.transaction;
        let request_type = match request.request_type {
            ExecuteTransactionRequestType::ImmediateReturn => {
                QuorumDriverRequestType::ImmediateReturn
            }
            ExecuteTransactionRequestType::WaitForTxCert => QuorumDriverRequestType::WaitForTxCert,
            ExecuteTransactionRequestType::WaitForEffectsCert
            | ExecuteTransactionRequestType::WaitForLocalExecution => {
                QuorumDriverRequestType::WaitForEffectsCert
            }
        };
        let execution_result = self
            .quorum_driver
            .execute_transaction(QuorumDriverRequest {
                transaction,
                request_type,
            })
            .await
            .tap_err(|err| debug!("Failed to execute transction via Quorum Driver: {:?}", err))?;

        match execution_result {
            QuorumDriverResponse::ImmediateReturn => {
                Ok(ExecuteTransactionResponse::ImmediateReturn)
            }
            QuorumDriverResponse::TxCert(result) => {
                Ok(ExecuteTransactionResponse::TxCert(Box::new(*result)))
            }
            QuorumDriverResponse::EffectsCert(result) => {
                let (tx_cert, effects_cert) = *result;
                if !wait_for_local_execution {
                    return Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert,
                        effects_cert,
                        false,
                    ))));
                }
                match Self::execute_finalized_tx_locally_with_timeout(
                    &self.validator_state,
                    &self.node_sync_handle,
                    &tx_cert,
                    &effects_cert,
                )
                .await
                {
                    Ok(_) => Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert,
                        effects_cert,
                        true,
                    )))),
                    Err(_) => Ok(ExecuteTransactionResponse::EffectsCert(Box::new((
                        tx_cert,
                        effects_cert,
                        false,
                    )))),
                }
            }
        }
    }

    async fn execute_finalized_tx_locally_with_timeout(
        validator_state: &Arc<AuthorityState>,
        node_sync_handle: &NodeSyncHandle,
        tx_cert: &CertifiedTransaction,
        effects_cert: &CertifiedTransactionEffects,
    ) -> SuiResult {
        // TODO: attempt a finalized tx at most once per request.
        // Every WaitForLocalExecution request will be attempted to execute twice,
        // one from the subscriber queue, one from the proactive execution before
        // returning results to clients. This is not insanely bad because:
        // 1. it's possible that one attempt finishes before the other, so there's
        //      zero extra work except DB checks
        // 2. an up-to-date fullnode should have minimal overhead to sync parents
        //      (for one extra time)
        // 3. at the end of day, the tx will be executed at most once per lock guard.
        let tx_digest = tx_cert.digest();
        if validator_state.is_tx_already_executed(tx_digest)? {
            return Ok(());
        }
        match timeout(
            LOCAL_EXECUTION_TIMEOUT,
            Self::execute_impl(validator_state, node_sync_handle, tx_cert, effects_cert),
        )
        .await
        {
            Err(_elapsed) => {
                debug!(
                    ?tx_digest,
                    "Executing tx locally by orchestrator timed out within {:?}.",
                    LOCAL_EXECUTION_TIMEOUT
                );
                Err(SuiError::TimeoutError)
            }
            Ok(Err(err)) => {
                debug!(
                    ?tx_digest,
                    "Executing tx locally by orchestrator failed with error: {:?}", err
                );
                Err(SuiError::TransactionOrchestratorLocalExecutionError {
                    error: err.to_string(),
                })
            }
            Ok(Ok(_)) => Ok(()),
        }
    }

    async fn loop_execute_finalized_tx_locally(
        validator_state: Arc<AuthorityState>,
        node_sync_handle: NodeSyncHandle,
        mut effects_receiver: Receiver<(CertifiedTransaction, CertifiedTransactionEffects)>,
    ) {
        loop {
            match effects_receiver.recv().await {
                Ok((tx_cert, effects_cert)) => {
                    let _ = Self::execute_finalized_tx_locally_with_timeout(
                        &validator_state,
                        &node_sync_handle,
                        &tx_cert,
                        &effects_cert,
                    )
                    .await;
                }
                Err(RecvError::Closed) => {
                    error!("Sender of effects subscriber queue has been dropped!");
                    return;
                }
                Err(RecvError::Lagged(skipped_count)) => {
                    warn!("Skipped {skipped_count} transasctions in effects subscriber queue.");
                }
            }
        }
    }

    pub fn quorum_driver(&self) -> &Arc<QuorumDriver<A>> {
        &self.quorum_driver
    }

    pub fn subscribe_to_effects_queue(
        &self,
    ) -> Receiver<(CertifiedTransaction, CertifiedTransactionEffects)> {
        self.quorum_driver_handler.subscribe()
    }

    /// Execute a finalized transaction locally.
    /// Firstly it tries to execute it optimistically. If there are missing
    /// dependencies, it then leverages Node Sync to process the parents.
    async fn execute_impl(
        state: &Arc<AuthorityState>,
        node_sync_handle: &NodeSyncHandle,
        tx_cert: &CertifiedTransaction,
        effects_cert: &CertifiedTransactionEffects,
    ) -> SuiResult {
        let tx_digest = tx_cert.digest();
        let res = state
            .handle_certificate_with_effects(tx_cert, effects_cert)
            .await;
        match res {
            Ok(_) => {
                debug!(
                    ?tx_digest,
                    "Orchestrator optimistically executed transaction successfully."
                );
                Ok(())
            }
            Err(SuiError::ObjectNotFound { .. }) | Err(SuiError::ObjectErrors { .. }) => {
                debug!(?tx_digest, "Orchestrator failed to executue transaction optimistically due to missing parents");

                match node_sync_handle
                    .handle_parents_request(
                        state.committee.load().epoch,
                        std::iter::once(*tx_digest),
                    )
                    .await?
                    .next()
                    .await
                    // Safe to unwrap because `handle_execution_request` wraps futures one by one
                    .unwrap()?
                {
                    SyncStatus::CertExecuted => {
                        debug!(
                            ?tx_digest,
                            "Orchestrator executed transaction via Node Sync."
                        );
                    }
                    SyncStatus::NotFinal => {
                        // This shall not happen
                        error!(
                            ?tx_digest,
                            "Orchestrator failed to execute finalized transaction via Node Sync"
                        );
                    }
                };
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
