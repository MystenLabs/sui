// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use std::collections::BTreeSet;
    use std::{
        borrow::Borrow,
        collections::{BTreeMap, HashMap},
        sync::Arc,
    };

    use crate::adapter::new_native_extensions;
    use crate::error::convert_vm_error;
    use crate::execution_mode::ExecutionMode;
    use crate::execution_value::{CommandKind, ObjectContents, TryFromValue, Value};
    use crate::execution_value::{
        ExecutionState, InputObjectMetadata, InputValue, ObjectValue, RawValueType, ResultValue,
        UsageKind,
    };
    use crate::gas_charger::GasCharger;
    use crate::programmable_transactions::linkage_view::LinkageView;
    use crate::type_resolver::TypeTagResolver;
    use move_binary_format::{
        errors::{Location, PartialVMError, PartialVMResult, VMError, VMResult},
        file_format::{CodeOffset, FunctionDefinitionIndex, TypeParameterIndex},
        CompiledModule,
    };
    use move_core_types::resolver::ModuleResolver;
    use move_core_types::vm_status::StatusCode;
    use move_core_types::{
        account_address::AccountAddress,
        identifier::IdentStr,
        language_storage::{ModuleId, StructTag, TypeTag},
    };
    use move_trace_format::format::MoveTraceBuilder;
    use move_vm_runtime::native_extensions::NativeContextExtensions;
    use move_vm_runtime::{
        move_vm::MoveVM,
        session::{LoadedFunctionInstantiation, SerializedReturnValues},
    };
    use move_vm_types::data_store::DataStore;
    use move_vm_types::loaded_data::runtime_types::Type;
    use sui_move_natives::object_runtime::{
        self, get_all_uids, max_event_error, LoadedRuntimeObject, ObjectRuntime, RuntimeResults,
    };
    use sui_protocol_config::ProtocolConfig;
    use sui_types::execution::ExecutionResults;
    use sui_types::storage::{DenyListResult, PackageObject};
    use sui_types::{
        balance::Balance,
        base_types::{MoveObjectType, ObjectID, SuiAddress, TxContext},
        coin::Coin,
        error::{ExecutionError, ExecutionErrorKind},
        event::Event,
        execution::ExecutionResultsV2,
        metrics::LimitsMetrics,
        move_package::MovePackage,
        object::{Data, MoveObject, Object, ObjectInner, Owner},
        storage::BackingPackageStore,
        transaction::{Argument, CallArg, ObjectArg},
    };
    use sui_types::{error::command_argument_error, execution_status::CommandArgumentError};
    use tracing::instrument;

    /// Maintains all runtime state specific to programmable transactions
    pub struct ExecutionContext<'vm, 'state, 'a> {
        /// The protocol config
        pub protocol_config: &'a ProtocolConfig,
        /// Metrics for reporting exceeded limits
        pub metrics: Arc<LimitsMetrics>,
        /// The MoveVM
        pub vm: &'vm MoveVM,
        /// The LinkageView for this session
        pub linkage_view: LinkageView<'state>,
        pub native_extensions: NativeContextExtensions<'state>,
        /// The global state, used for resolving packages
        pub state_view: &'state dyn ExecutionState,
        /// A shared transaction context, contains transaction digest information and manages the
        /// creation of new object IDs
        pub tx_context: &'a mut TxContext,
        /// The gas charger used for metering
        pub gas_charger: &'a mut GasCharger,
        /// Additional transfers not from the Move runtime
        additional_transfers: Vec<(/* new owner */ SuiAddress, ObjectValue)>,
        /// Newly published packages
        new_packages: Vec<MovePackage>,
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
    }

    /// A write for an object that was generated outside of the Move ObjectRuntime
    struct AdditionalWrite {
        /// The new owner of the object
        recipient: Owner,
        /// the type of the object,
        type_: Type,
        /// if the object has public transfer or not, i.e. if it has store
        has_public_transfer: bool,
        /// contents of the object
        bytes: Vec<u8>,
    }

    impl<'vm, 'state, 'a> ExecutionContext<'vm, 'state, 'a> {
        #[instrument(name = "ExecutionContext::new", level = "trace", skip_all)]
        pub fn new(
            protocol_config: &'a ProtocolConfig,
            metrics: Arc<LimitsMetrics>,
            vm: &'vm MoveVM,
            state_view: &'state dyn ExecutionState,
            tx_context: &'a mut TxContext,
            gas_charger: &'a mut GasCharger,
            inputs: Vec<CallArg>,
        ) -> Result<Self, ExecutionError>
        where
            'a: 'state,
        {
            let mut linkage_view = LinkageView::new(Box::new(state_view.as_sui_resolver()));
            let mut input_object_map = BTreeMap::new();
            let inputs = inputs
                .into_iter()
                .map(|call_arg| {
                    load_call_arg(
                        protocol_config,
                        vm,
                        state_view,
                        &mut linkage_view,
                        &[],
                        &mut input_object_map,
                        call_arg,
                    )
                })
                .collect::<Result<_, ExecutionError>>()?;
            let gas = if let Some(gas_coin) = gas_charger.gas_coin() {
                let mut gas = load_object(
                    protocol_config,
                    vm,
                    state_view,
                    &mut linkage_view,
                    &[],
                    &mut input_object_map,
                    /* imm override */ false,
                    gas_coin,
                )?;
                // subtract the max gas budget. This amount is off limits in the programmable transaction,
                // so to mimic this "off limits" behavior, we act as if the coin has less balance than
                // it really does
                let Some(Value::Object(ObjectValue {
                    contents: ObjectContents::Coin(coin),
                    ..
                })) = &mut gas.inner.value
                else {
                    invariant_violation!("Gas object should be a populated coin")
                };

                let max_gas_in_balance = gas_charger.gas_budget();
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
            let native_extensions = new_native_extensions(
                state_view.as_child_resolver(),
                input_object_map,
                !gas_charger.is_unmetered(),
                protocol_config,
                metrics.clone(),
                tx_context.epoch(),
            );

            // Set the profiler if in CLI
            #[skip_checked_arithmetic]
            move_vm_profiler::tracing_feature_enabled! {
                use move_vm_profiler::GasProfiler;
                use move_vm_types::gas::GasMeter;

                let tx_digest = tx_context.digest();
                let remaining_gas: u64 =
                    move_vm_types::gas::GasMeter::remaining_gas(gas_charger.move_gas_status())
                        .into();
                gas_charger
                    .move_gas_status_mut()
                    .set_profiler(GasProfiler::init(
                        &vm.config().profiler_config,
                        format!("{}", tx_digest),
                        remaining_gas,
                    ));
            }

            Ok(Self {
                protocol_config,
                metrics,
                vm,
                linkage_view,
                native_extensions,
                state_view,
                tx_context,
                gas_charger,
                gas,
                inputs,
                results: vec![],
                additional_transfers: vec![],
                new_packages: vec![],
                user_events: vec![],
                borrowed: HashMap::new(),
            })
        }

        pub fn object_runtime(&mut self) -> &ObjectRuntime {
            self.native_extensions.get()
        }

        /// Create a new ID and update the state
        pub fn fresh_id(&mut self) -> Result<ObjectID, ExecutionError> {
            let object_id = self.tx_context.fresh_id();
            let object_runtime: &mut ObjectRuntime = self.native_extensions.get_mut();
            object_runtime
                .new_id(object_id)
                .map_err(|e| self.convert_vm_error(e.finish(Location::Undefined)))?;
            Ok(object_id)
        }

        /// Delete an ID and update the state
        pub fn delete_id(&mut self, object_id: ObjectID) -> Result<(), ExecutionError> {
            let object_runtime: &mut ObjectRuntime = self.native_extensions.get_mut();
            object_runtime
                .delete_id(object_id)
                .map_err(|e| self.convert_vm_error(e.finish(Location::Undefined)))
        }

        /// Set the link context for the session from the linkage information in the MovePackage found
        /// at `package_id`.  Returns the runtime ID of the link context package on success.
        pub fn set_link_context(
            &mut self,
            package_id: ObjectID,
        ) -> Result<AccountAddress, ExecutionError> {
            if self.linkage_view.has_linkage(package_id) {
                // Setting same context again, can skip.
                return Ok(self
                    .linkage_view
                    .original_package_id()
                    .unwrap_or(*package_id));
            }

            let package = package_for_linkage(&self.linkage_view, package_id)
                .map_err(|e| self.convert_vm_error(e))?;

            self.linkage_view.set_linkage(package.move_package())
        }

        /// Load a type using the context's current session.
        pub fn load_type(&mut self, type_tag: &TypeTag) -> VMResult<Type> {
            load_type(
                self.vm,
                &mut self.linkage_view,
                &self.new_packages,
                type_tag,
            )
        }

        /// Load a type using the context's current session.
        pub fn load_type_from_struct(&mut self, struct_tag: &StructTag) -> VMResult<Type> {
            load_type_from_struct(
                self.vm,
                &mut self.linkage_view,
                &self.new_packages,
                struct_tag,
            )
        }

        /// Takes the user events from the runtime and tags them with the Move module of the function
        /// that was invoked for the command
        pub fn take_user_events(
            &mut self,
            module_id: &ModuleId,
            function: FunctionDefinitionIndex,
            last_offset: CodeOffset,
        ) -> Result<(), ExecutionError> {
            let object_runtime: &mut ObjectRuntime = self.native_extensions.get_mut();
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
                .map(|(ty, tag, value)| {
                    let layout = self
                        .vm
                        .get_runtime()
                        .type_to_type_layout(&ty)
                        .map_err(|e| self.convert_vm_error(e))?;
                    let Some(bytes) = value.simple_serialize(&layout) else {
                        invariant_violation!("Failed to deserialize already serialized Move value");
                    };
                    Ok((module_id.clone(), tag, bytes))
                })
                .collect::<Result<Vec<_>, ExecutionError>>()?;
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
            let shared_obj_deletion_enabled = self.protocol_config.shared_object_deletion();
            let is_borrowed = self.arg_is_borrowed(&arg);
            let (input_metadata_opt, val_opt) = self.borrow_mut(arg, UsageKind::ByValue)?;
            let is_copyable = if let Some(val) = val_opt {
                val.is_copyable()
            } else {
                return Err(CommandArgumentError::InvalidValueUsage);
            };
            // If it was taken, we catch this above.
            // If it was not copyable and was borrowed, error as it creates a dangling reference in
            // effect.
            // We allow copyable values to be copied out even if borrowed, as we do not care about
            // referential transparency at this level.
            if !is_copyable && is_borrowed {
                return Err(CommandArgumentError::InvalidValueUsage);
            }
            // Gas coin cannot be taken by value, except in TransferObjects
            if matches!(arg, Argument::GasCoin)
                && !matches!(command_kind, CommandKind::TransferObjects)
            {
                return Err(CommandArgumentError::InvalidGasCoinUsage);
            }
            // Immutable objects cannot be taken by value
            if matches!(
                input_metadata_opt,
                Some(InputObjectMetadata::InputObject {
                    owner: Owner::Immutable,
                    ..
                })
            ) {
                return Err(CommandArgumentError::InvalidObjectByValue);
            }
            if (
                // this check can be removed after shared_object_deletion feature flag is removed
                matches!(
                    input_metadata_opt,
                    Some(InputObjectMetadata::InputObject {
                        owner: Owner::Shared { .. },
                        ..
                    })
                ) && !shared_obj_deletion_enabled
            ) {
                return Err(CommandArgumentError::InvalidObjectByValue);
            }

            // Any input object taken by value must be mutable
            if matches!(
                input_metadata_opt,
                Some(InputObjectMetadata::InputObject {
                    is_mutable_input: false,
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
            // mutable borrowing requires unique usage
            if self.arg_is_borrowed(&arg) {
                return Err(CommandArgumentError::InvalidValueUsage);
            }
            self.borrowed.insert(arg, /* is_mut */ true);
            let (input_metadata_opt, val_opt) = self.borrow_mut(arg, UsageKind::BorrowMut)?;
            let is_copyable = if let Some(val) = val_opt {
                val.is_copyable()
            } else {
                // error if taken
                return Err(CommandArgumentError::InvalidValueUsage);
            };
            if let Some(InputObjectMetadata::InputObject {
                is_mutable_input: false,
                ..
            }) = input_metadata_opt
            {
                return Err(CommandArgumentError::InvalidObjectByMutRef);
            }
            // if it is copyable, don't take it as we allow for the value to be copied even if
            // mutably borrowed
            let val = if is_copyable {
                val_opt.as_ref().unwrap().clone()
            } else {
                val_opt.take().unwrap()
            };
            V::try_from_value(val)
        }

        /// Mimics an immutable borrow by cloning the argument value without setting its value to None
        /// Errors if out of bounds, if the argument is mutably borrowed,
        /// or if it is unavailable (already taken)
        pub fn borrow_arg<V: TryFromValue>(
            &mut self,
            arg_idx: usize,
            arg: Argument,
            type_: &Type,
        ) -> Result<V, ExecutionError> {
            self.borrow_arg_(arg, type_)
                .map_err(|e| command_argument_error(e, arg_idx))
        }
        fn borrow_arg_<V: TryFromValue>(
            &mut self,
            arg: Argument,
            arg_type: &Type,
        ) -> Result<V, CommandArgumentError> {
            // immutable borrowing requires the value was not mutably borrowed.
            // If it was copied, that is okay.
            // If it was taken/moved, we will find out below
            if self.arg_is_mut_borrowed(&arg) {
                return Err(CommandArgumentError::InvalidValueUsage);
            }
            self.borrowed.insert(arg, /* is_mut */ false);
            let (_input_metadata_opt, val_opt) = self.borrow_mut(arg, UsageKind::BorrowImm)?;
            if val_opt.is_none() {
                return Err(CommandArgumentError::InvalidValueUsage);
            }

            // We eagerly reify receiving argument types at the first usage of them.
            if let &mut Some(Value::Receiving(_, _, ref mut recv_arg_type @ None)) = val_opt {
                let Type::Reference(inner) = arg_type else {
                    return Err(CommandArgumentError::InvalidValueUsage);
                };
                *recv_arg_type = Some(*(*inner).clone());
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
                old_value.is_none() || old_value.unwrap().is_copyable(),
                "Should never restore a non-taken value, unless it is copyable. \
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
            &self,
            modules: &[CompiledModule],
            dependencies: impl IntoIterator<Item = &'p MovePackage>,
        ) -> Result<MovePackage, ExecutionError> {
            MovePackage::new_initial(
                modules,
                self.protocol_config.max_move_package_size(),
                self.protocol_config.move_binary_format_version(),
                dependencies,
            )
        }

        /// Create a package upgrade from `previous_package` with `new_modules` and `dependencies`
        pub fn upgrade_package<'p>(
            &self,
            storage_id: ObjectID,
            previous_package: &MovePackage,
            new_modules: &[CompiledModule],
            dependencies: impl IntoIterator<Item = &'p MovePackage>,
        ) -> Result<MovePackage, ExecutionError> {
            previous_package.new_upgraded(
                storage_id,
                new_modules,
                self.protocol_config,
                dependencies,
            )
        }

        /// Add a newly created package to write as an effect of the transaction
        pub fn write_package(&mut self, package: MovePackage) {
            self.new_packages.push(package);
        }

        /// Return the last package pushed in `write_package`.
        /// This function should be used in block of codes that push a package, verify
        /// it, run the init and in case of error will remove the package.
        /// The package has to be pushed for the init to run correctly.
        pub fn pop_package(&mut self) -> Option<MovePackage> {
            self.new_packages.pop()
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
            let Self {
                protocol_config,
                vm,
                linkage_view,
                mut native_extensions,
                tx_context,
                gas_charger,
                additional_transfers,
                new_packages,
                gas,
                inputs,
                results,
                user_events,
                state_view,
                ..
            } = self;
            let tx_digest = tx_context.digest();
            let gas_id_opt = gas.object_metadata.as_ref().map(|info| info.id());
            let mut loaded_runtime_objects = BTreeMap::new();
            let mut additional_writes = BTreeMap::new();
            let mut by_value_shared_objects = BTreeSet::new();
            for input in inputs.into_iter().chain(std::iter::once(gas)) {
                let InputValue {
                    object_metadata:
                        Some(InputObjectMetadata::InputObject {
                            // We are only interested in mutable inputs.
                            is_mutable_input: true,
                            id,
                            version,
                            owner,
                        }),
                    inner: ResultValue { value, .. },
                } = input
                else {
                    continue;
                };
                loaded_runtime_objects.insert(
                    id,
                    LoadedRuntimeObject {
                        version,
                        is_modified: true,
                    },
                );
                if let Some(Value::Object(object_value)) = value {
                    add_additional_write(&mut additional_writes, owner, object_value)?;
                } else if owner.is_shared() {
                    by_value_shared_objects.insert(id);
                }
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
                                if abilities.has_drop()
                                    || (abilities.has_copy()
                                        && matches!(last_usage_kind, Some(UsageKind::ByValue)))
                                {
                                } else {
                                    let msg = if abilities.has_copy() {
                                        "The value has copy, but not drop. \
                                        Its last usage must be by-value so it can be taken."
                                    } else {
                                        "Unused value without drop"
                                    };
                                    return Err(ExecutionError::new_with_source(
                                        ExecutionErrorKind::UnusedValueWithoutDrop {
                                            result_idx: i as u16,
                                            secondary_idx: j as u16,
                                        },
                                        msg,
                                    ));
                                }
                            }
                            // Receiving arguments can be dropped without being received
                            Some(Value::Receiving(_, _, _)) => (),
                        }
                    }
                }
            }
            // add transfers from TransferObjects command
            for (recipient, object_value) in additional_transfers {
                let owner = Owner::AddressOwner(recipient);
                add_additional_write(&mut additional_writes, owner, object_value)?;
            }
            // Refund unused gas
            if let Some(gas_id) = gas_id_opt {
                refund_max_gas_budget(&mut additional_writes, gas_charger, gas_id)?;
            }

            let object_runtime: ObjectRuntime = native_extensions.remove();

            let RuntimeResults {
                writes,
                user_events: remaining_events,
                loaded_child_objects,
                mut created_object_ids,
                deleted_object_ids,
            } = object_runtime.finish()?;
            assert_invariant!(
                remaining_events.is_empty(),
                "Events should be taken after every Move call"
            );

            loaded_runtime_objects.extend(loaded_child_objects);

            let mut written_objects = BTreeMap::new();
            for package in new_packages {
                let package_obj = Object::new_from_package(package, tx_digest);
                let id = package_obj.id();
                created_object_ids.insert(id);
                written_objects.insert(id, package_obj);
            }
            for (id, additional_write) in additional_writes {
                let AdditionalWrite {
                    recipient,
                    type_,
                    has_public_transfer,
                    bytes,
                } = additional_write;
                // safe given the invariant that the runtime correctly propagates has_public_transfer
                let move_object = unsafe {
                    create_written_object(
                        vm,
                        &linkage_view,
                        protocol_config,
                        &loaded_runtime_objects,
                        id,
                        type_,
                        has_public_transfer,
                        bytes,
                    )?
                };
                let object = Object::new_move(move_object, recipient, tx_digest);
                written_objects.insert(id, object);
                if let Some(loaded) = loaded_runtime_objects.get_mut(&id) {
                    loaded.is_modified = true;
                }
            }

            for (id, (recipient, ty, value)) in writes {
                let abilities = vm.get_runtime().get_type_abilities(&ty).map_err(|e| {
                    convert_vm_error(
                        e,
                        vm,
                        &linkage_view,
                        protocol_config.resolve_abort_locations_to_package_id(),
                    )
                })?;
                let has_public_transfer = abilities.has_store();
                let layout = vm.get_runtime().type_to_type_layout(&ty).map_err(|e| {
                    convert_vm_error(
                        e,
                        vm,
                        &linkage_view,
                        protocol_config.resolve_abort_locations_to_package_id(),
                    )
                })?;
                let Some(bytes) = value.simple_serialize(&layout) else {
                    invariant_violation!("Failed to deserialize already serialized Move value");
                };
                // safe because has_public_transfer has been determined by the abilities
                let move_object = unsafe {
                    create_written_object(
                        vm,
                        &linkage_view,
                        protocol_config,
                        &loaded_runtime_objects,
                        id,
                        ty,
                        has_public_transfer,
                        bytes,
                    )?
                };
                let object = Object::new_move(move_object, recipient, tx_digest);
                written_objects.insert(id, object);
            }

            // Before finishing, ensure that any shared object taken by value by the transaction is either:
            // 1. Mutated (and still has a shared ownership); or
            // 2. Deleted.
            // Otherwise, the shared object operation is not allowed and we fail the transaction.
            for id in &by_value_shared_objects {
                // If it's been written it must have been reshared so must still have an ownership
                // of `Shared`.
                if let Some(obj) = written_objects.get(id) {
                    if !obj.is_shared() {
                        return Err(ExecutionError::new(
                            ExecutionErrorKind::SharedObjectOperationNotAllowed,
                            Some(
                                format!(
                                    "Shared object operation on {} not allowed: \
                                     cannot be frozen, transferred, or wrapped",
                                    id
                                )
                                .into(),
                            ),
                        ));
                    }
                } else {
                    // If it's not in the written objects, the object must have been deleted. Otherwise
                    // it's an error.
                    if !deleted_object_ids.contains(id) {
                        return Err(ExecutionError::new(
                            ExecutionErrorKind::SharedObjectOperationNotAllowed,
                            Some(
                                format!("Shared object operation on {} not allowed: \
                                         shared objects used by value must be re-shared if not deleted", id).into(),
                            ),
                        ));
                    }
                }
            }

            if protocol_config.enable_coin_deny_list_v2() {
                let DenyListResult {
                    result,
                    num_non_gas_coin_owners,
                } = state_view.check_coin_deny_list(&written_objects);
                gas_charger.charge_coin_transfers(protocol_config, num_non_gas_coin_owners)?;
                result?;
            }

            let user_events = user_events
                .into_iter()
                .map(|(module_id, tag, contents)| {
                    Event::new(
                        module_id.address(),
                        module_id.name(),
                        tx_context.sender(),
                        tag,
                        contents,
                    )
                })
                .collect();

            Ok(ExecutionResults::V2(ExecutionResultsV2 {
                written_objects,
                modified_objects: loaded_runtime_objects
                    .into_iter()
                    .filter_map(|(id, loaded)| loaded.is_modified.then_some(id))
                    .collect(),
                created_object_ids: created_object_ids.into_iter().collect(),
                deleted_object_ids: deleted_object_ids.into_iter().collect(),
                user_events,
            }))
        }

        /// Convert a VM Error to an execution one
        pub fn convert_vm_error(&self, error: VMError) -> ExecutionError {
            crate::error::convert_vm_error(
                error,
                self.vm,
                &self.linkage_view,
                self.protocol_config.resolve_abort_locations_to_package_id(),
            )
        }

        /// Special case errors for type arguments to Move functions
        pub fn convert_type_argument_error(&self, idx: usize, error: VMError) -> ExecutionError {
            use move_core_types::vm_status::StatusCode;
            use sui_types::execution_status::TypeArgumentError;
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
        ) -> Result<(Option<&InputObjectMetadata>, &mut Option<Value>), CommandArgumentError>
        {
            self.borrow_mut_impl(arg, Some(usage))
        }

        /// Internal helper to borrow the value for an argument
        /// Updates the most recent usage if specified
        fn borrow_mut_impl(
            &mut self,
            arg: Argument,
            update_last_usage: Option<UsageKind>,
        ) -> Result<(Option<&InputObjectMetadata>, &mut Option<Value>), CommandArgumentError>
        {
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

        pub(crate) fn execute_function_bypass_visibility(
            &mut self,
            module: &ModuleId,
            function_name: &IdentStr,
            ty_args: Vec<Type>,
            args: Vec<impl Borrow<[u8]>>,
            tracer: &mut Option<MoveTraceBuilder>,
        ) -> VMResult<SerializedReturnValues> {
            let gas_status = self.gas_charger.move_gas_status_mut();
            let mut data_store = SuiDataStore::new(&self.linkage_view, &self.new_packages);
            self.vm.get_runtime().execute_function_bypass_visibility(
                module,
                function_name,
                ty_args,
                args,
                &mut data_store,
                gas_status,
                &mut self.native_extensions,
                tracer.as_mut(),
            )
        }

        pub(crate) fn load_function(
            &mut self,
            module_id: &ModuleId,
            function_name: &IdentStr,
            type_arguments: &[Type],
        ) -> VMResult<LoadedFunctionInstantiation> {
            let mut data_store = SuiDataStore::new(&self.linkage_view, &self.new_packages);
            self.vm.get_runtime().load_function(
                module_id,
                function_name,
                type_arguments,
                &mut data_store,
            )
        }

        pub(crate) fn make_object_value(
            &mut self,
            type_: MoveObjectType,
            has_public_transfer: bool,
            used_in_non_entry_move_call: bool,
            contents: &[u8],
        ) -> Result<ObjectValue, ExecutionError> {
            make_object_value(
                self.protocol_config,
                self.vm,
                &mut self.linkage_view,
                &self.new_packages,
                type_,
                has_public_transfer,
                used_in_non_entry_move_call,
                contents,
            )
        }

        pub fn publish_module_bundle(
            &mut self,
            modules: Vec<Vec<u8>>,
            sender: AccountAddress,
        ) -> VMResult<()> {
            // TODO: publish_module_bundle() currently doesn't charge gas.
            // Do we want to charge there?
            let mut data_store = SuiDataStore::new(&self.linkage_view, &self.new_packages);
            self.vm.get_runtime().publish_module_bundle(
                modules,
                sender,
                &mut data_store,
                self.gas_charger.move_gas_status_mut(),
            )
        }
    }

    impl<'vm, 'state, 'a> TypeTagResolver for ExecutionContext<'vm, 'state, 'a> {
        fn get_type_tag(&self, type_: &Type) -> Result<TypeTag, ExecutionError> {
            self.vm
                .get_runtime()
                .get_type_tag(type_)
                .map_err(|e| self.convert_vm_error(e))
        }
    }

    /// Fetch the package at `package_id` with a view to using it as a link context.  Produces an error
    /// if the object at that ID does not exist, or is not a package.
    fn package_for_linkage(
        linkage_view: &LinkageView,
        package_id: ObjectID,
    ) -> VMResult<PackageObject> {
        use move_binary_format::errors::PartialVMError;
        use move_core_types::vm_status::StatusCode;

        match linkage_view.get_package_object(&package_id) {
            Ok(Some(package)) => Ok(package),
            Ok(None) => Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!("Cannot find link context {package_id} in store"))
                .finish(Location::Undefined)),
            Err(err) => Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!(
                    "Error loading link context {package_id} from store: {err}"
                ))
                .finish(Location::Undefined)),
        }
    }

    pub fn load_type_from_struct(
        vm: &MoveVM,
        linkage_view: &mut LinkageView,
        new_packages: &[MovePackage],
        struct_tag: &StructTag,
    ) -> VMResult<Type> {
        fn verification_error<T>(code: StatusCode) -> VMResult<T> {
            Err(PartialVMError::new(code).finish(Location::Undefined))
        }

        let StructTag {
            address,
            module,
            name,
            type_params,
        } = struct_tag;

        // Load the package that the struct is defined in, in storage
        let defining_id = ObjectID::from_address(*address);
        let package = package_for_linkage(linkage_view, defining_id)?;

        // Set the defining package as the link context while loading the
        // struct
        let original_address = linkage_view
            .set_linkage(package.move_package())
            .map_err(|e| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(e.to_string())
                    .finish(Location::Undefined)
            })?;

        let runtime_id = ModuleId::new(original_address, module.clone());
        let data_store = SuiDataStore::new(linkage_view, new_packages);
        let res = vm.get_runtime().load_type(&runtime_id, name, &data_store);
        linkage_view.reset_linkage();
        let (idx, struct_type) = res?;

        // Recursively load type parameters, if necessary
        let type_param_constraints = struct_type.type_param_constraints();
        if type_param_constraints.len() != type_params.len() {
            return verification_error(StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH);
        }

        if type_params.is_empty() {
            Ok(Type::Datatype(idx))
        } else {
            let loaded_type_params = type_params
                .iter()
                .map(|type_param| load_type(vm, linkage_view, new_packages, type_param))
                .collect::<VMResult<Vec<_>>>()?;

            // Verify that the type parameter constraints on the struct are met
            for (constraint, param) in type_param_constraints.zip(&loaded_type_params) {
                let abilities = vm.get_runtime().get_type_abilities(param)?;
                if !constraint.is_subset(abilities) {
                    return verification_error(StatusCode::CONSTRAINT_NOT_SATISFIED);
                }
            }

            Ok(Type::DatatypeInstantiation(Box::new((
                idx,
                loaded_type_params,
            ))))
        }
    }

    /// Load `type_tag` to get a `Type` in the provided `session`.  `session`'s linkage context may be
    /// reset after this operation, because during the operation, it may change when loading a struct.
    pub fn load_type(
        vm: &MoveVM,
        linkage_view: &mut LinkageView,
        new_packages: &[MovePackage],
        type_tag: &TypeTag,
    ) -> VMResult<Type> {
        Ok(match type_tag {
            TypeTag::Bool => Type::Bool,
            TypeTag::U8 => Type::U8,
            TypeTag::U16 => Type::U16,
            TypeTag::U32 => Type::U32,
            TypeTag::U64 => Type::U64,
            TypeTag::U128 => Type::U128,
            TypeTag::U256 => Type::U256,
            TypeTag::Address => Type::Address,
            TypeTag::Signer => Type::Signer,

            TypeTag::Vector(inner) => {
                Type::Vector(Box::new(load_type(vm, linkage_view, new_packages, inner)?))
            }
            TypeTag::Struct(struct_tag) => {
                return load_type_from_struct(vm, linkage_view, new_packages, struct_tag)
            }
        })
    }

    pub(crate) fn make_object_value(
        protocol_config: &ProtocolConfig,
        vm: &MoveVM,
        linkage_view: &mut LinkageView,
        new_packages: &[MovePackage],
        type_: MoveObjectType,
        has_public_transfer: bool,
        used_in_non_entry_move_call: bool,
        contents: &[u8],
    ) -> Result<ObjectValue, ExecutionError> {
        let contents = if type_.is_coin() {
            let Ok(coin) = Coin::from_bcs_bytes(contents) else {
                invariant_violation!("Could not deserialize a coin")
            };
            ObjectContents::Coin(coin)
        } else {
            ObjectContents::Raw(contents.to_vec())
        };

        let tag: StructTag = type_.into();
        let type_ = load_type_from_struct(vm, linkage_view, new_packages, &tag).map_err(|e| {
            crate::error::convert_vm_error(
                e,
                vm,
                linkage_view,
                protocol_config.resolve_abort_locations_to_package_id(),
            )
        })?;
        let has_public_transfer = if protocol_config.recompute_has_public_transfer_in_execution() {
            let abilities = vm.get_runtime().get_type_abilities(&type_).map_err(|e| {
                crate::error::convert_vm_error(
                    e,
                    vm,
                    linkage_view,
                    protocol_config.resolve_abort_locations_to_package_id(),
                )
            })?;
            abilities.has_store()
        } else {
            has_public_transfer
        };
        Ok(ObjectValue {
            type_,
            has_public_transfer,
            used_in_non_entry_move_call,
            contents,
        })
    }

    pub(crate) fn value_from_object(
        protocol_config: &ProtocolConfig,
        vm: &MoveVM,
        linkage_view: &mut LinkageView,
        new_packages: &[MovePackage],
        object: &Object,
    ) -> Result<ObjectValue, ExecutionError> {
        let ObjectInner {
            data: Data::Move(object),
            ..
        } = object.as_inner()
        else {
            invariant_violation!("Expected a Move object");
        };

        let used_in_non_entry_move_call = false;
        make_object_value(
            protocol_config,
            vm,
            linkage_view,
            new_packages,
            object.type_().clone(),
            object.has_public_transfer(),
            used_in_non_entry_move_call,
            object.contents(),
        )
    }

    /// Load an input object from the state_view
    fn load_object(
        protocol_config: &ProtocolConfig,
        vm: &MoveVM,
        state_view: &dyn ExecutionState,
        linkage_view: &mut LinkageView,
        new_packages: &[MovePackage],
        input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
        override_as_immutable: bool,
        id: ObjectID,
    ) -> Result<InputValue, ExecutionError> {
        let Some(obj) = state_view.read_object(&id) else {
            // protected by transaction input checker
            invariant_violation!("Object {} does not exist yet", id);
        };
        // override_as_immutable ==> Owner::Shared or Owner::ConsensusV2
        assert_invariant!(
            !override_as_immutable
                || matches!(obj.owner, Owner::Shared { .. } | Owner::ConsensusV2 { .. }),
            "override_as_immutable should only be set for consensus objects"
        );
        let is_mutable_input = match obj.owner {
            Owner::AddressOwner(_) => true,
            Owner::Shared { .. } | Owner::ConsensusV2 { .. } => !override_as_immutable,
            Owner::Immutable => false,
            Owner::ObjectOwner(_) => {
                // protected by transaction input checker
                invariant_violation!("ObjectOwner objects cannot be input")
            }
        };
        let owner = obj.owner.clone();
        let version = obj.version();
        let object_metadata = InputObjectMetadata::InputObject {
            id,
            is_mutable_input,
            owner: owner.clone(),
            version,
        };
        let obj_value = value_from_object(protocol_config, vm, linkage_view, new_packages, obj)?;
        let contained_uids = {
            let fully_annotated_layout = vm
                .get_runtime()
                .type_to_fully_annotated_layout(&obj_value.type_)
                .map_err(|e| {
                    convert_vm_error(
                        e,
                        vm,
                        linkage_view,
                        protocol_config.resolve_abort_locations_to_package_id(),
                    )
                })?;
            let mut bytes = vec![];
            obj_value.write_bcs_bytes(&mut bytes);
            match get_all_uids(&fully_annotated_layout, &bytes) {
                Err(e) => {
                    invariant_violation!("Unable to retrieve UIDs for object. Got error: {e}")
                }
                Ok(uids) => uids,
            }
        };
        let runtime_input = object_runtime::InputObject {
            contained_uids,
            owner,
            version,
        };
        let prev = input_object_map.insert(id, runtime_input);
        // protected by transaction input checker
        assert_invariant!(prev.is_none(), "Duplicate input object {}", id);
        Ok(InputValue::new_object(object_metadata, obj_value))
    }

    /// Load a CallArg, either an object or a raw set of BCS bytes
    fn load_call_arg(
        protocol_config: &ProtocolConfig,
        vm: &MoveVM,
        state_view: &dyn ExecutionState,
        linkage_view: &mut LinkageView,
        new_packages: &[MovePackage],
        input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
        call_arg: CallArg,
    ) -> Result<InputValue, ExecutionError> {
        Ok(match call_arg {
            CallArg::Pure(bytes) => InputValue::new_raw(RawValueType::Any, bytes),
            CallArg::Object(obj_arg) => load_object_arg(
                protocol_config,
                vm,
                state_view,
                linkage_view,
                new_packages,
                input_object_map,
                obj_arg,
            )?,
        })
    }

    /// Load an ObjectArg from state view, marking if it can be treated as mutable or not
    fn load_object_arg(
        protocol_config: &ProtocolConfig,
        vm: &MoveVM,
        state_view: &dyn ExecutionState,
        linkage_view: &mut LinkageView,
        new_packages: &[MovePackage],
        input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
        obj_arg: ObjectArg,
    ) -> Result<InputValue, ExecutionError> {
        match obj_arg {
            ObjectArg::ImmOrOwnedObject((id, _, _)) => load_object(
                protocol_config,
                vm,
                state_view,
                linkage_view,
                new_packages,
                input_object_map,
                /* imm override */ false,
                id,
            ),
            ObjectArg::SharedObject { id, mutable, .. } => load_object(
                protocol_config,
                vm,
                state_view,
                linkage_view,
                new_packages,
                input_object_map,
                /* imm override */ !mutable,
                id,
            ),
            ObjectArg::Receiving((id, version, _)) => {
                Ok(InputValue::new_receiving_object(id, version))
            }
        }
    }

    /// Generate an additional write for an ObjectValue
    fn add_additional_write(
        additional_writes: &mut BTreeMap<ObjectID, AdditionalWrite>,
        owner: Owner,
        object_value: ObjectValue,
    ) -> Result<(), ExecutionError> {
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
        let object_id = MoveObject::id_opt(&bytes).map_err(|e| {
            ExecutionError::invariant_violation(format!("No id for Raw object bytes. {e}"))
        })?;
        let additional_write = AdditionalWrite {
            recipient: owner,
            type_,
            has_public_transfer,
            bytes,
        };
        additional_writes.insert(object_id, additional_write);
        Ok(())
    }

    /// The max budget was deducted from the gas coin at the beginning of the transaction,
    /// now we return exactly that amount. Gas will be charged by the execution engine
    fn refund_max_gas_budget(
        additional_writes: &mut BTreeMap<ObjectID, AdditionalWrite>,
        gas_charger: &mut GasCharger,
        gas_id: ObjectID,
    ) -> Result<(), ExecutionError> {
        let Some(AdditionalWrite { bytes, .. }) = additional_writes.get_mut(&gas_id) else {
            invariant_violation!("Gas object cannot be wrapped or destroyed")
        };
        let Ok(mut coin) = Coin::from_bcs_bytes(bytes) else {
            invariant_violation!("Gas object must be a coin")
        };
        let Some(new_balance) = coin.balance.value().checked_add(gas_charger.gas_budget()) else {
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
        vm: &MoveVM,
        linkage_view: &LinkageView,
        protocol_config: &ProtocolConfig,
        objects_modified_at: &BTreeMap<ObjectID, LoadedRuntimeObject>,
        id: ObjectID,
        type_: Type,
        has_public_transfer: bool,
        contents: Vec<u8>,
    ) -> Result<MoveObject, ExecutionError> {
        debug_assert_eq!(
            id,
            MoveObject::id_opt(&contents).expect("object contents should start with an id")
        );
        let old_obj_ver = objects_modified_at
            .get(&id)
            .map(|obj: &LoadedRuntimeObject| obj.version);

        let type_tag = vm.get_runtime().get_type_tag(&type_).map_err(|e| {
            crate::error::convert_vm_error(
                e,
                vm,
                linkage_view,
                protocol_config.resolve_abort_locations_to_package_id(),
            )
        })?;

        let struct_tag = match type_tag {
            TypeTag::Struct(inner) => *inner,
            _ => invariant_violation!("Non struct type for object"),
        };
        MoveObject::new_from_execution(
            struct_tag.into(),
            has_public_transfer,
            old_obj_ver.unwrap_or_default(),
            contents,
            protocol_config,
        )
    }

    // Implementation of the `DataStore` trait for the Move VM.
    // When used during execution it may have a list of new packages that have
    // just been published in the current context. Those are used for module/type
    // resolution when executing module init.
    // It may be created with an empty slice of packages either when no publish/upgrade
    // are performed or when a type is requested not during execution.
    pub(crate) struct SuiDataStore<'state, 'a> {
        linkage_view: &'a LinkageView<'state>,
        new_packages: &'a [MovePackage],
    }

    impl<'state, 'a> SuiDataStore<'state, 'a> {
        pub(crate) fn new(
            linkage_view: &'a LinkageView<'state>,
            new_packages: &'a [MovePackage],
        ) -> Self {
            Self {
                linkage_view,
                new_packages,
            }
        }

        fn get_module(&self, module_id: &ModuleId) -> Option<&Vec<u8>> {
            for package in self.new_packages {
                let module = package.get_module(module_id);
                if module.is_some() {
                    return module;
                }
            }
            None
        }
    }

    // TODO: `DataStore` will be reworked and this is likely to disappear.
    //       Leaving this comment around until then as testament to better days to come...
    impl<'state, 'a> DataStore for SuiDataStore<'state, 'a> {
        fn link_context(&self) -> AccountAddress {
            self.linkage_view.link_context()
        }

        fn relocate(&self, module_id: &ModuleId) -> PartialVMResult<ModuleId> {
            self.linkage_view.relocate(module_id).map_err(|err| {
                PartialVMError::new(StatusCode::LINKER_ERROR)
                    .with_message(format!("Error relocating {module_id}: {err:?}"))
            })
        }

        fn defining_module(
            &self,
            runtime_id: &ModuleId,
            struct_: &IdentStr,
        ) -> PartialVMResult<ModuleId> {
            self.linkage_view
                .defining_module(runtime_id, struct_)
                .map_err(|err| {
                    PartialVMError::new(StatusCode::LINKER_ERROR).with_message(format!(
                        "Error finding defining module for {runtime_id}::{struct_}: {err:?}"
                    ))
                })
        }

        fn load_module(&self, module_id: &ModuleId) -> VMResult<Vec<u8>> {
            if let Some(bytes) = self.get_module(module_id) {
                return Ok(bytes.clone());
            }
            match self.linkage_view.get_module(module_id) {
                Ok(Some(bytes)) => Ok(bytes),
                Ok(None) => Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                    .with_message(format!("Cannot find {:?} in data cache", module_id))
                    .finish(Location::Undefined)),
                Err(err) => {
                    let msg = format!("Unexpected storage error: {:?}", err);
                    Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(msg)
                            .finish(Location::Undefined),
                    )
                }
            }
        }

        fn publish_module(&mut self, _module_id: &ModuleId, _blob: Vec<u8>) -> VMResult<()> {
            // we cannot panic here because during execution and publishing this is
            // currently called from the publish flow in the Move runtime
            Ok(())
        }
    }
}
