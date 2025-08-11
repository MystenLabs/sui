// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::hash::HashFunction;

use shared_crypto::intent::{Intent, IntentMessage};
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::{DefaultHash, SignatureScheme, ToFromBytes};
use sui_types::signature::{GenericSignature, VerifyParams};
use sui_types::signature_verification::{
    verify_sender_signed_data_message_signatures, VerifiedDigestCache,
};
use sui_types::transaction::{Transaction, TransactionData, TransactionDataAPI};

use crate::errors::Error;
use crate::types::{
    Amount, ConstructionCombineRequest, ConstructionCombineResponse, ConstructionDeriveRequest,
    ConstructionDeriveResponse, ConstructionHashRequest, ConstructionMetadata,
    ConstructionMetadataRequest, ConstructionMetadataResponse, ConstructionParseRequest,
    ConstructionParseResponse, ConstructionPayloadsRequest, ConstructionPayloadsResponse,
    ConstructionPreprocessRequest, ConstructionPreprocessResponse, ConstructionSubmitRequest,
    InternalOperation, MetadataOptions, SignatureType, SigningPayload, TransactionIdentifier,
    TransactionIdentifierResponse,
};
use crate::{OnlineServerContext, SuiEnv};
use sui_rpc::proto::sui::rpc::v2beta2::{execution_error, ExecutionError};

// This module implements the [Rosetta Construction API](https://www.rosetta-api.org/docs/ConstructionApi.html)

/// Extract a simple error string from a GRPC ExecutionError
fn extract_error_kind(error: &ExecutionError) -> String {
    if let Some(kind) = error.kind {
        match execution_error::ExecutionErrorKind::try_from(kind) {
            Ok(execution_error::ExecutionErrorKind::InsufficientGas) => {
                "InsufficientGas".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::InvalidGasObject) => {
                "InvalidGasObject".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::InvariantViolation) => {
                "InvariantViolation".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::FeatureNotYetSupported) => {
                "FeatureNotYetSupported".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::ObjectTooBig) => "ObjectTooBig".to_string(),
            Ok(execution_error::ExecutionErrorKind::PackageTooBig) => "PackageTooBig".to_string(),
            Ok(execution_error::ExecutionErrorKind::CircularObjectOwnership) => {
                "CircularObjectOwnership".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::InsufficientCoinBalance) => {
                "InsufficientCoinBalance".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::CoinBalanceOverflow) => {
                "CoinBalanceOverflow".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::PublishErrorNonZeroAddress) => {
                "PublishErrorNonZeroAddress".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::SuiMoveVerificationError) => {
                "SuiMoveVerificationError".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::MovePrimitiveRuntimeError) => {
                "MovePrimitiveRuntimeError".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::MoveAbort) => "MoveAbort".to_string(),
            Ok(execution_error::ExecutionErrorKind::VmVerificationOrDeserializationError) => {
                "VmVerificationOrDeserializationError".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::VmInvariantViolation) => {
                "VmInvariantViolation".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::FunctionNotFound) => {
                "FunctionNotFound".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::ArityMismatch) => "ArityMismatch".to_string(),
            Ok(execution_error::ExecutionErrorKind::TypeArityMismatch) => {
                "TypeArityMismatch".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::NonEntryFunctionInvoked) => {
                "NonEntryFunctionInvoked".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::CommandArgumentError) => {
                "CommandArgumentError".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::TypeArgumentError) => {
                "TypeArgumentError".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::UnusedValueWithoutDrop) => {
                "UnusedValueWithoutDrop".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::InvalidPublicFunctionReturnType) => {
                "InvalidPublicFunctionReturnType".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::InvalidTransferObject) => {
                "InvalidTransferObject".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::EffectsTooLarge) => {
                "EffectsTooLarge".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::PublishUpgradeMissingDependency) => {
                "PublishUpgradeMissingDependency".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::PublishUpgradeDependencyDowngrade) => {
                "PublishUpgradeDependencyDowngrade".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::PackageUpgradeError) => {
                "PackageUpgradeError".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::WrittenObjectsTooLarge) => {
                "WrittenObjectsTooLarge".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::CertificateDenied) => {
                "CertificateDenied".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::SuiMoveVerificationTimedout) => {
                "SuiMoveVerificationTimedout".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::SharedObjectOperationNotAllowed) => {
                "SharedObjectOperationNotAllowed".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::InputObjectDeleted) => {
                "InputObjectDeleted".to_string()
            }
            Ok(
                execution_error::ExecutionErrorKind::ExecutionCanceledDueToSharedObjectCongestion,
            ) => "ExecutionCanceledDueToSharedObjectCongestion".to_string(),
            Ok(execution_error::ExecutionErrorKind::AddressDeniedForCoin) => {
                "AddressDeniedForCoin".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::CoinTypeGlobalPause) => {
                "CoinTypeGlobalPause".to_string()
            }
            Ok(
                execution_error::ExecutionErrorKind::ExecutionCanceledDueToRandomnessUnavailable,
            ) => "ExecutionCanceledDueToRandomnessUnavailable".to_string(),
            Ok(execution_error::ExecutionErrorKind::MoveVectorElemTooBig) => {
                "MoveVectorElemTooBig".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::MoveRawValueTooBig) => {
                "MoveRawValueTooBig".to_string()
            }
            Ok(execution_error::ExecutionErrorKind::InvalidLinkage) => "InvalidLinkage".to_string(),
            Ok(execution_error::ExecutionErrorKind::Unknown) | Err(_) => "Unknown".to_string(),
        }
    } else {
        format!("{:?}", error)
    }
}

/// Derive returns the AccountIdentifier associated with a public key.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionderive)
pub async fn derive(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionDeriveRequest>, Error>,
) -> Result<ConstructionDeriveResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let address: SuiAddress = request.public_key.try_into()?;
    Ok(ConstructionDeriveResponse {
        account_identifier: address.into(),
    })
}

/// Payloads is called with an array of operations and the response from /construction/metadata.
/// It returns an unsigned transaction blob and a collection of payloads that must be signed by
/// particular AccountIdentifiers using a certain SignatureType.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionpayloads)
pub async fn payloads(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionPayloadsRequest>, Error>,
) -> Result<ConstructionPayloadsResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let metadata = request.metadata.ok_or(Error::MissingMetadata)?;
    let address = metadata.sender;

    let data = request
        .operations
        .into_internal()?
        .try_into_data(metadata)?;
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), data);
    let intent_msg_bytes = bcs::to_bytes(&intent_msg)?;

    let mut hasher = DefaultHash::default();
    hasher.update(bcs::to_bytes(&intent_msg).expect("Message serialization should not fail"));
    let digest = hasher.finalize().digest;

    Ok(ConstructionPayloadsResponse {
        unsigned_transaction: Hex::from_bytes(&intent_msg_bytes),
        payloads: vec![SigningPayload {
            account_identifier: address.into(),
            hex_bytes: Hex::encode(digest),
            signature_type: Some(SignatureType::Ed25519),
        }],
    })
}

/// Combine creates a network-specific transaction from an unsigned transaction
/// and an array of provided signatures.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructioncombine)
pub async fn combine(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionCombineRequest>, Error>,
) -> Result<ConstructionCombineResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let unsigned_tx = request.unsigned_transaction.to_vec()?;
    let intent_msg: IntentMessage<TransactionData> = bcs::from_bytes(&unsigned_tx)?;
    let sig = request
        .signatures
        .first()
        .ok_or_else(|| Error::MissingInput("Signature".to_string()))?;
    let sig_bytes = sig.hex_bytes.to_vec()?;
    let pub_key = sig.public_key.hex_bytes.to_vec()?;
    let flag = vec![match sig.signature_type {
        SignatureType::Ed25519 => SignatureScheme::ED25519,
        SignatureType::Ecdsa => SignatureScheme::Secp256k1,
    }
    .flag()];

    let signed_tx = Transaction::from_generic_sig_data(
        intent_msg.value,
        vec![GenericSignature::from_bytes(
            &[&*flag, &*sig_bytes, &*pub_key].concat(),
        )?],
    );
    // TODO: this will likely fail with zklogin authenticator, since we do not know the current epoch.
    // As long as coinbase doesn't need to use zklogin for custodial wallets this is okay.
    let place_holder_epoch = 0;
    verify_sender_signed_data_message_signatures(
        &signed_tx,
        place_holder_epoch,
        &VerifyParams::default(),
        Arc::new(VerifiedDigestCache::new_empty()), // no need to use cache in rosetta
        None,
    )?;
    let signed_tx_bytes = bcs::to_bytes(&signed_tx)?;

    Ok(ConstructionCombineResponse {
        signed_transaction: Hex::from_bytes(&signed_tx_bytes),
    })
}

/// Submit a pre-signed transaction to the node.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionsubmit)
pub async fn submit(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionSubmitRequest>, Error>,
) -> Result<TransactionIdentifierResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let signed_tx: Transaction = bcs::from_bytes(&request.signed_transaction.to_vec()?)?;

    // According to RosettaClient.rosseta_flow() (see tests), this transaction has already passed
    // through a dry_run with a possibly invalid budget (metadata endpoint), but the requirements
    // are that it should pass from there and fail here.
    let tx_data = signed_tx.data().transaction_data().clone();
    let dry_run = context.client.simulate_transaction(tx_data).await?;

    // Check if simulation was successful
    let transaction = dry_run.transaction.as_ref().ok_or_else(|| {
        Error::TransactionDryRunError("No transaction returned from simulation".to_string())
    })?;

    let effects = transaction.effects.as_ref().ok_or_else(|| {
        Error::TransactionDryRunError("No effects returned from simulation".to_string())
    })?;

    if let Some(status) = &effects.status {
        if !status.success.unwrap_or(false) {
            if let Some(error) = &status.error {
                return Err(Error::TransactionDryRunError(extract_error_kind(error)));
            }
            return Err(Error::TransactionDryRunError(
                "Transaction simulation failed".to_string(),
            ));
        }
    }

    let response = context.client.execute_transaction(signed_tx).await?;

    // Check if execution was successful
    use sui_types::effects::TransactionEffectsAPI;
    match response.effects.status() {
        sui_types::execution_status::ExecutionStatus::Success => {}
        sui_types::execution_status::ExecutionStatus::Failure { error, .. } => {
            return Err(Error::TransactionExecutionError(error.to_string()));
        }
    }

    // Extract the transaction digest from the response
    let digest = response.effects.transaction_digest();

    Ok(TransactionIdentifierResponse {
        transaction_identifier: TransactionIdentifier { hash: *digest },
        metadata: None,
    })
}

/// Preprocess is called prior to /construction/payloads to construct a request for any metadata
/// that is needed for transaction construction given (i.e. account nonce).
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionpreprocess)
pub async fn preprocess(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionPreprocessRequest>, Error>,
) -> Result<ConstructionPreprocessResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let internal_operation = request.operations.into_internal()?;
    let sender = internal_operation.sender();
    let budget = request.metadata.and_then(|m| m.budget);
    Ok(ConstructionPreprocessResponse {
        options: Some(MetadataOptions {
            internal_operation,
            budget,
        }),
        required_public_keys: vec![sender.into()],
    })
}

/// TransactionHash returns the network-specific transaction hash for a signed transaction.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionhash)
pub async fn hash(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionHashRequest>, Error>,
) -> Result<TransactionIdentifierResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let tx_bytes = request.signed_transaction.to_vec()?;
    let tx: Transaction = bcs::from_bytes(&tx_bytes)?;

    Ok(TransactionIdentifierResponse {
        transaction_identifier: TransactionIdentifier { hash: *tx.digest() },
        metadata: None,
    })
}

/// Get any information required to construct a transaction for a specific network.
/// For Sui, we are returning the latest object refs for all the input objects,
/// which will be used in transaction construction.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionmetadata)
pub async fn metadata(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionMetadataRequest>, Error>,
) -> Result<ConstructionMetadataResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let option = request.options.ok_or(Error::MissingMetadata)?;
    let budget = option.budget;
    let sender = option.internal_operation.sender();
    let currency = match &option.internal_operation {
        InternalOperation::PayCoin { currency, .. } => Some(currency.clone()),
        _ => None,
    };
    let coin_type = currency.as_ref().map(|c| c.metadata.coin_type.clone());

    // Get the current epoch to fetch reference gas price
    let epoch = context
        .client
        .get_epoch(None) // None gets the current epoch
        .await?;

    let mut gas_price = epoch
        .reference_gas_price
        .ok_or_else(|| Error::DataError("No reference gas price in epoch".to_string()))?;
    // make sure it works over epoch changes
    gas_price += 100;

    // Get amount, objects, for the operation
    let (total_required_amount, objects) = match &option.internal_operation {
        InternalOperation::PaySui { amounts, .. } => {
            let amount = amounts.iter().sum::<u64>();
            (Some(amount), vec![])
        }
        InternalOperation::PayCoin { amounts, .. } => {
            let amount = amounts.iter().sum::<u64>();
            let selected_coins = context
                .client
                .select_coins(sender, coin_type, amount, vec![])
                .await
                .ok()
                .unwrap_or_default();
            let coin_objs: Vec<ObjectRef> = selected_coins
                .into_iter()
                .map(|(_, _, object_ref)| object_ref)
                .collect();
            (Some(0), coin_objs) // amount is 0 for gas coin
        }
        InternalOperation::Stake { amount, .. } => (*amount, vec![]),
        InternalOperation::WithdrawStake { sender, stake_ids } => {
            let stake_ids = if stake_ids.is_empty() {
                // unstake all
                context.client.get_stakes(*sender).await?
            } else {
                stake_ids.clone()
            };

            if stake_ids.is_empty() {
                return Err(Error::InvalidInput("No active stake to withdraw".into()));
            }

            let stake_refs = context.client.get_object_refs(stake_ids).await?;

            (Some(0), stake_refs)
        }
    };

    // Get budget for suggested_fee and metadata.budget
    let budget = match budget {
        Some(budget) => budget,
        None => {
            // Dry run the transaction to get the gas used, amount doesn't really matter here when using mock coins.
            // get gas estimation from dry-run, this will also return any tx error.
            let data = option
                .internal_operation
                .try_into_data(ConstructionMetadata {
                    sender,
                    coins: vec![],
                    objects: objects.clone(),
                    // Mock coin have 1B SUI
                    total_coin_value: 1_000_000_000 * 1_000_000_000,
                    gas_price,
                    // MAX BUDGET
                    budget: 50_000_000_000,
                    currency: currency.clone(),
                })?;

            let dry_run = context.client.simulate_transaction(data).await?;

            // Check if simulation was successful and get effects
            let transaction = dry_run.transaction.as_ref().ok_or_else(|| {
                Error::TransactionDryRunError("No transaction returned from simulation".to_string())
            })?;

            let effects = transaction.effects.as_ref().ok_or_else(|| {
                Error::TransactionDryRunError("No effects returned from simulation".to_string())
            })?;

            if let Some(status) = &effects.status {
                if !status.success.unwrap_or(false) {
                    if let Some(error) = &status.error {
                        return Err(Error::TransactionDryRunError(extract_error_kind(error)));
                    }
                    return Err(Error::TransactionDryRunError(
                        "Transaction simulation failed".to_string(),
                    ));
                }
            }

            // Extract gas cost from proto effects
            let gas_summary = effects.gas_used.as_ref().ok_or_else(|| {
                Error::TransactionDryRunError("No gas cost summary in effects".to_string())
            })?;

            let computation_cost = gas_summary.computation_cost.unwrap_or(0);
            let storage_cost = gas_summary.storage_cost.unwrap_or(0);
            let storage_rebate = gas_summary.storage_rebate.unwrap_or(0);

            // Total gas cost = computation + storage - rebate
            computation_cost + storage_cost - storage_rebate
        }
    };

    // Try select gas coins for required amounts
    let coins = if let Some(amount) = total_required_amount {
        let total_amount = amount + budget;
        context
            .client
            .select_coins(sender, None, total_amount, vec![])
            .await
            .ok()
            .map(|selected| {
                selected
                    .into_iter()
                    .map(|(id, balance, _obj_ref)| sui_types::coin::Coin::new(id, balance))
                    .collect()
            })
    } else {
        None
    };

    // If required amount is None (all SUI) or failed to select coin (might not have enough SUI), select all coins.
    let coins: Vec<sui_types::coin::Coin> = if let Some(coins) = coins {
        coins
    } else {
        let all_coins = context.client.get_all_coins(sender, None).await?;
        all_coins
            .into_iter()
            .map(|(id, balance, _obj_ref)| sui_types::coin::Coin::new(id, balance))
            .collect()
    };

    let total_coin_value = coins.iter().fold(0, |sum, coin| sum + coin.balance.value());

    // Need to get object references for coins - store them separately
    let coin_refs = context
        .client
        .get_object_refs(coins.iter().map(|c| *c.id()).collect())
        .await?;

    Ok(ConstructionMetadataResponse {
        metadata: ConstructionMetadata {
            sender,
            coins: coin_refs,
            objects,
            total_coin_value: total_coin_value.into(),
            gas_price,
            budget,
            currency,
        },
        suggested_fee: vec![Amount::new(budget as i128, None)],
    })
}

///  This is run as a sanity check before signing (after /construction/payloads)
/// and before broadcast (after /construction/combine).
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionparse)
pub async fn parse(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionParseRequest>, Error>,
) -> Result<ConstructionParseResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let data = if request.signed {
        let tx: Transaction = bcs::from_bytes(&request.transaction.to_vec()?)?;
        tx.into_data().intent_message().value.clone()
    } else {
        let intent: IntentMessage<TransactionData> =
            bcs::from_bytes(&request.transaction.to_vec()?)?;
        intent.value
    };
    let account_identifier_signers = if request.signed {
        vec![data.sender().into()]
    } else {
        vec![]
    };
    let operations = data.try_into()?;
    Ok(ConstructionParseResponse {
        operations,
        account_identifier_signers,
        metadata: None,
    })
}
