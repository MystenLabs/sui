// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

use crate::{
    adapter::new_native_extensions,
    gas_charger::GasCharger,
    static_programmable_transactions::{
        env::Env,
        execution::values::{
            borrow_value, coin_subtract_balance, load_value, InputObjectMetadata, InputObjectValue,
            InputValue,
        },
        typing::ast as T,
    },
};
use move_core_types::language_storage::{ModuleId, StructTag};
use move_vm_runtime::native_extensions::NativeContextExtensions;
use move_vm_types::values::Value;
use sui_move_natives::object_runtime::{self, get_all_uids};
use sui_types::{
    base_types::{ObjectID, TxContext},
    error::ExecutionError,
    metrics::LimitsMetrics,
};
use tracing::instrument;

/// Maintains all runtime state specific to programmable transactions
pub struct Context<'a, 'vm, 'state, 'linkage> {
    pub env: Env<'a, 'vm, 'state, 'linkage>,
    /// Metrics for reporting exceeded limits
    pub metrics: Arc<LimitsMetrics>,
    pub native_extensions: NativeContextExtensions<'state>,
    /// A shared transaction context, contains transaction digest information and manages the
    /// creation of new object IDs
    pub tx_context: Rc<RefCell<TxContext>>,
    /// The gas charger used for metering
    pub gas_charger: &'a mut GasCharger,
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

impl<'a, 'vm, 'state, 'linkage> Context<'a, 'vm, 'state, 'linkage> {
    #[instrument(name = "Context::new", level = "trace", skip_all)]
    pub fn new(
        env: Env<'a, 'vm, 'state, 'linkage>,
        metrics: Arc<LimitsMetrics>,
        tx_context: Rc<RefCell<TxContext>>,
        gas_charger: &'a mut GasCharger,
        inputs: T::Inputs,
    ) -> Result<Self, ExecutionError>
    where
        'a: 'state,
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
                let gas_ref = borrow_value(gas_ref)?;
                // We have already checked that the gas balance is enough to cover the gas budget
                let max_gas_in_balance = gas_charger.gas_budget();
                coin_subtract_balance(gas_ref, max_gas_in_balance)?;
                Some(gas)
            }
            None => None,
        };
        let native_extensions = new_native_extensions(
            env.state_view().as_child_resolver(),
            input_object_map,
            !gas_charger.is_unmetered(),
            env.protocol_config(),
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
}

fn load_input_arg(
    env: &Env,
    input_object_map: &mut BTreeMap<ObjectID, object_runtime::InputObject>,
    arg: T::InputArg,
    ty: T::InputType,
) -> Result<InputValue, ExecutionError> {
    Ok(match arg {
        T::InputArg::Pure(bytes) => InputValue::Pure(bytes),
        T::InputArg::Receiving((id, version, _)) => InputValue::Receiving { id, version },
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

    let v = load_value(env, move_obj.contents(), ty)?;
    let input_object_value = InputObjectValue {
        object_metadata,
        value: Some(v),
    };

    Ok(input_object_value)
}
