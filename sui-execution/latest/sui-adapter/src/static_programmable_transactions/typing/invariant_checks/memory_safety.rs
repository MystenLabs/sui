// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    sp,
    static_programmable_transactions::{env::Env, typing::ast as T},
};
use indexmap::IndexSet;
use std::rc::Rc;
use sui_types::error::ExecutionError;

/// We more or less have a new sym counter for each reference we make
/// We do not include the `Rc` in this type itself to make it clear where we are relying on the
/// Rc's reference counting for correctness
type AbstractLocation = u64;

/// Simple counter for abstract locations.
struct AbstractLocationCounter(u64);

struct AbstractReference {
    /// the roots this locations borrows from
    borrows_from: IndexSet<Rc<AbstractLocation>>,
    /// The root location of the reference, extensions borrow from this `root` and all locations
    /// this reference borrows from
    root: Rc<AbstractLocation>,
}

enum Value {
    NonRef,
    Ref {
        is_mut: bool,
        data: Rc<AbstractReference>,
    },
}

struct Location {
    // Much like a reference, this tracks used to track if the location is borrowed
    // However, this is not used if the value is a reference (since you cannot borrow a reference)
    root: Rc<AbstractLocation>,
    value: Option<Value>,
}

struct Context {
    counter: AbstractLocationCounter,
    tx_context: Location,
    gas: Location,
    inputs: Vec<Location>,
    results: Vec<Vec<Location>>,
}

impl AbstractLocationCounter {
    fn new() -> Self {
        Self(0)
    }

    fn next(&mut self) -> AbstractLocation {
        let abs = self.0;
        self.0 += 1;
        abs
    }
}

impl AbstractReference {
    fn new(
        counter: &mut AbstractLocationCounter,
        borrows_from: IndexSet<Rc<AbstractLocation>>,
    ) -> Self {
        Self {
            root: Rc::new(counter.next()),
            borrows_from,
        }
    }

    fn has_extensions(&self) -> bool {
        Rc::strong_count(&self.root) > 1
    }
}

impl Value {
    fn copy(&self) -> Value {
        match self {
            Value::NonRef => Value::NonRef,
            Value::Ref { is_mut, data } => Value::Ref {
                is_mut: *is_mut,
                data: data.clone(),
            },
        }
    }

    fn freeze(&mut self) -> anyhow::Result<Value> {
        let copied = self.copy();
        match copied {
            Value::NonRef => {
                anyhow::bail!("Cannot freeze a non-reference value")
            }
            Value::Ref { is_mut, data } => {
                anyhow::ensure!(is_mut, "Cannot freeze an immutable reference");
                Ok(Value::Ref {
                    is_mut: false,
                    data,
                })
            }
        }
    }
}

impl Location {
    fn non_ref(counter: &mut AbstractLocationCounter) -> Self {
        Self {
            root: Rc::new(counter.next()),
            value: Some(Value::NonRef),
        }
    }

    /// returns true if the location is borrowed.
    /// It does not check if the reference (if present) is borrowed.
    fn is_borrowed(&self) -> anyhow::Result<bool> {
        let location_is_borrowed = Rc::strong_count(&self.root) > 1;
        match &self.value {
            None => {
                anyhow::ensure!(
                    !location_is_borrowed,
                    "A location without a value should not be borrowed"
                );
                Ok(false)
            }
            Some(Value::Ref { .. }) => {
                anyhow::ensure!(
                    !location_is_borrowed,
                    "A reference value's location should not be borrowed"
                );
                Ok(false)
            }
            Some(Value::NonRef) => Ok(location_is_borrowed),
        }
    }

    fn copy_value(&self) -> anyhow::Result<Value> {
        let Some(value) = self.value.as_ref() else {
            anyhow::bail!("Use of invalid memory location")
        };
        Ok(value.copy())
    }

    fn take(&mut self) -> anyhow::Result<Value> {
        let Some(value) = self.value.take() else {
            anyhow::bail!("Use of invalid memory location")
        };
        // we have to check this after moving the value to ensure we are not considering
        // whether a reference stored here is borrowed
        if self.is_borrowed()? {
            anyhow::bail!("Cannot move borrowed value")
        }
        Ok(value)
    }

    fn use_(&mut self, usage: &T::Usage) -> anyhow::Result<Value> {
        match usage {
            T::Usage::Move(_) => self.take(),
            T::Usage::Copy { borrowed, .. } => {
                let Some(borrowed) = borrowed.get().copied() else {
                    anyhow::bail!("`borrowed` was not set for location usage")
                };
                anyhow::ensure!(
                    self.is_borrowed()? == borrowed,
                    "Value borrowed status does not match usage marked borrowed status"
                );
                self.copy_value()
            }
        }
    }

    fn borrow(
        &mut self,
        counter: &mut AbstractLocationCounter,
        is_mut: bool,
    ) -> anyhow::Result<Value> {
        match &self.value {
            None => {
                anyhow::bail!("Borrow of invalid memory location")
            }
            Some(Value::Ref { .. }) => {
                anyhow::bail!("Cannot borrow a reference")
            }
            Some(Value::NonRef) => {
                // a new reference that borrows from this location
                let borrows_from = IndexSet::from([self.root.clone()]);
                Ok(Value::Ref {
                    is_mut,
                    data: Rc::new(AbstractReference::new(counter, borrows_from)),
                })
            }
        }
    }
}

impl Context {
    fn new(txn: &T::Transaction) -> Self {
        let mut counter = AbstractLocationCounter::new();
        let tx_context = Location::non_ref(&mut counter);
        let gas = Location::non_ref(&mut counter);
        let inputs = txn
            .inputs
            .iter()
            .map(|_| Location::non_ref(&mut counter))
            .collect();
        Self {
            counter,
            tx_context,
            gas,
            inputs,
            results: vec![],
        }
    }

    fn add_result_values(&mut self, results: impl IntoIterator<Item = Value>) {
        self.results.push(
            results
                .into_iter()
                .map(|v| Location {
                    root: Rc::new(self.counter.next()),
                    value: Some(v),
                })
                .collect(),
        );
    }

    fn add_results(&mut self, results: &[T::Type]) {
        self.add_result_values(results.iter().map(|t| {
            debug_assert!(!matches!(t, T::Type::Reference(_, _)));
            Value::NonRef
        }));
    }

    fn location(
        &mut self,
        loc: T::Location,
    ) -> anyhow::Result<(&mut AbstractLocationCounter, &mut Location)> {
        let location = match loc {
            T::Location::TxContext => &mut self.tx_context,
            T::Location::GasCoin => &mut self.gas,
            T::Location::Input(i) => self
                .inputs
                .get_mut(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Input index out of bounds {i}"))?,
            T::Location::Result(i, j) => self
                .results
                .get_mut(i as usize)
                .and_then(|r| r.get_mut(j as usize))
                .ok_or_else(|| anyhow::anyhow!("Result index out of bounds ({i},{j})"))?,
        };
        Ok((&mut self.counter, location))
    }

    fn argument(&mut self, sp!(_, (arg, _)): &T::Argument) -> anyhow::Result<Value> {
        let (counter, location) = self.location(arg.location())?;
        let value = match arg {
            T::Argument__::Use(usage) => location.use_(usage)?,
            T::Argument__::Freeze(usage) => location.use_(usage)?.freeze()?,
            T::Argument__::Borrow(is_mut, _) => location.borrow(counter, *is_mut)?,
            T::Argument__::Read(usage) => {
                location.use_(usage)?;
                Value::NonRef
            }
        };
        Ok(value)
    }

    fn arguments(&mut self, args: &[T::Argument]) -> anyhow::Result<Vec<Value>> {
        args.iter()
            .map(|arg| self.argument(arg))
            .collect::<anyhow::Result<Vec<_>>>()
    }
}

/// Verifies memory safety of a transaction. This is a re-implementation of `verify::memory_safety`
/// using an alternative approach given the newness of the Regex based borrow graph in that
/// implementation.
/// This approach uses `Rc`s to track the borrowing of memory locations in the PTB. This only works
/// given the limited construction of references and their usage. If more direct aliasing and
/// extensions were possible, this implementation would not be expressive enough. In other words,
/// this implementation assumes that all extensions are the result of a "call" (a ".*" extension in
/// the regex based implementation).
/// Checks the following
/// - Values are not used after being moved
/// - Reference safety is upheld (no dangling references)
pub fn verify(_env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    verify_(txn).map_err(|e| make_invariant_violation!("{}. Transaction {:?}", e, txn))
}

fn verify_(txn: &T::Transaction) -> anyhow::Result<()> {
    let mut context = Context::new(txn);
    let T::Transaction {
        inputs: _,
        commands,
    } = txn;
    for (c, result_tys) in commands {
        command(&mut context, c, result_tys)?;
    }
    Ok(())
}

fn command(
    context: &mut Context,
    sp!(_, command): &T::Command,
    result_tys: &[T::Type],
) -> anyhow::Result<()> {
    match command {
        T::Command_::MoveCall(move_call) => {
            let T::MoveCall {
                function,
                arguments,
            } = &**move_call;
            let arg_values = context.arguments(arguments)?;
            call(context, &function.signature, arg_values)?;
        }
        T::Command_::TransferObjects(objs, recipient) => {
            context.arguments(objs)?;
            context.argument(recipient)?;
            context.add_results(result_tys);
        }
        T::Command_::SplitCoins(_, coin, amounts) => {
            context.arguments(amounts)?;
            context.argument(coin)?;
            context.add_results(result_tys);
        }
        T::Command_::MergeCoins(_, target, coins) => {
            context.arguments(coins)?;
            let target_value = context.argument(target)?;
            write_ref(target_value)?;
        }
        T::Command_::MakeMoveVec(_, arguments) => {
            context.arguments(arguments)?;
            context.add_results(result_tys);
        }
        T::Command_::Publish(_, _, _) => context.add_results(result_tys),
        T::Command_::Upgrade(_, _, _, ticket, _) => {
            context.argument(ticket)?;
            context.add_results(result_tys);
        }
    }

    Ok(())
}

fn write_ref(value: Value) -> anyhow::Result<()> {
    match value {
        Value::NonRef => {
            anyhow::bail!("Cannot write to a non-reference value");
        }

        Value::Ref { is_mut: false, .. } => {
            anyhow::bail!("Cannot write to an immutable reference");
        }
        Value::Ref { is_mut: true, data } => {
            anyhow::ensure!(
                !data.has_extensions(),
                "Cannot write to a mutable reference that has extensions"
            );
            Ok(())
        }
    }
}

fn call(
    context: &mut Context,
    signature: &T::LoadedFunctionInstantiation,
    arguments: Vec<Value>,
) -> anyhow::Result<()> {
    let return_ = &signature.return_;
    let mut all_locations: IndexSet<Rc<AbstractLocation>> = IndexSet::new();
    let mut imm_locations: IndexSet<Rc<AbstractLocation>> = IndexSet::new();
    let mut mut_locations: IndexSet<Rc<AbstractLocation>> = IndexSet::new();
    for arg in arguments {
        match arg {
            Value::NonRef => (),
            Value::Ref { is_mut: true, data } => {
                anyhow::ensure!(
                    !data.has_extensions(),
                    "Cannot transfer a mutable ref with extensions"
                );
                all_locations.extend(data.borrows_from.iter().cloned());
                // mutable borrows must be unique going into a call
                for loc in &data.borrows_from {
                    let was_new = mut_locations.insert(loc.clone());
                    // if not new, then the location was already passed to the call
                    anyhow::ensure!(was_new, "Double mutable borrow");
                }
            }
            Value::Ref {
                is_mut: false,
                data,
            } => {
                all_locations.extend(data.borrows_from.iter().cloned());
                imm_locations.extend(data.borrows_from.iter().cloned());
            }
        }
    }
    for mut_loc in &mut_locations {
        anyhow::ensure!(
            !imm_locations.contains(mut_loc),
            "Mutable and immutable borrows cannot overlap"
        );
    }
    let counter = &mut context.counter;
    let result_values = return_
        .iter()
        .map(|ty| {
            match ty {
                T::Type::Reference(/* is mut */ true, _) => Value::Ref {
                    is_mut: true,
                    data: Rc::new(AbstractReference::new(counter, mut_locations.clone())),
                },
                // technically the immutable references could borrow from each other, but it does
                // not matter in practice since we cannot write to them or their extensions
                T::Type::Reference(/* is mut */ false, _) => Value::Ref {
                    is_mut: true,
                    data: Rc::new(AbstractReference::new(counter, all_locations.clone())),
                },
                _ => Value::NonRef,
            }
        })
        .collect::<Vec<_>>();
    context.add_result_values(result_values);
    Ok(())
}
