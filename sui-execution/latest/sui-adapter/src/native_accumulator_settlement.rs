// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Native execution of `TransactionKind::AccumulatorSettlement`.
//!
//! The objects written here must be identical to the ones that `0x2::accumulator_settlement` and
//! `0x2::accumulator_metadata` write for the same updates, so that both forms produce the same
//! effects.

use std::collections::{BTreeMap, HashSet};

use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag, u256::U256};
use serde::{Serialize, de::DeserializeOwned};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID, TypeTag,
    accumulator_metadata::AccumulatorObjectCountKey,
    accumulator_root::{
        ACCUMULATOR_METADATA_MODULE, ACCUMULATOR_ROOT_SETTLE_U128_FUNC,
        ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC, ACCUMULATOR_ROOT_SETTLEMENT_SETTLE_EVENTS_FUNC,
        ACCUMULATOR_SETTLEMENT_MODULE, AccumulatorKey, EventStreamHead, U128,
    },
    accumulator_settlement::{
        AccumulatorBarrierV1, AccumulatorSettlementTransaction, AccumulatorSettlementV1,
        AccumulatorUpdateGroup, BalanceUpdate, EventStreamUpdate,
    },
    balance::{BALANCE_MODULE_NAME, BALANCE_STRUCT_NAME, Balance},
    base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest},
    digests::Digest,
    dynamic_field::{DynamicFieldInfo, Field, derive_dynamic_field_id},
    effects::AccumulatorOperation,
    error::ExecutionError,
    execution::{DynamicallyLoadedObjectMetadata, ExecutionResultsV2},
    execution_status::ExecutionErrorKind,
    id::UID,
    invariant_violation, make_invariant_violation,
    object::{MoveObject, Object, Owner},
    storage::RuntimeObjectResolver,
    transaction::{
        Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
        SharedObjectMutability,
    },
};

use mysten_common::debug_fatal;

use crate::temporary_store::TemporaryStore;

const RECORD_ACCUMULATOR_OBJECT_CHANGES_FUNC: &IdentStr =
    ident_str!("record_accumulator_object_changes");

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

#[cfg(debug_assertions)]
thread_local! {
    static EXECUTE_PTB_SETTLEMENTS_IN_MOVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with programmable settlement transactions executed by their Move calls. Debug builds
/// use this to check that both forms produce the same effects.
#[cfg(debug_assertions)]
pub(crate) fn with_ptb_settlements_in_move<R>(f: impl FnOnce() -> R) -> R {
    EXECUTE_PTB_SETTLEMENTS_IN_MOVE.set(true);
    let _reset = scopeguard::guard((), |()| EXECUTE_PTB_SETTLEMENTS_IN_MOVE.set(false));
    f()
}

/// Returns the native form of `pt` if it is a settlement or barrier that executes natively.
pub(crate) fn native_form_of_ptb_settlement(
    pt: &ProgrammableTransaction,
) -> Option<AccumulatorSettlementTransaction> {
    #[cfg(debug_assertions)]
    if EXECUTE_PTB_SETTLEMENTS_IN_MOVE.get() {
        return None;
    }
    let native = from_programmable_transaction(pt);
    // Only settlement transactions take the accumulator root, and the settlement builder only
    // produces convertible ones.
    if native.is_none()
        && pt.inputs.iter().any(|input| {
            matches!(input, CallArg::Object(ObjectArg::SharedObject { id, .. }) if *id == SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        })
    {
        debug_fatal!("settlement transaction has no native form: {pt:?}");
    }
    native
}

/// Converts a settlement or barrier built as a programmable system transaction into its native
/// form. Returns `None` unless `pt` has exactly the shape that the settlement builder produces and
/// executing the native form writes the same objects as the Move calls.
pub fn from_programmable_transaction(
    pt: &ProgrammableTransaction,
) -> Option<AccumulatorSettlementTransaction> {
    let parser = PtbParser { pt };
    let (prologue, rest) = pt.commands.split_first()?;
    let prologue = parser.move_call(
        prologue,
        ACCUMULATOR_SETTLEMENT_MODULE,
        ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC,
        0,
        6,
    )?;
    let root = prologue.arguments[0];
    let (root_initial_shared_version, mutability) = parser.root(root)?;
    let epoch: u64 = parser.pure(prologue.arguments[1])?;
    let checkpoint_height: u64 = parser.pure(prologue.arguments[2])?;
    let idx: u64 = parser.pure(prologue.arguments[3])?;
    let input_sui: u64 = parser.pure(prologue.arguments[4])?;
    let output_sui: u64 = parser.pure(prologue.arguments[5])?;

    match mutability {
        SharedObjectMutability::Mutable => {
            let [command] = rest else { return None };
            let call = parser.move_call(
                command,
                ACCUMULATOR_METADATA_MODULE,
                RECORD_ACCUMULATOR_OBJECT_CHANGES_FUNC,
                0,
                3,
            )?;
            // The native barrier records no SUI flows.
            if call.arguments[0] != root || input_sui != 0 || output_sui != 0 {
                return None;
            }
            Some(AccumulatorSettlementTransaction::BarrierV1(
                AccumulatorBarrierV1 {
                    epoch,
                    checkpoint_height,
                    num_settlements: idx,
                    objects_created: parser.pure(call.arguments[1])?,
                    objects_destroyed: parser.pure(call.arguments[2])?,
                    accumulator_root_initial_shared_version: root_initial_shared_version,
                },
            ))
        }
        SharedObjectMutability::NonExclusiveWrite => {
            let mut balance_groups = BTreeMap::<TypeTag, Vec<BalanceUpdate>>::new();
            let mut event_stream_updates = vec![];
            // Native execution reads each field once, so a field updated by two calls must run
            // in Move, where the second call sees the first call's write.
            let mut balance_keys = HashSet::new();
            let mut stream_ids = HashSet::new();
            let mut checkpoint_seq = None;

            for command in rest {
                if let Some(call) = parser.move_call(
                    command,
                    ACCUMULATOR_SETTLEMENT_MODULE,
                    ACCUMULATOR_ROOT_SETTLE_U128_FUNC,
                    1,
                    4,
                ) {
                    if call.arguments[0] != root {
                        return None;
                    }
                    let coin_type = balance_coin_type(call.type_arguments[0].to_type_tag().ok()?)?;
                    let owner: SuiAddress = parser.pure(call.arguments[1])?;
                    let merge: u128 = parser.pure(call.arguments[2])?;
                    let split: u128 = parser.pure(call.arguments[3])?;
                    // `settle_u128` aborts unless exactly one of the amounts is nonzero.
                    let (operation, amount) = match (merge, split) {
                        (0, 0) => return None,
                        (amount, 0) => (AccumulatorOperation::Merge, amount),
                        (0, amount) => (AccumulatorOperation::Split, amount),
                        _ => return None,
                    };
                    if !balance_keys.insert((coin_type.clone(), owner)) {
                        return None;
                    }
                    balance_groups
                        .entry(coin_type)
                        .or_default()
                        .push(BalanceUpdate {
                            owner,
                            operation,
                            amount,
                        });
                } else {
                    let call = parser.move_call(
                        command,
                        ACCUMULATOR_SETTLEMENT_MODULE,
                        ACCUMULATOR_ROOT_SETTLEMENT_SETTLE_EVENTS_FUNC,
                        0,
                        5,
                    )?;
                    if call.arguments[0] != root {
                        return None;
                    }
                    let stream_id: SuiAddress = parser.pure(call.arguments[1])?;
                    let merkle_root: U256 = parser.pure(call.arguments[2])?;
                    let event_count: u64 = parser.pure(call.arguments[3])?;
                    let seq: u64 = parser.pure(call.arguments[4])?;
                    // The native form carries one checkpoint sequence number for all streams.
                    if *checkpoint_seq.get_or_insert(seq) != seq || !stream_ids.insert(stream_id) {
                        return None;
                    }
                    event_stream_updates.push(EventStreamUpdate {
                        stream_id,
                        merkle_root: Digest::new(merkle_root.to_le_bytes()),
                        event_count,
                    });
                }
            }

            let mut updates: Vec<_> = balance_groups
                .into_iter()
                .map(|(coin_type, updates)| AccumulatorUpdateGroup::Balance { coin_type, updates })
                .collect();
            if !event_stream_updates.is_empty() {
                updates.push(AccumulatorUpdateGroup::EventStream(event_stream_updates));
            }
            Some(AccumulatorSettlementTransaction::SettlementV1(
                AccumulatorSettlementV1 {
                    epoch,
                    checkpoint_height,
                    idx,
                    checkpoint_seq: checkpoint_seq.unwrap_or(0),
                    input_sui,
                    output_sui,
                    accumulator_root_initial_shared_version: root_initial_shared_version,
                    updates,
                },
            ))
        }
        SharedObjectMutability::Immutable => None,
    }
}

/// Returns `C` if `ty` is `0x2::balance::Balance<C>`.
fn balance_coin_type(ty: TypeTag) -> Option<TypeTag> {
    let TypeTag::Struct(tag) = ty else {
        return None;
    };
    let StructTag {
        address,
        module,
        name,
        mut type_params,
    } = *tag;
    if address != SUI_FRAMEWORK_ADDRESS
        || module.as_ident_str() != BALANCE_MODULE_NAME
        || name.as_ident_str() != BALANCE_STRUCT_NAME
        || type_params.len() != 1
    {
        return None;
    }
    type_params.pop()
}

struct PtbParser<'a> {
    pt: &'a ProgrammableTransaction,
}

impl<'a> PtbParser<'a> {
    /// Returns `command` if it calls `0x2::module::function` with the given numbers of type
    /// arguments and arguments.
    fn move_call(
        &self,
        command: &'a Command,
        module: &IdentStr,
        function: &IdentStr,
        num_type_arguments: usize,
        num_arguments: usize,
    ) -> Option<&'a ProgrammableMoveCall> {
        let Command::MoveCall(call) = command else {
            return None;
        };
        (call.package == SUI_FRAMEWORK_PACKAGE_ID
            && call.module == module.as_str()
            && call.function == function.as_str()
            && call.type_arguments.len() == num_type_arguments
            && call.arguments.len() == num_arguments)
            .then_some(call.as_ref())
    }

    /// Returns the initial shared version and mutability of the accumulator root input.
    fn root(&self, arg: Argument) -> Option<(SequenceNumber, SharedObjectMutability)> {
        let Argument::Input(i) = arg else {
            return None;
        };
        match self.pt.inputs.get(i as usize)? {
            CallArg::Object(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutability,
            }) if *id == SUI_ACCUMULATOR_ROOT_OBJECT_ID => {
                Some((*initial_shared_version, *mutability))
            }
            _ => None,
        }
    }

    fn pure<T: DeserializeOwned>(&self, arg: Argument) -> Option<T> {
        let Argument::Input(i) = arg else {
            return None;
        };
        match self.pt.inputs.get(i as usize)? {
            CallArg::Pure(bytes) => bcs::from_bytes(bytes).ok(),
            _ => None,
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
                let key_type = AccumulatorKey::get_type_tag(&[Balance::type_tag(coin_type)]);
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
