// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::response::Bcs;
use crate::rest::openapi::{
    ApiEndpoint, OperationBuilder, RequestBodyBuilder, ResponseBuilder, RouteHandler,
};
use crate::types::ExecuteTransactionOptions;
use crate::types::ExecuteTransactionResponse;
use crate::{Result, RpcService};
use axum::extract::{Query, State};
use axum::Json;
use schemars::JsonSchema;
use std::net::SocketAddr;
use sui_sdk_types::types::{
    BalanceChange, Object, SignedTransaction, Transaction, TransactionEffects, TransactionEvents,
};

pub struct ExecuteTransaction;

impl ApiEndpoint<RpcService> for ExecuteTransaction {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::POST
    }

    fn path(&self) -> &'static str {
        "/transactions"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Transactions")
            .operation_id("ExecuteTransaction")
            .query_parameters::<ExecuteTransactionOptions>(generator)
            .request_body(RequestBodyBuilder::new().bcs_content().build())
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<ExecuteTransactionResponse>(generator)
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), execute_transaction)
    }
}

/// Execute Transaction REST endpoint.
///
/// Handles client transaction submission request by passing off the provided signed transaction to
/// an internal QuorumDriver which drives execution of the transaction with the current validator
/// set.
async fn execute_transaction(
    State(state): State<RpcService>,
    Query(options): Query<ExecuteTransactionOptions>,
    client_address: Option<axum::extract::ConnectInfo<SocketAddr>>,
    Bcs(transaction): Bcs<SignedTransaction>,
) -> Result<Json<ExecuteTransactionResponse>> {
    state
        .execute_transaction(transaction, client_address.map(|a| a.0), &options)
        .await
        .map(Json)
}

pub struct SimulateTransaction;

impl ApiEndpoint<RpcService> for SimulateTransaction {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::POST
    }

    fn path(&self) -> &'static str {
        "/transactions/simulate"
    }

    fn operation(
        &self,
        generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("Transactions")
            .operation_id("SimulateTransaction")
            .query_parameters::<SimulateTransactionQueryParameters>(generator)
            .request_body(RequestBodyBuilder::new().bcs_content().build())
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<TransactionSimulationResponse>(generator)
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<RpcService> {
        RouteHandler::new(self.method(), simulate_transaction)
    }
}

async fn simulate_transaction(
    State(state): State<RpcService>,
    Query(parameters): Query<SimulateTransactionQueryParameters>,
    //TODO allow accepting JSON as well as BCS
    Bcs(transaction): Bcs<Transaction>,
) -> Result<Json<TransactionSimulationResponse>> {
    state
        .simulate_transaction(&parameters, transaction)
        .map(Json)
}

/// Response type for the transaction simulation endpoint
#[derive(Debug, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct TransactionSimulationResponse {
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub balance_changes: Option<Vec<BalanceChange>>,
    pub input_objects: Option<Vec<Object>>,
    pub output_objects: Option<Vec<Object>>,
}

/// Query parameters for the simulate transaction endpoint
#[derive(Debug, Default, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct SimulateTransactionQueryParameters {
    /// Request `BalanceChanges` be included in the Response.
    #[serde(default)]
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    #[schemars(with = "bool")]
    pub balance_changes: bool,
    /// Request input `Object`s be included in the Response.
    #[serde(default)]
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    #[schemars(with = "bool")]
    pub input_objects: bool,
    /// Request output `Object`s be included in the Response.
    #[serde(default)]
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    #[schemars(with = "bool")]
    pub output_objects: bool,
}
