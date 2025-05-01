// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use crate::{
    adapter::new_native_extensions,
    gas_charger::GasCharger,
    static_programmable_transactions::{
        env::Env,
        execution::values::{self, ByteValue, InputObjectMetadata, InputObjectValue, InputValue},
        typing::ast::{self as T, Type},
    },
};
use move_binary_format::{
    errors::Location,
    file_format::{CodeOffset, FunctionDefinitionIndex},
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
};
use move_vm_runtime::native_extensions::NativeContextExtensions;
use move_vm_types::values::{VMValueCast, Value};
use sui_move_natives::object_runtime::{self, get_all_uids, max_event_error, ObjectRuntime};
use sui_types::{
    base_types::{ObjectID, TxContext},
    error::ExecutionError,
    execution::ExecutionResults,
    metrics::LimitsMetrics,
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

macro_rules! object_runtime_mut {
    ($context:ident) => {{
        $context
            .native_extensions
            .get_mut::<ObjectRuntime>()
            .map_err(|e| $context.env.convert_vm_error(e.finish(Location::Undefined)))
    }};
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
    pub native_extensions: NativeContextExtensions<'state>,
    /// A shared transaction context, contains transaction digest information and manages the
    /// creation of new object IDs
    pub tx_context: Rc<RefCell<TxContext>>,
    /// The gas charger used for metering
    pub gas_charger: &'gas mut GasCharger,
    /// User events are claimed after each Move call
    user_events: Vec<(ModuleId, StructTag, Vec<u8>)>,
    // runtime data
    /// The runtime value for the Gas coin, None if no gas coin is provided
    gas: Option<InputObjectValue>,
    /// The runtime value for the inputs/call args
    inputs: Vec<InputValue>,
    /// The results of a given command. For most commands, the inner vector will have length 1.
    /// It will only not be 1 for Move calls with multiple return values.
    /// Inner values are None if taken/moved by-value
    results: Vec<Vec<Option<Value>>>,
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
        'env: 'state,
    {
        let mut input_object_map = BTreeMap::new();
        let inputs = inputs
            .into_iter()
            .map(|(arg, ty)| load_input_arg(&env, &mut input_object_map, arg, ty))
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        let gas = match gas_charger.gas_coin() {
            Some(gas_coin) => {
                let ty = env.gas_coin_type()?;
                let gas = load_object_arg_impl(&env, &mut input_object_map, gas_coin, true, ty)?;
                let Some(gas_ref) = gas.value.as_ref() else {
                    invariant_violation!("Gas object should be a populated coin")
                };
                let gas_ref = values::borrow_value(gas_ref)?;
                // We have already checked that the gas balance is enough to cover the gas budget
                let max_gas_in_balance = gas_charger.gas_budget();
                values::coin_subtract_balance(gas_ref, max_gas_in_balance)?;
                Some(gas)
            }
            None => None,
        };
        let native_extensions = new_native_extensions(
            env.state_view.as_child_resolver(),
            input_object_map,
            !gas_charger.is_unmetered(),
            &env.protocol_config,
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

    pub fn finish(self) -> Result<ExecutionResults, ExecutionError> {
        todo!()
    }

    pub fn object_runtime(&self) -> Result<&ObjectRuntime, ExecutionError> {
        self.native_extensions
            .get::<ObjectRuntime>()
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))
    }

    pub fn take_user_events(
        &mut self,
        module_id: &ModuleId,
        function: FunctionDefinitionIndex,
        last_offset: CodeOffset,
    ) -> Result<(), ExecutionError> {
        let events = object_runtime_mut!(self)?.take_user_events();
        let num_events = self.user_events.len() + events.len();
        let max_events = self.env.protocol_config.max_num_event_emit();
        if num_events as u64 > max_events {
            let err = max_event_error(max_events)
                .at_code_offset(function, last_offset)
                .finish(Location::Module(module_id.clone()));
            return Err(self.env.convert_vm_error(err));
        }
        let new_events = events
            .into_iter()
            .map(|(ty, tag, value)| {
                let layout = self
                    .env
                    .vm
                    .get_runtime()
                    .type_to_type_layout(&ty)
                    .map_err(|e| self.env.convert_vm_error(e))?;
                let Some(bytes) = value.simple_serialize(&layout) else {
                    invariant_violation!("Failed to deserialize already serialized Move value");
                };
                Ok((module_id.clone(), tag, bytes))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        self.user_events.extend(new_events);
        Ok(())
    }

    fn location(
        &mut self,
        usage: UsageKind,
        location: T::Location,
        ty: Type,
    ) -> Result<Value, ExecutionError> {
        let value_opt = match location {
            T::Location::GasCoin => {
                todo!("better error here? How do we handle if there is no gas coin?");
                let gas = unwrap!(self.gas.as_mut(), "Gas coin not provided");
                &mut gas.value
            }
            T::Location::Result(i, j) => {
                let result = unwrap!(self.results.get_mut(i as usize), "bounds already verified");
                let v = unwrap!(result.get_mut(j as usize), "bounds already verified");
                v
            }
            T::Location::Input(i) => {
                let v = unwrap!(self.inputs.get_mut(i as usize), "bounds already verified");
                match v {
                    InputValue::Object(v) => &mut v.value,
                    InputValue::Fixed(v) => v,
                    InputValue::Bytes(bytes) => match usage {
                        UsageKind::Move | UsageKind::Borrow(true) => {
                            let value = load_byte_value(self.env, bytes, ty)?;
                            *v = InputValue::Fixed(Some(value));
                            match v {
                                InputValue::Fixed(v) => v,
                                _ => invariant_violation!("Expected fixed value"),
                            }
                        }
                        UsageKind::Copy | UsageKind::Borrow(false) => {
                            return load_byte_value(self.env, bytes, ty);
                        }
                    },
                }
            }
        };
        Ok(match usage {
            UsageKind::Move => unwrap!(value_opt.take(), "use after move"),
            UsageKind::Copy => {
                let value = unwrap!(value_opt.as_ref(), "use after move");
                copy_value(value)?
            }
            UsageKind::Borrow(_) => {
                let value = unwrap!(value_opt.as_ref(), "use after move");
                values::borrow_value(value)?
            }
        })
    }

    fn location_usage(&mut self, usage: T::Usage, ty: Type) -> Result<Value, ExecutionError> {
        match usage {
            T::Usage::Move(location) => self.location(UsageKind::Move, location, ty),
            T::Usage::Copy { location, .. } => self.location(UsageKind::Copy, location, ty),
        }
    }

    fn argument_value(&mut self, (arg_, ty): T::Argument) -> Result<Value, ExecutionError> {
        match arg_ {
            T::Argument_::Use(usage) => self.location_usage(usage, ty),
            T::Argument_::Borrow(is_mut, location) => {
                let ty = match ty {
                    Type::Reference(_, inner) => (*inner).clone(),
                    _ => invariant_violation!("Expected reference type"),
                };
                self.location(UsageKind::Borrow(is_mut), location, ty)
            }
            T::Argument_::Read(usage) => {
                let value = self.location_usage(usage, ty)?;
                todo!("charge gas");
                values::read_ref(value)
            }
        }
    }

    pub fn argument<V>(&mut self, arg: T::Argument) -> Result<V, ExecutionError>
    where
        Value: VMValueCast<V>,
    {
        let value = self.argument_value(arg)?;
        let value: V = values::cast(value)?;
        Ok(value)
    }

    pub fn arguments<V>(&mut self, args: Vec<T::Argument>) -> Result<Vec<V>, ExecutionError>
    where
        Value: VMValueCast<V>,
    {
        args.into_iter().map(|arg| self.argument(arg)).collect()
    }

    pub fn result(&mut self, result: Vec<Value>) -> Result<(), ExecutionError> {
        self.results.push(result.into_iter().map(Some).collect());
        Ok(())
    }

    pub fn vm_move_call(
        &mut self,
        function: T::LoadedFunction,
        args: Vec<Value>,
    ) -> Result<Vec<Value>, ExecutionError> {
        let storage_id = &function.storage_id;
        let (index, last_instr) = {
            &function;
            // access FunctionDefinitionIndex and last instruction CodeOffset
            todo!("LOADING")
        };
        let result = {
            function;
            todo!("RUNTIME")
        };
        self.take_user_events(storage_id, index, last_instr)?;
        Ok(result)
    }

    pub fn transfer_object(
        &mut self,
        recipient: AccountAddress,
        ty: Type,
        object: Value,
    ) -> Result<(), ExecutionError> {
        let ty = {
            ty;
            // ty to vm type
            todo!("LOADING")
        };
        let owner = sui_types::object::Owner::AddressOwner(recipient.into());
        object_runtime_mut!(self)?
            .transfer(owner, ty, object)
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
    }

    pub fn copy_value(&self, value: &Value) -> Result<Value, ExecutionError> {
        todo!("charge gas");
        values::copy_value(value)
    }

    pub fn new_coin(&mut self, amount: u64) -> Result<Value, ExecutionError> {
        let id = self.tx_context.borrow_mut().fresh_id();
        object_runtime_mut!(self)?
            .new_id(id)
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
        Ok(values::coin(id, amount))
    }

    pub fn destroy_coin(&mut self, coin: Value) -> Result<u64, ExecutionError> {
        let (id, amount) = values::unpack_coin(coin)?;
        object_runtime_mut!(self)?
            .delete_id(id)
            .map_err(|e| self.env.convert_vm_error(e.finish(Location::Undefined)))?;
        Ok(amount)
    }
}

fn load_input_arg(
    env: &Env,
    input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
    arg: T::InputArg,
    ty: T::InputType,
) -> Result<InputValue, ExecutionError> {
    Ok(match arg {
        T::InputArg::Pure(bytes) => InputValue::Bytes(ByteValue::Pure(bytes)),
        T::InputArg::Receiving((id, version, _)) => {
            InputValue::Bytes(ByteValue::Receiving { id, version })
        }
        T::InputArg::Object(arg) => {
            let T::InputType::Fixed(ty) = ty else {
                invariant_violation!("Expected fixed type for object arg");
            };
            InputValue::Object(load_object_arg(env, input_object_map, arg, ty)?)
        }
    })
}

fn load_object_arg(
    env: &Env,
    input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
    arg: T::ObjectArg,
    ty: T::Type,
) -> Result<InputObjectValue, ExecutionError> {
    let id = arg.id();
    let is_mutable_input = arg.is_mutable();
    load_object_arg_impl(env, input_object_map, id, is_mutable_input, ty)
}

fn load_object_arg_impl(
    env: &Env,
    input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
    id: ObjectID,
    is_mutable_input: bool,
    ty: T::Type,
) -> Result<InputObjectValue, ExecutionError> {
    let obj = env.read_object(&id)?;
    let owner = obj.owner.clone();
    let version = obj.version();
    let object_metadata = InputObjectMetadata {
        id,
        is_mutable_input,
        owner: owner.clone(),
        version,
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

    let v = values::load_value(env, move_obj.contents(), ty)?;
    let input_object_value = InputObjectValue {
        object_metadata,
        value: Some(v),
    };

    Ok(input_object_value)
}

fn load_byte_value(env: &Env, value: &ByteValue, ty: Type) -> Result<Value, ExecutionError> {
    let loaded = match value {
        ByteValue::Pure(bytes) => values::load_value(env, bytes, ty)?,
        ByteValue::Receiving { id, version } => values::receiving(*id, *version),
    };
    todo!("charge gas");
    Ok(loaded)
}

fn copy_value(value: &Value) -> Result<Value, ExecutionError> {
    todo!("charge gas");
    values::copy_value(value)
}
