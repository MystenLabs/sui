// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{env::Env, typing::ast as T};
use sui_types::error::ExecutionError;

/// Refines usage of values so that the last `Copy` of a value is a `Move` if it is not borrowed
/// After, it verifies the following
/// - No results without `drop` are unused (all unused non-input values have `drop`)
pub fn refine_and_verify(env: &Env, ast: &mut T::Transaction) -> Result<(), ExecutionError> {
    refine::transaction(ast);
    verify::transaction(env, ast)?;
    Ok(())
}

mod refine {
    use crate::{
        sp,
        static_programmable_transactions::typing::ast::{self as T},
    };
    use std::collections::BTreeSet;

    /// After memory safety, we can switch the last usage of a `Copy` to a `Move` if it is not
    /// borrowed at the time of the last usage.
    pub fn transaction(ast: &mut T::Transaction) {
        let mut used: BTreeSet<T::Location> = BTreeSet::new();
        for (c, _tys) in ast.commands.iter_mut().rev() {
            command(&mut used, c);
        }
    }

    fn command(used: &mut BTreeSet<T::Location>, sp!(_, command): &mut T::Command) {
        match command {
            T::Command_::MoveCall(mc) => arguments(used, &mut mc.arguments),
            T::Command_::TransferObjects(objects, recipient) => {
                argument(used, recipient);
                arguments(used, objects);
            }
            T::Command_::SplitCoins(_, coin, amounts) => {
                arguments(used, amounts);
                argument(used, coin);
            }
            T::Command_::MergeCoins(_, target, coins) => {
                arguments(used, coins);
                argument(used, target);
            }
            T::Command_::MakeMoveVec(_, xs) => arguments(used, xs),
            T::Command_::Publish(_, _, _) => (),
            T::Command_::Upgrade(_, _, _, x, _) => argument(used, x),
        }
    }

    fn arguments(used: &mut BTreeSet<T::Location>, args: &mut [T::Argument]) {
        for arg in args.iter_mut().rev() {
            argument(used, arg)
        }
    }

    fn argument(used: &mut BTreeSet<T::Location>, arg: &mut T::Argument) {
        let usage = match &mut arg.value.0 {
            T::Argument__::Use(u) | T::Argument__::Read(u) => u,
            T::Argument__::Borrow(_, _) => return,
        };
        match &usage {
            T::Usage::Move(loc) => {
                used.insert(*loc);
            }
            T::Usage::Copy { location, borrowed } => {
                // we are at the last usage of a reference result if it was not yet added to the set
                let location = *location;
                let last_usage = used.insert(location);
                if last_usage && !borrowed.get().unwrap() {
                    // if it was the last usage, we need to change the Copy to a Move
                    *usage = T::Usage::Move(location);
                }
            }
        }
    }
}

mod verify {
    use crate::{
        sp,
        static_programmable_transactions::{
            env::Env,
            typing::ast::{self as T, Type},
        },
    };
    use sui_types::error::{ExecutionError, ExecutionErrorKind};

    #[must_use]
    struct Value;

    struct Context {
        gas_coin: Option<Value>,
        inputs: Vec<Option<Value>>,
        results: Vec<Vec<Option<Value>>>,
    }

    impl Context {
        fn new(ast: &T::Transaction) -> Result<Self, ExecutionError> {
            let inputs = ast.inputs.iter().map(|_| Some(Value)).collect();
            Ok(Self {
                gas_coin: Some(Value),
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
    }

    /// Checks the following
    /// - All unused result values have `drop`
    pub fn transaction(_env: &Env, ast: &T::Transaction) -> Result<(), ExecutionError> {
        let mut context = Context::new(ast)?;
        let commands = &ast.commands;
        for (c, t) in commands {
            let result =
                command(&mut context, c, t).map_err(|e| e.with_command_index(c.idx as usize))?;
            assert_invariant!(result.len() == t.len(), "result length mismatch");
            context.results.push(result.into_iter().map(Some).collect());
        }

        let Context {
            gas_coin,
            inputs,
            results,
        } = context;
        consume_value_opt(gas_coin);
        // TODO do we want to check inputs in the dev inspect case?
        consume_value_opts(inputs);
        assert_invariant!(results.len() == commands.len(), "result length mismatch");
        for (i, (result, (_, tys))) in results.into_iter().zip(&ast.commands).enumerate() {
            assert_invariant!(result.len() == tys.len(), "result length mismatch");
            for (j, (vopt, ty)) in result.into_iter().zip(tys).enumerate() {
                drop_value_opt((i, j), vopt, ty)?;
            }
        }
        Ok(())
    }

    fn command(
        context: &mut Context,
        sp!(_, command): &T::Command,
        result_tys: &[Type],
    ) -> Result<Vec<Value>, ExecutionError> {
        Ok(match command {
            T::Command_::MoveCall(mc) => {
                let T::MoveCall {
                    function,
                    arguments: args,
                } = &**mc;
                let return_ = &function.signature.return_;
                let arg_values = arguments(context, args)?;
                consume_values(arg_values);
                (0..return_.len()).map(|_| Value).collect()
            }
            T::Command_::TransferObjects(objects, recipient) => {
                let object_values = arguments(context, objects)?;
                let recipient_value = argument(context, recipient)?;
                consume_values(object_values);
                consume_value(recipient_value);
                vec![]
            }
            T::Command_::SplitCoins(_, coin, amounts) => {
                let coin_value = argument(context, coin)?;
                let amount_values = arguments(context, amounts)?;
                consume_values(amount_values);
                consume_value(coin_value);
                (0..amounts.len()).map(|_| Value).collect()
            }
            T::Command_::MergeCoins(_, target, coins) => {
                let target_value = argument(context, target)?;
                let coin_values = arguments(context, coins)?;
                consume_values(coin_values);
                consume_value(target_value);
                vec![]
            }
            T::Command_::MakeMoveVec(_, xs) => {
                let vs = arguments(context, xs)?;
                consume_values(vs);
                vec![Value]
            }
            T::Command_::Publish(_, _, _) => result_tys.iter().map(|_| Value).collect(),
            T::Command_::Upgrade(_, _, _, x, _) => {
                let v = argument(context, x)?;
                consume_value(v);
                vec![Value]
            }
        })
    }

    fn consume_values(_: Vec<Value>) {}

    fn consume_value(_: Value) {}

    fn consume_value_opts(_: Vec<Option<Value>>) {}

    fn consume_value_opt(_: Option<Value>) {}

    fn drop_value_opt(
        idx: (usize, usize),
        value: Option<Value>,
        ty: &Type,
    ) -> Result<(), ExecutionError> {
        match value {
            Some(v) => drop_value(idx, v, ty),
            None => Ok(()),
        }
    }

    fn drop_value((i, j): (usize, usize), value: Value, ty: &Type) -> Result<(), ExecutionError> {
        if !ty.abilities().has_drop() {
            return Err(ExecutionErrorKind::UnusedValueWithoutDrop {
                result_idx: i as u16,
                secondary_idx: j as u16,
            }
            .into());
        }
        consume_value(value);
        Ok(())
    }

    fn arguments(context: &mut Context, xs: &[T::Argument]) -> Result<Vec<Value>, ExecutionError> {
        xs.iter().map(|x| argument(context, x)).collect()
    }

    fn argument(context: &mut Context, sp!(_, x): &T::Argument) -> Result<Value, ExecutionError> {
        match &x.0 {
            T::Argument__::Use(T::Usage::Move(location)) => move_value(context, *location),
            T::Argument__::Use(T::Usage::Copy { location, .. }) => copy_value(context, *location),
            T::Argument__::Borrow(_, location) => borrow_location(context, *location),
            T::Argument__::Read(usage) => read_ref(context, usage),
        }
    }

    fn move_value(context: &mut Context, l: T::Location) -> Result<Value, ExecutionError> {
        let Some(value) = context.location(l).take() else {
            invariant_violation!("memory safety should have failed")
        };
        Ok(value)
    }

    fn copy_value(context: &mut Context, l: T::Location) -> Result<Value, ExecutionError> {
        assert_invariant!(
            context.location(l).is_some(),
            "memory safety should have failed"
        );
        Ok(Value)
    }

    fn borrow_location(context: &mut Context, l: T::Location) -> Result<Value, ExecutionError> {
        assert_invariant!(
            context.location(l).is_some(),
            "memory safety should have failed"
        );
        Ok(Value)
    }

    fn read_ref(context: &mut Context, u: &T::Usage) -> Result<Value, ExecutionError> {
        let value = match u {
            T::Usage::Move(l) => move_value(context, *l)?,
            T::Usage::Copy { location, .. } => copy_value(context, *location)?,
        };
        consume_value(value);
        Ok(Value)
    }
}
