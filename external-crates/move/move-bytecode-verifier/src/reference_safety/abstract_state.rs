// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the abstract state for the type and memory safety analysis.
use crate::{
    absint::{AbstractDomain, JoinResult},
    meter::{Meter, Scope},
};
use move_binary_format::{
    binary_views::FunctionView,
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        CodeOffset, FieldHandleIndex, FunctionDefinitionIndex, LocalIndex, Signature,
        SignatureToken, StructDefinitionIndex,
    },
    safe_unwrap,
};
use move_borrow_graph::references::RefID;
use move_core_types::vm_status::StatusCode;
use std::collections::{BTreeMap, BTreeSet};

type BorrowGraph = move_borrow_graph::graph::BorrowGraph<(), Label>;

/// AbstractValue represents a reference or a non reference value, both on the stack and stored
/// in a local
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AbstractValue {
    Reference(RefID),
    NonReference,
}

impl AbstractValue {
    /// checks if self is a reference
    pub fn is_reference(&self) -> bool {
        match self {
            AbstractValue::Reference(_) => true,
            AbstractValue::NonReference => false,
        }
    }

    /// checks if self is a value
    pub fn is_value(&self) -> bool {
        !self.is_reference()
    }

    /// possibly extracts id from self
    pub fn ref_id(&self) -> Option<RefID> {
        match self {
            AbstractValue::Reference(id) => Some(*id),
            AbstractValue::NonReference => None,
        }
    }
}

/// Label is an element of a label on an edge in the borrow graph.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Label {
    Local(LocalIndex),
    Global(StructDefinitionIndex),
    Field(FieldHandleIndex),
}

// Needed for debugging with the borrow graph
impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Label::Local(i) => write!(f, "local#{}", i),
            Label::Global(i) => write!(f, "resource@{}", i),
            Label::Field(i) => write!(f, "field#{}", i),
        }
    }
}

pub(crate) const STEP_BASE_COST: u128 = 10;
pub(crate) const STEP_PER_LOCAL_COST: u128 = 20;
pub(crate) const STEP_PER_GRAPH_ITEM_COST: u128 = 50;
pub(crate) const JOIN_BASE_COST: u128 = 100;
pub(crate) const JOIN_PER_LOCAL_COST: u128 = 10;
pub(crate) const JOIN_PER_GRAPH_ITEM_COST: u128 = 50;

// The cost for an edge from an input reference parameter to output reference.
pub(crate) const REF_PARAM_EDGE_COST: u128 = 100;
pub(crate) const REF_PARAM_EDGE_COST_GROWTH: f32 = 1.5;

// The cost of an acquires in a call.
pub(crate) const CALL_PER_ACQUIRES_COST: u128 = 100;

/// AbstractState is the analysis state over which abstract interpretation is performed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AbstractState {
    current_function: Option<FunctionDefinitionIndex>,
    locals: Vec<AbstractValue>,
    borrow_graph: BorrowGraph,
    next_id: usize,
}

impl AbstractState {
    /// create a new abstract state
    pub fn new(function_view: &FunctionView) -> Self {
        let num_locals = function_view.parameters().len() + function_view.locals().len();
        // ids in [0, num_locals) are reserved for constructing canonical state
        // id at num_locals is reserved for the frame root
        let next_id = num_locals + 1;
        let mut state = AbstractState {
            current_function: function_view.index(),
            locals: vec![AbstractValue::NonReference; num_locals],
            borrow_graph: BorrowGraph::new(),
            next_id,
        };

        for (param_idx, param_ty) in function_view.parameters().0.iter().enumerate() {
            if param_ty.is_reference() {
                let id = RefID::new(param_idx);
                state
                    .borrow_graph
                    .new_ref(id, param_ty.is_mutable_reference());
                state.locals[param_idx] = AbstractValue::Reference(id)
            }
        }
        state.borrow_graph.new_ref(state.frame_root(), true);

        assert!(state.is_canonical());
        state
    }

    pub(crate) fn local_count(&self) -> usize {
        self.locals.len()
    }

    pub(crate) fn graph_size(&self) -> usize {
        self.borrow_graph.graph_size()
    }

    /// returns the frame root id
    fn frame_root(&self) -> RefID {
        RefID::new(self.locals.len())
    }

    fn error(&self, status: StatusCode, offset: CodeOffset) -> PartialVMError {
        PartialVMError::new(status).at_code_offset(
            self.current_function.unwrap_or(FunctionDefinitionIndex(0)),
            offset,
        )
    }

    //**********************************************************************************************
    // Core API
    //**********************************************************************************************

    pub fn value_for(&mut self, s: &SignatureToken) -> AbstractValue {
        match s {
            SignatureToken::Reference(_) => AbstractValue::Reference(self.new_ref(false)),
            SignatureToken::MutableReference(_) => AbstractValue::Reference(self.new_ref(true)),
            _ => AbstractValue::NonReference,
        }
    }

    /// adds and returns new id to borrow graph
    fn new_ref(&mut self, mut_: bool) -> RefID {
        let id = RefID::new(self.next_id);
        self.borrow_graph.new_ref(id, mut_);
        self.next_id += 1;
        id
    }

    fn add_copy(&mut self, parent: RefID, child: RefID) {
        self.borrow_graph.add_strong_borrow((), parent, child)
    }

    fn add_borrow(&mut self, parent: RefID, child: RefID) {
        self.borrow_graph.add_weak_borrow((), parent, child)
    }

    fn add_field_borrow(&mut self, parent: RefID, field: FieldHandleIndex, child: RefID) {
        self.borrow_graph
            .add_strong_field_borrow((), parent, Label::Field(field), child)
    }

    fn add_local_borrow(&mut self, local: LocalIndex, id: RefID) {
        self.borrow_graph
            .add_strong_field_borrow((), self.frame_root(), Label::Local(local), id)
    }

    fn add_resource_borrow(&mut self, resource: StructDefinitionIndex, id: RefID) {
        self.borrow_graph
            .add_weak_field_borrow((), self.frame_root(), Label::Global(resource), id)
    }

    /// removes `id` from borrow graph
    fn release(&mut self, id: RefID) {
        self.borrow_graph.release(id);
    }

    //**********************************************************************************************
    // Core Predicates
    //**********************************************************************************************

    /// checks if `id` is borrowed, but ignores field borrows
    fn has_full_borrows(&self, id: RefID) -> bool {
        let (full_borrows, _field_borrows) = self.borrow_graph.borrowed_by(id);
        !full_borrows.is_empty()
    }

    /// Checks if `id` is borrowed
    /// - All full/epsilon borrows are considered
    /// - Only field borrows the specified label (or all if one isn't specified) are considered
    fn has_consistent_borrows(&self, id: RefID, label_opt: Option<Label>) -> bool {
        let (full_borrows, field_borrows) = self.borrow_graph.borrowed_by(id);
        !full_borrows.is_empty() || {
            match label_opt {
                None => field_borrows.values().any(|borrows| !borrows.is_empty()),
                Some(label) => field_borrows
                    .get(&label)
                    .map(|borrows| !borrows.is_empty())
                    .unwrap_or(false),
            }
        }
    }

    /// Checks if `id` is mutable borrowed
    /// - All full/epsilon mutable borrows are considered
    /// - Only field mutable borrows the specified label (or all if one isn't specified) are
    ///   considered
    fn has_consistent_mutable_borrows(&self, id: RefID, label_opt: Option<Label>) -> bool {
        let (full_borrows, field_borrows) = self.borrow_graph.borrowed_by(id);
        !self.all_immutable(&full_borrows) || {
            match label_opt {
                None => field_borrows
                    .values()
                    .any(|borrows| !self.all_immutable(borrows)),
                Some(label) => field_borrows
                    .get(&label)
                    .map(|borrows| !self.all_immutable(borrows))
                    .unwrap_or(false),
            }
        }
    }

    /// checks if `id` is writable
    /// - Mutable references are writable if there are no consistent borrows
    /// - Immutable references are not writable by the typing rules
    fn is_writable(&self, id: RefID) -> bool {
        assert!(self.borrow_graph.is_mutable(id));
        !self.has_consistent_borrows(id, None)
    }

    /// checks if `id` is freezable
    /// - Mutable references are freezable if there are no consistent mutable borrows
    /// - Immutable references are not freezable by the typing rules
    fn is_freezable(&self, id: RefID, at_field_opt: Option<FieldHandleIndex>) -> bool {
        assert!(self.borrow_graph.is_mutable(id));
        !self.has_consistent_mutable_borrows(id, at_field_opt.map(Label::Field))
    }

    /// checks if `id` is readable
    /// - Mutable references are readable if they are freezable
    /// - Immutable references are always readable
    fn is_readable(&self, id: RefID, at_field_opt: Option<FieldHandleIndex>) -> bool {
        let is_mutable = self.borrow_graph.is_mutable(id);
        !is_mutable || self.is_freezable(id, at_field_opt)
    }

    /// checks if local@idx is borrowed
    fn is_local_borrowed(&self, idx: LocalIndex) -> bool {
        self.has_consistent_borrows(self.frame_root(), Some(Label::Local(idx)))
    }

    /// checks if local@idx is mutably borrowed
    fn is_local_mutably_borrowed(&self, idx: LocalIndex) -> bool {
        self.has_consistent_mutable_borrows(self.frame_root(), Some(Label::Local(idx)))
    }

    /// checks if global@idx is borrowed
    fn is_global_borrowed(&self, resource: StructDefinitionIndex) -> bool {
        self.has_consistent_borrows(self.frame_root(), Some(Label::Global(resource)))
    }

    /// checks if global@idx is mutably borrowed
    fn is_global_mutably_borrowed(&self, resource: StructDefinitionIndex) -> bool {
        self.has_consistent_mutable_borrows(self.frame_root(), Some(Label::Global(resource)))
    }

    /// checks if the stack frame of the function being analyzed can be safely destroyed.
    /// safe destruction requires that all references in locals have already been destroyed
    /// and all values in locals are copyable and unborrowed.
    fn is_frame_safe_to_destroy(&self) -> bool {
        !self.has_consistent_borrows(self.frame_root(), None)
    }

    //**********************************************************************************************
    // Instruction Entry Points
    //**********************************************************************************************

    /// destroys local@idx
    pub fn release_value(&mut self, value: AbstractValue) {
        match value {
            AbstractValue::Reference(id) => self.release(id),
            AbstractValue::NonReference => (),
        }
    }

    pub fn copy_loc(
        &mut self,
        offset: CodeOffset,
        local: LocalIndex,
    ) -> PartialVMResult<AbstractValue> {
        match safe_unwrap!(self.locals.get(local as usize)) {
            AbstractValue::Reference(id) => {
                let id = *id;
                let new_id = self.new_ref(self.borrow_graph.is_mutable(id));
                self.add_copy(id, new_id);
                Ok(AbstractValue::Reference(new_id))
            }
            AbstractValue::NonReference if self.is_local_mutably_borrowed(local) => {
                Err(self.error(StatusCode::COPYLOC_EXISTS_BORROW_ERROR, offset))
            }
            AbstractValue::NonReference => Ok(AbstractValue::NonReference),
        }
    }

    pub fn move_loc(
        &mut self,
        offset: CodeOffset,
        local: LocalIndex,
    ) -> PartialVMResult<AbstractValue> {
        let old_value = std::mem::replace(
            safe_unwrap!(self.locals.get_mut(local as usize)),
            AbstractValue::NonReference,
        );
        match old_value {
            AbstractValue::Reference(id) => Ok(AbstractValue::Reference(id)),
            AbstractValue::NonReference if self.is_local_borrowed(local) => {
                Err(self.error(StatusCode::MOVELOC_EXISTS_BORROW_ERROR, offset))
            }
            AbstractValue::NonReference => Ok(AbstractValue::NonReference),
        }
    }

    pub fn st_loc(
        &mut self,
        offset: CodeOffset,
        local: LocalIndex,
        new_value: AbstractValue,
    ) -> PartialVMResult<()> {
        let old_value =
            std::mem::replace(safe_unwrap!(self.locals.get_mut(local as usize)), new_value);
        match old_value {
            AbstractValue::Reference(id) => {
                self.release(id);
                Ok(())
            }
            AbstractValue::NonReference if self.is_local_borrowed(local) => {
                Err(self.error(StatusCode::STLOC_UNSAFE_TO_DESTROY_ERROR, offset))
            }
            AbstractValue::NonReference => Ok(()),
        }
    }

    pub fn freeze_ref(&mut self, offset: CodeOffset, id: RefID) -> PartialVMResult<AbstractValue> {
        if !self.is_freezable(id, None) {
            return Err(self.error(StatusCode::FREEZEREF_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }

        let frozen_id = self.new_ref(false);
        self.add_copy(id, frozen_id);
        self.release(id);
        Ok(AbstractValue::Reference(frozen_id))
    }

    pub fn comparison(
        &mut self,
        offset: CodeOffset,
        v1: AbstractValue,
        v2: AbstractValue,
    ) -> PartialVMResult<AbstractValue> {
        match (v1, v2) {
            (AbstractValue::Reference(id1), AbstractValue::Reference(id2))
                if !self.is_readable(id1, None) || !self.is_readable(id2, None) =>
            {
                // TODO better error code
                return Err(self.error(StatusCode::READREF_EXISTS_MUTABLE_BORROW_ERROR, offset));
            }
            (AbstractValue::Reference(id1), AbstractValue::Reference(id2)) => {
                self.release(id1);
                self.release(id2)
            }
            (v1, v2) => {
                assert!(v1.is_value());
                assert!(v2.is_value());
            }
        }
        Ok(AbstractValue::NonReference)
    }

    pub fn read_ref(&mut self, offset: CodeOffset, id: RefID) -> PartialVMResult<AbstractValue> {
        if !self.is_readable(id, None) {
            return Err(self.error(StatusCode::READREF_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }

        self.release(id);
        Ok(AbstractValue::NonReference)
    }

    pub fn write_ref(&mut self, offset: CodeOffset, id: RefID) -> PartialVMResult<()> {
        if !self.is_writable(id) {
            return Err(self.error(StatusCode::WRITEREF_EXISTS_BORROW_ERROR, offset));
        }

        self.release(id);
        Ok(())
    }

    pub fn borrow_loc(
        &mut self,
        offset: CodeOffset,
        mut_: bool,
        local: LocalIndex,
    ) -> PartialVMResult<AbstractValue> {
        // nothing to check in case borrow is mutable since the frame cannot have an full borrow/
        // epsilon outgoing edge
        if !mut_ && self.is_local_mutably_borrowed(local) {
            return Err(self.error(StatusCode::BORROWLOC_EXISTS_BORROW_ERROR, offset));
        }

        let new_id = self.new_ref(mut_);
        self.add_local_borrow(local, new_id);
        Ok(AbstractValue::Reference(new_id))
    }

    pub fn borrow_field(
        &mut self,
        offset: CodeOffset,
        mut_: bool,
        id: RefID,
        field: FieldHandleIndex,
    ) -> PartialVMResult<AbstractValue> {
        // Any field borrows will be factored out, so don't check in the mutable case
        let is_mut_borrow_with_full_borrows = || mut_ && self.has_full_borrows(id);
        // For new immutable borrow, the reference must be readable at that field
        // This means that there could exist a mutable borrow on some other field
        let is_imm_borrow_with_mut_borrows = || !mut_ && !self.is_readable(id, Some(field));

        if is_mut_borrow_with_full_borrows() || is_imm_borrow_with_mut_borrows() {
            // TODO improve error for mutable case
            return Err(self.error(StatusCode::BORROWFIELD_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }

        let field_borrow_id = self.new_ref(mut_);
        self.add_field_borrow(id, field, field_borrow_id);
        self.release(id);
        Ok(AbstractValue::Reference(field_borrow_id))
    }

    pub fn borrow_global(
        &mut self,
        offset: CodeOffset,
        mut_: bool,
        resource: StructDefinitionIndex,
    ) -> PartialVMResult<AbstractValue> {
        if (mut_ && self.is_global_borrowed(resource)) || self.is_global_mutably_borrowed(resource)
        {
            return Err(self.error(StatusCode::GLOBAL_REFERENCE_ERROR, offset));
        }

        let new_id = self.new_ref(mut_);
        self.add_resource_borrow(resource, new_id);
        Ok(AbstractValue::Reference(new_id))
    }

    pub fn move_from(
        &mut self,
        offset: CodeOffset,
        resource: StructDefinitionIndex,
    ) -> PartialVMResult<AbstractValue> {
        if self.is_global_borrowed(resource) {
            Err(self.error(StatusCode::GLOBAL_REFERENCE_ERROR, offset))
        } else {
            Ok(AbstractValue::NonReference)
        }
    }

    pub fn vector_op(
        &mut self,
        offset: CodeOffset,
        vector: AbstractValue,
        mut_: bool,
    ) -> PartialVMResult<()> {
        let id = safe_unwrap!(vector.ref_id());
        if mut_ && !self.is_writable(id) {
            return Err(self.error(StatusCode::VEC_UPDATE_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }
        self.release(id);
        Ok(())
    }

    pub fn vector_element_borrow(
        &mut self,
        offset: CodeOffset,
        vector: AbstractValue,
        mut_: bool,
    ) -> PartialVMResult<AbstractValue> {
        let vec_id = safe_unwrap!(vector.ref_id());
        if mut_ && !self.is_writable(vec_id) {
            return Err(self.error(
                StatusCode::VEC_BORROW_ELEMENT_EXISTS_MUTABLE_BORROW_ERROR,
                offset,
            ));
        }

        let elem_id = self.new_ref(mut_);
        self.add_borrow(vec_id, elem_id);

        self.release(vec_id);
        Ok(AbstractValue::Reference(elem_id))
    }

    pub fn call(
        &mut self,
        offset: CodeOffset,
        arguments: Vec<AbstractValue>,
        acquired_resources: &BTreeSet<StructDefinitionIndex>,
        return_: &Signature,
        meter: &mut impl Meter,
    ) -> PartialVMResult<Vec<AbstractValue>> {
        meter.add_items(
            Scope::Function,
            CALL_PER_ACQUIRES_COST,
            acquired_resources.len(),
        )?;
        // Check acquires
        for acquired_resource in acquired_resources {
            if self.is_global_borrowed(*acquired_resource) {
                return Err(self.error(StatusCode::GLOBAL_REFERENCE_ERROR, offset));
            }
        }

        // Check mutable references can be transfered
        let mut all_references_to_borrow_from = BTreeSet::new();
        let mut mutable_references_to_borrow_from = BTreeSet::new();
        for id in arguments.iter().filter_map(|v| v.ref_id()) {
            if self.borrow_graph.is_mutable(id) {
                if !self.is_writable(id) {
                    return Err(
                        self.error(StatusCode::CALL_BORROWED_MUTABLE_REFERENCE_ERROR, offset)
                    );
                }
                mutable_references_to_borrow_from.insert(id);
            }
            all_references_to_borrow_from.insert(id);
        }

        // Track borrow relationships of return values on inputs
        let mut returned_refs = 0;
        let return_values = return_
            .0
            .iter()
            .map(|return_type| match return_type {
                SignatureToken::MutableReference(_) => {
                    let id = self.new_ref(true);
                    for parent in &mutable_references_to_borrow_from {
                        self.add_borrow(*parent, id);
                    }
                    returned_refs += 1;
                    AbstractValue::Reference(id)
                }
                SignatureToken::Reference(_) => {
                    let id = self.new_ref(false);
                    for parent in &all_references_to_borrow_from {
                        self.add_borrow(*parent, id);
                    }
                    returned_refs += 1;
                    AbstractValue::Reference(id)
                }
                _ => AbstractValue::NonReference,
            })
            .collect();

        // Meter usage of reference edges
        meter.add_items_with_growth(
            Scope::Function,
            REF_PARAM_EDGE_COST,
            all_references_to_borrow_from
                .len()
                .saturating_mul(returned_refs),
            REF_PARAM_EDGE_COST_GROWTH,
        )?;

        // Release input references
        for id in all_references_to_borrow_from {
            self.release(id)
        }
        Ok(return_values)
    }

    pub fn ret(&mut self, offset: CodeOffset, values: Vec<AbstractValue>) -> PartialVMResult<()> {
        // release all local variables
        let mut released = BTreeSet::new();
        for stored_value in self.locals.iter() {
            if let AbstractValue::Reference(id) = stored_value {
                released.insert(*id);
            }
        }
        released.into_iter().for_each(|id| self.release(id));

        // Check that no local or global is borrowed
        if !self.is_frame_safe_to_destroy() {
            return Err(self.error(
                StatusCode::UNSAFE_RET_LOCAL_OR_RESOURCE_STILL_BORROWED,
                offset,
            ));
        }

        // Check mutable references can be transfered
        for id in values.into_iter().filter_map(|v| v.ref_id()) {
            if self.borrow_graph.is_mutable(id) && !self.is_writable(id) {
                return Err(self.error(StatusCode::RET_BORROWED_MUTABLE_REFERENCE_ERROR, offset));
            }
        }
        Ok(())
    }

    //**********************************************************************************************
    // Abstract Interpreter Entry Points
    //**********************************************************************************************

    /// returns the canonical representation of self
    pub fn construct_canonical_state(&self) -> Self {
        let mut id_map = BTreeMap::new();
        id_map.insert(self.frame_root(), self.frame_root());
        let locals = self
            .locals
            .iter()
            .enumerate()
            .map(|(local, value)| match value {
                AbstractValue::Reference(old_id) => {
                    let new_id = RefID::new(local);
                    id_map.insert(*old_id, new_id);
                    AbstractValue::Reference(new_id)
                }
                AbstractValue::NonReference => AbstractValue::NonReference,
            })
            .collect::<Vec<_>>();
        assert!(self.locals.len() == locals.len());
        let mut borrow_graph = self.borrow_graph.clone();
        borrow_graph.remap_refs(&id_map);
        let canonical_state = AbstractState {
            locals,
            borrow_graph,
            current_function: self.current_function,
            next_id: self.locals.len() + 1,
        };
        assert!(canonical_state.is_canonical());
        canonical_state
    }

    fn all_immutable(&self, borrows: &BTreeMap<RefID, ()>) -> bool {
        !borrows.keys().any(|x| self.borrow_graph.is_mutable(*x))
    }

    fn is_canonical(&self) -> bool {
        self.locals.len() + 1 == self.next_id
            && self.locals.iter().enumerate().all(|(local, value)| {
                value
                    .ref_id()
                    .map(|id| RefID::new(local) == id)
                    .unwrap_or(true)
            })
    }

    pub fn join_(&self, other: &Self) -> Self {
        assert!(self.current_function == other.current_function);
        assert!(self.is_canonical() && other.is_canonical());
        assert!(self.next_id == other.next_id);
        assert!(self.locals.len() == other.locals.len());
        let mut self_graph = self.borrow_graph.clone();
        let mut other_graph = other.borrow_graph.clone();
        let locals = self
            .locals
            .iter()
            .zip(&other.locals)
            .map(|(self_value, other_value)| {
                match (self_value, other_value) {
                    (AbstractValue::Reference(id), AbstractValue::NonReference) => {
                        self_graph.release(*id);
                        AbstractValue::NonReference
                    }
                    (AbstractValue::NonReference, AbstractValue::Reference(id)) => {
                        other_graph.release(*id);
                        AbstractValue::NonReference
                    }
                    // The local has a value on each side, add it to the state
                    (v1, v2) => {
                        assert!(v1 == v2);
                        *v1
                    }
                }
            })
            .collect();

        let borrow_graph = self_graph.join(&other_graph);
        let current_function = self.current_function;
        let next_id = self.next_id;

        Self {
            current_function,
            locals,
            borrow_graph,
            next_id,
        }
    }
}

impl AbstractDomain for AbstractState {
    /// attempts to join state to self and returns the result
    fn join(
        &mut self,
        state: &AbstractState,
        meter: &mut impl Meter,
    ) -> PartialVMResult<JoinResult> {
        let joined = Self::join_(self, state);
        assert!(joined.is_canonical());
        assert!(self.locals.len() == joined.locals.len());
        meter.add(Scope::Function, JOIN_BASE_COST)?;
        meter.add_items(Scope::Function, JOIN_PER_LOCAL_COST, self.locals.len())?;
        meter.add_items(
            Scope::Function,
            JOIN_PER_GRAPH_ITEM_COST,
            self.borrow_graph.graph_size(),
        )?;
        let locals_unchanged = self
            .locals
            .iter()
            .zip(&joined.locals)
            .all(|(self_value, joined_value)| self_value == joined_value);
        // locals unchanged and borrow graph covered, return unchanged
        // else mark as changed and update the state
        if locals_unchanged && self.borrow_graph.leq(&joined.borrow_graph) {
            Ok(JoinResult::Unchanged)
        } else {
            *self = joined;
            Ok(JoinResult::Changed)
        }
    }
}
