// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//**************************************************************************************************
// Abstract state
//**************************************************************************************************

use crate::{
    cfgir::absint::*,
    diag,
    diagnostics::{
        codes::{DiagnosticCode, ReferenceSafety},
        Diagnostic, Diagnostics,
    },
    expansion::ast::Mutability,
    hlir::{
        ast::{self as H, *},
        translate::{display_var, DisplayVar},
    },
    parser::ast::Field,
    shared::{unique_map::UniqueMap, *},
};
use move_borrow_graph::references::RefID;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Label {
    Local(Symbol),
    Field(Symbol),
}

type BorrowGraph = move_borrow_graph::graph::BorrowGraph<Loc, Label>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    NonRef,
    Ref(RefID),
}
pub type Values = Vec<Value>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExpBasedID {
    block: Option<H::Label>,
    command: usize,
    count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefExpInfo {
    pub loc: Loc,
    pub is_mut: bool,
    pub used_mutably: bool,
    pub param_name: Option<Var>,
}

pub type RefExpInfoMap = Rc<RefCell<BTreeMap<ExpBasedID, RefExpInfo>>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BorrowState {
    // metadata fields, needed for gathering warnings
    mutably_used: RefExpInfoMap,
    next_eid: ExpBasedID,
    id_to_exp: BTreeMap<RefID, BTreeSet<ExpBasedID>>,

    // fields necessary to the analysis
    locals: UniqueMap<Var, Value>,
    borrows: BorrowGraph,
    next_id: usize,
    // true if the previous pass had errors
    prev_had_errors: bool,
}

//**************************************************************************************************
// impls
//**************************************************************************************************

pub fn assert_single_value(mut values: Values) -> Value {
    assert!(values.len() == 1);
    values.pop().unwrap()
}

impl Value {
    pub fn is_ref(&self) -> bool {
        match self {
            Value::Ref(_) => true,
            Value::NonRef => false,
        }
    }

    pub fn as_vref(&self) -> Option<RefID> {
        match self {
            Value::Ref(id) => Some(*id),
            Value::NonRef => None,
        }
    }

    fn remap_refs(&mut self, id_map: &BTreeMap<RefID, RefID>) {
        match self {
            Value::Ref(id) if id_map.contains_key(id) => *id = id_map[id],
            _ => (),
        }
    }
}

impl BorrowState {
    pub fn initial<T>(
        locals: &UniqueMap<Var, T>,
        mutably_used: RefExpInfoMap,
        prev_had_errors: bool,
    ) -> Self {
        let mut new_state = BorrowState {
            locals: locals.ref_map(|_, _| Value::NonRef),
            borrows: BorrowGraph::new(),
            next_id: locals.len() + 1,
            prev_had_errors,
            mutably_used,
            next_eid: ExpBasedID {
                block: None,
                command: 0,
                count: 0,
            },
            id_to_exp: BTreeMap::new(),
        };
        new_state.borrows.new_ref(Self::LOCAL_ROOT, true);
        new_state
    }

    fn borrow_error<F: Fn() -> String>(
        borrows: &BorrowGraph,
        loc: Loc,
        full_borrows: &BTreeMap<RefID, Loc>,
        field_borrows: &BTreeMap<Label, BTreeMap<RefID, Loc>>,
        code: impl DiagnosticCode,
        msg: F,
    ) -> Option<Diagnostic> {
        if full_borrows.is_empty() && field_borrows.is_empty() {
            return None;
        }

        let mut_adj = |id| {
            if borrows.is_mutable(id) {
                "mutably "
            } else {
                ""
            }
        };
        let mut diag = diag!(code, (loc, msg()));
        for (borrower, rloc) in full_borrows {
            let adj = mut_adj(*borrower);
            diag.add_secondary_label((
                *rloc,
                format!("It is still being {}borrowed by this reference", adj),
            ))
        }
        for (field_lbl, borrowers) in field_borrows {
            for (borrower, rloc) in borrowers {
                let adj = mut_adj(*borrower);
                let field = match field_lbl {
                    Label::Field(f) => f,
                    Label::Local(_) => panic!(
                        "ICE local should not be field borrows as they only exist from \
                         the virtual 'root' reference"
                    ),
                };
                diag.add_secondary_label((
                    *rloc,
                    format!(
                        "Field '{}' is still being {}borrowed by this reference",
                        field, adj
                    ),
                ))
            }
        }
        assert!(diag.extra_labels_len() >= 1);
        Some(diag)
    }

    const LOCAL_ROOT: RefID = RefID::new(0);

    fn field_label(field: &Field) -> Label {
        Label::Field(field.value().to_owned())
    }

    fn local_label(local: &Var) -> Label {
        Label::Local(local.value().to_owned())
    }

    //**********************************************************************************************
    // Metadata API
    //**********************************************************************************************

    pub fn start_command(&mut self, block: H::Label, command: usize) {
        self.next_eid = ExpBasedID {
            block: Some(block),
            command,
            count: 0,
        };
    }

    fn set_param_name(&mut self, id: RefID, name: Var) {
        let exps = self.id_to_exp.get(&id).unwrap();
        assert_eq!(exps.len(), 1);
        let infos: &mut BTreeMap<ExpBasedID, RefExpInfo> =
            &mut RefCell::borrow_mut(&self.mutably_used);
        let info = infos.get_mut(exps.first().unwrap()).unwrap();
        assert!(info.param_name.is_none());
        info.param_name = Some(name)
    }

    fn mark_mutably_used(&mut self, id: RefID) {
        let Some(exps) = self.id_to_exp.get(&id) else {
            return;
        };
        let infos: &mut BTreeMap<ExpBasedID, RefExpInfo> =
            &mut RefCell::borrow_mut(&self.mutably_used);
        for e in exps {
            let info = infos.get_mut(e).unwrap();
            info.used_mutably = true;
        }
    }

    fn new_exp(&mut self, loc: Loc, is_mut: bool) -> ExpBasedID {
        let infos: &mut BTreeMap<ExpBasedID, RefExpInfo> =
            &mut RefCell::borrow_mut(&self.mutably_used);
        let eid = self.next_eid;
        infos.entry(eid).or_insert_with(|| RefExpInfo {
            loc,
            is_mut,
            used_mutably: false,
            param_name: None,
        });
        self.next_eid.count += 1;
        eid
    }

    //**********************************************************************************************
    // Core API
    //**********************************************************************************************

    fn single_type_value(&mut self, loc: Loc, s: &SingleType) -> Value {
        match &s.value {
            SingleType_::Base(_) => Value::NonRef,
            SingleType_::Ref(mut_, _) => Value::Ref(self.declare_new_ref(loc, *mut_)),
        }
    }

    fn declare_new_ref(&mut self, loc: Loc, mut_: bool) -> RefID {
        self.declare_new_ref_impl(loc, mut_, None)
    }

    fn declare_new_ref_impl(&mut self, loc: Loc, mut_: bool, copy_eid: Option<RefID>) -> RefID {
        fn new_id(next: &mut usize) -> RefID {
            *next += 1;
            RefID::new(*next)
        }

        let id = new_id(&mut self.next_id);
        self.borrows.new_ref(id, mut_);
        let eids = if let Some(parent) = copy_eid {
            self.id_to_exp.get(&parent).unwrap().clone()
        } else {
            BTreeSet::from([self.new_exp(loc, mut_)])
        };
        self.id_to_exp.insert(id, eids);
        id
    }

    fn add_copy(&mut self, loc: Loc, parent: RefID, child: RefID) {
        self.borrows.add_strong_borrow(loc, parent, child)
    }

    fn add_borrow(&mut self, loc: Loc, parent: RefID, child: RefID) {
        self.borrows.add_weak_borrow(loc, parent, child)
    }

    fn add_field_borrow(&mut self, loc: Loc, parent: RefID, field: Field, child: RefID) {
        self.borrows
            .add_strong_field_borrow(loc, parent, Self::field_label(&field), child)
    }

    fn add_local_borrow(&mut self, loc: Loc, local: &Var, id: RefID) {
        self.borrows
            .add_strong_field_borrow(loc, Self::LOCAL_ROOT, Self::local_label(local), id)
    }

    fn writable<F: Fn() -> String>(&self, loc: Loc, msg: F, id: RefID) -> Diagnostics {
        assert!(self.borrows.is_mutable(id), "ICE type checking failed");
        let (full_borrows, field_borrows) = self.borrows.borrowed_by(id);
        Self::borrow_error(
            &self.borrows,
            loc,
            &full_borrows,
            &field_borrows,
            ReferenceSafety::Dangling,
            msg,
        )
        .into()
    }

    fn freezable<F: Fn() -> String>(
        &self,
        loc: Loc,
        code: impl DiagnosticCode,
        msg: F,
        id: RefID,
        at_field_opt: Option<&Field>,
    ) -> Diagnostics {
        assert!(self.borrows.is_mutable(id), "ICE type checking failed");
        let (full_borrows, field_borrows) = self.borrows.borrowed_by(id);
        let mut_filter_set = |s: BTreeMap<RefID, Loc>| {
            s.into_iter()
                .filter(|(id, _loc)| self.borrows.is_mutable(*id))
                .collect::<BTreeMap<_, _>>()
        };
        let mut_full_borrows = mut_filter_set(full_borrows);
        let mut_field_borrows = field_borrows
            .into_iter()
            .filter_map(|(f, borrowers)| {
                match (at_field_opt, &f) {
                    // Borrow at the same field, so keep
                    (Some(at_field), Label::Field(f_)) if *f_ == at_field.value() => (),
                    // Borrow not at the same field, so skip
                    (Some(_at_field), _) => return None,
                    // Not freezing at a field, so consider any field borrows
                    (None, _) => (),
                }
                let borrowers = mut_filter_set(borrowers);
                if borrowers.is_empty() {
                    None
                } else {
                    Some((f, borrowers))
                }
            })
            .collect();
        Self::borrow_error(
            &self.borrows,
            loc,
            &mut_full_borrows,
            &mut_field_borrows,
            code,
            msg,
        )
        .into()
    }

    fn readable<F: Fn() -> String>(
        &self,
        loc: Loc,
        code: impl DiagnosticCode,
        msg: F,
        id: RefID,
        at_field_opt: Option<&Field>,
    ) -> Diagnostics {
        let is_mutable = self.borrows.is_mutable(id);
        if is_mutable {
            self.freezable(loc, code, msg, id, at_field_opt)
        } else {
            // immutable reference is always readable
            Diagnostics::new()
        }
    }

    fn release(&mut self, ref_id: RefID) {
        self.id_to_exp.remove(&ref_id);
        self.borrows.release(ref_id);
    }

    fn divergent_control_flow(&mut self) {
        *self = Self::initial(
            &self.locals,
            self.mutably_used.clone(),
            self.prev_had_errors,
        );
    }

    fn local_borrowed_by(&self, local: &Var) -> BTreeMap<RefID, Loc> {
        let (full_borrows, mut field_borrows) = self.borrows.borrowed_by(Self::LOCAL_ROOT);
        assert!(full_borrows.is_empty());
        field_borrows
            .remove(&Self::local_label(local))
            .unwrap_or_default()
    }

    // returns empty errors if borrowed_by is empty
    // Returns errors otherwise
    fn check_use_borrowed_by(
        borrows: &BorrowGraph,
        loc: Loc,
        local: &Var,
        full_borrows: &BTreeMap<RefID, Loc>,
        code: impl DiagnosticCode,
        verb: &'static str,
    ) -> Option<Diagnostic> {
        Self::borrow_error(
            borrows,
            loc,
            full_borrows,
            &BTreeMap::new(),
            code,
            move || match display_var(local.value()) {
                DisplayVar::Tmp => panic!(
                    "ICE invalid use of tmp local {} with borrows {:#?}",
                    local.value(),
                    borrows
                ),
                DisplayVar::MatchTmp(_s) => format!("Invalid {} of temporary match variable", verb),
                DisplayVar::Orig(s) => format!("Invalid {} of variable '{}'", verb, s),
            },
        )
    }

    //**********************************************************************************************
    // Command Entry Points
    //**********************************************************************************************

    pub fn bind_arguments(&mut self, parameter_types: &[(Mutability, Var, SingleType)]) {
        for (_mut, local, ty) in parameter_types.iter() {
            let value = self.single_type_value(ty.loc, ty);
            if let Value::Ref(id) = value {
                self.set_param_name(id, *local);
                // silence unused mutable reference error for parameters with a leading _
                if local.starts_with_underscore() {
                    self.mark_mutably_used(id)
                }
            }
            let diags = self.assign_local(local.loc(), local, value);
            assert!(diags.is_empty())
        }
    }

    pub fn release_values(&mut self, values: Values) {
        for value in values {
            self.release_value(value)
        }
    }

    pub fn release_value(&mut self, value: Value) {
        if let Value::Ref(id) = value {
            self.release(id)
        }
    }

    pub fn assign_local(&mut self, loc: Loc, local: &Var, new_value: Value) -> Diagnostics {
        let old_value = self.locals.remove(local).unwrap();
        self.locals.add(*local, new_value).unwrap();
        match old_value {
            Value::Ref(id) => {
                self.release(id);
                Diagnostics::new()
            }
            Value::NonRef => {
                let borrowed_by = self.local_borrowed_by(local);
                Self::check_use_borrowed_by(
                    &self.borrows,
                    loc,
                    local,
                    &borrowed_by,
                    ReferenceSafety::Dangling,
                    "assignment",
                )
                .into()
            }
        }
    }

    pub fn mutate(&mut self, loc: Loc, rvalue: Value) -> Diagnostics {
        let id = match rvalue {
            Value::NonRef => {
                assert!(
                    self.prev_had_errors,
                    "ICE borrow checking failed {:#?}",
                    loc
                );
                return Diagnostics::new();
            }
            Value::Ref(id) => id,
        };

        self.mark_mutably_used(id);
        let diags = self.writable(loc, || "Invalid mutation of reference.".into(), id);
        self.release(id);
        diags
    }

    pub fn return_(&mut self, loc: Loc, rvalues: Values) -> Diagnostics {
        let mut released = BTreeSet::new();
        for (_, _local, stored_value) in &self.locals {
            if let Value::Ref(id) = stored_value {
                released.insert(*id);
            }
        }
        released.into_iter().for_each(|id| self.release(id));

        // Check locals are not borrowed
        let mut diags = Diagnostics::new();
        for (local, stored_value) in self.locals.key_cloned_iter() {
            if let Value::NonRef = stored_value {
                let borrowed_by = self.local_borrowed_by(&local);
                let local_diag = Self::borrow_error(
                    &self.borrows,
                    loc,
                    &borrowed_by,
                    &BTreeMap::new(),
                    ReferenceSafety::InvalidReturn,
                    || {
                        let case = match display_var(local.value()) {
                            DisplayVar::Orig(v) => format!("Local variable '{v}'"),
                            DisplayVar::MatchTmp(_) => "Local value".to_string(),
                            DisplayVar::Tmp => "Local value".to_string(),
                        };
                        format!("Invalid return. {case} is still being borrowed.")
                    },
                );
                diags.add_opt(local_diag)
            }
        }

        // check any returned reference is not borrowed
        for rvalue in rvalues {
            match rvalue {
                Value::Ref(id) if self.borrows.is_mutable(id) => {
                    self.mark_mutably_used(id);
                    let (fulls, fields) = self.borrows.borrowed_by(id);
                    let msg = || {
                        "Invalid return of reference. Cannot transfer a mutable reference that is \
                         being borrowed"
                            .into()
                    };
                    let ds = Self::borrow_error(
                        &self.borrows,
                        loc,
                        &fulls,
                        &fields,
                        ReferenceSafety::InvalidTransfer,
                        msg,
                    );
                    diags.add_opt(ds);
                }
                _ => (),
            }
        }

        self.divergent_control_flow();
        diags
    }

    pub fn abort(&mut self) {
        self.divergent_control_flow()
    }

    //**********************************************************************************************
    // Expression Entry Points
    //**********************************************************************************************

    pub fn move_local(
        &mut self,
        loc: Loc,
        local: &Var,
        last_usage_inferred: bool,
    ) -> (Diagnostics, Value) {
        let old_value = self.locals.remove(local).unwrap();
        self.locals.add(*local, Value::NonRef).unwrap();
        match old_value {
            Value::Ref(id) => (Diagnostics::new(), Value::Ref(id)),
            Value::NonRef if last_usage_inferred => {
                let borrowed_by = self.local_borrowed_by(local);

                let mut diag_opt = Self::borrow_error(
                    &self.borrows,
                    loc,
                    &borrowed_by,
                    &BTreeMap::new(),
                    ReferenceSafety::AmbiguousVariableUsage,
                    || {
                        let vstr = match display_var(local.value()) {
                            DisplayVar::Tmp => {
                                panic!("ICE invalid use tmp local {}", local.value())
                            }
                            DisplayVar::MatchTmp(s) => {
                                panic!("ICE invalid use match tmp {}: {}", s, local.value())
                            }
                            DisplayVar::Orig(s) => s,
                        };
                        format!("Ambiguous usage of variable '{}'", vstr)
                    },
                );
                diag_opt.iter_mut().for_each(|diag| {
                    let vstr = match display_var(local.value()) {
                        DisplayVar::Tmp => {
                            panic!("ICE invalid use tmp local {}", local.value())
                        }
                        DisplayVar::MatchTmp(s) => {
                            panic!("ICE invalid use match tmp {}: {}", s, local.value())
                        }
                        DisplayVar::Orig(s) => s,
                    };
                    let tip = format!(
                        "Try an explicit annotation, e.g. 'move {v}' or 'copy {v}'",
                        v = vstr
                    );
                    const EXPLANATION: &str = "Ambiguous inference of 'move' or 'copy' for a \
                                               borrowed variable's last usage: A 'move' would \
                                               invalidate the borrowing reference, but a 'copy' \
                                               might not be the expected implicit behavior since \
                                               this the last direct usage of the variable.";
                    diag.add_secondary_label((loc, tip));
                    diag.add_note(EXPLANATION);
                });
                (diag_opt.into(), Value::NonRef)
            }
            Value::NonRef => {
                let borrowed_by = self.local_borrowed_by(local);
                let diag_opt = Self::check_use_borrowed_by(
                    &self.borrows,
                    loc,
                    local,
                    &borrowed_by,
                    ReferenceSafety::Dangling,
                    "move",
                );
                (diag_opt.into(), Value::NonRef)
            }
        }
    }

    pub fn copy_local(&mut self, loc: Loc, local: &Var) -> (Diagnostics, Value) {
        match self.locals.get(local).unwrap() {
            Value::Ref(id) => {
                let id = *id;
                let new_id = self.declare_new_ref_impl(loc, self.borrows.is_mutable(id), Some(id));
                self.add_copy(loc, id, new_id);
                (Diagnostics::new(), Value::Ref(new_id))
            }
            Value::NonRef => {
                let borrowed_by = self.local_borrowed_by(local);
                let borrows = &self.borrows;
                // check that it is 'readable'
                let mut_borrows = borrowed_by
                    .into_iter()
                    .filter(|(id, _loc)| borrows.is_mutable(*id))
                    .collect();
                let diags = Self::check_use_borrowed_by(
                    &self.borrows,
                    loc,
                    local,
                    &mut_borrows,
                    ReferenceSafety::MutOwns,
                    "copy",
                );
                (diags.into(), Value::NonRef)
            }
        }
    }

    pub fn borrow_local(&mut self, loc: Loc, mut_: bool, local: &Var) -> (Diagnostics, Value) {
        assert!(
            !self.locals.get(local).unwrap().is_ref(),
            "ICE borrow ref of {:?} at {:#?}. Should have been caught in typing",
            local,
            loc
        );
        let new_id = self.declare_new_ref(loc, mut_);
        // fails if there are full/epsilon borrows on the local
        let borrowed_by = self.local_borrowed_by(local);
        let diags = if !mut_ {
            let borrows = &self.borrows;
            // check that it is 'readable'
            let mut_borrows = borrowed_by
                .into_iter()
                .filter(|(id, _loc)| borrows.is_mutable(*id))
                .collect();
            Self::check_use_borrowed_by(
                borrows,
                loc,
                local,
                &mut_borrows,
                ReferenceSafety::RefTrans,
                "borrow",
            )
            .into()
        } else {
            Diagnostics::new()
        };
        self.add_local_borrow(loc, local, new_id);
        (diags, Value::Ref(new_id))
    }

    pub fn freeze(&mut self, loc: Loc, rvalue: Value) -> (Diagnostics, Value) {
        let id = match rvalue {
            Value::NonRef => {
                assert!(
                    self.prev_had_errors,
                    "ICE borrow checking failed {:#?}",
                    loc
                );
                return (Diagnostics::new(), Value::NonRef);
            }
            Value::Ref(id) => id,
        };

        let diags = self.freezable(
            loc,
            ReferenceSafety::MutOwns,
            || "Invalid freeze.".into(),
            id,
            None,
        );
        let frozen_id = self.declare_new_ref(loc, false);
        self.add_copy(loc, id, frozen_id);
        self.release(id);
        (diags, Value::Ref(frozen_id))
    }

    pub fn dereference(&mut self, loc: Loc, rvalue: Value) -> (Diagnostics, Value) {
        let id = match rvalue {
            Value::NonRef => {
                assert!(
                    self.prev_had_errors,
                    "ICE borrow checking failed {:#?}",
                    loc
                );
                return (Diagnostics::new(), Value::NonRef);
            }
            Value::Ref(id) => id,
        };

        let diags = self.readable(
            loc,
            ReferenceSafety::MutOwns,
            || "Invalid dereference.".into(),
            id,
            None,
        );
        self.release(id);
        (diags, Value::NonRef)
    }

    pub fn borrow_field(
        &mut self,
        loc: Loc,
        mut_: bool,
        rvalue: Value,
        field: &Field,
        from_unpack: FromUnpack,
    ) -> (Diagnostics, Value) {
        let id = match rvalue {
            Value::NonRef => {
                assert!(
                    self.prev_had_errors,
                    "ICE borrow checking failed {:#?}",
                    loc
                );
                return (Diagnostics::new(), Value::NonRef);
            }
            Value::Ref(id) => id,
        };

        let diags = if mut_ {
            if from_unpack.is_none() {
                self.mark_mutably_used(id);
            }
            let msg = || format!("Invalid mutable borrow at field '{}'.", field);
            let (full_borrows, _field_borrows) = self.borrows.borrowed_by(id);
            // Any field borrows will be factored out
            Self::borrow_error(
                &self.borrows,
                loc,
                &full_borrows,
                &BTreeMap::new(),
                ReferenceSafety::MutOwns,
                msg,
            )
            .into()
        } else {
            let msg = || format!("Invalid immutable borrow at field '{}'.", field);
            self.readable(loc, ReferenceSafety::RefTrans, msg, id, Some(field))
        };
        let copy_parent = if from_unpack.is_some() {
            Some(id)
        } else {
            None
        };
        let field_borrow_id = self.declare_new_ref_impl(loc, mut_, copy_parent);
        self.add_field_borrow(loc, id, *field, field_borrow_id);
        self.release(id);
        (diags, Value::Ref(field_borrow_id))
    }

    pub fn borrow_variant_fields(
        &mut self,
        loc: Loc,
        mut_: bool,
        rvalue: Value,
        fields: &[(Field, LValue)],
    ) -> (Diagnostics, Vec<Value>) {
        let mut diags = Diagnostics::new();
        let id = match rvalue {
            Value::NonRef => {
                assert!(
                    self.prev_had_errors,
                    "ICE borrow checking failed {:#?}",
                    loc
                );
                return (
                    Diagnostics::new(),
                    fields.iter().map(|_| Value::NonRef).collect::<Vec<_>>(),
                );
            }
            Value::Ref(id) => id,
        };
        let copy_parent = Some(id);
        let fvs = fields
            .iter()
            .map(|(field, _)| {
                let new_diags = if mut_ {
                    let msg = || format!("Invalid mutable borrow at field '{}'.", field);
                    let (full_borrows, _field_borrows) = self.borrows.borrowed_by(id);
                    // Any field borrows will be factored out
                    Self::borrow_error(
                        &self.borrows,
                        loc,
                        &full_borrows,
                        &BTreeMap::new(),
                        ReferenceSafety::MutOwns,
                        msg,
                    )
                    .into()
                } else {
                    let msg = || format!("Invalid immutable borrow at field '{}'.", field);
                    self.readable(loc, ReferenceSafety::RefTrans, msg, id, Some(field))
                };
                diags.extend(new_diags);
                let field_borrow_id = self.declare_new_ref_impl(loc, mut_, copy_parent);
                self.add_field_borrow(loc, id, *field, field_borrow_id);
                Value::Ref(field_borrow_id)
            })
            .collect::<Vec<_>>();
        self.release(id);
        (diags, fvs)
    }

    pub fn variant_switch(&mut self, loc: Loc, subject: Value) -> Diagnostics {
        let id = match subject {
            Value::NonRef => {
                assert!(
                    self.prev_had_errors,
                    "ICE borrow checking failed {:#?}",
                    loc
                );
                return Diagnostics::new();
            }
            Value::Ref(id) => id,
        };
        let msg = || "Invalid immutable borrow of match subject".to_string();
        let diags = self.readable(loc, ReferenceSafety::RefTrans, msg, id, None);
        self.release(id);
        diags
    }

    pub fn call(&mut self, loc: Loc, args: Values, return_ty: &Type) -> (Diagnostics, Values) {
        let mut diags = Diagnostics::new();

        // Check mutable arguments are not borrowed
        args.iter()
            .filter_map(|arg| arg.as_vref().filter(|id| self.borrows.is_mutable(*id)))
            .for_each(|mut_id| {
                let (fulls, fields) = self.borrows.borrowed_by(mut_id);
                let msg = || {
                    "Invalid usage of reference as function argument. Cannot transfer a mutable \
                     reference that is being borrowed"
                        .into()
                };
                let ds = Self::borrow_error(
                    &self.borrows,
                    loc,
                    &fulls,
                    &fields,
                    ReferenceSafety::InvalidTransfer,
                    msg,
                );
                diags.add_opt(ds);
            });

        let mut all_parents = BTreeSet::new();
        let mut mut_parents = BTreeSet::new();
        for id in args.into_iter().filter_map(|arg| arg.as_vref()) {
            all_parents.insert(id);
            if self.borrows.is_mutable(id) {
                self.mark_mutably_used(id);
                mut_parents.insert(id);
            }
        }

        let values = match &return_ty.value {
            Type_::Unit => vec![],
            Type_::Single(s) => vec![self.single_type_value(loc, s)],
            Type_::Multiple(ss) => ss.iter().map(|s| self.single_type_value(loc, s)).collect(),
        };
        for value in &values {
            if let Value::Ref(id) = value {
                // mark return values as used mutably, since the caller cannot change the signature
                // of the called function
                self.mark_mutably_used(*id);
                let parents = if self.borrows.is_mutable(*id) {
                    &mut_parents
                } else {
                    &all_parents
                };
                parents.iter().for_each(|p| self.add_borrow(loc, *p, *id));
            }
        }
        all_parents.into_iter().for_each(|id| self.release(id));

        (diags, values)
    }

    //**********************************************************************************************
    // Abstract State
    //**********************************************************************************************

    pub fn canonicalize_locals(&mut self, local_numbers: &UniqueMap<Var, usize>) {
        let mut all_refs = self.borrows.all_refs();
        let mut id_map = BTreeMap::new();
        for (_, local_, value) in &self.locals {
            if let Value::Ref(id) = value {
                assert!(all_refs.remove(id));
                id_map.insert(*id, RefID::new(*local_numbers.get_(local_).unwrap() + 1));
            }
        }
        all_refs.remove(&Self::LOCAL_ROOT);
        if !all_refs.is_empty() {
            for ref_ in all_refs {
                println!("had ref: {:?}", ref_);
            }
            println!("borrow graph:");
            self.borrows.display();
            println!("locals:");
            for (_, local_, value) in &self.locals {
                println!("{} -> {:?}", local_, value);
            }
            println!("id map:");
            for (key, value) in &id_map {
                println!("{:?} -> {:?}", key, value);
            }
            panic!("Had some refs left over");
        }
        self.locals
            .iter_mut()
            .for_each(|(_, _, v)| v.remap_refs(&id_map));
        self.id_to_exp = std::mem::take(&mut self.id_to_exp)
            .into_iter()
            .map(|(id, eids)| (*id_map.get(&id).unwrap(), eids))
            .collect();
        self.borrows.remap_refs(&id_map);
        self.next_id = self.locals.len() + 1;
    }

    pub fn join_(mut self, mut other: Self) -> Self {
        let mut released = BTreeSet::new();
        let mut locals = UniqueMap::new();
        for (local, self_value) in self.locals.key_cloned_iter() {
            let joined_value = match (self_value, other.locals.get(&local).unwrap()) {
                (Value::Ref(id1), Value::Ref(id2)) => {
                    assert!(id1 == id2);
                    Value::Ref(*id1)
                }
                (Value::NonRef, Value::Ref(released_id))
                | (Value::Ref(released_id), Value::NonRef) => {
                    released.insert(*released_id);
                    Value::NonRef
                }
                (Value::NonRef, Value::NonRef) => Value::NonRef,
            };
            locals.add(local, joined_value).unwrap();
        }
        for released_id in released {
            if self.borrows.contains_id(released_id) {
                self.release(released_id);
            }
            if other.borrows.contains_id(released_id) {
                other.release(released_id);
            }
        }

        let borrows = self.borrows.join(&other.borrows);
        let next_id = locals.len() + 1;
        let prev_had_errors = self.prev_had_errors;
        let mut id_to_exp = self.id_to_exp;
        for (id, exps) in other.id_to_exp {
            id_to_exp.entry(id).or_default().extend(exps);
        }
        assert!(next_id == self.next_id);
        assert!(next_id == other.next_id);
        assert!(prev_had_errors == other.prev_had_errors);

        Self {
            locals,
            borrows,
            next_id,
            prev_had_errors,
            next_eid: self.next_eid,
            mutably_used: self.mutably_used.clone(),
            id_to_exp,
        }
    }

    fn leq(&self, other: &Self) -> bool {
        let BorrowState {
            locals: self_locals,
            borrows: self_borrows,
            next_id: self_next,
            prev_had_errors: self_prev_had_errors,
            // metadata gathered
            mutably_used: _,
            next_eid: _,
            id_to_exp: self_id_to_exp,
        } = self;
        let BorrowState {
            locals: other_locals,
            borrows: other_borrows,
            next_id: other_next,
            prev_had_errors: other_prev_had_errors,
            // metadata gathered
            mutably_used: _,
            next_eid: _,
            id_to_exp: other_id_to_exp,
        } = other;
        assert!(self_next == other_next, "ICE canonicalization failed");
        assert!(
            self_prev_had_errors == other_prev_had_errors,
            "ICE previous errors flag changed"
        );
        self_locals == other_locals
            && self_borrows.leq(other_borrows)
            && other_id_to_exp.iter().all(|(id, other_eids)| {
                self_id_to_exp
                    .get(id)
                    .map(|self_eids| other_eids.is_subset(self_eids))
                    .unwrap_or(false)
            })
    }
}

impl AbstractDomain for BorrowState {
    fn join(&mut self, other: &Self) -> JoinResult {
        let joined = self.clone().join_(other.clone());
        if !self.leq(&joined) {
            *self = joined;
            JoinResult::Changed
        } else {
            JoinResult::Unchanged
        }
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Label::Local(s) => write!(f, "local%{}", s),
            Label::Field(s) => write!(f, "{}", s),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::NonRef => write!(f, "_"),
            Value::Ref(id) => write!(f, "{:?}", id),
        }
    }
}

impl BorrowState {
    #[allow(dead_code)]
    pub fn display(&self) {
        println!("NEXT ID: {}", self.next_id);
        println!("LOCALS:");
        for (_, var, value) in &self.locals {
            println!("  {}: {}", var, value)
        }
        println!("BORROWS: ");
        self.borrows.display();
        println!();
    }
}
