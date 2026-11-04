// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::indexing_slicing)]
#![deny(clippy::cast_possible_truncation)]

use crate::execution_value::ExecutionState;
use fastcrypto::hash::{Blake2b256, HashFunction};
use move_core_types::{
    language_storage::{StructTag, TypeTag},
    u256::U256,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    sync::LazyLock,
};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    MoveTypeTagTrait, MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
    accumulator_metadata::{
        ACCUMULATOR_METADATA_MODULE, AccumulatorObjectCountKey,
        RECORD_ACCUMULATOR_OBJECT_CHANGES_FUNC,
    },
    accumulator_root::{
        ACCUMULATOR_ROOT_MODULE, ACCUMULATOR_ROOT_SETTLE_U128_FUNC,
        ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC, ACCUMULATOR_ROOT_SETTLEMENT_SETTLE_EVENTS_FUNC,
        ACCUMULATOR_ROOT_STRUCT, ACCUMULATOR_SETTLEMENT_MODULE, AccumulatorKey, EventStreamHead,
        U128,
    },
    base_types::{ObjectID, SequenceNumber, SuiAddress, TxContext},
    dynamic_field::{DynamicFieldInfo, Field, derive_dynamic_field_id},
    error::{ExecutionError, ExecutionErrorTrait},
    execution::{DynamicallyLoadedObjectMetadata, ExecutionResults, ExecutionResultsV2},
    id::UID,
    object::{MoveObject, Object, Owner},
    transaction::{
        Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
        SharedObjectMutability,
    },
};

const ACCUMULATOR_SETTLEMENT_MODULE_NAME: &str = ACCUMULATOR_SETTLEMENT_MODULE.as_str();
const ACCUMULATOR_METADATA_MODULE_NAME: &str = ACCUMULATOR_METADATA_MODULE.as_str();
const SETTLEMENT_PROLOGUE: &str = ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC.as_str();
const SETTLE_U128: &str = ACCUMULATOR_ROOT_SETTLE_U128_FUNC.as_str();
const SETTLE_EVENTS: &str = ACCUMULATOR_ROOT_SETTLEMENT_SETTLE_EVENTS_FUNC.as_str();
const RECORD_ACCUMULATOR_OBJECT_CHANGES: &str = RECORD_ACCUMULATOR_OBJECT_CHANGES_FUNC.as_str();
const ACCUMULATOR_ROOT_STRUCT_NAME: &str = ACCUMULATOR_ROOT_STRUCT.as_str();

struct ChildObject {
    original: Option<Object>,
    current: Option<Object>,
}

struct Context<'a> {
    state: &'a dyn ExecutionState,
    protocol_config: &'a ProtocolConfig,
    tx_context: &'a TxContext,
    inputs: Vec<CallArg>,
    root_argument: Argument,
    root: Object,
    children: BTreeMap<ObjectID, ChildObject>,
    generated_ids: BTreeSet<ObjectID>,
    input_sui: u64,
    output_sui: u64,
}

impl<'a> Context<'a> {
    fn new(
        protocol_config: &'a ProtocolConfig,
        state: &'a dyn ExecutionState,
        tx_context: &'a TxContext,
        inputs: Vec<CallArg>,
    ) -> Result<Self, ExecutionError> {
        assert_invariant!(
            tx_context.sender() == SuiAddress::ZERO,
            "Settlement sender must be the system address"
        );
        let mut root_argument = None;
        for (index, input) in inputs.iter().enumerate() {
            match input {
                CallArg::Pure(_) => (),
                CallArg::Object(ObjectArg::SharedObject {
                    id,
                    mutability:
                        SharedObjectMutability::NonExclusiveWrite | SharedObjectMutability::Mutable,
                    ..
                }) => {
                    assert_invariant!(
                        *id == SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                        "Settlement shared input must be the accumulator root"
                    );
                    assert_invariant!(
                        root_argument.is_none(),
                        "Settlement must have only one accumulator root input"
                    );
                    root_argument = Some(Argument::Input(checked_as!(index, u16)?));
                }
                _ => invariant_violation!("Unexpected settlement input: {input:?}"),
            }
        }
        let Some(root_argument) = root_argument else {
            invariant_violation!("Settlement must have an accumulator root input");
        };
        let Some(root) = state.read_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID).cloned() else {
            invariant_violation!("Settlement accumulator root is missing");
        };
        Ok(Self {
            state,
            protocol_config,
            tx_context,
            inputs,
            root_argument,
            root,
            children: BTreeMap::new(),
            generated_ids: BTreeSet::new(),
            input_sui: 0,
            output_sui: 0,
        })
    }

    fn read_dynamic_field<K: Serialize + DeserializeOwned, V: DeserializeOwned>(
        &mut self,
        key: &K,
        key_type: &TypeTag,
        value_type: &TypeTag,
    ) -> Result<(ObjectID, Option<V>), ExecutionError> {
        let bytes = bcs::to_bytes(key)
            .map_err(|e| make_invariant_violation!("Cannot serialize settlement key: {e}"))?;
        let id = derive_dynamic_field_id(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key_type, &bytes)
            .map_err(|e| make_invariant_violation!("Cannot derive settlement field ID: {e}"))?;
        let child = match self.children.entry(id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let original = self
                    .state
                    .read_child_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID, &id, self.root.version())
                    .map_err(|e| {
                        make_invariant_violation!("Cannot read settlement field {id}: {e}")
                    })?;
                entry.insert(ChildObject {
                    current: original.clone(),
                    original,
                })
            }
        };
        let Some(object) = &child.current else {
            return Ok((id, None));
        };
        assert_invariant!(
            object.id() == id,
            "Invalid settlement field ID: expected {id}, got {}",
            object.id()
        );
        assert_invariant!(
            object.owner == Owner::ObjectOwner(SUI_ACCUMULATOR_ROOT_OBJECT_ID.into()),
            "Invalid settlement field owner for {id}: {:?}",
            object.owner
        );
        let Some(object) = object.data.try_as_move() else {
            invariant_violation!("Settlement field {id} must be a Move object");
        };
        assert_invariant!(
            *object.type_()
                == DynamicFieldInfo::dynamic_field_type(key_type.clone(), value_type.clone())
                    .into(),
            "Settlement field {id} has an unexpected type"
        );
        let field: Field<K, V> = bcs::from_bytes(object.contents()).map_err(|e| {
            make_invariant_violation!("Cannot deserialize settlement field {id}: {e}")
        })?;
        Ok((id, Some(field.value)))
    }

    fn write_dynamic_field<K: Serialize, V: Serialize>(
        &mut self,
        id: ObjectID,
        key: K,
        key_type: TypeTag,
        value: V,
        value_type: TypeTag,
    ) -> Result<(), ExecutionError> {
        let child = self
            .children
            .get_mut(&id)
            .ok_or_else(|| make_invariant_violation!("Settlement field {id} was not loaded"))?;
        if child.current.is_none() {
            self.generated_ids.insert(id);
        }
        let contents = bcs::to_bytes(&Field {
            id: UID::new(id),
            name: key,
            value,
        })
        .map_err(|e| make_invariant_violation!("Cannot serialize settlement field {id}: {e}"))?;
        // Field has key but not store, regardless of its type arguments.
        let object = unsafe {
            MoveObject::new_from_execution(
                DynamicFieldInfo::dynamic_field_type(key_type, value_type).into(),
                /* has_public_transfer */ false,
                child
                    .original
                    .as_ref()
                    .map_or(SequenceNumber::MIN, |object| object.version()),
                contents,
                self.protocol_config,
                /* system_mutation */ true,
            )?
        };
        child.current = Some(Object::new_move(
            object,
            Owner::ObjectOwner(SUI_ACCUMULATOR_ROOT_OBJECT_ID.into()),
            self.tx_context.digest(),
        ));
        Ok(())
    }

    fn finish(self) -> (ExecutionResultsV2, BTreeSet<ObjectID>) {
        let Self {
            root,
            children,
            generated_ids,
            input_sui: settlement_input_sui,
            output_sui: settlement_output_sui,
            state: _,
            protocol_config: _,
            tx_context: _,
            inputs: _,
            root_argument: _,
        } = self;
        let root_id = root.id();
        let mut written_objects = BTreeMap::new();
        let mut modified_objects = BTreeSet::new();
        let mut created_object_ids = BTreeSet::new();
        let mut deleted_object_ids = BTreeSet::new();
        written_objects.insert(root_id, root);
        modified_objects.insert(root_id);
        for (id, ChildObject { original, current }) in children {
            match (original, current) {
                (None, None) => (),
                (None, Some(object)) => {
                    created_object_ids.insert(id);
                    written_objects.insert(id, object);
                }
                (Some(_), None) => {
                    modified_objects.insert(id);
                    deleted_object_ids.insert(id);
                }
                (Some(original), Some(object)) => {
                    // Match Move's child-object change detection:
                    // unchanged values produce no write.
                    // Transaction metadata and storage rebates are finalized later.
                    if original.data != object.data || original.owner != object.owner {
                        modified_objects.insert(id);
                        written_objects.insert(id, object);
                    }
                }
            }
        }
        let execution_results = ExecutionResultsV2 {
            settlement_input_sui,
            settlement_output_sui,
            written_objects,
            modified_objects,
            created_object_ids,
            deleted_object_ids,
            user_events: vec![],
            accumulator_events: vec![],
        };
        (execution_results, generated_ids)
    }
}

/// Executes accumulator settlement calls directly in Rust.
/// Type arguments come from accumulator events and already contain defining IDs. Settlement
/// only uses those types as phantom dynamic-field keys, so it needs neither code nor layouts.
pub(crate) fn execute<E: ExecutionErrorTrait>(
    protocol_config: &ProtocolConfig,
    state: &mut dyn ExecutionState,
    tx_context: &TxContext,
    transaction: ProgrammableTransaction,
) -> Result<(), E> {
    let ProgrammableTransaction { inputs, commands } = transaction;
    let Some(last_command_index) = commands.len().checked_sub(1) else {
        invariant_violation!("Settlement must contain a prologue");
    };
    // Execute the commands
    let (execution_results, generated_ids) = {
        let mut context = Context::new(protocol_config, state, tx_context, inputs)?;
        let command_result = commands
            .into_iter()
            .enumerate()
            .try_for_each(|(index, cmd)| {
                command(&mut context, index, last_command_index, cmd)
                    .map_err(|e| e.with_command_index(index))
            });
        let loaded = context
            .children
            .iter()
            .filter_map(|(id, child)| {
                let object = child.original.as_ref()?;
                Some((
                    *id,
                    DynamicallyLoadedObjectMetadata {
                        version: object.version(),
                        digest: object.digest(),
                        owner: object.owner.clone(),
                        storage_rebate: object.storage_rebate,
                        previous_transaction: object.previous_transaction,
                    },
                ))
            })
            .collect();
        let final_result = context.finish();
        // Settlement must succeed. If an invariant or storage error is returned, retain
        // the reads for replay and leave the writes unapplied.
        state.save_loaded_runtime_objects(loaded);
        command_result?;
        final_result
    };
    state.record_execution_results(ExecutionResults::V2(execution_results))?;
    state.record_generated_object_ids(generated_ids);
    Ok(())
}

fn command(
    context: &mut Context<'_>,
    index: usize,
    last_command_index: usize,
    command: Command,
) -> Result<(), ExecutionError> {
    let Command::MoveCall(call) = command else {
        invariant_violation!("Settlement commands must be Move calls");
    };
    let ProgrammableMoveCall {
        package,
        module,
        function,
        type_arguments,
        arguments,
    } = *call;
    assert_invariant!(
        package == SUI_FRAMEWORK_PACKAGE_ID,
        "Unexpected settlement package: {package}"
    );
    match (module.as_str(), function.as_str()) {
        (ACCUMULATOR_SETTLEMENT_MODULE_NAME, SETTLEMENT_PROLOGUE) => {
            assert_invariant!(index == 0, "Settlement prologue must be the first command");
            let [] = type_arguments.as_slice() else {
                invariant_violation!("Settlement prologue expects no type arguments");
            };
            let [root, epoch, height, offset, input, output] = arguments.as_slice() else {
                invariant_violation!("Settlement prologue expects six arguments");
            };
            assert_invariant!(
                *root == context.root_argument,
                "Settlement call has an incorrect root argument"
            );
            // These values distinguish transaction digests. Move also ignores their values,
            // but execution still validates their encoding as u64 arguments.
            let _: u64 = pure(&context.inputs, epoch)?;
            let _: u64 = pure(&context.inputs, height)?;
            let _: u64 = pure(&context.inputs, offset)?;
            context.input_sui = pure::<u64>(&context.inputs, input)?;
            context.output_sui = pure::<u64>(&context.inputs, output)?;
            Ok(())
        }
        (ACCUMULATOR_SETTLEMENT_MODULE_NAME, SETTLE_U128) => {
            assert_invariant!(index > 0, "Balance settlement must follow the prologue");
            let [ty] = type_arguments.as_slice() else {
                invariant_violation!("Balance settlement expects one type argument");
            };
            let [root, owner, merge, split] = arguments.as_slice() else {
                invariant_violation!("Balance settlement expects four arguments");
            };
            let ty = ty
                .to_type_tag()
                .map_err(|e| make_invariant_violation!("Invalid settlement type: {e}"))?;
            assert_invariant!(
                *root == context.root_argument,
                "Settlement call has an incorrect root argument"
            );
            let owner: SuiAddress = pure(&context.inputs, owner)?;
            let merge: u128 = pure(&context.inputs, merge)?;
            let split: u128 = pure(&context.inputs, split)?;
            settle_u128(context, ty, owner, merge, split)
        }
        (ACCUMULATOR_SETTLEMENT_MODULE_NAME, SETTLE_EVENTS) => {
            assert_invariant!(index > 0, "Event settlement must follow the prologue");
            let [] = type_arguments.as_slice() else {
                invariant_violation!("Event settlement expects no type arguments");
            };
            let [root, stream, new_root, count, checkpoint] = arguments.as_slice() else {
                invariant_violation!("Event settlement expects five arguments");
            };
            assert_invariant!(
                *root == context.root_argument,
                "Settlement call has an incorrect root argument"
            );
            let stream: SuiAddress = pure(&context.inputs, stream)?;
            let new_root: U256 = pure(&context.inputs, new_root)?;
            let count: u64 = pure(&context.inputs, count)?;
            let checkpoint: u64 = pure(&context.inputs, checkpoint)?;
            settle_events(context, stream, new_root, count, checkpoint)
        }
        (ACCUMULATOR_METADATA_MODULE_NAME, RECORD_ACCUMULATOR_OBJECT_CHANGES) => {
            assert_invariant!(index > 0, "Object-count update must follow the prologue");
            assert_invariant!(
                index == last_command_index,
                "Object-count update must be the last command"
            );
            let [] = type_arguments.as_slice() else {
                invariant_violation!("Object-count update expects no type arguments");
            };
            let [root, created, destroyed] = arguments.as_slice() else {
                invariant_violation!("Object-count update expects three arguments");
            };
            assert_invariant!(
                *root == context.root_argument,
                "Settlement call has an incorrect root argument"
            );
            let created: u64 = pure(&context.inputs, created)?;
            let destroyed: u64 = pure(&context.inputs, destroyed)?;
            record_object_changes(context, created, destroyed)
        }
        _ => invariant_violation!("Unexpected settlement call: {module}::{function}"),
    }
}

fn pure<T: DeserializeOwned>(inputs: &[CallArg], argument: &Argument) -> Result<T, ExecutionError> {
    let Argument::Input(index) = argument else {
        invariant_violation!("Settlement arguments must refer to inputs: {argument:?}");
    };
    let Some(input) = inputs.get(usize::from(*index)) else {
        invariant_violation!("Settlement input index out of bounds: {index}")
    };
    let CallArg::Pure(bytes) = input else {
        invariant_violation!("Settlement expected a pure argument: {argument:?}");
    };
    bcs::from_bytes(bytes)
        .map_err(|e| make_invariant_violation!("Invalid settlement argument: {e}"))
}

fn settle_u128(
    context: &mut Context<'_>,
    ty: TypeTag,
    owner: SuiAddress,
    merge: u128,
    split: u128,
) -> Result<(), ExecutionError> {
    let key = AccumulatorKey { owner };
    let key_type = AccumulatorKey::get_type_tag(&[ty]);
    let (id, previous) =
        context.read_dynamic_field::<_, U128>(&key, &key_type, &U128::get_type_tag())?;
    assert_invariant!(
        previous.is_some() || split == 0,
        "Cannot withdraw from a missing accumulator"
    );
    let previous = previous.map_or(0, |v| v.value);
    let value = match (merge, split) {
        (0, 0) => invariant_violation!("Settlement must merge or split a nonzero amount"),
        (merge, 0) => previous.checked_add(merge),
        (0, split) => previous.checked_sub(split),
        (_, _) => invariant_violation!("Settlement cannot both merge and split"),
    };
    let Some(value) = value else {
        invariant_violation!("Settlement balance overflow or underflow");
    };
    if value == 0 {
        context
            .children
            .get_mut(&id)
            .ok_or_else(|| make_invariant_violation!("Settlement field was not loaded"))?
            .current = None;
        Ok(())
    } else {
        context.write_dynamic_field(id, key, key_type, U128 { value }, U128::get_type_tag())
    }
}

fn settle_events(
    context: &mut Context<'_>,
    stream: SuiAddress,
    new_root: U256,
    count: u64,
    checkpoint: u64,
) -> Result<(), ExecutionError> {
    let key = AccumulatorKey { owner: stream };
    let value_type = EventStreamHead::get_type_tag();
    let key_type = AccumulatorKey::get_type_tag(std::slice::from_ref(&value_type));
    let (id, previous) =
        context.read_dynamic_field::<_, EventStreamHead>(&key, &key_type, &value_type)?;
    let mut head = previous.unwrap_or_default();
    add_to_mmr(new_root, &mut head.mmr);
    head.num_events = head
        .num_events
        .checked_add(count)
        .ok_or_else(|| make_invariant_violation!("Settlement event count overflow"))?;
    head.checkpoint_seq = checkpoint;
    context.write_dynamic_field(id, key, key_type, head, value_type)
}

fn add_to_mmr(new_root: U256, mmr: &mut Vec<U256>) {
    let mut current = new_root;
    for peak in mmr.iter_mut() {
        if *peak == U256::zero() {
            *peak = current;
            return;
        }
        current = hash_two_to_one_u256(*peak, current);
        *peak = U256::zero();
    }
    mmr.push(current);
}

fn hash_two_to_one_u256(left: U256, right: U256) -> U256 {
    let mut hash = Blake2b256::default();
    hash.update(left.to_le_bytes());
    hash.update(right.to_le_bytes());
    U256::from_le_bytes(&hash.finalize().digest)
}

fn record_object_changes(
    context: &mut Context<'_>,
    created: u64,
    destroyed: u64,
) -> Result<(), ExecutionError> {
    let key = AccumulatorObjectCountKey::default();
    let key_type = AccumulatorObjectCountKey::get_type_tag();
    let (id, previous) = context.read_dynamic_field::<_, u64>(&key, &key_type, &TypeTag::U64)?;
    let count = previous
        .unwrap_or(0)
        .checked_add(created)
        .and_then(|v| v.checked_sub(destroyed))
        .ok_or_else(|| {
            make_invariant_violation!("Settlement object count overflow or underflow")
        })?;
    context.write_dynamic_field(id, key, key_type, count, TypeTag::U64)
}

/// These framework layouts contain no SUI. Their type arguments are phantom, so conservation
/// checks do not need to resolve user packages just to inspect settlement outputs.
pub(crate) fn has_no_sui(object: &Object) -> bool {
    static EVENT_STREAM_FIELD_TYPE: LazyLock<StructTag> = LazyLock::new(|| {
        let event = EventStreamHead::get_type_tag();
        DynamicFieldInfo::dynamic_field_type(
            AccumulatorKey::get_type_tag(std::slice::from_ref(&event)),
            event,
        )
    });
    static OBJECT_COUNT_FIELD_TYPE: LazyLock<StructTag> = LazyLock::new(|| {
        DynamicFieldInfo::dynamic_field_type(
            AccumulatorObjectCountKey::get_type_tag(),
            TypeTag::U64,
        )
    });

    let Some(move_object) = object.data.try_as_move() else {
        return false;
    };
    if move_object.type_().is_balance_accumulator_field() {
        return !move_object.type_().is_sui_balance_accumulator_field();
    }
    let Some(tag) = move_object.type_().other() else {
        return false;
    };
    if tag.address == SUI_FRAMEWORK_ADDRESS
        && tag.module.as_ident_str() == ACCUMULATOR_ROOT_MODULE
        && tag.name.as_str() == ACCUMULATOR_ROOT_STRUCT_NAME
        && tag.type_params.is_empty()
    {
        return true;
    }
    tag == &*EVENT_STREAM_FIELD_TYPE || tag == &*OBJECT_COUNT_FIELD_TYPE
}
