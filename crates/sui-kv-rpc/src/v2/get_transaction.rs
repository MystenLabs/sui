// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::str::FromStr;
use sui_kvstore::{BigTableClient, KeyValueStoreReader, TransactionData};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, ExecutedTransaction,
    GetTransactionRequest, GetTransactionResponse, GetTransactionResult, Transaction,
    TransactionEffects, TransactionEvents, UserSignature,
};
use sui_rpc_api::{
    ErrorReason, RpcError, TransactionNotFoundError,
    proto::google::rpc::bad_request::FieldViolation, proto::timestamp_ms_to_proto,
};
use sui_types::base_types::TransactionDigest;

pub const MAX_BATCH_REQUESTS: usize = 200;
pub const READ_MASK_DEFAULT: &str = "digest";

fn validate_read_mask(read_mask: Option<FieldMask>) -> Result<FieldMaskTree, RpcError> {
    let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
    read_mask
        .validate::<ExecutedTransaction>()
        .map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
    Ok(FieldMaskTree::from(read_mask))
}

pub async fn get_transaction(
    mut client: BigTableClient,
    request: GetTransactionRequest,
) -> Result<GetTransactionResponse, RpcError> {
    let transaction_digest = request
        .digest
        .ok_or_else(|| {
            FieldViolation::new("digest")
                .with_description("missing digest")
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<TransactionDigest>()
        .map_err(|e| {
            FieldViolation::new("digest")
                .with_description(format!("invalid digest: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let read_mask = validate_read_mask(request.read_mask)?;

    let mut response = client.get_transactions(&[transaction_digest]).await?;
    let transaction = response
        .pop()
        .ok_or(TransactionNotFoundError(transaction_digest.into()))?;
    Ok(GetTransactionResponse::new(transaction_to_response(
        transaction,
        &read_mask,
    )?))
}

pub async fn batch_get_transactions(
    mut client: BigTableClient,
    BatchGetTransactionsRequest {
        digests, read_mask, ..
    }: BatchGetTransactionsRequest,
) -> Result<BatchGetTransactionsResponse, RpcError> {
    let read_mask = validate_read_mask(read_mask)?;

    if digests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }

    let digests = digests
        .iter()
        .map(|digest| TransactionDigest::from_str(digest))
        .collect::<Result<Vec<_>, _>>()?;
    let response: HashMap<_, _> = client
        .get_transactions(&digests)
        .await?
        .into_iter()
        .map(|tx| (*tx.transaction.digest(), tx))
        .collect();

    let transactions = digests
        .into_iter()
        .map(|digest| {
            if let Some(tx) = response.get(&digest) {
                return match transaction_to_response(tx.clone(), &read_mask) {
                    Ok(tx) => GetTransactionResult::new_transaction(tx),
                    Err(err) => GetTransactionResult::new_error(err.into_status_proto()),
                };
            }
            let err: RpcError = TransactionNotFoundError(digest.into()).into();
            GetTransactionResult::new_error(err.into_status_proto())
        })
        .collect();
    Ok(BatchGetTransactionsResponse::new(transactions))
}

fn transaction_to_response(
    source: TransactionData,
    mask: &FieldMaskTree,
) -> Result<ExecutedTransaction, RpcError> {
    let mut message = ExecutedTransaction::default();

    if mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
        message.digest = Some(source.transaction.digest().to_string());
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::TRANSACTION_FIELD.name) {
        let transaction =
            sui_sdk_types::Transaction::try_from(source.transaction.transaction_data().clone())?;
        message.transaction = Some(Transaction::merge_from(transaction, &submask));
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::SIGNATURES_FIELD.name) {
        message.signatures = source
            .transaction
            .tx_signatures()
            .iter()
            .map(|s| {
                sui_sdk_types::UserSignature::try_from(s.clone())
                    .map(|s| UserSignature::merge_from(s, &submask))
            })
            .collect::<Result<_, _>>()?;
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name) {
        let mut effects = TransactionEffects::merge_from(
            &sui_sdk_types::TransactionEffects::try_from(source.effects)?,
            &submask,
        );
        if submask.contains(TransactionEffects::UNCHANGED_LOADED_RUNTIME_OBJECTS_FIELD.name) {
            effects.unchanged_loaded_runtime_objects = source
                .unchanged_loaded_runtime_objects
                .iter()
                .map(Into::into)
                .collect();
        }
        // TODO: add support for object_types in the KV store
        message.effects = Some(effects);
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EVENTS_FIELD.name)
        && let Some(events) = source.events
    {
        message.events = Some(TransactionEvents::merge_from(
            sui_sdk_types::TransactionEvents::try_from(events)?,
            &submask,
        ));
        // TODO: add support for JSON layout
    }
    if mask.contains(ExecutedTransaction::CHECKPOINT_FIELD.name) {
        message.checkpoint = Some(source.checkpoint_number);
    }
    if mask.contains(ExecutedTransaction::TIMESTAMP_FIELD.name) {
        message.timestamp = Some(timestamp_ms_to_proto(source.timestamp));
    }

    if mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name) {
        message.balance_changes = source.balance_changes.into_iter().map(Into::into).collect();
    }

    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_kvstore::TransactionData as KvTransactionData;
    use sui_rpc::proto::sui::rpc::v2::BalanceChange as ProtoBalanceChange;
    use sui_rpc::proto::sui::rpc::v2::ObjectReference;
    use sui_types::TypeTag;
    use sui_types::balance_change::BalanceChange;
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::effects::TestEffectsBuilder;
    use sui_types::object::Object;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::storage::ObjectKey;
    use sui_types::transaction::{
        SenderSignedData, Transaction, TransactionData as SuiTransactionData,
    };

    fn test_transaction() -> Transaction {
        let sender = SuiAddress::random_for_testing_only();
        let gas = Object::immutable_with_id_for_testing(ObjectID::random());
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder.transfer_sui(SuiAddress::random_for_testing_only(), None);
            builder.finish()
        };
        let data = SuiTransactionData::new_programmable(
            sender,
            vec![gas.compute_object_reference()],
            pt,
            1_000_000,
            1,
        );
        Transaction::new(SenderSignedData::new(data, vec![]))
    }

    #[test]
    fn transaction_to_response_returns_balance_changes_when_requested() {
        let transaction = test_transaction();
        let effects = TestEffectsBuilder::new(transaction.data()).build();
        let balance_change = BalanceChange {
            address: SuiAddress::random_for_testing_only(),
            coin_type: TypeTag::U64,
            amount: 42,
        };
        let source = KvTransactionData {
            transaction,
            effects,
            events: None,
            checkpoint_number: 7,
            timestamp: 42,
            balance_changes: vec![balance_change.clone()],
            unchanged_loaded_runtime_objects: vec![],
        };
        let mask = FieldMaskTree::from(FieldMask::from_str("balance_changes"));

        let response = transaction_to_response(source, &mask).expect("render should succeed");

        assert_eq!(
            response.balance_changes,
            vec![ProtoBalanceChange::from(balance_change)]
        );
    }

    #[test]
    fn transaction_to_response_returns_unchanged_loaded_runtime_objects_when_requested() {
        let transaction = test_transaction();
        let effects = TestEffectsBuilder::new(transaction.data()).build();
        let obj_key = ObjectKey(ObjectID::random(), 3.into());
        let source = KvTransactionData {
            transaction,
            effects,
            events: None,
            checkpoint_number: 7,
            timestamp: 42,
            balance_changes: vec![],
            unchanged_loaded_runtime_objects: vec![obj_key],
        };
        let mask = FieldMaskTree::from(FieldMask::from_str(
            "effects.unchanged_loaded_runtime_objects",
        ));

        let response = transaction_to_response(source, &mask).expect("render should succeed");

        let effects = response.effects.expect("effects should be present");
        assert_eq!(
            effects.unchanged_loaded_runtime_objects,
            vec![ObjectReference::from(&obj_key)]
        );
    }
}
