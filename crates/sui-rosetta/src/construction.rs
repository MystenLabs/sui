// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{Extension, Json};
use serde_json::json;
use tracing::info;

use sui_types::base_types::{encode_bytes_hex, SuiAddress};
use sui_types::crypto;
use sui_types::crypto::{SignableBytes, SignatureScheme, ToFromBytes};
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    Transaction, TransactionData,
};
use sui_types::sui_serde::Hex;

use crate::errors::Error;
use crate::operations::Operation;
use crate::types::{
    AccountIdentifier, ConstructionCombineRequest, ConstructionCombineResponse,
    ConstructionDeriveRequest, ConstructionDeriveResponse, ConstructionHashRequest,
    ConstructionMetadataRequest, ConstructionMetadataResponse, ConstructionParseRequest,
    ConstructionParseResponse, ConstructionPayloadsRequest, ConstructionPayloadsResponse,
    ConstructionPreprocessRequest, ConstructionPreprocessResponse, ConstructionSubmitRequest,
    SignatureType, SigningPayload, TransactionIdentifier, TransactionIdentifierResponse,
};
use crate::ErrorType::InternalError;
use crate::ServerContext;

pub async fn derive(
    Json(payload): Json<ConstructionDeriveRequest>,
    Extension(state): Extension<Arc<ServerContext>>,
) -> Result<ConstructionDeriveResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let address: SuiAddress = payload.public_key.try_into()?;
    Ok(ConstructionDeriveResponse {
        account_identifier: AccountIdentifier { address },
    })
}

pub async fn payloads(
    Json(payload): Json<ConstructionPayloadsRequest>,
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<ConstructionPayloadsResponse, Error> {
    info!("payload");
    context.checks_network_identifier(&payload.network_identifier)?;
    let data = Operation::parse_transaction_data(payload.operations, &context.state).await?;
    let hex_bytes = encode_bytes_hex(data.to_bytes());

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

pub async fn combine(
    Json(payload): Json<ConstructionCombineRequest>,
    Extension(state): Extension<Arc<ServerContext>>,
) -> Result<ConstructionCombineResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let unsigned_tx = payload.unsigned_transaction.to_vec()?;
    let data = TransactionData::from_signable_bytes(&unsigned_tx)?;
    let sig = payload.signatures.first().unwrap();
    let sig_bytes = sig.hex_bytes.to_vec()?;
    let pub_key = sig.public_key.hex_bytes.to_vec()?;
    let flag = vec![match sig.signature_type {
        SignatureType::Ed25519 => SignatureScheme::ED25519,
        SignatureType::Ecdsa => SignatureScheme::Secp256k1,
    }
    .flag()];

    let signed_tx = Transaction::new(
        data,
        crypto::Signature::from_bytes(&[&*flag, &*sig_bytes, &*pub_key].concat())?,
    );
    signed_tx.verify_sender_signature()?;
    let signed_tx_bytes = bcs::to_bytes(&signed_tx)?;

    Ok(ConstructionCombineResponse {
        signed_transaction: Hex::from_bytes(&signed_tx_bytes),
    })
}

pub async fn submit(
    Json(payload): Json<ConstructionSubmitRequest>,
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<TransactionIdentifierResponse, Error> {
    context.checks_network_identifier(&payload.network_identifier)?;
    let signed_tx: Transaction = bcs::from_bytes(&payload.signed_transaction.to_vec()?)?;
    signed_tx.verify_sender_signature()?;
    let hash = *signed_tx.digest();

    let response = context
        .quorum_driver
        .execute_transaction(ExecuteTransactionRequest {
            transaction: signed_tx,
            request_type: ExecuteTransactionRequestType::ImmediateReturn,
        })
        .await?;

    Ok(match response {
        ExecuteTransactionResponse::ImmediateReturn => TransactionIdentifierResponse {
            transaction_identifier: TransactionIdentifier { hash },
            metadata: None,
        },
        // Should not happen
        _ => return Err(Error::new(InternalError)),
    })
}

pub async fn preprocess(
    Json(payload): Json<ConstructionPreprocessRequest>,
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<ConstructionPreprocessResponse, Error> {
    context.checks_network_identifier(&payload.network_identifier)?;
    let data = Operation::parse_transaction_data(payload.operations, &context.state).await?;
    let signer = data.signer();
    Ok(ConstructionPreprocessResponse {
        options: Some(json!({})),
        required_public_keys: vec![AccountIdentifier { address: signer }],
    })
}

pub async fn hash(
    Json(payload): Json<ConstructionHashRequest>,
    Extension(state): Extension<Arc<ServerContext>>,
) -> Result<TransactionIdentifierResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let tx_bytes = payload.signed_transaction.to_vec()?;
    let tx: Transaction = bcs::from_bytes(&tx_bytes)?;

    Ok(TransactionIdentifierResponse {
        transaction_identifier: TransactionIdentifier { hash: *tx.digest() },
        metadata: None,
    })
}
pub async fn metadata(
    Json(payload): Json<ConstructionMetadataRequest>,
    Extension(state): Extension<Arc<ServerContext>>,
) -> Result<ConstructionMetadataResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    Ok(ConstructionMetadataResponse {
        metadata: json!({}),
        suggested_fee: vec![],
    })
}

#[axum_macros::debug_handler]
pub async fn parse(
    Json(payload): Json<ConstructionParseRequest>,
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<ConstructionParseResponse, Error> {
    context.checks_network_identifier(&payload.network_identifier)?;

    let data = if payload.signed {
        let tx: Transaction = bcs::from_bytes(&payload.transaction.to_vec()?)?;
        tx.signed_data.data
    } else {
        TransactionData::from_signable_bytes(&payload.transaction.to_vec()?)?
    };
    let account_identifier_signers = if payload.signed {
        vec![AccountIdentifier {
            address: data.signer(),
        }]
    } else {
        vec![]
    };
    let operations = Operation::from_data_and_effect(&data, None, &context.state).await?;

    Ok(ConstructionParseResponse {
        operations,
        account_identifier_signers,
        metadata: None,
    })
}
