// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use crate::{
    execution_mode::ExecutionMode,
    sp,
    static_programmable_transactions::{
        env::Env,
        typing::ast::{self as T, Type},
    },
};
use move_regex_borrow_graph::{MeterError, meter::DummyMeter, references::Ref};
use mysten_common::ZipDebugEqIteratorExt;
use sui_types::{
    error::{ExecutionErrorTrait, SafeIndex},
    execution_status::{CommandArgumentError, ExecutionErrorKind},
};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Location(T::Location);

type Graph = move_regex_borrow_graph::collections::Graph<(), Location>;
type Paths = move_regex_borrow_graph::collections::Paths<(), Location>;

#[must_use]
enum Value {
    Ref(Ref),
    NonRef,
}

struct Context {
    graph: Graph,
    local_root: Ref,
    tx_context: Option<Value>,
    gas_coin: Option<Value>,
    objects: Vec<Option<Value>>,
    withdrawals: Vec<Option<Value>>,
    pure: Vec<Option<Value>>,
    receiving: Vec<Option<Value>>,
    results: Vec<Vec<Option<Value>>>,
}

impl Value {
    fn is_ref(&self) -> bool {
        match self {
            Value::Ref(_) => true,
            Value::NonRef => false,
        }
    }

    fn is_non_ref(&self) -> bool {
        match self {
            Value::Ref(_) => false,
            Value::NonRef => true,
        }
    }

    fn to_ref(&self) -> Option<Ref> {
        match self {
            Value::Ref(r) => Some(*r),
            Value::NonRef => None,
        }
    }
}

impl Context {
    fn new<Mode: ExecutionMode>(
        _env: &Env<Mode>,
        ast: &T::Transaction,
    ) -> Result<Self, Mode::Error> {
        let gas_coin = if ast.gas_payment.is_none() {
            None
        } else {
            Some(Value::NonRef)
        };
        let objects = ast.objects.iter().map(|_| Some(Value::NonRef)).collect();
        let withdrawals = ast
            .withdrawals
            .iter()
            .map(|_| Some(Value::NonRef))
            .collect::<Vec<_>>();
        let pure = ast
            .pure
            .iter()
            .map(|_| Some(Value::NonRef))
            .collect::<Vec<_>>();
        let receiving = ast
            .receiving
            .iter()
            .map(|_| Some(Value::NonRef))
            .collect::<Vec<_>>();
        let canonical_reference_capacity = ast
            .commands
            .iter()
            .flat_map(|command| &command.value.result_type)
            .filter(|ty| matches!(&ty, Type::Reference(_, _)))
            .count();
        let (mut graph, _locals) =
            Graph::new::<()>(canonical_reference_capacity, []).map_err(graph_err::<Mode::Error>)?;
        let local_root = graph
            .extend_by_epsilon(
                (),
                std::iter::empty(),
                /* is_mut */ true,
                &mut DummyMeter,
            )
            .map_err(graph_meter_err::<Mode::Error>)?;
        Ok(Self {
            graph,
            local_root,
            tx_context: Some(Value::NonRef),
            gas_coin,
            objects,
            withdrawals,
            pure,
            receiving,
            results: Vec::with_capacity(ast.commands.len()),
        })
    }

    fn location<E: ExecutionErrorTrait>(
        &mut self,
        l: T::Location,
    ) -> Result<&mut Option<Value>, E> {
        Ok(match l {
            T::Location::TxContext => &mut self.tx_context,
            T::Location::GasCoin => &mut self.gas_coin,
            T::Location::ObjectInput(i) => self.objects.safe_get_mut(i as usize)?,
            T::Location::WithdrawalInput(i) => self.withdrawals.safe_get_mut(i as usize)?,
            T::Location::PureInput(i) => self.pure.safe_get_mut(i as usize)?,
            T::Location::ReceivingInput(i) => self.receiving.safe_get_mut(i as usize)?,
            T::Location::Result(i, j) => self
                .results
                .safe_get_mut(i as usize)?
                .safe_get_mut(j as usize)?,
        })
    }

    fn is_mutable<E: ExecutionErrorTrait>(&self, r: Ref) -> Result<bool, E> {
        self.graph.is_mutable(r).map_err(graph_err::<E>)
    }

    fn borrowed_by<E: ExecutionErrorTrait>(&self, r: Ref) -> Result<BTreeMap<Ref, Paths>, E> {
        self.graph
            .borrowed_by(r, &mut DummyMeter)
            .map_err(graph_meter_err::<E>)
    }

    /// Used for checking if a location is borrowed
    /// Used for updating the borrowed marker in Copy, and for correctness of Move
    fn is_location_borrowed<E: ExecutionErrorTrait>(&self, l: T::Location) -> Result<bool, E> {
        let borrowed_by = self.borrowed_by::<E>(self.local_root)?;
        Ok(borrowed_by
            .iter()
            .any(|(_, paths)| paths.iter().any(|path| path.starts_with(&Location(l)))))
    }

    fn release<E: ExecutionErrorTrait>(&mut self, r: Ref) -> Result<(), E> {
        self.graph
            .release(r, &mut DummyMeter)
            .map_err(graph_meter_err::<E>)
    }

    fn extend_by_epsilon<E: ExecutionErrorTrait>(
        &mut self,
        r: Ref,
        is_mut: bool,
    ) -> Result<Ref, E> {
        let new_r = self
            .graph
            .extend_by_epsilon((), std::iter::once(r), is_mut, &mut DummyMeter)
            .map_err(graph_meter_err::<E>)?;
        Ok(new_r)
    }

    fn extend_by_label<E: ExecutionErrorTrait>(
        &mut self,
        r: Ref,
        is_mut: bool,
        extension: T::Location,
    ) -> Result<Ref, E> {
        let new_r = self
            .graph
            .extend_by_label(
                (),
                std::iter::once(r),
                is_mut,
                Location(extension),
                &mut DummyMeter,
            )
            .map_err(graph_meter_err::<E>)?;
        Ok(new_r)
    }

    fn extend_by_dot_star_for_call<E: ExecutionErrorTrait>(
        &mut self,
        sources: &BTreeSet<Ref>,
        mutabilities: Vec<bool>,
    ) -> Result<Vec<Ref>, E> {
        let new_refs = self
            .graph
            .extend_by_dot_star_for_call((), sources, mutabilities, &mut DummyMeter)
            .map_err(graph_meter_err::<E>)?;
        Ok(new_refs)
    }

    // Writable if
    // No imm equal
    // No extensions
    fn is_writable<E: ExecutionErrorTrait>(&self, r: Ref) -> Result<bool, E> {
        debug_assert!(self.is_mutable::<E>(r)?);
        Ok(self
            .borrowed_by::<E>(r)?
            .values()
            .all(|paths| paths.iter().all(|path| path.is_epsilon())))
    }

    // is in reference not able to be used in a call or return
    fn find_non_transferrable<E: ExecutionErrorTrait>(
        &self,
        refs: &BTreeSet<Ref>,
    ) -> Result<Option<Ref>, E> {
        let borrows = refs
            .iter()
            .copied()
            .map(|r| Ok((r, self.borrowed_by::<E>(r)?)))
            .collect::<Result<BTreeMap<_, _>, E>>()?;
        let mut_refs = refs
            .iter()
            .copied()
            .filter_map(|r| match self.is_mutable::<E>(r) {
                Ok(true) => Some(Ok(r)),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<BTreeSet<_>, E>>()?;
        for (r, borrowed_by) in borrows {
            let is_mut = mut_refs.contains(&r);
            for (borrower, paths) in borrowed_by {
                if !is_mut {
                    if mut_refs.contains(&borrower) {
                        // If the ref is imm, but is borrowed by a mut ref in the set
                        // the mut ref is not transferrable
                        // In other words, the mut ref is an extension of the imm ref
                        return Ok(Some(borrower));
                    }
                } else {
                    for path in paths {
                        if !path.is_epsilon() || refs.contains(&borrower) {
                            // If the ref is mut, it cannot have any non-epsilon extensions
                            // If extension is epsilon (an alias), it cannot be in the transfer set
                            return Ok(Some(r));
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}

/// Checks the following
/// - Values are not used after being moved
/// - Reference safety is upheld (no dangling references)
pub fn verify<Mode: ExecutionMode>(
    env: &Env<Mode>,
    ast: &T::Transaction,
) -> Result<(), Mode::Error> {
    let mut context = Context::new(env, ast)?;
    let commands = &ast.commands;
    for c in commands {
        let result = command::<Mode::Error>(&mut context, c)
            .map_err(|e| e.with_command_index(c.idx as usize))?;
        assert_invariant!(
            result.len() == c.value.result_type.len(),
            "result length mismatch for command. {c:?}"
        );
        // drop unused result values
        assert_invariant!(
            result.len() == c.value.drop_values.len(),
            "drop values length mismatch for command. {c:?}"
        );
        let result_values = result
            .into_iter()
            .zip_debug_eq(c.value.drop_values.iter().copied())
            .map(|(v, drop)| {
                Ok(if !drop {
                    Some(v)
                } else {
                    consume_value::<Mode::Error>(&mut context, v)?;
                    None
                })
            })
            .collect::<Result<Vec<_>, Mode::Error>>()?;
        context.results.push(result_values);
    }

    let Context {
        gas_coin,
        objects,
        pure,
        receiving,
        results,
        ..
    } = &mut context;
    let gas_coin = gas_coin.take();
    let objects = std::mem::take(objects);
    let pure = std::mem::take(pure);
    let receiving = std::mem::take(receiving);
    let results = std::mem::take(results);
    consume_value_opt::<Mode::Error>(&mut context, gas_coin)?;
    for vopt in objects.into_iter().chain(pure).chain(receiving) {
        consume_value_opt::<Mode::Error>(&mut context, vopt)?;
    }
    for result in results {
        for vopt in result {
            consume_value_opt::<Mode::Error>(&mut context, vopt)?;
        }
    }

    assert_invariant!(
        context
            .borrowed_by::<Mode::Error>(context.local_root)?
            .is_empty(),
        "reference to local root not released"
    );
    context.release::<Mode::Error>(context.local_root)?;
    assert_invariant!(context.graph.is_empty(), "reference not released");
    assert_invariant!(
        context.tx_context.is_some(),
        "tx_context should never be moved"
    );

    Ok(())
}

fn command<E: ExecutionErrorTrait>(
    context: &mut Context,
    sp!(_, c): &T::Command,
) -> Result<Vec<Value>, E> {
    let result_tys = &c.result_type;
    Ok(match &c.command {
        T::Command__::MoveCall(mc) => {
            let T::MoveCall {
                function,
                arguments: args,
            } = &**mc;
            let arg_values = arguments::<E>(context, args)?;
            call::<E>(context, arg_values, &function.signature)?
        }
        T::Command__::TransferObjects(objects, recipient) => {
            let object_values = arguments::<E>(context, objects)?;
            let recipient_value = argument::<E>(context, recipient)?;
            consume_values::<E>(context, object_values)?;
            consume_value::<E>(context, recipient_value)?;
            vec![]
        }
        T::Command__::SplitCoins(_, coin, amounts) => {
            let coin_value = argument::<E>(context, coin)?;
            let amount_values = arguments::<E>(context, amounts)?;
            consume_values::<E>(context, amount_values)?;
            write_ref::<E>(context, 0, coin_value)?;
            (0..amounts.len()).map(|_| Value::NonRef).collect()
        }
        T::Command__::MergeCoins(_, target, coins) => {
            let target_value = argument::<E>(context, target)?;
            let coin_values = arguments::<E>(context, coins)?;
            consume_values::<E>(context, coin_values)?;
            write_ref::<E>(context, 0, target_value)?;
            vec![]
        }
        T::Command__::MakeMoveVec(_, xs) => {
            let vs = arguments::<E>(context, xs)?;
            consume_values::<E>(context, vs)?;
            vec![Value::NonRef]
        }
        T::Command__::Publish(_, _, _) => result_tys.iter().map(|_| Value::NonRef).collect(),
        T::Command__::Upgrade(_, _, _, x, _) => {
            let v = argument::<E>(context, x)?;
            consume_value::<E>(context, v)?;
            vec![Value::NonRef]
        }
    })
}

//**************************************************************************************************
// Abstract State
//**************************************************************************************************

fn consume_values<E: ExecutionErrorTrait>(
    context: &mut Context,
    values: Vec<Value>,
) -> Result<(), E> {
    for v in values {
        consume_value::<E>(context, v)?;
    }
    Ok(())
}

fn consume_value_opt<E: ExecutionErrorTrait>(
    context: &mut Context,
    value: Option<Value>,
) -> Result<(), E> {
    match value {
        Some(v) => consume_value::<E>(context, v),
        None => Ok(()),
    }
}

fn consume_value<E: ExecutionErrorTrait>(context: &mut Context, value: Value) -> Result<(), E> {
    match value {
        Value::NonRef => Ok(()),
        Value::Ref(r) => {
            context.release::<E>(r)?;
            Ok(())
        }
    }
}

fn arguments<E: ExecutionErrorTrait>(
    context: &mut Context,
    xs: &[T::Argument],
) -> Result<Vec<Value>, E> {
    xs.iter().map(|x| argument::<E>(context, x)).collect()
}

fn argument<E: ExecutionErrorTrait>(context: &mut Context, x: &T::Argument) -> Result<Value, E> {
    match &x.value.0 {
        T::Argument__::Use(T::Usage::Move(location)) => move_value::<E>(context, x.idx, *location),
        T::Argument__::Use(T::Usage::Copy { location, borrowed }) => {
            copy_value::<E>(context, x.idx, *location, borrowed)
        }
        T::Argument__::Borrow(is_mut, location) => {
            borrow_location::<E>(context, x.idx, *is_mut, *location)
        }
        T::Argument__::Read(usage) => read_ref::<E>(context, x.idx, usage),
        T::Argument__::Freeze(usage) => freeze_ref::<E>(context, x.idx, usage),
    }
}

fn move_value<E: ExecutionErrorTrait>(
    context: &mut Context,
    arg_idx: u16,
    l: T::Location,
) -> Result<Value, E> {
    if context.is_location_borrowed::<E>(l)? {
        // TODO more specific error
        return Err(E::from_kind(ExecutionErrorKind::command_argument_error(
            CommandArgumentError::CannotMoveBorrowedValue,
            arg_idx,
        )));
    }
    let Some(value) = context.location::<E>(l)?.take() else {
        return Err(E::from_kind(ExecutionErrorKind::command_argument_error(
            CommandArgumentError::ArgumentWithoutValue,
            arg_idx,
        )));
    };
    Ok(value)
}

fn copy_value<E: ExecutionErrorTrait>(
    context: &mut Context,
    arg_idx: u16,
    l: T::Location,
    borrowed: &OnceCell<bool>,
) -> Result<Value, E> {
    let is_borrowed = context.is_location_borrowed::<E>(l)?;
    borrowed
        .set(is_borrowed)
        .map_err(|_| make_invariant_violation!("Copy's borrowed marker should not yet be set"))?;

    let Some(value) = context.location::<E>(l)? else {
        // TODO more specific error
        return Err(E::from_kind(ExecutionErrorKind::command_argument_error(
            CommandArgumentError::ArgumentWithoutValue,
            arg_idx,
        )));
    };
    Ok(match value {
        Value::Ref(r) => {
            let r = *r;
            let is_mut = context.is_mutable::<E>(r)?;
            let new_r = context.extend_by_epsilon::<E>(r, is_mut)?;
            Value::Ref(new_r)
        }
        Value::NonRef => Value::NonRef,
    })
}

fn borrow_location<E: ExecutionErrorTrait>(
    context: &mut Context,
    arg_idx: u16,
    is_mut: bool,
    l: T::Location,
) -> Result<Value, E> {
    // check that the location has a value
    let Some(value) = context.location::<E>(l)? else {
        // TODO more specific error
        return Err(E::from_kind(ExecutionErrorKind::command_argument_error(
            CommandArgumentError::ArgumentWithoutValue,
            arg_idx,
        )));
    };
    assert_invariant!(
        value.is_non_ref(),
        "type checking should guarantee no borrowing of references"
    );
    let new_r = context.extend_by_label::<E>(context.local_root, is_mut, l)?;
    Ok(Value::Ref(new_r))
}

/// Creates an alias to the reference, but one that is immutable
fn freeze_ref<E: ExecutionErrorTrait>(
    context: &mut Context,
    arg_idx: u16,
    u: &T::Usage,
) -> Result<Value, E> {
    let value = match u {
        T::Usage::Move(l) => move_value::<E>(context, arg_idx, *l)?,
        T::Usage::Copy { location, borrowed } => {
            copy_value::<E>(context, arg_idx, *location, borrowed)?
        }
    };
    let Some(r) = value.to_ref() else {
        invariant_violation!("type checking should guarantee FreezeRef is used on only references")
    };
    let new_r = context.extend_by_epsilon::<E>(r, /* is_mut */ false)?;
    consume_value::<E>(context, value)?;
    Ok(Value::Ref(new_r))
}

fn read_ref<E: ExecutionErrorTrait>(
    context: &mut Context,
    arg_idx: u16,
    u: &T::Usage,
) -> Result<Value, E> {
    let value = match u {
        T::Usage::Move(l) => move_value::<E>(context, arg_idx, *l)?,
        T::Usage::Copy { location, borrowed } => {
            copy_value::<E>(context, arg_idx, *location, borrowed)?
        }
    };
    assert_invariant!(
        value.is_ref(),
        "type checking should guarantee ReadRef is used on only references"
    );
    consume_value::<E>(context, value)?;
    Ok(Value::NonRef)
}

fn write_ref<E: ExecutionErrorTrait>(
    context: &mut Context,
    arg_idx: usize,
    value: Value,
) -> Result<(), E> {
    let Value::Ref(r) = value else {
        invariant_violation!("type checking should guarantee WriteRef is used on only references");
    };

    if !context.is_writable::<E>(r)? {
        // TODO more specific error
        // TODO checked_as!
        #[allow(clippy::cast_possible_truncation)]
        return Err(E::from_kind(ExecutionErrorKind::command_argument_error(
            CommandArgumentError::CannotWriteToExtendedReference,
            arg_idx as u16,
        )));
    }
    consume_value::<E>(context, value)?;
    Ok(())
}

fn call<E: ExecutionErrorTrait>(
    context: &mut Context,
    arg_values: Vec<Value>,
    signature: &T::LoadedFunctionInstantiation,
) -> Result<Vec<Value>, E> {
    let sources = arg_values
        .iter()
        .filter_map(|v| v.to_ref())
        .collect::<BTreeSet<_>>();
    if let Some(v) = context.find_non_transferrable::<E>(&sources)? {
        let mut_idx = arg_values
            .iter()
            .zip_debug_eq(&signature.parameters)
            .enumerate()
            .find(|(_, (x, ty))| x.to_ref() == Some(v) && matches!(ty, Type::Reference(true, _)));

        let Some((idx, _)) = mut_idx else {
            invariant_violation!("non transferrable value was not found in arguments");
        };
        // TODO checked_as!
        #[allow(clippy::cast_possible_truncation)]
        return Err(E::from_kind(ExecutionErrorKind::command_argument_error(
            CommandArgumentError::InvalidReferenceArgument,
            idx as u16,
        )));
    }
    let mutabilities = signature
        .return_
        .iter()
        .filter_map(|ty| match ty {
            Type::Reference(is_mut, _) => Some(*is_mut),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mutabilities_len = mutabilities.len();
    let mut return_references = context.extend_by_dot_star_for_call::<E>(&sources, mutabilities)?;
    assert_invariant!(
        return_references.len() == mutabilities_len,
        "return_references should have the same length as mutabilities"
    );

    let mut return_values: Vec<_> = signature
        .return_
        .iter()
        .rev()
        .map(|ty| {
            Ok(match ty {
                Type::Reference(_is_mut, _) => {
                    let Some(new_ref) = return_references.pop() else {
                        invariant_violation!("return_references has less references than return_");
                    };
                    debug_assert_eq!(context.is_mutable::<E>(new_ref)?, *_is_mut);
                    Value::Ref(new_ref)
                }
                _ => Value::NonRef,
            })
        })
        .collect::<Result<Vec<_>, E>>()?;
    return_values.reverse();
    assert_invariant!(
        return_references.is_empty(),
        "return_references has more references than return_"
    );
    consume_values::<E>(context, arg_values)?;
    Ok(return_values)
}

fn graph_meter_err<E: ExecutionErrorTrait>(e: MeterError<()>) -> E {
    match e {
        MeterError::Meter(()) => {
            make_invariant_violation!("DummyMeter should never produce a Meter error").into()
        }
        MeterError::InvariantViolation(iv) => graph_err::<E>(iv),
    }
}

fn graph_err<E: ExecutionErrorTrait>(e: move_regex_borrow_graph::InvariantViolation) -> E {
    make_invariant_violation!("Borrow graph invariant violation: {}", e.0).into()
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            T::Location::TxContext => write!(f, "TxContext"),
            T::Location::GasCoin => write!(f, "GasCoin"),
            T::Location::ObjectInput(idx) => write!(f, "ObjectInput({idx})"),
            T::Location::WithdrawalInput(idx) => write!(f, "WithdrawalInput({idx})"),
            T::Location::PureInput(idx) => write!(f, "PureInput({idx})"),
            T::Location::ReceivingInput(idx) => write!(f, "ReceivingInput({idx})"),
            T::Location::Result(i, j) => write!(f, "Result({i}, {j})"),
        }
    }
}
