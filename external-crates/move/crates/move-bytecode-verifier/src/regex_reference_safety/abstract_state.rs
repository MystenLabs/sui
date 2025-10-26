// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the abstract state for the type and memory safety analysis.
use crate::absint::{AbstractDomain, FunctionContext, JoinResult};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        CodeOffset, EnumDefinitionIndex, FieldHandleIndex, FunctionDefinitionIndex, LocalIndex,
        MemberCount, Signature, SignatureToken, VariantDefinition, VariantTag,
    },
    safe_assert, safe_unwrap,
};
use move_bytecode_verifier_meter::{Meter, Scope};
use move_core_types::vm_status::StatusCode;
use move_regex_borrow_graph::references::Ref;
use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet},
};

type Graph = move_regex_borrow_graph::collections::Graph<(), Label>;
type Paths = move_regex_borrow_graph::collections::Paths<(), Label>;

/// AbstractValue represents a reference or a non reference value, both on the stack and stored
/// in a local
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AbstractValue {
    Reference(Ref),
    NonReference,
}

/// ValueKind is used for specifying the type of value expected to be returned
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueKind {
    Reference(/* is_mut */ bool),
    NonReference,
}

impl AbstractValue {
    /// checks if self is a reference
    #[allow(unused)]
    pub fn is_ref(&self) -> bool {
        match self {
            AbstractValue::Reference(_) => true,
            AbstractValue::NonReference => false,
        }
    }

    pub fn is_non_ref(&self) -> bool {
        match self {
            AbstractValue::Reference(_) => false,
            AbstractValue::NonReference => true,
        }
    }

    /// possibly extracts ref from self
    pub fn to_ref(self) -> Option<Ref> {
        match self {
            AbstractValue::Reference(r) => Some(r),
            AbstractValue::NonReference => None,
        }
    }
}

impl ValueKind {
    pub fn for_signature(s: &Signature) -> Vec<Self> {
        s.0.iter().map(Self::for_type).collect()
    }

    pub fn for_type(s: &SignatureToken) -> Self {
        match s {
            SignatureToken::Reference(_) => Self::Reference(false),
            SignatureToken::MutableReference(_) => Self::Reference(true),
            SignatureToken::Bool
            | SignatureToken::U8
            | SignatureToken::U64
            | SignatureToken::U128
            | SignatureToken::Address
            | SignatureToken::Signer
            | SignatureToken::Vector(_)
            | SignatureToken::Datatype(_)
            | SignatureToken::DatatypeInstantiation(_)
            | SignatureToken::TypeParameter(_)
            | SignatureToken::U16
            | SignatureToken::U32
            | SignatureToken::U256 => Self::NonReference,
        }
    }
}

/// Label is used to specify path extensions
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Label {
    /// A reference created by borrowing a local variable
    Local(LocalIndex),
    /// A reference that is the struct field extension of another reference
    Field(FieldHandleIndex),
    /// A reference that is the enum field extension of another reference
    VariantField(EnumDefinitionIndex, VariantTag, MemberCount),
}

// Needed for debugging with the borrow graph
impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Label::Local(i) => write!(f, "local#{i}"),
            Label::Field(i) => write!(f, "field#{i}"),
            Label::VariantField(eidx, tag, field_idx) => {
                write!(f, "variant_field#{}#{}#{}", eidx, tag, field_idx)
            }
        }
    }
}

pub(crate) const STEP_BASE_COST: u128 = 1;
pub(crate) const JOIN_BASE_COST: u128 = 10;

pub(crate) const PER_GRAPH_ITEM: u128 = 4;

pub(crate) const JOIN_ITEM_COST: u128 = 4;

/// AbstractState is the analysis state over which abstract interpretation is performed.
#[derive(Clone, Debug)]
pub(crate) struct AbstractState {
    current_function: Option<FunctionDefinitionIndex>,
    local_root: Ref,
    locals: BTreeMap<LocalIndex, Ref>,
    graph: Graph,
}

impl AbstractState {
    /// create a new abstract state
    pub fn new(function_context: &FunctionContext) -> PartialVMResult<Self> {
        let param_refs = function_context
            .parameters()
            .0
            .iter()
            .enumerate()
            .filter_map(|(idx, ty)| {
                let mutable = match ty {
                    SignatureToken::MutableReference(_) => true,
                    SignatureToken::Reference(_) => false,
                    _ => return None,
                };
                let idx = idx as LocalIndex;
                Some((idx, (), mutable))
            });
        let (mut graph, locals) = Graph::new(param_refs)?;
        let local_root = graph.extend_by_epsilon((), std::iter::empty(), /* is_mut */ true)?;

        let mut state = AbstractState {
            current_function: function_context.index(),
            local_root,
            locals,
            graph,
        };
        state.canonicalize()?;
        Ok(state)
    }

    pub(crate) fn graph_size(&self) -> usize {
        self.graph.abstract_size()
    }

    pub(crate) fn reference_size(&self, r: Ref) -> PartialVMResult<usize> {
        Ok(self.graph.reference_size(r)?)
    }

    fn error(&self, status: StatusCode, offset: CodeOffset) -> PartialVMError {
        PartialVMError::new(status).at_code_offset(
            self.current_function.unwrap_or(FunctionDefinitionIndex(0)),
            offset,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn display(&self) {
        self.graph.print();
        println!()
    }

    //**********************************************************************************************
    // Core Predicates
    //**********************************************************************************************

    // Writable if
    // No imm equal
    // No extensions
    fn is_writable(&self, r: Ref, meter: &mut (impl Meter + ?Sized)) -> PartialVMResult<bool> {
        debug_assert!(self.graph.is_mutable(r)?);
        charge_graph_size(self.graph_size(), meter)?;

        Ok(self
            .graph
            .borrowed_by(r)?
            .values()
            .all(|paths| paths.iter().all(|path| path.is_epsilon())))
    }

    // are the references able to be used in a call or return
    fn are_transferrable(
        &self,
        refs: &BTreeSet<Ref>,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<bool> {
        let borrows = refs
            .iter()
            .copied()
            .map(|r| Ok((r, self.graph.borrowed_by(r)?)))
            .collect::<PartialVMResult<BTreeMap<_, _>>>()?;
        charge_graph_size(all_borrowed_by_size(&borrows), meter)?;
        let mut_refs = refs
            .iter()
            .copied()
            .filter_map(|r| match self.graph.is_mutable(r) {
                Ok(true) => Some(Ok(r)),
                Ok(false) => None,
                Err(e) => Some(Err(e.into())),
            })
            .collect::<PartialVMResult<BTreeSet<_>>>()?;
        for (r, borrowed_by) in borrows {
            let is_mut = mut_refs.contains(&r);
            for (borrower, paths) in borrowed_by {
                if !is_mut {
                    if mut_refs.contains(&borrower) {
                        // If the ref is imm, but is borrowed by a mut ref in the set
                        // the mut ref is not transferrable
                        // In other words, the mut ref is an extension of the imm ref
                        return Ok(false);
                    }
                } else {
                    for path in paths {
                        if !path.is_epsilon() || refs.contains(&borrower) {
                            // If the ref is mut, it cannot have any non-epsilon extensions
                            // If extension is epsilon (an alias), it cannot be in the transfer set
                            return Ok(false);
                        }
                    }
                }
            }
        }
        Ok(true)
    }

    /// checks if local#idx is borrowed
    fn is_local_borrowed(
        &self,
        idx: LocalIndex,
        exclude_alias: bool,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<bool> {
        let lbl = Label::Local(idx);
        let borrowed_by = self.graph.borrowed_by(self.local_root)?;
        charge_graph_size(borrowed_by_size(&borrowed_by), meter)?;
        let mut paths = borrowed_by.values().flatten();
        Ok(if exclude_alias {
            // the path starts with the label but is not the label itself
            paths.any(|p| p.starts_with(&lbl) && !p.is_label(&lbl))
        } else {
            // the path starts with the label (possibly nothing more than the label itself)
            paths.any(|p| p.starts_with(&lbl))
        })
    }

    /// checks if any local#_ is borrowed
    fn is_any_local_borrowed(&self, meter: &mut (impl Meter + ?Sized)) -> PartialVMResult<bool> {
        charge_graph_size(self.graph_size(), meter)?;
        Ok(!self.graph.borrowed_by(self.local_root)?.is_empty())
    }

    //**********************************************************************************************
    // Extension
    //**********************************************************************************************

    fn extend_by_epsilon(
        &mut self,
        r: Ref,
        is_mut: bool,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<Ref> {
        let size = self.graph_size();
        let new_r = self
            .graph
            .extend_by_epsilon((), std::iter::once(r), is_mut)?;
        charge_graph_size_increase(size, self.graph_size(), meter)?;
        Ok(new_r)
    }

    fn extend_by_label(
        &mut self,
        r: Ref,
        is_mut: bool,
        extension: Label,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<Ref> {
        let size = self.graph_size();
        let new_r = self
            .graph
            .extend_by_label((), std::iter::once(r), is_mut, extension)?;
        charge_graph_size_increase(size, self.graph_size(), meter)?;
        Ok(new_r)
    }

    fn extend_by_dot_star_for_call(
        &mut self,
        sources: &BTreeSet<Ref>,
        mutabilities: Vec<bool>,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<Vec<Ref>> {
        let size = self.graph_size();
        let new_refs =
            self.graph
                .extend_by_dot_star_for_call((), sources.iter().copied(), mutabilities)?;
        charge_graph_size_increase(size, self.graph_size(), meter)?;
        Ok(new_refs)
    }

    //**********************************************************************************************
    // Instruction Entry Points
    //**********************************************************************************************

    /// destroys local@idx
    pub fn release_value(&mut self, value: AbstractValue) -> PartialVMResult<()> {
        match value {
            AbstractValue::Reference(r) => Ok(self.graph.release(r)?),
            AbstractValue::NonReference => Ok(()),
        }
    }

    pub fn copy_loc(
        &mut self,
        _offset: CodeOffset,
        local: LocalIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        match self.locals.get(&local) {
            Some(r) => {
                let r = *r;
                let new_r = self.extend_by_epsilon(r, self.graph.is_mutable(r)?, meter)?;
                Ok(AbstractValue::Reference(new_r))
            }
            None => Ok(AbstractValue::NonReference),
        }
    }

    pub fn move_loc(
        &mut self,
        offset: CodeOffset,
        local: LocalIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        match self.locals.remove(&local) {
            Some(r) => Ok(AbstractValue::Reference(r)),
            None if self.is_local_borrowed(local, /* exclude alias */ false, meter)? => {
                Err(self.error(StatusCode::MOVELOC_EXISTS_BORROW_ERROR, offset))
            }
            None => Ok(AbstractValue::NonReference),
        }
    }

    pub fn st_loc(
        &mut self,
        offset: CodeOffset,
        local: LocalIndex,
        new_value: AbstractValue,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        if self.is_local_borrowed(local, /* exclude alias */ true, meter)? {
            return Err(self.error(StatusCode::STLOC_UNSAFE_TO_DESTROY_ERROR, offset));
        }

        if let Some(old_r) = self.locals.remove(&local) {
            self.graph.release(old_r)?;
        }
        if let Some(new_r) = new_value.to_ref() {
            let old = self.locals.insert(local, new_r);
            debug_assert!(old.is_none());
        }
        Ok(())
    }

    pub fn freeze_ref(
        &mut self,
        _offset: CodeOffset,
        r: Ref,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        let frozen = self.extend_by_epsilon(r, /* is_mut */ false, meter)?;
        self.graph.release(r)?;
        Ok(AbstractValue::Reference(frozen))
    }

    pub fn comparison(
        &mut self,
        _offset: CodeOffset,
        v1: AbstractValue,
        v2: AbstractValue,
    ) -> PartialVMResult<AbstractValue> {
        self.release_value(v1)?;
        self.release_value(v2)?;
        Ok(AbstractValue::NonReference)
    }

    pub fn read_ref(&mut self, _offset: CodeOffset, r: Ref) -> PartialVMResult<AbstractValue> {
        self.graph.release(r)?;
        Ok(AbstractValue::NonReference)
    }

    pub fn write_ref(
        &mut self,
        offset: CodeOffset,
        r: Ref,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        if !self.is_writable(r, meter)? {
            return Err(self.error(StatusCode::WRITEREF_EXISTS_BORROW_ERROR, offset));
        }

        self.graph.release(r)?;
        Ok(())
    }

    pub fn borrow_loc(
        &mut self,
        _offset: CodeOffset,
        mut_: bool,
        local: LocalIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        let local_root = self.local_root;
        let new_r = self.extend_by_label(local_root, mut_, Label::Local(local), meter)?;
        Ok(AbstractValue::Reference(new_r))
    }

    pub fn borrow_field(
        &mut self,
        _offset: CodeOffset,
        mut_: bool,
        r: Ref,
        field: FieldHandleIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        let new_r = self.extend_by_label(r, mut_, Label::Field(field), meter)?;
        self.graph.release(r)?;
        Ok(AbstractValue::Reference(new_r))
    }

    pub fn unpack_enum_variant_ref(
        &mut self,
        _offset: CodeOffset,
        enum_def_idx: EnumDefinitionIndex,
        variant_tag: VariantTag,
        variant_def: &VariantDefinition,
        mut_: bool,
        r: Ref,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<Vec<AbstractValue>> {
        charge_graph_size(self.reference_size(r)?, meter)?;
        let field_label =
            |field_index| Label::VariantField(enum_def_idx, variant_tag, field_index as u16);
        let field_borrows = variant_def
            .fields
            .iter()
            .enumerate()
            .map(|(field_index, _)| {
                let new_r = self.graph.extend_by_label(
                    (),
                    std::iter::once(r),
                    mut_,
                    field_label(field_index),
                )?;
                Ok(AbstractValue::Reference(new_r))
            })
            .collect::<PartialVMResult<_>>()?;

        self.graph.release(r)?;
        Ok(field_borrows)
    }

    pub fn vector_op(
        &mut self,
        offset: CodeOffset,
        vector: AbstractValue,
        mut_: bool,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        let r = safe_unwrap!(vector.to_ref());
        if mut_ && !self.is_writable(r, meter)? {
            return Err(self.error(StatusCode::VEC_UPDATE_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }
        self.graph.release(r)?;
        Ok(())
    }

    pub fn call(
        &mut self,
        offset: CodeOffset,
        arguments: Vec<AbstractValue>,
        return_: &[ValueKind],
        meter: &mut (impl Meter + ?Sized),
        code: StatusCode,
    ) -> PartialVMResult<Vec<AbstractValue>> {
        charge_graph_size(self.graph_size(), meter)?;
        // Check mutable references can be transferred
        let sources = arguments
            .iter()
            .filter_map(|v| v.to_ref())
            .collect::<BTreeSet<_>>();

        if !self.are_transferrable(&sources, meter)? {
            return Err(self.error(code, offset));
        }
        let mutabilities = return_
            .iter()
            .filter_map(|return_kind| match return_kind {
                ValueKind::Reference(is_mut) => Some(*is_mut),
                ValueKind::NonReference => None,
            })
            .collect::<Vec<_>>();
        let _mutabilities_len = mutabilities.len();
        let mut return_references =
            self.extend_by_dot_star_for_call(&sources, mutabilities, meter)?;
        debug_assert_eq!(return_references.len(), _mutabilities_len);

        let mut return_values: Vec<_> = return_
            .iter()
            .rev()
            .map(|return_kind| match return_kind {
                ValueKind::Reference(_is_mut) => {
                    let new_ref = safe_unwrap!(return_references.pop());
                    debug_assert_eq!(self.graph.is_mutable(new_ref)?, *_is_mut);
                    Ok(AbstractValue::Reference(new_ref))
                }
                ValueKind::NonReference => Ok(AbstractValue::NonReference),
            })
            .collect::<PartialVMResult<Vec<_>>>()?;
        return_values.reverse();
        debug_assert!(return_references.is_empty());

        // Release input references
        for r in sources {
            self.graph.release(r)?
        }
        Ok(return_values)
    }

    pub fn ret(
        &mut self,
        offset: CodeOffset,
        values: Vec<AbstractValue>,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        // release all local variables
        for (_, r) in std::mem::take(&mut self.locals) {
            self.graph.release(r)?
        }

        // Check that no local or global is borrowed
        if self.is_any_local_borrowed(meter)? {
            return Err(self.error(
                StatusCode::UNSAFE_RET_LOCAL_OR_RESOURCE_STILL_BORROWED,
                offset,
            ));
        }

        // Check mutable references can be transferred
        let returned_refs: BTreeSet<Ref> = values.iter().filter_map(|v| v.to_ref()).collect();
        if !self.are_transferrable(&returned_refs, meter)? {
            return Err(self.error(StatusCode::RET_BORROWED_MUTABLE_REFERENCE_ERROR, offset));
        }
        for r in returned_refs {
            self.graph.release(r)?
        }

        Ok(())
    }

    pub fn abort(&mut self) {
        // release all references
        self.locals.clear();
        self.graph.release_all()
    }

    //**********************************************************************************************
    // Abstract Interpreter Entry Points
    //**********************************************************************************************

    pub fn canonicalize(&mut self) -> PartialVMResult<()> {
        self.check_invariants();
        let mut old_to_new = BTreeMap::new();
        old_to_new.insert(self.local_root, 0);
        for (&local, old_r) in &self.locals {
            let new_r = (local as usize) + 1;
            let old_value = old_to_new.insert(*old_r, new_r);
            safe_assert!(old_value.is_none());
        }
        self.local_root = self.local_root.canonicalize(&old_to_new)?;
        for old in self.locals.values_mut() {
            *old = (*old).canonicalize(&old_to_new)?;
        }
        self.graph.canonicalize(&old_to_new)?;
        debug_assert!(self.is_canonical());
        Ok(())
    }

    pub fn refresh(&mut self) -> PartialVMResult<()> {
        self.check_invariants();
        self.local_root = self.local_root.refresh()?;
        for old in self.locals.values_mut() {
            *old = (*old).refresh()?;
        }
        self.graph.refresh_refs()?;
        debug_assert!(self.is_fresh());
        Ok(())
    }

    pub fn is_canonical(&self) -> bool {
        self.local_root.is_canonical()
            && self.locals.values().copied().all(|r| r.is_canonical())
            && self.graph.is_canonical()
    }

    pub fn is_fresh(&self) -> bool {
        self.local_root.is_fresh()
            && self.locals.values().copied().all(|r| r.is_fresh())
            && self.graph.is_fresh()
    }

    pub fn check_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            self.graph.check_invariants();
            debug_assert!(self.is_canonical() || self.is_fresh());
            let refs: BTreeSet<_> = self.graph.keys().collect();
            let mut local_refs = BTreeSet::new();
            for r in self.locals.values() {
                debug_assert_ne!(*r, self.local_root, "local root should not be a local");
                // locals are unique
                debug_assert!(local_refs.insert(*r), "duplicate local ref {r}");
                // all locals are in the graph
                debug_assert!(refs.contains(r), "local ref {r} not in graph");
            }
            // local root should only be gone if the graph is empty from an abort
            debug_assert!(self.graph.abstract_size() == 0 || refs.contains(&self.local_root));
            // the local root is not borrowed by epsilon or dotstar, i.e. all extensions of the
            // local root begin with a label
            if refs.contains(&self.local_root) {
                for (borrower, paths) in self.graph.borrowed_by(self.local_root).unwrap() {
                    debug_assert_ne!(borrower, self.local_root);
                    for path in paths {
                        debug_assert!(!path.is_epsilon());
                        debug_assert!(!path.is_dot_star());
                    }
                }
            }
        }
    }

    pub fn join_(&mut self, other: &Self) -> PartialVMResult</* changed */ bool> {
        let mut changed = false;
        safe_assert!(self.current_function == other.current_function);
        safe_assert!(self.local_root == other.local_root);
        self.check_invariants();
        other.check_invariants();
        safe_assert!(self.is_canonical());
        safe_assert!(other.is_canonical());
        for (local, r) in self.locals.clone() {
            if !other.locals.contains_key(&local) {
                self.locals.remove(&local);
                self.graph.release(r)?;
                changed = true;
            } else {
                safe_assert!(Some(r) == other.locals.get(&local).copied());
            }
        }

        let graph_changed = self.graph.join(&other.graph)?;
        changed = changed || graph_changed;
        safe_assert!(self.is_canonical());
        self.check_invariants();
        Ok(changed)
    }
}

impl AbstractDomain for AbstractState {
    /// attempts to join state to self and returns the result
    fn join(
        &mut self,
        state: &AbstractState,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<JoinResult> {
        meter.add(Scope::Function, JOIN_BASE_COST)?;
        let self_size = self.graph_size();
        let state_size = state.graph_size();
        let changed = Self::join_(self, state)?;
        meter.add(Scope::Function, JOIN_BASE_COST)?;
        let max_size = max(max(self_size, state_size), self.graph_size());
        charge_join(self_size, state_size, meter)?;
        charge_graph_size(max_size, meter)?;
        if changed {
            Ok(JoinResult::Changed)
        } else {
            Ok(JoinResult::Unchanged)
        }
    }
}

fn charge_graph_size(size: usize, meter: &mut (impl Meter + ?Sized)) -> PartialVMResult<()> {
    let size = max(size, 1);
    meter.add_items(Scope::Function, PER_GRAPH_ITEM, size)
}

fn charge_graph_size_increase(
    previous: usize,
    current: usize,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let increase = max(current.saturating_sub(previous), 1);
    charge_graph_size(increase, meter)
}

fn charge_join(
    size1: usize,
    size2: usize,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let size1 = max(size1, 1);
    let size2 = max(size2, 1);
    let size = size1.saturating_add(size2);
    meter.add_items(Scope::Function, JOIN_ITEM_COST, size)
}

fn all_borrowed_by_size(borrows_map: &BTreeMap<Ref, BTreeMap<Ref, Paths>>) -> usize {
    borrows_map
        .iter()
        .flat_map(|(_, paths_map)| paths_map.values())
        .flatten()
        .fold(0, |acc, path| acc.saturating_add(path.abstract_size()))
}

fn borrowed_by_size(paths_map: &BTreeMap<Ref, Paths>) -> usize {
    paths_map
        .iter()
        .flat_map(|(_, paths)| paths)
        .fold(0, |acc, path| acc.saturating_add(path.abstract_size()))
}
