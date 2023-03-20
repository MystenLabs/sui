// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
    marker::PhantomData,
};

use move_binary_format::{
    errors::{Location, VMError},
    file_format::{CodeOffset, FunctionDefinitionIndex, TypeParameterIndex},
};
use move_core_types::language_storage::{ModuleId, StructTag, TypeTag};
use move_vm_runtime::{move_vm::MoveVM, session::Session};
use sui_framework::natives::object_runtime::{max_event_error, ObjectRuntime, RuntimeResults};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    balance::Balance,
    base_types::{MoveObjectType, ObjectID, SequenceNumber, SuiAddress, TxContext},
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind},
    gas::SuiGasStatus,
    messages::{Argument, CallArg, CommandArgumentError, ObjectArg},
    move_package::MovePackage,
    object::{MoveObject, Object, Owner, OBJECT_START_VERSION},
    storage::{ObjectChange, WriteKind},
};

use crate::{
    adapter::{missing_unwrapped_msg, new_session},
    execution_mode::ExecutionMode,
};

use super::types::*;

/// Maintains all runtime state specific to programmable transactions
pub struct ExecutionContext<'vm, 'state, 'a, 'b, E: fmt::Debug, S: StorageView<E>> {
    /// The protocol config
    pub protocol_config: &'a ProtocolConfig,
    /// The MoveVM
    pub vm: &'vm MoveVM,
    /// The global state, used for resolving packages
    pub state_view: &'state S,
    /// A shared transaction context, contains transaction digest information and manages the
    /// creation of new object IDs
    pub tx_context: &'a mut TxContext,
    /// The gas status used for metering
    pub gas_status: &'a mut SuiGasStatus<'b>,
    /// The session used for interacting with Move types and calls
    pub session: Session<'state, 'vm, S>,
    /// Additional transfers not from the Move runtime
    additional_transfers: Vec<(/* new owner */ SuiAddress, ObjectValue)>,
    /// Newly published packages
    new_packages: Vec<Object>,
    /// User events are claimed after each Move call
    user_events: Vec<(ModuleId, StructTag, Vec<u8>)>,
    // runtime data
    /// The runtime value for the Gas coin, None if it has been taken/moved
    gas: InputValue,
    /// The runtime value for the inputs/call args, None if it has been taken/moved
    inputs: Vec<InputValue>,
    /// The results of a given command. For most commands, the inner vector will have length 1.
    /// It will only not be 1 for Move calls with multiple return values.
    /// Inner values are None if taken/moved by-value
    results: Vec<Vec<ResultValue>>,
    /// Map of arguments that are currently borrowed in this command, true if the borrow is mutable
    /// This gets cleared out when new results are pushed, i.e. the end of a command
    borrowed: HashMap<Argument, /* mut */ bool>,
    _e: PhantomData<E>,
}

/// A write for an object that was generated outside of the Move ObjectRuntime
struct AdditionalWrite {
    /// The new owner of the object
    recipient: Owner,
    /// the type of the object,
    type_: MoveObjectType,
    /// if the object has public transfer or not, i.e. if it has store
    has_public_transfer: bool,
    /// contents of the object
    bytes: Vec<u8>,
}

impl<'vm, 'state, 'a, 'b, E, S> ExecutionContext<'vm, 'state, 'a, 'b, E, S>
where
    E: fmt::Debug,
    S: StorageView<E>,
{
    pub fn new(
        protocol_config: &'a ProtocolConfig,
        vm: &'vm MoveVM,
        state_view: &'state S,
        tx_context: &'a mut TxContext,
        gas_status: &'a mut SuiGasStatus<'b>,
        gas_coin_opt: Option<ObjectID>,
        inputs: Vec<CallArg>,
    ) -> Result<Self, ExecutionError> {
        let mut object_owner_map = BTreeMap::new();
        let inputs = inputs
            .into_iter()
            .map(|call_arg| load_call_arg(state_view, &mut object_owner_map, call_arg))
            .collect::<Result<_, ExecutionError>>()?;
        let gas = if let Some(gas_coin) = gas_coin_opt {
            let mut gas = load_object(
                state_view,
                &mut object_owner_map,
                /* imm override */ false,
                gas_coin,
            )?;
            // subtract the max gas budget. This amount is off limits in the programmable transaction,
            // so to mimic this "off limits" behavior, we act as if the coin has less balance than
            // it really does
            let Some(Value::Object(ObjectValue {
                contents: ObjectContents::Coin(coin),
                ..
            })) = &mut gas.inner.value else {
                invariant_violation!("Gas object should be a populated coin")
            };
            let max_gas_in_balance = gas_status.max_gax_budget_in_balance();
            let Some(new_balance) = coin.balance.value().checked_sub(max_gas_in_balance) else {
                invariant_violation!(
                    "Transaction input checker should check that there is enough gas"
                );
            };
            coin.balance = Balance::new(new_balance);
            gas
        } else {
            InputValue {
                object_metadata: None,
                inner: ResultValue {
                    last_usage_kind: None,
                    value: None,
                },
            }
        };
        let session = new_session(
            vm,
            state_view,
            object_owner_map,
            !gas_status.is_unmetered(),
            protocol_config,
        );
        Ok(Self {
            protocol_config,
            vm,
            state_view,
            tx_context,
            gas_status,
            session,
            gas,
            inputs,
            results: vec![],
            additional_transfers: vec![],
            new_packages: vec![],
            user_events: vec![],
            borrowed: HashMap::new(),
            _e: PhantomData,
        })
    }

    /// Create a new ID and update the state
    pub fn fresh_id(&mut self) -> Result<ObjectID, ExecutionError> {
        let object_id = self.tx_context.fresh_id();
        let object_runtime: &mut ObjectRuntime = self.session.get_native_extensions().get_mut();
        object_runtime
            .new_id(object_id)
            .map_err(|e| self.convert_vm_error(e.finish(Location::Undefined)))?;
        Ok(object_id)
    }

    /// Delete an ID and update the state
    pub fn delete_id(&mut self, object_id: ObjectID) -> Result<(), ExecutionError> {
        let object_runtime: &mut ObjectRuntime = self.session.get_native_extensions().get_mut();
        object_runtime
            .delete_id(object_id)
            .map_err(|e| self.convert_vm_error(e.finish(Location::Undefined)))
    }

    /// Takes the user events from the runtime and tags them with the Move module of the function
    /// that was invoked for the command
    pub fn take_user_events(
        &mut self,
        module_id: &ModuleId,
        function: FunctionDefinitionIndex,
        last_offset: CodeOffset,
    ) -> Result<(), ExecutionError> {
        let object_runtime: &mut ObjectRuntime = self.session.get_native_extensions().get_mut();
        let events = object_runtime.take_user_events();
        let num_events = self.user_events.len() + events.len();
        let max_events = self.protocol_config.max_num_event_emit();
        if num_events as u64 > max_events {
            let err = max_event_error(max_events)
                .at_code_offset(function, last_offset)
                .finish(Location::Module(module_id.clone()));
            return Err(self.convert_vm_error(err));
        }
        let new_events = events
            .into_iter()
            .map(|(tag, value)| {
                let layout = self
                    .session
                    .get_type_layout(&TypeTag::Struct(Box::new(tag.clone())))?;
                let bytes = value.simple_serialize(&layout).unwrap();
                Ok((module_id.clone(), tag, bytes))
            })
            .collect::<Result<Vec<_>, VMError>>()
            .map_err(|e| self.convert_vm_error(e))?;
        self.user_events.extend(new_events);
        Ok(())
    }

    /// Get the argument value. Cloning the value if it is copyable, and setting its value to None
    /// if it is not (making it unavailable).
    /// Errors if out of bounds, if the argument is borrowed, if it is unavailable (already taken),
    /// or if it is an object that cannot be taken by value (shared or immutable)
    pub fn by_value_arg<V: TryFromValue>(
        &mut self,
        command_kind: CommandKind<'_>,
        arg_idx: usize,
        arg: Argument,
    ) -> Result<V, ExecutionError> {
        self.by_value_arg_(command_kind, arg)
            .map_err(|e| command_argument_error(e, arg_idx))
    }
    fn by_value_arg_<V: TryFromValue>(
        &mut self,
        command_kind: CommandKind<'_>,
        arg: Argument,
    ) -> Result<V, CommandArgumentError> {
        if matches!(arg, Argument::GasCoin) && !matches!(command_kind, CommandKind::TransferObjects)
        {
            return Err(CommandArgumentError::InvalidGasCoinUsage);
        }
        if self.arg_is_borrowed(&arg) {
            return Err(CommandArgumentError::InvalidUsageOfBorrowedValue);
        }
        let (input_metadata_opt, val_opt) = self.borrow_mut(arg, UsageKind::ByValue)?;
        let is_copyable = if let Some(val) = val_opt {
            val.is_copyable()
        } else {
            return Err(CommandArgumentError::InvalidUsageOfTakenValue);
        };
        if matches!(
            input_metadata_opt,
            Some(InputObjectMetadata {
                owner: Owner::Immutable | Owner::Shared { .. },
                ..
            })
        ) {
            return Err(CommandArgumentError::InvalidObjectByValue);
        }
        let val = if is_copyable {
            val_opt.as_ref().unwrap().clone()
        } else {
            val_opt.take().unwrap()
        };
        V::try_from_value(val)
    }

    /// Mimic a mutable borrow by taking the argument value, setting its value to None,
    /// making it unavailable. The value will be marked as borrowed and must be returned with
    /// restore_arg
    /// Errors if out of bounds, if the argument is borrowed, if it is unavailable (already taken),
    /// or if it is an object that cannot be mutably borrowed (immutable)
    pub fn borrow_arg_mut<V: TryFromValue>(
        &mut self,
        arg_idx: usize,
        arg: Argument,
    ) -> Result<V, ExecutionError> {
        self.borrow_arg_mut_(arg)
            .map_err(|e| command_argument_error(e, arg_idx))
    }
    fn borrow_arg_mut_<V: TryFromValue>(
        &mut self,
        arg: Argument,
    ) -> Result<V, CommandArgumentError> {
        if self.arg_is_borrowed(&arg) {
            return Err(CommandArgumentError::InvalidUsageOfBorrowedValue);
        }
        self.borrowed.insert(arg, /* is_mut */ true);
        let (input_metadata_opt, val_opt) = self.borrow_mut(arg, UsageKind::BorrowMut)?;
        if val_opt.is_none() {
            return Err(CommandArgumentError::InvalidUsageOfTakenValue);
        }
        if input_metadata_opt.is_some() && !input_metadata_opt.unwrap().is_mutable_input {
            return Err(CommandArgumentError::InvalidObjectByMutRef);
        }
        V::try_from_value(val_opt.take().unwrap())
    }

    /// Mimics an immutable borrow by cloning the argument value without setting its value to None
    /// Errors if out of bounds, if the argument is mutably borrowed,
    /// or if it is unavailable (already taken)
    pub fn borrow_arg<V: TryFromValue>(
        &mut self,
        arg_idx: usize,
        arg: Argument,
    ) -> Result<V, ExecutionError> {
        self.borrow_arg_(arg)
            .map_err(|e| command_argument_error(e, arg_idx))
    }
    fn borrow_arg_<V: TryFromValue>(&mut self, arg: Argument) -> Result<V, CommandArgumentError> {
        if self.arg_is_mut_borrowed(&arg) {
            return Err(CommandArgumentError::InvalidUsageOfBorrowedValue);
        }
        self.borrowed.insert(arg, /* is_mut */ false);
        let (_input_metadata_opt, val_opt) = self.borrow_mut(arg, UsageKind::BorrowImm)?;
        if val_opt.is_none() {
            return Err(CommandArgumentError::InvalidUsageOfTakenValue);
        }
        V::try_from_value(val_opt.as_ref().unwrap().clone())
    }

    /// Restore an argument after being mutably borrowed
    pub fn restore_arg<Mode: ExecutionMode>(
        &mut self,
        updates: &mut Mode::ArgumentUpdates,
        arg: Argument,
        value: Value,
    ) -> Result<(), ExecutionError> {
        Mode::add_argument_update(self, updates, arg, &value)?;
        let was_mut_opt = self.borrowed.remove(&arg);
        assert_invariant!(
            was_mut_opt.is_some() && was_mut_opt.unwrap(),
            "Should never restore a non-mut borrowed value. \
            The take+restore is an implementation detail of mutable references"
        );
        // restore is exclusively used for mut
        let Ok((_, value_opt)) = self.borrow_mut_impl(arg, None) else {
            invariant_violation!("Should be able to borrow argument to restore it")
        };
        let old_value = value_opt.replace(value);
        assert_invariant!(
            old_value.is_none(),
            "Should never restore a non-taken value. \
            The take+restore is an implementation detail of mutable references"
        );
        Ok(())
    }

    /// Transfer the object to a new owner
    pub fn transfer_object(
        &mut self,
        obj: ObjectValue,
        addr: SuiAddress,
    ) -> Result<(), ExecutionError> {
        self.additional_transfers.push((addr, obj));
        Ok(())
    }

    /// Create a new package
    pub fn new_package<'p>(
        &mut self,
        modules: Vec<move_binary_format::CompiledModule>,
        dependencies: impl IntoIterator<Item = &'p MovePackage>,
        version: Option<SequenceNumber>,
    ) -> Result<ObjectID, ExecutionError> {
        // wrap the modules in an object, write it to the store
        let object = Object::new_package(
            modules,
            version.unwrap_or(OBJECT_START_VERSION),
            self.tx_context.digest(),
            self.protocol_config.max_move_package_size(),
            dependencies,
        )?;
        let object_id = object.id();
        self.new_packages.push(object);
        Ok(object_id)
    }

    /// Create a package upgrade from `previous_package` with `new_modules` and `dependencies`
    pub fn upgrade_package<'p>(
        &mut self,
        previous_package: &MovePackage,
        new_modules: Vec<move_binary_format::CompiledModule>,
        dependencies: impl IntoIterator<Item = &'p MovePackage>,
    ) -> Result<ObjectID, ExecutionError> {
        let new_package_object_id = self.tx_context.fresh_id();
        let object = Object::new_upgraded_package(
            previous_package,
            new_package_object_id,
            new_modules,
            self.tx_context.digest(),
            self.protocol_config.max_move_package_size(),
            dependencies,
        )?;
        let object_id = object.id();
        self.new_packages.push(object);
        Ok(object_id)
    }

    /// Finish a command: clearing the borrows and adding the results to the result vector
    pub fn push_command_results(&mut self, results: Vec<Value>) -> Result<(), ExecutionError> {
        assert_invariant!(
            self.borrowed.values().all(|is_mut| !is_mut),
            "all mut borrows should be restored"
        );
        // clear borrow state
        self.borrowed = HashMap::new();
        self.results
            .push(results.into_iter().map(ResultValue::new).collect());
        Ok(())
    }

    /// Determine the object changes and collect all user events
    pub fn finish<Mode: ExecutionMode>(self) -> Result<ExecutionResults, ExecutionError> {
        use sui_types::error::convert_vm_error;
        let Self {
            protocol_config,
            vm,
            state_view,
            tx_context,
            gas_status,
            session,
            additional_transfers,
            new_packages,
            gas,
            inputs,
            results,
            user_events,
            ..
        } = self;
        let tx_digest = tx_context.digest();
        let mut additional_writes = BTreeMap::new();
        let mut input_object_metadata = BTreeMap::new();
        // Any object value that has not been taken (still has `Some` for it's value) needs to
        // written as it's value might have changed (and eventually it's sequence number will need
        // to increase)
        let mut by_value_inputs = BTreeSet::new();
        let mut add_input_object_write = |input| {
            let InputValue {
                object_metadata: object_metadata_opt,
                inner: ResultValue { value, .. },
            } = input;
            let Some(object_metadata) = object_metadata_opt else { return };
            let is_mutable_input = object_metadata.is_mutable_input;
            let owner = object_metadata.owner;
            let id = object_metadata.id;
            input_object_metadata.insert(object_metadata.id, object_metadata);
            let Some(Value::Object(object_value)) = value else {
                by_value_inputs.insert(id);
                return
            };
            if is_mutable_input {
                add_additional_write(&mut additional_writes, owner, object_value);
            }
        };
        let gas_id_opt = gas.object_metadata.as_ref().map(|info| info.id);
        add_input_object_write(gas);
        for input in inputs {
            add_input_object_write(input)
        }
        // check for unused values
        // disable this check for dev inspect
        if !Mode::allow_arbitrary_values() {
            for (i, command_result) in results.iter().enumerate() {
                for (j, result_value) in command_result.iter().enumerate() {
                    let ResultValue {
                        last_usage_kind,
                        value,
                    } = result_value;
                    match value {
                        None => (),
                        Some(Value::Object(_)) => {
                            return Err(ExecutionErrorKind::UnusedValueWithoutDrop {
                                result_idx: i as u16,
                                secondary_idx: j as u16,
                            }
                            .into())
                        }
                        Some(Value::Raw(RawValueType::Any, _)) => (),
                        Some(Value::Raw(RawValueType::Loaded { abilities, .. }, _)) => {
                            // - nothing to check for drop
                            // - if it does not have drop, but has copy,
                            //   the last usage must be by value in order to "lie" and say that the
                            //   last usage is actually a take instead of a clone
                            // - Otherwise, an error
                            if abilities.has_drop() {
                            } else if abilities.has_copy()
                                && !matches!(last_usage_kind, Some(UsageKind::ByValue))
                            {
                                let msg = "The value has copy, but not drop. \
                                Its last usage must be by-value so it can be taken.";
                                return Err(ExecutionError::new_with_source(
                                    ExecutionErrorKind::UnusedValueWithoutDrop {
                                        result_idx: i as u16,
                                        secondary_idx: j as u16,
                                    },
                                    msg,
                                ));
                            }
                        }
                    }
                }
            }
        }
        // add transfers from TransferObjects command
        for (recipient, object_value) in additional_transfers {
            let owner = Owner::AddressOwner(recipient);
            add_additional_write(&mut additional_writes, owner, object_value)
        }
        // Refund unused gas
        if let Some(gas_id) = gas_id_opt {
            refund_max_gas_budget(&mut additional_writes, gas_status, gas_id)?;
        }

        let (change_set, events, mut native_context_extensions) = session
            .finish_with_extensions()
            .map_err(|e| convert_vm_error(e, vm, state_view))?;
        // Sui Move programs should never touch global state, so resources should be empty
        assert_invariant!(
            change_set.resources().next().is_none(),
            "Change set must be empty"
        );
        // Sui Move no longer uses Move's internal event system
        assert_invariant!(events.is_empty(), "Events must be empty");
        let object_runtime: ObjectRuntime = native_context_extensions.remove();
        let new_ids = object_runtime.new_ids().clone();
        // tell the object runtime what input objects were taken and which were transferred
        let external_transfers = additional_writes.keys().copied().collect();
        let RuntimeResults {
            writes,
            deletions,
            user_events: remaining_events,
            loaded_child_objects,
        } = object_runtime.finish(by_value_inputs, external_transfers)?;
        assert_invariant!(
            remaining_events.is_empty(),
            "Events should be taken after every Move call"
        );
        // todo remove this when system events are removed
        let mut object_changes = BTreeMap::new();
        for package in new_packages {
            let id = package.id();
            let change = ObjectChange::Write(package, WriteKind::Create);
            object_changes.insert(id, change);
        }
        for (id, additional_write) in additional_writes {
            let AdditionalWrite {
                recipient,
                type_,
                has_public_transfer,
                bytes,
            } = additional_write;
            let write_kind = if input_object_metadata.contains_key(&id)
                || loaded_child_objects.contains_key(&id)
            {
                assert_invariant!(
                    !new_ids.contains_key(&id),
                    "new id should not be in mutations"
                );
                WriteKind::Mutate
            } else if new_ids.contains_key(&id) {
                WriteKind::Create
            } else {
                WriteKind::Unwrap
            };
            // safe given the invariant that the runtime correctly propagates has_public_transfer
            let move_object = unsafe {
                create_written_object(
                    protocol_config,
                    &input_object_metadata,
                    &loaded_child_objects,
                    id,
                    type_,
                    has_public_transfer,
                    bytes,
                    write_kind,
                )?
            };
            let object = Object::new_move(move_object, recipient, tx_digest);
            let change = ObjectChange::Write(object, write_kind);
            object_changes.insert(id, change);
        }

        // we need a new session just for deserializing and fetching abilities. Which is sad
        let tmp_session = new_session(
            vm,
            state_view,
            BTreeMap::new(),
            !gas_status.is_unmetered(),
            protocol_config,
        );
        for (id, (write_kind, recipient, ty, move_type, value)) in writes {
            let abilities = tmp_session
                .get_type_abilities(&ty)
                .map_err(|e| convert_vm_error(e, vm, state_view))?;
            let has_public_transfer = abilities.has_store();
            let layout = tmp_session
                .get_type_layout(&move_type.clone().into())
                .map_err(|e| convert_vm_error(e, vm, state_view))?;
            let bytes = value.simple_serialize(&layout).unwrap();
            // safe because has_public_transfer has been determined by the abilities
            let move_object = unsafe {
                create_written_object(
                    protocol_config,
                    &input_object_metadata,
                    &loaded_child_objects,
                    id,
                    move_type,
                    has_public_transfer,
                    bytes,
                    write_kind,
                )?
            };
            let object = Object::new_move(move_object, recipient, tx_digest);
            let change = ObjectChange::Write(object, write_kind);
            object_changes.insert(id, change);
        }
        for (id, delete_kind) in deletions {
            let version = match input_object_metadata.get(&id) {
                Some(metadata) => metadata.version,
                None => match state_view.get_latest_parent_entry_ref(id) {
                    Ok(Some((_, previous_version, _))) => previous_version,
                    // This object was not created this transaction but has never existed in
                    // storage, skip it.
                    Ok(None) => continue,
                    Err(_) => invariant_violation!(missing_unwrapped_msg(&id)),
                },
            };
            object_changes.insert(id, ObjectChange::Delete(version, delete_kind));
        }
        let (change_set, move_events) = tmp_session
            .finish()
            .map_err(|e| convert_vm_error(e, vm, state_view))?;
        // the session was just used for ability and layout metadata fetching, no changes should
        // exist. Plus, Sui Move does not use these changes or events
        assert_invariant!(change_set.accounts().is_empty(), "Change set must be empty");
        assert_invariant!(move_events.is_empty(), "Events must be empty");

        Ok(ExecutionResults {
            object_changes,
            user_events,
        })
    }

    /// Convert a VM Error to an execution one
    pub fn convert_vm_error(&self, error: VMError) -> ExecutionError {
        sui_types::error::convert_vm_error(error, self.vm, self.state_view)
    }

    /// Special case errors for type arguments to Move functions
    pub fn convert_type_argument_error(&self, idx: usize, error: VMError) -> ExecutionError {
        use move_core_types::vm_status::StatusCode;
        use sui_types::messages::TypeArgumentError;
        match error.major_status() {
            StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH => {
                ExecutionErrorKind::TypeArityMismatch.into()
            }
            StatusCode::TYPE_RESOLUTION_FAILURE => ExecutionErrorKind::TypeArgumentError {
                argument_idx: idx as TypeParameterIndex,
                kind: TypeArgumentError::TypeNotFound,
            }
            .into(),
            StatusCode::CONSTRAINT_NOT_SATISFIED => ExecutionErrorKind::TypeArgumentError {
                argument_idx: idx as TypeParameterIndex,
                kind: TypeArgumentError::ConstraintNotSatisfied,
            }
            .into(),
            _ => self.convert_vm_error(error),
        }
    }

    /// Returns true if the value at the argument's location is borrowed, mutably or immutably
    fn arg_is_borrowed(&self, arg: &Argument) -> bool {
        self.borrowed.contains_key(arg)
    }

    /// Returns true if the value at the argument's location is mutably borrowed
    fn arg_is_mut_borrowed(&self, arg: &Argument) -> bool {
        matches!(self.borrowed.get(arg), Some(/* mut */ true))
    }

    /// Internal helper to borrow the value for an argument and update the most recent usage
    fn borrow_mut(
        &mut self,
        arg: Argument,
        usage: UsageKind,
    ) -> Result<(Option<&InputObjectMetadata>, &mut Option<Value>), CommandArgumentError> {
        self.borrow_mut_impl(arg, Some(usage))
    }

    /// Internal helper to borrow the value for an argument
    /// Updates the most recent usage if specified
    fn borrow_mut_impl(
        &mut self,
        arg: Argument,
        update_last_usage: Option<UsageKind>,
    ) -> Result<(Option<&InputObjectMetadata>, &mut Option<Value>), CommandArgumentError> {
        let (metadata, result_value) = match arg {
            Argument::GasCoin => (self.gas.object_metadata.as_ref(), &mut self.gas.inner),
            Argument::Input(i) => {
                let Some(input_value) = self.inputs.get_mut(i as usize) else {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i });
                };
                (input_value.object_metadata.as_ref(), &mut input_value.inner)
            }
            Argument::Result(i) => {
                let Some(command_result) = self.results.get_mut(i as usize) else {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i });
                };
                if command_result.len() != 1 {
                    return Err(CommandArgumentError::InvalidResultArity { result_idx: i });
                }
                (None, &mut command_result[0])
            }
            Argument::NestedResult(i, j) => {
                let Some(command_result) = self.results.get_mut(i as usize) else {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i });
                };
                let Some(result_value) = command_result.get_mut(j as usize) else {
                    return Err(CommandArgumentError::SecondaryIndexOutOfBounds {
                        result_idx: i,
                        secondary_idx: j,
                    });
                };
                (None, result_value)
            }
        };
        if let Some(usage) = update_last_usage {
            result_value.last_usage_kind = Some(usage);
        }
        Ok((metadata, &mut result_value.value))
    }
}

/// Load an input object from the state_view
fn load_object<E: fmt::Debug, S: StorageView<E>>(
    state_view: &S,
    object_owner_map: &mut BTreeMap<ObjectID, Owner>,
    override_as_immutable: bool,
    id: ObjectID,
) -> Result<InputValue, ExecutionError> {
    let Some(obj) = state_view.read_object(&id) else {
        // protected by transaction input checker
        invariant_violation!(format!("Object {} does not exist yet", id));
    };
    // override_as_immutable ==> Owner::Shared
    assert_invariant!(
        !override_as_immutable || matches!(obj.owner, Owner::Shared { .. }),
        "override_as_immutable should only be set for shared objects"
    );
    let is_mutable_input = match obj.owner {
        Owner::AddressOwner(_) => true,
        Owner::Shared { .. } => !override_as_immutable,
        Owner::Immutable => false,
        Owner::ObjectOwner(_) => {
            // protected by transaction input checker
            invariant_violation!("ObjectOwner objects cannot be input")
        }
    };
    let object_metadata = InputObjectMetadata {
        id,
        is_mutable_input,
        owner: obj.owner,
        version: obj.version(),
    };
    let prev = object_owner_map.insert(id, obj.owner);
    // protected by transaction input checker
    assert_invariant!(prev.is_none(), format!("Duplicate input object {}", id));
    let mut obj_value = ObjectValue::from_object(obj)?;
    loaded_object_type_to_initial(state_view, &mut obj_value)?;
    Ok(InputValue::new_object(object_metadata, obj_value))
}

// For objects introduced by an upgraded package versions, replace the upgraded package ID in their
// type definition with the ID of the initial version of the package to satisfy the runtime (which
// only sees original package IDs).
fn loaded_object_type_to_initial<E: fmt::Debug, S: StorageView<E>>(
    state_view: &S,
    obj: &mut ObjectValue,
) -> Result<(), ExecutionError> {
    if let MoveObjectType::Other(struct_tag) = &obj.type_ {
        let upgraded_id: ObjectID = struct_tag.address.into();
        let pkg_obj = state_view.get_package(&upgraded_id);
        if pkg_obj.is_err() {
            invariant_violation!(format!("Package object {} does not exist yet", upgraded_id));
        }
        let Some(move_pkg) = pkg_obj.unwrap() else {
            invariant_violation!(format!("Object {} is not a Move package", upgraded_id));
        };
        let original_id = move_pkg.original_package_id();
        let mut new_struct_tag = struct_tag.clone();
        new_struct_tag.address = original_id.into();
        obj.type_ = MoveObjectType::Other(new_struct_tag);
    }
    // TODO: Should we translate MoveObjectType::Coin as well? At this point, the framework assumes
    // that it will always be at SUI_FRAMEWORK_ADDRESS (e.g., see coin::is_coin function)
    Ok(())
}

/// Load an a CallArg, either an object or a raw set of BCS bytes
fn load_call_arg<E: fmt::Debug, S: StorageView<E>>(
    state_view: &S,
    object_owner_map: &mut BTreeMap<ObjectID, Owner>,
    call_arg: CallArg,
) -> Result<InputValue, ExecutionError> {
    Ok(match call_arg {
        CallArg::Pure(bytes) => InputValue::new_raw(RawValueType::Any, bytes),
        CallArg::Object(obj_arg) => load_object_arg(state_view, object_owner_map, obj_arg)?,
    })
}

/// Load an ObjectArg from state view, marking if it can be treated as mutable or not
fn load_object_arg<E: fmt::Debug, S: StorageView<E>>(
    state_view: &S,
    object_owner_map: &mut BTreeMap<ObjectID, Owner>,
    obj_arg: ObjectArg,
) -> Result<InputValue, ExecutionError> {
    match obj_arg {
        ObjectArg::ImmOrOwnedObject((id, _, _)) => load_object(
            state_view,
            object_owner_map,
            /* imm override */ false,
            id,
        ),
        ObjectArg::SharedObject { id, mutable, .. } => load_object(
            state_view,
            object_owner_map,
            /* imm override */ !mutable,
            id,
        ),
    }
}

/// Generate an additional write for an ObjectValue
fn add_additional_write(
    additional_writes: &mut BTreeMap<ObjectID, AdditionalWrite>,
    owner: Owner,
    object_value: ObjectValue,
) {
    let ObjectValue {
        type_,
        has_public_transfer,
        contents,
        ..
    } = object_value;
    let bytes = match contents {
        ObjectContents::Coin(coin) => coin.to_bcs_bytes(),
        ObjectContents::Raw(bytes) => bytes,
    };
    let object_id = MoveObject::id_opt(&bytes).unwrap();
    let additional_write = AdditionalWrite {
        recipient: owner,
        type_,
        has_public_transfer,
        bytes,
    };
    additional_writes.insert(object_id, additional_write);
}

/// The max budget was deducted from the gas coin at the beginning of the transaction,
/// now we return exactly that amount. Gas will be charged by the execution engine
fn refund_max_gas_budget(
    additional_writes: &mut BTreeMap<ObjectID, AdditionalWrite>,
    gas_status: &SuiGasStatus,
    gas_id: ObjectID,
) -> Result<(), ExecutionError> {
    let Some(AdditionalWrite { bytes,.. }) = additional_writes.get_mut(&gas_id) else {
        invariant_violation!("Gas object cannot be wrapped or destroyed")
    };
    let Ok(mut coin) = Coin::from_bcs_bytes(bytes) else {
        invariant_violation!("Gas object must be a coin")
    };
    let Some(new_balance) = coin
        .balance
        .value()
        .checked_add(gas_status.max_gax_budget_in_balance()) else {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::CoinBalanceOverflow,
                "Gas coin too large after returning the max gas budget",
            ));
        };
    coin.balance = Balance::new(new_balance);
    *bytes = coin.to_bcs_bytes();
    Ok(())
}

/// Generate an MoveObject given an updated/written object
/// # Safety
///
/// This function assumes proper generation of has_public_transfer, either from the abilities of
/// the StructTag, or from the runtime correctly propagating from the inputs
unsafe fn create_written_object(
    protocol_config: &ProtocolConfig,
    input_object_metadata: &BTreeMap<ObjectID, InputObjectMetadata>,
    loaded_child_objects: &BTreeMap<ObjectID, SequenceNumber>,
    id: ObjectID,
    type_: MoveObjectType,
    has_public_transfer: bool,
    contents: Vec<u8>,
    write_kind: WriteKind,
) -> Result<MoveObject, ExecutionError> {
    debug_assert_eq!(
        id,
        MoveObject::id_opt(&contents).expect("object contents should start with an id")
    );
    let metadata_opt = input_object_metadata.get(&id);
    let loaded_child_version_opt = loaded_child_objects.get(&id);
    assert_invariant!(
        metadata_opt.is_none() || loaded_child_version_opt.is_none(),
        format!("Loaded {id} as a child, but that object was an input object")
    );

    let old_obj_ver = metadata_opt
        .map(|metadata| metadata.version)
        .or_else(|| loaded_child_version_opt.copied());

    debug_assert!(
        (write_kind == WriteKind::Mutate) == old_obj_ver.is_some(),
        "Inconsistent state: write_kind: {write_kind:?}, old ver: {old_obj_ver:?}"
    );

    MoveObject::new_from_execution(
        type_,
        has_public_transfer,
        old_obj_ver.unwrap_or_else(SequenceNumber::new),
        contents,
        protocol_config,
    )
}
