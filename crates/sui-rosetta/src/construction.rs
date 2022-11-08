// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{Extension, Json};
use fastcrypto::encoding::{Encoding, Hex};
use sui_types::base_types::SuiAddress;
use sui_types::crypto;
use sui_types::crypto::{SignableBytes, SignatureScheme, ToFromBytes};
use sui_types::messages::{
    QuorumDriverRequest, QuorumDriverRequestType, QuorumDriverResponse, SenderSignedData,
    Transaction, TransactionData,
};
use sui_types::object::Owner;

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
use crate::ErrorType::InternalError;
use crate::{ErrorType, OnlineServerContext, SuiEnv};

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
    let metadata = request
        .metadata
        .ok_or_else(|| Error::new(ErrorType::MissingMetadata))?;

    let data = Operation::create_data(request.operations, metadata).await?;
    let hex_bytes = Hex::encode(data.to_bytes());

    Ok(ConstructionPayloadsResponse {
        unsigned_transaction: Hex::from_bytes(&data.to_bytes()),
        payloads: vec![SigningPayload {
            account_identifier: AccountIdentifier {
                address: data.signer(),
            },
            hex_bytes,
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
        .map_err(|e| anyhow::anyhow!(e))?;
    let data = TransactionData::from_signable_bytes(&unsigned_tx)?;
    let sig = request.signatures.first().unwrap();
    let sig_bytes = sig.hex_bytes.to_vec().map_err(|e| anyhow::anyhow!(e))?;
    let pub_key = sig
        .public_key
        .hex_bytes
        .to_vec()
        .map_err(|e| anyhow::anyhow!(e))?;
    let flag = vec![match sig.signature_type {
        SignatureType::Ed25519 => SignatureScheme::ED25519,
        SignatureType::Ecdsa => SignatureScheme::Secp256k1,
    }
    .flag()];

    let signed_tx = Transaction::new(SenderSignedData::new(
        data,
        crypto::Signature::from_bytes(&[&*flag, &*sig_bytes, &*pub_key].concat())?,
    ));
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
    let signed_tx: Transaction = bcs::from_bytes(
        &request
            .signed_transaction
            .to_vec()
            .map_err(|e| anyhow::anyhow!(e))?,
    )?;
    let signed_tx = signed_tx.verify()?;
    let hash = *signed_tx.digest();

    let response = context
        .quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: signed_tx,
            request_type: QuorumDriverRequestType::ImmediateReturn,
        })
        .await?;

    Ok(match response {
        QuorumDriverResponse::ImmediateReturn => TransactionIdentifierResponse {
            transaction_identifier: TransactionIdentifier { hash },
            metadata: None,
        },
        // Should not happen
        _ => return Err(Error::new(InternalError)),
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
    let sender = request
        .operations
        .first()
        .and_then(|op| op.account.clone())
        .ok_or_else(|| Error::new(ErrorType::MalformedOperationError))?
        .address;
    Ok(ConstructionPreprocessResponse {
        options: Some(MetadataOptions { sender }),
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
        context
            .state
            .get_owner_objects(Owner::AddressOwner(option.sender))?
            .iter()
            .filter(|info| info.type_.is_gas_coin())
            .map(|info| info.into())
            .collect::<Vec<_>>()
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
        tx.into_data().data
    } else {
        TransactionData::from_signable_bytes(
            &request
                .transaction
                .to_vec()
                .map_err(|e| anyhow::anyhow!(e))?,
        )?
    };
    let account_identifier_signers = if request.signed {
        vec![AccountIdentifier {
            address: data.signer(),
        }]
    } else {
        vec![]
    };
    let operations = Operation::from_data(&data)?;

    Ok(ConstructionParseResponse {
        operations,
        account_identifier_signers,
        metadata: None,
    })
}
