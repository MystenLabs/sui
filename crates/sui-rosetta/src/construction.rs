// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{Extension, Json};
use serde_json::json;

use sui_sdk::rpc_types::SuiExecuteTransactionResponse;
use sui_types::base_types::SuiAddress;
use sui_types::crypto;
use sui_types::crypto::{SignableBytes, ToFromBytes};
use sui_types::messages::{ExecuteTransactionRequestType, Transaction, TransactionData};
use sui_types::sui_serde::Hex;

use crate::actions::SuiAction;
use crate::errors::Error;
use crate::types::{
    AccountIdentifier, ConstructionCombineRequest, ConstructionCombineResponse,
    ConstructionDeriveRequest, ConstructionDeriveResponse, ConstructionHashRequest,
    ConstructionMetadataRequest, ConstructionMetadataResponse, ConstructionParseRequest,
    ConstructionParseResponse, ConstructionPayloadsRequest, ConstructionPayloadsResponse,
    ConstructionPreprocessRequest, ConstructionPreprocessResponse, ConstructionSubmitRequest,
    SignatureType, SigningPayload, TransactionIdentifier, TransactionIdentifierResponse,
};
use crate::ErrorType::InternalError;
use crate::{ApiState, ErrorType};

pub async fn derive(
    Json(payload): Json<ConstructionDeriveRequest>,
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<ConstructionDeriveResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let address: SuiAddress = payload.public_key.try_into()?;
    Ok(ConstructionDeriveResponse {
        account_identifier: AccountIdentifier { address },
    })
}

pub async fn payload(
    Json(payload): Json<ConstructionPayloadsRequest>,
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<ConstructionPayloadsResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let action: SuiAction = payload.operations.try_into()?;
    let data = action
        .into_data(state.get_client(payload.network_identifier.network).await?)
        .await?;

    let signer = data.signer();
    let hex_bytes = payload
        .public_keys.into_iter().find_map(|pub_key| {
        let key_hex = pub_key.hex_bytes.clone();
        let address: SuiAddress = pub_key.try_into().ok()?;
        if address == signer {
            Some(key_hex)
        } else {
            None
        }
    })
        .ok_or_else(|| Error::new_with_detail(ErrorType::MissingInput, json!({"input": "public keys", "cause":format!("Public key for address [{signer}] not found.")})))?;

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
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<ConstructionCombineResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let unsigned_tx = payload.unsigned_transaction.to_vec()?;
    let data = TransactionData::from_signable_bytes(&unsigned_tx)?;
    let sig = payload.signatures.first().unwrap().hex_bytes.to_vec()?;
    let signed_tx = Transaction::new(data, crypto::Signature::from_bytes(&sig)?);
    signed_tx.verify_sender_signature()?;
    let signed_tx_bytes = bcs::to_bytes(&signed_tx)?;

    Ok(ConstructionCombineResponse {
        signed_transaction: Hex::from_bytes(&signed_tx_bytes),
    })
}

pub async fn submit(
    Json(payload): Json<ConstructionSubmitRequest>,
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<TransactionIdentifierResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let signed_tx: Transaction = bcs::from_bytes(&payload.signed_transaction.to_vec()?)?;
    signed_tx.verify_sender_signature()?;

    let response = state
        .get_client(payload.network_identifier.network)
        .await?
        .quorum_driver()
        .execute_transaction_by_fullnode(signed_tx, ExecuteTransactionRequestType::ImmediateReturn)
        .await?;

    Ok(match response {
        SuiExecuteTransactionResponse::ImmediateReturn { tx_digest } => {
            TransactionIdentifierResponse {
                transaction_identifier: TransactionIdentifier { hash: tx_digest },
                metadata: None,
            }
        }
        // Should not happen
        _ => return Err(Error::new(InternalError)),
    })
}

pub async fn preprocess(
    Json(payload): Json<ConstructionPreprocessRequest>,
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<ConstructionPreprocessResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let action: SuiAction = payload.operations.try_into()?;
    let data = action
        .into_data(state.get_client(payload.network_identifier.network).await?)
        .await?;

    let signer = data.signer();
    Ok(ConstructionPreprocessResponse {
        options: None,
        required_public_keys: vec![AccountIdentifier { address: signer }],
    })
}

pub async fn hash(
    Json(payload): Json<ConstructionHashRequest>,
    Extension(state): Extension<Arc<ApiState>>,
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
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<ConstructionMetadataResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    Ok(ConstructionMetadataResponse {
        metadata: Default::default(),
        suggested_fee: vec![],
    })
}

#[axum_macros::debug_handler]
pub async fn parse(
    Json(payload): Json<ConstructionParseRequest>,
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<ConstructionParseResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;

    let data = if payload.signed {
        let tx: Transaction = bcs::from_bytes(&payload.transaction.to_vec()?)?;
        tx.signed_data.data
    } else {
        TransactionData::from_signable_bytes(&payload.transaction.to_vec()?)?
    };

    let action = SuiAction::from_data(&data)?;

    Ok(ConstructionParseResponse {
        operations: action.try_into()?,
        account_identifier_signers: vec![AccountIdentifier {
            address: data.signer(),
        }],
        metadata: None,
    })
}
