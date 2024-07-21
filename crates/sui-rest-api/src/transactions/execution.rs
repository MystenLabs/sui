// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Query, State};
use sui_sdk2::types::framework::Coin;
use sui_sdk2::types::{
    Address, BalanceChange, CheckpointSequenceNumber, Object, Owner, SignedTransaction,
    TransactionEffects, TransactionEvents, ValidatorAggregatedSignature,
};
use tap::Pipe;

use crate::openapi::{ApiEndpoint, RouteHandler};
use crate::response::Bcs;
use crate::{accept::AcceptFormat, response::ResponseContent};
use crate::{RestService, Result};

/// Trait to define the interface for how the REST service interacts with a a QuorumDriver or a
/// simulated transaction executor.
#[async_trait::async_trait]
pub trait TransactionExecutor: Send + Sync {
    async fn execute_transaction(
        &self,
        request: sui_types::quorum_driver_types::ExecuteTransactionRequestV3,
        client_addr: Option<std::net::SocketAddr>,
    ) -> Result<
        sui_types::quorum_driver_types::ExecuteTransactionResponseV3,
        sui_types::quorum_driver_types::QuorumDriverError,
    >;

    //TODO include Simulate functionality
}

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
        generator.subschema_for::<SignedTransaction>();

        openapiv3::v3_1::Operation::default()
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
        transaction: transaction.into(),
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

        (effects.into(), finality)
    };

    let events = if parameters.events {
        events.map(Into::into)
    } else {
        None
    };

    let input_objects =
        input_objects.map(|objects| objects.into_iter().map(Into::into).collect::<Vec<_>>());
    let output_objects =
        output_objects.map(|objects| objects.into_iter().map(Into::into).collect::<Vec<_>>());

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
#[derive(Debug, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, serde::Serialize, serde::Deserialize)]
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

#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "untagged")]
enum ReadableEffectsFinality {
    Certified {
        /// Validator aggregated signature
        signature: ValidatorAggregatedSignature,
    },
    Checkpointed {
        #[serde_as(as = "sui_types::sui_serde::Readable<sui_types::sui_serde::BigInt<u64>, _>")]
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
            *acc.entry((address, coin.coin_type())).or_default() -= coin.balance() as i128;
            acc
        },
    );

    // 2. add all mutated coins
    let balances = coins(output_objects).fold(balances, |mut acc, (address, coin)| {
        *acc.entry((address, coin.coin_type())).or_default() += coin.balance() as i128;
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
                coin_type: coin_type.to_owned(),
                amount,
            })
        })
        .collect()
}
