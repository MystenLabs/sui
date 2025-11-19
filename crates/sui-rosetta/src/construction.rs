// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::hash::HashFunction;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    Bcs, ExecuteTransactionRequest, SimulateTransactionRequest, Transaction, UserSignature,
    simulate_transaction_request::TransactionChecks,
};

use shared_crypto::intent::{Intent, IntentMessage};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{DefaultHash, SignatureScheme, ToFromBytes};
use sui_types::digests::TransactionDigest;
use sui_types::signature::{GenericSignature, VerifyParams};
use sui_types::signature_verification::{
    VerifiedDigestCache, verify_sender_signed_data_message_signatures,
};
use sui_types::transaction::{TransactionData, TransactionDataAPI};

use crate::errors::Error;
use crate::operations::Operations;
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

// This module implements the [Mesh Construction API](https://docs.cdp.coinbase.com/mesh/mesh-api-spec/api-reference#construction)

/// Derive returns the AccountIdentifier associated with a public key.
///
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/construction/derive-accountidentifier-from-publickey)
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
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/construction/generate-unsigned-transaction-and-signing-payloads)
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
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/construction/create-network-transaction-from-signatures)
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
    let flag = vec![
        match sig.signature_type {
            SignatureType::Ed25519 => SignatureScheme::ED25519,
            SignatureType::Ecdsa => SignatureScheme::Secp256k1,
        }
        .flag(),
    ];

    let signed_tx = sui_types::transaction::Transaction::from_generic_sig_data(
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
        // TODO: This will fail for tx sent from aliased addresses.
        vec![],
    )?;
    let signed_tx_bytes = bcs::to_bytes(&signed_tx)?;

    Ok(ConstructionCombineResponse {
        signed_transaction: Hex::from_bytes(&signed_tx_bytes),
    })
}

/// Submit a pre-signed transaction to the node.
///
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/construction/submit-signed-transaction)
pub async fn submit(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionSubmitRequest>, Error>,
) -> Result<TransactionIdentifierResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let signed_tx: sui_types::transaction::Transaction =
        bcs::from_bytes(&request.signed_transaction.to_vec()?)?;

    let signatures = signed_tx
        .tx_signatures()
        .iter()
        .cloned()
        .map(UserSignature::from)
        .collect();

    let tx_data = signed_tx.into_data().into_inner().intent_message.value;
    let proto_transaction =
        Transaction::default().with_bcs(Bcs::default().with_value(bcs::to_bytes(&tx_data)?));

    // According to RosettaClient.rosseta_flow() (see tests), this transaction has already passed
    // through a dry_run with a possibly invalid budget (metadata endpoint), but the requirements
    // are that it should pass from there and fail here.
    let request = SimulateTransactionRequest::new(proto_transaction.clone())
        .with_read_mask(FieldMask::from_paths(["transaction.effects.status"]))
        .with_checks(TransactionChecks::Enabled)
        .with_do_gas_selection(false);

    let response = context
        .client
        .clone()
        .execution_client()
        .simulate_transaction(request)
        .await?
        .into_inner();

    let effects = response.transaction().effects();

    if !effects.status().success() {
        return Err(Error::TransactionDryRunError(Box::new(
            effects.status().error().clone(),
        )));
    };

    let mut client = context.client.clone();
    let mut execution_client = client.execution_client();

    let exec_request = ExecuteTransactionRequest::default()
        .with_transaction(proto_transaction)
        .with_signatures(signatures)
        .with_read_mask(FieldMask::from_paths(["*"]));

    let grpc_response = execution_client
        .execute_transaction(exec_request)
        .await?
        .into_inner();

    let transaction = grpc_response.transaction();
    let effects = transaction.effects();
    if !effects.status().success() {
        return Err(Error::TransactionExecutionError(Box::new(
            effects.status().error().clone(),
        )));
    }

    let digest = transaction
        .digest()
        .parse::<TransactionDigest>()
        .map_err(|e| Error::DataError(format!("Invalid transaction digest: {}", e)))?;

    Ok(TransactionIdentifierResponse {
        transaction_identifier: TransactionIdentifier { hash: digest },
        metadata: None,
    })
}

/// Preprocess is called prior to /construction/payloads to construct a request for any metadata
/// that is needed for transaction construction given (i.e. account nonce).
///
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/construction/create-request-to-fetch-metadata)
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
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/construction/get-hash-of-signed-transaction)
pub async fn hash(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionHashRequest>, Error>,
) -> Result<TransactionIdentifierResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let tx_bytes = request.signed_transaction.to_vec()?;
    let tx: sui_types::transaction::Transaction = bcs::from_bytes(&tx_bytes)?;

    Ok(TransactionIdentifierResponse {
        transaction_identifier: TransactionIdentifier { hash: *tx.digest() },
        metadata: None,
    })
}

/// Get any information required to construct a transaction for a specific network.
/// For Sui, we are returning the latest object refs for all the input objects,
/// which will be used in transaction construction.
///
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/construction/get-metadata-for-transaction-construction)
pub async fn metadata(
    State(mut context): State<OnlineServerContext>,
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

    let mut gas_price = context.client.get_reference_gas_price().await?;
    // make sure it works over epoch changes
    gas_price += 100;

    // Check operation type before moving it
    let is_pay_sui_or_stake = matches!(
        &option.internal_operation,
        InternalOperation::PaySui(_) | InternalOperation::Stake(_)
    );

    let TransactionObjectData {
        gas_coins,
        objects,
        party_objects,
        total_sui_balance,
        budget,
    } = option
        .internal_operation
        .try_fetch_needed_objects(&mut context.client.clone(), Some(gas_price), budget)
        .await?;

    // For backwards compatibility during rolling deployments, populate extra_gas_coins.
    // Old clients expect this field to be present.
    // For PaySui/Stake: extra_gas_coins contains the coins to merge (same as objects)
    // For PayCoin/WithdrawStake: extra_gas_coins is empty
    let extra_gas_coins = if is_pay_sui_or_stake {
        objects.clone()
    } else {
        vec![]
    };

    Ok(ConstructionMetadataResponse {
        metadata: ConstructionMetadata {
            sender,
            gas_coins,
            extra_gas_coins,
            objects,
            party_objects,
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
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/construction/parse-transaction)
pub async fn parse(
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<ConstructionParseRequest>, Error>,
) -> Result<ConstructionParseResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;

    let (data, sender) = if request.signed {
        let tx: sui_types::transaction::Transaction =
            bcs::from_bytes(&request.transaction.to_vec()?)?;
        let intent = tx.into_data().intent_message().value.clone();
        let sender = intent.sender();
        (intent, sender)
    } else {
        let intent: IntentMessage<TransactionData> =
            bcs::from_bytes(&request.transaction.to_vec()?)?;
        let sender = intent.value.sender();
        (intent.value, sender)
    };
    let account_identifier_signers = if request.signed {
        vec![sender.into()]
    } else {
        vec![]
    };
    let proto_tx: Transaction = data.into();
    let tx_kind = proto_tx
        .kind
        .ok_or_else(|| Error::DataError("Transaction missing kind".to_string()))?;
    let operations = Operations::new(Operations::from_transaction(tx_kind, sender, None)?);
    Ok(ConstructionParseResponse {
        operations,
        account_identifier_signers,
        metadata: None,
    })
}
