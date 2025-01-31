// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use std::cell::RefMut;
    use std::collections::BTreeSet;
    use std::rc::Rc;
    use std::{
        borrow::Borrow,
        collections::{BTreeMap, HashMap},
        sync::Arc,
    };

    use crate::adapter::new_native_extensions;
    use crate::error::convert_vm_error;
    use crate::execution_mode::ExecutionMode;
    use crate::execution_value::{CommandKind, ExecutionType, ObjectContents, TryFromValue, Value};
    use crate::execution_value::{
        ExecutionState, InputObjectMetadata, InputValue, ObjectValue, RawValueType, ResultValue,
        UsageKind,
    };
    use crate::gas_charger::GasCharger;
    use crate::linkage_resolution::{into_linkage_context, LinkageAnalysis, ResolvedLinkage};
    use crate::programmable_transactions::datastore::{PackageStore, SuiDataStore};
    use move_binary_format::{
        errors::{Location, VMError, VMResult},
        file_format::{CodeOffset, FunctionDefinitionIndex, TypeParameterIndex},
        CompiledModule,
    };
    use move_core_types::resolver::ModuleResolver;
    use move_core_types::{
        account_address::AccountAddress,
        identifier::IdentStr,
        language_storage::{ModuleId, StructTag, TypeTag},
    };
    use move_vm_runtime::execution::vm::{LoadedFunctionInformation, MoveVM};
    use move_vm_runtime::execution::Type;
    use move_vm_runtime::natives::extensions::{NativeContextExtensions, NativeContextMut};
    use move_vm_runtime::runtime::MoveRuntime;
    use move_vm_runtime::shared::serialization::SerializedReturnValues;
    use sui_move_natives::object_runtime::{
        self, get_all_uids, max_event_error, LoadedRuntimeObject, ObjectRuntime, RuntimeResults,
    };
    use sui_protocol_config::ProtocolConfig;
    use sui_types::error::SuiResult;
    use sui_types::execution::ExecutionResults;
    use sui_types::storage::DenyListResult;
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
        pub vm: &'vm MoveRuntime,
        /// The linkage analyzer to be used. This needs to be `dyn` as the implementation may
        /// change across protocol configs.
        pub linkage_analyzer: &'a mut dyn LinkageAnalysis,
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
        /// TODO(vm-rewrite): see about removing this
        /// Newly published packages
        pub new_packages: Vec<MovePackage>,
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
        type_: TypeTag,
        /// if the object has public transfer or not, i.e. if it has store
        has_public_transfer: bool,
        /// contents of the object
        bytes: Vec<u8>,
    }

    /// A `LinkedContext` is an execution context with a specific linkage that has been determined.
    /// This is usually for a specific command, but the linkage can be derived, e.g., for the
    /// entire PTB.
    pub struct LinkedContext<'ctx, 'vm, 'state, 'a> {
        /// The execution context
        pub ctx: &'ctx mut ExecutionContext<'vm, 'state, 'a>,
        pub vm_instance: MoveVM<'state>,
        /// The specified linkage for this linked context.
        /// This is a mapping of runtime_id -> storage_id
        pub linkage: ResolvedLinkage,
        /// A "reverse" linkage of storage_id -> runtime_id
        pub reverse_linkage: BTreeMap<ObjectID, ObjectID>,
    }

    impl<'ctx, 'vm, 'state, 'a> LinkedContext<'ctx, 'vm, 'state, 'a> {
        pub fn new(
            ctx: &'ctx mut ExecutionContext<'vm, 'state, 'a>,
            linkage: ResolvedLinkage,
        ) -> VMResult<Self> {
            let vm_instance = ctx.vm.make_vm_with_native_extensions(
                SuiDataStore::new(&ctx.state_view.as_sui_resolver(), &ctx.new_packages),
                into_linkage_context(linkage.clone()),
                ctx.native_extensions.clone(),
            )?;
            let reverse_linkage = linkage
                .iter()
                .map(|(k, v)| (*v, *k))
                .collect::<BTreeMap<_, _>>();
            Ok(Self {
                ctx,
                linkage,
                vm_instance,
                reverse_linkage,
            })
        }

        pub fn new_with_vm_instance(
            ctx: &'ctx mut ExecutionContext<'vm, 'state, 'a>,
            vm_instance: MoveVM<'state>,
            linkage: ResolvedLinkage,
        ) -> Self {
            let reverse_linkage = linkage
                .iter()
                .map(|(k, v)| (*v, *k))
                .collect::<BTreeMap<_, _>>();
            Self {
                ctx,
                linkage,
                vm_instance,
                reverse_linkage,
            }
        }

        pub fn destroy(self) -> NativeContextExtensions<'state> {
            let Self {
                vm_instance,
                ctx: _,
                linkage: _,
                reverse_linkage: _,
            } = self;
            vm_instance.into_extensions()
        }

        //---------------------------------------------------------------------------
        // Package Resolution
        //---------------------------------------------------------------------------

        pub fn runtime_id_for_storage_id(&self, storage_id: &ObjectID) -> Option<ObjectID> {
            self.reverse_linkage.get(storage_id).copied()
        }

        pub fn storage_id_for_runtime_id(&self, runtime_id: &ObjectID) -> Option<ObjectID> {
            self.linkage.get(runtime_id).copied()
        }

        //---------------------------------------------------------------------------
        // Type Resolution
        //---------------------------------------------------------------------------

        pub fn execution_type_for_runtime_type(
            &self,
            runtime_type: &Type,
        ) -> VMResult<ExecutionType> {
            Self::execution_type_for_runtime_type_impl(&self.vm_instance, runtime_type)
        }

        fn execution_type_for_runtime_type_impl(
            vm_instance: &MoveVM<'state>,
            runtime_type: &Type,
        ) -> VMResult<ExecutionType> {
            let type_tag = vm_instance.type_tag_for_type_defining_ids(runtime_type)?;
            let abilities = vm_instance.type_abilities(runtime_type)?;
            Ok(ExecutionType {
                type_: type_tag,
                abilities,
            })
        }

        /// Load a type. Linkage context is created ad-hoc for the type and its arguments.
        pub fn load_type(&mut self, type_tag: &TypeTag) -> VMResult<ExecutionType> {
            self.vm_instance
                .load_type(type_tag)
                .and_then(|runtime_type| self.execution_type_for_runtime_type(&runtime_type))
        }

        /// Load a type using the context's current session.
        pub fn load_type_from_struct(&mut self, struct_tag: &StructTag) -> VMResult<ExecutionType> {
            self.load_type(&TypeTag::Struct(Box::new(struct_tag.clone())))
        }

        //---------------------------------------------------------------------------
        // Error Resolution
        //---------------------------------------------------------------------------

        /// Convert a VM Error to an execution one
        pub fn convert_vm_error(&self, error: VMError) -> ExecutionError {
            crate::error::convert_vm_error(
                error,
                &self.linkage,
                &SuiDataStore::new(&self.ctx.state_view, &self.ctx.new_packages),
                &self.ctx.protocol_config,
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
                StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR => {
                    ExecutionErrorKind::TypeArgumentError {
                        argument_idx: idx as TypeParameterIndex,
                        kind: TypeArgumentError::TypeNotFound,
                    }
                    .into()
                }
                StatusCode::CONSTRAINT_NOT_SATISFIED => ExecutionErrorKind::TypeArgumentError {
                    argument_idx: idx as TypeParameterIndex,
                    kind: TypeArgumentError::ConstraintNotSatisfied,
                }
                .into(),
                _ => self.convert_vm_error(error),
            }
        }

        /// Takes the user events from the runtime and tags them with the Move module of the function
        /// that was invoked for the command
        pub fn take_user_events(
            &mut self,
            module_id: &ModuleId,
            function: FunctionDefinitionIndex,
            last_offset: CodeOffset,
        ) -> Result<(), ExecutionError> {
            let mut object_runtime: RefMut<ObjectRuntime> = self
                .vm_instance
                .extensions()
                .get::<NativeContextMut<ObjectRuntime>>()
                .get_mut();
            let events = object_runtime.take_user_events();
            let num_events = self.ctx.user_events.len() + events.len();
            let max_events = self.ctx.protocol_config.max_num_event_emit();
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
                        .vm_instance
                        .runtime_type_layout(&TypeTag::Struct(Box::new(tag.clone())))
                        .map_err(|e| self.convert_vm_error(e))?;
                    let Some(bytes) = value.simple_serialize(&layout) else {
                        invariant_violation!("Failed to deserialize already serialized Move value");
                    };
                    Ok((module_id.clone(), tag, bytes))
                })
                .collect::<Result<Vec<_>, ExecutionError>>()?;
            self.ctx.user_events.extend(new_events);
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
            let shared_obj_deletion_enabled = self.ctx.protocol_config.shared_object_deletion();
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
            self.ctx.borrowed.insert(arg, /* is_mut */ true);
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
            self.ctx.borrowed.insert(arg, /* is_mut */ false);
            let (_input_metadata_opt, val_opt) =
                Self::borrow_mut_impl(self.ctx, arg, Some(UsageKind::BorrowImm))?;
            if val_opt.is_none() {
                return Err(CommandArgumentError::InvalidValueUsage);
            }

            // We eagerly reify receiving argument types at the first usage of them.
            if let &mut Some(Value::Receiving(_, _, ref mut recv_arg_type @ None)) = val_opt {
                let Type::Reference(inner) = arg_type else {
                    return Err(CommandArgumentError::InvalidValueUsage);
                };
                *recv_arg_type = Some(
                    Self::execution_type_for_runtime_type_impl(&self.vm_instance, inner)
                        .map_err(|_| CommandArgumentError::InvalidValueUsage)?,
                );
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
            Mode::add_argument_update(updates, arg, &value)?;
            let was_mut_opt = self.ctx.borrowed.remove(&arg);
            assert_invariant!(
                was_mut_opt.is_some() && was_mut_opt.unwrap(),
                "Should never restore a non-mut borrowed value. \
                The take+restore is an implementation detail of mutable references"
            );
            // restore is exclusively used for mut
            let Ok((_, value_opt)) = Self::borrow_mut_impl(self.ctx, arg, None) else {
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

        /// Returns true if the value at the argument's location is borrowed, mutably or immutably
        fn arg_is_borrowed(&self, arg: &Argument) -> bool {
            self.ctx.borrowed.contains_key(arg)
        }

        /// Returns true if the value at the argument's location is mutably borrowed
        fn arg_is_mut_borrowed(&self, arg: &Argument) -> bool {
            matches!(self.ctx.borrowed.get(arg), Some(/* mut */ true))
        }

        /// Internal helper to borrow the value for an argument and update the most recent usage
        fn borrow_mut(
            &mut self,
            arg: Argument,
            usage: UsageKind,
        ) -> Result<(Option<&InputObjectMetadata>, &mut Option<Value>), CommandArgumentError>
        {
            Self::borrow_mut_impl(self.ctx, arg, Some(usage))
        }

        /// Internal helper to borrow the value for an argument
        /// Updates the most recent usage if specified
        fn borrow_mut_impl<'val>(
            ctx: &'val mut ExecutionContext<'vm, 'state, 'a>,
            arg: Argument,
            update_last_usage: Option<UsageKind>,
        ) -> Result<
            (Option<&'val InputObjectMetadata>, &'val mut Option<Value>),
            CommandArgumentError,
        > {
            let (metadata, result_value) = match arg {
                Argument::GasCoin => (ctx.gas.object_metadata.as_ref(), &mut ctx.gas.inner),
                Argument::Input(i) => {
                    let Some(input_value) = ctx.inputs.get_mut(i as usize) else {
                        return Err(CommandArgumentError::IndexOutOfBounds { idx: i });
                    };
                    (input_value.object_metadata.as_ref(), &mut input_value.inner)
                }
                Argument::Result(i) => {
                    let Some(command_result) = ctx.results.get_mut(i as usize) else {
                        return Err(CommandArgumentError::IndexOutOfBounds { idx: i });
                    };
                    if command_result.len() != 1 {
                        return Err(CommandArgumentError::InvalidResultArity { result_idx: i });
                    }
                    (None, &mut command_result[0])
                }
                Argument::NestedResult(i, j) => {
                    let Some(command_result) = ctx.results.get_mut(i as usize) else {
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
        ) -> VMResult<SerializedReturnValues> {
            let gas_status = self.ctx.gas_charger.move_gas_status_mut();
            self.vm_instance.execute_function_bypass_visibility(
                module,
                function_name,
                ty_args,
                args,
                gas_status,
            )
        }

        // NB: address in `module_id` is the runtime id
        pub(crate) fn load_function(
            &mut self,
            module_id: &ModuleId,
            function_name: &IdentStr,
            type_arguments: &[Type],
        ) -> VMResult<LoadedFunctionInformation> {
            self.vm_instance
                .function_information(module_id, function_name, type_arguments)
        }

        pub(crate) fn make_object_value(
            &mut self,
            type_: MoveObjectType,
            has_public_transfer: bool,
            used_in_non_entry_move_call: bool,
            contents: &[u8],
        ) -> Result<ObjectValue, ExecutionError> {
            let state = SuiDataStore::new(
                self.ctx
                    .state_view
                    .as_sui_resolver()
                    .as_backing_package_store(),
                &self.ctx.new_packages,
            );
            make_object_value(
                self.ctx.protocol_config,
                self.ctx.linkage_analyzer,
                &self.ctx.vm,
                &state,
                type_,
                has_public_transfer,
                used_in_non_entry_move_call,
                contents,
            )
        }

        pub fn publish_module_bundle<'outer_context>(
            &'outer_context mut self,
            runtime_id: AccountAddress,
            pkg: MovePackage,
        ) -> Result<(LinkedContext<'outer_context, 'vm, 'state, 'a>, MovePackage), ExecutionError>
        {
            // // TODO: publish_module_bundle() currently doesn't charge gas.
            // // Do we want to charge there?
            let serialized_package = pkg.into_serialized_move_package();
            let new_packages = [pkg];
            let data_store = SuiDataStore::new(&self.ctx.state_view, &new_packages);
            // Linkage is exactly what is specified in the package's linkage
            let linkage: ResolvedLinkage = serialized_package
                .linkage_table
                .iter()
                .map(|(k, v)| (ObjectID::from(*k), ObjectID::from(*v)))
                .collect();

            let (_, vm) = self
                .ctx
                .vm
                .validate_package(
                    data_store,
                    runtime_id,
                    serialized_package,
                    self.ctx.gas_charger.move_gas_status_mut(),
                    self.vm_instance.extensions().clone(),
                )
                .map_err(|e| self.convert_vm_error(e))?;
            let [pkg] = new_packages;
            Ok((
                LinkedContext::new_with_vm_instance(self.ctx, vm, linkage),
                pkg,
            ))
        }

        /// Transfer the object to a new owner
        pub fn transfer_object(
            &mut self,
            obj: ObjectValue,
            addr: SuiAddress,
        ) -> Result<(), ExecutionError> {
            self.ctx.additional_transfers.push((addr, obj));
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
                self.ctx.protocol_config.max_move_package_size(),
                self.ctx.protocol_config.move_binary_format_version(),
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
                self.ctx.protocol_config,
                dependencies,
            )
        }

        /// Add a newly created package to write as an effect of the transaction
        pub fn write_package(&mut self, package: MovePackage) {
            self.ctx.new_packages.push(package);
        }

        /// Return the last package pushed in `write_package`.
        /// This function should be used in block of codes that push a package, verify
        /// it, run the init and in case of error will remove the package.
        /// The package has to be pushed for the init to run correctly.
        pub fn pop_package(&mut self) -> Option<MovePackage> {
            self.ctx.new_packages.pop()
        }

        /// Finish a command: clearing the borrows and adding the results to the result vector
        pub fn push_command_results(&mut self, results: Vec<Value>) -> Result<(), ExecutionError> {
            assert_invariant!(
                self.ctx.borrowed.values().all(|is_mut| !is_mut),
                "all mut borrows should be restored"
            );
            // clear borrow state
            self.ctx.borrowed = HashMap::new();
            self.ctx
                .results
                .push(results.into_iter().map(ResultValue::new).collect());
            Ok(())
        }

        /// Create a new ID and update the state
        pub fn fresh_id(&mut self) -> Result<ObjectID, ExecutionError> {
            let object_id = self.ctx.tx_context.fresh_id();
            let mut object_runtime: RefMut<ObjectRuntime> = self
                .vm_instance
                .extensions()
                .get::<NativeContextMut<ObjectRuntime>>()
                .get_mut();
            object_runtime
                .new_id(object_id)
                .map_err(|e| self.convert_vm_error(e.finish(Location::Undefined)))?;
            Ok(object_id)
        }

        /// Delete an ID and update the state
        pub fn delete_id(&mut self, object_id: ObjectID) -> Result<(), ExecutionError> {
            let mut object_runtime: RefMut<ObjectRuntime> = self
                .vm_instance
                .extensions()
                .get::<NativeContextMut<ObjectRuntime>>()
                .get_mut();
            object_runtime
                .delete_id(object_id)
                .map_err(|e| self.convert_vm_error(e.finish(Location::Undefined)))
        }
    }

    // --------------------------------------------------------------------------------
    // --------------------------------------------------------------------------------
    // --------------------------------------------------------------------------------
    // --------------------------------------------------------------------------------
    // --------------------------------------------------------------------------------
    // --------------------------------------------------------------------------------

    impl<'vm, 'state, 'a> ExecutionContext<'vm, 'state, 'a> {
        #[instrument(name = "ExecutionContext::new", level = "trace", skip_all)]
        pub fn new(
            protocol_config: &'a ProtocolConfig,
            metrics: Arc<LimitsMetrics>,
            vm: &'vm MoveRuntime,
            linkage_analyzer: &'a mut dyn LinkageAnalysis,
            state_view: &'state dyn ExecutionState,
            tx_context: &'a mut TxContext,
            gas_charger: &'a mut GasCharger,
            inputs: Vec<CallArg>,
        ) -> Result<Self, ExecutionError>
        where
            'a: 'state,
        {
            let mut input_object_map = BTreeMap::new();
            let inputs = inputs
                .into_iter()
                .map(|call_arg| {
                    load_call_arg(
                        protocol_config,
                        linkage_analyzer,
                        vm,
                        state_view,
                        &mut input_object_map,
                        call_arg,
                    )
                })
                .collect::<Result<_, ExecutionError>>()?;
            let gas = if let Some(gas_coin) = gas_charger.gas_coin() {
                let mut gas = load_object(
                    protocol_config,
                    linkage_analyzer,
                    vm,
                    state_view,
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
                use move_vm_runtime::shared::gas::GasMeter;

                let tx_digest = tx_context.digest();
                let remaining_gas: u64 =
                    move_vm_runtime::shared::gas::GasMeter::remaining_gas(gas_charger.move_gas_status())
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
                linkage_analyzer,
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

        /// Determine the object changes and collect all user events
        pub fn finish<Mode: ExecutionMode>(self) -> Result<ExecutionResults, ExecutionError> {
            let Self {
                protocol_config,
                vm,
                tx_context,
                gas_charger,
                additional_transfers,
                new_packages,
                gas,
                inputs,
                results,
                user_events,
                state_view,
                linkage_analyzer,
                mut native_extensions,
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
                            Some(Value::Raw(RawValueType::Loaded { ty, .. }, _)) => {
                                // - nothing to check for drop
                                // - if it does not have drop, but has copy,
                                //   the last usage must be by value in order to "lie" and say that the
                                //   last usage is actually a take instead of a clone
                                // - Otherwise, an error
                                if ty.abilities.has_drop()
                                    || (ty.abilities.has_copy()
                                        && matches!(last_usage_kind, Some(UsageKind::ByValue)))
                                {
                                } else {
                                    let msg = if ty.abilities.has_copy() {
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

            let object_runtime: Rc<NativeContextMut<ObjectRuntime>> =
                native_extensions.remove::<NativeContextMut<ObjectRuntime>>();
            let Some(object_runtime) = Rc::into_inner(object_runtime) else {
                invariant_violation!("Object runtime has outstanding borrows at end of execution")
            };

            let RuntimeResults {
                writes,
                user_events: remaining_events,
                loaded_child_objects,
                mut created_object_ids,
                deleted_object_ids,
            } = object_runtime.into_inner().finish()?;
            assert_invariant!(
                remaining_events.is_empty(),
                "Events should be taken after every Move call"
            );

            loaded_runtime_objects.extend(loaded_child_objects);

            let mut written_objects = BTreeMap::new();
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

            let tys = writes
                .iter()
                .map(|(_, (_, ty, _))| ty.clone().into())
                .collect::<Vec<_>>();

            let (vm_instance, unified_linkage) = vm_for_type_tags(
                linkage_analyzer,
                vm,
                &tys,
                &SuiDataStore::new(
                    state_view.as_sui_resolver().as_backing_package_store(),
                    &new_packages,
                ),
            )
            .map_err(|e| {
                ExecutionError::new_with_source(
                    ExecutionErrorKind::VMVerificationOrDeserializationError,
                    e.to_string(),
                )
            })?;

            for (id, (recipient, ty, value)) in writes {
                // TODO: Lift this VM instance out the loop and create a combined linkage across
                // all writes.
                let tag = ty.into();
                let abilities = vm_instance
                    .load_type(&tag)
                    .and_then(|ty| vm_instance.type_abilities(&ty))
                    .map_err(|e| {
                        convert_vm_error(
                            e,
                            &unified_linkage,
                            &SuiDataStore::new(
                                state_view.as_sui_resolver().as_backing_package_store(),
                                &new_packages,
                            ),
                            protocol_config,
                        )
                    })?;
                let has_public_transfer = abilities.has_store();
                let layout = vm_instance.runtime_type_layout(&tag).map_err(|e| {
                    convert_vm_error(
                        e,
                        &unified_linkage,
                        &SuiDataStore::new(
                            state_view.as_sui_resolver().as_backing_package_store(),
                            &new_packages,
                        ),
                        protocol_config,
                    )
                })?;
                let Some(bytes) = value.simple_serialize(&layout) else {
                    invariant_violation!("Failed to deserialize already serialized Move value");
                };
                // safe because has_public_transfer has been determined by the abilities
                let move_object = unsafe {
                    create_written_object(
                        protocol_config,
                        &loaded_runtime_objects,
                        id,
                        tag,
                        has_public_transfer,
                        bytes,
                    )?
                };
                let object = Object::new_move(move_object, recipient, tx_digest);
                written_objects.insert(id, object);
            }

            for package in new_packages {
                let package_obj = Object::new_from_package(package, tx_digest);
                let id = package_obj.id();
                created_object_ids.insert(id);
                written_objects.insert(id, package_obj);
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
    }

    pub(crate) fn make_object_value(
        protocol_config: &ProtocolConfig,
        linkage_analyzer: &mut dyn LinkageAnalysis,
        vm: &MoveRuntime,
        state: &(impl PackageStore + ModuleResolver),
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
        let type_tag = TypeTag::Struct(Box::new(tag));
        let (vm, linkage) =
            vm_for_type_tags(linkage_analyzer, vm, [&type_tag], state).map_err(|_| {
                ExecutionError::from_kind(ExecutionErrorKind::VMVerificationOrDeserializationError)
            })?;
        let type_ = vm
            .load_type(&type_tag)
            .map_err(|e| convert_vm_error(e, &linkage, state, protocol_config))?;
        let abilities = vm
            .type_abilities(&type_)
            .map_err(|e| convert_vm_error(e, &linkage, state, protocol_config))?;
        let has_public_transfer = if protocol_config.recompute_has_public_transfer_in_execution() {
            abilities.has_store()
        } else {
            has_public_transfer
        };
        Ok(ObjectValue {
            type_: ExecutionType {
                type_: type_tag,
                abilities,
            },
            has_public_transfer,
            used_in_non_entry_move_call,
            contents,
        })
    }

    pub(crate) fn value_from_object(
        protocol_config: &ProtocolConfig,
        linkage_analyzer: &mut dyn LinkageAnalysis,
        vm: &MoveRuntime,
        state_view: &(impl PackageStore + ModuleResolver),
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
            linkage_analyzer,
            vm,
            state_view,
            object.type_().clone(),
            object.has_public_transfer(),
            used_in_non_entry_move_call,
            object.contents(),
        )
    }

    /// Load an input object from the state_view
    fn load_object(
        protocol_config: &ProtocolConfig,
        link_ctx: &mut dyn LinkageAnalysis,
        vm: &MoveRuntime,
        state_view: &dyn ExecutionState,
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
        let store = SuiDataStore::new(state_view.as_sui_resolver().as_backing_package_store(), &[]);
        let obj_value = value_from_object(protocol_config, link_ctx, vm, &store, obj)?;
        let contained_uids = {
            let (vm, link_ctx) = vm_for_type_tags(link_ctx, vm, [&obj_value.type_.type_], &store)
                .map_err(|_| {
                ExecutionError::from_kind(ExecutionErrorKind::VMVerificationOrDeserializationError)
            })?;
            let fully_annotated_layout = vm
                .annotated_type_layout(&obj_value.type_.type_)
                .map_err(|e| convert_vm_error(e, &link_ctx, &store, protocol_config))?;
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

    /// Load an a CallArg, either an object or a raw set of BCS bytes
    fn load_call_arg(
        protocol_config: &ProtocolConfig,
        linkage_analyzer: &mut dyn LinkageAnalysis,
        vm: &MoveRuntime,
        state_view: &dyn ExecutionState,
        input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
        call_arg: CallArg,
    ) -> Result<InputValue, ExecutionError> {
        Ok(match call_arg {
            CallArg::Pure(bytes) => InputValue::new_raw(RawValueType::Any, bytes),
            CallArg::Object(obj_arg) => load_object_arg(
                protocol_config,
                linkage_analyzer,
                vm,
                state_view,
                input_object_map,
                obj_arg,
            )?,
        })
    }

    /// Load an ObjectArg from state view, marking if it can be treated as mutable or not
    fn load_object_arg(
        protocol_config: &ProtocolConfig,
        linkage_analyzer: &mut dyn LinkageAnalysis,
        vm: &MoveRuntime,
        state_view: &dyn ExecutionState,
        input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
        obj_arg: ObjectArg,
    ) -> Result<InputValue, ExecutionError> {
        match obj_arg {
            ObjectArg::ImmOrOwnedObject((id, _, _)) => load_object(
                protocol_config,
                linkage_analyzer,
                vm,
                state_view,
                input_object_map,
                /* imm override */ false,
                id,
            ),
            ObjectArg::SharedObject { id, mutable, .. } => load_object(
                protocol_config,
                linkage_analyzer,
                vm,
                state_view,
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
            type_: type_.type_,
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
        protocol_config: &ProtocolConfig,
        objects_modified_at: &BTreeMap<ObjectID, LoadedRuntimeObject>,
        id: ObjectID,
        type_tag: TypeTag,
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

    fn identity_linkage_for_type_tags<'a>(
        linkage_analyzer: &mut dyn LinkageAnalysis,
        tags: impl IntoIterator<Item = &'a TypeTag>,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        let tags: Vec<_> = tags
            .into_iter()
            .flat_map(|tag| tag.all_addresses())
            .map(ObjectID::from)
            .collect();
        linkage_analyzer.generate_type_linkage(&tags, store)
    }

    fn identity_linkage_for_struct_tags<'a>(
        linkage_analyzer: &mut dyn LinkageAnalysis,
        tags: impl IntoIterator<Item = &'a StructTag>,
        store: &dyn PackageStore,
    ) -> Result<ResolvedLinkage, ExecutionError> {
        let tags: Vec<_> = tags
            .into_iter()
            .flat_map(|tag| tag.all_addresses())
            .map(ObjectID::from)
            .collect();
        linkage_analyzer.generate_type_linkage(&tags, store)
    }

    // NB: The typetag must be defining ID based
    fn vm_for_type_tags<'a, 'b>(
        linkage_analyzer: &mut dyn LinkageAnalysis,
        runtime: &'a MoveRuntime,
        tags: impl IntoIterator<Item = &'b TypeTag>,
        data_store: &(impl PackageStore + ModuleResolver),
    ) -> SuiResult<(MoveVM<'a>, ResolvedLinkage)> {
        let resolved_linkage = identity_linkage_for_type_tags(linkage_analyzer, tags, data_store)?;
        let linkage_context = into_linkage_context(resolved_linkage.clone());
        runtime
            .make_vm(data_store, linkage_context.clone())
            .map_err(|_| {
                ExecutionError::from_kind(ExecutionErrorKind::VMVerificationOrDeserializationError)
                    .into()
            })
            .map(|vm| (vm, resolved_linkage))
    }

    // NB: The struct tag must be defining ID based
    pub(crate) fn vm_for_struct_tags<'a, 'b>(
        linkage_analyzer: &mut dyn LinkageAnalysis,
        runtime: &'a MoveRuntime,
        tags: impl IntoIterator<Item = &'b StructTag>,
        data_store: &(impl PackageStore + ModuleResolver),
    ) -> SuiResult<MoveVM<'a>> {
        let linkage_context = into_linkage_context(identity_linkage_for_struct_tags(
            linkage_analyzer,
            tags,
            data_store,
        )?);
        runtime.make_vm(data_store, linkage_context).map_err(|_| {
            ExecutionError::from_kind(ExecutionErrorKind::VMVerificationOrDeserializationError)
                .into()
        })
    }
}
