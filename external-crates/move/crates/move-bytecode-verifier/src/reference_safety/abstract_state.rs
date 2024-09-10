// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines the abstract state for the type and memory safety analysis.
use move_abstract_interpreter::absint::{AbstractDomain, FunctionContext, JoinResult};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        CodeOffset, EnumDefinitionIndex, FieldHandleIndex, FunctionDefinitionIndex, LocalIndex,
        MemberCount, Signature, SignatureToken, StructDefinitionIndex, VariantDefinition,
        VariantTag,
    },
    safe_unwrap,
};
use move_borrow_graph::references::RefID;
use move_bytecode_verifier_meter::{Meter, Scope};
use move_core_types::vm_status::StatusCode;
use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet},
};

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
    StructField(FieldHandleIndex),
    VariantField(EnumDefinitionIndex, VariantTag, MemberCount),
}

// Needed for debugging with the borrow graph
impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Label::Local(i) => write!(f, "local#{}", i),
            Label::Global(i) => write!(f, "resource@{}", i),
            Label::StructField(i) => write!(f, "struct_field#{}", i),
            Label::VariantField(eidx, tag, field_idx) => {
                write!(f, "variant_field#{}#{}#{}", eidx, tag, field_idx)
            }
        }
    }
}

pub(crate) const STEP_BASE_COST: u128 = 1;
pub(crate) const JOIN_BASE_COST: u128 = 10;

pub(crate) const PER_GRAPH_ITEM_COST: u128 = 4;

pub(crate) const RELEASE_ITEM_COST: u128 = 3;
pub(crate) const RELEASE_ITEM_QUADRATIC_THRESHOLD: usize = 5;

pub(crate) const JOIN_ITEM_COST: u128 = 4;
pub(crate) const JOIN_ITEM_QUADRATIC_THRESHOLD: usize = 10;

pub(crate) const ADD_BORROW_COST: u128 = 3;

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
    pub fn new(function_context: &FunctionContext) -> Self {
        let num_locals = function_context.parameters().len() + function_context.locals().len();
        // ids in [0, num_locals) are reserved for constructing canonical state
        // id at num_locals is reserved for the frame root
        let next_id = num_locals + 1;
        let mut state = AbstractState {
            current_function: function_context.index(),
            locals: vec![AbstractValue::NonReference; num_locals],
            borrow_graph: BorrowGraph::new(),
            next_id,
        };

        for (param_idx, param_ty) in function_context.parameters().0.iter().enumerate() {
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

    fn add_copy(
        &mut self,
        parent: RefID,
        child: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        meter.add(Scope::Function, ADD_BORROW_COST)?;
        self.borrow_graph.add_strong_borrow((), parent, child);
        Ok(())
    }

    fn add_borrow(
        &mut self,
        parent: RefID,
        child: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        meter.add(Scope::Function, ADD_BORROW_COST)?;
        self.borrow_graph.add_weak_borrow((), parent, child);
        Ok(())
    }

    fn add_field_borrow(
        &mut self,
        parent: RefID,
        field: FieldHandleIndex,
        child: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        meter.add(Scope::Function, ADD_BORROW_COST)?;
        self.borrow_graph
            .add_strong_field_borrow((), parent, Label::StructField(field), child);
        Ok(())
    }

    fn add_local_borrow(
        &mut self,
        local: LocalIndex,
        id: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        meter.add(Scope::Function, ADD_BORROW_COST)?;
        self.borrow_graph
            .add_strong_field_borrow((), self.frame_root(), Label::Local(local), id);
        Ok(())
    }

    fn add_resource_borrow(
        &mut self,
        resource: StructDefinitionIndex,
        id: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        meter.add(Scope::Function, ADD_BORROW_COST)?;
        self.borrow_graph
            .add_weak_field_borrow((), self.frame_root(), Label::Global(resource), id);
        Ok(())
    }

    fn add_variant_field_borrow(
        &mut self,
        parent: RefID,
        enum_def_idx: EnumDefinitionIndex,
        variant_tag: VariantTag,
        field_index: MemberCount,
        child_id: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        meter.add(Scope::Function, ADD_BORROW_COST)?;
        self.borrow_graph.add_strong_field_borrow(
            (),
            parent,
            Label::VariantField(enum_def_idx, variant_tag, field_index),
            child_id,
        );
        Ok(())
    }

    /// removes `id` from borrow graph
    fn release(&mut self, id: RefID, meter: &mut (impl Meter + ?Sized)) -> PartialVMResult<()> {
        let released_edges = self.borrow_graph.release(id);
        charge_release(released_edges, meter)
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
    fn is_writable(&self, id: RefID, meter: &mut (impl Meter + ?Sized)) -> PartialVMResult<bool> {
        assert!(self.borrow_graph.is_mutable(id));
        charge_graph_size(self.graph_size(), meter)?;
        Ok(!self.has_consistent_borrows(id, None))
    }

    /// checks if `id` is freezable
    /// - Mutable references are freezable if there are no consistent mutable borrows
    /// - Immutable references are not freezable by the typing rules
    fn is_freezable(
        &self,
        id: RefID,
        at_field_opt: Option<FieldHandleIndex>,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<bool> {
        assert!(self.borrow_graph.is_mutable(id));
        charge_graph_size(self.graph_size(), meter)?;
        Ok(!self.has_consistent_mutable_borrows(id, at_field_opt.map(Label::StructField)))
    }

    /// checks if `id` is readable
    /// - Mutable references are readable if they are freezable
    /// - Immutable references are always readable
    fn is_readable(
        &self,
        id: RefID,
        at_field_opt: Option<FieldHandleIndex>,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<bool> {
        let is_mutable = self.borrow_graph.is_mutable(id);
        Ok(!is_mutable || self.is_freezable(id, at_field_opt, meter)?)
    }

    /// checks if local@idx is borrowed
    fn is_local_borrowed(
        &self,
        idx: LocalIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<bool> {
        charge_graph_size(self.graph_size(), meter)?;
        Ok(self.has_consistent_borrows(self.frame_root(), Some(Label::Local(idx))))
    }

    /// checks if local@idx is mutably borrowed
    fn is_local_mutably_borrowed(
        &self,
        idx: LocalIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<bool> {
        charge_graph_size(self.graph_size(), meter)?;
        Ok(self.has_consistent_mutable_borrows(self.frame_root(), Some(Label::Local(idx))))
    }

    /// checks if global@idx is borrowed
    fn is_global_borrowed(
        &self,
        resource: StructDefinitionIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<bool> {
        charge_graph_size(self.graph_size(), meter)?;
        Ok(self.has_consistent_borrows(self.frame_root(), Some(Label::Global(resource))))
    }

    /// checks if global@idx is mutably borrowed
    fn is_global_mutably_borrowed(
        &self,
        resource: StructDefinitionIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<bool> {
        charge_graph_size(self.graph_size(), meter)?;
        Ok(self.has_consistent_mutable_borrows(self.frame_root(), Some(Label::Global(resource))))
    }

    /// checks if the stack frame of the function being analyzed can be safely destroyed.
    /// safe destruction requires that all references in locals have already been destroyed
    /// and all values in locals are copyable and unborrowed.
    fn is_frame_safe_to_destroy(&self, meter: &mut (impl Meter + ?Sized)) -> PartialVMResult<bool> {
        charge_graph_size(self.graph_size(), meter)?;
        Ok(!self.has_consistent_borrows(self.frame_root(), None))
    }

    //**********************************************************************************************
    // Instruction Entry Points
    //**********************************************************************************************

    /// destroys local@idx
    pub fn release_value(
        &mut self,
        value: AbstractValue,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        match value {
            AbstractValue::Reference(id) => self.release(id, meter),
            AbstractValue::NonReference => Ok(()),
        }
    }

    pub fn copy_loc(
        &mut self,
        offset: CodeOffset,
        local: LocalIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        match safe_unwrap!(self.locals.get(local as usize)) {
            AbstractValue::Reference(id) => {
                let id = *id;
                let new_id = self.new_ref(self.borrow_graph.is_mutable(id));
                self.add_copy(id, new_id, meter)?;
                Ok(AbstractValue::Reference(new_id))
            }
            AbstractValue::NonReference if self.is_local_mutably_borrowed(local, meter)? => {
                Err(self.error(StatusCode::COPYLOC_EXISTS_BORROW_ERROR, offset))
            }
            AbstractValue::NonReference => Ok(AbstractValue::NonReference),
        }
    }

    pub fn move_loc(
        &mut self,
        offset: CodeOffset,
        local: LocalIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        let old_value = std::mem::replace(
            safe_unwrap!(self.locals.get_mut(local as usize)),
            AbstractValue::NonReference,
        );
        match old_value {
            AbstractValue::Reference(id) => Ok(AbstractValue::Reference(id)),
            AbstractValue::NonReference if self.is_local_borrowed(local, meter)? => {
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
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        let old_value =
            std::mem::replace(safe_unwrap!(self.locals.get_mut(local as usize)), new_value);
        match old_value {
            AbstractValue::Reference(id) => self.release(id, meter),
            AbstractValue::NonReference if self.is_local_borrowed(local, meter)? => {
                Err(self.error(StatusCode::STLOC_UNSAFE_TO_DESTROY_ERROR, offset))
            }
            AbstractValue::NonReference => Ok(()),
        }
    }

    pub fn freeze_ref(
        &mut self,
        offset: CodeOffset,
        id: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        if !self.is_freezable(id, None, meter)? {
            return Err(self.error(StatusCode::FREEZEREF_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }

        let frozen_id = self.new_ref(false);
        self.add_copy(id, frozen_id, meter)?;
        self.release(id, meter)?;
        Ok(AbstractValue::Reference(frozen_id))
    }

    pub fn comparison(
        &mut self,
        offset: CodeOffset,
        v1: AbstractValue,
        v2: AbstractValue,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        match (v1, v2) {
            (AbstractValue::Reference(id1), AbstractValue::Reference(id2))
                if !self.is_readable(id1, None, meter)?
                    || !self.is_readable(id2, None, meter)? =>
            {
                // TODO better error code
                return Err(self.error(StatusCode::READREF_EXISTS_MUTABLE_BORROW_ERROR, offset));
            }
            (AbstractValue::Reference(id1), AbstractValue::Reference(id2)) => {
                self.release(id1, meter)?;
                self.release(id2, meter)?;
            }
            (v1, v2) => {
                assert!(v1.is_value());
                assert!(v2.is_value());
            }
        }
        Ok(AbstractValue::NonReference)
    }

    pub fn read_ref(
        &mut self,
        offset: CodeOffset,
        id: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        if !self.is_readable(id, None, meter)? {
            return Err(self.error(StatusCode::READREF_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }

        self.release(id, meter)?;
        Ok(AbstractValue::NonReference)
    }

    pub fn write_ref(
        &mut self,
        offset: CodeOffset,
        id: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        if !self.is_writable(id, meter)? {
            return Err(self.error(StatusCode::WRITEREF_EXISTS_BORROW_ERROR, offset));
        }

        self.release(id, meter)?;
        Ok(())
    }

    pub fn borrow_loc(
        &mut self,
        offset: CodeOffset,
        mut_: bool,
        local: LocalIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        // nothing to check in case borrow is mutable since the frame cannot have an full borrow/
        // epsilon outgoing edge
        if !mut_ && self.is_local_mutably_borrowed(local, meter)? {
            return Err(self.error(StatusCode::BORROWLOC_EXISTS_BORROW_ERROR, offset));
        }

        let new_id = self.new_ref(mut_);
        self.add_local_borrow(local, new_id, meter)?;
        Ok(AbstractValue::Reference(new_id))
    }

    pub fn borrow_field(
        &mut self,
        offset: CodeOffset,
        mut_: bool,
        id: RefID,
        field: FieldHandleIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        // Any field borrows will be factored out, so don't check in the mutable case
        macro_rules! is_mut_borrow_with_full_borrows {
            () => {
                mut_ && self.has_full_borrows(id)
            };
        }
        // For new immutable borrow, the reference must be readable at that field
        // This means that there could exist a mutable borrow on some other field
        macro_rules! is_imm_borrow_with_mut_borrows {
            () => {
                !mut_ && !self.is_readable(id, Some(field), meter)?
            };
        }
        if is_mut_borrow_with_full_borrows!() || is_imm_borrow_with_mut_borrows!() {
            // TODO improve error for mutable case
            return Err(self.error(StatusCode::FIELD_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }

        let field_borrow_id = self.new_ref(mut_);
        self.add_field_borrow(id, field, field_borrow_id, meter)?;
        self.release(id, meter)?;
        Ok(AbstractValue::Reference(field_borrow_id))
    }

    pub fn unpack_enum_variant_ref(
        &mut self,
        offset: CodeOffset,
        enum_def_idx: EnumDefinitionIndex,
        variant_tag: VariantTag,
        variant_def: &VariantDefinition,
        mut_: bool,
        id: RefID,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<Vec<AbstractValue>> {
        // Any field borrows will be factored out, so don't check in the mutable case
        macro_rules! is_mut_borrow_with_full_borrows {
            () => {
                mut_ && self.has_full_borrows(id)
            };
        }
        // For new immutable borrow, the reference to the variant must be readable.
        // This means that there _does not_ exist a mutable borrow on some other field
        macro_rules! is_imm_borrow_with_mut_borrows {
            () => {
                !mut_ && !self.is_readable(id, None, meter)?
            };
        }
        if is_mut_borrow_with_full_borrows!() || is_imm_borrow_with_mut_borrows!() {
            return Err(self.error(StatusCode::FIELD_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }

        let field_borrows = variant_def
            .fields
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let field_borrow_id = self.new_ref(mut_);
                self.add_variant_field_borrow(
                    id,
                    enum_def_idx,
                    variant_tag,
                    i as MemberCount,
                    field_borrow_id,
                    meter,
                )?;
                Ok(AbstractValue::Reference(field_borrow_id))
            })
            .collect::<PartialVMResult<_>>()?;

        self.release(id, meter)?;
        Ok(field_borrows)
    }

    pub fn borrow_global(
        &mut self,
        offset: CodeOffset,
        mut_: bool,
        resource: StructDefinitionIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        if (mut_ && self.is_global_borrowed(resource, meter)?)
            || self.is_global_mutably_borrowed(resource, meter)?
        {
            return Err(self.error(StatusCode::GLOBAL_REFERENCE_ERROR, offset));
        }

        let new_id = self.new_ref(mut_);
        self.add_resource_borrow(resource, new_id, meter)?;
        Ok(AbstractValue::Reference(new_id))
    }

    pub fn move_from(
        &mut self,
        offset: CodeOffset,
        resource: StructDefinitionIndex,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        if self.is_global_borrowed(resource, meter)? {
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
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()> {
        let id = safe_unwrap!(vector.ref_id());
        if mut_ && !self.is_writable(id, meter)? {
            return Err(self.error(StatusCode::VEC_UPDATE_EXISTS_MUTABLE_BORROW_ERROR, offset));
        }
        self.release(id, meter)?;
        Ok(())
    }

    pub fn vector_element_borrow(
        &mut self,
        offset: CodeOffset,
        vector: AbstractValue,
        mut_: bool,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<AbstractValue> {
        let vec_id = safe_unwrap!(vector.ref_id());
        if mut_ && !self.is_writable(vec_id, meter)? {
            return Err(self.error(
                StatusCode::VEC_BORROW_ELEMENT_EXISTS_MUTABLE_BORROW_ERROR,
                offset,
            ));
        }

        let elem_id = self.new_ref(mut_);
        self.add_borrow(vec_id, elem_id, meter)?;

        self.release(vec_id, meter)?;
        Ok(AbstractValue::Reference(elem_id))
    }

    pub fn call(
        &mut self,
        offset: CodeOffset,
        arguments: Vec<AbstractValue>,
        acquired_resources: &BTreeSet<StructDefinitionIndex>,
        return_: &Signature,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<Vec<AbstractValue>> {
        // Check acquires
        for acquired_resource in acquired_resources {
            if self.is_global_borrowed(*acquired_resource, meter)? {
                return Err(self.error(StatusCode::GLOBAL_REFERENCE_ERROR, offset));
            }
        }

        // Check mutable references can be transferred
        let mut all_references_to_borrow_from = BTreeSet::new();
        let mut mutable_references_to_borrow_from = BTreeSet::new();
        for id in arguments.iter().filter_map(|v| v.ref_id()) {
            if self.borrow_graph.is_mutable(id) {
                if !self.is_writable(id, meter)? {
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
            .map(|return_type| {
                Ok(match return_type {
                    SignatureToken::MutableReference(_) => {
                        let id = self.new_ref(true);
                        for parent in &mutable_references_to_borrow_from {
                            self.add_borrow(*parent, id, meter)?;
                        }
                        returned_refs += 1;
                        AbstractValue::Reference(id)
                    }
                    SignatureToken::Reference(_) => {
                        let id = self.new_ref(false);
                        for parent in &all_references_to_borrow_from {
                            self.add_borrow(*parent, id, meter)?;
                        }
                        returned_refs += 1;
                        AbstractValue::Reference(id)
                    }
                    _ => AbstractValue::NonReference,
                })
            })
            .collect::<PartialVMResult<_>>()?;

        // Release input references
        for id in all_references_to_borrow_from {
            self.release(id, meter)?
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
        let mut released = BTreeSet::new();
        for stored_value in self.locals.iter() {
            if let AbstractValue::Reference(id) = stored_value {
                released.insert(*id);
            }
        }
        for id in released {
            self.release(id, meter)?
        }

        // Check that no local or global is borrowed
        if !self.is_frame_safe_to_destroy(meter)? {
            return Err(self.error(
                StatusCode::UNSAFE_RET_LOCAL_OR_RESOURCE_STILL_BORROWED,
                offset,
            ));
        }

        // Check mutable references can be transferred
        for id in values.into_iter().filter_map(|v| v.ref_id()) {
            if self.borrow_graph.is_mutable(id) && !self.is_writable(id, meter)? {
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

    pub fn join_(&self, other: &Self) -> (Self, usize) {
        assert!(self.current_function == other.current_function);
        assert!(self.is_canonical() && other.is_canonical());
        assert!(self.next_id == other.next_id);
        assert!(self.locals.len() == other.locals.len());
        let mut self_graph = self.borrow_graph.clone();
        let mut other_graph = other.borrow_graph.clone();
        let mut released = 0;
        let locals = self
            .locals
            .iter()
            .zip(&other.locals)
            .map(|(self_value, other_value)| {
                match (self_value, other_value) {
                    (AbstractValue::Reference(id), AbstractValue::NonReference) => {
                        released += self_graph.release(*id);
                        AbstractValue::NonReference
                    }
                    (AbstractValue::NonReference, AbstractValue::Reference(id)) => {
                        released += other_graph.release(*id);
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
        let joined = Self {
            current_function,
            locals,
            borrow_graph,
            next_id,
        };
        (joined, released)
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
        let (joined, released) = Self::join_(self, state);
        assert!(joined.is_canonical());
        assert!(self.locals.len() == joined.locals.len());
        let max_size = max(max(self_size, state_size), joined.graph_size());
        charge_join(self_size, state_size, meter)?;
        charge_graph_size(max_size, meter)?;
        charge_release(released, meter)?;
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

fn charge_graph_size(size: usize, meter: &mut (impl Meter + ?Sized)) -> PartialVMResult<()> {
    let size = max(size, 1);
    meter.add_items(Scope::Function, PER_GRAPH_ITEM_COST, size)
}

fn charge_release(released: usize, meter: &mut (impl Meter + ?Sized)) -> PartialVMResult<()> {
    let size = max(released, 1);
    meter.add_items(
        Scope::Function,
        RELEASE_ITEM_COST,
        // max(x, x^2/5)
        max(
            size,
            size.saturating_mul(size) / RELEASE_ITEM_QUADRATIC_THRESHOLD,
        ),
    )
}

fn charge_join(
    size1: usize,
    size2: usize,
    meter: &mut (impl Meter + ?Sized),
) -> PartialVMResult<()> {
    let size1 = max(size1, 1);
    let size2 = max(size2, 1);
    let size = size1.saturating_add(size2);
    meter.add_items(
        Scope::Function,
        JOIN_ITEM_COST,
        // max(x, x^2/10)
        max(
            size,
            size.saturating_mul(size) / JOIN_ITEM_QUADRATIC_THRESHOLD,
        ),
    )
}
