// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::openapi::{
    ApiEndpoint, OperationBuilder, RequestBodyBuilder, ResponseBuilder, RouteHandler,
};
use crate::response::Bcs;
use crate::{accept::AcceptFormat, response::ResponseContent};
use crate::{RestError, RestService, Result};
use axum::extract::{Query, State};
use schemars::JsonSchema;
use std::net::SocketAddr;
use std::sync::Arc;
use sui_sdk_types::types::framework::Coin;
use sui_sdk_types::types::{
    Address, BalanceChange, CheckpointSequenceNumber, Object, Owner, SignedTransaction,
    Transaction, TransactionEffects, TransactionEvents, ValidatorAggregatedSignature,
};
use sui_types::transaction_executor::{SimulateTransactionResult, TransactionExecutor};
use tap::Pipe;

pub struct ExecuteTransaction;

impl ApiEndpoint<RestService> for ExecuteTransaction {
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
            .query_parameters::<ExecuteTransactionQueryParameters>(generator)
            .request_body(RequestBodyBuilder::new().bcs_content().build())
            .response(
                200,
                ResponseBuilder::new()
                    .json_content::<TransactionExecutionResponse>(generator)
                    .bcs_content()
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), execute_transaction)
    }
}

/// Execute Transaction REST endpoint.
///
/// Handles client transaction submission request by passing off the provided signed transaction to
/// an internal QuorumDriver which drives execution of the transaction with the current validator
/// set.
///
/// A client can signal, using the `Accept` header, the response format as either JSON or BCS.
async fn execute_transaction(
    State(state): State<Option<Arc<dyn TransactionExecutor>>>,
    Query(parameters): Query<ExecuteTransactionQueryParameters>,
    client_address: Option<axum::extract::ConnectInfo<SocketAddr>>,
    accept: AcceptFormat,
    Bcs(transaction): Bcs<SignedTransaction>,
) -> Result<ResponseContent<TransactionExecutionResponse>> {
    let executor = state.ok_or_else(|| anyhow::anyhow!("No Transaction Executor"))?;
    let request = sui_types::quorum_driver_types::ExecuteTransactionRequestV3 {
        transaction: transaction.try_into()?,
        include_events: parameters.events,
        include_input_objects: parameters.input_objects || parameters.balance_changes,
        include_output_objects: parameters.output_objects || parameters.balance_changes,
        include_auxiliary_data: false,
    };

    let sui_types::quorum_driver_types::ExecuteTransactionResponseV3 {
        effects,
        events,
        input_objects,
        output_objects,
        auxiliary_data: _,
    } = executor
        .execute_transaction(request, client_address.map(|a| a.0))
        .await?;

    let (effects, finality) = {
        let sui_types::quorum_driver_types::FinalizedEffects {
            effects,
            finality_info,
        } = effects;
        let finality = match finality_info {
            sui_types::quorum_driver_types::EffectsFinalityInfo::Certified(sig) => {
                EffectsFinality::Certified {
                    signature: sig.into(),
                }
            }
            sui_types::quorum_driver_types::EffectsFinalityInfo::Checkpointed(
                _epoch,
                checkpoint,
            ) => EffectsFinality::Checkpointed { checkpoint },
        };

        (effects.try_into()?, finality)
    };

    let events = if parameters.events {
        events.map(TryInto::try_into).transpose()?
    } else {
        None
    };

    let input_objects = input_objects
        .map(|objects| {
            objects
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let output_objects = output_objects
        .map(|objects| {
            objects
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;

    let balance_changes = match (parameters.balance_changes, &input_objects, &output_objects) {
        (true, Some(input_objects), Some(output_objects)) => Some(derive_balance_changes(
            &effects,
            input_objects,
            output_objects,
        )),
        _ => None,
    };

    let input_objects = if parameters.input_objects {
        input_objects
    } else {
        None
    };

    let output_objects = if parameters.output_objects {
        output_objects
    } else {
        None
    };

    let response = TransactionExecutionResponse {
        effects,
        finality,
        events,
        balance_changes,
        input_objects,
        output_objects,
    };

    match accept {
        AcceptFormat::Json => ResponseContent::Json(response),
        AcceptFormat::Bcs => ResponseContent::Bcs(response),
    }
    .pipe(Ok)
}

/// Query parameters for the execute transaction endpoint
#[derive(Debug, Default, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct ExecuteTransactionQueryParameters {
    // TODO once transaction finality support is more fully implemented up and down the stack, add
    // back in this parameter, which will be mutally-exclusive with the other parameters. When
    // `true` will submit the txn and return a `202 Accepted` response with no payload.
    // effects: Option<bool>,
    /// Request `TransactionEvents` be included in the Response.
    #[serde(default)]
    pub events: bool,
    /// Request `BalanceChanges` be included in the Response.
    #[serde(default)]
    pub balance_changes: bool,
    /// Request input `Object`s be included in the Response.
    #[serde(default)]
    pub input_objects: bool,
    /// Request output `Object`s be included in the Response.
    #[serde(default)]
    pub output_objects: bool,
}

/// Response type for the execute transaction endpoint
#[derive(Debug, serde::Serialize, serde::Deserialize, JsonSchema)]
pub struct TransactionExecutionResponse {
    effects: TransactionEffects,

    finality: EffectsFinality,
    events: Option<TransactionEvents>,
    balance_changes: Option<Vec<BalanceChange>>,
    input_objects: Option<Vec<Object>>,
    output_objects: Option<Vec<Object>>,
}

#[derive(Clone, Debug)]
pub enum EffectsFinality {
    Certified {
        /// Validator aggregated signature
        signature: ValidatorAggregatedSignature,
    },
    Checkpointed {
        checkpoint: CheckpointSequenceNumber,
    },
}

impl serde::Serialize for EffectsFinality {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            let readable = match self.clone() {
                EffectsFinality::Certified { signature } => {
                    ReadableEffectsFinality::Certified { signature }
                }
                EffectsFinality::Checkpointed { checkpoint } => {
                    ReadableEffectsFinality::Checkpointed { checkpoint }
                }
            };
            readable.serialize(serializer)
        } else {
            let binary = match self.clone() {
                EffectsFinality::Certified { signature } => {
                    BinaryEffectsFinality::Certified { signature }
                }
                EffectsFinality::Checkpointed { checkpoint } => {
                    BinaryEffectsFinality::Checkpointed { checkpoint }
                }
            };
            binary.serialize(serializer)
        }
    }
}

impl<'de> serde::Deserialize<'de> for EffectsFinality {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            ReadableEffectsFinality::deserialize(deserializer).map(|readable| match readable {
                ReadableEffectsFinality::Certified { signature } => {
                    EffectsFinality::Certified { signature }
                }
                ReadableEffectsFinality::Checkpointed { checkpoint } => {
                    EffectsFinality::Checkpointed { checkpoint }
                }
            })
        } else {
            BinaryEffectsFinality::deserialize(deserializer).map(|binary| match binary {
                BinaryEffectsFinality::Certified { signature } => {
                    EffectsFinality::Certified { signature }
                }
                BinaryEffectsFinality::Checkpointed { checkpoint } => {
                    EffectsFinality::Checkpointed { checkpoint }
                }
            })
        }
    }
}

impl JsonSchema for EffectsFinality {
    fn schema_name() -> String {
        ReadableEffectsFinality::schema_name()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        ReadableEffectsFinality::json_schema(gen)
    }
}

#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize, JsonSchema)]
#[serde(rename = "EffectsFinality", untagged)]
enum ReadableEffectsFinality {
    Certified {
        /// Validator aggregated signature
        signature: ValidatorAggregatedSignature,
    },
    Checkpointed {
        #[serde_as(as = "sui_types::sui_serde::Readable<sui_types::sui_serde::BigInt<u64>, _>")]
        #[schemars(with = "crate::_schemars::U64")]
        checkpoint: CheckpointSequenceNumber,
    },
}

#[derive(serde::Serialize, serde::Deserialize)]
enum BinaryEffectsFinality {
    Certified {
        /// Validator aggregated signature
        signature: ValidatorAggregatedSignature,
    },
    Checkpointed {
        checkpoint: CheckpointSequenceNumber,
    },
}

fn coins(objects: &[Object]) -> impl Iterator<Item = (&Address, Coin<'_>)> + '_ {
    objects.iter().filter_map(|object| {
        let address = match object.owner() {
            Owner::Address(address) => address,
            Owner::Object(object_id) => object_id.as_address(),
            Owner::Shared { .. } | Owner::Immutable => return None,
        };
        let coin = Coin::try_from_object(object)?;
        Some((address, coin))
    })
}

fn derive_balance_changes(
    _effects: &TransactionEffects,
    input_objects: &[Object],
    output_objects: &[Object],
) -> Vec<BalanceChange> {
    // 1. subtract all input coins
    let balances = coins(input_objects).fold(
        std::collections::BTreeMap::<_, i128>::new(),
        |mut acc, (address, coin)| {
            *acc.entry((address, coin.coin_type().to_owned()))
                .or_default() -= coin.balance() as i128;
            acc
        },
    );

    // 2. add all mutated coins
    let balances = coins(output_objects).fold(balances, |mut acc, (address, coin)| {
        *acc.entry((address, coin.coin_type().to_owned()))
            .or_default() += coin.balance() as i128;
        acc
    });

    balances
        .into_iter()
        .filter_map(|((address, coin_type), amount)| {
            if amount == 0 {
                return None;
            }

            Some(BalanceChange {
                address: *address,
                coin_type,
                amount,
            })
        })
        .collect()
}

pub struct SimulateTransaction;

impl ApiEndpoint<RestService> for SimulateTransaction {
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
                    .bcs_content()
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<RestService> {
        RouteHandler::new(self.method(), simulate_transaction)
    }
}

async fn simulate_transaction(
    State(state): State<Option<Arc<dyn TransactionExecutor>>>,
    Query(parameters): Query<SimulateTransactionQueryParameters>,
    accept: AcceptFormat,
    //TODO allow accepting JSON as well as BCS
    Bcs(transaction): Bcs<Transaction>,
) -> Result<ResponseContent<TransactionSimulationResponse>> {
    let executor = state.ok_or_else(|| anyhow::anyhow!("No Transaction Executor"))?;

    simulate_transaction_impl(&executor, &parameters, transaction).map(|response| match accept {
        AcceptFormat::Json => ResponseContent::Json(response),
        AcceptFormat::Bcs => ResponseContent::Bcs(response),
    })
}

pub(super) fn simulate_transaction_impl(
    executor: &Arc<dyn TransactionExecutor>,
    parameters: &SimulateTransactionQueryParameters,
    transaction: Transaction,
) -> Result<TransactionSimulationResponse> {
    if transaction.gas_payment.objects.is_empty() {
        return Err(RestError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "no gas payment provided",
        ));
    }

    let SimulateTransactionResult {
        input_objects,
        output_objects,
        events,
        effects,
        mock_gas_id,
    } = executor
        .simulate_transaction(transaction.try_into()?)
        .map_err(anyhow::Error::from)?;

    if mock_gas_id.is_some() {
        return Err(RestError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "simulate unexpectedly used a mock gas payment",
        ));
    }

    let events = events.map(TryInto::try_into).transpose()?;
    let effects = effects.try_into()?;

    let input_objects = input_objects
        .into_values()
        .map(TryInto::try_into)
        .collect::<Result<Vec<_>, _>>()?;
    let output_objects = output_objects
        .into_values()
        .map(TryInto::try_into)
        .collect::<Result<Vec<_>, _>>()?;
    let balance_changes = derive_balance_changes(&effects, &input_objects, &output_objects);

    TransactionSimulationResponse {
        events,
        effects,
        balance_changes: parameters.balance_changes.then_some(balance_changes),
        input_objects: parameters.input_objects.then_some(input_objects),
        output_objects: parameters.output_objects.then_some(output_objects),
    }
    .pipe(Ok)
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
