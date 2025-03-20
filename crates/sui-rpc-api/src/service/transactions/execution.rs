// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::field_mask::FieldMaskTree;
use crate::field_mask::FieldMaskUtil;
use crate::proto::google::rpc::bad_request::FieldViolation;
use crate::proto::google::rpc::BadRequest;
use crate::proto::node::v2::effects_finality::Finality;
use crate::proto::node::v2::EffectsFinality;
use crate::proto::node::v2::ExecuteTransactionRequest;
use crate::proto::node::v2::ExecuteTransactionResponse;
use crate::types::SimulateTransactionQueryParameters;
use crate::types::TransactionSimulationResponse;
use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use prost_types::FieldMask;
use sui_sdk_types::framework::Coin;
use sui_sdk_types::Address;
use sui_sdk_types::BalanceChange;
use sui_sdk_types::Object;
use sui_sdk_types::Owner;
use sui_sdk_types::SignedTransaction;
use sui_sdk_types::Transaction;
use sui_sdk_types::TransactionEffects;
use sui_types::transaction_executor::SimulateTransactionResult;
use tap::Pipe;

impl RpcService {
    pub async fn execute_transaction(
        &self,
        request: ExecuteTransactionRequest,
    ) -> Result<ExecuteTransactionResponse> {
        let executor = self
            .executor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Transaction Executor"))?;

        let transaction = match (request.transaction, request.transaction_bcs) {
            (Some(_), Some(_)) => {
                let description = "only one of `transaction` or `transaction_bcs` can be provided";
                let bad_request = BadRequest {
                    field_violations: vec![
                        FieldViolation::new("transaction")
                            .with_description(description)
                            .with_reason(ErrorReason::FieldInvalid),
                        FieldViolation::new("transaction_bcs")
                            .with_description(description)
                            .with_reason(ErrorReason::FieldInvalid),
                    ],
                };
                return Err(bad_request.into());
            }

            (Some(transaction), None) => Transaction::try_from(&transaction).map_err(|e| {
                FieldViolation::new("transaction")
                    .with_description(format!("invalid transaction: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?,

            (None, Some(bcs)) => bcs.deserialize::<Transaction>().map_err(|e| {
                FieldViolation::new("transaction_bcs")
                    .with_description(format!("invalid transaction: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?,

            (None, None) => {
                let description = "one of `transaction` or `transaction_bcs` must be provided";
                let bad_request = BadRequest {
                    field_violations: vec![
                        FieldViolation::new("transaction")
                            .with_description(description)
                            .with_reason(ErrorReason::FieldMissing),
                        FieldViolation::new("transaction_bcs")
                            .with_description(description)
                            .with_reason(ErrorReason::FieldMissing),
                    ],
                };
                return Err(bad_request.into());
            }
        };

        let mut signatures: Vec<sui_sdk_types::UserSignature> = Vec::new();

        if !request.signatures.is_empty() {
            let from_proto_signatures = request
                .signatures
                .iter()
                .enumerate()
                .map(|(i, signature)| {
                    signature.try_into().map_err(|e| {
                        FieldViolation::new_at("signatures", i)
                            .with_description(format!("invalid signature: {e}"))
                            .with_reason(ErrorReason::FieldInvalid)
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            signatures.extend(from_proto_signatures);
        }

        if !request.signatures_bytes.is_empty() {
            let from_bytes_signatures = request
                .signatures_bytes
                .iter()
                .enumerate()
                .map(|(i, bytes)| {
                    sui_sdk_types::UserSignature::from_bytes(bytes).map_err(|e| {
                        FieldViolation::new_at("signatures_bcs", i)
                            .with_description(format!("invalid signature: {e}"))
                            .with_reason(ErrorReason::FieldInvalid)
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            signatures.extend(from_bytes_signatures);
        }

        let signed_transaction = SignedTransaction {
            transaction,
            signatures,
        };

        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(ExecuteTransactionRequest::READ_MASK_DEFAULT));
        ExecuteTransactionResponse::validate_read_mask(&read_mask).map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        let read_mask = FieldMaskTree::from(read_mask);

        let request = sui_types::quorum_driver_types::ExecuteTransactionRequestV3 {
            transaction: signed_transaction.try_into()?,
            include_events: read_mask.contains("events") || read_mask.contains("events_bcs"),
            include_input_objects: read_mask.contains("balance_changes"),
            include_output_objects: read_mask.contains("balance_changes"),
            include_auxiliary_data: false,
        };

        let sui_types::quorum_driver_types::ExecuteTransactionResponseV3 {
            effects,
            events,
            input_objects,
            output_objects,
            auxiliary_data: _,
        } = executor.execute_transaction(request, None).await?;

        let (effects, finality) = {
            let sui_types::quorum_driver_types::FinalizedEffects {
                effects,
                finality_info,
            } = effects;
            let finality = match finality_info {
                sui_types::quorum_driver_types::EffectsFinalityInfo::Certified(sig) => {
                    Finality::Certified(
                        sui_sdk_types::ValidatorAggregatedSignature::from(sig).into(),
                    )
                }
                sui_types::quorum_driver_types::EffectsFinalityInfo::Checkpointed(
                    _epoch,
                    checkpoint,
                ) => Finality::Checkpointed(checkpoint),
                sui_types::quorum_driver_types::EffectsFinalityInfo::QuorumExecuted(_) => {
                    Finality::QuorumExecuted(())
                }
            };

            (
                effects.try_into()?,
                EffectsFinality {
                    finality: Some(finality),
                },
            )
        };

        let effects_bcs = read_mask
            .contains("effects_bcs")
            .then(|| bcs::to_bytes(&effects))
            .transpose()?
            .map(Into::into);

        let events = events
            .map(sui_sdk_types::TransactionEvents::try_from)
            .transpose()?;
        let events_bcs = read_mask
            .contains("events_bcs")
            .then(|| events.as_ref().map(bcs::to_bytes))
            .flatten()
            .transpose()?
            .map(Into::into);

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

        let balance_changes = match (
            read_mask.contains("balance_changes"),
            &input_objects,
            &output_objects,
        ) {
            (true, Some(input_objects), Some(output_objects)) => Some(derive_balance_changes(
                &effects,
                input_objects,
                output_objects,
            )),
            _ => None,
        };

        ExecuteTransactionResponse {
            finality: read_mask.contains("finality").then_some(finality),
            effects: read_mask.contains("effects").then(|| effects.into()),
            effects_bcs,
            events: read_mask
                .contains("events")
                .then(|| events.map(Into::into))
                .flatten(),
            events_bcs,
            balance_changes: balance_changes
                .map(|b| b.into_iter().map(Into::into).collect())
                .unwrap_or_default(),
        }
        .pipe(Ok)
    }

    pub fn simulate_transaction(
        &self,
        parameters: &SimulateTransactionQueryParameters,
        transaction: Transaction,
    ) -> Result<TransactionSimulationResponse> {
        let executor = self
            .executor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No Transaction Executor"))?;

        if transaction.gas_payment.objects.is_empty() {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
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
            return Err(RpcError::new(
                tonic::Code::Internal,
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

pub(crate) fn derive_balance_changes(
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
