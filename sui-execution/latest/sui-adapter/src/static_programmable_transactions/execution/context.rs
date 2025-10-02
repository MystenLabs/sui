// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    adapter,
    execution_mode::ExecutionMode,
    execution_value::ExecutionState,
    gas_charger::GasCharger,
    gas_meter::SuiGasMeter,
    sp,
    static_programmable_transactions::{
        env::Env,
        execution::{
            self,
            values::{Local, Locals, Value},
        },
        linkage::resolved_linkage::{ExecutableLinkage, ResolvedLinkage},
        typing::ast::{self as T, Type},
    },
};
use indexmap::{IndexMap, IndexSet};
use move_binary_format::{
    CompiledModule,
    compatibility::{Compatibility, InclusionCheck},
    errors::{Location, PartialVMError, PartialVMResult, VMResult},
    file_format::FunctionDefinitionIndex,
    normalized,
};
use move_core_types::{
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag},
    u256::U256,
};
use move_trace_format::format::MoveTraceBuilder;
use move_vm_runtime::{
    execution::{
        Type as VMType, TypeSubst as _,
        values::{VMValueCast, Value as VMValue},
        vm::{LoadedFunctionInformation, MoveVM},
    },
    natives::extensions::NativeExtensions,
    shared::gas::GasMeter as _,
    validation::verification::ast::Package as VerifiedPackage,
};
use mysten_common::debug_fatal;
use serde::{Deserialize, de::DeserializeSeed};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
    rc::Rc,
    sync::Arc,
};
use sui_move_natives::object_runtime::{
    self, LoadedRuntimeObject, MoveAccumulatorEvent, MoveAccumulatorValue, ObjectRuntime,
    RuntimeResults, get_all_uids, max_event_error,
};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    SUI_FRAMEWORK_ADDRESS, TypeTag,
    accumulator_event::AccumulatorEvent,
    accumulator_root::AccumulatorObjId,
    balance::Balance,
    base_types::{
        MoveObjectType, ObjectID, RESOLVED_ASCII_STR, RESOLVED_UTF8_STR, SequenceNumber,
        SuiAddress, TxContext,
    },
    effects::{AccumulatorAddress, AccumulatorValue, AccumulatorWriteV1},
    error::{ExecutionError, ExecutionErrorKind, command_argument_error},
    event::Event,
    execution::{ExecutionResults, ExecutionResultsV2},
    execution_config_utils::to_binary_config,
    execution_status::{CommandArgumentError, PackageUpgradeError},
    metrics::LimitsMetrics,
    move_package::{
        MovePackage, UpgradeCap, UpgradePolicy, UpgradeReceipt, UpgradeTicket,
        normalize_deserialized_modules,
    },
    object::{MoveObject, Object, Owner},
    storage::{BackingPackageStore, DenyListResult, PackageObject, get_package_objects},
};
use sui_verifier::{
    INIT_FN_NAME,
    private_generics::{EVENT_MODULE, PRIVATE_TRANSFER_FUNCTIONS, TRANSFER_MODULE},
};
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

#[macro_export]
macro_rules! object_runtime {
    ($context:ident) => {
        $context
            .native_extensions
            .read()
            .get::<sui_move_natives::object_runtime::ObjectRuntime>()
            .map_err(|e| {
                $context
                    .env
                    .convert_vm_error(e.finish(move_binary_format::errors::Location::Undefined))
            })
    };
}

macro_rules! object_runtime_mut {
    ($context:ident) => {
        $context
            .native_extensions
            .write()
            .get_mut::<ObjectRuntime>()
            .map_err(|e| $context.env.convert_vm_error(e.finish(Location::Undefined)))
    };
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
    pub native_extensions: NativeExtensions<'env>,
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
    fn resolve(&mut self, location: T::Location) -> Result<ResolvedLocation, ExecutionError> {
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
                let mut gas_local = gas_locals.local(0)?;
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
            native_extensions,
            tx_context,
            gas_charger,
            user_events,
            ..
        } = self;
        let ref_context: &RefCell<TxContext> = &tx_context;
        let tx_context: &TxContext = &ref_context.borrow();
        let tx_digest = ref_context.borrow().digest();

        let object_runtime: ObjectRuntime = native_extensions
            .write()
            .remove()
            .map_err(|e| env.convert_vm_error(e.finish(Location::Undefined)))?;

        let RuntimeResults {
            mut writes,
            user_events: remaining_events,
            loaded_child_objects,
            mut created_object_ids,
            deleted_object_ids,
            accumulator_events,
            settlement_input_sui,
            settlement_output_sui,
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

        for package in self
            .env
            .linkable_store
            .package_store
            .to_new_packages()
            .into_iter()
        {
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

        execution::context::finish(
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
            settlement_input_sui,
            settlement_output_sui,
        )
    }

    pub fn take_user_events(
        &mut self,
        version_mid: ModuleId,
        function_def_idx: FunctionDefinitionIndex,
        instr_length: u16,
        linkage: &ExecutableLinkage,
    ) -> Result<(), ExecutionError> {
        let events = object_runtime_mut!(self)?.take_user_events();
        let num_events = self.user_events.len() + events.len();
        let max_events = self.env.protocol_config.max_num_event_emit();
        if num_events as u64 > max_events {
            let err = max_event_error(max_events)
                .at_code_offset(function_def_idx, instr_length)
                .finish(Location::Module(version_mid.clone()));
            return Err(self.env.convert_linked_vm_error(err, linkage));
        }
        let new_events = events
            .into_iter()
            .map(|(tag, value)| {
                let Some(bytes) = value.serialize() else {
                    invariant_violation!("Failed to serialize Move event");
                };
                Ok((version_mid.clone(), tag, bytes))
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

    pub fn new_upgrade_cap(&mut self, version_id: ObjectID) -> Result<CtxValue, ExecutionError> {
        let id = self.tx_context.borrow_mut().fresh_id();
        object_runtime_mut!(self)?
            .new_id(id)
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
        let cap = UpgradeCap::new(id, version_id);
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
            &function.original_mid,
            &function.name,
            &function.type_arguments,
            args,
            &function.linkage,
            trace_builder_opt,
        )?;
        self.take_user_events(
            function.version_mid,
            function.definition_index,
            function.instruction_length,
            &function.linkage,
        )?;
        Ok(result)
    }

    pub fn execute_function_bypass_visibility(
        &mut self,
        original_mid: &ModuleId,
        function_name: &IdentStr,
        ty_args: &[Type],
        args: Vec<CtxValue>,
        linkage: &ExecutableLinkage,
        tracer: Option<&mut MoveTraceBuilder>,
    ) -> Result<Vec<CtxValue>, ExecutionError> {
        let ty_args = ty_args
            .iter()
            .enumerate()
            .map(|(idx, ty)| self.env.load_vm_type_argument_from_adapter_type(idx, ty))
            .collect::<Result<Vec<_>, _>>()?;
        let data_store = &self.env.linkable_store.package_store;
        let link_context = linkage.linkage_context();
        let mut vm = self
            .env
            .vm
            .make_vm_with_native_extensions(
                data_store,
                link_context,
                self.native_extensions.clone(),
            )
            .map_err(|e| self.env.convert_linked_vm_error(e, linkage))?;
        self.execute_function_bypass_visibility_with_vm(
            &mut vm,
            original_mid,
            function_name,
            ty_args,
            args,
            linkage,
            tracer,
        )
    }

    // TODO(vm-rewrite): Need to update the function call to pass deserialized args to the VM.
    fn execute_function_bypass_visibility_with_vm(
        &mut self,
        vm: &mut MoveVM<'env>,
        original_mid: &ModuleId,
        function_name: &IdentStr,
        ty_args: Vec<VMType>,
        args: Vec<CtxValue>,
        linkage: &ExecutableLinkage,
        tracer: Option<&mut MoveTraceBuilder>,
    ) -> Result<Vec<CtxValue>, ExecutionError> {
        let gas_status = self.gas_charger.move_gas_status_mut();
        let values = vm
            .execute_function_bypass_visibility(
                original_mid,
                function_name,
                ty_args,
                args.into_iter().map(|v| v.0.into()).collect(),
                &mut SuiGasMeter(gas_status),
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
        fetch_package(&self.env.state_view, dependency_id)
    }

    fn fetch_packages(
        &mut self,
        dependency_ids: &[ObjectID],
    ) -> Result<Vec<PackageObject>, ExecutionError> {
        fetch_packages(&self.env.state_view, dependency_ids)
    }

    fn publish_and_verify_modules(
        &mut self,
        package_id: ObjectID,
        pkg: &MovePackage,
        modules: &[CompiledModule],
        linkage: &ExecutableLinkage,
    ) -> Result<(VerifiedPackage, MoveVM<'env>), ExecutionError> {
        let serialized_pkg = pkg.into_serialized_move_package();
        let data_store = &self.env.linkable_store.package_store;
        let vm = self
            .env
            .vm
            .validate_package(
                data_store,
                *package_id,
                serialized_pkg,
                &mut SuiGasMeter(self.gas_charger.move_gas_status_mut()),
                self.native_extensions.clone(),
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

        Ok(vm)
    }

    fn init_modules(
        &mut self,
        mut vm: MoveVM<'env>,
        package_id: ObjectID,
        modules: &[CompiledModule],
        linkage: &ExecutableLinkage,
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
            let return_values = self.execute_function_bypass_visibility_with_vm(
                &mut vm,
                &module.self_id(),
                INIT_FN_NAME,
                vec![],
                args,
                linkage,
                trace_builder_opt.as_deref_mut(),
            )?;

            let version_mid = ModuleId::new(package_id.into(), module.self_id().name().to_owned());
            self.take_user_events(
                version_mid,
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
        let original_id = if <Mode>::packages_are_predefined() {
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

        let linkage = ResolvedLinkage::update_for_publication(package_id, original_id, linkage);

        let (pkg, vm) =
            self.publish_and_verify_modules(original_id, &package, &modules, &linkage)?;
        // Here we optimistically push the package that is being published/upgraded
        // and if there is an error of any kind (verification or module init) we
        // remove it.
        // The call to `pop_last_package` later is fine because we cannot re-enter and
        // the last package we pushed is the one we are verifying and running the init from
        self.env
            .linkable_store
            .package_store
            .push_package(package_id, package.clone(), pkg)?;

        match self.init_modules(vm, package_id, &modules, &linkage, trace_builder_opt) {
            Ok(()) => Ok(original_id),
            Err(e) => {
                self.env
                    .linkable_store
                    .package_store
                    .pop_package(package_id)?;
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

        let original_id = current_package.move_package().original_package_id();
        adapter::substitute_package_id(&mut modules, original_id)?;

        // Upgraded packages share their predecessor's runtime ID but get a new storage ID.
        // It should be fine that this does not go through the object runtime since it does not
        // need to know about new packages created, since Move objects and Move packages
        // cannot interact
        let version_id = self.tx_context.borrow_mut().fresh_id();

        let dependencies = self.fetch_packages(dep_ids)?;
        let current_move_package = current_package.move_package();
        let package = current_move_package.new_upgraded(
            version_id,
            &modules,
            self.env.protocol_config,
            dependencies.iter().map(|p| p.move_package()),
        )?;

        let linkage = ResolvedLinkage::update_for_publication(version_id, original_id, linkage);
        let (verified_pkg, _) =
            self.publish_and_verify_modules(original_id, &package, &modules, &linkage)?;

        check_compatibility(
            self.env.protocol_config,
            current_package.move_package(),
            &modules,
            upgrade_ticket_policy,
        )?;

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

        self.env.linkable_store.package_store.push_package(
            version_id,
            Rc::new(package),
            verified_pkg,
        )?;
        Ok(version_id)
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
        check_shared_object_usage(object_runtime!(self)?, &consumed_shared_objects)
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

/// substitutes the type arguments into the parameter and return types
pub fn subst_signature(
    signature: LoadedFunctionInformation,
    type_arguments: &[VMType],
) -> VMResult<LoadedFunctionInformation> {
    let LoadedFunctionInformation {
        parameters,
        return_,
        is_entry,
        is_native,
        visibility,
        index,
        instruction_count,
    } = signature;
    let parameters = parameters
        .into_iter()
        .map(|ty| ty.subst(type_arguments))
        .collect::<PartialVMResult<Vec<_>>>()
        .map_err(|err| err.finish(Location::Undefined))?;
    let return_ = return_
        .into_iter()
        .map(|ty| ty.subst(type_arguments))
        .collect::<PartialVMResult<Vec<_>>>()
        .map_err(|err| err.finish(Location::Undefined))?;
    Ok(LoadedFunctionInformation {
        parameters,
        return_,
        is_entry,
        is_native,
        visibility,
        index,
        instruction_count,
    })
}

pub enum EitherError {
    CommandArgument(CommandArgumentError),
    Execution(ExecutionError),
}

impl From<ExecutionError> for EitherError {
    fn from(e: ExecutionError) -> Self {
        EitherError::Execution(e)
    }
}

impl From<CommandArgumentError> for EitherError {
    fn from(e: CommandArgumentError) -> Self {
        EitherError::CommandArgument(e)
    }
}

impl EitherError {
    pub fn into_execution_error(self, command_index: usize) -> ExecutionError {
        match self {
            EitherError::CommandArgument(e) => command_argument_error(e, command_index),
            EitherError::Execution(e) => e,
        }
    }
}

/***************************************************************************************************
 * Special serialization formats
 **************************************************************************************************/

/// Special enum for values that need additional validation, in other words
/// There is validation to do on top of the BCS layout. Currently only needed for
/// strings
#[derive(Debug)]
pub enum PrimitiveArgumentLayout {
    /// An option
    Option(Box<PrimitiveArgumentLayout>),
    /// A vector
    Vector(Box<PrimitiveArgumentLayout>),
    /// An ASCII encoded string
    Ascii,
    /// A UTF8 encoded string
    UTF8,
    // needed for Option validation
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
}

impl PrimitiveArgumentLayout {
    /// returns true iff all BCS compatible bytes are actually values for this type.
    /// For example, this function returns false for Option and Strings since they need additional
    /// validation.
    pub fn bcs_only(&self) -> bool {
        match self {
            // have additional restrictions past BCS
            PrimitiveArgumentLayout::Option(_)
            | PrimitiveArgumentLayout::Ascii
            | PrimitiveArgumentLayout::UTF8 => false,
            // Move primitives are BCS compatible and do not need additional validation
            PrimitiveArgumentLayout::Bool
            | PrimitiveArgumentLayout::U8
            | PrimitiveArgumentLayout::U16
            | PrimitiveArgumentLayout::U32
            | PrimitiveArgumentLayout::U64
            | PrimitiveArgumentLayout::U128
            | PrimitiveArgumentLayout::U256
            | PrimitiveArgumentLayout::Address => true,
            // vector only needs validation if it's inner type does
            PrimitiveArgumentLayout::Vector(inner) => inner.bcs_only(),
        }
    }
}

/// Checks the bytes against the `SpecialArgumentLayout` using `bcs`. It does not actually generate
/// the deserialized value, only walks the bytes. While not necessary if the layout does not contain
/// special arguments (e.g. Option or String) we check the BCS bytes for predictability
pub fn bcs_argument_validate(
    bytes: &[u8],
    idx: u16,
    layout: PrimitiveArgumentLayout,
) -> Result<(), ExecutionError> {
    bcs::from_bytes_seed(&layout, bytes).map_err(|_| {
        ExecutionError::new_with_source(
            ExecutionErrorKind::command_argument_error(CommandArgumentError::InvalidBCSBytes, idx),
            format!("Function expects {layout} but provided argument's value does not match",),
        )
    })
}

impl<'d> serde::de::DeserializeSeed<'d> for &PrimitiveArgumentLayout {
    type Value = ();
    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        use serde::de::Error;
        match self {
            PrimitiveArgumentLayout::Ascii => {
                let s: &str = serde::Deserialize::deserialize(deserializer)?;
                if !s.is_ascii() {
                    Err(D::Error::custom("not an ascii string"))
                } else {
                    Ok(())
                }
            }
            PrimitiveArgumentLayout::UTF8 => {
                deserializer.deserialize_string(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::Option(layout) => {
                deserializer.deserialize_option(OptionElementVisitor(layout))
            }
            PrimitiveArgumentLayout::Vector(layout) => {
                deserializer.deserialize_seq(VectorElementVisitor(layout))
            }
            // primitive move value cases, which are hit to make sure the correct number of bytes
            // are removed for elements of an option/vector
            PrimitiveArgumentLayout::Bool => {
                deserializer.deserialize_bool(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U8 => {
                deserializer.deserialize_u8(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U16 => {
                deserializer.deserialize_u16(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U32 => {
                deserializer.deserialize_u32(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U64 => {
                deserializer.deserialize_u64(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U128 => {
                deserializer.deserialize_u128(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U256 => {
                U256::deserialize(deserializer)?;
                Ok(())
            }
            PrimitiveArgumentLayout::Address => {
                SuiAddress::deserialize(deserializer)?;
                Ok(())
            }
        }
    }
}

struct VectorElementVisitor<'a>(&'a PrimitiveArgumentLayout);

impl<'d> serde::de::Visitor<'d> for VectorElementVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Vector")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        while seq.next_element_seed(self.0)?.is_some() {}
        Ok(())
    }
}

struct OptionElementVisitor<'a>(&'a PrimitiveArgumentLayout);

impl<'d> serde::de::Visitor<'d> for OptionElementVisitor<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Option")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'d>,
    {
        self.0.deserialize(deserializer)
    }
}

impl fmt::Display for PrimitiveArgumentLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveArgumentLayout::Vector(inner) => {
                write!(f, "vector<{inner}>")
            }
            PrimitiveArgumentLayout::Option(inner) => {
                write!(f, "std::option::Option<{inner}>")
            }
            PrimitiveArgumentLayout::Ascii => {
                write!(f, "std::{}::{}", RESOLVED_ASCII_STR.1, RESOLVED_ASCII_STR.2)
            }
            PrimitiveArgumentLayout::UTF8 => {
                write!(f, "std::{}::{}", RESOLVED_UTF8_STR.1, RESOLVED_UTF8_STR.2)
            }
            PrimitiveArgumentLayout::Bool => write!(f, "bool"),
            PrimitiveArgumentLayout::U8 => write!(f, "u8"),
            PrimitiveArgumentLayout::U16 => write!(f, "u16"),
            PrimitiveArgumentLayout::U32 => write!(f, "u32"),
            PrimitiveArgumentLayout::U64 => write!(f, "u64"),
            PrimitiveArgumentLayout::U128 => write!(f, "u128"),
            PrimitiveArgumentLayout::U256 => write!(f, "u256"),
            PrimitiveArgumentLayout::Address => write!(f, "address"),
        }
    }
}

pub fn check_private_generics(
    module_id: &ModuleId,
    function: &IdentStr,
) -> Result<(), ExecutionError> {
    let module_ident = (module_id.address(), module_id.name());
    if module_ident == (&SUI_FRAMEWORK_ADDRESS, EVENT_MODULE) {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::NonEntryFunctionInvoked,
            format!("Cannot directly call functions in sui::{}", EVENT_MODULE),
        ));
    }

    if module_ident == (&SUI_FRAMEWORK_ADDRESS, TRANSFER_MODULE)
        && PRIVATE_TRANSFER_FUNCTIONS.contains(&function)
    {
        let msg = format!(
            "Cannot directly call sui::{m}::{f}. \
                Use the public variant instead, sui::{m}::public_{f}",
            m = TRANSFER_MODULE,
            f = function
        );
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::NonEntryFunctionInvoked,
            msg,
        ));
    }

    Ok(())
}

pub fn finish(
    protocol_config: &ProtocolConfig,
    state_view: &dyn ExecutionState,
    gas_charger: &mut GasCharger,
    tx_context: &TxContext,
    by_value_shared_objects: &BTreeSet<ObjectID>,
    consensus_owner_objects: &BTreeMap<ObjectID, Owner>,
    loaded_runtime_objects: BTreeMap<ObjectID, LoadedRuntimeObject>,
    written_objects: BTreeMap<ObjectID, Object>,
    created_object_ids: IndexSet<ObjectID>,
    deleted_object_ids: IndexSet<ObjectID>,
    user_events: Vec<(ModuleId, StructTag, Vec<u8>)>,
    accumulator_events: Vec<MoveAccumulatorEvent>,
    settlement_input_sui: u64,
    settlement_output_sui: u64,
) -> Result<ExecutionResults, ExecutionError> {
    // Before finishing, ensure that any shared object taken by value by the transaction is either:
    // 1. Mutated (and still has a shared ownership); or
    // 2. Deleted.
    // Otherwise, the shared object operation is not allowed and we fail the transaction.
    for id in by_value_shared_objects {
        // If it's been written it must have been reshared so must still have an ownership
        // of `Shared`.
        if let Some(obj) = written_objects.get(id) {
            if !obj.is_shared() {
                if protocol_config.per_command_shared_object_transfer_rules() {
                    invariant_violation!(
                        "There should be no shared objects unaccounted for when \
                            per_command_shared_object_transfer_rules is enabled"
                    )
                } else {
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
            }
        } else {
            // If it's not in the written objects, the object must have been deleted. Otherwise
            // it's an error.
            if !deleted_object_ids.contains(id) {
                if protocol_config.per_command_shared_object_transfer_rules() {
                    invariant_violation!(
                        "There should be no shared objects unaccounted for when \
                            per_command_shared_object_transfer_rules is enabled"
                    )
                } else {
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
    }

    // Before finishing, enforce auth restrictions on consensus objects.
    for (id, original_owner) in consensus_owner_objects {
        let Owner::ConsensusAddressOwner { owner, .. } = original_owner else {
            panic!(
                "verified before adding to `consensus_owner_objects` that these are ConsensusAddressOwner"
            );
        };
        // Already verified in pre-execution checks that tx sender is the object owner.
        // Owner is allowed to do anything with the object.
        if tx_context.sender() != *owner {
            debug_fatal!(
                "transaction with a singly owned input object where the tx sender is not the owner should never be executed"
            );
            if protocol_config.per_command_shared_object_transfer_rules() {
                invariant_violation!(
                    "Shared object operation on {} not allowed: \
                        transaction with singly owned input object must be sent by the owner",
                    id,
                );
            } else {
                return Err(ExecutionError::new(
                                ExecutionErrorKind::SharedObjectOperationNotAllowed,
                                Some(
                                    format!("Shared object operation on {} not allowed: \
                                             transaction with singly owned input object must be sent by the owner", id).into(),
                                ),
                            ));
            }
        }
        // If an Owner type is implemented with support for more fine-grained authorization,
        // checks should be performed here. For example, transfers and wraps can be detected
        // by comparing `original_owner` with:
        // let new_owner = written_objects.get(&id).map(|obj| obj.owner);
        //
        // Deletions can be detected with:
        // let deleted = deleted_object_ids.contains(&id);
    }

    let user_events: Vec<Event> = user_events
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

    let mut receiving_funds_type_and_owners = BTreeMap::new();
    let accumulator_events = accumulator_events
        .into_iter()
        .map(|accum_event| {
            if let Some(ty) = Balance::maybe_get_balance_type_param(&accum_event.target_ty) {
                receiving_funds_type_and_owners
                    .entry(ty)
                    .or_insert_with(BTreeSet::new)
                    .insert(accum_event.target_addr.into());
            }
            let value = match accum_event.value {
                MoveAccumulatorValue::U64(amount) => AccumulatorValue::Integer(amount),
                MoveAccumulatorValue::EventRef(event_idx) => {
                    let Some(event) = user_events.get(event_idx as usize) else {
                        invariant_violation!(
                            "Could not find authenticated event at index {}",
                            event_idx
                        );
                    };
                    let digest = event.digest();
                    AccumulatorValue::EventDigest(event_idx, digest)
                }
            };

            let address =
                AccumulatorAddress::new(accum_event.target_addr.into(), accum_event.target_ty);

            let write = AccumulatorWriteV1 {
                address,
                operation: accum_event.action.into_sui_accumulator_action(),
                value,
            };

            Ok(AccumulatorEvent::new(
                AccumulatorObjId::new_unchecked(accum_event.accumulator_id),
                write,
            ))
        })
        .collect::<Result<Vec<_>, ExecutionError>>()?;

    for object in written_objects.values() {
        let coin_type = object.type_().and_then(|ty| ty.coin_type_maybe());
        let owner = object.owner.get_address_owner_address();
        if let (Some(ty), Ok(owner)) = (coin_type, owner) {
            receiving_funds_type_and_owners
                .entry(ty)
                .or_insert_with(BTreeSet::new)
                .insert(owner);
        }
    }
    let DenyListResult {
        result,
        num_non_gas_coin_owners,
    } = state_view.check_coin_deny_list(receiving_funds_type_and_owners);
    gas_charger.charge_coin_transfers(protocol_config, num_non_gas_coin_owners)?;
    result?;

    Ok(ExecutionResults::V2(ExecutionResultsV2 {
        written_objects,
        modified_objects: loaded_runtime_objects
            .into_iter()
            .filter_map(|(id, loaded)| loaded.is_modified.then_some(id))
            .collect(),
        created_object_ids: created_object_ids.into_iter().collect(),
        deleted_object_ids: deleted_object_ids.into_iter().collect(),
        user_events,
        accumulator_events,
        settlement_input_sui,
        settlement_output_sui,
    }))
}

pub fn fetch_package(
    state_view: &impl BackingPackageStore,
    package_id: &ObjectID,
) -> Result<PackageObject, ExecutionError> {
    let mut fetched_packages = fetch_packages(state_view, vec![package_id])?;
    assert_invariant!(
        fetched_packages.len() == 1,
        "Number of fetched packages must match the number of package object IDs if successful."
    );
    match fetched_packages.pop() {
        Some(pkg) => Ok(pkg),
        None => invariant_violation!(
            "We should always fetch a package for each object or return a dependency error."
        ),
    }
}

pub fn fetch_packages<'ctx, 'state>(
    state_view: &'state impl BackingPackageStore,
    package_ids: impl IntoIterator<Item = &'ctx ObjectID>,
) -> Result<Vec<PackageObject>, ExecutionError> {
    let package_ids: BTreeSet<_> = package_ids.into_iter().collect();
    match get_package_objects(state_view, package_ids) {
        Err(e) => Err(ExecutionError::new_with_source(
            ExecutionErrorKind::PublishUpgradeMissingDependency,
            e,
        )),
        Ok(Err(missing_deps)) => {
            let msg = format!(
                "Missing dependencies: {}",
                missing_deps
                    .into_iter()
                    .map(|dep| format!("{}", dep))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PublishUpgradeMissingDependency,
                msg,
            ))
        }
        Ok(Ok(pkgs)) => Ok(pkgs),
    }
}

pub fn check_compatibility(
    protocol_config: &ProtocolConfig,
    existing_package: &MovePackage,
    upgrading_modules: &[CompiledModule],
    policy: u8,
) -> Result<(), ExecutionError> {
    // Make sure this is a known upgrade policy.
    let Ok(policy) = UpgradePolicy::try_from(policy) else {
        return Err(ExecutionError::from_kind(
            ExecutionErrorKind::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::UnknownUpgradePolicy { policy },
            },
        ));
    };

    let pool = &mut normalized::RcPool::new();
    let binary_config = to_binary_config(protocol_config);
    let Ok(current_normalized) =
        existing_package.normalize(pool, &binary_config, /* include code */ true)
    else {
        invariant_violation!("Tried to normalize modules in existing package but failed")
    };

    let existing_modules_len = current_normalized.len();
    let upgrading_modules_len = upgrading_modules.len();
    let disallow_new_modules = policy as u8 == UpgradePolicy::DEP_ONLY;

    if disallow_new_modules && existing_modules_len != upgrading_modules_len {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
            },
            format!(
                "Existing package has {existing_modules_len} modules, but new package has \
                     {upgrading_modules_len}. Adding or removing a module to a deps only package is not allowed."
            ),
        ));
    }

    let mut new_normalized = normalize_deserialized_modules(
        pool,
        upgrading_modules.iter(),
        /* include code */ true,
    );
    for (name, cur_module) in current_normalized {
        let Some(new_module) = new_normalized.remove(&name) else {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PackageUpgradeError {
                    upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
                },
                format!("Existing module {name} not found in next version of package"),
            ));
        };

        check_module_compatibility(&policy, &cur_module, &new_module)?;
    }

    // If we disallow new modules double check that there are no modules left in `new_normalized`.
    debug_assert!(!disallow_new_modules || new_normalized.is_empty());

    Ok(())
}

fn check_module_compatibility(
    policy: &UpgradePolicy,
    cur_module: &move_binary_format::compatibility::Module,
    new_module: &move_binary_format::compatibility::Module,
) -> Result<(), ExecutionError> {
    match policy {
        UpgradePolicy::Additive => InclusionCheck::Subset.check(cur_module, new_module),
        UpgradePolicy::DepOnly => InclusionCheck::Equal.check(cur_module, new_module),
        UpgradePolicy::Compatible => {
            let compatibility = Compatibility::upgrade_check();

            compatibility.check(cur_module, new_module)
        }
    }
    .map_err(|e| {
        ExecutionError::new_with_source(
            ExecutionErrorKind::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
            },
            e,
        )
    })
}

/// Check for valid shared object usage, either deleted or re-shared, at the end of a command
pub fn check_shared_object_usage<'a>(
    object_runtime: &ObjectRuntime,
    consumed_shared_objects: impl IntoIterator<Item = &'a ObjectID>,
) -> Result<(), ExecutionError> {
    for id in consumed_shared_objects {
        // valid if done deleted or re-shared
        let is_valid_usage = object_runtime.is_deleted(id)
            || matches!(
                object_runtime.is_transferred(id),
                Some(Owner::Shared { .. })
            );
        if !is_valid_usage {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::SharedObjectOperationNotAllowed,
                format!(
                    "Shared object operation on {} not allowed: \
                        cannot be frozen, transferred, or wrapped",
                    id
                ),
            ));
        }
    }
    Ok(())
}
