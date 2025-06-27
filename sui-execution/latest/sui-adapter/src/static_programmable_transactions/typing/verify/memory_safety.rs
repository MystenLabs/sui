// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
};

use crate::{
    sp,
    static_programmable_transactions::{
        env::Env,
        typing::ast::{self as T, Type},
    },
};
use move_regex_borrow_graph::references::Ref;
use sui_types::{
    error::{ExecutionError, command_argument_error},
    execution_status::CommandArgumentError,
};

type Graph = move_regex_borrow_graph::collections::Graph<(), T::Location>;
type Paths = move_regex_borrow_graph::collections::Paths<(), T::Location>;

#[must_use]
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
    fn new(ast: &T::Transaction) -> Result<Self, ExecutionError> {
        let inputs = ast
            .inputs
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
            results: Vec::with_capacity(ast.commands.len()),
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

    /// Used for checking if a location is borrowed, primarily for recording this information on
    /// a specific Copy instance
    fn is_location_borrowed(&self, l: T::Location) -> Result<bool, ExecutionError> {
        let borrowed_by = self.borrowed_by(self.local_root)?;
        Ok(borrowed_by
            .iter()
            .any(|(_, paths)| paths.iter().any(|path| path.starts_with(&l))))
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

/// Checks the following
/// - Values are not used after being moved
/// - Reference safety is upheld (no dangling references)
pub fn verify(_env: &Env, ast: &T::Transaction) -> Result<(), ExecutionError> {
    let mut context = Context::new(ast)?;
    let commands = &ast.commands;
    for (c, t) in commands {
        let result =
            command(&mut context, c, t).map_err(|e| e.with_command_index(c.idx as usize))?;
        assert_invariant!(
            result.len() == t.len(),
            "result length mismatch for command. {c:?}"
        );
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
    for result in results {
        for vopt in result {
            consume_value_opt(&mut context, vopt)?;
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

fn command(
    context: &mut Context,
    sp!(_, command): &T::Command,
    result_tys: &[T::Type],
) -> Result<Vec<Value>, ExecutionError> {
    Ok(match command {
        T::Command_::MoveCall(mc) => {
            let T::MoveCall {
                function,
                arguments: args,
            } = &**mc;
            let return_ = &function.signature.return_;
            let arg_values = arguments(context, args)?;
            call(context, arg_values, return_)?
        }
        T::Command_::TransferObjects(objects, recipient) => {
            let object_values = arguments(context, objects)?;
            let recipient_value = argument(context, recipient)?;
            consume_values(context, object_values)?;
            consume_value(context, recipient_value)?;
            vec![]
        }
        T::Command_::SplitCoins(_, coin, amounts) => {
            let coin_value = argument(context, coin)?;
            let amount_values = arguments(context, amounts)?;
            consume_values(context, amount_values)?;
            write_ref(context, 0, coin_value)?;
            (0..amounts.len()).map(|_| Value::NonRef).collect()
        }
        T::Command_::MergeCoins(_, target, coins) => {
            let target_value = argument(context, target)?;
            let coin_values = arguments(context, coins)?;
            consume_values(context, coin_values)?;
            write_ref(context, 0, target_value)?;
            vec![]
        }
        T::Command_::MakeMoveVec(_, xs) => {
            let vs = arguments(context, xs)?;
            consume_values(context, vs)?;
            vec![Value::NonRef]
        }
        T::Command_::Publish(_, _, _) => result_tys.iter().map(|_| Value::NonRef).collect(),
        T::Command_::Upgrade(_, _, _, x, _) => {
            let v = argument(context, x)?;
            consume_value(context, v)?;
            vec![Value::NonRef]
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

fn arguments(context: &mut Context, xs: &[T::Argument]) -> Result<Vec<Value>, ExecutionError> {
    xs.iter().map(|x| argument(context, x)).collect()
}

fn argument(context: &mut Context, x: &T::Argument) -> Result<Value, ExecutionError> {
    match &x.value.0 {
        T::Argument__::Use(T::Usage::Move(location)) => move_value(context, x.idx, *location),
        T::Argument__::Use(T::Usage::Copy { location, borrowed }) => {
            copy_value(context, x.idx, *location, borrowed)
        }
        T::Argument__::Borrow(is_mut, location) => {
            borrow_location(context, x.idx, *is_mut, *location)
        }
        T::Argument__::Read(usage) => read_ref(context, x.idx, usage),
    }
}

fn move_value(
    context: &mut Context,
    arg_idx: u16,
    l: T::Location,
) -> Result<Value, ExecutionError> {
    let Some(value) = context.location(l).take() else {
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            arg_idx as usize,
        ));
    };
    Ok(value)
}

fn copy_value(
    context: &mut Context,
    arg_idx: u16,
    l: T::Location,
    borrowed: &OnceCell<bool>,
) -> Result<Value, ExecutionError> {
    let is_borrowed = context.is_location_borrowed(l)?;
    borrowed
        .set(is_borrowed)
        .map_err(|_| make_invariant_violation!("Copy's borrowed marker should not yet be set"))?;

    let Some(value) = context.location(l) else {
        // TODO more specific error
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            arg_idx as usize,
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
    arg_idx: u16,
    is_mut: bool,
    l: T::Location,
) -> Result<Value, ExecutionError> {
    // check that the location has a value
    let Some(value) = context.location(l) else {
        // TODO more specific error
        return Err(command_argument_error(
            CommandArgumentError::InvalidValueUsage,
            arg_idx as usize,
        ));
    };
    assert_invariant!(
        value.is_non_ref(),
        "type checking should guarantee no borrowing of references"
    );
    let new_r = context.extend_by_label(context.local_root, is_mut, l)?;
    Ok(Value::Ref(new_r))
}

fn read_ref(context: &mut Context, arg_idx: u16, u: &T::Usage) -> Result<Value, ExecutionError> {
    let value = match u {
        T::Usage::Move(l) => move_value(context, arg_idx, *l)?,
        T::Usage::Copy { location, borrowed } => copy_value(context, arg_idx, *location, borrowed)?,
    };
    assert_invariant!(
        value.is_ref(),
        "type checking should guarantee ReadRef is used on only references"
    );
    consume_value(context, value)?;
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
