// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future;
use std::sync::Arc;

use axum::{Extension, Json};
use fastcrypto::encoding::{Encoding, Hex};
use futures::StreamExt;
use sui_sdk::SUI_COIN_TYPE;

use sui_types::base_types::SuiAddress;
use sui_types::crypto;
use sui_types::crypto::{SignatureScheme, ToFromBytes};
use sui_types::messages::{ExecuteTransactionRequestType, Transaction, TransactionData};

use crate::errors::Error;
use crate::operations::Operation;
use crate::types::{
    AccountIdentifier, ConstructionCombineRequest, ConstructionCombineResponse,
    ConstructionDeriveRequest, ConstructionDeriveResponse, ConstructionHashRequest,
    ConstructionMetadata, ConstructionMetadataRequest, ConstructionMetadataResponse,
    ConstructionParseRequest, ConstructionParseResponse, ConstructionPayloadsRequest,
    ConstructionPayloadsResponse, ConstructionPreprocessRequest, ConstructionPreprocessResponse,
    ConstructionSubmitRequest, MetadataOptions, SignatureType, SigningPayload,
    TransactionIdentifier, TransactionIdentifierResponse,
};
use crate::{OnlineServerContext, SuiEnv};
use anyhow::anyhow;
use sui_types::intent::{Intent, IntentMessage};

/// This module implements the [Rosetta Construction API](https://www.rosetta-api.org/docs/ConstructionApi.html)

/// Derive returns the AccountIdentifier associated with a public key.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionderive)
pub async fn derive(
    Json(request): Json<ConstructionDeriveRequest>,
    Extension(env): Extension<SuiEnv>,
) -> Result<ConstructionDeriveResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let address: SuiAddress = request.public_key.try_into()?;
    Ok(ConstructionDeriveResponse {
        account_identifier: AccountIdentifier { address },
    })
}

/// Payloads is called with an array of operations and the response from /construction/metadata.
/// It returns an unsigned transaction blob and a collection of payloads that must be signed by
/// particular AccountIdentifiers using a certain SignatureType.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionpayloads)
pub async fn payloads(
    Json(request): Json<ConstructionPayloadsRequest>,
    Extension(env): Extension<SuiEnv>,
) -> Result<ConstructionPayloadsResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let metadata = request.metadata.ok_or(Error::MissingMetadata)?;

    let data = Operation::create_data(request.operations, metadata).await?;
    let address = data.signer();
    let intent_msg = IntentMessage::new(Intent::default(), data);
    let intent_msg_bytes = bcs::to_bytes(&intent_msg)?;

    Ok(ConstructionPayloadsResponse {
        unsigned_transaction: Hex::from_bytes(&intent_msg_bytes),
        payloads: vec![SigningPayload {
            account_identifier: AccountIdentifier { address },
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
    Json(request): Json<ConstructionCombineRequest>,
    Extension(env): Extension<SuiEnv>,
) -> Result<ConstructionCombineResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let unsigned_tx = request
        .unsigned_transaction
        .to_vec()
        .map_err(|e| anyhow!(e))?;
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
    Json(request): Json<ConstructionSubmitRequest>,
    Extension(context): Extension<Arc<OnlineServerContext>>,
    Extension(env): Extension<SuiEnv>,
) -> Result<TransactionIdentifierResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let signed_tx: Transaction = bcs::from_bytes(&request.signed_transaction.to_vec()?)?;
    let signed_tx = signed_tx.verify()?;

    let response = context
        .client
        .quorum_driver()
        .execute_transaction(
            signed_tx,
            Some(ExecuteTransactionRequestType::ImmediateReturn),
        )
        .await?;

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
    Json(request): Json<ConstructionPreprocessRequest>,
    Extension(env): Extension<SuiEnv>,
) -> Result<ConstructionPreprocessResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let (sender, amount) = request
        .operations
        .iter()
        .find_map(|op| match (&op.account, &op.amount) {
            (Some(acc), Some(amount)) => {
                if amount.value.is_negative() {
                    Some((acc.address, amount.value.abs()))
                } else {
                    None
                }
            }
            _ => None,
        })
        .ok_or_else(|| {
            Error::MalformedOperationError(
                "Cannot extract sender's address from operations.".to_string(),
            )
        })?;

    Ok(ConstructionPreprocessResponse {
        options: Some(MetadataOptions { sender, amount }),
        required_public_keys: vec![AccountIdentifier { address: sender }],
    })
}

/// TransactionHash returns the network-specific transaction hash for a signed transaction.
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionhash)
pub async fn hash(
    Json(request): Json<ConstructionHashRequest>,
    Extension(env): Extension<SuiEnv>,
) -> Result<TransactionIdentifierResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let tx_bytes = request
        .signed_transaction
        .to_vec()
        .map_err(|e| anyhow::anyhow!(e))?;
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
    Json(request): Json<ConstructionMetadataRequest>,
    Extension(context): Extension<Arc<OnlineServerContext>>,
    Extension(env): Extension<SuiEnv>,
) -> Result<ConstructionMetadataResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let sender_coins = if let Some(option) = request.options {
        let mut total = 0u128;
        let coins = context
            .client
            .coin_read_api()
            .get_coins_stream(option.sender, Some(SUI_COIN_TYPE.to_string()))
            .take_while(|coin| {
                let ready = future::ready(total < option.amount);
                total += coin.balance as u128;
                ready
            })
            .map(|c| c.object_ref())
            .collect::<Vec<_>>()
            .await;

        if total < option.amount {
            return Err(Error::InsufficientFund {
                address: option.sender,
                amount: option.amount,
            });
        }
        coins
    } else {
        Default::default()
    };

    Ok(ConstructionMetadataResponse {
        metadata: ConstructionMetadata { sender_coins },
        suggested_fee: vec![],
    })
}

///  This is run as a sanity check before signing (after /construction/payloads)
/// and before broadcast (after /construction/combine).
///
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/ConstructionApi.html#constructionparse)
pub async fn parse(
    Json(request): Json<ConstructionParseRequest>,
    Extension(env): Extension<SuiEnv>,
) -> Result<ConstructionParseResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let data = if request.signed {
        let tx: Transaction = bcs::from_bytes(
            &request
                .transaction
                .to_vec()
                .map_err(|e| anyhow::anyhow!(e))?,
        )?;
        tx.into_data().intent_message.value
    } else {
        let intent: IntentMessage<TransactionData> =
            bcs::from_bytes(&request.transaction.to_vec().map_err(|e| anyhow!(e))?)?;
        intent.value
    };
    let account_identifier_signers = if request.signed {
        vec![AccountIdentifier {
            address: data.signer(),
        }]
    } else {
        vec![]
    };
    let operations = Operation::from_data(&data.try_into()?)?;

    Ok(ConstructionParseResponse {
        operations,
        account_identifier_signers,
        metadata: None,
    })
}
