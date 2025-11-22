// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use async_graphql::SimpleObject;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::transaction::TransactionData;

use crate::{error::RpcError, scope::Scope};

use super::{command_result::CommandResult, transaction_effects::TransactionEffects};

/// The result of simulating a transaction, including the predicted effects and any errors.
#[derive(Clone, SimpleObject)]
pub struct SimulationResult {
    /// The predicted effects of the transaction if it were executed.
    ///
    /// `None` if the simulation failed due to an error.
    pub effects: Option<TransactionEffects>,

    /// The intermediate outputs for each command of the transaction simulation, including contents of mutated references and return values.
    pub outputs: Option<Vec<CommandResult>>,

    /// Error message if the simulation failed.
    ///
    /// `None` if the simulation was successful.
    pub error: Option<String>,
}

impl SimulationResult {
    /// Create a SimulationResult from a gRPC SimulateTransactionResponse.
    pub(crate) fn from_simulation_response(
        scope: Scope,
        response: proto::SimulateTransactionResponse,
        transaction_data: TransactionData,
    ) -> Result<Self, RpcError> {
        let executed_transaction = response
            .transaction
            .as_ref()
            .context("SimulateTransactionResponse should have transaction")?;

        // Create scope with execution objects
        let scope = scope.with_executed_transaction(executed_transaction)?;

        let effects = TransactionEffects::from_executed_transaction(
            scope.clone(),
            executed_transaction,
            transaction_data.clone(),
            vec![], // No signatures for simulated transactions
        )?;

        // Extract command results from the response
        let outputs = Some(
            response
                .command_outputs
                .into_iter()
                .map(|output| CommandResult::from_proto(output, scope.clone()))
                .collect(),
        );

        Ok(Self {
            effects: Some(effects),
            outputs,
            error: None,
        })
    }
}
