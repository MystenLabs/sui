// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution_mode::ExecutionMode,
    sp,
    static_programmable_transactions::{env::Env, typing::ast as T},
};
use indexmap::{IndexMap, IndexSet};
use mysten_common::ZipDebugEqIteratorExt;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RootLocation {
    Unknown { command: u16 },
    Known(T::Location),
}

type NodeID = usize;

/// A packed set of small indices. Index `n` is stored in word `n / 64` at bit `n % 64`. Used both
/// for node-id ancestor sets and, in `Memory::parent_commands`, for command-id sets.
#[derive(Debug, Clone, Default)]
struct BitSet {
    words: Vec<u64>,
}

#[derive(Debug)]
enum NodeKind {
    Root,
    Delta { command: u16 },
}

#[derive(Debug)]
struct Node {
    kind: NodeKind,
    /// Contains the node itself and all of its transitive ancestors
    ancestors: BitSet,
    children: Vec<NodeID>,
}

#[derive(Debug)]
struct Memory {
    nodes: Vec<Node>,
    roots: BTreeMap<RootLocation, NodeID>,
    /// Commands that created delta children of each parent. Used to detect when a parent
    /// has children from more than one command.
    parent_commands: IndexMap<NodeID, BitSet>,
    /// Parents with delta children from multiple commands. Only these parents can
    /// cause cross-command overlap.
    conflict_parents: IndexSet<NodeID>,
}

#[derive(Debug)]
enum Value {
    NonRef,
    Ref { is_mut: bool, node: NodeID },
}

#[derive(Debug)]
struct Location {
    /// The location represented by this root. Its graph node is created on first borrow,
    /// so values that are never borrowed need no node.
    root: RootLocation,
    value: Option<Value>,
}

#[derive(Debug)]
struct Context {
    memory: Memory,
    allow_references_in_ptbs: bool,
    tx_context: Location,
    gas: Location,
    object_inputs: Vec<Location>,
    withdrawal_inputs: Vec<Location>,
    pure_inputs: Vec<Location>,
    receiving_inputs: Vec<Location>,
    results: Vec<Vec<Location>>,
    /// Indices into `results` of rows that held at least one reference when produced.
    /// This lets `all_references` skip result rows that never contained references.
    result_ref_rows: Vec<usize>,
    // References passed to earlier arguments must keep their sources borrowed until the command
    // finishes, even after being moved out of their locations.
    arg_reference_nodes: Vec<NodeID>,
}

impl BitSet {
    /// Creates an empty set with capacity for `bits` indices.
    fn with_bits(bits: usize) -> Self {
        Self {
            words: vec![0; bits.div_ceil(64).max(1)],
        }
    }

    /// Sets the bit and returns whether it was already set. Errors if it is out of bounds.
    fn set(&mut self, bit: usize) -> anyhow::Result<bool> {
        let word = self
            .words
            .get_mut(bit / 64)
            .ok_or_else(|| anyhow::anyhow!("BitSet index {bit} is out of bounds"))?;
        let mask = 1 << (bit % 64);
        let was_set = *word & mask != 0;
        *word |= mask;
        Ok(was_set)
    }

    /// Sets the bit, growing the allocation to fit.
    fn set_grow(&mut self, bit: usize) {
        let word_index = bit / 64;
        if word_index >= self.words.len() {
            self.words.resize(word_index.saturating_add(1), 0);
        }
        if let Some(word) = self.words.get_mut(word_index) {
            *word |= 1 << (bit % 64);
        }
    }

    /// Out-of-range bits are treated as unset.
    fn contains(&self, bit: usize) -> bool {
        self.words
            .get(bit / 64)
            .is_some_and(|word| word & (1 << (bit % 64)) != 0)
    }

    fn count(&self) -> u32 {
        self.words.iter().map(|word| word.count_ones()).sum()
    }

    /// Adds all bits from `other`, growing the allocation if needed.
    fn union(&mut self, other: &Self) {
        if self.words.len() < other.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        // Extra words in `self` are unchanged, so zip may stop at the end of `other`.
        #[allow(clippy::disallowed_methods)]
        let words = self.words.iter_mut().zip(&other.words);
        for (word, other_word) in words {
            *word |= other_word;
        }
    }

    /// Tests shared indices until `f` returns true or an error.
    fn try_any_common(
        &self,
        other: &Self,
        mut f: impl FnMut(usize) -> anyhow::Result<bool>,
    ) -> anyhow::Result<bool> {
        // Missing words are zero, so the intersection ends with the shorter set.
        #[allow(clippy::disallowed_methods)]
        let words = self.words.iter().zip(&other.words);
        for (word_index, (left, right)) in words.enumerate() {
            let mut common = left & right;
            while common != 0 {
                if f(Self::bit_index(word_index, common)?)? {
                    return Ok(true);
                }
                common = Self::clear_lowest_bit(common)?;
            }
        }
        Ok(false)
    }

    /// Returns the absolute index of the lowest set bit of `word`, which lives in `word_index`.
    fn bit_index(word_index: usize, word: u64) -> anyhow::Result<usize> {
        debug_assert!(word != 0);
        word_index
            .checked_mul(64)
            .and_then(|base| base.checked_add(word.trailing_zeros() as usize))
            .ok_or_else(|| anyhow::anyhow!("BitSet index overflow for word {word_index}"))
    }

    /// Removes the lowest set bit of `word`, which must be non-zero.
    fn clear_lowest_bit(word: u64) -> anyhow::Result<u64> {
        let sub = word
            .checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("clear_lowest_bit called on 0"))?;
        Ok(word & sub)
    }
}

impl Memory {
    fn new() -> Self {
        Self {
            nodes: vec![],
            roots: BTreeMap::new(),
            parent_commands: IndexMap::new(),
            conflict_parents: IndexSet::new(),
        }
    }

    fn node(&self, id: NodeID) -> anyhow::Result<&Node> {
        self.nodes
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Node index {id} is out of bounds"))
    }

    fn node_mut(&mut self, id: NodeID) -> anyhow::Result<&mut Node> {
        self.nodes
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Node index {id} is out of bounds"))
    }

    /// Creates a node whose ancestor set is itself plus every ancestor of its parents, and records
    /// reverse parent-to-child edges for identifying extensions from distinct commands.
    fn new_node(&mut self, kind: NodeKind, parents: &[NodeID]) -> anyhow::Result<NodeID> {
        let id = self.nodes.len();
        let bits = id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Node ID overflow"))?;
        let mut ancestors = BitSet::with_bits(bits);
        anyhow::ensure!(
            !ancestors.set(id)?,
            "Node {id} is already set in its fresh ancestor set"
        );
        for &parent in parents {
            ancestors.union(&self.node(parent)?.ancestors);
        }
        for &parent in parents {
            self.node_mut(parent)?.children.push(id);
        }
        if let NodeKind::Delta { command } = &kind {
            let command = *command as usize;
            for &parent in parents {
                let is_conflict = {
                    let commands = self.parent_commands.entry(parent).or_default();
                    commands.set_grow(command);
                    commands.count() >= 2
                };
                if is_conflict {
                    self.conflict_parents.insert(parent);
                }
            }
        }
        self.nodes.push(Node {
            kind,
            ancestors,
            children: vec![],
        });
        Ok(id)
    }

    /// Returns the unique node for a PTB location, creating it on first use.
    fn root(&mut self, root: RootLocation) -> anyhow::Result<NodeID> {
        if let Some(id) = self.roots.get(&root) {
            return Ok(*id);
        }
        let id = self.new_node(NodeKind::Root, &[])?;
        self.roots.insert(root, id);
        Ok(id)
    }

    /// Returns the node for a PTB location if one has been created
    fn get_root(&self, root: RootLocation) -> Option<NodeID> {
        self.roots.get(&root).copied()
    }

    /// Creates a call-return delta reference, whose ancestors are derived from parent reference
    /// arguments passed to the command.
    /// For immutable references, the parents are all reference arguments.
    /// For mutable references, the parents are the mutable reference arguments.
    fn call_return(&mut self, command: u16, parents: &[NodeID]) -> anyhow::Result<NodeID> {
        self.new_node(NodeKind::Delta { command }, parents)
    }

    /// Returns whether `node` can derive from `ancestor`.
    fn is_ancestor(&self, node: NodeID, ancestor: NodeID) -> anyhow::Result<bool> {
        Ok(self.node(node)?.ancestors.contains(ancestor))
    }

    /// Checks whether `common` has delta children from different commands in the two ancestor sets.
    fn common_has_cross_command(
        &self,
        common: NodeID,
        left_ancestors: &BitSet,
        right_ancestors: &BitSet,
    ) -> anyhow::Result<bool> {
        let mut left_cmd: Option<u16> = None;
        let mut left_multi = false;
        let mut right_cmd: Option<u16> = None;
        let mut right_multi = false;
        for &child in &self.node(common)?.children {
            let NodeKind::Delta { command } = &self.node(child)?.kind else {
                continue;
            };
            let command = *command;
            if left_ancestors.contains(child) {
                match left_cmd {
                    None => left_cmd = Some(command),
                    Some(c) if c != command => left_multi = true,
                    _ => {}
                }
            }
            if right_ancestors.contains(child) {
                match right_cmd {
                    None => right_cmd = Some(command),
                    Some(c) if c != command => right_multi = true,
                    _ => {}
                }
            }
        }
        // Both sides must contain a delta child. If either side contains children from multiple
        // commands, at least one pair must have different commands.
        Ok(match (left_cmd, right_cmd) {
            (Some(l), Some(r)) => left_multi || right_multi || l != r,
            _ => false,
        })
    }

    /// Checks for paths from a shared ancestor through delta children from different commands.
    /// Such paths may overlap; only ancestors in `conflict_parents` need checking.
    fn has_cross_command_extensions(&self, left: NodeID, right: NodeID) -> anyhow::Result<bool> {
        if self.conflict_parents.is_empty() {
            return Ok(false);
        }
        let left_ancestors = &self.node(left)?.ancestors;
        let right_ancestors = &self.node(right)?.ancestors;
        let min_width = left_ancestors.words.len().min(right_ancestors.words.len());
        // Heuristic: estimate scan cost by comparing the number of conflict parents with the number
        // of words in the shorter ancestor bitset.
        if self.conflict_parents.len() <= min_width {
            for &parent in &self.conflict_parents {
                if left_ancestors.contains(parent)
                    && right_ancestors.contains(parent)
                    && self.common_has_cross_command(parent, left_ancestors, right_ancestors)?
                {
                    return Ok(true);
                }
            }
            Ok(false)
        } else {
            left_ancestors.try_any_common(right_ancestors, |common| {
                let is_conflict_parent = self
                    .parent_commands
                    .get(&common)
                    .is_some_and(|commands| commands.count() >= 2);
                Ok(is_conflict_parent
                    && self.common_has_cross_command(common, left_ancestors, right_ancestors)?)
            })
        }
    }

    /// Returns whether `left` may extend `right` through ancestry or cross-command extensions.
    fn extends(&self, left: NodeID, right: NodeID) -> anyhow::Result<bool> {
        Ok((left != right && self.is_ancestor(left, right)?)
            || self.has_cross_command_extensions(left, right)?)
    }

    /// Returns whether neither node derives from the other and neither has cross-command extensions.
    fn is_disjoint(&self, left: NodeID, right: NodeID) -> anyhow::Result<bool> {
        Ok(left != right
            && !self.is_ancestor(left, right)?
            && !self.is_ancestor(right, left)?
            && !self.has_cross_command_extensions(left, right)?)
    }
}

impl Value {
    fn copy(&self) -> Value {
        match self {
            Value::NonRef => Value::NonRef,
            Value::Ref { is_mut, node } => Value::Ref {
                is_mut: *is_mut,
                node: *node,
            },
        }
    }

    fn freeze(&mut self) -> anyhow::Result<Value> {
        match self.copy() {
            Value::NonRef => anyhow::bail!("Cannot freeze a non-reference value"),
            Value::Ref { is_mut, node } => {
                anyhow::ensure!(is_mut, "Cannot freeze an immutable reference");
                Ok(Value::Ref {
                    is_mut: false,
                    node,
                })
            }
        }
    }
}

impl Location {
    fn non_ref(root: RootLocation) -> Self {
        Self {
            root,
            value: Some(Value::NonRef),
        }
    }

    fn copy_value(&self) -> anyhow::Result<Value> {
        self.value
            .as_ref()
            .map(Value::copy)
            .ok_or_else(|| anyhow::anyhow!("Use of invalid memory location"))
    }

    fn move_value(&mut self) -> anyhow::Result<Value> {
        self.value
            .take()
            .ok_or_else(|| anyhow::anyhow!("Use of invalid memory location"))
    }

    fn use_(&mut self, usage: &T::Usage) -> anyhow::Result<Value> {
        match usage {
            T::Usage::Move(_) => self.move_value(),
            T::Usage::Copy { .. } => self.copy_value(),
        }
    }

    fn assert_borrowable(&self) -> anyhow::Result<()> {
        match self.value.as_ref() {
            None => anyhow::bail!("Borrow of invalid memory location"),
            Some(Value::Ref { .. }) => anyhow::bail!("Cannot borrow a reference"),
            Some(Value::NonRef) => Ok(()),
        }
    }
}

impl Context {
    fn new<Mode: ExecutionMode>(_env: &Env<Mode>, txn: &T::Transaction) -> anyhow::Result<Self> {
        let T::Transaction {
            gas_payment,
            bytes: _,
            objects,
            withdrawals,
            pure,
            receiving,
            withdrawal_compatibility_conversions: _,
            original_command_len: _,
            commands: _,
            unified_linkage: _,
        } = txn;
        let memory = Memory::new();
        let tx_context = Location::non_ref(RootLocation::Known(T::Location::TxContext));
        let mut gas = Location::non_ref(RootLocation::Known(T::Location::GasCoin));
        if gas_payment.is_none() {
            gas.move_value()
                .map_err(|_| anyhow::anyhow!("gas coin should be initialized"))?;
        }
        let object_inputs = (0..objects.len())
            .map(|i| {
                Ok(Location::non_ref(RootLocation::Known(
                    T::Location::ObjectInput(checked_as!(i, u16)?),
                )))
            })
            .collect::<anyhow::Result<_>>()?;
        let withdrawal_inputs = (0..withdrawals.len())
            .map(|i| {
                Ok(Location::non_ref(RootLocation::Known(
                    T::Location::WithdrawalInput(checked_as!(i, u16)?),
                )))
            })
            .collect::<anyhow::Result<_>>()?;
        let pure_inputs = (0..pure.len())
            .map(|i| {
                Ok(Location::non_ref(RootLocation::Known(
                    T::Location::PureInput(checked_as!(i, u16)?),
                )))
            })
            .collect::<anyhow::Result<_>>()?;
        let receiving_inputs = (0..receiving.len())
            .map(|i| {
                Ok(Location::non_ref(RootLocation::Known(
                    T::Location::ReceivingInput(checked_as!(i, u16)?),
                )))
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Self {
            memory,
            allow_references_in_ptbs: _env.protocol_config.allow_references_in_ptbs(),
            tx_context,
            gas,
            object_inputs,
            withdrawal_inputs,
            pure_inputs,
            receiving_inputs,
            results: vec![],
            result_ref_rows: vec![],
            arg_reference_nodes: vec![],
        })
    }

    fn current_command(&self) -> anyhow::Result<u16> {
        Ok(checked_as!(self.results.len(), u16)?)
    }

    fn add_result_values(
        &mut self,
        results: impl IntoIterator<Item = Option<Value>>,
    ) -> anyhow::Result<()> {
        let command = self.current_command()?;
        let row = results
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                Ok(Location {
                    root: RootLocation::Known(T::Location::Result(command, checked_as!(i, u16)?)),
                    value: v,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        if row
            .iter()
            .any(|loc| matches!(loc.value, Some(Value::Ref { .. })))
        {
            self.result_ref_rows.push(self.results.len());
        }
        self.results.push(row);
        Ok(())
    }

    fn location(&self, loc: T::Location) -> anyhow::Result<&Location> {
        Ok(match loc {
            T::Location::TxContext => &self.tx_context,
            T::Location::GasCoin => &self.gas,
            T::Location::ObjectInput(i) => self
                .object_inputs
                .get(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Object input index out of bounds {i}"))?,
            T::Location::WithdrawalInput(i) => self
                .withdrawal_inputs
                .get(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Withdrawal input index out of bounds {i}"))?,
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
            T::Location::WithdrawalInput(i) => self
                .withdrawal_inputs
                .get_mut(i as usize)
                .ok_or_else(|| anyhow::anyhow!("Withdrawal input index out of bounds {i}"))?,
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

    /// Returns true iff any live reference borrows `location`.
    fn location_is_borrowed(&self, location: &Location) -> anyhow::Result<bool> {
        match self.memory.get_root(location.root) {
            Some(node) => self.any_extends(node, /* ignore alias */ false),
            None => Ok(false),
        }
    }

    /// Whether the location is borrowed by a reference argument already seen this command, i.e. its
    /// root node is an ancestor of one of `arg_reference_nodes`. A location never borrowed has no
    /// node and cannot be such an ancestor.
    fn borrowed_by_arg_reference(&self, location: T::Location) -> anyhow::Result<bool> {
        let Some(loc_node) = self.memory.get_root(RootLocation::Known(location)) else {
            return Ok(false);
        };
        for &node in &self.arg_reference_nodes {
            if self.memory.is_ancestor(node, loc_node)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn check_usage(&self, usage: &T::Usage, location: &Location) -> anyhow::Result<()> {
        let is_borrowed = self.location_is_borrowed(location)?
            || self.borrowed_by_arg_reference(usage.location())?;
        match usage {
            T::Usage::Move(_) => {
                anyhow::ensure!(!is_borrowed, "Cannot move a value that is borrowed");
            }
            T::Usage::Copy {
                borrowed: borrowed_flag,
                ..
            } => {
                let Some(borrowed_flag) = borrowed_flag.get().copied() else {
                    anyhow::bail!("Borrowed flag not set for copy usage");
                };
                // Drop-safety refinement can release references earlier than the original borrow
                // check. A true borrowed flag may therefore be stale, but a location that is still
                // borrowed must have the flag set.
                if is_borrowed {
                    anyhow::ensure!(
                        borrowed_flag,
                        "Copy of borrowed location {:?} in command {} is not flagged as borrowed",
                        location.root,
                        self.current_command()?
                    );
                }
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
        let value = match arg {
            T::Argument__::Use(usage) => self.location_mut(arg.location())?.use_(usage)?,
            T::Argument__::Freeze(usage) => {
                self.location_mut(arg.location())?.use_(usage)?.freeze()?
            }
            T::Argument__::Borrow(is_mut, _) => self.borrow_location(arg.location(), *is_mut)?,
            T::Argument__::Read(usage) => {
                self.location_mut(arg.location())?.use_(usage)?;
                Value::NonRef
            }
        };
        if let Value::Ref { node, .. } = &value {
            self.arg_reference_nodes.push(*node);
        }
        Ok(value)
    }

    /// Borrows a location, creating its graph node on first use.
    fn borrow_location(&mut self, loc: T::Location, is_mut: bool) -> anyhow::Result<Value> {
        let root = {
            let location = self.location(loc)?;
            location.assert_borrowable()?;
            location.root
        };
        let node = self.memory.root(root)?;
        Ok(Value::Ref { is_mut, node })
    }

    fn arguments(&mut self, args: &[T::Argument]) -> anyhow::Result<Vec<Value>> {
        args.iter()
            .map(|arg| self.argument(arg))
            .collect::<anyhow::Result<Vec<_>>>()
    }

    fn all_references(&self) -> impl Iterator<Item = NodeID> + '_ {
        // Only results can hold references. `tx_context`, `gas`, and every input are created
        // non-reference and never reassigned, so they are not scanned. Among results, only rows
        // produced by reference-returning commands can hold references.
        self.result_ref_rows
            .iter()
            .filter_map(move |&i| self.results.get(i))
            .flatten()
            .filter_map(|v| match v.value.as_ref() {
                Some(Value::Ref { node, .. }) => Some(*node),
                Some(Value::NonRef) | None => None,
            })
    }

    /// Returns whether any live reference may extend `node`. Excludes aliases when requested.
    fn any_extends(&self, node: NodeID, ignore_aliases: bool) -> anyhow::Result<bool> {
        for other in self.all_references() {
            if (!ignore_aliases && other == node) || self.memory.extends(other, node)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Verifies memory safety using an alternative implementation of `verify::memory_safety`.
///
/// It represents memory locations and call returns as graph nodes. Root nodes represent known PTB
/// locations (or in rare cases unknown values from calls without reference arguments). Each node
/// tracks its transitive ancestors in a bitset. A call-return delta records its command identity,
/// analogous to the regex verifier's `.*` extension, while retaining enough identity to use the
/// guarantee that mutable references returned by one call do not overlap with other references
/// returned from that call. Extensions from distinct commands are conservatively treated as
/// potentially overlapping. PTBs have no control flow, which makes this representation sufficient
/// as an invariant check for the regex implementation.
/// Checks the following
/// - Values are not used after being moved
/// - Reference safety is upheld (no dangling references)
pub fn verify<Mode: ExecutionMode>(
    env: &Env<Mode>,
    txn: &T::Transaction,
) -> Result<(), Mode::Error> {
    if !env.protocol_config.memory_safety_invariant_check_v2() {
        return legacy::verify(env, txn);
    }
    Ok(verify_(env, txn).map_err(|e| make_invariant_violation!("{}. Transaction {:?}", e, txn))?)
}

pub(crate) fn verify_<Mode: ExecutionMode>(
    env: &Env<Mode>,
    txn: &T::Transaction,
) -> anyhow::Result<()> {
    let mut context = Context::new(env, txn)?;
    let T::Transaction {
        gas_payment: _,
        bytes: _,
        objects: _,
        withdrawals: _,
        pure: _,
        receiving: _,
        withdrawal_compatibility_conversions: _,
        original_command_len: _,
        commands,
        unified_linkage: _,
    } = txn;
    for c in commands {
        command(&mut context, c)?;
    }
    Ok(())
}

fn command(context: &mut Context, c: &T::Command) -> anyhow::Result<()> {
    debug_assert!(context.arg_reference_nodes.is_empty());
    let results = command_(context, c)?;
    // drop unused result values by marking them as `None`
    assert_invariant!(
        results.len() == c.value.drop_values.len(),
        "result length mismatch. expected {}, got {}",
        c.value.drop_values.len(),
        results.len()
    );
    context.add_result_values(
        results
            .into_iter()
            .zip_debug_eq(c.value.drop_values.iter().copied())
            .map(|(v, drop)| if drop { None } else { Some(v) }),
    )?;
    context.arg_reference_nodes.clear();
    Ok(())
}

fn command_(context: &mut Context, sp!(_, c): &T::Command) -> anyhow::Result<Vec<Value>> {
    let result_tys = &c.result_type;
    let results = match &c.command {
        T::Command__::MoveCall(move_call) => {
            let T::MoveCall {
                function,
                arguments,
            } = &**move_call;
            let arg_values = context.arguments(arguments)?;
            call(context, &function.signature, arg_values)?
        }
        T::Command__::TransferObjects(objs, recipient) => {
            context.arguments(objs)?;
            context.argument(recipient)?;
            non_ref_results(result_tys)?
        }
        T::Command__::SplitCoins(_, coin, amounts) => {
            context.arguments(amounts)?;
            let coin_value = context.argument(coin)?;
            write_ref(context, coin_value)?;
            non_ref_results(result_tys)?
        }
        T::Command__::MergeCoins(_, target, coins) => {
            context.arguments(coins)?;
            let target_value = context.argument(target)?;
            write_ref(context, target_value)?;
            non_ref_results(result_tys)?
        }
        T::Command__::MakeMoveVec(_, arguments) => {
            context.arguments(arguments)?;
            non_ref_results(result_tys)?
        }
        T::Command__::Publish(_, _, _) => non_ref_results(result_tys)?,
        T::Command__::Upgrade(_, _, _, ticket, _) => {
            context.argument(ticket)?;
            non_ref_results(result_tys)?
        }
    };
    assert_invariant!(
        result_tys.len() == results.len(),
        "result length mismatch. Expected {}, got {}",
        result_tys.len(),
        results.len()
    );
    Ok(results)
}

fn write_ref(context: &Context, value: Value) -> anyhow::Result<()> {
    match value {
        Value::NonRef => {
            anyhow::bail!("Cannot write to a non-reference value");
        }

        Value::Ref { is_mut: false, .. } => {
            anyhow::bail!("Cannot write to an immutable reference");
        }
        Value::Ref { is_mut: true, node } => {
            anyhow::ensure!(
                !context.any_extends(node, /* ignore alias */ true)?,
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
) -> anyhow::Result<Vec<Value>> {
    let return_ = &signature.return_;
    let mut all_nodes = Vec::new();
    let mut imm_nodes = Vec::new();
    let mut mut_nodes = Vec::new();
    for arg in arguments {
        match arg {
            Value::NonRef => (),
            Value::Ref { is_mut: true, node } => {
                anyhow::ensure!(
                    !context.any_extends(node, /* ignore alias */ true)?,
                    "Cannot transfer a mutable ref with extensions"
                );
                for other in &mut_nodes {
                    anyhow::ensure!(
                        context.memory.is_disjoint(*other, node)?,
                        "Double mutable borrow"
                    );
                }
                all_nodes.push(node);
                mut_nodes.push(node);
            }
            Value::Ref {
                is_mut: false,
                node,
            } => {
                all_nodes.push(node);
                imm_nodes.push(node);
            }
        }
    }
    // All mutable references must be disjoint from all immutable references
    for immutable in &imm_nodes {
        for mutable in &mut_nodes {
            anyhow::ensure!(
                context.memory.is_disjoint(*immutable, *mutable)?,
                "Mutable and immutable borrows cannot overlap"
            );
        }
    }
    if context.allow_references_in_ptbs {
        // Returned references cannot borrow from TxContext, so exclude arguments derived from it
        // when choosing parents for return nodes. Keep the old behavior when the flag is off:
        // dev-inspect can return references derived from TxContext in that mode.
        if let Some(tx_context_root) = context
            .memory
            .get_root(RootLocation::Known(T::Location::TxContext))
        {
            let mut tx_context_nodes = BTreeSet::new();
            // `mut_nodes` is a subset of `all_nodes`, so all candidates are covered.
            for node in &all_nodes {
                if context.memory.is_ancestor(*node, tx_context_root)? {
                    tx_context_nodes.insert(*node);
                }
            }
            mut_nodes.retain(|node| !tx_context_nodes.contains(node));
            all_nodes.retain(|node| !tx_context_nodes.contains(node));
        }
    }
    let command = context.current_command()?;
    // Reference returns derive from an unknown root when no reference arguments feed them. Calls
    // that return no references need no such node, so avoid creating one.
    let has_reference_return = return_
        .iter()
        .any(|ty| matches!(ty, T::Type::Reference(_, _)));
    let mut_nodes = if mut_nodes.is_empty() && has_reference_return {
        vec![context.memory.root(RootLocation::Unknown { command })?]
    } else {
        mut_nodes
    };
    let all_nodes = if all_nodes.is_empty() && has_reference_return {
        vec![context.memory.root(RootLocation::Unknown { command })?]
    } else {
        all_nodes
    };
    return_
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            let _result = checked_as!(i, u16)?;
            Ok(match ty {
                T::Type::Reference(/* is mut */ true, _) => Value::Ref {
                    is_mut: true,
                    node: context.memory.call_return(command, &mut_nodes)?,
                },
                T::Type::Reference(/* is mut */ false, _) => Value::Ref {
                    is_mut: false,
                    node: context.memory.call_return(command, &all_nodes)?,
                },
                _ => Value::NonRef,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()
}

fn non_ref_results(results: &[T::Type]) -> anyhow::Result<Vec<Value>> {
    results
        .iter()
        .map(|t| {
            anyhow::ensure!(
                !matches!(t, T::Type::Reference(_, _)),
                "attempted to create a non-reference result from a reference type",
            );
            Ok(Value::NonRef)
        })
        .collect()
}

mod legacy {
    use crate::{
        execution_mode::ExecutionMode,
        sp,
        static_programmable_transactions::{env::Env, typing::ast as T},
    };
    use indexmap::IndexSet;
    use mysten_common::ZipDebugEqIteratorExt;
    use std::rc::Rc;
    use sui_types::error::ExecutionError;

    /// An arbitrary extension of a reference, identified by the call and return index.
    /// Distinct returns from one call cannot overlap if either is mutable. Extensions from
    /// different calls may overlap, like `.*` in the regex borrow checker.
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
        allow_references_in_ptbs: bool,
        tx_context: Location,
        gas: Location,
        object_inputs: Vec<Location>,
        withdrawal_inputs: Vec<Location>,
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
                        PathComparison::Prefix
                        | PathComparison::Aliases
                        | PathComparison::Extends => false,
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
        fn new<Mode: ExecutionMode>(env: &Env<Mode>, txn: &T::Transaction) -> anyhow::Result<Self> {
            let T::Transaction {
                gas_payment,
                bytes: _,
                objects,
                withdrawals,
                pure,
                receiving,
                withdrawal_compatibility_conversions: _,
                original_command_len: _,
                commands: _,
                unified_linkage: _,
            } = txn;
            let tx_context = Location::non_ref(T::Location::TxContext);
            let mut gas = Location::non_ref(T::Location::GasCoin);
            if gas_payment.is_none() {
                gas.move_value()
                    .map_err(|_| anyhow::anyhow!("gas coin should be initialized"))?;
            }
            let object_inputs = (0..objects.len())
                .map(|i| {
                    Ok(Location::non_ref(T::Location::ObjectInput(checked_as!(
                        i, u16
                    )?)))
                })
                .collect::<Result<_, ExecutionError>>()?;
            let withdrawal_inputs = (0..withdrawals.len())
                .map(|i| {
                    Ok(Location::non_ref(T::Location::WithdrawalInput(
                        checked_as!(i, u16)?,
                    )))
                })
                .collect::<Result<_, ExecutionError>>()?;
            let pure_inputs = (0..pure.len())
                .map(|i| {
                    Ok(Location::non_ref(T::Location::PureInput(checked_as!(
                        i, u16
                    )?)))
                })
                .collect::<Result<_, ExecutionError>>()?;
            let receiving_inputs = (0..receiving.len())
                .map(|i| {
                    Ok(Location::non_ref(T::Location::ReceivingInput(checked_as!(
                        i, u16
                    )?)))
                })
                .collect::<Result<_, ExecutionError>>()?;
            Ok(Self {
                allow_references_in_ptbs: env.protocol_config.allow_references_in_ptbs(),
                tx_context,
                gas,
                object_inputs,
                withdrawal_inputs,
                pure_inputs,
                receiving_inputs,
                results: vec![],
                arg_roots: IndexSet::new(),
            })
        }

        fn current_command(&self) -> anyhow::Result<u16> {
            Ok(checked_as!(self.results.len(), u16)?)
        }

        fn add_result_values(
            &mut self,
            results: impl IntoIterator<Item = Option<Value>>,
        ) -> anyhow::Result<()> {
            let command = self.current_command()?;
            let allow_references_in_ptbs = self.allow_references_in_ptbs;
            self.results.push(
                results
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        // Post-condition of the source strip in `call`: for path sets, `TxContext`
                        // can never be returned. Gated because flag-off dev-inspect legitimately
                        // produces ctx-rooted results.
                        if allow_references_in_ptbs && let Some(Value::Ref { paths, .. }) = &v {
                            anyhow::ensure!(
                                paths
                                    .0
                                    .iter()
                                    .all(|p| p.root != RootLocation::Known(T::Location::TxContext)),
                                "TxContext can never be returned"
                            );
                        }
                        Ok(Location {
                            self_path: Rc::new(PathSet::initial(T::Location::Result(
                                command,
                                checked_as!(i, u16)?,
                            ))),
                            value: v,
                        })
                    })
                    .collect::<anyhow::Result<_>>()?,
            );
            Ok(())
        }

        fn location(&self, loc: T::Location) -> anyhow::Result<&Location> {
            Ok(match loc {
                T::Location::TxContext => &self.tx_context,
                T::Location::GasCoin => &self.gas,
                T::Location::ObjectInput(i) => self
                    .object_inputs
                    .get(i as usize)
                    .ok_or_else(|| anyhow::anyhow!("Object input index out of bounds {i}"))?,
                T::Location::WithdrawalInput(i) => self
                    .withdrawal_inputs
                    .get(i as usize)
                    .ok_or_else(|| anyhow::anyhow!("Withdrawal input index out of bounds {i}"))?,
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
                T::Location::WithdrawalInput(i) => self
                    .withdrawal_inputs
                    .get_mut(i as usize)
                    .ok_or_else(|| anyhow::anyhow!("Withdrawal input index out of bounds {i}"))?,
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
                        self.current_command()?
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
                allow_references_in_ptbs: _,
                tx_context,
                gas,
                object_inputs,
                withdrawal_inputs,
                pure_inputs,
                receiving_inputs,
                results,
                arg_roots: _,
            } = self;
            std::iter::once(tx_context)
                .chain(std::iter::once(gas))
                .chain(object_inputs)
                .chain(withdrawal_inputs)
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

    /// Independently checks memory safety using sets of paths. Used when
    /// `memory_safety_invariant_check_v2` is disabled.
    ///
    /// Each path starts at a PTB location or an unknown root and records extensions from Move
    /// calls. Each extension identifies the command and return index, allowing distinct returns
    /// from one call to be treated as disjoint when either is mutable. Extensions from different
    /// calls may overlap. PTBs have no control flow, so no path merging across branches is needed.
    ///
    /// Checks for use after move, dangling references, and conflicting borrows.
    pub fn verify<Mode: ExecutionMode>(
        env: &Env<Mode>,
        txn: &T::Transaction,
    ) -> Result<(), Mode::Error> {
        Ok(verify_(env, txn)
            .map_err(|e| make_invariant_violation!("{}. Transaction {:?}", e, txn))?)
    }

    fn verify_<Mode: ExecutionMode>(env: &Env<Mode>, txn: &T::Transaction) -> anyhow::Result<()> {
        if env
            .protocol_config
            .max_ptb_live_references_as_option()
            .is_some()
        {
            return Ok(());
        }
        let mut context = Context::new(env, txn)?;
        let T::Transaction {
            gas_payment: _,
            bytes: _,
            objects: _,
            withdrawals: _,
            pure: _,
            receiving: _,
            withdrawal_compatibility_conversions: _,
            original_command_len: _,
            commands,
            unified_linkage: _,
        } = txn;
        for c in commands {
            command(&mut context, c)?;
        }
        Ok(())
    }

    fn command(context: &mut Context, c: &T::Command) -> anyhow::Result<()> {
        debug_assert!(context.arg_roots.is_empty());
        let results = command_(context, c)?;
        // drop unused result values by marking them as `None`
        assert_invariant!(
            results.len() == c.value.drop_values.len(),
            "result length mismatch. expected {}, got {}",
            c.value.drop_values.len(),
            results.len()
        );
        context.add_result_values(
            results
                .into_iter()
                .zip_debug_eq(c.value.drop_values.iter().copied())
                .map(|(v, drop)| if drop { None } else { Some(v) }),
        )?;
        context.arg_roots.clear();
        Ok(())
    }

    fn command_(context: &mut Context, sp!(_, c): &T::Command) -> anyhow::Result<Vec<Value>> {
        let result_tys = &c.result_type;
        let results = match &c.command {
            T::Command__::MoveCall(move_call) => {
                let T::MoveCall {
                    function,
                    arguments,
                } = &**move_call;
                let arg_values = context.arguments(arguments)?;
                call(context, &function.signature, arg_values)?
            }
            T::Command__::TransferObjects(objs, recipient) => {
                context.arguments(objs)?;
                context.argument(recipient)?;
                non_ref_results(result_tys)?
            }
            T::Command__::SplitCoins(_, coin, amounts) => {
                context.arguments(amounts)?;
                let coin_value = context.argument(coin)?;
                write_ref(context, coin_value)?;
                non_ref_results(result_tys)?
            }
            T::Command__::MergeCoins(_, target, coins) => {
                context.arguments(coins)?;
                let target_value = context.argument(target)?;
                write_ref(context, target_value)?;
                non_ref_results(result_tys)?
            }
            T::Command__::MakeMoveVec(_, arguments) => {
                context.arguments(arguments)?;
                non_ref_results(result_tys)?
            }
            T::Command__::Publish(_, _, _) => non_ref_results(result_tys)?,
            T::Command__::Upgrade(_, _, _, ticket, _) => {
                context.argument(ticket)?;
                non_ref_results(result_tys)?
            }
        };
        assert_invariant!(
            result_tys.len() == results.len(),
            "result length mismatch. Expected {}, got {}",
            result_tys.len(),
            results.len()
        );
        Ok(results)
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
    ) -> anyhow::Result<Vec<Value>> {
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
        // With references enabled in PTBs, TxContext still cannot be the
        // root of a returned reference. We ensure this by removing them from
        // any possible input.
        if context.allow_references_in_ptbs {
            let is_ctx = |p: &Path| p.root == RootLocation::Known(T::Location::TxContext);
            mut_paths.0.retain(|p| !is_ctx(p));
            all_paths.0.retain(|p| !is_ctx(p));
        }
        let command = context.current_command()?;
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
        return_
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                let delta = Delta {
                    command,
                    result: checked_as!(i, u16)?,
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
            .collect::<anyhow::Result<Vec<_>>>()
    }

    fn non_ref_results(results: &[T::Type]) -> anyhow::Result<Vec<Value>> {
        results
            .iter()
            .map(|t| {
                anyhow::ensure!(
                    !matches!(t, T::Type::Reference(_, _)),
                    "attempted to create a non-reference result from a reference type",
                );
                Ok(Value::NonRef)
            })
            .collect()
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
}
