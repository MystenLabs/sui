// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    sp,
    static_programmable_transactions::{env::Env, typing::ast as T},
};
use indexmap::IndexSet;
use std::rc::Rc;
use sui_types::error::ExecutionError;

/// A dot-star like extension, but with a unique identifier. Deltas can be compared between
/// different Deltas of the same command, otherwise they behave like .* in the regex based
/// implementation. This means that it represents an arbitrary field extension of the reference
/// in question. However, due to invariants within reference safety, for mutable references these
/// extensions cannot with other references from the same command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Delta {
    command: u16,
    result: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RootLocation {
    /// The result of a command, specifically a `MoveCall`, without any input references.
    /// These calls will always abort, but we still must track them.
    Unknown {
        command: u16,
    },
    Known(T::Location),
}

/// A path points to an abstract memory location, rooted in an input or command result. Any
/// extension is the result from a reference being returned from a command.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Path {
    root: RootLocation,
    extensions: Vec<Delta>,
}

#[derive(Debug)]
struct PathSet(IndexSet<Path>);

#[derive(Debug)]
enum Value {
    NonRef,
    Ref { is_mut: bool, paths: Rc<PathSet> },
}

#[derive(Debug)]
struct Location {
    // A singleton set pointing to the location itself
    self_path: Rc<PathSet>,
    value: Option<Value>,
}

#[derive(Debug)]
struct Context {
    tx_context: Location,
    gas: Location,
    object_inputs: Vec<Location>,
    pure_inputs: Vec<Location>,
    receiving_inputs: Vec<Location>,
    results: Vec<Vec<Location>>,
    // Temporary set of locations borrowed by arguments seen thus far for the current command.
    // Used exclusively for checking the validity copy/move.
    arg_roots: IndexSet<T::Location>,
}

enum PathComparison {
    /// `self` is a strict prefix of `other`
    Prefix,
    /// `self` and `other` are the same path
    Aliases,
    /// `self` extends `other`
    Extends,
    /// `self` and `other` point to distinct regions of memory. They might however be rooted
    /// in the same parent region, i.e. the same `root` location or a prefix of the same
    /// `extensions`
    Disjoint,
}

impl Path {
    fn initial(location: T::Location) -> Self {
        Self {
            root: RootLocation::Known(location),
            extensions: vec![],
        }
    }

    /// See `PathComparison` for the meaning of the return value
    fn compare(&self, other: &Self) -> PathComparison {
        if self.root != other.root {
            return PathComparison::Disjoint;
        };
        let mut self_extensions = self.extensions.iter();
        let mut other_extensions = other.extensions.iter();
        loop {
            match (self_extensions.next(), other_extensions.next()) {
                (Some(self_ext), Some(other_ext)) => {
                    if self_ext.command != other_ext.command {
                        // Cannot compare `Delta` from distinct commands, as such we do assume
                        // the possibility that `self` extends `other`
                        return PathComparison::Extends;
                    }
                    if self_ext.result != other_ext.result {
                        // If the command is the same, but the result is different, we know that
                        // they must be disjoint. Or they are immutable references, in which case
                        // we do not care.
                        return PathComparison::Disjoint;
                    }
                }
                (None, Some(_)) => return PathComparison::Prefix,
                (Some(_), None) => return PathComparison::Extends,
                (None, None) => return PathComparison::Aliases,
            }
        }
    }

    /// Create a new `Path` that extends the current path with the given `extension`.
    fn extend(&self, extension: Delta) -> Self {
        let mut new_extensions = self.extensions.clone();
        new_extensions.push(extension);
        Self {
            root: self.root,
            extensions: new_extensions,
        }
    }
}

impl PathSet {
    /// Should be used only for `call` for creating initial path sets.
    fn empty() -> Self {
        Self(IndexSet::new())
    }

    fn initial(location: T::Location) -> Self {
        Self(IndexSet::from([Path::initial(location)]))
    }

    fn unknown_root(command: u16) -> Self {
        Self(IndexSet::from([Path {
            root: RootLocation::Unknown { command },
            extensions: vec![],
        }]))
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns true if any path in `self` `Extends` with any path in `other`.
    /// Excludes `Aliases` if `ignore_aliases` is true.
    fn extends(&self, other: &Self, ignore_aliases: bool) -> bool {
        self.0.iter().any(|self_path| {
            other
                .0
                .iter()
                .any(|other_path| match self_path.compare(other_path) {
                    PathComparison::Prefix | PathComparison::Disjoint => false,
                    PathComparison::Aliases => !ignore_aliases,
                    PathComparison::Extends => true,
                })
        })
    }

    /// Returns true if all paths in `self` are `Disjoint` with all paths in `other`.
    fn is_disjoint(&self, other: &Self) -> bool {
        self.0.iter().all(|self_path| {
            other
                .0
                .iter()
                .all(|other_path| match self_path.compare(other_path) {
                    PathComparison::Disjoint => true,
                    PathComparison::Prefix | PathComparison::Aliases | PathComparison::Extends => {
                        false
                    }
                })
        })
    }

    /// Insert all paths from `other` into `self`.
    fn union(&mut self, other: &PathSet) {
        // We might be able to optimize this slightly by not including paths that are extensions
        // of existing paths
        self.0.extend(other.0.iter().cloned());
    }

    /// Create a new `PathSet` where all paths in `self` are extended with the given `extension`.
    fn extend(&self, extension: Delta) -> Self {
        let mut new_paths = IndexSet::with_capacity(self.0.len());
        for path in &self.0 {
            new_paths.insert(path.extend(extension));
        }
        Self(new_paths)
    }
}

impl Value {
    /// Create a new reference value
    fn ref_(is_mut: bool, paths: PathSet) -> anyhow::Result<Value> {
        anyhow::ensure!(
            !paths.is_empty(),
            "Cannot create a reference with an empty path set"
        );
        Ok(Value::Ref {
            is_mut,
            paths: Rc::new(paths),
        })
    }

    fn copy(&self) -> Value {
        match self {
            Value::NonRef => Value::NonRef,
            Value::Ref { is_mut, paths } => Value::Ref {
                is_mut: *is_mut,
                paths: paths.clone(),
            },
        }
    }

    fn freeze(&mut self) -> anyhow::Result<Value> {
        let copied = self.copy();
        match copied {
            Value::NonRef => {
                anyhow::bail!("Cannot freeze a non-reference value")
            }
            Value::Ref { is_mut, paths } => {
                anyhow::ensure!(is_mut, "Cannot freeze an immutable reference");
                Ok(Value::Ref {
                    is_mut: false,
                    paths,
                })
            }
        }
    }
}

impl Location {
    fn non_ref(location: T::Location) -> Self {
        Self {
            self_path: Rc::new(PathSet::initial(location)),
            value: Some(Value::NonRef),
        }
    }

    fn copy_value(&self) -> anyhow::Result<Value> {
        let Some(value) = self.value.as_ref() else {
            anyhow::bail!("Use of invalid memory location")
        };
        Ok(value.copy())
    }

    fn move_value(&mut self) -> anyhow::Result<Value> {
        let Some(value) = self.value.take() else {
            anyhow::bail!("Use of invalid memory location")
        };
        Ok(value)
    }

    fn use_(&mut self, usage: &T::Usage) -> anyhow::Result<Value> {
        match usage {
            T::Usage::Move(_) => self.move_value(),
            T::Usage::Copy { .. } => self.copy_value(),
        }
    }

    fn borrow(&mut self, is_mut: bool) -> anyhow::Result<Value> {
        let Some(value) = self.value.as_ref() else {
            anyhow::bail!("Borrow of invalid memory location")
        };
        match value {
            Value::Ref { .. } => {
                anyhow::bail!("Cannot borrow a reference")
            }
            Value::NonRef => {
                anyhow::ensure!(
                    !self.self_path.is_empty(),
                    "Cannot have an empty location to borrow from"
                );
                // a new reference that borrows from this location
                Ok(Value::Ref {
                    is_mut,
                    paths: self.self_path.clone(),
                })
            }
        }
    }
}

impl Context {
    fn new(txn: &T::Transaction) -> Self {
        let T::Transaction {
            bytes: _,
            objects,
            pure,
            receiving,
            commands: _,
        } = txn;
        let tx_context = Location::non_ref(T::Location::TxContext);
        let gas = Location::non_ref(T::Location::GasCoin);
        let object_inputs = (0..objects.len())
            .map(|i| Location::non_ref(T::Location::ObjectInput(i as u16)))
            .collect();
        let pure_inputs = (0..pure.len())
            .map(|i| Location::non_ref(T::Location::PureInput(i as u16)))
            .collect();
        let receiving_inputs = (0..receiving.len())
            .map(|i| Location::non_ref(T::Location::ReceivingInput(i as u16)))
            .collect();
        Self {
            tx_context,
            gas,
            object_inputs,
            pure_inputs,
            receiving_inputs,
            results: vec![],
            arg_roots: IndexSet::new(),
        }
    }

    fn current_command(&self) -> u16 {
        self.results.len() as u16
    }

    fn add_result_values(&mut self, results: impl IntoIterator<Item = Value>) {
        let command = self.current_command();
        self.results.push(
            results
                .into_iter()
                .enumerate()
                .map(|(i, v)| Location {
                    self_path: Rc::new(PathSet::initial(T::Location::Result(command, i as u16))),
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

    fn location(&self, loc: T::Location) -> anyhow::Result<&Location> {
        Ok(match loc {
            T::Location::TxContext => &self.tx_context,
            T::Location::GasCoin => &self.gas,
            T::Location::ObjectInput(i) => self
                .object_inputs
                .get(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Object input index out of bounds {i}"))?,
            T::Location::PureInput(i) => self
                .pure_inputs
                .get(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Pure input index out of bounds {i}"))?,
            T::Location::ReceivingInput(i) => self
                .receiving_inputs
                .get(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Receiving input index out of bounds {i}"))?,
            T::Location::Result(i, j) => self
                .results
                .get(i as usize)
                .and_then(|r| r.get(j as usize))
                .ok_or_else(|| anyhow::anyhow!("Result index out of bounds ({i},{j})"))?,
        })
    }

    fn location_mut(&mut self, loc: T::Location) -> anyhow::Result<&mut Location> {
        Ok(match loc {
            T::Location::TxContext => &mut self.tx_context,
            T::Location::GasCoin => &mut self.gas,
            T::Location::ObjectInput(i) => self
                .object_inputs
                .get_mut(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Object input index out of bounds {i}"))?,
            T::Location::PureInput(i) => self
                .pure_inputs
                .get_mut(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Pure input index out of bounds {i}"))?,
            T::Location::ReceivingInput(i) => self
                .receiving_inputs
                .get_mut(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Receiving input index out of bounds {i}"))?,
            T::Location::Result(i, j) => self
                .results
                .get_mut(i as usize)
                .and_then(|r| r.get_mut(j as usize))
                .ok_or_else(|| anyhow::anyhow!("Result index out of bounds ({i},{j})"))?,
        })
    }

    fn check_usage(&self, usage: &T::Usage, location: &Location) -> anyhow::Result<()> {
        // by marking "ignore alias" as `false`, we will also check for `Alias` paths, i.e. paths
        // that point to the location itself without any extensions.
        let is_borrowed = self.any_extends(&location.self_path, /* ignore alias */ false)
            || self.arg_roots.contains(&usage.location());
        match usage {
            T::Usage::Move(_) => {
                anyhow::ensure!(!is_borrowed, "Cannot move a value that is borrowed");
            }
            T::Usage::Copy { borrowed, .. } => {
                let Some(borrowed) = borrowed.get().copied() else {
                    anyhow::bail!("Borrowed flag not set for copy usage");
                };
                anyhow::ensure!(
                    borrowed == is_borrowed,
                    "Borrowed flag mismatch for copy usage: expected {borrowed}, got {is_borrowed} \
                    location {:?} for in command {}",
                    location.self_path,
                    self.current_command()
                );
            }
        }
        Ok(())
    }

    fn argument(&mut self, sp!(_, (arg, _)): &T::Argument) -> anyhow::Result<Value> {
        let location = self.location(arg.location())?;
        match arg {
            T::Argument__::Use(usage)
            | T::Argument__::Freeze(usage)
            | T::Argument__::Read(usage) => self.check_usage(usage, location)?,
            T::Argument__::Borrow(_, _) => (),
        };
        let location = self.location_mut(arg.location())?;
        let value = match arg {
            T::Argument__::Use(usage) => location.use_(usage)?,
            T::Argument__::Freeze(usage) => location.use_(usage)?.freeze()?,
            T::Argument__::Borrow(is_mut, _) => location.borrow(*is_mut)?,
            T::Argument__::Read(usage) => {
                location.use_(usage)?;
                Value::NonRef
            }
        };
        if let Value::Ref { paths, .. } = &value {
            for p in &paths.0 {
                match p.root {
                    RootLocation::Unknown { .. } => (),
                    RootLocation::Known(location) => {
                        self.arg_roots.insert(location);
                    }
                }
            }
        }
        Ok(value)
    }

    fn arguments(&mut self, args: &[T::Argument]) -> anyhow::Result<Vec<Value>> {
        args.iter()
            .map(|arg| self.argument(arg))
            .collect::<anyhow::Result<Vec<_>>>()
    }

    fn all_references(&self) -> impl Iterator<Item = Rc<PathSet>> {
        let Self {
            tx_context,
            gas,
            object_inputs,
            pure_inputs,
            receiving_inputs,
            results,
            arg_roots: _,
        } = self;
        std::iter::once(tx_context)
            .chain(std::iter::once(gas))
            .chain(object_inputs)
            .chain(pure_inputs)
            .chain(receiving_inputs)
            .chain(results.iter().flatten())
            .filter_map(|v| -> Option<Rc<PathSet>> {
                match v.value.as_ref() {
                    Some(Value::Ref { paths, .. }) => Some(paths.clone()),
                    Some(Value::NonRef) | None => None,
                }
            })
    }

    /// Returns true if any of the references in a given `T::Location` extends a path in `paths`.
    /// Excludes `Aliases` if `ignore_aliases` is true.
    fn any_extends(&self, paths: &PathSet, ignore_aliases: bool) -> bool {
        self.all_references()
            .any(|other| other.extends(paths, ignore_aliases))
    }
}

/// Verifies memory safety of a transaction. This is a re-implementation of `verify::memory_safety`
/// using an alternative approach given the newness of the Regex based borrow graph in that
/// implementation.
/// This is a set based approach were each reference is represent as a set of paths. A path
/// is has a root (basically a `T::Location` plus some edge case massaging) and a list of extensions
/// resulting from Move function calls. Each one of those Move calls gets a `Delta` extension for
/// each return value. The `Delta` is like the ".*" in the regex based implementation but where it
/// carries a sense of identity. This identity allows for invariants from the return values of the
/// Move call to be leveraged. For example, mutable references returned from a call cannot overlap.
/// If we just used ".*", we would not be able to express this invariant without some sense of
/// identity for the reference itself (which is what is going on in the Regex based implementation).
/// This implementation stems from research work for the Move borrow checker, but would normally
/// not be expressive enough in the presence of control flow. Luckily, PTBs do not have control flow
/// so we can use this approach as a safety net for the Regex based implementation until that
/// code is sufficiently. tested and hardened.
/// Checks the following
/// - Values are not used after being moved
/// - Reference safety is upheld (no dangling references)
pub fn verify(_env: &Env, txn: &T::Transaction) -> Result<(), ExecutionError> {
    verify_(txn).map_err(|e| make_invariant_violation!("{}. Transaction {:?}", e, txn))
}

fn verify_(txn: &T::Transaction) -> anyhow::Result<()> {
    let mut context = Context::new(txn);
    let T::Transaction {
        bytes: _,
        objects: _,
        pure: _,
        receiving: _,
        commands,
    } = txn;
    for (c, result_tys) in commands {
        debug_assert!(context.arg_roots.is_empty());
        command(&mut context, c, result_tys)?;
        context.arg_roots.clear();
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
            let coin_value = context.argument(coin)?;
            write_ref(context, coin_value)?;
            context.add_results(result_tys);
        }
        T::Command_::MergeCoins(_, target, coins) => {
            context.arguments(coins)?;
            let target_value = context.argument(target)?;
            write_ref(context, target_value)?;
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

fn write_ref(context: &Context, value: Value) -> anyhow::Result<()> {
    match value {
        Value::NonRef => {
            anyhow::bail!("Cannot write to a non-reference value");
        }

        Value::Ref { is_mut: false, .. } => {
            anyhow::bail!("Cannot write to an immutable reference");
        }
        Value::Ref {
            is_mut: true,
            paths,
        } => {
            anyhow::ensure!(
                !context.any_extends(&paths, /* ignore alias */ true),
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
    let mut all_paths: PathSet = PathSet::empty();
    let mut imm_paths: PathSet = PathSet::empty();
    let mut mut_paths: PathSet = PathSet::empty();
    for arg in arguments {
        match arg {
            Value::NonRef => (),
            Value::Ref {
                is_mut: true,
                paths,
            } => {
                // Allow alias conflicts with references not passed as arguments
                anyhow::ensure!(
                    !context.any_extends(&paths, /* ignore alias */ true),
                    "Cannot transfer a mutable ref with extensions"
                );
                // All mutable argument references must be disjoint from all other references
                anyhow::ensure!(mut_paths.is_disjoint(&paths), "Double mutable borrow");
                all_paths.union(&paths);
                mut_paths.union(&paths);
            }
            Value::Ref {
                is_mut: false,
                paths,
            } => {
                all_paths.union(&paths);
                imm_paths.union(&paths);
            }
        }
    }
    // All mutable references must be disjoint from all immutable references
    anyhow::ensure!(
        imm_paths.is_disjoint(&mut_paths),
        "Mutable and immutable borrows cannot overlap"
    );
    let command = context.current_command();
    let mut_paths = if mut_paths.is_empty() {
        PathSet::unknown_root(command)
    } else {
        mut_paths
    };
    let all_paths = if all_paths.is_empty() {
        PathSet::unknown_root(command)
    } else {
        all_paths
    };
    let result_values = return_
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            let delta = Delta {
                command,
                result: i as u16,
            };
            match ty {
                T::Type::Reference(/* is mut */ true, _) => {
                    Value::ref_(true, mut_paths.extend(delta))
                }
                T::Type::Reference(/* is mut */ false, _) => {
                    Value::ref_(false, all_paths.extend(delta))
                }
                _ => Ok(Value::NonRef),
            }
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    context.add_result_values(result_values);
    Ok(())
}

//**************************************************************************************************
// impl
//**************************************************************************************************

impl Path {
    #[cfg(debug_assertions)]
    #[allow(unused)]
    fn print(&self) {
        print!("{:?}", self.root);
        for ext in &self.extensions {
            let Delta { command, result } = ext;
            print!(".d{}_{}", command, result);
        }
        println!(",");
    }
}

impl PathSet {
    #[cfg(debug_assertions)]
    #[allow(unused)]
    fn print(&self) {
        println!("{{");
        for path in &self.0 {
            path.print();
        }
        println!("}}");
    }
}

impl Value {
    #[cfg(debug_assertions)]
    #[allow(unused)]
    fn print(&self) {
        match self {
            Value::NonRef => print!("NonRef"),
            Value::Ref { is_mut, paths } => {
                if *is_mut {
                    print!("mut ");
                } else {
                    print!("imm ");
                }
                paths.print();
            }
        }
    }
}

impl Location {
    #[cfg(debug_assertions)]
    #[allow(unused)]
    fn print(&self) {
        print!("{{ self_path: ");
        self.self_path.print();
        print!(", value: ");
        if let Some(value) = &self.value {
            value.print();
        } else {
            println!("_");
        }
        println!("}}");
    }
}

impl Context {
    #[cfg(debug_assertions)]
    #[allow(unused)]
    fn print(&self) {
        println!("Context {{");
        println!("  tx_context: ");
        self.tx_context.print();
        println!("  gas: ");
        self.gas.print();
        println!("  object_inputs: [");
        for input in &self.object_inputs {
            input.print();
        }
        println!("  ],");
        println!("  pure_inputs: [");
        for input in &self.pure_inputs {
            input.print();
        }
        println!("  ],");
        println!("  receiving_inputs: [");
        for input in &self.receiving_inputs {
            input.print();
        }
        println!("  ],");
        println!("  results: [");
        for result in &self.results {
            for loc in result {
                loc.print();
            }
            println!(",");
        }
        println!("  ],");
        println!("}}");
    }
}
