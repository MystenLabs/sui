// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ast as T;
use crate::{
    execution_value::ExecutionState,
    programmable_transactions::{
        context::{load_type_from_struct, EitherError},
        linkage_view::LinkageView,
    },
    sui_types::move_package::UpgradeTicket,
};
use move_binary_format::{errors::VMError, file_format::AbilitySet};
use move_core_types::language_storage::StructTag;
use move_vm_runtime::move_vm::MoveVM;
use move_vm_types::loaded_data::runtime_types::Type;
use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    error::{command_argument_error, ExecutionError},
    execution_status::CommandArgumentError,
    move_package::UpgradeReceipt,
    transaction::{self as P, CallArg, ObjectArg, ProgrammableTransaction},
};

struct TypingContext<'a, 'b, 'state> {
    protocol_config: &'a ProtocolConfig,
    vm: &'a MoveVM,
    state_view: &'a dyn ExecutionState,
    linkage_view: &'b mut LinkageView<'state>,
    gathered_input_types: BTreeMap<u16, BTreeSet<Type>>,
    inputs: Vec<(CallArg, InputType)>,
    results: Vec<T::ResultType>,
    gas_coin_type: OnceCell<Type>,
    upgrade_ticket_type: OnceCell<Type>,
    upgrade_receipt_type: OnceCell<Type>,
}

enum InputType {
    BCSBytes,
    Receiving,
    Fixed(Type),
}

macro_rules! get_or_init_ty {
    ($context:expr, $ident:ident, $tag:expr) => {{
        let context = $context;
        if context.$ident.get().is_none() {
            let tag = $tag;
            let ty = context.load_type_from_struct(&tag)?;
            context.$ident.set(ty.clone()).unwrap();
        }
        Ok(context.$ident.get().unwrap())
    }};
}

impl<'a, 'b, 'state> TypingContext<'a, 'b, 'state> {
    fn new(
        protocol_config: &'a ProtocolConfig,
        vm: &'a MoveVM,
        state_view: &'a dyn ExecutionState,
        linkage_view: &'b mut LinkageView<'state>,
        input_args: Vec<CallArg>,
    ) -> Result<Self, ExecutionError> {
        let mut context = Self {
            protocol_config,
            vm,
            state_view,
            linkage_view,
            gathered_input_types: BTreeMap::new(),
            inputs: vec![],
            results: vec![],
            gas_coin_type: OnceCell::new(),
            upgrade_ticket_type: OnceCell::new(),
            upgrade_receipt_type: OnceCell::new(),
        };
        context.inputs = input_args
            .into_iter()
            .map(|arg| {
                let ty = match &arg {
                    CallArg::Pure(_) => InputType::BCSBytes,
                    CallArg::Object(
                        ObjectArg::ImmOrOwnedObject((id, _, _))
                        | ObjectArg::SharedObject { id, .. },
                    ) => {
                        let Some(obj) = state_view.read_object(&id) else {
                            // protected by transaction input checker
                            invariant_violation!("Object {:?} does not exist", id);
                        };
                        let Some(ty) = obj.type_() else {
                            invariant_violation!("Object {:?} has does not have a Move type", id);
                        };
                        let tag: StructTag = ty.clone().into();
                        let ty = context.load_type_from_struct(&tag)?;
                        InputType::Fixed(ty)
                    }
                    CallArg::Object(ObjectArg::Receiving(_)) => InputType::Receiving,
                };
                Ok((arg, ty))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        Ok(context)
    }

    fn finish(self) -> Vec<(CallArg, T::InputType)> {
        let Self {
            mut gathered_input_types,
            inputs,
            ..
        } = self;
        inputs
            .into_iter()
            .enumerate()
            .map(|(i, (arg, ty))| {
                let ty = match ty {
                    InputType::Fixed(t) => T::InputType::Fixed(t),
                    InputType::Receiving => T::InputType::Receiving,
                    InputType::BCSBytes => {
                        let tys = gathered_input_types.remove(&(i as u16)).unwrap_or_default();
                        T::InputType::BCSBytes(tys)
                    }
                };
                (arg, ty)
            })
            .collect()
    }

    fn convert_vm_error(&self, e: VMError) -> ExecutionError {
        crate::error::convert_vm_error(
            e,
            self.vm,
            self.linkage_view,
            self.protocol_config.resolve_abort_locations_to_package_id(),
        )
    }

    fn load_type_from_struct(&mut self, tag: &StructTag) -> Result<Type, ExecutionError> {
        load_type_from_struct(self.vm, self.linkage_view, &[], tag)
            .map_err(|e| self.convert_vm_error(e))
    }

    fn gas_coin_type(&mut self) -> Result<&Type, ExecutionError> {
        get_or_init_ty!(self, gas_coin_type, GAS::type_())
    }

    fn upgrade_ticket_type(&mut self) -> Result<&Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_ticket, UpgradeTicket::type_())
    }

    fn upgrade_receipt_type(&mut self) -> Result<&Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_receipt, UpgradeReceipt::type_())
    }

    fn abilities(&self, ty: &Type) -> Result<AbilitySet, ExecutionError> {
        self.vm
            .get_runtime()
            .get_type_abilities(ty)
            .map_err(|e| self.convert_vm_error(e))
    }
}

pub fn translate(
    protocol_config: &ProtocolConfig,
    vm: &MoveVM,
    state_view: &dyn ExecutionState,
    linkage_view: &mut LinkageView,
    pt: ProgrammableTransaction,
) -> Result<T::Transaction, ExecutionError> {
    let ProgrammableTransaction { inputs, commands } = pt;
    let mut context = TypingContext::new(protocol_config, vm, state_view, linkage_view, inputs)?;
    let commands = commands
        .into_iter()
        .enumerate()
        .map(|(i, c)| command(&mut context, c).map_err(|e| e.with_command_index(i)))
        .collect::<Result<Vec<_>, _>>()?;
    let inputs = context.finish();
    Ok(T::Transaction { inputs, commands })
}

fn command(
    context: &mut TypingContext,
    command: P::Command,
) -> Result<(T::Command, T::ResultType), ExecutionError> {
    Ok(match command {
        P::Command::MoveCall(programmable_move_call) => todo!(),
        P::Command::TransferObjects(arguments, argument) => todo!(),
        P::Command::SplitCoins(argument, arguments) => todo!(),
        P::Command::MergeCoins(argument, arguments) => todo!(),
        P::Command::MakeMoveVec(type_input, arguments) => todo!(),
        P::Command::Publish(items, object_ids) => (T::Command::Publish(items, object_ids), vec![]),
        P::Command::Upgrade(items, object_ids, object_id, pa) => {
            let location = one_location(context, 0, pa)?;
            let expected_ty = context.upgrade_ticket()?;
            let a = argument(context, location, expected_ty)?;
            let res = context.upgrade_receipt()?;
            (
                T::Command::Upgrade(items, object_ids, object_id, a),
                vec![res.clone()],
            )
        }
    })
}

fn one_location(
    context: &mut TypingContext,
    command_arg_idx: usize,
    arg: P::Argument,
) -> Result<T::Location, ExecutionError> {
    let locs = locations(context, command_arg_idx, vec![arg])?;
    let Ok([loc]): Result<[T::Location; 1], _> = locs.try_into() else {
        return Err(command_argument_error(
            CommandArgumentError::InvalidArgumentArity,
            command_arg_idx,
        ));
    };
    Ok(loc)
}

fn locations<Items: IntoIterator<Item = P::Argument>>(
    context: &mut TypingContext,
    start_idx: usize,
    args: Items,
) -> Result<Vec<T::Location>, ExecutionError>
where
    Items::IntoIter: ExactSizeIterator,
{
    fn splat_arg(
        context: &mut TypingContext,
        res: &mut Vec<T::Location>,
        arg: P::Argument,
    ) -> Result<(), EitherError> {
        match arg {
            P::Argument::GasCoin => res.push(T::Location::GasCoin),
            P::Argument::Input(i) => {
                if i as usize >= context.inputs.len() {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i }.into());
                }
                res.push(T::Location::Input(i))
            }
            P::Argument::NestedResult(i, j) => {
                let Some(command_result) = context.results.get(i as usize) else {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i }.into());
                };
                if j as usize >= command_result.len() {
                    return Err(CommandArgumentError::SecondaryIndexOutOfBounds {
                        result_idx: i,
                        secondary_idx: j,
                    }
                    .into());
                };
                res.push(T::Location::Result(i, j))
            }
            P::Argument::Result(i) => {
                let Some(result) = context.results.get(i as usize) else {
                    return Err(CommandArgumentError::IndexOutOfBounds { idx: i }.into());
                };
                let Ok(len): Result<u16, _> = result.len().try_into() else {
                    invariant_violation!("Result of length greater than u16::MAX");
                };
                if len != 1 {
                    // TODO protocol config to allow splatting of args
                    return Err(CommandArgumentError::InvalidResultArity { result_idx: i }.into());
                }
                res.extend((0..len).map(|j| T::Location::Result(i, j)))
            }
        }
        Ok(())
    }

    let args = args.into_iter();
    let _args_len = args.len();
    let mut res = vec![];
    for (arg_idx, arg) in args.enumerate() {
        splat_arg(context, &mut res, arg)
            .map_err(|e| e.into_execution_error(start_idx + arg_idx))?;
    }
    debug_assert_eq!(res.len(), _args_len);
    Ok(res)
}

fn argument(
    context: &mut TypingContext,
    location: T::Location,
    expected_ty: &Type,
) -> Result<T::Argument, ExecutionError> {
    fn check_location_type(
        context: &mut TypingContext,
        location: T::Location,
        expected_ty: &Type,
    ) -> Result<(), ExecutionError> {
        let ty = match location {
            T::Location::GasCoin => todo!(),
            T::Location::Input(_) => todo!(),
            T::Location::Result(_, _) => todo!(),
        }
    }


    Ok(match expected_ty {
        Type::Reference(inner) => {
            check_location_type(context, location, inner)?;
            T::Argument::Borrow(/* mut */ false, location)
        }
        Type::MutableReference(inner) => {
            check_location_type(context, location, inner)?;
            maybe_fix_pure_type(context, location, inner)?;
            T::Argument::Borrow(/* mut */ true, location)
        }
        _ => {
            check_location_type(context, location, expected_ty)?;
            if context.abilities(expected_ty)?.has_copy() {
                T::Argument::Copy(location)
            } else {
                T::Argument::Move(location)
            }
        }
    })
}
