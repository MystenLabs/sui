// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::borrow;

use crate::static_programmable_transactions::{
    env::Env,
    typing::ast::{self as T, Type},
};
use move_regex_borrow_graph::references::Ref;
use sui_types::error::ExecutionError;

type Graph = move_regex_borrow_graph::collections::Graph<(), T::Location>;

enum Value {
    Ref(Ref),
    NonRef,
}

struct Context {
    graph: Graph,
    local_root: Ref,
    inputs: Vec<Option<Value>>,
    results: Vec<Vec<Option<Value>>>,
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
            inputs,
            results: vec![],
        })
    }
}

pub fn verify(_env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    let T::Transaction { inputs, commands } = txn;
    let mut context = Context::new(inputs)?;
    for (c, _t) in commands {
        let result = command(&mut context, c)?;
        context.results.push(result.into_iter().map(Some).collect());
    }
    todo!()
}

fn command(context: &mut Context, command: &T::Command) -> Result<Vec<Value>, ExecutionError> {
    Ok(match command {
        T::Command::MoveCall(mc) => todo!(),
        T::Command::TransferObjects(objects, recipient) => {
            let object_values = arguments(context, objects)?;
            let recipient_value = argument(context, recipient)?;
            consume_values(context, object_values)?;
            consume_value(context, recipient_value)?;
            vec![]
        }
        T::Command::SplitCoins(_, coin, amounts) => {
            let amount_values = arguments(context, amounts)?;
            let coin_value = argument(context, coin)?;
            consume_values(context, amount_values)?;
            write_ref(context, coin_value)?;
            vec![Value::NonRef; amounts.len()]
        }
        T::Command::MergeCoins(_, target, coins) => {
            let coin_values = arguments(context, coins)?;
            let target_value = argument(context, target)?;
            consume_values(context, coin_values)?;
            write_ref(context, target_value)?;
            vec![Value::NonRef]
        }
        T::Command::MakeMoveVec(_, xs) => {
            let vs = arguments(context, xs)?;
            consume_values(context, vs)?;
            vec![Value::NonRef]
        }
        T::Command::Publish(_, _) => {
            vec![]
        }
        T::Command::Upgrade(_, _, _, x) => {
            let v = argument(context, x)?;
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

fn consume_value(context: &mut Context, value: Value) -> Result<(), ExecutionError> {
    match value {
        Value::NonRef => Ok(()),
        Value::Ref(r) => {
            debug_assert!(
                false,
                "consume value should not be used for reference values"
            );
            context.graph.release(r).map_err(graph_err)?;
            Ok(())
        }
    }
}

fn arguments(context: &mut Context, xs: &[T::Argument]) -> Result<Vec<Value>, ExecutionError> {
    xs.iter().map(|x| argument(context, x)).collect()
}

fn argument(context: &mut Context, x: &T::Argument) -> Result<Value, ExecutionError> {
    match x {
        T::Argument::Move(location) => move_value(context, location),
        T::Argument::Copy(location) => copy_value(context, location),
        T::Argument::Borrow(_, location) => borrow_location(context, location),
        T::Argument::Read(location) => read_location(context, location),
    }
}

fn move_value(context: &mut Context, location: &T::Location) -> Result<Value, ExecutionError> {
    todo!()
}

fn copy_value(context: &mut Context, location: &T::Location) -> Result<Value, ExecutionError> {
    todo!()
}

fn borrow_location(context: &mut Context, location: &T::Location) -> Result<Value, ExecutionError> {
    todo!()
}

fn read_location(context: &mut Context, location: &T::Location) -> Result<Value, ExecutionError> {
    todo!()
}

fn write_ref(context: &mut Context, value: Value) -> Result<(), ExecutionError> {
    todo!()
}

fn graph_err(e: move_regex_borrow_graph::InvariantViolation) -> ExecutionError {
    ExecutionError::invariant_violation(format!("Borrow graph invariant violation: {}", e.0))
}
