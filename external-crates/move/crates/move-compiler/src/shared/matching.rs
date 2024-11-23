// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::DiagnosticReporter,
    expansion::ast::{Fields, ModuleIdent, Mutability, Value},
    hlir::translate::NEW_NAME_DELIM,
    ice,
    naming::ast::{self as N, Type, Var},
    parser::ast::{BinOp_, ConstantName, Field, VariantName},
    shared::{program_info::ProgramInfo, unique_map::UniqueMap, CompilationEnv},
    typing::ast::{self as T, MatchArm_, MatchPattern, UnannotatedPat_ as TP},
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

//**************************************************************************************************
// Pattern Matrix Definitions for Matching
//**************************************************************************************************

#[derive(Clone, Debug)]
pub struct FringeEntry {
    pub var: Var,
    pub ty: Type,
}

pub type Binders = Vec<(Mutability, Var)>;
pub type PatBindings = BTreeMap<Var, (Mutability, FringeEntry)>;
pub type Guard = Option<Box<T::Exp>>;

#[derive(Clone, Debug)]
pub struct Arm {
    pub orig_pattern: MatchPattern,
    pub rhs_binders: BTreeSet<Var>,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct PatternArm {
    pub pats: VecDeque<T::MatchPattern>,
    pub guard: Guard,
    pub arm: Arm,
}

#[derive(Clone, Debug)]
pub struct PatternMatrix {
    pub tys: Vec<Type>,
    pub loc: Loc,
    pub patterns: Vec<PatternArm>,
}

#[derive(Clone, Debug)]
pub struct ArmResult {
    pub loc: Loc,
    pub bindings: PatBindings,
    pub guard: Option<Box<T::Exp>>,
    pub arm: Arm,
}

//**************************************************************************************************
// Match Context
//**************************************************************************************************

/// A shared match context trait for use with counterexample generation in Typing and match
/// compilation in HLIR lowering.
pub trait MatchContext<const AFTER_TYPING: bool> {
    fn env(&self) -> &CompilationEnv;
    fn reporter(&self) -> &DiagnosticReporter;
    fn new_match_var(&mut self, name: String, loc: Loc) -> N::Var;
    fn program_info(&self) -> &ProgramInfo<AFTER_TYPING>;

    //********************************************
    // Helpers for Compiling Pattern Matricies
    //********************************************

    fn make_imm_ref_match_binders(
        &mut self,
        decl_fields: UniqueMap<Field, usize>,
        pattern_loc: Loc,
        arg_types: Fields<N::Type>,
    ) -> Vec<(Field, N::Var, N::Type)> {
        fn make_imm_ref_ty(ty: N::Type) -> N::Type {
            match ty {
                sp!(_, N::Type_::Ref(false, _)) => ty,
                sp!(loc, N::Type_::Ref(true, inner)) => sp(loc, N::Type_::Ref(false, inner)),
                ty => {
                    let loc = ty.loc;
                    sp(loc, N::Type_::Ref(false, Box::new(ty)))
                }
            }
        }

        let fields = order_fields_by_decl(decl_fields, arg_types.clone());
        fields
            .into_iter()
            .map(|(_, field_name, field_type)| {
                (
                    field_name,
                    self.new_match_var(field_name.to_string(), pattern_loc),
                    make_imm_ref_ty(field_type),
                )
            })
            .collect::<Vec<_>>()
    }

    fn make_unpack_binders(
        &mut self,
        decl_fields: UniqueMap<Field, usize>,
        pattern_loc: Loc,
        arg_types: Fields<N::Type>,
    ) -> Vec<(Field, N::Var, N::Type)> {
        let fields = order_fields_by_decl(decl_fields, arg_types.clone());
        fields
            .into_iter()
            .map(|(_, field_name, field_type)| {
                (
                    field_name,
                    self.new_match_var(field_name.to_string(), pattern_loc),
                    field_type,
                )
            })
            .collect::<Vec<_>>()
    }
}

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl FringeEntry {
    pub fn into_move_exp(self) -> T::Exp {
        let FringeEntry { var, ty } = self;
        let move_exp = T::UnannotatedExp_::Move {
            from_user: false,
            var,
        };
        T::exp(ty, sp(var.loc, move_exp))
    }
}

impl PatternArm {
    fn pattern_empty(&self) -> bool {
        self.pats.is_empty()
    }

    /// Returns true if every entry is a wildcard or binder.
    fn is_wild_arm(&self) -> bool {
        self.pats
            .iter()
            .all(|pat| matches!(pat.pat.value, TP::Wildcard | TP::Binder(_, _)))
    }

    fn all_wild_arm(&mut self, fringe: &VecDeque<FringeEntry>) -> Option<ArmResult> {
        if self.is_wild_arm() {
            let bindings = self.make_arm_bindings(fringe);
            let PatternArm {
                pats: _,
                guard,
                arm,
            } = self;
            let arm = ArmResult {
                loc: arm.orig_pattern.pat.loc,
                bindings,
                guard: guard.clone(),
                arm: arm.clone(),
            };
            Some(arm)
        } else {
            None
        }
    }

    fn make_arm_bindings(&mut self, fringe: &VecDeque<FringeEntry>) -> PatBindings {
        let mut bindings = BTreeMap::new();
        assert!(self.pats.len() == fringe.len());
        for (pmut, subject) in self.pats.iter_mut().zip(fringe.iter()) {
            if let TP::Binder(mut_, x) = pmut.pat.value {
                if bindings.insert(x, (mut_, subject.clone())).is_some() {
                    panic!("ICE should have failed in naming");
                };
                pmut.pat.value = TP::Wildcard;
            }
        }
        bindings
    }

    fn first_variant(&self) -> Option<(VariantName, (Loc, Fields<Type>))> {
        if self.pats.is_empty() {
            return None;
        }

        fn first_variant_recur(pat: MatchPattern) -> Option<(VariantName, (Loc, Fields<Type>))> {
            match pat.pat.value {
                TP::Variant(_, _, name, _, fields) => {
                    let ty_fields: Fields<Type> = fields.clone().map(|_, (ndx, (ty, _))| (ndx, ty));
                    Some((name, (pat.pat.loc, ty_fields)))
                }
                TP::BorrowVariant(_, _, _, name, _, fields) => {
                    let ty_fields: Fields<Type> = fields.clone().map(|_, (ndx, (ty, _))| (ndx, ty));
                    Some((name, (pat.pat.loc, ty_fields)))
                }
                TP::At(_, inner) => first_variant_recur(*inner),
                TP::Struct(..) | TP::BorrowStruct(..) => None,
                TP::Constant(_, _)
                | TP::Binder(_, _)
                | TP::Literal(_)
                | TP::Wildcard
                | TP::ErrorPat => None,
                TP::Or(_, _) => unreachable!(),
            }
        }

        first_variant_recur(self.pats.front().unwrap().clone())
    }

    fn first_struct(&self) -> Option<(Loc, Fields<Type>)> {
        if self.pats.is_empty() {
            return None;
        }

        fn first_struct_recur(pat: MatchPattern) -> Option<(Loc, Fields<Type>)> {
            match pat.pat.value {
                TP::Struct(_, _, _, fields) => {
                    let ty_fields: Fields<Type> = fields.clone().map(|_, (ndx, (ty, _))| (ndx, ty));
                    Some((pat.pat.loc, ty_fields))
                }
                TP::BorrowStruct(_, _, _, _, fields) => {
                    let ty_fields: Fields<Type> = fields.clone().map(|_, (ndx, (ty, _))| (ndx, ty));
                    Some((pat.pat.loc, ty_fields))
                }
                TP::At(_, inner) => first_struct_recur(*inner),
                TP::Variant(..) | TP::BorrowVariant(..) => None,
                TP::Constant(_, _)
                | TP::Binder(_, _)
                | TP::Literal(_)
                | TP::Wildcard
                | TP::ErrorPat => None,
                TP::Or(_, _) => unreachable!(),
            }
        }

        first_struct_recur(self.pats.front().unwrap().clone())
    }

    fn first_lit(&self) -> Option<Value> {
        if self.pats.is_empty() {
            return None;
        }

        fn first_lit_recur(pat: MatchPattern) -> Option<Value> {
            match pat.pat.value {
                TP::Literal(v) => Some(v),
                TP::At(_, inner) => first_lit_recur(*inner),
                TP::Variant(_, _, _, _, _)
                | TP::BorrowVariant(_, _, _, _, _, _)
                | TP::Struct(..)
                | TP::BorrowStruct(..)
                | TP::Binder(_, _)
                | TP::Wildcard
                | TP::ErrorPat => None,
                TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
            }
        }

        first_lit_recur(self.pats.front().unwrap().clone())
    }

    fn specialize_variant<const AFTER_TYPING: bool, MC: MatchContext<AFTER_TYPING>>(
        &self,
        context: &MC,
        ctor_name: &VariantName,
        arg_types: &Vec<&Type>,
    ) -> Option<(Binders, PatternArm)> {
        let mut output = self.clone();
        let first_pattern = output.pats.pop_front().unwrap();
        let loc = first_pattern.pat.loc;
        match first_pattern.pat.value {
            TP::Variant(mident, enum_, name, _, fields)
            | TP::BorrowVariant(_, mident, enum_, name, _, fields)
                if &name == ctor_name =>
            {
                let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));
                let decl_fields = context
                    .program_info()
                    .enum_variant_fields(&mident, &enum_, &name)
                    .unwrap();
                let ordered_pats = order_fields_by_decl(decl_fields, field_pats);
                for (_, _, pat) in ordered_pats.into_iter().rev() {
                    output.pats.push_front(pat);
                }
                Some((vec![], output))
            }
            TP::Variant(_, _, _, _, _) | TP::BorrowVariant(_, _, _, _, _, _) => None,
            TP::Struct(_, _, _, _) | TP::BorrowStruct(_, _, _, _, _) => None,
            TP::Literal(_) => None,
            TP::Binder(mut_, x) => {
                for arg_type in arg_types
                    .clone()
                    .into_iter()
                    .map(|ty| ty_to_wildcard_pattern(ty.clone(), loc))
                    .rev()
                {
                    output.pats.push_front(arg_type);
                }
                Some((vec![(mut_, x)], output))
            }
            TP::Wildcard => {
                for arg_type in arg_types
                    .clone()
                    .into_iter()
                    .map(|ty| ty_to_wildcard_pattern(ty.clone(), loc))
                    .rev()
                {
                    output.pats.push_front(arg_type);
                }
                Some((vec![], output))
            }
            TP::At(x, inner) => {
                output.pats.push_front(*inner);
                output
                    .specialize_variant(context, ctor_name, arg_types)
                    .map(|(mut binders, inner)| {
                        binders.push((Mutability::Imm, x));
                        (binders, inner)
                    })
            }
            TP::ErrorPat => None,
            TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
        }
    }

    fn specialize_struct<const AFTER_TYPING: bool, MC: MatchContext<AFTER_TYPING>>(
        &self,
        context: &MC,
        arg_types: &Vec<&Type>,
    ) -> Option<(Binders, PatternArm)> {
        let mut output = self.clone();
        let first_pattern = output.pats.pop_front().unwrap();
        let loc = first_pattern.pat.loc;
        match first_pattern.pat.value {
            TP::Struct(mident, struct_, _, fields)
            | TP::BorrowStruct(_, mident, struct_, _, fields) => {
                let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));
                let decl_fields = context
                    .program_info()
                    .struct_fields(&mident, &struct_)
                    .unwrap();
                let ordered_pats = order_fields_by_decl(decl_fields, field_pats);
                for (_, _, pat) in ordered_pats.into_iter().rev() {
                    output.pats.push_front(pat);
                }
                Some((vec![], output))
            }
            TP::Literal(_) => None,
            TP::Variant(_, _, _, _, _) | TP::BorrowVariant(_, _, _, _, _, _) => None,
            TP::Binder(mut_, x) => {
                for arg_type in arg_types
                    .clone()
                    .into_iter()
                    .map(|ty| ty_to_wildcard_pattern(ty.clone(), loc))
                    .rev()
                {
                    output.pats.push_front(arg_type);
                }
                Some((vec![(mut_, x)], output))
            }
            TP::Wildcard => {
                for arg_type in arg_types
                    .clone()
                    .into_iter()
                    .map(|ty| ty_to_wildcard_pattern(ty.clone(), loc))
                    .rev()
                {
                    output.pats.push_front(arg_type);
                }
                Some((vec![], output))
            }
            TP::At(x, inner) => {
                output.pats.push_front(*inner);
                output
                    .specialize_struct(context, arg_types)
                    .map(|(mut binders, inner)| {
                        binders.push((Mutability::Imm, x));
                        (binders, inner)
                    })
            }
            TP::ErrorPat => None,
            TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
        }
    }

    fn specialize_literal(&self, literal: &Value) -> Option<(Binders, PatternArm)> {
        let mut output = self.clone();
        let first_pattern = output.pats.pop_front().unwrap();
        match first_pattern.pat.value {
            TP::Literal(v) if &v == literal => Some((vec![], output)),
            TP::Literal(_) => None,
            TP::Variant(_, _, _, _, _) | TP::BorrowVariant(_, _, _, _, _, _) => None,
            TP::Struct(_, _, _, _) | TP::BorrowStruct(_, _, _, _, _) => None,
            TP::Binder(mut_, x) => Some((vec![(mut_, x)], output)),
            TP::Wildcard => Some((vec![], output)),
            TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
            TP::At(x, inner) => {
                output.pats.push_front(*inner);
                output
                    .specialize_literal(literal)
                    .map(|(mut binders, inner)| {
                        binders.push((Mutability::Imm, x));
                        (binders, inner)
                    })
            }
            TP::ErrorPat => None,
        }
    }

    fn specialize_default(&self) -> Option<(Binders, PatternArm)> {
        let mut output = self.clone();
        let first_pattern = output.pats.pop_front().unwrap();
        match first_pattern.pat.value {
            TP::Literal(_) => None,
            TP::Variant(_, _, _, _, _) | TP::BorrowVariant(_, _, _, _, _, _) => None,
            TP::Struct(_, _, _, _) | TP::BorrowStruct(_, _, _, _, _) => None,
            TP::Binder(mut_, x) => Some((vec![(mut_, x)], output)),
            TP::Wildcard => Some((vec![], output)),
            TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
            TP::At(x, inner) => {
                output.pats.push_front(*inner);
                output.specialize_default().map(|(mut binders, inner)| {
                    binders.push((Mutability::Imm, x));
                    (binders, inner)
                })
            }
            TP::ErrorPat => None,
        }
    }
}

impl PatternMatrix {
    /// Converts a subject and match arms into a Pattern Matrix and the RHS arms (indexed by
    /// position in the vector). This process works as follows:
    /// - For each arm (pattern, guard, rhs):
    ///   - Add the RHS to the vector
    ///   - Split any OR patterns into their own pattern matrix entry, realizing each
    ///     independently. For each of these:
    ///     - Convert any CONSTANT patterns into a binding with a guard check for equality
    ///     - Store the original pattern on the entry, along with its RHS binders and arm index.
    ///     - Rewrite the pattern with the guard binders.
    ///     - Insert these resultant pattern into the pattern matrix, using the RHS index as the
    ///       index for all of them.
    pub fn from<const AFTER_TYPING: bool, MC: MatchContext<AFTER_TYPING>>(
        context: &mut MC,
        loc: Loc,
        subject_ty: Type,
        arms: Vec<T::MatchArm>,
    ) -> (PatternMatrix, Vec<T::Exp>) {
        let tys = vec![subject_ty];
        let mut patterns = vec![];
        let mut rhss = vec![];
        for sp!(_, arm) in arms {
            let MatchArm_ {
                pattern,
                binders: _,
                guard,
                guard_binders,
                rhs_binders,
                rhs,
            } = arm;
            rhss.push(*rhs);
            let index = rhss.len() - 1;
            let new_patterns = flatten_or(pattern);
            for mut pat in new_patterns {
                let (guard, const_binders) = const_pats_to_guards(context, &mut pat, guard.clone());
                let arm = Arm {
                    orig_pattern: pat.clone(),
                    rhs_binders: rhs_binders.clone(),
                    index,
                };
                // Make a match pattern that only holds guard binders
                let guard_binders = guard_binders.union_with(&const_binders, |k, _, x| {
                    let msg = "Match compilation made a binder for this during const compilation";
                    context.reporter().add_diag(ice!((k.loc, msg)));
                    *x
                });
                let pat = apply_pattern_subst(pat, &guard_binders);
                patterns.push(PatternArm {
                    pats: VecDeque::from([pat]),
                    guard,
                    arm,
                });
            }
        }
        (PatternMatrix { tys, loc, patterns }, rhss)
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    pub fn patterns_empty(&self) -> bool {
        !self.patterns.is_empty() && self.patterns.iter().all(|pat| pat.pattern_empty())
    }

    pub fn all_errors(&self) -> bool {
        self.patterns.iter().all(|arm| {
            arm.pats
                .iter()
                .all(|pat| matches!(pat.pat.value, TP::ErrorPat))
        })
    }

    /// Returns true if there is an arm made up entirely of wildcards / binders with no guard.
    pub fn has_default_arm(&self) -> bool {
        self.patterns
            .iter()
            .any(|pat| pat.is_wild_arm() && pat.guard.is_none())
    }

    pub fn wild_tree_opt(&mut self, fringe: &VecDeque<FringeEntry>) -> Option<Vec<ArmResult>> {
        // NB: If the first row is all wild, we need to collect _all_ wild rows that have guards
        // until we find one that does not. If we do not find one without a guard, then this isn't
        // a wild tree.
        if let Some(arm) = self.patterns[0].all_wild_arm(fringe) {
            if arm.guard.is_none() {
                return Some(vec![arm]);
            }
            let mut result = vec![arm];
            for pat in self.patterns[1..].iter_mut() {
                if let Some(arm) = pat.all_wild_arm(fringe) {
                    let has_guard = arm.guard.is_some();
                    result.push(arm);
                    if !has_guard {
                        return Some(result);
                    }
                }
            }
            None
        } else {
            None
        }
    }

    pub fn specialize_variant<const AFTER_TYPING: bool, MC: MatchContext<AFTER_TYPING>>(
        &self,
        context: &MC,
        ctor_name: &VariantName,
        arg_types: Vec<&Type>,
    ) -> (Binders, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        let loc = self.loc;
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) =
                entry.specialize_variant(context, ctor_name, &arg_types)
            {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let tys = arg_types
            .into_iter()
            .cloned()
            .chain(self.tys.clone().into_iter().skip(1))
            .collect::<Vec<_>>();
        let matrix = PatternMatrix { tys, loc, patterns };
        (bindings, matrix)
    }

    pub fn specialize_struct<const AFTER_TYPING: bool, MC: MatchContext<AFTER_TYPING>>(
        &self,
        context: &MC,
        arg_types: Vec<&Type>,
    ) -> (Binders, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        let loc = self.loc;
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) = entry.specialize_struct(context, &arg_types) {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let tys = arg_types
            .into_iter()
            .cloned()
            .chain(self.tys.clone().into_iter().skip(1))
            .collect::<Vec<_>>();
        let matrix = PatternMatrix { tys, loc, patterns };
        (bindings, matrix)
    }

    pub fn specialize_literal(&self, lit: &Value) -> (Binders, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        let loc = self.loc;
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) = entry.specialize_literal(lit) {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let tys = self.tys[1..].to_vec();
        let matrix = PatternMatrix { tys, loc, patterns };
        (bindings, matrix)
    }

    pub fn specialize_default(&self) -> (Binders, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        let loc = self.loc;
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) = entry.specialize_default() {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let tys = self.tys[1..].to_vec();
        let matrix = PatternMatrix { tys, loc, patterns };
        (bindings, matrix)
    }

    pub fn first_variant_ctors(&self) -> BTreeMap<VariantName, (Loc, Fields<Type>)> {
        self.patterns
            .iter()
            .flat_map(|pat| pat.first_variant())
            .collect()
    }

    pub fn first_struct_ctors(&self) -> Option<(Loc, Fields<Type>)> {
        self.patterns.iter().find_map(|pat| pat.first_struct())
    }

    pub fn first_lits(&self) -> BTreeSet<Value> {
        self.patterns
            .iter()
            .flat_map(|pat| pat.first_lit())
            .collect()
    }

    pub fn has_guards(&self) -> bool {
        self.patterns.iter().any(|pat| pat.guard.is_some())
    }

    pub fn remove_guarded_arms(&mut self) {
        let pats = std::mem::take(&mut self.patterns);
        self.patterns = pats.into_iter().filter(|pat| pat.guard.is_none()).collect();
    }
}

//**************************************************************************************************
// Helper Functions
//**************************************************************************************************

fn ty_to_wildcard_pattern(ty: Type, loc: Loc) -> T::MatchPattern {
    T::MatchPattern {
        ty,
        pat: sp(loc, T::UnannotatedPat_::Wildcard),
    }
}

// NB: this converts any binders not in `env` to wildcards, and strips any `at` pattern binders
// that is not in the `env`
fn apply_pattern_subst(pat: MatchPattern, env: &UniqueMap<Var, Var>) -> MatchPattern {
    let MatchPattern {
        ty,
        pat: sp!(ploc, pat),
    } = pat;
    let new_pat = match pat {
        TP::Variant(m, e, v, ta, spats) => {
            let out_fields =
                spats.map(|_, (ndx, (t, pat))| (ndx, (t, apply_pattern_subst(pat, env))));
            TP::Variant(m, e, v, ta, out_fields)
        }
        TP::BorrowVariant(mut_, m, e, v, ta, spats) => {
            let out_fields =
                spats.map(|_, (ndx, (t, pat))| (ndx, (t, apply_pattern_subst(pat, env))));
            TP::BorrowVariant(mut_, m, e, v, ta, out_fields)
        }
        TP::Struct(m, s, ta, spats) => {
            let out_fields =
                spats.map(|_, (ndx, (t, pat))| (ndx, (t, apply_pattern_subst(pat, env))));
            TP::Struct(m, s, ta, out_fields)
        }
        TP::BorrowStruct(mut_, m, s, ta, spats) => {
            let out_fields =
                spats.map(|_, (ndx, (t, pat))| (ndx, (t, apply_pattern_subst(pat, env))));
            TP::BorrowStruct(mut_, m, s, ta, out_fields)
        }
        TP::At(x, inner) => {
            let xloc = x.loc;
            // Since we are only applying the guard environment, this may be unused here.
            // If it is, we simply elide the `@` form.
            if let Some(y) = env.get(&x) {
                TP::At(
                    sp(xloc, y.value),
                    Box::new(apply_pattern_subst(*inner, env)),
                )
            } else {
                apply_pattern_subst(*inner, env).pat.value
            }
        }
        TP::Binder(mut_, x) => {
            let xloc = x.loc;
            if let Some(y) = env.get(&x) {
                TP::Binder(mut_, sp(xloc, y.value))
            } else {
                TP::Wildcard
            }
        }
        pat @ (TP::Literal(_) | TP::ErrorPat | TP::Wildcard) => pat,
        TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
    };
    MatchPattern {
        ty,
        pat: sp(ploc, new_pat),
    }
}

fn flatten_or(pat: MatchPattern) -> Vec<MatchPattern> {
    let ploc = pat.pat.loc;
    match pat.pat.value {
        TP::Constant(_, _) | TP::Literal(_) | TP::Binder(_, _) | TP::Wildcard | TP::ErrorPat => {
            vec![pat]
        }
        TP::Variant(_, _, _, _, ref pats)
        | TP::BorrowVariant(_, _, _, _, _, ref pats)
        | TP::Struct(_, _, _, ref pats)
        | TP::BorrowStruct(_, _, _, _, ref pats)
            if pats.is_empty() =>
        {
            vec![pat]
        }
        TP::Variant(m, e, v, ta, spats) => {
            let all_spats = spats.map(|_, (ndx, (t, pat))| (ndx, (t, flatten_or(pat))));
            let fields_lists: Vec<Fields<(Type, MatchPattern)>> = combine_pattern_fields(all_spats);
            fields_lists
                .into_iter()
                .map(|field_list| MatchPattern {
                    ty: pat.ty.clone(),
                    pat: sp(ploc, TP::Variant(m, e, v, ta.clone(), field_list)),
                })
                .collect::<Vec<_>>()
        }
        TP::BorrowVariant(mut_, m, e, v, ta, spats) => {
            let all_spats = spats.map(|_, (ndx, (t, pat))| (ndx, (t, flatten_or(pat))));
            let fields_lists: Vec<Fields<(Type, MatchPattern)>> = combine_pattern_fields(all_spats);
            fields_lists
                .into_iter()
                .map(|field_list| MatchPattern {
                    ty: pat.ty.clone(),
                    pat: sp(
                        ploc,
                        TP::BorrowVariant(mut_, m, e, v, ta.clone(), field_list),
                    ),
                })
                .collect::<Vec<_>>()
        }
        TP::Struct(m, s, ta, spats) => {
            let all_spats = spats.map(|_, (ndx, (t, pat))| (ndx, (t, flatten_or(pat))));
            let fields_lists: Vec<Fields<(Type, MatchPattern)>> = combine_pattern_fields(all_spats);
            fields_lists
                .into_iter()
                .map(|field_list| MatchPattern {
                    ty: pat.ty.clone(),
                    pat: sp(ploc, TP::Struct(m, s, ta.clone(), field_list)),
                })
                .collect::<Vec<_>>()
        }
        TP::BorrowStruct(mut_, m, s, ta, spats) => {
            let all_spats = spats.map(|_, (ndx, (t, pat))| (ndx, (t, flatten_or(pat))));
            let fields_lists: Vec<Fields<(Type, MatchPattern)>> = combine_pattern_fields(all_spats);
            fields_lists
                .into_iter()
                .map(|field_list| MatchPattern {
                    ty: pat.ty.clone(),
                    pat: sp(ploc, TP::BorrowStruct(mut_, m, s, ta.clone(), field_list)),
                })
                .collect::<Vec<_>>()
        }
        TP::Or(lhs, rhs) => {
            let mut lhs_rec = flatten_or(*lhs);
            let mut rhs_rec = flatten_or(*rhs);
            lhs_rec.append(&mut rhs_rec);
            lhs_rec
        }
        TP::At(x, inner) => flatten_or(*inner)
            .into_iter()
            .map(|pat| MatchPattern {
                ty: pat.ty.clone(),
                pat: sp(ploc, TP::At(x, Box::new(pat))),
            })
            .collect::<Vec<_>>(),
    }
}

// A BRIEF OVERVIEW OF CONSTANT MATCH HANDLING
//
// To handle constants, we need to do two things: first, we need to replace all of the constants in
// the patterns with _something_, and then we need to push the constant checks into guards in the
// right-hand side. We take advantage of existing assumptions for match compilation to accomplish
// this, allowing us to reuse the existing match compilation machinery:
//
// 1. We traverse the pattern mutably, replacing all constants with new, fresh variables.
// 2. We generate a new 'guard' variable that acts as the "guard variable map" entry for that
//    binder, indicating that this guard variable should be bound during match decision tree
//    compilation for guard checking. This guard variable, as all, is typed as an immutable
//    reference of the value in question.
// 3. We generate a guard check `guard_var == &const`.
//
// Finally, we hand back:
//
// 1. a new guard expression made up of the new guards plus the old guard (if any), in that order;
// 2. and a guard binder map that maps the new pattern variable to their new guard versions.
//
// As an example:
//
//  match (Option::Some(5)) {
//    Option::Some(y @ CONST) if (y#guard == 0) => rhs0,
//    Option::Some(x) if (x#guard == 1) => rhs1,
//    _ => rhs2
//  }
//
// will be translated as:
//
//  match (Option::Some(5)) {
//    Option::Some(y @ _match_var) if (_match_var#guard == &CONST && y#guard == 0) => rhs0,
//    Option::Some(x) if (x#guard == 1) => rhs1,
//    _ => rhs2
//  }
//
// At this point, match compilation can proceed normally.
//
// NB: Since `_match_var` is not in the `rhs_binders` list, it will be erased in the final arm.

/// Assumes `flatten_or` has already been performed.
fn const_pats_to_guards<const AFTER_TYPING: bool, MC: MatchContext<AFTER_TYPING>>(
    context: &mut MC,
    pat: &mut MatchPattern,
    guard: Option<Box<T::Exp>>,
) -> (Option<Box<T::Exp>>, UniqueMap<Var, Var>) {
    #[growing_stack]
    fn convert_recur<const AFTER_TYPING: bool, MC: MatchContext<AFTER_TYPING>>(
        context: &mut MC,
        input: &mut MatchPattern,
        guard_exps: &mut Vec<T::Exp>,
        guard_map: &mut UniqueMap<Var, Var>,
    ) {
        match &mut input.pat.value {
            TP::Constant(m, const_) => {
                let loc = input.pat.loc;
                let pat_var = context.new_match_var("const".to_string(), loc);
                let guard_var = context.new_match_var("const_guard".to_string(), loc);
                guard_map.add(pat_var, guard_var).expect("This cannot fail");
                let guard_exp = make_const_test(input.ty.clone(), guard_var, loc, *m, *const_);
                let MatchPattern { ty, pat: _ } = input;
                guard_exps.push(guard_exp);
                *input = T::pat(ty.clone(), sp(loc, TP::Binder(Mutability::Imm, pat_var)));
            }
            TP::Variant(_, _, _, _, fields)
            | TP::BorrowVariant(_, _, _, _, _, fields)
            | TP::Struct(_, _, _, fields)
            | TP::BorrowStruct(_, _, _, _, fields) => {
                for (_, _, (_, (_, rhs))) in fields.iter_mut() {
                    convert_recur(context, rhs, guard_exps, guard_map);
                }
            }
            TP::At(_, inner) => convert_recur(context, inner, guard_exps, guard_map),
            TP::Literal(_) | TP::Binder(_, _) | TP::Wildcard | TP::ErrorPat => (),
            TP::Or(_, _) => unreachable!(),
        }
    }

    fn combine_guards(mut guards: Vec<T::Exp>, guard: Option<Box<T::Exp>>) -> Option<Box<T::Exp>> {
        if let Some(guard) = guard {
            guards.push(*guard);
        }
        let Some(mut guard) = guards.pop() else {
            return None;
        };
        while let Some(new_guard) = guards.pop() {
            // FIXME: No good `Loc` to use here...
            guard = make_and_test(new_guard.exp.loc, new_guard, guard);
        }
        Some(Box::new(guard))
    }

    let mut const_guards = vec![];
    let mut const_guard_map = UniqueMap::new();

    convert_recur(context, pat, &mut const_guards, &mut const_guard_map);
    (combine_guards(const_guards, guard), const_guard_map)
}

fn combine_pattern_fields(
    fields: Fields<(Type, Vec<MatchPattern>)>,
) -> Vec<Fields<(Type, MatchPattern)>> {
    type VFields = Vec<(Field, (usize, (Spanned<N::Type_>, MatchPattern)))>;
    type VVFields = Vec<(Field, (usize, (Spanned<N::Type_>, Vec<MatchPattern>)))>;

    fn combine_recur(vec: &mut VVFields) -> Vec<VFields> {
        if let Some((f, (ndx, (ty, pats)))) = vec.pop() {
            let rec_fields = combine_recur(vec);
            let mut output = vec![];
            for entry in rec_fields {
                for pat in pats.clone() {
                    let mut entry = entry.clone();
                    entry.push((f, (ndx, (ty.clone(), pat))));
                    output.push(entry);
                }
            }
            output
        } else {
            // Base case: a single match of no fields. We must have at least one, or else we would
            // not have called `combine_match_patterns`.
            vec![vec![]]
        }
    }

    let mut vvfields: VVFields = fields.into_iter().collect::<Vec<_>>();
    let output_vec = combine_recur(&mut vvfields);
    output_vec
        .into_iter()
        .map(|vfields| UniqueMap::maybe_from_iter(vfields.into_iter()).unwrap())
        .collect::<Vec<_>>()
}

/// Helper function for creating an ordered list of fields Field information and Fields.
pub fn order_fields_by_decl<T: std::fmt::Debug>(
    decl_fields: UniqueMap<Field, usize>,
    fields: Fields<T>,
) -> Vec<(usize, Field, T)> {
    let mut texp_fields: Vec<(usize, Field, T)> = fields
        .into_iter()
        .map(|(f, (_exp_idx, t))| (*decl_fields.get(&f).unwrap(), f, t))
        .collect();
    texp_fields.sort_by(|(decl_idx1, _, _), (decl_idx2, _, _)| decl_idx1.cmp(decl_idx2));
    texp_fields
}

pub const MATCH_TEMP_PREFIX: &str = "__match_tmp%";

// Expression Creation Helpers
// NOTE: this _must_ be a string that a user cannot write, as otherwise we could incorrectly shadow
// macro-expanded names.
/// Create a new name for match variable usage.
pub fn new_match_var_name(name: &str, id: usize) -> Symbol {
    format!("{MATCH_TEMP_PREFIX}{NEW_NAME_DELIM}{name}{NEW_NAME_DELIM}{id}").into()
}

// Since these are guards, we need to borrow the constant
fn make_const_test(ty: N::Type, var: N::Var, loc: Loc, m: ModuleIdent, c: ConstantName) -> T::Exp {
    use T::UnannotatedExp_ as E;
    let base_ty = sp(loc, ty.value.base_type_());
    let ref_ty = sp(loc, N::Type_::Ref(false, Box::new(base_ty.clone())));
    let var_exp = T::exp(
        ref_ty.clone(),
        sp(
            loc,
            E::Move {
                from_user: false,
                var,
            },
        ),
    );
    let const_exp = {
        // We're in a guard, so we need to borrow the constant immutable.
        let const_exp = T::exp(base_ty, sp(loc, E::Constant(m, c)));
        let borrow_exp = E::TempBorrow(false, Box::new(const_exp));
        Box::new(T::exp(ref_ty, sp(loc, borrow_exp)))
    };
    make_eq_test(loc, var_exp, *const_exp)
}

pub fn make_eq_test(loc: Loc, lhs: T::Exp, rhs: T::Exp) -> T::Exp {
    let bool = N::Type_::bool(loc);
    let equality_exp_ = T::UnannotatedExp_::BinopExp(
        Box::new(lhs),
        sp(loc, BinOp_::Eq),
        Box::new(bool.clone()),
        Box::new(rhs),
    );
    T::exp(bool, sp(loc, equality_exp_))
}

fn make_and_test(loc: Loc, lhs: T::Exp, rhs: T::Exp) -> T::Exp {
    let bool = N::Type_::bool(loc);
    let equality_exp_ = T::UnannotatedExp_::BinopExp(
        Box::new(lhs),
        sp(loc, BinOp_::And),
        Box::new(bool.clone()),
        Box::new(rhs),
    );
    T::exp(bool, sp(loc, equality_exp_))
}
