// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::hash::HashFunction;

use shared_crypto::intent::{Intent, IntentMessage};
use sui_json_rpc_types::{SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions};
use sui_sdk::rpc_types::SuiExecutionStatus;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{DefaultHash, SignatureScheme, ToFromBytes};
use sui_types::signature::{GenericSignature, VerifyParams};
use sui_types::signature_verification::{
    verify_sender_signed_data_message_signatures, VerifiedDigestCache,
};
use sui_types::transaction::{Transaction, TransactionData, TransactionDataAPI};

use crate::errors::Error;
use crate::types::internal_operation::{PayCoin, TransactionObjectData, TryConstructTransaction};
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

// This module implements the [Rosetta Construction API](https://www.rosetta-api.org/docs/ConstructionApi.html)

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
    let dry_run = context
        .client
        .read_api()
        .dry_run_transaction_block(tx_data)
        .await?;
    if let SuiExecutionStatus::Failure { error } = dry_run.effects.status() {
        return Err(Error::TransactionDryRunError(error.clone()));
    };

    let response = context
        .client
        .quorum_driver_api()
        .execute_transaction_block(
            signed_tx,
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_effects()
                .with_balance_changes(),
            None,
        )
        .await?;

    if let SuiExecutionStatus::Failure { error } = response
        .effects
        .expect("Execute transaction should return effects")
        .status()
    {
        return Err(Error::TransactionExecutionError(error.to_string()));
    }

    Ok(TransactionIdentifierResponse {
        transaction_identifier: TransactionIdentifier {
            hash: response.digest,
        },
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
        InternalOperation::PayCoin(PayCoin { currency, .. }) => Some(currency.clone()),
        _ => None,
    };

    let mut gas_price = context
        .client
        .governance_api()
        .get_reference_gas_price()
        .await?;
    // make sure it works over epoch changes
    gas_price += 100;

    let TransactionObjectData {
        gas_coins,
        extra_gas_coins,
        objects,
        total_sui_balance,
        budget,
    } = option
        .internal_operation
        .try_fetch_needed_objects(&context.client, Some(gas_price), budget)
        .await?;
    Ok(ConstructionMetadataResponse {
        metadata: ConstructionMetadata {
            sender,
            gas_coins,
            extra_gas_coins,
            objects,
            total_coin_value: total_sui_balance,
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
