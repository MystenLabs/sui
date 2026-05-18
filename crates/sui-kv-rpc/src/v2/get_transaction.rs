// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::ZipDebugEqIteratorExt;
use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;
use sui_kvstore::tables::transactions::col;
use sui_kvstore::{BigTableClient, KeyValueStoreReader, TransactionData};
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetTransactionsRequest, BatchGetTransactionsResponse, Event, ExecutedTransaction,
    GetTransactionRequest, GetTransactionResponse, GetTransactionResult, Transaction,
    TransactionEffects, TransactionEvents, UserSignature,
};
use sui_rpc_api::{
    ErrorReason, RpcError, TransactionNotFoundError,
    proto::google::rpc::bad_request::FieldViolation, proto::timestamp_ms_to_proto,
};
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use tracing::warn;

use super::render_json;
use crate::PackageResolver;

pub const MAX_BATCH_REQUESTS: usize = 200;
pub const READ_MASK_DEFAULT: &str = "digest";

pub(crate) fn validate_read_mask(read_mask: Option<FieldMask>) -> Result<FieldMaskTree, RpcError> {
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
    resolver: &PackageResolver,
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

    let columns = transaction_columns(&read_mask);
    let mut response = client
        .get_transactions_filtered(&[transaction_digest], Some(&columns))
        .await?;
    let transaction = response
        .pop()
        .ok_or(TransactionNotFoundError(transaction_digest.into()))?;
    let objects = if needs_object_types(&read_mask) {
        fetch_object_map(&mut client, std::iter::once(&transaction)).await?
    } else {
        HashMap::new()
    };
    Ok(GetTransactionResponse::new(
        transaction_to_response(transaction, &read_mask, &objects, resolver).await?,
    ))
}

pub async fn batch_get_transactions(
    mut client: BigTableClient,
    BatchGetTransactionsRequest {
        digests, read_mask, ..
    }: BatchGetTransactionsRequest,
    resolver: &PackageResolver,
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
    let columns = transaction_columns(&read_mask);
    let response: HashMap<_, _> = client
        .get_transactions_filtered(&digests, Some(&columns))
        .await?
        .into_iter()
        .map(|tx| (tx.digest, tx))
        .collect();

    let objects = if needs_object_types(&read_mask) {
        fetch_object_map(&mut client, response.values()).await?
    } else {
        HashMap::new()
    };

    let mut transactions = Vec::with_capacity(digests.len());
    for digest in digests {
        if let Some(tx) = response.get(&digest) {
            match transaction_to_response(tx.clone(), &read_mask, &objects, resolver).await {
                Ok(tx) => transactions.push(GetTransactionResult::new_transaction(tx)),
                Err(err) => {
                    transactions.push(GetTransactionResult::new_error(err.into_status_proto()))
                }
            }
        } else {
            let err: RpcError = TransactionNotFoundError(digest.into()).into();
            transactions.push(GetTransactionResult::new_error(err.into_status_proto()));
        }
    }
    Ok(BatchGetTransactionsResponse::new(transactions))
}

pub(crate) async fn transaction_to_response(
    source: TransactionData,
    mask: &FieldMaskTree,
    objects: &HashMap<ObjectKey, Object>,
    resolver: &PackageResolver,
) -> Result<ExecutedTransaction, RpcError> {
    let mut message = ExecutedTransaction::default();

    if mask.contains(ExecutedTransaction::DIGEST_FIELD.name) {
        message.digest = Some(source.digest.to_string());
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::TRANSACTION_FIELD.name)
        && let Some(tx_data) = &source.transaction_data
    {
        let transaction = sui_sdk_types::Transaction::try_from(tx_data.clone())?;
        message.transaction = Some(Transaction::merge_from(transaction, &submask));
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::SIGNATURES_FIELD.name)
        && let Some(sigs) = &source.signatures
    {
        message.signatures = sigs
            .iter()
            .map(|s| {
                sui_sdk_types::UserSignature::try_from(s.clone())
                    .map(|s| UserSignature::merge_from(s, &submask))
            })
            .collect::<Result<_, _>>()?;
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name)
        && let Some(effects) = source.effects
    {
        let mut effects = TransactionEffects::merge_from(
            &sui_sdk_types::TransactionEffects::try_from(effects)?,
            &submask,
        );
        if submask.contains(TransactionEffects::UNCHANGED_LOADED_RUNTIME_OBJECTS_FIELD.name) {
            effects.unchanged_loaded_runtime_objects = source
                .unchanged_loaded_runtime_objects
                .iter()
                .map(Into::into)
                .collect();
        }
        for changed_object in effects.changed_objects.iter_mut() {
            let Ok(object_id) = changed_object.object_id().parse::<ObjectID>() else {
                warn!(
                    object_id = changed_object.object_id(),
                    "failed to parse object_id in changed_objects"
                );
                continue;
            };
            let version = changed_object
                .input_version_opt()
                .unwrap_or_else(|| changed_object.output_version());
            if let Some(object) = objects.get(&ObjectKey(object_id, version.into())) {
                changed_object.set_object_type(object_type_to_string(object.into()));
            }
        }

        for unchanged in effects.unchanged_consensus_objects.iter_mut() {
            let Ok(object_id) = unchanged.object_id().parse::<ObjectID>() else {
                warn!(
                    object_id = unchanged.object_id(),
                    "failed to parse object_id in unchanged_consensus_objects"
                );
                continue;
            };
            if let Some(object) = objects.get(&ObjectKey(object_id, unchanged.version().into())) {
                unchanged.set_object_type(object_type_to_string(object.into()));
            }
        }

        message.effects = Some(effects);
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::EVENTS_FIELD.name)
        && let Some(events) = &source.events
    {
        message.events = Some(TransactionEvents::merge_from(events, &submask));

        if let Some(event_mask) = submask.subtree(TransactionEvents::EVENTS_FIELD.name)
            && event_mask.contains(Event::JSON_FIELD.name)
            && let Some(proto_events) = message.events.as_mut()
        {
            for (proto_event, sui_event) in
                proto_events.events.iter_mut().zip_debug_eq(&events.data)
            {
                proto_event.json = render_json(resolver, &sui_event.type_, &sui_event.contents)
                    .await
                    .map(Box::new);
            }
        }
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

/// Compute the set of BigTable columns needed for the given read mask.
/// Always includes `cn` and `ts` (small metadata).
/// Only includes `td`, `sg`, `ef`, `ev`, `bc`, and `ul` when the corresponding fields
/// are in the mask.
pub(crate) fn transaction_columns(mask: &FieldMaskTree) -> Vec<&'static str> {
    let mut columns = vec![col::CHECKPOINT_NUMBER, col::TIMESTAMP];

    if mask
        .subtree(ExecutedTransaction::TRANSACTION_FIELD.name)
        .is_some()
    {
        columns.push(col::DATA);
    }
    if mask
        .subtree(ExecutedTransaction::SIGNATURES_FIELD.name)
        .is_some()
    {
        columns.push(col::SIGNATURES);
    }
    if let Some(effects_submask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name) {
        columns.push(col::EFFECTS);
        let needs_objects = needs_object_types(mask);
        if effects_submask.contains(TransactionEffects::UNCHANGED_LOADED_RUNTIME_OBJECTS_FIELD.name)
            || needs_objects
        {
            columns.push(col::UNCHANGED_LOADED);
        }
        if needs_objects {
            columns.push(col::DATA);
        }
    }
    if mask
        .subtree(ExecutedTransaction::EVENTS_FIELD.name)
        .is_some()
    {
        columns.push(col::EVENTS);
    }
    if mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD.name) {
        columns.push(col::BALANCE_CHANGES);
    }

    columns
}

pub(crate) fn needs_object_types(mask: &FieldMaskTree) -> bool {
    mask.subtree(ExecutedTransaction::EFFECTS_FIELD.name)
        .is_some_and(|submask| {
            submask.contains(TransactionEffects::CHANGED_OBJECTS_FIELD.name)
                || submask.contains(TransactionEffects::UNCHANGED_CONSENSUS_OBJECTS_FIELD.name)
        })
}

pub(crate) fn compute_object_keys(source: &TransactionData) -> BTreeSet<ObjectKey> {
    match (&source.transaction_data, &source.effects) {
        (Some(tx_data), Some(effects)) => sui_types::storage::get_transaction_object_set(
            tx_data,
            effects,
            &source.unchanged_loaded_runtime_objects,
        ),
        _ => BTreeSet::new(),
    }
}

pub(crate) async fn fetch_object_map<'a>(
    client: &mut BigTableClient,
    transactions: impl Iterator<Item = &'a TransactionData>,
) -> Result<HashMap<ObjectKey, Object>, RpcError> {
    let keys: Vec<_> = transactions
        .flat_map(compute_object_keys)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(client
        .get_objects(&keys)
        .await?
        .into_iter()
        .map(|o| (ObjectKey(o.id(), o.version()), o))
        .collect())
}

fn object_type_to_string(object_type: sui_types::base_types::ObjectType) -> String {
    match object_type {
        sui_types::base_types::ObjectType::Package => "package".to_owned(),
        sui_types::base_types::ObjectType::Struct(move_object_type) => {
            move_object_type.to_canonical_string(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_core_types::account_address::AccountAddress;
    use std::sync::Arc;
    use sui_kvstore::TransactionData as KvTransactionData;
    use sui_package_resolver::{Package, PackageStore, Resolver};
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

    use sui_types::digests::TransactionDigest;

    fn test_tx_data() -> (TransactionDigest, SuiTransactionData) {
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
        let tx = Transaction::new(SenderSignedData::new(data.clone(), vec![]));
        (*tx.digest(), data)
    }

    /// Empty package store for tests that don't exercise JSON rendering.
    struct EmptyPackageStore;

    #[async_trait::async_trait]
    impl PackageStore for EmptyPackageStore {
        async fn fetch(&self, id: AccountAddress) -> sui_package_resolver::Result<Arc<Package>> {
            Err(sui_package_resolver::error::Error::PackageNotFound(id))
        }
    }

    fn test_resolver() -> PackageResolver {
        let store: Arc<dyn PackageStore> = Arc::new(EmptyPackageStore);
        Arc::new(Resolver::new(store))
    }

    #[tokio::test]
    async fn transaction_to_response_returns_balance_changes_when_requested() {
        let (digest, tx_data) = test_tx_data();
        let tx = Transaction::new(SenderSignedData::new(tx_data.clone(), vec![]));
        let effects = TestEffectsBuilder::new(tx.data()).build();
        let balance_change = BalanceChange {
            address: SuiAddress::random_for_testing_only(),
            coin_type: TypeTag::U64,
            amount: 42,
        };
        let source = KvTransactionData {
            digest,
            transaction_data: Some(tx_data),
            signatures: Some(vec![]),
            effects: Some(effects),
            events: None,
            checkpoint_number: 7,
            timestamp: 42,
            balance_changes: vec![balance_change.clone()],
            unchanged_loaded_runtime_objects: vec![],
        };
        let mask = FieldMaskTree::from(FieldMask::from_str("balance_changes"));
        let resolver = test_resolver();

        let response = transaction_to_response(source, &mask, &HashMap::new(), &resolver)
            .await
            .expect("render should succeed");

        assert_eq!(
            response.balance_changes,
            vec![ProtoBalanceChange::from(balance_change)]
        );
    }

    #[tokio::test]
    async fn transaction_to_response_returns_unchanged_loaded_runtime_objects_when_requested() {
        let (digest, tx_data) = test_tx_data();
        let tx = Transaction::new(SenderSignedData::new(tx_data.clone(), vec![]));
        let effects = TestEffectsBuilder::new(tx.data()).build();
        let obj_key = ObjectKey(ObjectID::random(), 3.into());
        let source = KvTransactionData {
            digest,
            transaction_data: Some(tx_data),
            signatures: Some(vec![]),
            effects: Some(effects),
            events: None,
            checkpoint_number: 7,
            timestamp: 42,
            balance_changes: vec![],
            unchanged_loaded_runtime_objects: vec![obj_key],
        };
        let mask = FieldMaskTree::from(FieldMask::from_str(
            "effects.unchanged_loaded_runtime_objects",
        ));
        let resolver = test_resolver();

        let response = transaction_to_response(source, &mask, &HashMap::new(), &resolver)
            .await
            .expect("render should succeed");

        let effects = response.effects.expect("effects should be present");
        assert_eq!(
            effects.unchanged_loaded_runtime_objects,
            vec![ObjectReference::from(&obj_key)]
        );
    }
}
