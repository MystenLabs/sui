// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    marker::PhantomData,
};

use move_binary_format::{errors::VMError, file_format::LocalIndex};
use move_vm_runtime::{move_vm::MoveVM, session::Session};
use sui_cost_tables::bytecode_tables::GasStatus;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TxContext},
    error::{ExecutionError, ExecutionErrorKind},
    messages::{Argument, CallArg, EntryArgumentErrorKind, ObjectArg},
    object::Owner,
    storage::Storage,
};

use crate::adapter::new_session;

use super::types::*;

pub struct ExecutionContext<'vm, 'state, 'a, 'b, E: fmt::Debug, S: StorageView<E>> {
    pub protocol_config: &'a ProtocolConfig,
    /// The MoveVM
    pub vm: &'vm MoveVM,
    /// The global state, used for resolving packages
    pub state_view: &'state S,
    /// A shared transaction context, contains transaction digest information and manages the
    /// creation of new object IDs
    pub tx_context: &'a mut TxContext,
    /// The gas status used for metering
    pub gas_status: &'a mut GasStatus<'b>,
    /// The session used for interacting with Move types and calls
    pub session: Session<'state, 'vm, S>,
    /// Owner meta data for input objects,
    _object_owner_map: BTreeMap<ObjectID, Owner>,
    /// Additional transfers not from the Move runtime
    additional_transfers: Vec<(/* new owner */ SuiAddress, ObjectValue)>,
    // runtime data
    /// The runtime value for the Gas coin, None if it has been taken/moved
    gas: Option<Value>,
    /// The runtime value for the inputs/call args, None if it has been taken/moved
    inputs: Vec<Option<Value>>,
    /// The results of a given command. For most commands, the inner vector will have length 1.
    /// It will only not be 1 for Move calls with multiple return values.
    /// Inner values are None if taken/moved by-value
    results: Vec<Vec<Option<Value>>>,
    /// Map of arguments that are currently borrowed in this command, true if the borrow is mutable
    /// This gets cleared out when new results are pushed, i.e. the end of a command
    borrowed: HashMap<Argument, /* mut */ bool>,
    _e: PhantomData<E>,
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
        gas_status: &'a mut GasStatus<'b>,
        gas_coin: ObjectID,
        inputs: Vec<CallArg>,
    ) -> Result<Self, ExecutionError> {
        let mut _object_owner_map = BTreeMap::new();
        let inputs = inputs
            .into_iter()
            .map(|call_arg| {
                Ok(Some(load_call_arg(
                    state_view,
                    &mut _object_owner_map,
                    call_arg,
                )?))
            })
            .collect::<Result<_, ExecutionError>>()?;
        let gas = Some(Value::Object(load_object(
            state_view,
            &mut _object_owner_map,
            None,
            gas_coin,
        )?));
        let session = new_session(
            vm,
            state_view,
            _object_owner_map.clone(),
            gas_status.is_metered(),
            protocol_config,
        );
        Ok(Self {
            protocol_config,
            vm,
            state_view,
            tx_context,
            gas_status,
            session,
            _object_owner_map,
            gas,
            inputs,
            results: vec![],
            additional_transfers: vec![],
            borrowed: HashMap::new(),
            _e: PhantomData,
        })
    }

    /// Create a new ID and update the state
    pub fn fresh_id(&mut self) -> Result<ObjectID, ExecutionError> {
        if true {
            todo!("update native context set")
        }
        Ok(self.tx_context.fresh_id())
    }

    /// Delete an ID and update the state
    pub fn delete_id(&mut self, _object_id: ObjectID) -> Result<(), ExecutionError> {
        if true {
            todo!("update native context set")
        }
        Ok(())
    }

    pub fn gas_is_taken(&self) -> bool {
        self.gas.is_none()
    }

    pub fn take_arg<V: TryFromValue>(
        &mut self,
        command_kind: CommandKind<'_>,
        arg_idx: usize,
        arg: Argument,
    ) -> Result<V, ExecutionError> {
        if matches!(arg, Argument::GasCoin) && !matches!(command_kind, CommandKind::TransferObjects)
        {
            panic!("cannot take gas")
        }
        if self.arg_is_borrowed(&arg) {
            panic!("taken borrowed value")
        }
        let val_opt = self.borrow_mut(arg)?;
        if val_opt.is_none() {
            panic!("taken value")
        }
        let val = val_opt.take().unwrap();
        if let Value::Object(obj) = &val {
            if let Some(Owner::Shared { .. } | Owner::Immutable) = obj.owner {
                let error = format!(
                    "Immutable and shared objects cannot be passed by-value, \
                                violation found in argument {}",
                    arg_idx
                );
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::entry_argument_error(
                        arg_idx as LocalIndex,
                        EntryArgumentErrorKind::InvalidObjectByValue,
                    ),
                    error,
                ));
            }
        }
        V::try_from_value(val)
    }

    pub fn borrow_arg_mut<V: TryFromValue>(
        &mut self,
        arg_idx: usize,
        arg: Argument,
    ) -> Result<V, ExecutionError> {
        if self.arg_is_borrowed(&arg) {
            panic!("mutable args can only be used once in a given command")
        }
        self.borrowed.insert(arg, /* is_mut */ true);
        let val_opt = self.borrow_mut(arg)?;
        if val_opt.is_none() {
            panic!("taken value")
        }
        let val = val_opt.take().unwrap();
        if let Value::Object(obj) = &val {
            if let Some(Owner::Immutable) = obj.owner {
                let error = format!(
                    "Argument {} is expected to be mutable, immutable object found",
                    arg_idx
                );
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::entry_argument_error(
                        arg_idx as LocalIndex,
                        EntryArgumentErrorKind::InvalidObjectByMuteRef,
                    ),
                    error,
                ));
            }
        }
        V::try_from_value(val)
    }

    pub fn clone_arg<V: TryFromValue>(
        &mut self,
        _arg_idx: usize,
        arg: Argument,
    ) -> Result<V, ExecutionError> {
        if self.arg_is_mut_borrowed(&arg) {
            panic!("mutable args can only be used once in a given command")
        }
        let val_opt = self.borrow_mut(arg)?;
        if val_opt.is_none() {
            panic!("taken value")
        }
        let val = val_opt.as_ref().unwrap().clone();
        V::try_from_value(val)
    }

    pub fn borrow_arg<V: TryFromValue>(
        &mut self,
        _arg_idx: usize,
        arg: Argument,
    ) -> Result<V, ExecutionError> {
        if self.arg_is_mut_borrowed(&arg) {
            panic!("mutable args can only be used once in a given command")
        }
        self.borrowed.insert(arg, /* is_mut */ false);
        let val_opt = self.borrow_mut(arg)?;
        if val_opt.is_none() {
            panic!("taken value")
        }
        V::try_from_value(val_opt.as_ref().unwrap().clone())
    }

    pub fn restore_arg(&mut self, arg: Argument, value: Value) -> Result<(), ExecutionError> {
        assert_invariant!(
            self.arg_is_mut_borrowed(&arg),
            "Should never restore a non-mut borrowed value. \
            The take+restore is an implementation detail of mutable references"
        );
        let old_value = self.borrow_mut(arg)?.replace(value);
        assert_invariant!(
            old_value.is_none(),
            "Should never restore a non-taken value. \
            The take+restore is an implementation detail of mutable references"
        );
        Ok(())
    }

    pub fn mark_used_in_non_entry_move_call(&mut self, arg: Argument) {
        if let Ok(Some(val)) = self.borrow_mut(arg) {
            match val {
                Value::Object(obj) => obj.used_in_non_entry_move_call = true,
                // nothing to do for raw, either it is pure bytes from input and there is nothing
                // to change, or it is a Move value and it is never not tainted
                Value::Raw(_, _) => (),
            }
        }
    }

    pub fn push_command_results(&mut self, results: Vec<Value>) -> Result<(), ExecutionError> {
        assert_invariant!(
            self.borrowed.values().all(|is_mut| !is_mut),
            "all mut borrows should be restored"
        );
        // clear borrow state
        self.borrowed = HashMap::new();
        self.results.push(results.into_iter().map(Some).collect());
        Ok(())
    }

    fn arg_is_borrowed(&self, arg: &Argument) -> bool {
        self.borrowed.contains_key(arg)
    }

    fn arg_is_mut_borrowed(&self, arg: &Argument) -> bool {
        matches!(self.borrowed.get(arg), Some(/* mut */ true))
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

    pub fn convert_vm_error(&self, error: VMError) -> ExecutionError {
        sui_types::error::convert_vm_error(error, self.vm, self.state_view)
    }
}

fn load_object<S: Storage>(
    state_view: &S,
    object_owner_map: &mut BTreeMap<ObjectID, Owner>,
    // used for masking the owner, specifically in the case where a shared object is read-only and
    // acts like an immutable object
    owner_override: Option<Owner>,
    id: ObjectID,
) -> Result<ObjectValue, ExecutionError> {
    let Some(obj) = state_view.read_object(&id) else {
        // protected by transaction input checker
        invariant_violation!(format!("Object {} does not exist yet", id));
    };
    let owner = owner_override.unwrap_or(obj.owner);
    let prev = object_owner_map.insert(id, owner);
    // protected by transaction input checker
    assert_invariant!(prev.is_none(), format!("Duplicate input object {}", id));
    let mut obj_value = ObjectValue::from_object(obj)?;
    // propagate override
    obj_value.owner = Some(owner);
    Ok(obj_value)
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
        ObjectArg::ImmOrOwnedObject((id, _, _))
        | ObjectArg::SharedObject {
            id, mutable: true, ..
        } => load_object(state_view, object_owner_map, None, id),
        ObjectArg::SharedObject {
            id, mutable: false, ..
        } => load_object(state_view, object_owner_map, Some(Owner::Immutable), id),
    }
}
