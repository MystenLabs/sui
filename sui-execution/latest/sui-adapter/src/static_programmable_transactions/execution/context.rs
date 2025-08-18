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
        env::Env,
        execution::values::{Local, Locals, Value},
        linkage::resolved_linkage::{ResolvedLinkage, RootedLinkage},
        typing::ast::{self as T, Type},
    },
};
use indexmap::{IndexMap, IndexSet};
use move_binary_format::{
    CompiledModule,
    errors::{Location, PartialVMError, VMResult},
    file_format::FunctionDefinitionIndex,
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
    base_types::{MoveObjectType, ObjectID, SequenceNumber, TxContext},
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
#[derive(Debug)]
pub struct CtxValue(Value);

#[derive(Clone, Debug)]
pub struct InputObjectMetadata {
    pub id: ObjectID,
    pub is_mutable_input: bool,
    pub owner: Owner,
    pub version: SequenceNumber,
    pub type_: Type,
}

#[derive(Copy, Clone)]
enum UsageKind {
    Move,
    Copy,
    Borrow,
}

// Locals and metadata for all `Location`s. Separated from `Context` for lifetime reasons.
struct Locations {
    // A single local for holding the TxContext
    tx_context_value: Locals,
    /// The runtime value for the Gas coin, None if no gas coin is provided
    gas: Option<(InputObjectMetadata, Locals)>,
    /// The runtime value for the input objects args
    input_object_metadata: Vec<(T::InputIndex, InputObjectMetadata)>,
    object_inputs: Locals,
    pure_input_bytes: IndexSet<Vec<u8>>,
    pure_input_metadata: Vec<T::PureInput>,
    pure_inputs: Locals,
    receiving_input_metadata: Vec<T::ReceivingInput>,
    receiving_inputs: Locals,
    /// The results of a given command. For most commands, the inner vector will have length 1.
    /// It will only not be 1 for Move calls with multiple return values.
    /// Inner values are None if taken/moved by-value
    results: Vec<Locals>,
}

enum ResolvedLocation<'a> {
    Local(Local<'a>),
    Pure {
        bytes: &'a [u8],
        metadata: &'a T::PureInput,
        local: Local<'a>,
    },
    Receiving {
        metadata: &'a T::ReceivingInput,
        local: Local<'a>,
    },
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
    locations: Locations,
}

impl Locations {
    /// NOTE! This does not charge gas and should not be used directly. It is exposed for
    /// dev-inspect
    fn resolve(&mut self, location: T::Location) -> Result<ResolvedLocation<'_>, ExecutionError> {
        Ok(match location {
            T::Location::TxContext => ResolvedLocation::Local(self.tx_context_value.local(0)?),
            T::Location::GasCoin => {
                let (_, gas_locals) = unwrap!(self.gas.as_mut(), "Gas coin not provided");
                ResolvedLocation::Local(gas_locals.local(0)?)
            }
            T::Location::ObjectInput(i) => ResolvedLocation::Local(self.object_inputs.local(i)?),
            T::Location::Result(i, j) => {
                let result = unwrap!(self.results.get_mut(i as usize), "bounds already verified");
                ResolvedLocation::Local(result.local(j)?)
            }
            T::Location::PureInput(i) => {
                let local = self.pure_inputs.local(i)?;
                let metadata = &self.pure_input_metadata[i as usize];
                let bytes = self
                    .pure_input_bytes
                    .get_index(metadata.byte_index)
                    .ok_or_else(|| {
                        make_invariant_violation!(
                            "Pure input {} bytes out of bounds at index {}",
                            metadata.original_input_index.0,
                            metadata.byte_index,
                        )
                    })?;
                ResolvedLocation::Pure {
                    bytes,
                    metadata,
                    local,
                }
            }
            T::Location::ReceivingInput(i) => ResolvedLocation::Receiving {
                metadata: &self.receiving_input_metadata[i as usize],
                local: self.receiving_inputs.local(i)?,
            },
        })
    }
}

impl<'env, 'pc, 'vm, 'state, 'linkage, 'gas> Context<'env, 'pc, 'vm, 'state, 'linkage, 'gas> {
    #[instrument(name = "Context::new", level = "trace", skip_all)]
    pub fn new(
        env: &'env Env<'pc, 'vm, 'state, 'linkage>,
        metrics: Arc<LimitsMetrics>,
        tx_context: Rc<RefCell<TxContext>>,
        gas_charger: &'gas mut GasCharger,
        pure_input_bytes: IndexSet<Vec<u8>>,
        object_inputs: Vec<T::ObjectInput>,
        pure_input_metadata: Vec<T::PureInput>,
        receiving_input_metadata: Vec<T::ReceivingInput>,
    ) -> Result<Self, ExecutionError>
    where
        'pc: 'state,
    {
        let mut input_object_map = BTreeMap::new();
        let mut input_object_metadata = Vec::with_capacity(object_inputs.len());
        let mut object_values = Vec::with_capacity(object_inputs.len());
        for object_input in object_inputs {
            let (i, m, v) = load_object_arg(gas_charger, env, &mut input_object_map, object_input)?;
            input_object_metadata.push((i, m));
            object_values.push(Some(v));
        }
        let object_inputs = Locals::new(object_values)?;
        let pure_inputs = Locals::new_invalid(pure_input_metadata.len())?;
        let receiving_inputs = Locals::new_invalid(receiving_input_metadata.len())?;
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
                let mut gas_locals = Locals::new([Some(gas_value)])?;
                let gas_local = gas_locals.local(0)?;
                let gas_ref = gas_local.borrow()?;
                // We have already checked that the gas balance is enough to cover the gas budget
                let max_gas_in_balance = gas_charger.gas_budget();
                gas_ref.coin_ref_subtract_balance(max_gas_in_balance)?;
                Some((gas_metadata, gas_locals))
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

        let tx_context_value = Locals::new(vec![Some(Value::new_tx_context(
            tx_context.borrow().digest(),
        )?)])?;
        Ok(Self {
            env,
            metrics,
            native_extensions,
            tx_context,
            gas_charger,
            user_events: vec![],
            locations: Locations {
                tx_context_value,
                gas,
                input_object_metadata,
                object_inputs,
                pure_input_bytes,
                pure_input_metadata,
                pure_inputs,
                receiving_input_metadata,
                receiving_inputs,
                results: vec![],
            },
        })
    }

    pub fn finish<Mode: ExecutionMode>(mut self) -> Result<ExecutionResults, ExecutionError> {
        assert_invariant!(
            !self.locations.tx_context_value.local(0)?.is_invalid()?,
            "tx context value should be present"
        );
        let gas = std::mem::take(&mut self.locations.gas);
        let object_input_metadata = std::mem::take(&mut self.locations.input_object_metadata);
        let mut object_inputs =
            std::mem::replace(&mut self.locations.object_inputs, Locals::new_invalid(0)?);
        let mut loaded_runtime_objects = BTreeMap::new();
        let mut by_value_shared_objects = BTreeSet::new();
        let mut consensus_owner_objects = BTreeMap::new();
        let gas = gas
            .map(|(m, mut g)| Result::<_, ExecutionError>::Ok((m, g.local(0)?.move_if_valid()?)))
            .transpose()?;
        let gas_id_opt = gas.as_ref().map(|(m, _)| m.id);
        let object_inputs = object_input_metadata
            .into_iter()
            .enumerate()
            .map(|(i, (_, m))| {
                let v_opt = object_inputs.local(i as u16)?.move_if_valid()?;
                Ok((m, v_opt))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        for (metadata, value_opt) in object_inputs.into_iter().chain(gas) {
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
            if let Some(object) = value_opt {
                self.transfer_object_(
                    owner,
                    type_,
                    CtxValue(object),
                    /* end of transaction */ true,
                )?;
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

        for (id, (recipient, ty, value)) in writes {
            let ty: Type = env.load_type_from_struct(&ty.clone().into())?;
            let abilities = ty.abilities();
            let has_public_transfer = abilities.has_store();
            let Some(bytes) = value.serialize() else {
                invariant_violation!("Failed to serialize already deserialized Move value");
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

        for package in self.env.linkable_store.to_new_packages().into_iter() {
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

    pub fn object_runtime(&self) -> Result<&ObjectRuntime<'_>, ExecutionError> {
        self.native_extensions
            .get::<ObjectRuntime>()
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))
    }

    pub fn take_user_events(
        &mut self,
        storage_id: ModuleId,
        function_def_idx: FunctionDefinitionIndex,
        instr_length: u16,
        linkage: &RootedLinkage,
    ) -> Result<(), ExecutionError> {
        let events = object_runtime_mut!(self)?.take_user_events();
        let num_events = self.user_events.len() + events.len();
        let max_events = self.env.protocol_config.max_num_event_emit();
        if num_events as u64 > max_events {
            let err = max_event_error(max_events)
                .at_code_offset(function_def_idx, instr_length)
                .finish(Location::Module(storage_id.clone()));
            return Err(self.env.convert_linked_vm_error(err, linkage));
        }
        let new_events = events
            .into_iter()
            .map(|(tag, value)| {
                let Some(bytes) = value.serialize() else {
                    invariant_violation!("Failed to serialize Move event");
                };
                Ok((storage_id.clone(), tag, bytes))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        self.user_events.extend(new_events);
        Ok(())
    }

    //
    // Arguments and Values
    //

    fn location(
        &mut self,
        usage: UsageKind,
        location: T::Location,
    ) -> Result<Value, ExecutionError> {
        let resolved = self.locations.resolve(location)?;
        let mut local = match resolved {
            ResolvedLocation::Local(l) => l,
            ResolvedLocation::Pure {
                bytes,
                metadata,
                mut local,
            } => {
                if local.is_invalid()? {
                    let v = load_pure_value(self.gas_charger, self.env, bytes, metadata)?;
                    local.store(v)?;
                }
                local
            }
            ResolvedLocation::Receiving {
                metadata,
                mut local,
            } => {
                if local.is_invalid()? {
                    let v = load_receiving_value(self.gas_charger, self.env, metadata)?;
                    local.store(v)?;
                }
                local
            }
        };
        Ok(match usage {
            UsageKind::Move => local.move_()?,
            UsageKind::Copy => {
                let value = local.copy()?;
                charge_gas_!(self.gas_charger, self.env, charge_copy_loc, &value)?;
                value
            }
            UsageKind::Borrow => local.borrow()?,
        })
    }

    fn location_usage(&mut self, usage: T::Usage) -> Result<Value, ExecutionError> {
        match usage {
            T::Usage::Move(location) => self.location(UsageKind::Move, location),
            T::Usage::Copy { location, .. } => self.location(UsageKind::Copy, location),
        }
    }

    fn argument_value(&mut self, sp!(_, (arg_, _)): T::Argument) -> Result<Value, ExecutionError> {
        match arg_ {
            T::Argument__::Use(usage) => self.location_usage(usage),
            // freeze is a no-op for references since the value does not track mutability
            T::Argument__::Freeze(usage) => self.location_usage(usage),
            T::Argument__::Borrow(_, location) => self.location(UsageKind::Borrow, location),
            T::Argument__::Read(usage) => {
                let reference = self.location_usage(usage)?;
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

    pub fn result(&mut self, result: Vec<Option<CtxValue>>) -> Result<(), ExecutionError> {
        self.locations
            .results
            .push(Locals::new(result.into_iter().map(|v| v.map(|v| v.0)))?);
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
        args: Vec<CtxValue>,
        trace_builder_opt: Option<&mut MoveTraceBuilder>,
    ) -> Result<Vec<CtxValue>, ExecutionError> {
        let result = self.execute_function_bypass_visibility(
            &function.runtime_id,
            &function.name,
            &function.type_arguments,
            args,
            &function.linkage,
            trace_builder_opt,
        )?;
        self.take_user_events(
            function.storage_id,
            function.definition_index,
            function.instruction_length,
            &function.linkage,
        )?;
        Ok(result)
    }

    pub fn execute_function_bypass_visibility(
        &mut self,
        runtime_id: &ModuleId,
        function_name: &IdentStr,
        ty_args: &[Type],
        args: Vec<CtxValue>,
        linkage: &RootedLinkage,
        tracer: Option<&mut MoveTraceBuilder>,
    ) -> Result<Vec<CtxValue>, ExecutionError> {
        let ty_args = ty_args
            .iter()
            .enumerate()
            .map(|(idx, ty)| self.env.load_vm_type_argument_from_adapter_type(idx, ty))
            .collect::<Result<_, _>>()?;
        let gas_status = self.gas_charger.move_gas_status_mut();
        let mut data_store = LinkedDataStore::new(linkage, self.env.linkable_store);
        let values = self
            .env
            .vm
            .get_runtime()
            .execute_function_with_values_bypass_visibility(
                runtime_id,
                function_name,
                ty_args,
                args.into_iter().map(|v| v.0.into()).collect(),
                &mut data_store,
                &mut SuiGasMeter(gas_status),
                &mut self.native_extensions,
                tracer,
            )
            .map_err(|e| self.env.convert_linked_vm_error(e, linkage))?;
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
            .map_err(|e| self.env.convert_linked_vm_error(e, linkage))?;

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
        package_id: ObjectID,
        modules: &[CompiledModule],
        linkage: &RootedLinkage,
        mut trace_builder_opt: Option<&mut MoveTraceBuilder>,
    ) -> Result<(), ExecutionError> {
        for module in modules {
            let Some((fdef_idx, fdef)) = module.find_function_def_by_name(INIT_FN_NAME.as_str())
            else {
                continue;
            };
            let fhandle = module.function_handle_at(fdef.function);
            let fparameters = module.signature_at(fhandle.parameters);
            assert_invariant!(
                fparameters.0.len() <= 2,
                "init function should have at most 2 parameters"
            );
            let has_otw = fparameters.0.len() == 2;
            let tx_context = self
                .location(UsageKind::Borrow, T::Location::TxContext)
                .map_err(|e| {
                    make_invariant_violation!("Failed to get tx context for init function: {}", e)
                })?;
            let args = if has_otw {
                vec![CtxValue(Value::one_time_witness()?), CtxValue(tx_context)]
            } else {
                vec![CtxValue(tx_context)]
            };
            let return_values = self.execute_function_bypass_visibility(
                &module.self_id(),
                INIT_FN_NAME,
                &[],
                args,
                linkage,
                trace_builder_opt.as_deref_mut(),
            )?;

            let storage_id = ModuleId::new(package_id.into(), module.self_id().name().to_owned());
            self.take_user_events(
                storage_id,
                fdef_idx,
                fdef.code.as_ref().map(|c| c.code.len() as u16).unwrap_or(0),
                linkage,
            )?;
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
            self.env.protocol_config,
            dependencies.iter().map(|p| p.move_package()),
        )?);
        let package_id = package.id();

        // Here we optimistically push the package that is being published/upgraded
        // and if there is an error of any kind (verification or module init) we
        // remove it.
        // The call to `pop_last_package` later is fine because we cannot re-enter and
        // the last package we pushed is the one we are verifying and running the init from
        let linkage = RootedLinkage::new_for_publication(package_id, runtime_id, linkage);

        self.env.linkable_store.push_package(package_id, package)?;
        let res = self
            .publish_and_verify_modules(runtime_id, &modules, &linkage)
            .and_then(|_| self.init_modules(package_id, &modules, &linkage, trace_builder_opt));
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

        let linkage = RootedLinkage::new_for_publication(storage_id, runtime_id, linkage);
        self.publish_and_verify_modules(runtime_id, &modules, &linkage)?;

        legacy_ptb::execution::check_compatibility(
            self.env.protocol_config,
            current_package.move_package(),
            &modules,
            upgrade_ticket_policy,
        )?;

        if self.env.protocol_config.check_for_init_during_upgrade() {
            // find newly added modules to the package,
            // and error if they have init functions
            let current_module_names: BTreeSet<&str> = current_package
                .move_package()
                .serialized_module_map()
                .keys()
                .map(|s| s.as_str())
                .collect();
            let upgrade_module_names: BTreeSet<&str> = package
                .serialized_module_map()
                .keys()
                .map(|s| s.as_str())
                .collect();
            let new_module_names = upgrade_module_names
                .difference(&current_module_names)
                .copied()
                .collect::<BTreeSet<&str>>();
            let new_modules = modules
                .iter()
                .filter(|m| {
                    let name = m.identifier_at(m.self_handle().name).as_str();
                    new_module_names.contains(name)
                })
                .collect::<Vec<&CompiledModule>>();
            let new_module_has_init = new_modules.iter().any(|module| {
                module.function_defs.iter().any(|fdef| {
                    let fhandle = module.function_handle_at(fdef.function);
                    let fname = module.identifier_at(fhandle.name);
                    fname == INIT_FN_NAME
                })
            });
            if new_module_has_init {
                // TODO we cannot run 'init' on upgrade yet due to global type cache limitations
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::FeatureNotYetSupported,
                    "`init` in new modules on upgrade is not yet supported",
                ));
            }
        }

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
        self.transfer_object_(recipient, ty, object, /* end of transaction */ false)
    }

    fn transfer_object_(
        &mut self,
        recipient: Owner,
        ty: Type,
        object: CtxValue,
        end_of_transaction: bool,
    ) -> Result<(), ExecutionError> {
        let tag = TypeTag::try_from(ty)
            .map_err(|_| make_invariant_violation!("Unable to convert Type to TypeTag"))?;
        let TypeTag::Struct(tag) = tag else {
            invariant_violation!("Expected struct type tag");
        };
        let ty = MoveObjectType::from(*tag);
        object_runtime_mut!(self)?
            .transfer(recipient, ty, object.0.into(), end_of_transaction)
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
        Ok(())
    }

    /// Check for valid shared object usage, either deleted or re-shared, at the end of a command
    pub fn check_shared_object_usage(
        &mut self,
        consumed_shared_objects: Vec<ObjectID>,
    ) -> Result<(), ExecutionError> {
        let object_runtime = self.object_runtime()?;
        legacy_ptb::context::check_shared_object_usage(object_runtime, &consumed_shared_objects)
    }

    //
    // Dev Inspect tracking
    //

    pub fn argument_updates(
        &mut self,
        args: Vec<T::Argument>,
    ) -> Result<Vec<(sui_types::transaction::Argument, Vec<u8>, TypeTag)>, ExecutionError> {
        args.into_iter()
            .filter_map(|arg| self.argument_update(arg).transpose())
            .collect()
    }

    fn argument_update(
        &mut self,
        sp!(_, (arg, ty)): T::Argument,
    ) -> Result<Option<(sui_types::transaction::Argument, Vec<u8>, TypeTag)>, ExecutionError> {
        use sui_types::transaction::Argument as TxArgument;
        let ty = match ty {
            Type::Reference(true, inner) => (*inner).clone(),
            ty => {
                debug_assert!(
                    false,
                    "Unexpected non reference type in location update: {ty:?}"
                );
                return Ok(None);
            }
        };
        let Ok(tag): Result<TypeTag, _> = ty.clone().try_into() else {
            invariant_violation!("unable to generate type tag from type")
        };
        let location = arg.location();
        let resolved = self.locations.resolve(location)?;
        let local = match resolved {
            ResolvedLocation::Local(local)
            | ResolvedLocation::Pure { local, .. }
            | ResolvedLocation::Receiving { local, .. } => local,
        };
        if local.is_invalid()? {
            return Ok(None);
        }
        // copy the value from the local
        let value = local.copy()?;
        let value = match arg {
            T::Argument__::Use(_) => {
                // dereference the reference
                value.read_ref()?
            }
            T::Argument__::Borrow(_, _) => {
                // value is not a reference, nothing to do
                value
            }
            T::Argument__::Freeze(_) => {
                invariant_violation!("freeze should not be used for a mutable reference")
            }
            T::Argument__::Read(_) => {
                invariant_violation!("read should not return a reference")
            }
        };
        let Some(bytes) = value.serialize() else {
            invariant_violation!("Failed to serialize Move value");
        };
        let arg = match location {
            T::Location::TxContext => return Ok(None),
            T::Location::GasCoin => TxArgument::GasCoin,
            T::Location::Result(i, j) => TxArgument::NestedResult(i, j),
            T::Location::ObjectInput(i) => {
                TxArgument::Input(self.locations.input_object_metadata[i as usize].0.0)
            }
            T::Location::PureInput(i) => TxArgument::Input(
                self.locations.pure_input_metadata[i as usize]
                    .original_input_index
                    .0,
            ),
            T::Location::ReceivingInput(i) => TxArgument::Input(
                self.locations.receiving_input_metadata[i as usize]
                    .original_input_index
                    .0,
            ),
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

fn load_object_arg(
    meter: &mut GasCharger,
    env: &Env,
    input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
    input: T::ObjectInput,
) -> Result<(T::InputIndex, InputObjectMetadata, Value), ExecutionError> {
    let id = input.arg.id();
    let is_mutable_input = input.arg.is_mutable();
    let (metadata, value) =
        load_object_arg_impl(meter, env, input_object_map, id, is_mutable_input, input.ty)?;
    Ok((input.original_input_index, metadata, value))
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

fn load_pure_value(
    meter: &mut GasCharger,
    env: &Env,
    bytes: &[u8],
    metadata: &T::PureInput,
) -> Result<Value, ExecutionError> {
    let loaded = Value::deserialize(env, bytes, metadata.ty.clone())?;
    // ByteValue::Receiving { id, version } => Value::receiving(*id, *version),
    charge_gas_!(meter, env, charge_copy_loc, &loaded)?;
    Ok(loaded)
}

fn load_receiving_value(
    meter: &mut GasCharger,
    env: &Env,
    metadata: &T::ReceivingInput,
) -> Result<Value, ExecutionError> {
    let (id, version, _) = metadata.object_ref;
    let loaded = Value::receiving(id, version);
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
    let mut locals = Locals::new([Some(value.into())])?;
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
