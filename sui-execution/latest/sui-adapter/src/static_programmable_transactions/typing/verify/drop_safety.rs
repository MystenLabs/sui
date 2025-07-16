// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    static_programmable_transactions::{env::Env, typing::ast as T},
};
use sui_types::error::ExecutionError;

/// Refines usage of values so that the last `Copy` of a value is a `Move` if it is not borrowed
/// After, it verifies the following
/// - No results without `drop` are unused (all unused non-input values have `drop`)
pub fn refine_and_verify<Mode: ExecutionMode>(
    env: &Env,
    ast: &mut T::Transaction,
) -> Result<(), ExecutionError> {
    refine::transaction(ast);
    verify::transaction::<Mode>(env, ast)?;
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
        for c in ast.commands.iter_mut().rev() {
            command(&mut used, c);
        }
    }

    fn command(used: &mut BTreeSet<T::Location>, sp!(_, c): &mut T::Command) {
        match &mut c.command {
            T::Command__::MoveCall(mc) => arguments(used, &mut mc.arguments),
            T::Command__::TransferObjects(objects, recipient) => {
                argument(used, recipient);
                arguments(used, objects);
            }
            T::Command__::SplitCoins(_, coin, amounts) => {
                arguments(used, amounts);
                argument(used, coin);
            }
            T::Command__::MergeCoins(_, target, coins) => {
                arguments(used, coins);
                argument(used, target);
            }
            T::Command__::MakeMoveVec(_, xs) => arguments(used, xs),
            T::Command__::Publish(_, _, _) => (),
            T::Command__::Upgrade(_, _, _, x, _) => argument(used, x),
        }
    }

    fn arguments(used: &mut BTreeSet<T::Location>, args: &mut [T::Argument]) {
        for arg in args.iter_mut().rev() {
            argument(used, arg)
        }
    }

    fn argument(used: &mut BTreeSet<T::Location>, arg: &mut T::Argument) {
        let usage = match &mut arg.value.0 {
            T::Argument__::Use(u) | T::Argument__::Read(u) | T::Argument__::Freeze(u) => u,
            T::Argument__::Borrow(_, loc) => {
                // mark location as used
                used.insert(*loc);
                return;
            }
        };
        match &usage {
            T::Usage::Move(loc) => {
                // mark location as used
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
        execution_mode::ExecutionMode,
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
        tx_context: Option<Value>,
        gas_coin: Option<Value>,
        objects: Vec<Option<Value>>,
        pure: Vec<Option<Value>>,
        receiving: Vec<Option<Value>>,
        results: Vec<Vec<Option<Value>>>,
    }

    impl Context {
        fn new(ast: &T::Transaction) -> Result<Self, ExecutionError> {
            let objects = ast.objects.iter().map(|_| Some(Value)).collect::<Vec<_>>();
            let pure = ast.pure.iter().map(|_| Some(Value)).collect::<Vec<_>>();
            let receiving = ast
                .receiving
                .iter()
                .map(|_| Some(Value))
                .collect::<Vec<_>>();
            Ok(Self {
                tx_context: Some(Value),
                gas_coin: Some(Value),
                objects,
                pure,
                receiving,
                results: Vec::with_capacity(ast.commands.len()),
            })
        }

        fn location(&mut self, l: T::Location) -> &mut Option<Value> {
            match l {
                T::Location::TxContext => &mut self.tx_context,
                T::Location::GasCoin => &mut self.gas_coin,
                T::Location::ObjectInput(i) => &mut self.objects[i as usize],
                T::Location::PureInput(i) => &mut self.pure[i as usize],
                T::Location::ReceivingInput(i) => &mut self.receiving[i as usize],
                T::Location::Result(i, j) => &mut self.results[i as usize][j as usize],
            }
        }
    }

    /// Checks the following
    /// - All unused result values have `drop`
    pub fn transaction<Mode: ExecutionMode>(
        _env: &Env,
        ast: &T::Transaction,
    ) -> Result<(), ExecutionError> {
        let mut context = Context::new(ast)?;
        let commands = &ast.commands;
        for c in commands {
            let result =
                command(&mut context, c).map_err(|e| e.with_command_index(c.idx as usize))?;
            assert_invariant!(
                result.len() == c.value.result_type.len(),
                "result length mismatch"
            );
            // drop unused result values
            assert_invariant!(
                result.len() == c.value.drop_values.len(),
                "drop values length mismatch"
            );
            let result_values = result
                .into_iter()
                .zip(c.value.drop_values.iter().copied())
                .map(|(v, drop)| {
                    if !drop {
                        Some(v)
                    } else {
                        consume_value(v);
                        None
                    }
                })
                .collect();
            context.results.push(result_values);
        }

        let Context {
            tx_context,
            gas_coin,
            objects,
            pure,
            receiving,
            results,
        } = context;
        consume_value_opt(gas_coin);
        // TODO do we want to check inputs in the dev inspect case?
        consume_value_opts(objects);
        consume_value_opts(pure);
        consume_value_opts(receiving);
        assert_invariant!(results.len() == commands.len(), "result length mismatch");
        for (i, (result, c)) in results.into_iter().zip(&ast.commands).enumerate() {
            let tys = &c.value.result_type;
            assert_invariant!(result.len() == tys.len(), "result length mismatch");
            for (j, (vopt, ty)) in result.into_iter().zip(tys).enumerate() {
                drop_value_opt::<Mode>((i, j), vopt, ty)?;
            }
        }
        assert_invariant!(tx_context.is_some(), "tx_context should never be moved");
        Ok(())
    }

    fn command(
        context: &mut Context,
        sp!(_, c): &T::Command,
    ) -> Result<Vec<Value>, ExecutionError> {
        let result_tys = &c.result_type;
        Ok(match &c.command {
            T::Command__::MoveCall(mc) => {
                let T::MoveCall {
                    function,
                    arguments: args,
                } = &**mc;
                let return_ = &function.signature.return_;
                let arg_values = arguments(context, args)?;
                consume_values(arg_values);
                (0..return_.len()).map(|_| Value).collect()
            }
            T::Command__::TransferObjects(objects, recipient) => {
                let object_values = arguments(context, objects)?;
                let recipient_value = argument(context, recipient)?;
                consume_values(object_values);
                consume_value(recipient_value);
                vec![]
            }
            T::Command__::SplitCoins(_, coin, amounts) => {
                let coin_value = argument(context, coin)?;
                let amount_values = arguments(context, amounts)?;
                consume_values(amount_values);
                consume_value(coin_value);
                (0..amounts.len()).map(|_| Value).collect()
            }
            T::Command__::MergeCoins(_, target, coins) => {
                let target_value = argument(context, target)?;
                let coin_values = arguments(context, coins)?;
                consume_values(coin_values);
                consume_value(target_value);
                vec![]
            }
            T::Command__::MakeMoveVec(_, xs) => {
                let vs = arguments(context, xs)?;
                consume_values(vs);
                vec![Value]
            }
            T::Command__::Publish(_, _, _) => result_tys.iter().map(|_| Value).collect(),
            T::Command__::Upgrade(_, _, _, x, _) => {
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

    fn drop_value_opt<Mode: ExecutionMode>(
        idx: (usize, usize),
        value: Option<Value>,
        ty: &Type,
    ) -> Result<(), ExecutionError> {
        match value {
            Some(v) => drop_value::<Mode>(idx, v, ty),
            None => Ok(()),
        }
    }

    fn drop_value<Mode: ExecutionMode>(
        (i, j): (usize, usize),
        value: Value,
        ty: &Type,
    ) -> Result<(), ExecutionError> {
        let abilities = ty.abilities();
        if !abilities.has_drop() && !Mode::allow_arbitrary_values() {
            let msg = if abilities.has_copy() {
                "The value has copy, but not drop. \
                Its last usage must be by-value so it can be taken."
            } else {
                "Unused value without drop"
            };
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::UnusedValueWithoutDrop {
                    result_idx: i as u16,
                    secondary_idx: j as u16,
                },
                msg,
            ));
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
            T::Argument__::Freeze(usage) => freeze_ref(context, usage),
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

    fn freeze_ref(context: &mut Context, u: &T::Usage) -> Result<Value, ExecutionError> {
        let value = match u {
            T::Usage::Move(l) => move_value(context, *l)?,
            T::Usage::Copy { location, .. } => copy_value(context, *location)?,
        };
        consume_value(value);
        Ok(Value)
    }
}
