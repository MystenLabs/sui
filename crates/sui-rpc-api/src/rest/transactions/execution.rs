// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ApiEndpoint, RouteHandler};
use crate::response::Bcs;
use crate::{Result, RpcService};
use axum::extract::{Query, State};
use axum::Json;
use sui_sdk_types::{
    BalanceChange, Object, Transaction, TransactionEffects, TransactionEvents,
};

pub struct SimulateTransaction;

impl ApiEndpoint<RpcService> for SimulateTransaction {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::POST
    }

    fn path(&self) -> &'static str {
        "/transactions/simulate"
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
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TransactionSimulationResponse {
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub balance_changes: Option<Vec<BalanceChange>>,
    pub input_objects: Option<Vec<Object>>,
    pub output_objects: Option<Vec<Object>>,
}

/// Query parameters for the simulate transaction endpoint
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SimulateTransactionQueryParameters {
    /// Request `BalanceChanges` be included in the Response.
    #[serde(default)]
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    pub balance_changes: bool,
    /// Request input `Object`s be included in the Response.
    #[serde(default)]
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    pub input_objects: bool,
    /// Request output `Object`s be included in the Response.
    #[serde(default)]
    #[serde(with = "serde_with::As::<serde_with::DisplayFromStr>")]
    pub output_objects: bool,
}
