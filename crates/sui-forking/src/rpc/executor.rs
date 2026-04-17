// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Adapter that exposes the forked-network's Simulacrum through the
//! `sui_types::transaction_executor::TransactionExecutor` trait so that the
//! `TransactionExecutionService` gRPC endpoints served by `sui-rpc-api` can
//! drive it.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;

use sui_types::error::{SuiError, SuiErrorKind};
use sui_types::transaction::TransactionData;
use sui_types::transaction_driver_types::{
    EffectsFinalityInfo, ExecuteTransactionRequestV3, ExecuteTransactionResponseV3,
    FinalizedEffects, TransactionSubmissionError,
};
use sui_types::transaction_executor::{
    SimulateTransactionResult, TransactionChecks, TransactionExecutor,
};

use crate::context::Context;
use crate::execution::execute_transaction;

/// `TransactionExecutor` implementation that runs transactions against the
/// forked network's Simulacrum. Signatures on inbound requests are discarded
/// and every transaction is executed under impersonation — the forked network
/// does not have access to the original account keys.
pub(crate) struct ForkedTransactionExecutor {
    context: Arc<Context>,
}

impl ForkedTransactionExecutor {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl TransactionExecutor for ForkedTransactionExecutor {
    async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequestV3,
        _client_addr: Option<SocketAddr>,
    ) -> Result<ExecuteTransactionResponseV3, TransactionSubmissionError> {
        let tx_data: TransactionData = request.transaction.data().transaction_data().clone();

        let result = execute_transaction(&self.context, tx_data)
            .await
            .map_err(|e| {
                TransactionSubmissionError::TransactionDriverInternalError(SuiError::from(
                    format!("forked execution failed: {e}"),
                ))
            })?;

        Ok(ExecuteTransactionResponseV3 {
            effects: FinalizedEffects {
                effects: result.effects,
                // The forked network is single-node, so nothing is "finalized"
                // in the quorum sense. The gRPC layer discards this field, but
                // we fill it in to satisfy the type.
                finality_info: EffectsFinalityInfo::QuorumExecuted(0),
            },
            // Events / input / output objects require threading the
            // `InnerTemporaryStore` out of Simulacrum, which `execute_transaction`
            // currently drops. Left for a follow-up.
            events: None,
            input_objects: None,
            output_objects: None,
            auxiliary_data: None,
        })
    }

    fn simulate_transaction(
        &self,
        _transaction: TransactionData,
        _checks: TransactionChecks,
        _allow_mock_gas_coin: bool,
    ) -> Result<SimulateTransactionResult, SuiError> {
        Err(SuiErrorKind::Unknown(
            "simulate_transaction is not supported by the forked network yet".to_string(),
        )
        .into())
    }
}
