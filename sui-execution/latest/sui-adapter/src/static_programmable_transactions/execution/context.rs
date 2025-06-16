// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    adapter,
    data_store::linked_data_store::LinkedDataStore,
    execution_mode::ExecutionMode,
    gas_charger::GasCharger,
    gas_meter::SuiGasMeter,
    programmable_transactions as legacy_ptb, sp,
    static_programmable_transactions::{
        better_todo,
        env::Env,
        execution::values::{
            ByteValue, InitialInput, InputObjectMetadata, InputValue, Inputs, Local, Locals, Value,
        },
        linkage::resolved_linkage::{ResolvedLinkage, RootedLinkage},
        typing::ast::{self as T, Type},
    },
};
use indexmap::IndexMap;
use move_binary_format::{
    CompiledModule,
    errors::{Location, PartialVMError, VMResult},
    file_format_common::VERSION_6,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
};
use move_trace_format::format::MoveTraceBuilder;
use move_vm_runtime::native_extensions::NativeContextExtensions;
use move_vm_types::{
    gas::GasMeter,
    values::{VMValueCast, Value as VMValue},
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
};
use sui_move_natives::object_runtime::{
    self, LoadedRuntimeObject, ObjectRuntime, RuntimeResults, get_all_uids, max_event_error,
};
use sui_types::{
    TypeTag,
    base_types::{MoveObjectType, ObjectID, TxContext, TxContextKind},
    error::{ExecutionError, ExecutionErrorKind},
    execution::ExecutionResults,
    execution_config_utils::to_binary_config,
    metrics::LimitsMetrics,
    move_package::{MovePackage, UpgradeCap, UpgradeReceipt, UpgradeTicket},
    object::{MoveObject, Object, Owner},
    storage::PackageObject,
};
use sui_verifier::INIT_FN_NAME;
use tracing::instrument;

macro_rules! unwrap {
    ($e:expr, $($args:expr),* $(,)?) => {
        match $e {
            Some(v) => v,
            None => {
                invariant_violation!("Unexpected none: {}", format!($($args),*))
            }
        }

    };
}

macro_rules! object_runtime_mut {
    ($context:ident) => {{
        $context
            .native_extensions
            .get_mut::<ObjectRuntime>()
            .map_err(|e| $context.env.convert_vm_error(e.finish(Location::Undefined)))
    }};
}

macro_rules! charge_gas_ {
    ($gas_charger:expr, $env:expr, $case:ident, $value_view:expr) => {{
        SuiGasMeter($gas_charger.move_gas_status_mut())
            .$case($value_view)
            .map_err(|e| $env.convert_vm_error(e.finish(Location::Undefined)))
    }};
}

macro_rules! charge_gas {
    ($context:ident, $case:ident, $value_view:expr) => {{ charge_gas_!($context.gas_charger, $context.env, $case, $value_view) }};
}

/// Type wrapper around Value to ensure safe usage
pub struct CtxValue(Value);

enum LocationValue<'a> {
    Loaded(Local<'a>),
    InputBytes(&'a mut Inputs, u16, Type),
}

#[derive(Copy, Clone)]
enum UsageKind {
    Move,
    Copy,
    Borrow(/* mut */ bool),
}

/// Maintains all runtime state specific to programmable transactions
pub struct Context<'env, 'pc, 'vm, 'state, 'linkage, 'gas> {
    pub env: &'env Env<'pc, 'vm, 'state, 'linkage>,
    /// Metrics for reporting exceeded limits
    pub metrics: Arc<LimitsMetrics>,
    pub native_extensions: NativeContextExtensions<'env>,
    /// A shared transaction context, contains transaction digest information and manages the
    /// creation of new object IDs
    pub tx_context: Rc<RefCell<TxContext>>,
    /// The gas charger used for metering
    pub gas_charger: &'gas mut GasCharger,
    /// User events are claimed after each Move call
    user_events: Vec<(ModuleId, StructTag, Vec<u8>)>,
    // runtime data
    /// The runtime value for the Gas coin, None if no gas coin is provided
    gas: Option<Inputs>,
    /// The runtime value for the inputs/call args
    inputs: Inputs,
    /// The results of a given command. For most commands, the inner vector will have length 1.
    /// It will only not be 1 for Move calls with multiple return values.
    /// Inner values are None if taken/moved by-value
    results: Vec<Locals>,
}

impl<'env, 'pc, 'vm, 'state, 'linkage, 'gas> Context<'env, 'pc, 'vm, 'state, 'linkage, 'gas> {
    #[instrument(name = "Context::new", level = "trace", skip_all)]
    pub fn new(
        env: &'env Env<'pc, 'vm, 'state, 'linkage>,
        metrics: Arc<LimitsMetrics>,
        tx_context: Rc<RefCell<TxContext>>,
        gas_charger: &'gas mut GasCharger,
        inputs: T::Inputs,
    ) -> Result<Self, ExecutionError>
    where
        'pc: 'state,
    {
        let mut input_object_map = BTreeMap::new();
        let inputs = inputs
            .into_iter()
            .map(|(arg, ty)| load_input_arg(gas_charger, env, &mut input_object_map, arg, ty))
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        let inputs = Inputs::new(inputs)?;
        let gas = match gas_charger.gas_coin() {
            Some(gas_coin) => {
                let ty = env.gas_coin_type()?;
                let (gas_metadata, gas_value) = load_object_arg_impl(
                    gas_charger,
                    env,
                    &mut input_object_map,
                    gas_coin,
                    true,
                    ty,
                )?;
                let mut gas_locals = Inputs::new([InitialInput::Object(gas_metadata, gas_value)])?;
                let InputValue::Loaded(gas_local) = gas_locals.get(0)? else {
                    invariant_violation!("Gas coin should be loaded, not bytes");
                };
                let gas_ref = gas_local.borrow()?;
                // We have already checked that the gas balance is enough to cover the gas budget
                let max_gas_in_balance = gas_charger.gas_budget();
                gas_ref.coin_ref_subtract_balance(max_gas_in_balance)?;
                Some(gas_locals)
            }
            None => None,
        };
        let native_extensions = adapter::new_native_extensions(
            env.state_view.as_child_resolver(),
            input_object_map,
            !gas_charger.is_unmetered(),
            env.protocol_config,
            metrics.clone(),
            tx_context.clone(),
        );

        Ok(Self {
            env,
            metrics,
            native_extensions,
            tx_context,
            gas_charger,
            user_events: vec![],
            gas,
            inputs,
            results: vec![],
        })
    }

    pub fn finish<Mode: ExecutionMode>(mut self) -> Result<ExecutionResults, ExecutionError> {
        let gas = std::mem::take(&mut self.gas);
        let inputs = std::mem::replace(&mut self.inputs, Inputs::new([])?);
        let mut loaded_runtime_objects = BTreeMap::new();
        let mut by_value_shared_objects = BTreeSet::new();
        let mut consensus_owner_objects = BTreeMap::new();
        let gas_object = gas
            .map(|g| g.into_objects())
            .transpose()?
            .unwrap_or_default();
        debug_assert!(gas_object.len() <= 1);
        let gas_id_opt = gas_object.first().map(|(o, _)| o.id);
        let input_objects = inputs.into_objects()?;
        for (metadata, value) in input_objects.into_iter().chain(gas_object) {
            let InputObjectMetadata {
                id,
                is_mutable_input,
                owner,
                version,
                type_,
            } = metadata;
            // We are only interested in mutable inputs.
            if !is_mutable_input {
                continue;
            }
            loaded_runtime_objects.insert(
                id,
                LoadedRuntimeObject {
                    version,
                    is_modified: true,
                },
            );
            if let Some(object) = value {
                self.transfer_object(owner, type_, CtxValue(object))?;
            } else if owner.is_shared() {
                by_value_shared_objects.insert(id);
            } else if matches!(owner, Owner::ConsensusAddressOwner { .. }) {
                consensus_owner_objects.insert(id, owner.clone());
            }
        }

        let Self {
            env,
            mut native_extensions,
            tx_context,
            gas_charger,
            user_events,
            ..
        } = self;
        let ref_context: &RefCell<TxContext> = &tx_context;
        let tx_context: &TxContext = &ref_context.borrow();
        let tx_digest = ref_context.borrow().digest();

        let object_runtime: ObjectRuntime = native_extensions
            .remove()
            .map_err(|e| env.convert_vm_error(e.finish(Location::Undefined)))?;

        let RuntimeResults {
            mut writes,
            user_events: remaining_events,
            loaded_child_objects,
            mut created_object_ids,
            deleted_object_ids,
            accumulator_events,
        } = object_runtime.finish()?;
        assert_invariant!(
            remaining_events.is_empty(),
            "Events should be taken after every Move call"
        );
        // Refund unused gas
        if let Some(gas_id) = gas_id_opt {
            refund_max_gas_budget(&mut writes, gas_charger, gas_id)?;
        }

        loaded_runtime_objects.extend(loaded_child_objects);

        let mut written_objects = BTreeMap::new();
        for (_, package) in self.env.linkable_store.take_new_packages().into_iter() {
            let Some(package) = Rc::into_inner(package) else {
                invariant_violation!(
                    "Package should have no outstanding references at end of execution"
                );
            };
            let package_obj = Object::new_from_package(package, tx_digest);
            let id = package_obj.id();
            created_object_ids.insert(id);
            written_objects.insert(id, package_obj);
        }

        for (id, (recipient, ty, value)) in writes {
            let ty: Type = env.load_type_from_struct(&ty.clone().into())?;
            let abilities = ty.abilities();
            let has_public_transfer = abilities.has_store();
            let Some(bytes) = value.serialize() else {
                invariant_violation!("Failed to deserialize already serialized Move value");
            };
            // safe because has_public_transfer has been determined by the abilities
            let move_object = unsafe {
                create_written_object::<Mode>(
                    env,
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
                            format!(
                                "Shared object operation on {} not allowed: \
                                     shared objects used by value must be re-shared if not deleted",
                                id
                            )
                            .into(),
                        ),
                    ));
                }
            }
        }

        legacy_ptb::context::finish(
            env.protocol_config,
            env.state_view,
            gas_charger,
            tx_context,
            &by_value_shared_objects,
            &consensus_owner_objects,
            loaded_runtime_objects,
            written_objects,
            created_object_ids,
            deleted_object_ids,
            user_events,
            accumulator_events,
        )
    }

    pub fn object_runtime(&self) -> Result<&ObjectRuntime, ExecutionError> {
        self.native_extensions
            .get::<ObjectRuntime>()
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))
    }

    pub fn take_user_events(&mut self, function: &T::LoadedFunction) -> Result<(), ExecutionError> {
        let events = object_runtime_mut!(self)?.take_user_events();
        let num_events = self.user_events.len() + events.len();
        let max_events = self.env.protocol_config.max_num_event_emit();
        if num_events as u64 > max_events {
            let err = max_event_error(max_events)
                .at_code_offset(function.definition_index, function.instruction_length)
                .finish(Location::Module(function.storage_id.clone()));
            return Err(self.env.convert_linked_vm_error(err, &function.linkage));
        }
        let new_events = events
            .into_iter()
            .map(|(tag, value)| {
                let Some(bytes) = value.serialize() else {
                    invariant_violation!("Failed to serialize Move event");
                };
                Ok((function.storage_id.clone(), tag, bytes))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        self.user_events.extend(new_events);
        Ok(())
    }

    //
    // Arguments and Values
    //

    /// NOTE! This does not charge gas and should not be used directly. It is exposed for
    /// dev-inspect
    fn location_value<'a>(
        &'a mut self,
        location: T::Location,
        ty: Type,
    ) -> Result<
        (
            &'a mut GasCharger,
            &'env Env<'pc, 'vm, 'state, 'linkage>,
            LocationValue<'a>,
        ),
        ExecutionError,
    > {
        let v = match location {
            T::Location::GasCoin => {
                let () =
                    better_todo!("better error here? How do we handle if there is no gas coin?");
                let gas_locals = unwrap!(self.gas.as_mut(), "Gas coin not provided");
                let InputValue::Loaded(gas_local) = gas_locals.get(0)? else {
                    invariant_violation!("Gas coin should be loaded, not bytes");
                };
                LocationValue::Loaded(gas_local)
            }
            T::Location::Result(i, j) => {
                let result = unwrap!(self.results.get_mut(i as usize), "bounds already verified");
                let v = result.local(j)?;
                LocationValue::Loaded(v)
            }
            T::Location::Input(i) => {
                let is_bytes = self.inputs.is_bytes(i);
                if is_bytes {
                    LocationValue::InputBytes(&mut self.inputs, i, ty)
                } else {
                    let InputValue::Loaded(v) = self.inputs.get(i)? else {
                        invariant_violation!("Expected local");
                    };
                    LocationValue::Loaded(v)
                }
            }
        };
        Ok((self.gas_charger, self.env, v))
    }

    fn location(
        &mut self,
        usage: UsageKind,
        location: T::Location,
        ty: Type,
    ) -> Result<Value, ExecutionError> {
        let (gas_charger, env, lv) = self.location_value(location, ty)?;
        let mut local = match lv {
            LocationValue::Loaded(v) => v,
            LocationValue::InputBytes(inputs, i, ty) => match usage {
                UsageKind::Move | UsageKind::Borrow(true) => {
                    let bytes = match inputs.get(i)? {
                        InputValue::Bytes(v) => v,
                        InputValue::Loaded(_) => invariant_violation!("Expected bytes"),
                    };
                    let value = load_byte_value(gas_charger, env, bytes, ty)?;
                    inputs.fix(i, value)?;
                    match inputs.get(i)? {
                        InputValue::Loaded(v) => v,
                        InputValue::Bytes(_) => invariant_violation!("Expected fixed value"),
                    }
                }
                UsageKind::Copy | UsageKind::Borrow(false) => {
                    let bytes = match inputs.get(i)? {
                        InputValue::Bytes(v) => v,
                        InputValue::Loaded(_) => invariant_violation!("Expected bytes"),
                    };
                    return load_byte_value(gas_charger, env, bytes, ty);
                }
            },
        };
        Ok(match usage {
            UsageKind::Move => local.move_()?,
            UsageKind::Copy => {
                let value = local.copy()?;
                charge_gas_!(gas_charger, env, charge_copy_loc, &value)?;
                value
            }
            UsageKind::Borrow(_) => local.borrow()?,
        })
    }

    fn location_usage(&mut self, usage: T::Usage, ty: Type) -> Result<Value, ExecutionError> {
        match usage {
            T::Usage::Move(location) => self.location(UsageKind::Move, location, ty),
            T::Usage::Copy { location, .. } => self.location(UsageKind::Copy, location, ty),
        }
    }

    fn argument_value(&mut self, sp!(_, (arg_, ty)): T::Argument) -> Result<Value, ExecutionError> {
        match arg_ {
            T::Argument__::Use(usage) => self.location_usage(usage, ty),
            T::Argument__::Borrow(is_mut, location) => {
                let ty = match ty {
                    Type::Reference(_, inner) => (*inner).clone(),
                    _ => invariant_violation!("Expected reference type"),
                };
                self.location(UsageKind::Borrow(is_mut), location, ty)
            }
            T::Argument__::Read(usage) => {
                let reference = self.location_usage(usage, ty)?;
                charge_gas!(self, charge_read_ref, &reference)?;
                reference.read_ref()
            }
        }
    }

    pub fn argument<V>(&mut self, arg: T::Argument) -> Result<V, ExecutionError>
    where
        VMValue: VMValueCast<V>,
    {
        let value = self.argument_value(arg)?;
        let value: V = value.cast()?;
        Ok(value)
    }

    pub fn arguments<V>(&mut self, args: Vec<T::Argument>) -> Result<Vec<V>, ExecutionError>
    where
        VMValue: VMValueCast<V>,
    {
        args.into_iter().map(|arg| self.argument(arg)).collect()
    }

    pub fn result(&mut self, result: Vec<CtxValue>) -> Result<(), ExecutionError> {
        self.results
            .push(Locals::new(result.into_iter().map(|v| v.0))?);
        Ok(())
    }

    pub fn copy_value(&mut self, value: &CtxValue) -> Result<CtxValue, ExecutionError> {
        Ok(CtxValue(copy_value(self.gas_charger, self.env, &value.0)?))
    }

    pub fn new_coin(&mut self, amount: u64) -> Result<CtxValue, ExecutionError> {
        let id = self.tx_context.borrow_mut().fresh_id();
        object_runtime_mut!(self)?
            .new_id(id)
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
        Ok(CtxValue(Value::coin(id, amount)))
    }

    pub fn destroy_coin(&mut self, coin: CtxValue) -> Result<u64, ExecutionError> {
        let (id, amount) = coin.0.unpack_coin()?;
        object_runtime_mut!(self)?
            .delete_id(id)
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
        Ok(amount)
    }

    pub fn new_upgrade_cap(&mut self, storage_id: ObjectID) -> Result<CtxValue, ExecutionError> {
        let id = self.tx_context.borrow_mut().fresh_id();
        object_runtime_mut!(self)?
            .new_id(id)
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
        let cap = UpgradeCap::new(id, storage_id);
        Ok(CtxValue(Value::upgrade_cap(cap)))
    }

    pub fn upgrade_receipt(
        &self,
        upgrade_ticket: UpgradeTicket,
        upgraded_package_id: ObjectID,
    ) -> CtxValue {
        let receipt = UpgradeReceipt::new(upgrade_ticket, upgraded_package_id);
        CtxValue(Value::upgrade_receipt(receipt))
    }

    //
    // Move calls
    //

    pub fn vm_move_call(
        &mut self,
        function: T::LoadedFunction,
        mut args: Vec<CtxValue>,
        trace_builder_opt: Option<&mut MoveTraceBuilder>,
    ) -> Result<Vec<CtxValue>, ExecutionError> {
        match function.tx_context {
            TxContextKind::None => (),
            TxContextKind::Mutable | TxContextKind::Immutable => args.push(CtxValue(
                Value::tx_context(self.tx_context.borrow().digest())?,
            )),
        }
        let result = self
            .execute_function_bypass_visibility(
                &function.storage_id,
                &function.name,
                &function.type_arguments,
                args,
                &function.linkage,
                trace_builder_opt,
            )
            .map_err(|e| self.env.convert_vm_error(e))?;
        self.take_user_events(&function)?;
        Ok(result)
    }

    pub fn execute_function_bypass_visibility(
        &mut self,
        storage_id: &ModuleId,
        function_name: &IdentStr,
        ty_args: &[Type],
        args: Vec<CtxValue>,
        linkage: &RootedLinkage,
        tracer: Option<&mut MoveTraceBuilder>,
    ) -> VMResult<Vec<CtxValue>> {
        let ty_args = {
            // load type arguments for VM
            let _ = ty_args;
            better_todo!("LOADING")
        };
        let gas_status = self.gas_charger.move_gas_status_mut();
        let mut data_store = LinkedDataStore::new(linkage, self.env.linkable_store);
        let values = self
            .env
            .vm
            .get_runtime()
            .execute_function_with_values_bypass_visibility(
                storage_id,
                function_name,
                ty_args,
                args.into_iter().map(|v| v.0.into()).collect(),
                &mut data_store,
                &mut SuiGasMeter(gas_status),
                &mut self.native_extensions,
                tracer,
            )?;
        Ok(values.into_iter().map(|v| CtxValue(v.into())).collect())
    }

    //
    // Publish and Upgrade
    //

    // is_upgrade is used for gas charging. Assumed to be a new publish if false.
    pub fn deserialize_modules(
        &mut self,
        module_bytes: &[Vec<u8>],
        is_upgrade: bool,
    ) -> Result<Vec<CompiledModule>, ExecutionError> {
        assert_invariant!(
            !module_bytes.is_empty(),
            "empty package is checked in transaction input checker"
        );
        let total_bytes = module_bytes.iter().map(|v| v.len()).sum();
        if is_upgrade {
            self.gas_charger.charge_upgrade_package(total_bytes)?
        } else {
            self.gas_charger.charge_publish_package(total_bytes)?
        }

        let binary_config = to_binary_config(self.env.protocol_config);
        let modules = module_bytes
            .iter()
            .map(|b| {
                CompiledModule::deserialize_with_config(b, &binary_config)
                    .map_err(|e| e.finish(Location::Undefined))
            })
            .collect::<VMResult<Vec<CompiledModule>>>()
            .map_err(|e| self.env.convert_vm_error(e))?;
        Ok(modules)
    }

    fn fetch_package(&mut self, dependency_id: &ObjectID) -> Result<PackageObject, ExecutionError> {
        legacy_ptb::execution::fetch_package(&self.env.state_view, dependency_id)
    }

    fn fetch_packages(
        &mut self,
        dependency_ids: &[ObjectID],
    ) -> Result<Vec<PackageObject>, ExecutionError> {
        legacy_ptb::execution::fetch_packages(&self.env.state_view, dependency_ids)
    }

    fn publish_and_verify_modules(
        &mut self,
        package_id: ObjectID,
        modules: &[CompiledModule],
        linkage: &RootedLinkage,
    ) -> Result<(), ExecutionError> {
        // TODO(https://github.com/MystenLabs/sui/issues/69): avoid this redundant serialization by exposing VM API that allows us to run the linker directly on `Vec<CompiledModule>`
        let binary_version = self.env.protocol_config.move_binary_format_version();
        let new_module_bytes: Vec<_> = modules
            .iter()
            .map(|m| {
                let mut bytes = Vec::new();
                let version = if binary_version > VERSION_6 {
                    m.version
                } else {
                    VERSION_6
                };
                m.serialize_with_version(version, &mut bytes).unwrap();
                bytes
            })
            .collect();
        let mut data_store = LinkedDataStore::new(linkage, self.env.linkable_store);
        self.env
            .vm
            .get_runtime()
            .publish_module_bundle(
                new_module_bytes,
                AccountAddress::from(package_id),
                &mut data_store,
                &mut SuiGasMeter(self.gas_charger.move_gas_status_mut()),
            )
            .map_err(|e| self.env.convert_vm_error(e))?;

        // run the Sui verifier
        for module in modules {
            // Run Sui bytecode verifier, which runs some additional checks that assume the Move
            // bytecode verifier has passed.
            sui_verifier::verifier::sui_verify_module_unmetered(
                module,
                &BTreeMap::new(),
                &self
                    .env
                    .protocol_config
                    .verifier_config(/* signing_limits */ None),
            )?;
        }

        Ok(())
    }

    fn init_modules(
        &mut self,
        modules: &[CompiledModule],
        linkage: &RootedLinkage,
        mut trace_builder_opt: Option<&mut MoveTraceBuilder>,
    ) -> Result<(), ExecutionError> {
        let modules_to_init = modules.iter().filter_map(|module| {
            for fdef in &module.function_defs {
                let fhandle = module.function_handle_at(fdef.function);
                let fname = module.identifier_at(fhandle.name);
                if fname == INIT_FN_NAME {
                    return Some(module.self_id());
                }
            }
            None
        });

        for module_id in modules_to_init {
            let args = vec![CtxValue(Value::tx_context(
                self.tx_context.borrow().digest(),
            )?)];
            let return_values = self
                .execute_function_bypass_visibility(
                    &module_id,
                    INIT_FN_NAME,
                    &[],
                    args,
                    linkage,
                    trace_builder_opt.as_deref_mut(),
                )
                .map_err(|e| self.env.convert_vm_error(e))?;

            assert_invariant!(
                return_values.is_empty(),
                "init should not have return values"
            )
        }

        Ok(())
    }

    pub fn publish_and_init_package<Mode: ExecutionMode>(
        &mut self,
        mut modules: Vec<CompiledModule>,
        dep_ids: &[ObjectID],
        linkage: ResolvedLinkage,
        trace_builder_opt: Option<&mut MoveTraceBuilder>,
    ) -> Result<ObjectID, ExecutionError> {
        let runtime_id = if <Mode>::packages_are_predefined() {
            // do not calculate or substitute id for predefined packages
            (*modules[0].self_id().address()).into()
        } else {
            // It should be fine that this does not go through the object runtime since it does not
            // need to know about new packages created, since Move objects and Move packages
            // cannot interact
            let id = self.tx_context.borrow_mut().fresh_id();
            adapter::substitute_package_id(&mut modules, id)?;
            id
        };

        let dependencies = self.fetch_packages(dep_ids)?;
        let package = Rc::new(MovePackage::new_initial(
            &modules,
            self.env.protocol_config.max_move_package_size(),
            self.env.protocol_config.move_binary_format_version(),
            dependencies.iter().map(|p| p.move_package()),
        )?);
        let package_id = package.id();

        // Here we optimistically push the package that is being published/upgraded
        // and if there is an error of any kind (verification or module init) we
        // remove it.
        // The call to `pop_last_package` later is fine because we cannot re-enter and
        // the last package we pushed is the one we are verifying and running the init from
        let linkage = RootedLinkage::new(*package_id, linkage);

        self.env.linkable_store.push_package(package_id, package)?;
        let res = self
            .publish_and_verify_modules(runtime_id, &modules, &linkage)
            .and_then(|_| self.init_modules(&modules, &linkage, trace_builder_opt));
        match res {
            Ok(()) => Ok(runtime_id),
            Err(e) => {
                self.env.linkable_store.pop_package(package_id)?;
                Err(e)
            }
        }
    }

    pub fn upgrade(
        &mut self,
        mut modules: Vec<CompiledModule>,
        dep_ids: &[ObjectID],
        current_package_id: ObjectID,
        upgrade_ticket_policy: u8,
        linkage: ResolvedLinkage,
    ) -> Result<ObjectID, ExecutionError> {
        // Check that this package ID points to a package and get the package we're upgrading.
        let current_package = self.fetch_package(&current_package_id)?;

        let runtime_id = current_package.move_package().original_package_id();
        adapter::substitute_package_id(&mut modules, runtime_id)?;

        // Upgraded packages share their predecessor's runtime ID but get a new storage ID.
        // It should be fine that this does not go through the object runtime since it does not
        // need to know about new packages created, since Move objects and Move packages
        // cannot interact
        let storage_id = self.tx_context.borrow_mut().fresh_id();

        let dependencies = self.fetch_packages(dep_ids)?;
        let current_move_package = current_package.move_package();
        let package = current_move_package.new_upgraded(
            storage_id,
            &modules,
            self.env.protocol_config,
            dependencies.iter().map(|p| p.move_package()),
        )?;

        let linkage = RootedLinkage::new(*storage_id, linkage);
        self.publish_and_verify_modules(runtime_id, &modules, &linkage)?;

        legacy_ptb::execution::check_compatibility(
            self.env.protocol_config,
            current_package.move_package(),
            &modules,
            upgrade_ticket_policy,
        )?;

        self.env
            .linkable_store
            .push_package(storage_id, Rc::new(package))?;
        Ok(storage_id)
    }

    //
    // Commands
    //

    pub fn transfer_object(
        &mut self,
        recipient: Owner,
        ty: Type,
        object: CtxValue,
    ) -> Result<(), ExecutionError> {
        let tag = TypeTag::try_from(ty)
            .map_err(|_| make_invariant_violation!("Unable to convert Type to TypeTag"))?;
        let TypeTag::Struct(tag) = tag else {
            invariant_violation!("Expected struct type tag");
        };
        let ty = MoveObjectType::from(*tag);
        object_runtime_mut!(self)?
            .transfer(recipient, ty, object.0.into())
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
        Ok(())
    }

    //
    // Dev Inspect tracking
    //

    pub fn location_updates(
        &mut self,
        args: Vec<(T::Location, Type)>,
    ) -> Result<Vec<(sui_types::transaction::Argument, Vec<u8>, TypeTag)>, ExecutionError> {
        args.into_iter()
            .filter_map(|(location, ty)| self.location_update(location, ty).transpose())
            .collect()
    }

    fn location_update(
        &mut self,
        location: T::Location,
        ty: Type,
    ) -> Result<Option<(sui_types::transaction::Argument, Vec<u8>, TypeTag)>, ExecutionError> {
        use sui_types::transaction::Argument as TxArgument;
        let ty = match ty {
            Type::Reference(_, inner) => (*inner).clone(),
            ty => ty,
        };
        let Ok(tag): Result<TypeTag, _> = ty.clone().try_into() else {
            invariant_violation!("unable to generate type tag from type")
        };
        let (_, _, lv) = self.location_value(location, ty)?;
        let local = match lv {
            LocationValue::Loaded(v) => {
                if v.is_invalid()? {
                    return Ok(None);
                }
                v
            }
            LocationValue::InputBytes(_, _, _) => return Ok(None),
        };
        let Some(bytes) = local.copy()?.serialize() else {
            invariant_violation!("Failed to serialize Move value");
        };
        let arg = match location {
            T::Location::GasCoin => TxArgument::GasCoin,
            T::Location::Input(i) => TxArgument::Input(i),
            T::Location::Result(i, j) => TxArgument::NestedResult(i, j),
        };
        Ok(Some((arg, bytes, tag)))
    }

    pub fn tracked_results(
        &self,
        results: &[CtxValue],
        result_tys: &T::ResultType,
    ) -> Result<Vec<(Vec<u8>, TypeTag)>, ExecutionError> {
        assert_invariant!(
            results.len() == result_tys.len(),
            "results and result types should match"
        );
        results
            .iter()
            .zip(result_tys)
            .map(|(v, ty)| self.tracked_result(&v.0, ty.clone()))
            .collect()
    }

    fn tracked_result(
        &self,
        result: &Value,
        ty: Type,
    ) -> Result<(Vec<u8>, TypeTag), ExecutionError> {
        let inner_value;
        let (v, ty) = match ty {
            Type::Reference(_, inner) => {
                inner_value = result.copy()?.read_ref()?;
                (&inner_value, (*inner).clone())
            }
            _ => (result, ty),
        };
        let Some(bytes) = v.serialize() else {
            invariant_violation!("Failed to serialize Move value");
        };
        let Ok(tag): Result<TypeTag, _> = ty.try_into() else {
            invariant_violation!("unable to generate type tag from type")
        };
        Ok((bytes, tag))
    }
}

impl VMValueCast<CtxValue> for VMValue {
    fn cast(self) -> Result<CtxValue, PartialVMError> {
        Ok(CtxValue(self.into()))
    }
}

impl CtxValue {
    pub fn vec_pack(ty: Type, values: Vec<CtxValue>) -> Result<CtxValue, ExecutionError> {
        Ok(CtxValue(Value::vec_pack(
            ty,
            values.into_iter().map(|v| v.0).collect(),
        )?))
    }

    pub fn coin_ref_value(self) -> Result<u64, ExecutionError> {
        self.0.coin_ref_value()
    }

    pub fn coin_ref_subtract_balance(self, amount: u64) -> Result<(), ExecutionError> {
        self.0.coin_ref_subtract_balance(amount)
    }

    pub fn coin_ref_add_balance(self, amount: u64) -> Result<(), ExecutionError> {
        self.0.coin_ref_add_balance(amount)
    }

    pub fn into_upgrade_ticket(self) -> Result<UpgradeTicket, ExecutionError> {
        self.0.into_upgrade_ticket()
    }
}

fn load_input_arg(
    meter: &mut GasCharger,
    env: &Env,
    input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
    arg: T::InputArg,
    ty: T::InputType,
) -> Result<InitialInput, ExecutionError> {
    Ok(match arg {
        T::InputArg::Pure(bytes) => InitialInput::Bytes(ByteValue::Pure(bytes)),
        T::InputArg::Receiving((id, version, _)) => {
            InitialInput::Bytes(ByteValue::Receiving { id, version })
        }
        T::InputArg::Object(arg) => {
            let T::InputType::Fixed(ty) = ty else {
                invariant_violation!("Expected fixed type for object arg");
            };
            let (object_metadata, value) = load_object_arg(meter, env, input_object_map, arg, ty)?;
            InitialInput::Object(object_metadata, value)
        }
    })
}

fn load_object_arg(
    meter: &mut GasCharger,
    env: &Env,
    input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
    arg: T::ObjectArg,
    ty: T::Type,
) -> Result<(InputObjectMetadata, Value), ExecutionError> {
    let id = arg.id();
    let is_mutable_input = arg.is_mutable();
    load_object_arg_impl(meter, env, input_object_map, id, is_mutable_input, ty)
}

fn load_object_arg_impl(
    meter: &mut GasCharger,
    env: &Env,
    input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
    id: ObjectID,
    is_mutable_input: bool,
    ty: T::Type,
) -> Result<(InputObjectMetadata, Value), ExecutionError> {
    let obj = env.read_object(&id)?;
    let owner = obj.owner.clone();
    let version = obj.version();
    let object_metadata = InputObjectMetadata {
        id,
        is_mutable_input,
        owner: owner.clone(),
        version,
        type_: ty.clone(),
    };
    let sui_types::object::ObjectInner {
        data: sui_types::object::Data::Move(move_obj),
        ..
    } = obj.as_inner()
    else {
        invariant_violation!("Expected a Move object");
    };
    let contained_uids = {
        let fully_annotated_layout = env.fully_annotated_layout(&ty)?;
        get_all_uids(&fully_annotated_layout, move_obj.contents()).map_err(|e| {
            make_invariant_violation!("Unable to retrieve UIDs for object. Got error: {e}")
        })?
    };
    input_object_map.insert(
        id,
        object_runtime::InputObject {
            contained_uids,
            version,
            owner,
        },
    );

    let v = Value::deserialize(env, move_obj.contents(), ty)?;
    charge_gas_!(meter, env, charge_copy_loc, &v)?;
    Ok((object_metadata, v))
}

fn load_byte_value(
    meter: &mut GasCharger,
    env: &Env,
    value: &ByteValue,
    ty: Type,
) -> Result<Value, ExecutionError> {
    let loaded = match value {
        ByteValue::Pure(bytes) => Value::deserialize(env, bytes, ty)?,
        ByteValue::Receiving { id, version } => Value::receiving(*id, *version),
    };
    charge_gas_!(meter, env, charge_copy_loc, &loaded)?;
    Ok(loaded)
}

fn copy_value(meter: &mut GasCharger, env: &Env, value: &Value) -> Result<Value, ExecutionError> {
    charge_gas_!(meter, env, charge_copy_loc, value)?;
    value.copy()
}

/// The max budget was deducted from the gas coin at the beginning of the transaction,
/// now we return exactly that amount. Gas will be charged by the execution engine
fn refund_max_gas_budget<OType>(
    writes: &mut IndexMap<ObjectID, (Owner, OType, VMValue)>,
    gas_charger: &mut GasCharger,
    gas_id: ObjectID,
) -> Result<(), ExecutionError> {
    let Some((_, _, value_ref)) = writes.get_mut(&gas_id) else {
        invariant_violation!("Gas object cannot be wrapped or destroyed")
    };
    // replace with dummy value
    let value = std::mem::replace(value_ref, VMValue::u8(0));
    let mut locals = Locals::new([value.into()])?;
    let mut local = locals.local(0)?;
    let coin_value = local.borrow()?.coin_ref_value()?;
    let additional = gas_charger.gas_budget();
    if coin_value.checked_add(additional).is_none() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::CoinBalanceOverflow,
            "Gas coin too large after returning the max gas budget",
        ));
    };
    local.borrow()?.coin_ref_add_balance(additional)?;
    // put the value back
    *value_ref = local.move_()?.into();
    Ok(())
}

/// Generate an MoveObject given an updated/written object
/// # Safety
///
/// This function assumes proper generation of has_public_transfer, either from the abilities of
/// the StructTag, or from the runtime correctly propagating from the inputs
unsafe fn create_written_object<Mode: ExecutionMode>(
    env: &Env,
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

    let Ok(type_tag): Result<TypeTag, _> = type_.try_into() else {
        invariant_violation!("unable to generate type tag from type")
    };

    let struct_tag = match type_tag {
        TypeTag::Struct(inner) => *inner,
        _ => invariant_violation!("Non struct type for object"),
    };
    unsafe {
        MoveObject::new_from_execution(
            struct_tag.into(),
            has_public_transfer,
            old_obj_ver.unwrap_or_default(),
            contents,
            env.protocol_config,
            Mode::packages_are_predefined(),
        )
    }
}
