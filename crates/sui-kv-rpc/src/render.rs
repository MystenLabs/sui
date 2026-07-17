// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared proto-rendering layer: turns resolved BigTable data
//! (`CheckpointData`/`TransactionData` + object maps) into the `sui.rpc.v2`
//! proto messages, honoring a `FieldMaskTree`. Used by both the v2 point-get
//! handlers and the list handlers so rendering is identical across
//! them.

use std::collections::HashMap;
use std::sync::Arc;

use move_core_types::language_storage::StructTag;
use mysten_common::ZipDebugEqIteratorExt;
use sui_kvstore::{CheckpointData, TransactionData};
use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::{
    Checkpoint, Event, ExecutedTransaction, Transaction, TransactionEffects, TransactionEvents,
    UserSignature,
};
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::timestamp_ms_to_proto;
use sui_types::TypeTag;
use sui_types::base_types::ObjectID;
use sui_types::full_checkpoint_content::Checkpoint as FullCheckpoint;
use sui_types::full_checkpoint_content::ExecutedTransaction as FullExecutedTransaction;
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::messages_checkpoint::CertifiedCheckpointSummary;
use sui_types::object::Object;
use sui_types::object::rpc_visitor::proto::ProtoVisitor;
use sui_types::storage::ObjectKey;
use tracing::warn;

use crate::PackageResolver;
use crate::object_cache::ObjectMap;

/// Maximum size in bytes for JSON-rendered Move values (1 MiB).
const MAX_JSON_MOVE_VALUE_SIZE: usize = 1024 * 1024;

/// Render a Move value as JSON using the package resolver for type layout.
pub(crate) async fn render_json(
    resolver: &PackageResolver,
    struct_tag: &StructTag,
    contents: &[u8],
) -> Option<prost_types::Value> {
    let type_tag = TypeTag::Struct(Box::new(struct_tag.clone()));
    let layout = resolver.type_layout(type_tag).await.ok()?;
    ProtoVisitor::new(MAX_JSON_MOVE_VALUE_SIZE)
        .deserialize_value(contents, &layout)
        .ok()
}

/// Render a summary-only `CheckpointData` into the proto `Checkpoint` (the fast
/// path: read mask requests neither transactions nor objects).
pub(crate) fn checkpoint_to_response(
    checkpoint: CheckpointData,
    read_mask: &FieldMaskTree,
) -> Result<Checkpoint, RpcError> {
    let summary = checkpoint
        .summary
        .ok_or_else(|| anyhow::anyhow!("checkpoint summary missing"))?;
    let mut message = Checkpoint::default();
    let summary: sui_sdk_types::CheckpointSummary = summary.try_into()?;
    message.merge(&summary, read_mask);

    if read_mask.contains(Checkpoint::SIGNATURE_FIELD) {
        let signatures = checkpoint
            .signatures
            .ok_or_else(|| anyhow::anyhow!("checkpoint signatures missing"))?;
        let signatures: sui_sdk_types::ValidatorAggregatedSignature = signatures.into();
        message.merge(signatures, read_mask);
    }

    if read_mask.contains(Checkpoint::CONTENTS_FIELD.name) {
        let contents = checkpoint
            .contents
            .ok_or_else(|| anyhow::anyhow!("checkpoint contents missing"))?;
        message.merge(
            sui_sdk_types::CheckpointContents::try_from(contents)?,
            read_mask,
        );
    }

    Ok(message)
}

/// Build the full proto `Checkpoint` (summary + signatures + contents +
/// transactions + objects) from resolved BigTable data. `objects` must contain
/// exactly the objects referenced by this checkpoint's transactions — the whole
/// map is folded into the rendered `ObjectSet`. Takes the `ObjectMap` by value
/// so objects can be moved into the `ObjectSet` when the map is uniquely owned
/// (the common case: one map per checkpoint); a shared map falls back to a clone.
pub(crate) fn render_full_checkpoint(
    checkpoint: CheckpointData,
    txs: Vec<TransactionData>,
    objects: ObjectMap,
    read_mask: &FieldMaskTree,
) -> Result<Checkpoint, RpcError> {
    let summary = checkpoint
        .summary
        .ok_or_else(|| RpcError::new(tonic::Code::Internal, "checkpoint summary column missing"))?;
    let cp_seq = summary.sequence_number;
    let signatures = checkpoint.signatures.ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!("checkpoint {cp_seq} signatures column missing"),
        )
    })?;
    let contents = checkpoint.contents.ok_or_else(|| {
        RpcError::new(
            tonic::Code::Internal,
            format!("checkpoint {cp_seq} contents column missing"),
        )
    })?;

    let executed_transactions = txs
        .into_iter()
        .map(|tx| {
            let transaction = tx.transaction_data.ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("transaction {} data column missing", tx.digest),
                )
            })?;
            let effects = tx.effects.ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    format!("transaction {} effects column missing", tx.digest),
                )
            })?;
            Ok::<_, RpcError>(FullExecutedTransaction {
                transaction,
                signatures: tx.signatures.unwrap_or_default(),
                effects,
                events: tx.events,
                unchanged_loaded_runtime_objects: tx.unchanged_loaded_runtime_objects,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut object_set = ObjectSet::default();
    // The map is uniquely owned per checkpoint, so move each `Object` into the
    // set rather than deep-cloning it; a shared map (refcount > 1) falls back
    // to a one-time clone.
    let objects = Arc::try_unwrap(objects).unwrap_or_else(|arc| (*arc).clone());
    for (_, obj) in objects {
        object_set.insert(obj);
    }

    let full_checkpoint = FullCheckpoint {
        summary: CertifiedCheckpointSummary::new_from_data_and_sig(summary, signatures),
        contents,
        transactions: executed_transactions,
        object_set,
    };

    let mut message = Checkpoint::default();
    message.merge(&full_checkpoint, read_mask);
    Ok(message)
}

/// Render a `TransactionData` into the proto `ExecutedTransaction`. `objects`
/// supplies the object types for the transaction's changed/unchanged-consensus
/// objects (keyed by `(id, version)`); keys absent from the map are simply not
/// annotated.
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
        && let Some(tx_data) = source.transaction_data
    {
        let transaction = sui_sdk_types::Transaction::try_from(tx_data)?;
        message.transaction = Some(Transaction::merge_from(transaction, &submask));
    }

    if let Some(submask) = mask.subtree(ExecutedTransaction::SIGNATURES_FIELD.name)
        && let Some(sigs) = source.signatures
    {
        message.signatures = sigs
            .into_iter()
            .map(|s| {
                sui_sdk_types::UserSignature::try_from(s)
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
    use sui_rpc::field::{FieldMask, FieldMaskUtil};
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
