// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Native execution of `TransactionKind::AccumulatorSettlement`.
//!
//! The objects written here must be identical to the ones that `0x2::accumulator_settlement` and
//! `0x2::accumulator_metadata` write for the same updates, so that both forms produce the same
//! effects.

use std::collections::BTreeMap;

use move_core_types::u256::U256;
use serde::{Serialize, de::DeserializeOwned};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID, TypeTag,
    accumulator_metadata::AccumulatorObjectCountKey,
    accumulator_root::{AccumulatorKey, EventStreamHead, U128},
    accumulator_settlement::{
        AccumulatorBarrierV1, AccumulatorSettlementTransaction, AccumulatorSettlementV1,
        AccumulatorUpdateGroup, BalanceUpdate, EventStreamUpdate,
    },
    balance::Balance,
    base_types::{ObjectID, SequenceNumber, TransactionDigest},
    dynamic_field::{DynamicFieldInfo, Field, derive_dynamic_field_id},
    effects::AccumulatorOperation,
    error::ExecutionError,
    execution::{DynamicallyLoadedObjectMetadata, ExecutionResultsV2},
    execution_status::ExecutionErrorKind,
    id::UID,
    invariant_violation, make_invariant_violation,
    object::{MoveObject, Object, Owner},
    storage::RuntimeObjectResolver,
};

use crate::temporary_store::TemporaryStore;

pub(crate) fn execute(
    temporary_store: &mut TemporaryStore<'_>,
    protocol_config: &ProtocolConfig,
    tx_digest: TransactionDigest,
    transaction: AccumulatorSettlementTransaction,
) -> Result<(), ExecutionError> {
    match transaction {
        AccumulatorSettlementTransaction::SettlementV1(settlement) => {
            execute_settlement(temporary_store, protocol_config, tx_digest, settlement)
        }
        AccumulatorSettlementTransaction::BarrierV1(barrier) => {
            execute_barrier(temporary_store, protocol_config, tx_digest, barrier)
        }
    }
}

fn execute_settlement(
    temporary_store: &mut TemporaryStore<'_>,
    protocol_config: &ProtocolConfig,
    tx_digest: TransactionDigest,
    settlement: AccumulatorSettlementV1,
) -> Result<(), ExecutionError> {
    let AccumulatorSettlementV1 {
        checkpoint_seq,
        input_sui,
        output_sui,
        updates,
        ..
    } = settlement;

    let mut writer = RootFieldWriter::new(temporary_store, protocol_config, tx_digest)?;
    for group in updates {
        match group {
            AccumulatorUpdateGroup::Balance { coin_type, updates } => {
                let key_type =
                    AccumulatorKey::get_type_tag(&[Balance::type_tag(coin_type.clone())]);
                for update in updates {
                    writer.settle_balance(&key_type, update)?;
                }
            }
            AccumulatorUpdateGroup::EventStream(updates) => {
                let key_type = AccumulatorKey::get_type_tag(&[EventStreamHead::get_type_tag()]);
                for update in updates {
                    writer.settle_event_stream(&key_type, update, checkpoint_seq)?;
                }
            }
        }
    }

    let (mut results, loaded) = writer.finish();
    results.settlement_input_sui = input_sui;
    results.settlement_output_sui = output_sui;
    temporary_store.record_native_settlement_results(results, loaded)
}

fn execute_barrier(
    temporary_store: &mut TemporaryStore<'_>,
    protocol_config: &ProtocolConfig,
    tx_digest: TransactionDigest,
    barrier: AccumulatorBarrierV1,
) -> Result<(), ExecutionError> {
    let AccumulatorBarrierV1 {
        objects_created,
        objects_destroyed,
        ..
    } = barrier;

    let mut writer = RootFieldWriter::new(temporary_store, protocol_config, tx_digest)?;
    let key_type = AccumulatorObjectCountKey::get_type_tag();
    let key = AccumulatorObjectCountKey::default();
    let (id, existing) = writer.load::<AccumulatorObjectCountKey, u64>(&key_type, &key)?;
    let current = existing.as_ref().map_or(0, |(_, field)| field.value);
    let Some(count) = current
        .checked_add(objects_created)
        .and_then(|count| count.checked_sub(objects_destroyed))
    else {
        invariant_violation!(
            "accumulator object count {current} + {objects_created} - {objects_destroyed} is not representable"
        )
    };
    writer.write(
        id,
        &key_type,
        &u64::get_type_tag(),
        key,
        count,
        existing.as_ref().map(|(object, _)| object),
    )?;
    let (results, loaded) = writer.finish();

    // The Move barrier takes the root by `&mut`, which rewrites it unchanged.
    let Some(root) = temporary_store
        .read_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        .cloned()
    else {
        invariant_violation!("accumulator root is not an input of the barrier")
    };
    temporary_store.mutate_input_object(root);
    temporary_store.record_native_settlement_results(results, loaded)
}

/// An existing field object and its deserialized contents.
type LoadedField<K, V> = (Object, Field<K, V>);

/// Reads and writes dynamic fields of the accumulator root, collecting the writes the same way
/// the Move object runtime would.
struct RootFieldWriter<'a, 'b> {
    temporary_store: &'a TemporaryStore<'b>,
    protocol_config: &'a ProtocolConfig,
    tx_digest: TransactionDigest,
    root_version: SequenceNumber,
    results: ExecutionResultsV2,
    loaded: BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
}

impl<'a, 'b> RootFieldWriter<'a, 'b> {
    fn new(
        temporary_store: &'a TemporaryStore<'b>,
        protocol_config: &'a ProtocolConfig,
        tx_digest: TransactionDigest,
    ) -> Result<Self, ExecutionError> {
        let Some(root) = temporary_store.read_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID) else {
            invariant_violation!("accumulator root is not an input of the settlement")
        };
        Ok(Self {
            root_version: root.version(),
            temporary_store,
            protocol_config,
            tx_digest,
            results: ExecutionResultsV2::default(),
            loaded: BTreeMap::new(),
        })
    }

    fn finish(
        self,
    ) -> (
        ExecutionResultsV2,
        BTreeMap<ObjectID, DynamicallyLoadedObjectMetadata>,
    ) {
        (self.results, self.loaded)
    }

    /// Mirrors `accumulator_settlement::settle_u128`.
    fn settle_balance(
        &mut self,
        key_type: &TypeTag,
        update: BalanceUpdate,
    ) -> Result<(), ExecutionError> {
        let BalanceUpdate {
            owner,
            operation,
            amount,
        } = update;
        if amount == 0 {
            invariant_violation!("zero-amount settlement for {owner}")
        }

        let key = AccumulatorKey { owner };
        let (id, existing) = self.load::<AccumulatorKey, U128>(key_type, &key)?;
        let current = existing.as_ref().map_or(0, |(_, field)| field.value.value);
        let new_value = match operation {
            // Each settled transaction deposits at most u64::MAX, so this cannot overflow in
            // practice. Checked anyway because Move aborts on overflow.
            AccumulatorOperation::Merge => current
                .checked_add(amount)
                .ok_or_else(|| make_invariant_violation!("accumulator {id} overflowed"))?,
            AccumulatorOperation::Split => current.checked_sub(amount).ok_or_else(|| {
                ExecutionError::new_with_source(
                    ExecutionErrorKind::InsufficientFundsForWithdraw,
                    format!("settlement splits {amount} from accumulator {id} holding {current}"),
                )
            })?,
        };

        match existing {
            Some(_) if new_value == 0 => self.delete(id),
            existing => self.write(
                id,
                key_type,
                &U128::get_type_tag(),
                key,
                U128 { value: new_value },
                existing.as_ref().map(|(object, _)| object),
            )?,
        }
        Ok(())
    }

    /// Mirrors `accumulator_settlement::settle_events`.
    fn settle_event_stream(
        &mut self,
        key_type: &TypeTag,
        update: EventStreamUpdate,
        checkpoint_seq: u64,
    ) -> Result<(), ExecutionError> {
        let EventStreamUpdate {
            stream_id,
            merkle_root,
            event_count,
        } = update;

        let key = AccumulatorKey { owner: stream_id };
        let (id, existing) = self.load::<AccumulatorKey, EventStreamHead>(key_type, &key)?;
        let (mut head, old_object) = match existing {
            Some((object, field)) => (field.value, Some(object)),
            None => (EventStreamHead::new(), None),
        };
        let merkle_root = U256::from_le_bytes(&merkle_root.into_inner());
        if head
            .append(merkle_root, event_count, checkpoint_seq)
            .is_none()
        {
            invariant_violation!("event count of stream {stream_id} overflowed")
        }
        self.write(
            id,
            key_type,
            &EventStreamHead::get_type_tag(),
            key,
            head,
            old_object.as_ref(),
        )
    }

    /// Loads the field `key` of the root, recording it as a loaded runtime object if it exists.
    fn load<K, V>(
        &mut self,
        key_type: &TypeTag,
        key: &K,
    ) -> Result<(ObjectID, Option<LoadedField<K, V>>), ExecutionError>
    where
        K: Serialize + DeserializeOwned,
        V: DeserializeOwned,
    {
        let key_bytes = bcs::to_bytes(key)
            .map_err(|e| make_invariant_violation!("cannot serialize field key: {e}"))?;
        let id = derive_dynamic_field_id(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key_type, &key_bytes)
            .map_err(|e| make_invariant_violation!("cannot derive field id: {e}"))?;

        let object = self
            .temporary_store
            .read_child_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID, &id, self.root_version)
            .map_err(|e| make_invariant_violation!("cannot read accumulator {id}: {e}"))?;
        let Some(object) = object else {
            return Ok((id, None));
        };

        let Some(field) = object.to_rust::<Field<K, V>>() else {
            invariant_violation!("accumulator {id} has unexpected contents")
        };
        let (_, version, digest) = object.compute_object_reference();
        self.loaded.insert(
            id,
            DynamicallyLoadedObjectMetadata {
                version,
                digest,
                owner: object.owner.clone(),
                storage_rebate: object.storage_rebate,
                previous_transaction: object.previous_transaction,
            },
        );
        Ok((id, Some((object, field))))
    }

    /// Writes `Field { id, name: key, value }` as a child of the root. `old` is the existing
    /// object, if any. Leaves `old` untouched if the contents are unchanged.
    fn write<K, V>(
        &mut self,
        id: ObjectID,
        key_type: &TypeTag,
        value_type: &TypeTag,
        key: K,
        value: V,
        old: Option<&Object>,
    ) -> Result<(), ExecutionError>
    where
        K: Serialize,
        V: Serialize,
    {
        let field = Field {
            id: UID::new(id),
            name: key,
            value,
        };
        let contents = bcs::to_bytes(&field)
            .map_err(|e| make_invariant_violation!("cannot serialize accumulator {id}: {e}"))?;
        // The Move runtime does not write a child whose value is unchanged.
        if old
            .and_then(|o| o.data.try_as_move())
            .is_some_and(|o| o.contents() == contents.as_slice())
        {
            return Ok(());
        }
        let type_ = DynamicFieldInfo::dynamic_field_type(key_type.clone(), value_type.clone());
        // SAFETY: `Field` does not have `store`, so it never has public transfer.
        let move_object = unsafe {
            MoveObject::new_from_execution(
                type_.into(),
                false,
                old.map(|o| o.version()).unwrap_or_default(),
                contents,
                self.protocol_config,
                /* system_mutation */ true,
            )?
        };
        let object = Object::new_move(
            move_object,
            Owner::ObjectOwner(SUI_ACCUMULATOR_ROOT_OBJECT_ID.into()),
            self.tx_digest,
        );

        if old.is_some() {
            self.results.modified_objects.insert(id);
        } else {
            self.results.created_object_ids.insert(id);
        }
        self.results.written_objects.insert(id, object);
        Ok(())
    }

    fn delete(&mut self, id: ObjectID) {
        self.results.modified_objects.insert(id);
        self.results.deleted_object_ids.insert(id);
    }
}
