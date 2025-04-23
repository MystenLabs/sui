// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use crate::static_programmable_transactions::{
    env::Env,
    typing::ast::{self as T, Type},
};
use move_regex_borrow_graph::references::Ref;
use sui_types::{
    error::{command_argument_error, ExecutionError, ExecutionErrorKind},
    execution_status::CommandArgumentError,
};

type Graph = move_regex_borrow_graph::collections::Graph<(), T::Location>;
type Paths = move_regex_borrow_graph::collections::Paths<(), T::Location>;

enum Value {
    Ref(Ref),
    NonRef,
}

struct Context {
    graph: Graph,
    local_root: Ref,
    gas_coin: Option<Value>,
    inputs: Vec<Option<Value>>,
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
    fn new(inputs: &T::Inputs) -> Result<Self, ExecutionError> {
        let inputs = inputs
            .iter()
            .map(|(_, ty)| {
                Some(match ty {
                    T::InputType::Bytes(_) => Value::NonRef,
                    T::InputType::Fixed(ty) => {
                        debug_assert!(!matches!(ty, Type::Reference(_, _)));
                        Value::NonRef
                    }
                })
            })
            .collect();
        let (mut graph, _locals) = Graph::new::<()>([]).map_err(graph_err)?;
        let local_root = graph
            .extend_by_epsilon((), std::iter::empty(), /* is_mut */ true)
            .map_err(graph_err)?;
        Ok(Self {
            graph,
            local_root,
            gas_coin: Some(Value::NonRef),
            inputs,
            results: vec![],
        })
    }

    fn location(&mut self, l: T::Location) -> &mut Option<Value> {
        match l {
            T::Location::GasCoin => &mut self.gas_coin,
            T::Location::Input(i) => &mut self.inputs[i as usize],
            T::Location::Result(i, j) => &mut self.results[i as usize][j as usize],
        }
    }

    fn is_mutable(&self, r: Ref) -> Result<bool, ExecutionError> {
        self.graph.is_mutable(r).map_err(graph_err)
    }

    fn borrowed_by(&self, r: Ref) -> Result<BTreeMap<Ref, Paths>, ExecutionError> {
        self.graph.borrowed_by(r).map_err(graph_err)
    }

    fn release(&mut self, r: Ref) -> Result<(), ExecutionError> {
        self.graph.release(r).map_err(graph_err)
    }

    fn extend_by_epsilon(&mut self, r: Ref, is_mut: bool) -> Result<Ref, ExecutionError> {
        let new_r = self
            .graph
            .extend_by_epsilon((), std::iter::once(r), is_mut)
            .map_err(graph_err)?;
        Ok(new_r)
    }

    fn extend_by_label(
        &mut self,
        r: Ref,
        is_mut: bool,
        extension: T::Location,
    ) -> Result<Ref, ExecutionError> {
        let new_r = self
            .graph
            .extend_by_label((), std::iter::once(r), is_mut, extension)
            .map_err(graph_err)?;
        Ok(new_r)
    }

    fn extend_by_dot_star_for_call(
        &mut self,
        sources: &BTreeSet<Ref>,
        mutabilities: Vec<bool>,
    ) -> Result<Vec<Ref>, ExecutionError> {
        let new_refs = self
            .graph
            .extend_by_dot_star_for_call((), sources.iter().copied(), mutabilities)
            .map_err(graph_err)?;
        Ok(new_refs)
    }

    // Writable if
    // No imm equal
    // No extensions
    fn is_writable(&self, r: Ref) -> Result<bool, ExecutionError> {
        debug_assert!(self.is_mutable(r)?);
        Ok(self
            .borrowed_by(r)?
            .values()
            .all(|paths| paths.iter().all(|path| path.is_epsilon())))
    }

    // is in reference not able to be used in a call or return
    fn find_non_transferrable(&self, refs: &BTreeSet<Ref>) -> Result<Option<Ref>, ExecutionError> {
        let borrows = refs
            .iter()
            .copied()
            .map(|r| Ok((r, self.borrowed_by(r)?)))
            .collect::<Result<BTreeMap<_, _>, ExecutionError>>()?;
        let mut_refs = refs
            .iter()
            .copied()
            .filter_map(|r| match self.is_mutable(r) {
                Ok(true) => Some(Ok(r)),
                Ok(false) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<BTreeSet<_>, ExecutionError>>()?;
        for (r, borrowed_by) in borrows {
            let is_mut = mut_refs.contains(&r);
            for (borrower, paths) in borrowed_by {
                if !is_mut {
                    if mut_refs.contains(&borrower) {
                        // If the ref is imm, but is borrowed by a mut ref in the set
                        // the mut ref is not transferrable
                        // In other words, the mut ref is an extension of the imm ref
                        return Ok(Some(r));
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

pub fn verify(_env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    let T::Transaction { inputs, commands } = txn;
    let mut context = Context::new(inputs)?;
    for (i, (c, _t)) in commands.iter().enumerate() {
        let result = command(&mut context, c).map_err(|e| e.with_command_index(i))?;
        assert_invariant!(result.len() == _t.len(), "result length mismatch");
        context.results.push(result.into_iter().map(Some).collect());
    }

    let Context {
        gas_coin,
        inputs,
        results,
        ..
    } = &mut context;
    let gas_coin = gas_coin.take();
    let inputs = std::mem::take(inputs);
    let results = std::mem::take(results);
    consume_value_opt(&mut context, gas_coin)?;
    for vopt in inputs {
        consume_value_opt(&mut context, vopt)?;
    }
    assert_invariant!(commands.len() == results.len(), "command length mismatch");
    for (i, (result, (_, tys))) in results.into_iter().zip(commands).enumerate() {
        assert_invariant!(result.len() == tys.len(), "result length mismatch");
        for (j, (vopt, ty)) in result.into_iter().zip(tys).enumerate() {
            drop_value_opt(&mut context, (i, j), vopt, ty).unwrap();
        }
    }

    assert_invariant!(
        context.borrowed_by(context.local_root)?.is_empty(),
        "reference to local root not released"
    );
    context.release(context.local_root)?;
    assert_invariant!(context.graph.abstract_size() == 0, "reference not released");

    Ok(())
}

fn command(context: &mut Context, command: &T::Command) -> Result<Vec<Value>, ExecutionError> {
    Ok(match command {
        T::Command::MoveCall(mc) => {
            let T::MoveCall {
                function,
                arguments: args,
            } = &**mc;
            let return_ = &function.signature.return_;
            let arg_values = arguments(context, 0, args)?;
            call(context, arg_values, return_)?
        }
        T::Command::TransferObjects(objects, recipient) => {
            let object_values = arguments(context, 0, objects)?;
            let recipient_value = argument(context, objects.len(), recipient)?;
            consume_values(context, object_values)?;
            consume_value(context, recipient_value)?;
            vec![]
        }
        T::Command::SplitCoins(_, coin, amounts) => {
            let coin_value = argument(context, 0, coin)?;
            let amount_values = arguments(context, 1, amounts)?;
            consume_values(context, amount_values)?;
            write_ref(context, 0, coin_value)?;
            (0..amounts.len()).map(|_| Value::NonRef).collect()
        }
        T::Command::MergeCoins(_, target, coins) => {
            let target_value = argument(context, 0, target)?;
            let coin_values = arguments(context, 1, coins)?;
            consume_values(context, coin_values)?;
            write_ref(context, 0, target_value)?;
            vec![Value::NonRef]
        }
        T::Command::MakeMoveVec(_, xs) => {
            let vs = arguments(context, 0, xs)?;
            consume_values(context, vs)?;
            vec![Value::NonRef]
        }
        T::Command::Publish(_, _) => {
            vec![]
        }
        T::Command::Upgrade(_, _, _, x) => {
            let v = argument(context, 0, x)?;
            consume_value(context, v)?;
            vec![]
        }
    })
}

//**************************************************************************************************
// Abstract State
//**************************************************************************************************

fn consume_values(context: &mut Context, values: Vec<Value>) -> Result<(), ExecutionError> {
    for v in values {
        consume_value(context, v)?;
    }
    Ok(())
}

fn consume_value_opt(context: &mut Context, value: Option<Value>) -> Result<(), ExecutionError> {
    match value {
        Some(v) => consume_value(context, v),
        None => Ok(()),
    }
}

fn consume_value(context: &mut Context, value: Value) -> Result<(), ExecutionError> {
    match value {
        Value::NonRef => Ok(()),
        Value::Ref(r) => {
            context.release(r)?;
            Ok(())
        }
    }
}

fn drop_value_opt(
    context: &mut Context,
    idx: (usize, usize),
    value: Option<Value>,
    ty: &Type,
) -> Result<(), ExecutionError> {
    match value {
        Some(v) => drop_value(context, idx, v, ty),
        None => Ok(()),
    }
}

fn drop_value(
    context: &mut Context,
    (i, j): (usize, usize),
    value: Value,
    ty: &Type,
) -> Result<(), ExecutionError> {
    if !ty.abilities().has_drop() {
        return Err(ExecutionErrorKind::UnusedValueWithoutDrop {
            result_idx: i as u16,
            secondary_idx: j as u16,
        }
        .into());
    }
    consume_value(context, value)?;
    Ok(())
}

fn arguments(
    context: &mut Context,
    start: usize,
    xs: &[T::Argument],
) -> Result<Vec<Value>, ExecutionError> {
    xs.iter()
        .enumerate()
        .map(|(i, x)| argument(context, start + i, x))
        .collect()
}

fn argument(context: &mut Context, idx: usize, x: &T::Argument) -> Result<Value, ExecutionError> {
    match x {
        T::Argument::Move(location) => move_value(context, idx, *location),
        T::Argument::Copy(location) => copy_value(context, idx, *location),
        T::Argument::Borrow(is_mut, location) => borrow_location(context, idx, *is_mut, *location),
        T::Argument::Read(location) => read_ref(context, idx, *location),
    }
}

fn move_value(
    context: &mut Context,
    arg_idx: usize,
    l: T::Location,
) -> Result<Value, ExecutionError> {
    let Some(value) = context.location(l).take() else {
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            arg_idx,
        ));
    };
    Ok(value)
}

fn copy_value(
    context: &mut Context,
    arg_idx: usize,
    l: T::Location,
) -> Result<Value, ExecutionError> {
    let Some(value) = context.location(l) else {
        // TODO more specific error
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            arg_idx,
        ));
    };
    Ok(match value {
        Value::Ref(r) => {
            let r = *r;
            let is_mut = context.is_mutable(r)?;
            let new_r = context.extend_by_epsilon(r, is_mut)?;
            Value::Ref(new_r)
        }
        Value::NonRef => Value::NonRef,
    })
}

fn borrow_location(
    context: &mut Context,
    arg_idx: usize,
    is_mut: bool,
    l: T::Location,
) -> Result<Value, ExecutionError> {
    let Some(value) = context.location(l) else {
        // TODO more specific error
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            arg_idx,
        ));
    };
    assert_invariant!(
        value.is_non_ref(),
        "type checking should guarantee no borrowing of references"
    );
    let new_r = context.extend_by_label(context.local_root, is_mut, l)?;
    Ok(Value::Ref(new_r))
}

fn read_ref(
    context: &mut Context,
    arg_idx: usize,
    l: T::Location,
) -> Result<Value, ExecutionError> {
    let Some(value) = context.location(l) else {
        // TODO more specific error
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            arg_idx,
        ));
    };

    assert_invariant!(
        value.is_ref(),
        "type checking should guarantee ReadRef is used on only references"
    );
    Ok(Value::NonRef)
}

fn write_ref(context: &mut Context, arg_idx: usize, value: Value) -> Result<(), ExecutionError> {
    let Value::Ref(r) = value else {
        invariant_violation!("type checking should guarantee WriteRef is used on only references");
    };

    if !context.is_writable(r)? {
        // TODO more specific error
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            arg_idx,
        ));
    }
    consume_value(context, value)?;
    Ok(())
}

fn call(
    context: &mut Context,
    arg_values: Vec<Value>,
    return_: &[Type],
) -> Result<Vec<Value>, ExecutionError> {
    let sources = arg_values
        .iter()
        .filter_map(|v| v.to_ref())
        .collect::<BTreeSet<_>>();
    if let Some(v) = context.find_non_transferrable(&sources)? {
        let idx = arg_values
            .iter()
            .position(|x| x.to_ref() == Some(v))
            .unwrap_or(arg_values.len());
        assert_invariant!(
            idx < arg_values.len(),
            "non transferrable value was not found in arguments"
        );
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            idx,
        ));
    }
    let mutabilities = return_
        .iter()
        .filter_map(|ty| match ty {
            Type::Reference(is_mut, _) => Some(*is_mut),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mutabilities_len = mutabilities.len();
    let mut return_references = context.extend_by_dot_star_for_call(&sources, mutabilities)?;
    assert_invariant!(
        return_references.len() == mutabilities_len,
        "return_references should have the same length as mutabilities"
    );

    let mut return_values: Vec<_> = return_
        .iter()
        .rev()
        .map(|ty| {
            Ok(match ty {
                Type::Reference(_is_mut, _) => {
                    let Some(new_ref) = return_references.pop() else {
                        invariant_violation!("return_references has less references than return_");
                    };
                    debug_assert_eq!(context.is_mutable(new_ref)?, *_is_mut);
                    Value::Ref(new_ref)
                }
                _ => Value::NonRef,
            })
        })
        .collect::<Result<Vec<_>, ExecutionError>>()?;
    return_values.reverse();
    assert_invariant!(
        return_references.is_empty(),
        "return_references has more references than return_"
    );
    consume_values(context, arg_values)?;
    Ok(return_values)
}

fn graph_err(e: move_regex_borrow_graph::InvariantViolation) -> ExecutionError {
    ExecutionError::invariant_violation(format!("Borrow graph invariant violation: {}", e.0))
}
