// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt};

use move_core_types::resolver::{ModuleResolver, ResourceResolver};
use move_vm_runtime::{move_vm::MoveVM, session::Session};
use sui_cost_tables::bytecode_tables::GasStatus;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TxContext},
    error::ExecutionError,
    messages::{Argument, CallArg, ObjectArg},
    object::Owner,
    storage::{ChildObjectResolver, ParentSync, Storage},
};

use crate::adapter::new_session;

use super::types::*;

pub struct ExecutionContext<
    'vm,
    'state,
    'a,
    'b,
    E: fmt::Debug,
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
> {
    pub protocol_config: &'a ProtocolConfig,
    /// The MoveVM
    vm: &'vm MoveVM,
    /// The global state, used for resolving packages
    state_view: &'state S,
    /// A shared transaction context, contains transaction digest information and manages the
    /// creation of new object IDs
    ctx: &'a mut TxContext,
    /// The gas status used for metering
    gas_status: &'a mut GasStatus<'b>,
    /// The session used for interacting with Move types and calls
    session: Option<Session<'state, 'vm, S>>,
    /// Owner meta data for input objects,
    object_owner_map: BTreeMap<ObjectID, Owner>,
    /// The runtime value for the Gas coin, None if it has been taken/moved
    pub gas: Option<Value>,
    /// The runtime value for the inputs/call args, None if it has been taken/moved
    pub inputs: Vec<Option<Value>>,
    /// The results of a given command. For most commands, the inner vector will have length 1.
    /// It will only not be 1 for Move calls with multiple return values.
    /// Inner values are None if taken/moved by-value
    pub results: Vec<Vec<Option<Value>>>,
    /// Additional transfers not from the Move runtime
    additional_transfers: Vec<(/* new owner */ SuiAddress, ObjectValue)>,
}
impl<'vm, 'state, 'a, 'b, E, S> ExecutionContext<'vm, 'state, 'a, 'b, E, S>
where
    E: fmt::Debug,
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
{
    pub fn new(
        protocol_config: &'a ProtocolConfig,
        vm: &'vm MoveVM,
        state_view: &'state S,
        ctx: &'a mut TxContext,
        gas_status: &'a mut GasStatus<'b>,
        gas_coin: ObjectID,
        inputs: Vec<CallArg>,
    ) -> Result<Self, ExecutionError> {
        let mut object_owner_map = BTreeMap::new();
        let inputs = inputs
            .into_iter()
            .map(|call_arg| {
                Ok(Some(load_call_arg(
                    state_view,
                    &mut object_owner_map,
                    call_arg,
                )?))
            })
            .collect::<Result<_, ExecutionError>>()?;
        let gas = Some(Value::Object(load_object(
            state_view,
            &mut object_owner_map,
            gas_coin,
        )?));
        Ok(Self {
            protocol_config,
            vm,
            state_view,
            ctx,
            gas_status,
            session: None,
            object_owner_map,
            gas,
            inputs,
            results: vec![],
            additional_transfers: vec![],
        })
    }

    /// Access the session
    pub fn session(&mut self) -> &Session<'state, 'vm, S> {
        self.session.get_or_insert_with(|| {
            new_session(
                self.vm,
                self.state_view,
                self.object_owner_map.clone(),
                self.gas_status.is_metered(),
                self.protocol_config,
            )
        })
    }

    /// Create a new ID and update the state
    pub fn fresh_id(&mut self) -> Result<ObjectID, ExecutionError> {
        if true {
            todo!("update native context set")
        }
        Ok(self.ctx.fresh_id())
    }

    /// Delete an ID and update the state
    pub fn delete_id(&mut self, _object_id: ObjectID) -> Result<(), ExecutionError> {
        if true {
            todo!("update native context set")
        }
        Ok(())
    }

    pub fn take_args<V: TryFromValue>(
        &mut self,
        args: Vec<Argument>,
    ) -> Result<Vec<V>, ExecutionError> {
        args.into_iter()
            .map(|arg| self.take_arg::<V>(arg))
            .collect()
    }

    pub fn take_arg<V: TryFromValue>(&mut self, arg: Argument) -> Result<V, ExecutionError> {
        let val_opt = self.borrow_mut(arg)?;
        if val_opt.is_none() {
            panic!("taken value")
        }
        V::try_from_value(val_opt.take().unwrap())
    }

    pub fn restore_arg(&mut self, arg: Argument, value: Value) -> Result<(), ExecutionError> {
        let old_val = self.borrow_mut(arg)?.replace(value);
        assert_invariant!(
            old_val.is_none(),
            "Should never restore a non-taken value. \
            The take+restore is an implementation detail of mutable references"
        );
        Ok(())
    }

    fn borrow_mut(&mut self, arg: Argument) -> Result<&mut Option<Value>, ExecutionError> {
        Ok(match arg {
            Argument::GasCoin => &mut self.gas,
            Argument::Input(i) => {
                let Some(inner_opt) = self.inputs.get_mut(i as usize) else {
                    panic!("out of bounds")
                };
                inner_opt
            }
            Argument::Result(i) => {
                let Some(command_result) = self.results.get_mut(i as usize) else {
                    panic!("out of bounds")
                };
                if command_result.len() != 1 {
                    panic!("expected a single result")
                }
                &mut command_result[0]
            }
            Argument::NestedResult(i, j) => {
                let Some(command_result) = self.results.get_mut(i as usize) else {
                    panic!("out of bounds")
                };
                let Some(inner_opt) = command_result.get_mut(j as usize) else {
                    panic!("out of bounds")
                };
                inner_opt
            }
        })
    }

    pub fn transfer_object(
        &mut self,
        obj: ObjectValue,
        arg: SuiAddress,
    ) -> Result<(), ExecutionError> {
        self.additional_transfers.push((arg, obj));
        Ok(())
    }
}

fn load_object<S: Storage>(
    state_view: &S,
    object_owner_map: &mut BTreeMap<ObjectID, Owner>,
    id: ObjectID,
) -> Result<ObjectValue, ExecutionError> {
    let Some(obj) = state_view.read_object(&id) else {
        // protected by transaction input checker
        invariant_violation!(format!("Object {} does not exist yet", id));
    };
    let prev = object_owner_map.insert(id, obj.owner);
    // protected by transaction input checker
    assert_invariant!(prev.is_none(), format!("Duplicate input object {}", id));
    ObjectValue::from_object(obj)
}

fn load_call_arg<S: Storage>(
    state_view: &S,
    object_owner_map: &mut BTreeMap<ObjectID, Owner>,
    call_arg: CallArg,
) -> Result<Value, ExecutionError> {
    Ok(match call_arg {
        CallArg::Pure(bytes) => Value::Raw(ValueType::Any, bytes),
        CallArg::Object(obj_arg) => {
            Value::Object(load_object_arg(state_view, object_owner_map, obj_arg)?)
        }
        CallArg::ObjVec(_) => {
            // protected by transaction input checker
            invariant_violation!("ObjVec is not supported in programmable transactions")
        }
    })
}

fn load_object_arg<S: Storage>(
    state_view: &S,
    object_owner_map: &mut BTreeMap<ObjectID, Owner>,
    obj_arg: ObjectArg,
) -> Result<ObjectValue, ExecutionError> {
    match obj_arg {
        ObjectArg::ImmOrOwnedObject((id, _, _)) | ObjectArg::SharedObject { id, .. } => {
            load_object(state_view, object_owner_map, id)
        }
    }
}
