// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{Extension, Json};
use fastcrypto::encoding::{Encoding, Hex};
use sui_types::base_types::SuiAddress;
use sui_types::crypto;
use sui_types::crypto::{SignatureScheme, ToFromBytes};
use sui_types::messages::{
    ExecuteTransactionRequestType, SingleTransactionKind, Transaction, TransactionData,
    TransactionKind,
};

use crate::errors::Error;
use crate::types::{
    Amount, ConstructionCombineRequest, ConstructionCombineResponse, ConstructionDeriveRequest,
    ConstructionDeriveResponse, ConstructionHashRequest, ConstructionMetadata,
    ConstructionMetadataRequest, ConstructionMetadataResponse, ConstructionParseRequest,
    ConstructionParseResponse, ConstructionPayloadsRequest, ConstructionPayloadsResponse,
    ConstructionPreprocessRequest, ConstructionPreprocessResponse, ConstructionSubmitRequest,
    InternalOperation, MetadataOptions, SignatureType, SigningPayload, TransactionIdentifier,
    TransactionIdentifierResponse, TransactionMetadata,
};
use crate::{OnlineServerContext, SuiEnv};
use axum::extract::State;
use axum_extra::extract::WithRejection;
use sui_sdk::rpc_types::SuiExecutionStatus;
use sui_types::intent::{Intent, IntentMessage};

/// This module implements the [Rosetta Construction API](https://www.rosetta-api.org/docs/ConstructionApi.html)

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
        .into_internal(Some(metadata.tx_metadata.clone().into()))?
        .try_into_data(metadata)?;
    let intent_msg = IntentMessage::new(Intent::default(), data);
    let intent_msg_bytes = bcs::to_bytes(&intent_msg)?;

    Ok(ConstructionPayloadsResponse {
        unsigned_transaction: Hex::from_bytes(&intent_msg_bytes),
        payloads: vec![SigningPayload {
            account_identifier: address.into(),
            hex_bytes: Hex::encode(bcs::to_bytes(&intent_msg)?),
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

    let signed_tx = Transaction::from_data(
        intent_msg.value,
        Intent::default(),
        crypto::Signature::from_bytes(&[&*flag, &*sig_bytes, &*pub_key].concat())?,
    );
    signed_tx.verify_signature()?;
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
    let signed_tx = signed_tx.verify()?;

    let response = context
        .client
        .quorum_driver()
        .execute_transaction(
            signed_tx,
            Some(ExecuteTransactionRequestType::WaitForEffectsCert),
        )
        .await?;

    if let Some(effect) = response.effects {
        if let SuiExecutionStatus::Failure { error } = effect.status {
            return Err(Error::TransactionExecutionError(error));
        }
    }

    Ok(TransactionIdentifierResponse {
        transaction_identifier: TransactionIdentifier {
            hash: response.tx_digest,
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

    let internal_operation = request.operations.into_internal(request.metadata)?;
    let sender = internal_operation.sender();

    Ok(ConstructionPreprocessResponse {
        options: Some(MetadataOptions { internal_operation }),
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
    let sender = option.internal_operation.sender();
    let (tx_metadata, gas) = match &option.internal_operation {
        InternalOperation::PaySui {
            sender, amounts, ..
        } => {
            let amount = amounts.iter().sum::<u64>() as u128;
            let sender_coins = context
                .client
                .coin_read_api()
                .select_coins(*sender, None, amount + 1000, None, vec![])
                .await?
                .into_iter()
                .map(|coin| coin.object_ref())
                .collect::<Vec<_>>();
            // gas is always the first coin for pay_sui
            let gas = sender_coins[0];
            (TransactionMetadata::PaySui(sender_coins), gas)
        }
        InternalOperation::Delegation {
            sender,
            validator,
            amount,
            locked_until_epoch,
        } => {
            let coins = context
                .client
                .coin_read_api()
                .select_coins(*sender, None, *amount as u128, *locked_until_epoch, vec![])
                .await?
                .into_iter()
                .map(|coin| coin.object_ref())
                .collect::<Vec<_>>();

            let data = context
                .client
                .transaction_builder()
                .request_add_delegation(
                    *sender,
                    coins.iter().map(|coin| coin.0).collect(),
                    Some(*amount as u64),
                    *validator,
                    None,
                    2000,
                )
                .await?;

            let gas = data.gas();
            let TransactionKind::Single(SingleTransactionKind::Call(call)) = data.kind else{
                // This will not happen because `request_add_delegation` call creates a move call transaction.
                panic!("Malformed transaction received from TransactionBuilder.")
            };

            (
                TransactionMetadata::Delegation {
                    sui_framework: call.package,
                    coins,
                    locked_until_epoch: *locked_until_epoch,
                },
                gas,
            )
        }
    };
    // get gas estimation from dry-run, this will also return any tx error.
    let data = option
        .internal_operation
        .try_into_data(ConstructionMetadata {
            tx_metadata: tx_metadata.clone(),
            sender,
            gas,
            budget: 1000,
        })?;
    let dry_run = context.client.read_api().dry_run_transaction(data).await?;

    let budget = dry_run.gas_used.computation_cost + dry_run.gas_used.storage_cost;

    Ok(ConstructionMetadataResponse {
        metadata: ConstructionMetadata {
            tx_metadata,
            sender,
            gas,
            budget,
        },
        suggested_fee: vec![Amount::new(budget as i128)],
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
        tx.into_data().intent_message.value
    } else {
        let intent: IntentMessage<TransactionData> =
            bcs::from_bytes(&request.transaction.to_vec()?)?;
        intent.value
    };
    let account_identifier_signers = if request.signed {
        vec![data.signer().into()]
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
