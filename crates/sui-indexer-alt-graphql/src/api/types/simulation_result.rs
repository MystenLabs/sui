// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use async_graphql::SimpleObject;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::transaction::TransactionData;

use crate::{error::RpcError, scope::Scope};

use super::{command_result::CommandResult, event::Event, transaction_effects::TransactionEffects};

/// The result of simulating a transaction, including the predicted effects, events, and any errors.
#[derive(Clone, SimpleObject)]
pub struct SimulationResult {
    /// The predicted effects of the transaction if it were executed.
    ///
    /// `None` if the simulation failed due to an error.
    pub effects: Option<TransactionEffects>,

    /// The events that would be emitted if the transaction were executed.
    ///
    /// `None` if the simulation failed or no events would be emitted.
    pub events: Option<Vec<Event>>,

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
        let effects = Some(TransactionEffects::from_simulation_response(
            scope.clone(),
            response.clone(),
            transaction_data.clone(),
        )?);

        // Parse events - break into clear steps
        let executed_transaction = response
            .transaction
            .as_ref()
            .context("No transaction in simulation response")?;

        let events_bcs = executed_transaction
            .events
            .as_ref()
            .and_then(|events| events.bcs.as_ref());

        let transaction_events = events_bcs
            .map(|bcs| bcs.deserialize())
            .transpose()
            .context("Failed to deserialize events BCS")?;

        let events = transaction_events.map(|events: sui_types::effects::TransactionEvents| {
            events
                .data
                .into_iter()
                .enumerate()
                .map(|(sequence, native_event)| Event {
                    scope: scope.clone(),
                    native: native_event,
                    transaction_digest: transaction_data.digest(),
                    sequence_number: sequence as u64,
                    timestamp_ms: 0, // No timestamp for simulation
                })
                .collect()
        });

        // Extract command results from the response
        let outputs = Some(
            response
                .command_outputs
                .into_iter()
                .map(|output| CommandResult::from_proto(output, scope.clone()))
                .collect(),
        );

        Ok(Self {
            effects,
            events,
            outputs,
            error: None,
        })
    }
}
