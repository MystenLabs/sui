// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::sync::Arc;

use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::hash::HashFunction;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    ExecuteTransactionRequest, SimulateTransactionRequest, UserSignature,
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
use sui_types::transaction::TransactionDataAPI;

use crate::errors::Error;
use crate::operations::reconstruct_operations;
use crate::types::internal_operation::{PayCoin, TransactionObjectData, TryConstructTransaction};
use crate::types::transaction_envelope;
use crate::types::{
    Amount, AuxData, ConstructionCombineRequest, ConstructionCombineResponse,
    ConstructionDeriveRequest, ConstructionDeriveResponse, ConstructionHashRequest,
    ConstructionMetadata, ConstructionMetadataRequest, ConstructionMetadataResponse,
    ConstructionParseRequest, ConstructionParseResponse, ConstructionPayloadsRequest,
    ConstructionPayloadsResponse, ConstructionPreprocessRequest, ConstructionPreprocessResponse,
    ConstructionSubmitRequest, InternalOperation, MetadataOptions, RosettaTransaction,
    SignatureType, SigningPayload, TransactionIdentifier, TransactionIdentifierResponse,
};
use crate::{OnlineServerContext, SuiEnv};
use move_core_types::language_storage::TypeTag;

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

    let internal = request.operations.into_internal()?;
    let data = internal.clone().try_into_data(metadata)?;
    let wrapper = RosettaTransaction {
        transaction: transaction_envelope::encode_inner_proto(&data),
        signatures: vec![],
        aux: internal.aux(),
    };
    let unsigned = transaction_envelope::encode(&wrapper)?;

    let sender = data.sender();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), data);
    let mut hasher = DefaultHash::default();
    bcs::serialize_into(&mut hasher, &intent_msg).expect("Message serialization should not fail");
    let digest = hasher.finalize().digest;

    Ok(ConstructionPayloadsResponse {
        unsigned_transaction: unsigned,
        payloads: vec![SigningPayload {
            account_identifier: sender.into(),
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

    let unsigned = transaction_envelope::decode(&request.unsigned_transaction)?;
    let proto = transaction_envelope::decode_inner_proto(&unsigned.transaction)?;
    let data = transaction_envelope::proto_to_transaction_data(proto)?;

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
    let generic_sig_bytes = [&*flag, &*sig_bytes, &*pub_key].concat();
    let generic_sig = GenericSignature::from_bytes(&generic_sig_bytes)?;

    let signed_tx =
        sui_types::transaction::Transaction::from_generic_sig_data(data, vec![generic_sig]);
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

    // Pass the unchanged proto bytes and aux data through to the signed
    // wrapper. Signatures live alongside; the inner transaction (and its
    // aux data) is identical to what came in.
    let signed_wrapper = RosettaTransaction {
        transaction: unsigned.transaction,
        signatures: vec![Hex::from_bytes(&generic_sig_bytes)],
        aux: unsigned.aux,
    };

    Ok(ConstructionCombineResponse {
        signed_transaction: transaction_envelope::encode(&signed_wrapper)?,
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

    let wrapper = transaction_envelope::decode(&request.signed_transaction)?;
    if wrapper.signatures.is_empty() {
        return Err(Error::DataError(
            "cannot submit an unsigned transaction: wrapper carries no signatures".to_string(),
        ));
    }
    // The wire-form proto has structured fields populated and bcs cleared,
    // and gRPC accepts the structured form directly.
    let proto_transaction = transaction_envelope::decode_inner_proto(&wrapper.transaction)?;

    // Carry signatures straight from the wrapper.
    let signatures = wrapper
        .signatures
        .iter()
        .map(|sig_hex| {
            let bytes = sig_hex.to_vec()?;
            let generic = GenericSignature::from_bytes(&bytes)?;
            Ok::<UserSignature, Error>(UserSignature::from(&generic))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    // According to RosettaClient.rosseta_flow() (see tests), this transaction has already passed
    // through a dry_run with a possibly invalid budget (metadata endpoint), but the requirements
    // are that it should pass from there and fail here.
    //
    // The balance-change read mask lets us verify, online, that a PayCoin
    // wrapper's currency label matches the coin the transaction actually
    // moves — the one part of the aux data that is fundamentally
    // unverifiable offline (the source coin's on-chain type is not in the PTB).
    let request = SimulateTransactionRequest::new(proto_transaction.clone())
        .with_read_mask(FieldMask::from_paths([
            "transaction.effects.status",
            "transaction.balance_changes",
        ]))
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

    // Close the offline label-vs-reality gap (§7.7): if the wrapper claims
    // a PayCoin currency, require the simulated balance changes to contain a
    // non-SUI delta of that exact coin type. Otherwise the currency label
    // disagrees with what the transaction actually moves — reject before
    // broadcast. FSS validator / AtMost-cap online verification is deferred for
    // v1 (those labels are display-only — the signed PTB determines execution,
    // and `/block` re-derives the truth from chain).
    verify_pay_coin_currency(
        &wrapper.aux,
        response
            .transaction()
            .balance_changes()
            .iter()
            .map(|bc| bc.coin_type()),
    )?;

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

    let wrapper = transaction_envelope::decode(&request.signed_transaction)?;
    if wrapper.signatures.is_empty() {
        return Err(Error::DataError(
            "cannot hash an unsigned transaction: wrapper carries no signatures".to_string(),
        ));
    }
    let proto = transaction_envelope::decode_inner_proto(&wrapper.transaction)?;
    let data = transaction_envelope::proto_to_transaction_data(proto)?;

    // sui_types::transaction::Transaction::digest() is a hash over
    // bcs(TransactionData) with the intent prefix — signatures don't affect it.
    // Reconstruct the signatures purely to satisfy the constructor; the digest
    // would be identical with a dummy signature too.
    let signatures = wrapper
        .signatures
        .iter()
        .map(|sig_hex| {
            let bytes = sig_hex.to_vec()?;
            GenericSignature::from_bytes(&bytes).map_err(Error::from)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let tx = sui_types::transaction::Transaction::from_generic_sig_data(data, signatures);

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
        address_balance_withdrawal,
        fss_object_count,
        redeem_token_amount,
        redeem_plan,
        bind_epoch,
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

    // Fetch epoch and chain_id for address-balance gas transactions.
    //
    // Prefer `bind_epoch` (atomic with the rate snapshot from
    // `get_validator_set_snapshot`) over a separate `get_current_epoch` RPC.
    // If both are needed and `bind_epoch` is set, reusing it both saves an
    // RPC and guarantees `metadata.epoch == bind_epoch`. Without this, an
    // epoch transition between the two RPCs would leave them disagreeing,
    // causing the bind-epoch mismatch check at signing time to reject the
    // metadata even though both reads were individually valid.
    let needs_address_balance_metadata = gas_coins.is_empty() || address_balance_withdrawal > 0;
    let (epoch, chain_id) = if needs_address_balance_metadata {
        let epoch = match bind_epoch {
            Some(e) => e,
            None => crate::get_current_epoch(&mut context.client.clone()).await?,
        };
        let chain_id_str =
            sui_types::digests::CheckpointDigest::new(*context.chain_id.as_bytes()).base58_encode();
        (Some(epoch), Some(chain_id_str))
    } else {
        (None, None)
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
            address_balance_withdrawal,
            epoch,
            chain_id,
            fss_object_count,
            redeem_token_amount,
            redeem_plan,
            bind_epoch,
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

    // /parse reconstructs operations *from the transaction* (the same parser
    // the indexing/`/block` path uses), then applies the wrapper's aux data
    // (the labels the PTB cannot encode). The PTB fields are signature-covered;
    // the aux-data labels are server-supplied (PayCoin currency is verified
    // online in `/submit`, FSS labels are display-only).
    let wrapper = transaction_envelope::decode(&request.transaction)?;
    let (aux, transaction, signed) = (wrapper.aux, wrapper.transaction, request.signed);

    let proto = transaction_envelope::decode_inner_proto(&transaction)?;
    let operations = reconstruct_operations(&proto, &aux, None)?;

    // Signers come from the transaction sender, never the aux data.
    let account_identifier_signers = if signed {
        vec![
            SuiAddress::from_str(proto.sender())
                .map_err(|e| Error::DataError(format!("invalid transaction sender: {e}")))?
                .into(),
        ]
    } else {
        vec![]
    };

    // Force a full `TransactionData` decode so envelopes with a valid-looking
    // `kind` but malformed gas payment / expiration / etc. are rejected here
    // rather than at `/hash` / `/combine` / `/submit`. `/parse` is the spec's
    // sanity check; it must surface structural decode failures the same way
    // the downstream endpoints would.
    let _ = transaction_envelope::proto_to_transaction_data(proto)?;

    Ok(ConstructionParseResponse {
        operations,
        account_identifier_signers,
        metadata: None,
    })
}

/// For a `PayCoin` aux-data label, require that the transaction's (simulated)
/// balance changes actually move a non-SUI coin of the labelled type. This is
/// the one part of the aux data that cannot be verified offline — the source
/// coin's on-chain type is not encoded in the PTB — so it is checked online in
/// `/submit` against the simulate response. Non-`PayCoin` aux data is a
/// no-op.
fn verify_pay_coin_currency<'a>(
    aux: &AuxData,
    moved_coin_types: impl IntoIterator<Item = &'a str>,
) -> Result<(), Error> {
    let AuxData::PayCoin { currency } = aux else {
        return Ok(());
    };
    let want = TypeTag::from_str(&currency.metadata.coin_type)
        .map_err(|e| Error::DataError(format!("invalid PayCoin currency coin_type: {e}")))?;
    let sui = TypeTag::from_str("0x2::sui::SUI").expect("0x2::sui::SUI is a valid type tag");
    let matched = moved_coin_types.into_iter().any(|ct| {
        TypeTag::from_str(ct)
            .map(|t| t == want && t != sui)
            .unwrap_or(false)
    });
    if matched {
        Ok(())
    } else {
        Err(Error::DataError(format!(
            "PayCoin currency {} does not match any non-SUI balance change in the simulated \
             transaction",
            currency.metadata.coin_type
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Currency, CurrencyMetadata};

    fn pay_coin(coin_type: &str) -> AuxData {
        AuxData::PayCoin {
            currency: Currency {
                symbol: "USDC".to_string(),
                decimals: 6,
                metadata: CurrencyMetadata {
                    coin_type: coin_type.to_string(),
                },
            },
        }
    }

    /// §12 test 14: the `/submit` PayCoin currency check accepts a balance
    /// change of the labelled coin type and rejects when only SUI / a
    /// different coin moves.
    #[test]
    fn test_verify_pay_coin_currency() {
        let usdc = "0x5::usdc::USDC";
        let aux = pay_coin(usdc);

        // Labelled coin present among the balance changes → ok.
        assert!(verify_pay_coin_currency(&aux, ["0x2::sui::SUI", usdc]).is_ok());

        // A different non-SUI coin moves (currency label disagrees with
        // reality) → reject.
        let err = verify_pay_coin_currency(&aux, ["0x2::sui::SUI", "0x9::other::OTHER"])
            .expect_err("mismatched currency must be rejected");
        assert!(format!("{err:?}").contains("does not match"));

        // Only SUI moves → reject.
        assert!(verify_pay_coin_currency(&aux, ["0x2::sui::SUI"]).is_err());

        // A PayCoin label that (wrongly) names SUI never matches — SUI is
        // explicitly excluded as a non-SUI delta.
        assert!(verify_pay_coin_currency(&pay_coin("0x2::sui::SUI"), ["0x2::sui::SUI"]).is_err());

        // Non-PayCoin aux data is a no-op regardless of balance changes.
        assert!(verify_pay_coin_currency(&AuxData::None, std::iter::empty()).is_ok());
    }
}
