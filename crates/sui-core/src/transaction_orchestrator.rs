// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
    FIXME
*/

use std::sync::Arc;
use std::time::Duration;

use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use crate::node_sync::NodeSyncState;
use crate::quorum_driver::{QuorumDriver, QuorumDriverHandler, QuorumDriverMetrics};
use prometheus::Registry;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    CertifiedTransaction, CertifiedTransactionEffects, ExecuteTransactionRequest,
    ExecuteTransactionResponse,
};
use tap::TapFallible;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, error, warn};

/// When requested to execute a transaction with WaitForEffectsCert,
/// TransactionOrchestrator attemps to execute this transaction locally
/// after it is finalized.
/// Some(true) => executed locally
/// Some(false) => not executed locally, due to timeout or other errors
/// None => did not attempt to execute locally (not with WaitForEffectsCert request type)
pub type IsTransactionExecutedLocally = Option<bool>;

// How long to wait for local execution (including parents) before a timeout
// is returned to client.
const LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct TransactiondOrchestrator<A> {
    _quorum_driver_handler: QuorumDriverHandler<A>,
    quorum_driver: Arc<QuorumDriver<A>>,
    node_sync_state: Arc<NodeSyncState<A>>,
    _local_executor_handle: JoinHandle<()>,
}

impl<A> TransactiondOrchestrator<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    pub fn new(
        validators: AuthorityAggregator<A>,
        node_sync_state: Arc<NodeSyncState<A>>,
        prometheus_registry: &Registry,
    ) -> Self {
        let quorum_driver_handler =
            QuorumDriverHandler::new(validators, QuorumDriverMetrics::new(prometheus_registry));
        let quorum_driver = quorum_driver_handler.clone_quorum_driver();
        let effects_receiver = quorum_driver_handler.subscribe();
        let state_clone = node_sync_state.clone();
        let _local_executor_handle = {
            tokio::task::spawn(async move {
                Self::loop_execute_finalized_tx_locally(state_clone, effects_receiver).await;
            })
        };
        Self {
            _quorum_driver_handler: quorum_driver_handler,
            quorum_driver,
            node_sync_state,
            _local_executor_handle,
        }
    }

    pub async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequest,
    ) -> SuiResult<(ExecuteTransactionResponse, IsTransactionExecutedLocally)> {
        // TODO check if tx is already executed on this node.
        // Note: since EffectsCert is not stored today, we need to gather that from validators
        // (and maybe store it for caching purposes)

        let execution_result = self
            .quorum_driver
            .execute_transaction(request)
            .await
            .tap_err(|err| debug!("Failed to execute transction via Quorum Driver: {:?}", err))?;

        match &execution_result {
            ExecuteTransactionResponse::EffectsCert(result) => {
                let (tx_cert, effects_cert) = result.as_ref();
                match Self::execute_finalized_tx_locally(
                    &self.node_sync_state,
                    tx_cert,
                    effects_cert,
                )
                .await
                {
                    Ok(_) => Ok((execution_result, Some(true))),
                    Err(_) => Ok((execution_result, Some(false))),
                }
            }
            _ => Ok((execution_result, None)),
        }
    }

    async fn execute_finalized_tx_locally(
        node_sync_state: &Arc<NodeSyncState<A>>,
        tx_cert: &CertifiedTransaction,
        effects_cert: &CertifiedTransactionEffects,
    ) -> SuiResult {
        // TODO: attempt a finalized tx at most once per request.
        // Every WaitForEffectsCert request will be attempted to execute twice,
        // one from the subscriber queue, one from the proactively execution
        // before returning results to clients. This is not insanely bad because
        // 1. it's possible that one attempt finishes before the other, so there's
        //      zero extra work
        // 2. an up-to-date fullnode should have minimal overhead to sync parents
        //      (for one extra time)
        // 3. the tx will be executed at most once per lock guard.
        let tx_digest = tx_cert.digest();
        if node_sync_state.is_tx_finalized_and_executed_locally(tx_digest)? {
            return Ok(());
        }
        match timeout(
            LOCAL_EXECUTION_TIMEOUT,
            node_sync_state.execute_finalized_transaction_for_orchestrator(tx_cert, effects_cert),
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
        node_sync_state: Arc<NodeSyncState<A>>,
        mut effects_receiver: Receiver<(CertifiedTransaction, CertifiedTransactionEffects)>,
    ) {
        loop {
            match effects_receiver.recv().await {
                Ok((tx_cert, effects_cert)) => {
                    Self::execute_finalized_tx_locally(&node_sync_state, &tx_cert, &effects_cert)
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
}
