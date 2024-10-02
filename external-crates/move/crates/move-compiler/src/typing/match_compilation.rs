// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    expansion::ast::{Fields, ModuleIdent, Mutability, Value, Value_},
    ice, ice_assert,
    naming::ast::{self as N, BuiltinTypeName_, Type, UseFuns, Var},
    parser::ast::{BinOp_, ConstantName, DatatypeName, Field, VariantName},
    shared::{
        ast_debug::{AstDebug, AstWriter},
        ide::{IDEAnnotation, MissingMatchArmsInfo, PatternSuggestion},
        string_utils::{debug_print, format_oxford_list},
        unique_map::UniqueMap,
        Identifier,
    },
    typing::ast::{self as T, MatchArm_, MatchPattern, UnannotatedPat_ as TP},
    typing::core::{error_format, Context, Subst},
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Display,
};

use super::visitor::TypingVisitorContext;

//**************************************************************************************************
// Description
//**************************************************************************************************
// This mostly follows the classical Maranget (2008) implementation toward optimal decision trees.

//**************************************************************************************************
// Entry and Visitor
//**************************************************************************************************

struct MatchCompiler<'ctx, 'env> {
    context: &'ctx mut Context<'env>,
}

impl TypingVisitorContext for MatchCompiler<'_, '_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        use T::UnannotatedExp_ as E;
        if let E::Match(_, _) = &exp.exp.value {
            let E::Match(mut subject, arms) =
                std::mem::replace(&mut exp.exp.value, E::UnresolvedError)
            else {
                unreachable!()
            };
            self.visit_exp(&mut subject);
            debug_print!(self.context.debug.match_translation,
                ("subject" => subject),
                (lines "arms" => &arms.value)
            );
            let _ = std::mem::replace(exp, compile_match(self.context, &exp.ty, *subject, arms));
            debug_print!(self.context.debug.match_translation, ("compiled" => exp));
            true
        } else {
            false
        }
    }

    fn add_warning_filter_scope(&mut self, filter: crate::diagnostics::WarningFilters) {
        self.context.env.add_warning_filter_scope(filter);
    }

    fn pop_warning_filter_scope(&mut self) {
        self.context.env.pop_warning_filter_scope();
    }
}

pub fn function_body_(context: &mut Context, b_: &mut T::FunctionBody_) {
    match b_ {
        T::FunctionBody_::Native | T::FunctionBody_::Macro => (),
        T::FunctionBody_::Defined(es) => {
            let mut compiler = MatchCompiler { context };
            compiler.visit_seq(es);
        }
    }
}

//**************************************************************************************************
// Match Trees
//**************************************************************************************************

#[derive(Clone, Debug)]
struct FringeEntry {
    var: Var,
    ty: Type,
}

type Binders = Vec<(Mutability, Var)>;
type PatBindings = BTreeMap<Var, (Mutability, FringeEntry)>;
type Guard = Option<Box<T::Exp>>;

#[derive(Clone, Debug)]
struct Arm {
    orig_pattern: MatchPattern,
    rhs_binders: BTreeSet<Var>,
    index: usize,
}

#[derive(Clone, Debug)]
struct PatternArm {
    pats: VecDeque<T::MatchPattern>,
    guard: Guard,
    arm: Arm,
}

#[derive(Clone, Debug)]
struct PatternMatrix {
    tys: Vec<Type>,
    patterns: Vec<PatternArm>,
}

#[derive(Clone, Debug)]
struct ArmResult {
    loc: Loc,
    bindings: PatBindings,
    guard: Option<Box<T::Exp>>,
    arm: Arm,
}

impl FringeEntry {
    fn into_move_exp(self) -> T::Exp {
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

    fn specialize_variant(
        &self,
        context: &Context,
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
                let decl_fields = context.enum_variant_fields(&mident, &enum_, &name);
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

    fn specialize_struct(
        &self,
        context: &Context,
        arg_types: &Vec<&Type>,
    ) -> Option<(Binders, PatternArm)> {
        let mut output = self.clone();
        let first_pattern = output.pats.pop_front().unwrap();
        let loc = first_pattern.pat.loc;
        match first_pattern.pat.value {
            TP::Struct(mident, struct_, _, fields)
            | TP::BorrowStruct(_, mident, struct_, _, fields) => {
                let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));
                let decl_fields = context.struct_fields(&mident, &struct_);
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
    fn from(
        context: &mut Context,
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
                debug_print!(context.debug.match_constant_conversion, ("with consts" => pat));
                let (guard, const_binders) = const_pats_to_guards(context, &mut pat, guard.clone());
                debug_print!(context.debug.match_constant_conversion, ("no consts" => pat));
                let arm = Arm {
                    orig_pattern: pat.clone(),
                    rhs_binders: rhs_binders.clone(),
                    index,
                };
                // Make a match pattern that only holds guard binders
                let guard_binders = guard_binders.union_with(&const_binders, |k, _, x| {
                    let msg = "Match compilation made a binder for this during const compilation";
                    context.env.add_diag(ice!((k.loc, msg)));
                    *x
                });
                let pat = apply_pattern_subst(pat, &guard_binders);
                debug_print!(context.debug.match_constant_conversion, ("after subst" => pat));
                patterns.push(PatternArm {
                    pats: VecDeque::from([pat]),
                    guard,
                    arm,
                });
            }
        }
        (PatternMatrix { tys, patterns }, rhss)
    }

    fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    fn patterns_empty(&self) -> bool {
        !self.patterns.is_empty() && self.patterns.iter().all(|pat| pat.pattern_empty())
    }

    fn all_errors(&self) -> bool {
        self.patterns.iter().all(|arm| {
            arm.pats
                .iter()
                .all(|pat| matches!(pat.pat.value, TP::ErrorPat))
        })
    }

    /// Returns true if there is an arm made up entirely of wildcards / binders with no guard.
    fn has_default_arm(&self) -> bool {
        self.patterns
            .iter()
            .any(|pat| pat.is_wild_arm() && pat.guard.is_none())
    }

    fn wild_arm_opt(&mut self, fringe: &VecDeque<FringeEntry>) -> Option<Vec<ArmResult>> {
        // NB: If the first row is all wild, we need to collect _all_ wild rows that have guards
        // until we find one that does not.
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
            Some(result)
        } else {
            None
        }
    }

    fn specialize_variant(
        &self,
        context: &Context,
        ctor_name: &VariantName,
        arg_types: Vec<&Type>,
    ) -> (Binders, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
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
        let matrix = PatternMatrix { tys, patterns };
        (bindings, matrix)
    }

    fn specialize_struct(
        &self,
        context: &Context,
        arg_types: Vec<&Type>,
    ) -> (Binders, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
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
        let matrix = PatternMatrix { tys, patterns };
        (bindings, matrix)
    }

    fn specialize_literal(&self, lit: &Value) -> (Binders, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) = entry.specialize_literal(lit) {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let tys = self.tys[1..].to_vec();
        let matrix = PatternMatrix { tys, patterns };
        (bindings, matrix)
    }

    fn specialize_default(&self) -> (Binders, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) = entry.specialize_default() {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let tys = self.tys[1..].to_vec();
        let matrix = PatternMatrix { tys, patterns };
        (bindings, matrix)
    }

    fn first_variant_ctors(&self) -> BTreeMap<VariantName, (Loc, Fields<Type>)> {
        self.patterns
            .iter()
            .flat_map(|pat| pat.first_variant())
            .collect()
    }

    fn first_struct_ctors(&self) -> Option<(Loc, Fields<Type>)> {
        self.patterns.iter().find_map(|pat| pat.first_struct())
    }

    fn first_lits(&self) -> BTreeSet<Value> {
        self.patterns
            .iter()
            .flat_map(|pat| pat.first_lit())
            .collect()
    }

    fn has_guards(&self) -> bool {
        self.patterns.iter().any(|pat| pat.guard.is_some())
    }

    fn remove_guarded_arms(&mut self) {
        let pats = std::mem::take(&mut self.patterns);
        self.patterns = pats.into_iter().filter(|pat| pat.guard.is_none()).collect();
    }
}

// -----------------------------------------------
// Pattern Helpers
// -----------------------------------------------

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
fn const_pats_to_guards(
    context: &mut Context,
    pat: &mut MatchPattern,
    guard: Option<Box<T::Exp>>,
) -> (Option<Box<T::Exp>>, UniqueMap<Var, Var>) {
    #[growing_stack]
    fn convert_recur(
        context: &mut Context,
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

//**************************************************************************************************
// Match Compilation
//**************************************************************************************************

type Fringe = VecDeque<FringeEntry>;

#[derive(Clone)]
enum StructUnpack<T> {
    Default(T),
    Unpack(Vec<(Field, Var, Type)>, T),
}

enum MatchStep {
    Leaf(Vec<ArmResult>),
    Failure,
    LiteralSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        fringe: Fringe,
        arms: BTreeMap<Value, PatternMatrix>,
        default: PatternMatrix,
    },
    StructUnpack {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        unpack: StructUnpack<(Fringe, PatternMatrix)>,
    },
    VariantSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        arms: BTreeMap<VariantName, (Vec<(Field, Var, Type)>, Fringe, PatternMatrix)>,
        default: (Fringe, PatternMatrix),
    },
}

#[derive(Clone)]
enum WorkResult {
    Leaf(Vec<ArmResult>),
    Failure,
    LiteralSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        arms: BTreeMap<Value, usize>,
        default: usize, // default
    },
    StructUnpack {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        unpack: StructUnpack<usize>,
    },
    VariantSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        arms: BTreeMap<VariantName, (Vec<(Field, Var, Type)>, usize)>,
        default: usize,
    },
}

pub fn compile_match(
    context: &mut Context,
    result_type: &Type,
    subject: T::Exp,
    arms: Spanned<Vec<T::MatchArm>>,
) -> T::Exp {
    let arms_loc = arms.loc;
    let (pattern_matrix, arms) = PatternMatrix::from(context, subject.ty.clone(), arms.value);

    let mut counterexample_matrix = pattern_matrix.clone();
    let has_guards = counterexample_matrix.has_guards();
    counterexample_matrix.remove_guarded_arms();
    if context.env.ide_mode() {
        // Do this first, as it's a borrow and a shallow walk.
        ide_report_missing_arms(context, arms_loc, &counterexample_matrix);
    }
    if find_counterexample(context, subject.exp.loc, counterexample_matrix, has_guards) {
        return T::exp(
            result_type.clone(),
            sp(subject.exp.loc, T::UnannotatedExp_::UnresolvedError),
        );
    }

    let mut compilation_results: BTreeMap<usize, WorkResult> = BTreeMap::new();

    let (mut initial_binders, init_subject, match_subject) = {
        let subject_var = context.new_match_var("unpack_subject".to_string(), arms_loc);
        let subject_loc = subject.exp.loc;
        let match_var = context.new_match_var("match_subject".to_string(), arms_loc);

        let subject_entry = FringeEntry {
            var: subject_var,
            ty: subject.ty.clone(),
        };
        let subject_borrow_rhs = make_var_ref(subject_entry.clone());

        let match_entry = FringeEntry {
            var: match_var,
            ty: subject_borrow_rhs.ty.clone(),
        };

        let subject_binder = {
            let lhs_loc = subject_loc;
            let lhs_lvalue = make_lvalue(subject_var, Mutability::Imm, subject.ty.clone());
            let binder = T::SequenceItem_::Bind(
                sp(lhs_loc, vec![lhs_lvalue]),
                vec![Some(subject.ty.clone())],
                Box::new(subject),
            );
            sp(lhs_loc, binder)
        };

        let subject_borrow = {
            let lhs_loc = arms_loc;
            let lhs_lvalue = make_lvalue(match_var, Mutability::Imm, subject_borrow_rhs.ty.clone());
            let binder = T::SequenceItem_::Bind(
                sp(lhs_loc, vec![lhs_lvalue]),
                vec![Some(subject_borrow_rhs.ty.clone())],
                subject_borrow_rhs,
            );
            sp(lhs_loc, binder)
        };

        (
            VecDeque::from([subject_binder, subject_borrow]),
            subject_entry,
            match_entry,
        )
    };

    let mut work_queue: Vec<(usize, Fringe, PatternMatrix)> =
        vec![(0, VecDeque::from([match_subject]), pattern_matrix)];

    let mut work_id = 0;

    let mut next_id = || {
        work_id += 1;
        work_id
    };

    while let Some((cur_id, init_fringe, matrix)) = work_queue.pop() {
        debug_print!(
            context.debug.match_work_queue,
            ("work queue entry" => cur_id; fmt),
            (lines "fringe" => &init_fringe; sdbg),
            ("matrix" => matrix; verbose)
        );
        let redefined: Option<WorkResult> =
            match compile_match_head(context, init_fringe.clone(), matrix) {
                MatchStep::Leaf(leaf) => compilation_results.insert(cur_id, WorkResult::Leaf(leaf)),
                MatchStep::Failure => compilation_results.insert(cur_id, WorkResult::Failure),
                MatchStep::LiteralSwitch {
                    subject,
                    subject_binders,
                    fringe,
                    arms,
                    default,
                } => {
                    let mut answer_map = BTreeMap::new();
                    for (value, matrix) in arms {
                        let work_id = next_id();
                        answer_map.insert(value, work_id);
                        work_queue.push((work_id, fringe.clone(), matrix));
                    }
                    let default_work_id = next_id();
                    work_queue.push((default_work_id, fringe, default));
                    let result = WorkResult::LiteralSwitch {
                        subject,
                        subject_binders,
                        arms: answer_map,
                        default: default_work_id,
                    };
                    compilation_results.insert(cur_id, result)
                }
                MatchStep::StructUnpack {
                    subject,
                    subject_binders,
                    tyargs,
                    unpack,
                } => {
                    let unpack_work_id = next_id();
                    let unpack = match unpack {
                        StructUnpack::Default((fringe, matrix)) => {
                            work_queue.push((unpack_work_id, fringe, matrix));
                            StructUnpack::Default(unpack_work_id)
                        }
                        StructUnpack::Unpack(dtor_fields, (fringe, matrix)) => {
                            work_queue.push((unpack_work_id, fringe, matrix));
                            StructUnpack::Unpack(dtor_fields, unpack_work_id)
                        }
                    };
                    compilation_results.insert(
                        cur_id,
                        WorkResult::StructUnpack {
                            subject,
                            subject_binders,
                            tyargs,
                            unpack,
                        },
                    )
                }

                MatchStep::VariantSwitch {
                    subject,
                    subject_binders,
                    tyargs,
                    arms,
                    default: (dfringe, dmatrix),
                } => {
                    let mut answer_map = BTreeMap::new();
                    for (name, (dtor_fields, fringe, matrix)) in arms {
                        let work_id = next_id();
                        answer_map.insert(name, (dtor_fields, work_id));
                        work_queue.push((work_id, fringe, matrix));
                    }
                    let default_work_id = next_id();
                    work_queue.push((default_work_id, dfringe, dmatrix));
                    compilation_results.insert(
                        cur_id,
                        WorkResult::VariantSwitch {
                            subject,
                            subject_binders,
                            tyargs,
                            arms: answer_map,
                            default: default_work_id,
                        },
                    )
                }
            };
        ice_assert!(
            context.env,
            redefined.is_none(),
            arms_loc,
            "Match work queue went awry"
        );
    }

    let match_start = compilation_results.remove(&0).unwrap();
    let mut resolution_context = ResolutionContext {
        hlir_context: context,
        output_type: result_type,
        arms: &arms,
        arms_loc,
        results: &mut compilation_results,
    };
    let match_exp = resolve_result(&mut resolution_context, &init_subject, match_start);

    let eloc = match_exp.exp.loc;
    let mut seq = VecDeque::new();
    seq.append(&mut initial_binders);
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(match_exp))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type.clone(), exp_value)
}

fn compile_match_head(
    context: &mut Context,
    mut fringe: VecDeque<FringeEntry>,
    mut matrix: PatternMatrix,
) -> MatchStep {
    debug_print!(
        context.debug.match_specialization,
        ("-----\ncompiling with fringe queue entry" => fringe; dbg)
    );
    if matrix.is_empty() {
        MatchStep::Failure
    } else if let Some(leaf) = matrix.wild_arm_opt(&fringe) {
        MatchStep::Leaf(leaf)
    } else if fringe[0].ty.value.unfold_to_builtin_type_name().is_some() {
        let subject = fringe
            .pop_front()
            .expect("ICE empty fringe in match compilation");
        let mut subject_binders = vec![];
        // treat column as a literal
        let lits = matrix.first_lits();
        let mut arms = BTreeMap::new();
        for lit in lits {
            let lit_loc = lit.loc;
            debug_print!(context.debug.match_specialization, ("lit specializing" => lit ; fmt));
            let (mut new_binders, inner_matrix) = matrix.specialize_literal(&lit);
            debug_print!(
                context.debug.match_specialization,
                ("binders" => &new_binders; dbg), ("specialized" => inner_matrix)
            );
            subject_binders.append(&mut new_binders);
            ice_assert!(
                context.env,
                arms.insert(lit, inner_matrix).is_none(),
                lit_loc,
                "Specialization failed"
            );
        }
        let (mut new_binders, default) = matrix.specialize_default();
        debug_print!(context.debug.match_specialization, ("default binders" => &new_binders; dbg));
        subject_binders.append(&mut new_binders);
        MatchStep::LiteralSwitch {
            subject,
            subject_binders,
            fringe,
            arms,
            default,
        }
    } else {
        let subject = fringe
            .pop_front()
            .expect("ICE empty fringe in match compilation");
        let tyargs = subject.ty.value.type_arguments().unwrap().clone();
        let mut subject_binders = vec![];
        debug_print!(
            context.debug.match_specialization,
            ("subject" => subject),
            ("matrix" => matrix)
        );
        let (mident, datatype_name) = subject
            .ty
            .value
            .unfold_to_type_name()
            .and_then(|sp!(_, name)| name.datatype_name())
            .expect("ICE non-datatype type in head constructor fringe position");

        if context.is_struct(&mident, &datatype_name) {
            // If we have an actual destructuring anywhere, we do that and take the specialized
            // matrix (which holds the default matrix and bindings, for our purpose). If we don't,
            // we just take the default matrix.
            let unpack = if let Some((ploc, arg_types)) = matrix.first_struct_ctors() {
                let fringe_binders = context.make_imm_ref_match_binders(ploc, arg_types);
                let fringe_exps = make_fringe_entries(&fringe_binders);
                let mut inner_fringe = fringe.clone();
                for fringe_exp in fringe_exps.into_iter().rev() {
                    inner_fringe.push_front(fringe_exp);
                }
                let bind_tys = fringe_binders
                    .iter()
                    .map(|(_, _, ty)| ty)
                    .collect::<Vec<_>>();
                debug_print!(
                    context.debug.match_specialization, ("struct specialized" => datatype_name; dbg)
                );
                let (mut new_binders, inner_matrix) = matrix.specialize_struct(context, bind_tys);
                debug_print!(context.debug.match_specialization,
                             ("binders" => new_binders; dbg),
                             ("specialized" => inner_matrix));
                subject_binders.append(&mut new_binders);
                StructUnpack::Unpack(fringe_binders, (inner_fringe, inner_matrix))
            } else {
                let (mut new_binders, default_matrix) = matrix.specialize_default();
                subject_binders.append(&mut new_binders);
                StructUnpack::Default((fringe, default_matrix))
            };
            MatchStep::StructUnpack {
                subject,
                subject_binders,
                tyargs,
                unpack,
            }
        } else {
            let mut unmatched_variants = context
                .enum_variants(&mident, &datatype_name)
                .into_iter()
                .collect::<BTreeSet<_>>();

            let ctors = matrix.first_variant_ctors();

            let mut arms = BTreeMap::new();
            for (ctor, (ploc, arg_types)) in ctors {
                unmatched_variants.remove(&ctor);
                let fringe_binders = context.make_imm_ref_match_binders(ploc, arg_types);
                let fringe_exps = make_fringe_entries(&fringe_binders);
                let mut inner_fringe = fringe.clone();
                for fringe_exp in fringe_exps.into_iter().rev() {
                    inner_fringe.push_front(fringe_exp);
                }
                let bind_tys = fringe_binders
                    .iter()
                    .map(|(_, _, ty)| ty)
                    .collect::<Vec<_>>();
                debug_print!(
                    context.debug.match_specialization, ("enum specialized" => datatype_name; dbg)
                );
                let (mut new_binders, inner_matrix) =
                    matrix.specialize_variant(context, &ctor, bind_tys);
                debug_print!(context.debug.match_specialization,
                             ("binders" => new_binders; dbg),
                             ("specialized" => inner_matrix));
                subject_binders.append(&mut new_binders);
                ice_assert!(
                    context.env,
                    arms.insert(ctor, (fringe_binders, inner_fringe, inner_matrix))
                        .is_none(),
                    ploc,
                    "Inserted duplicate ctor"
                );
            }

            let (mut new_binders, default_matrix) = matrix.specialize_default();
            subject_binders.append(&mut new_binders);

            MatchStep::VariantSwitch {
                subject,
                subject_binders,
                tyargs,
                arms,
                default: (fringe, default_matrix),
            }
        }
    }
}

pub fn order_fields_by_decl<T: std::fmt::Debug>(
    decl_fields: Option<UniqueMap<Field, usize>>,
    fields: Fields<T>,
) -> Vec<(usize, Field, T)> {
    let mut texp_fields: Vec<(usize, Field, T)> = if let Some(field_map) = decl_fields {
        fields
            .into_iter()
            .map(|(f, (_exp_idx, t))| (*field_map.get(&f).unwrap(), f, t))
            .collect()
    } else {
        // If no field map, compiler error in typing.
        fields
            .into_iter()
            .enumerate()
            .map(|(ndx, (f, (_exp_idx, t)))| (ndx, f, t))
            .collect()
    };
    texp_fields.sort_by(|(decl_idx1, _, _), (decl_idx2, _, _)| decl_idx1.cmp(decl_idx2));
    texp_fields
}

fn make_fringe_entries(binders: &[(Field, Var, Type)]) -> VecDeque<FringeEntry> {
    binders
        .iter()
        .map(|(_, var, ty)| FringeEntry {
            var: *var,
            ty: ty.clone(),
        })
        .collect::<VecDeque<_>>()
}

//------------------------------------------------
// Result Construction
//------------------------------------------------

struct ResolutionContext<'ctxt, 'call> {
    hlir_context: &'call mut Context<'ctxt>,
    output_type: &'call Type,
    arms: &'call Vec<T::Exp>,
    arms_loc: Loc,
    results: &'call mut BTreeMap<usize, WorkResult>,
}

impl<'ctxt, 'call> ResolutionContext<'ctxt, 'call> {
    fn arm(&self, index: usize) -> T::Exp {
        self.arms[index].clone()
    }

    fn arms_loc(&self) -> Loc {
        self.arms_loc
    }

    fn work_result(&mut self, work_id: usize) -> WorkResult {
        self.results.remove(&work_id).unwrap()
    }

    fn copy_work_result(&mut self, work_id: usize) -> WorkResult {
        self.results.get(&work_id).unwrap().clone()
    }

    fn output_type(&self) -> Type {
        self.output_type.clone()
    }
}

#[growing_stack]
fn resolve_result(
    context: &mut ResolutionContext,
    init_subject: &FringeEntry,
    result: WorkResult,
) -> T::Exp {
    match result {
        WorkResult::Leaf(leaf) => make_leaf(context, init_subject, leaf),
        WorkResult::Failure => T::exp(
            context.output_type(),
            sp(context.arms_loc, T::UnannotatedExp_::UnresolvedError),
        ),
        WorkResult::VariantSwitch {
            subject,
            subject_binders,
            tyargs,
            mut arms,
            default: default_ndx,
        } => {
            let (m, e) = subject
                .ty
                .value
                .unfold_to_type_name()
                .and_then(|sp!(_, name)| name.datatype_name())
                .unwrap();
            let bindings = subject_binders
                .into_iter()
                .map(|(mut_, binder)| (binder, (mut_, subject.clone())))
                .collect();

            let sorted_variants: Vec<VariantName> = context.hlir_context.enum_variants(&m, &e);
            let mut blocks = vec![];
            for v in sorted_variants {
                if let Some((unpack_fields, result_ndx)) = arms.remove(&v) {
                    let work_result = context.work_result(result_ndx);
                    let rest_result = resolve_result(context, init_subject, work_result);
                    let unpack_block = make_match_variant_unpack(
                        m,
                        e,
                        v,
                        tyargs.clone(),
                        unpack_fields,
                        subject.clone(),
                        rest_result,
                    );
                    blocks.push((v, unpack_block));
                } else {
                    let work_result = context.copy_work_result(default_ndx);
                    let rest_result = resolve_result(context, init_subject, work_result);
                    blocks.push((v, rest_result));
                }
            }
            let out_exp = T::UnannotatedExp_::VariantMatch(make_var_ref(subject), (m, e), blocks);
            let body_exp = T::exp(context.output_type(), sp(context.arms_loc(), out_exp));
            make_copy_bindings(bindings, body_exp)
        }
        WorkResult::StructUnpack {
            subject,
            subject_binders,
            tyargs,
            unpack,
        } => {
            let (m, s) = subject
                .ty
                .value
                .unfold_to_type_name()
                .and_then(|sp!(_, name)| name.datatype_name())
                .unwrap();
            let bindings = subject_binders
                .into_iter()
                .map(|(mut_, binder)| (binder, (mut_, subject.clone())))
                .collect();
            let unpack_exp = match unpack {
                StructUnpack::Default(result_ndx) => {
                    let work_result = context.work_result(result_ndx);
                    resolve_result(context, init_subject, work_result)
                }
                StructUnpack::Unpack(unpack_fields, result_ndx) => {
                    let work_result = context.work_result(result_ndx);
                    let rest_result = resolve_result(context, init_subject, work_result);
                    make_match_struct_unpack(
                        m,
                        s,
                        tyargs.clone(),
                        unpack_fields,
                        subject.clone(),
                        rest_result,
                    )
                }
            };
            make_copy_bindings(bindings, unpack_exp)
        }
        WorkResult::LiteralSwitch {
            subject,
            subject_binders,
            mut arms,
            default: _,
        } if matches!(
            subject.ty.value.unfold_to_builtin_type_name(),
            Some(sp!(_, BuiltinTypeName_::Bool))
        ) && arms.len() == 2 =>
        {
            let bindings = subject_binders
                .into_iter()
                .map(|(mut_, binder)| (binder, (mut_, subject.clone())))
                .collect();
            // If the literal switch for a boolean is saturated, no default case.
            let lit_subject = make_match_lit(subject.clone());
            let true_arm_ndx = arms
                .remove(&sp(Loc::invalid(), Value_::Bool(true)))
                .unwrap();
            let false_arm_ndx = arms
                .remove(&sp(Loc::invalid(), Value_::Bool(false)))
                .unwrap();

            let true_arm_result = context.work_result(true_arm_ndx);
            let false_arm_result = context.work_result(false_arm_ndx);

            let true_arm = resolve_result(context, init_subject, true_arm_result);
            let false_arm = resolve_result(context, init_subject, false_arm_result);
            let result_type = true_arm.ty.clone();

            make_copy_bindings(
                bindings,
                make_if_else(lit_subject, true_arm, false_arm, result_type),
            )
        }
        WorkResult::LiteralSwitch {
            subject,
            subject_binders,
            arms: map,
            default,
        } => {
            let bindings = subject_binders
                .into_iter()
                .map(|(mut_, binder)| (binder, (mut_, subject.clone())))
                .collect();
            let lit_subject = make_match_lit(subject.clone());

            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.sort_by(|(key1, _), (key2, _)| key1.cmp(key2));

            let else_work_result = context.work_result(default);
            let mut out_exp = resolve_result(context, init_subject, else_work_result);

            for (key, result_ndx) in entries.into_iter().rev() {
                let work_result = context.work_result(result_ndx);
                let match_arm = resolve_result(context, init_subject, work_result);
                let test_exp = make_lit_test(lit_subject.clone(), key);
                let result_ty = out_exp.ty.clone();
                out_exp = make_if_else(test_exp, match_arm, out_exp, result_ty);
            }
            make_copy_bindings(bindings, out_exp)
        }
    }
}

fn make_leaf(
    context: &mut ResolutionContext,
    subject: &FringeEntry,
    mut leaf: Vec<ArmResult>,
) -> T::Exp {
    assert!(!leaf.is_empty(), "ICE empty leaf in matching");

    if leaf.len() == 1 {
        let last = leaf.pop().unwrap();
        ice_assert!(
            context.hlir_context.env,
            last.guard.is_none(),
            last.guard.unwrap().exp.loc,
            "Must have a non-guarded leaf"
        );
        return make_copy_bindings(last.bindings, make_arm(context, subject.clone(), last.arm));
    }

    let last = leaf.pop().unwrap();
    ice_assert!(
        context.hlir_context.env,
        last.guard.is_none(),
        last.guard.unwrap().exp.loc,
        "Must have a non-guarded leaf"
    );
    let mut out_exp =
        make_copy_bindings(last.bindings, make_arm(context, subject.clone(), last.arm));
    let out_ty = out_exp.ty.clone();
    while let Some(arm) = leaf.pop() {
        ice_assert!(
            context.hlir_context.env,
            arm.guard.is_some(),
            arm.loc,
            "Expected a guard"
        );
        out_exp = make_guard_exp(context, subject, arm, out_exp, out_ty.clone());
    }
    out_exp
}

fn make_guard_exp(
    context: &mut ResolutionContext,
    subject: &FringeEntry,
    arm: ArmResult,
    cur_exp: T::Exp,
    result_ty: Type,
) -> T::Exp {
    let ArmResult {
        loc: _,
        bindings,
        guard,
        arm,
    } = arm;
    let guard_arm = make_arm(context, subject.clone(), arm);
    let body = make_if_else(*guard.unwrap(), guard_arm, cur_exp, result_ty);
    make_copy_bindings(bindings, body)
}

fn make_arm(context: &mut ResolutionContext, subject: FringeEntry, arm: Arm) -> T::Exp {
    let arm_exp = context.arm(arm.index);
    make_arm_unpack(
        context,
        subject,
        arm.orig_pattern,
        &arm.rhs_binders,
        arm_exp,
    )
}

fn make_arm_unpack(
    context: &mut ResolutionContext,
    subject: FringeEntry,
    pattern: MatchPattern,
    rhs_binders: &BTreeSet<Var>,
    next: T::Exp,
) -> T::Exp {
    let ploc = pattern.pat.loc;
    let mut seq = VecDeque::new();

    let mut queue: VecDeque<(FringeEntry, MatchPattern)> = VecDeque::from([(subject, pattern)]);

    // TODO(cgswords): we can coalese patterns a bit here, but don't for now.
    while let Some((entry, pat)) = queue.pop_front() {
        let ploc = pat.pat.loc;
        match pat.pat.value {
            TP::Variant(m, e, v, tys, fs) => {
                let Some((queue_entries, unpack)) =
                    arm_variant_unpack(context, None, ploc, m, e, tys, v, fs, entry)
                else {
                    context.hlir_context.env.add_diag(ice!((
                        ploc,
                        "Did not build an arm unpack for a value variant"
                    )));
                    continue;
                };
                for entry in queue_entries.into_iter().rev() {
                    queue.push_front(entry);
                }
                seq.push_back(unpack);
            }
            TP::BorrowVariant(mut_, m, e, v, tys, fs) => {
                let Some((queue_entries, unpack)) =
                    arm_variant_unpack(context, Some(mut_), ploc, m, e, tys, v, fs, entry)
                else {
                    continue;
                };
                for entry in queue_entries.into_iter().rev() {
                    queue.push_front(entry);
                }
                seq.push_back(unpack);
            }
            TP::Struct(m, s, tys, fs) => {
                let Some((queue_entries, unpack)) =
                    arm_struct_unpack(context, None, ploc, m, s, tys, fs, entry)
                else {
                    context.hlir_context.env.add_diag(ice!((
                        ploc,
                        "Did not build an arm unpack for a value struct"
                    )));
                    continue;
                };
                for entry in queue_entries.into_iter().rev() {
                    queue.push_front(entry);
                }
                seq.push_back(unpack);
            }
            TP::BorrowStruct(mut_, m, s, tys, fs) => {
                let Some((queue_entries, unpack)) =
                    arm_struct_unpack(context, Some(mut_), ploc, m, s, tys, fs, entry)
                else {
                    continue;
                };
                for entry in queue_entries.into_iter().rev() {
                    queue.push_front(entry);
                }
                seq.push_back(unpack);
            }
            TP::Literal(_) => (),
            TP::Binder(mut_, x) if rhs_binders.contains(&x) => {
                seq.push_back(make_move_binding(x, mut_, entry.ty.clone(), entry))
            }
            TP::Binder(_, _) => (),
            TP::Wildcard => (),
            TP::At(x, inner) => {
                // See comment in typing/translate.rs at pattern typing for more information.
                let x_in_rhs_binders = rhs_binders.contains(&x);
                let inner_has_rhs_binders = match_pattern_has_binders(&inner, rhs_binders);
                match (x_in_rhs_binders, inner_has_rhs_binders) {
                    // make a copy of the value (or ref) and do both sides
                    (true, true) => {
                        let bind_entry = entry.clone();
                        seq.push_back(make_copy_binding(
                            x,
                            Mutability::Imm,
                            bind_entry.ty.clone(),
                            bind_entry,
                        ));
                        queue.push_front((entry, *inner));
                    }
                    // no unpack needed, just move the value to the x
                    (true, false) => seq.push_back(make_move_binding(
                        x,
                        Mutability::Imm,
                        entry.ty.clone(),
                        entry,
                    )),
                    // we need to unpack either way, handling wildcards and the like
                    (false, _) => queue.push_front((entry, *inner)),
                }
            }
            TP::ErrorPat => (),
            TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
        }
    }

    let nloc = next.exp.loc;
    let out_type = next.ty.clone();
    seq.push_back(sp(nloc, T::SequenceItem_::Seq(Box::new(next))));

    let body = T::UnannotatedExp_::Block((UseFuns::new(0), seq));
    T::exp(out_type, sp(ploc, body))
}

fn match_pattern_has_binders(pat: &T::MatchPattern, rhs_binders: &BTreeSet<Var>) -> bool {
    match &pat.pat.value {
        TP::Binder(_, x) => rhs_binders.contains(x),
        TP::At(x, inner) => {
            rhs_binders.contains(x) || match_pattern_has_binders(inner, rhs_binders)
        }
        TP::Variant(_, _, _, _, fields) | TP::BorrowVariant(_, _, _, _, _, fields) => fields
            .iter()
            .any(|(_, _, (_, (_, pat)))| match_pattern_has_binders(pat, rhs_binders)),
        TP::Struct(_, _, _, fields) | TP::BorrowStruct(_, _, _, _, fields) => fields
            .iter()
            .any(|(_, _, (_, (_, pat)))| match_pattern_has_binders(pat, rhs_binders)),
        TP::Literal(_) => false,
        TP::Wildcard => false,
        TP::ErrorPat => false,
        TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
    }
}

fn arm_variant_unpack(
    context: &mut ResolutionContext,
    mut_ref: Option<bool>,
    pat_loc: Loc,
    mident: ModuleIdent,
    enum_: DatatypeName,
    tyargs: Vec<Type>,
    variant: VariantName,
    fields: Fields<(Type, MatchPattern)>,
    rhs: FringeEntry,
) -> Option<(Vec<(FringeEntry, MatchPattern)>, T::SequenceItem)> {
    let all_wild = fields
        .iter()
        .all(|(_, _, (_, (_, pat)))| matches!(pat.pat.value, TP::Wildcard))
        || fields.is_empty();
    // If we are  matching a  ref with no fields under it, we aren't going to drop so
    // we just continue on.
    if all_wild && mut_ref.is_some() {
        return None;
    }

    let (queue_entries, fields) =
        make_arm_variant_unpack_fields(context, mut_ref, pat_loc, mident, enum_, variant, fields);
    let unpack = make_arm_variant_unpack_stmt(mut_ref, mident, enum_, variant, tyargs, fields, rhs);
    Some((queue_entries, unpack))
}

fn arm_struct_unpack(
    context: &mut ResolutionContext,
    mut_ref: Option<bool>,
    pat_loc: Loc,
    mident: ModuleIdent,
    struct_: DatatypeName,
    tyargs: Vec<Type>,
    fields: Fields<(Type, MatchPattern)>,
    rhs: FringeEntry,
) -> Option<(Vec<(FringeEntry, MatchPattern)>, T::SequenceItem)> {
    let all_wild = fields
        .iter()
        .all(|(_, _, (_, (_, pat)))| matches!(pat.pat.value, TP::Wildcard))
        || fields.is_empty();
    // If we are  matching a  ref with no fields under it, we aren't going to drop so
    // we just continue on.
    if all_wild && mut_ref.is_some() {
        return None;
    }

    let (queue_entries, fields) =
        make_arm_struct_unpack_fields(context, mut_ref, pat_loc, mident, struct_, fields);
    let unpack = make_arm_struct_unpack_stmt(mut_ref, mident, struct_, tyargs, fields, rhs);
    Some((queue_entries, unpack))
}

//------------------------------------------------
// Unpack Field Builders
//------------------------------------------------

fn make_arm_variant_unpack_fields(
    context: &mut ResolutionContext,
    mut_ref: Option<bool>,
    pat_loc: Loc,
    mident: ModuleIdent,
    enum_: DatatypeName,
    variant: VariantName,
    fields: Fields<(Type, MatchPattern)>,
) -> (Vec<(FringeEntry, MatchPattern)>, Vec<(Field, Var, Type)>) {
    let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));

    let field_tys = {
        let field_tys = fields.map(|_key, (ndx, (ty, _))| (ndx, ty));
        if let Some(mut_) = mut_ref {
            field_tys.map(|_field, (ndx, sp!(loc, ty))| {
                (
                    ndx,
                    sp(loc, N::Type_::Ref(mut_, Box::new(sp(loc, ty.base_type_())))),
                )
            })
        } else {
            field_tys
        }
    };
    let fringe_binders = context.hlir_context.make_unpack_binders(pat_loc, field_tys);
    let fringe_exps = make_fringe_entries(&fringe_binders);

    let decl_fields = context
        .hlir_context
        .enum_variant_fields(&mident, &enum_, &variant);
    let ordered_pats = order_fields_by_decl(decl_fields, field_pats);

    let mut unpack_fields: Vec<(Field, Var, Type)> = vec![];
    assert!(fringe_exps.len() == ordered_pats.len());
    for (fringe_exp, (_, field, _)) in fringe_exps.iter().zip(ordered_pats.iter()) {
        unpack_fields.push((*field, fringe_exp.var, fringe_exp.ty.clone()));
    }
    let queue_entries = fringe_exps
        .into_iter()
        .zip(
            ordered_pats
                .into_iter()
                .map(|(_, _, ordered_pat)| ordered_pat),
        )
        .collect::<Vec<_>>();

    (queue_entries, unpack_fields)
}

fn make_arm_struct_unpack_fields(
    context: &mut ResolutionContext,
    mut_ref: Option<bool>,
    pat_loc: Loc,
    mident: ModuleIdent,
    struct_: DatatypeName,
    fields: Fields<(Type, MatchPattern)>,
) -> (Vec<(FringeEntry, MatchPattern)>, Vec<(Field, Var, Type)>) {
    let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));

    let field_tys = {
        let field_tys = fields.map(|_key, (ndx, (ty, _))| (ndx, ty));
        if let Some(mut_) = mut_ref {
            field_tys.map(|_field, (ndx, sp!(loc, ty))| {
                (
                    ndx,
                    sp(loc, N::Type_::Ref(mut_, Box::new(sp(loc, ty.base_type_())))),
                )
            })
        } else {
            field_tys
        }
    };
    let fringe_binders = context.hlir_context.make_unpack_binders(pat_loc, field_tys);
    let fringe_exps = make_fringe_entries(&fringe_binders);

    let decl_fields = context.hlir_context.struct_fields(&mident, &struct_);
    let ordered_pats = order_fields_by_decl(decl_fields, field_pats);

    let mut unpack_fields: Vec<(Field, Var, Type)> = vec![];
    assert!(fringe_exps.len() == ordered_pats.len());
    for (fringe_exp, (_, field, _)) in fringe_exps.iter().zip(ordered_pats.iter()) {
        unpack_fields.push((*field, fringe_exp.var, fringe_exp.ty.clone()));
    }
    let queue_entries = fringe_exps
        .into_iter()
        .zip(
            ordered_pats
                .into_iter()
                .map(|(_, _, ordered_pat)| ordered_pat),
        )
        .collect::<Vec<_>>();

    (queue_entries, unpack_fields)
}

//------------------------------------------------
// Expression Creation Helpers
//------------------------------------------------

fn make_var_ref(subject: FringeEntry) -> Box<T::Exp> {
    let FringeEntry { var, ty } = subject;
    match ty {
        sp!(_, N::Type_::Ref(false, _)) => {
            let loc = var.loc;
            Box::new(make_copy_exp(ty, loc, var))
        }
        sp!(_, N::Type_::Ref(true, inner)) => {
            // NB(cswords): we freeze the mut ref at the non-mut ref type.
            let loc = var.loc;
            let ref_ty = sp(loc, N::Type_::Ref(true, inner.clone()));
            let freeze_arg = make_copy_exp(ref_ty, loc, var);
            let freeze_ty = sp(loc, N::Type_::Ref(false, inner));
            Box::new(make_freeze_exp(freeze_ty, loc, freeze_arg))
        }
        ty => {
            // NB(cswords): we borrow the local
            let loc = var.loc;
            let ref_ty = sp(loc, N::Type_::Ref(false, Box::new(ty)));
            let borrow_exp = T::UnannotatedExp_::BorrowLocal(false, var);
            Box::new(T::exp(ref_ty, sp(loc, borrow_exp)))
        }
    }
}

// Performs an unpack for the purpose of matching, where we are matching against an imm. ref.
fn make_match_variant_unpack(
    mident: ModuleIdent,
    enum_: DatatypeName,
    variant: VariantName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
    next: T::Exp,
) -> T::Exp {
    assert!(matches!(rhs.ty.value, N::Type_::Ref(false, _)));
    let mut seq = VecDeque::new();

    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        assert!(ty.value.is_ref().is_some());
        let var_lvalue = make_lvalue(var, Mutability::Imm, ty.clone());
        let lhs_ty = sp(ty.loc, ty.value.base_type_());
        lvalue_fields
            .add(field_name, (ndx, (lhs_ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue = sp(
        rhs_loc,
        T::LValue_::BorrowUnpackVariant(false, mident, enum_, variant, tyargs, lvalue_fields),
    );

    let FringeEntry { var, ty } = rhs;
    let rhs = Box::new(make_copy_exp(ty.clone(), var.loc, var));
    let binder = T::SequenceItem_::Bind(sp(rhs_loc, vec![unpack_lvalue]), vec![Some(ty)], rhs);
    seq.push_back(sp(rhs_loc, binder));

    let result_type = next.ty.clone();
    let eloc = next.exp.loc;
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));

    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type, exp_value)
}

// Performs a struct unpack for the purpose of matching, where we are matching against an imm. ref.
// Note that unpacking refs is a lie; this is
fn make_match_struct_unpack(
    mident: ModuleIdent,
    struct_: DatatypeName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
    next: T::Exp,
) -> T::Exp {
    assert!(matches!(rhs.ty.value, N::Type_::Ref(false, _)));
    let mut seq = VecDeque::new();

    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        assert!(ty.value.is_ref().is_some());
        let var_lvalue = make_lvalue(var, Mutability::Imm, ty.clone());
        let lhs_ty = sp(ty.loc, ty.value.base_type_());
        lvalue_fields
            .add(field_name, (ndx, (lhs_ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue = sp(
        rhs_loc,
        T::LValue_::BorrowUnpack(false, mident, struct_, tyargs, lvalue_fields),
    );

    let FringeEntry { var, ty } = rhs;
    let rhs = Box::new(make_copy_exp(ty.clone(), var.loc, var));
    let binder = T::SequenceItem_::Bind(sp(rhs_loc, vec![unpack_lvalue]), vec![Some(ty)], rhs);
    seq.push_back(sp(rhs_loc, binder));

    let result_type = next.ty.clone();
    let eloc = next.exp.loc;
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));

    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type, exp_value)
}

fn make_arm_variant_unpack_stmt(
    mut_ref: Option<bool>,
    mident: ModuleIdent,
    enum_: DatatypeName,
    variant: VariantName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
) -> T::SequenceItem {
    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        let var_lvalue = make_lvalue(var, Mutability::Imm, ty.clone());
        let lhs_ty = sp(ty.loc, ty.value.base_type_());
        lvalue_fields
            .add(field_name, (ndx, (lhs_ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue_ = if let Some(mut_) = mut_ref {
        T::LValue_::BorrowUnpackVariant(mut_, mident, enum_, variant, tyargs, lvalue_fields)
    } else {
        T::LValue_::UnpackVariant(mident, enum_, variant, tyargs, lvalue_fields)
    };
    let rhs_ty = rhs.ty.clone();
    let rhs: Box<T::Exp> = Box::new(rhs.into_move_exp());
    let binder = T::SequenceItem_::Bind(
        sp(rhs_loc, vec![sp(rhs_loc, unpack_lvalue_)]),
        vec![Some(rhs_ty)],
        rhs,
    );
    sp(rhs_loc, binder)
}

fn make_arm_struct_unpack_stmt(
    mut_ref: Option<bool>,
    mident: ModuleIdent,
    struct_: DatatypeName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
) -> T::SequenceItem {
    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        let var_lvalue = make_lvalue(var, Mutability::Imm, ty.clone());
        let lhs_ty = sp(ty.loc, ty.value.base_type_());
        lvalue_fields
            .add(field_name, (ndx, (lhs_ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue_ = if let Some(mut_) = mut_ref {
        T::LValue_::BorrowUnpack(mut_, mident, struct_, tyargs, lvalue_fields)
    } else {
        T::LValue_::Unpack(mident, struct_, tyargs, lvalue_fields)
    };
    let rhs_ty = rhs.ty.clone();
    let rhs: Box<T::Exp> = Box::new(rhs.into_move_exp());
    let binder = T::SequenceItem_::Bind(
        sp(rhs_loc, vec![sp(rhs_loc, unpack_lvalue_)]),
        vec![Some(rhs_ty)],
        rhs,
    );
    sp(rhs_loc, binder)
}

fn make_match_lit(subject: FringeEntry) -> T::Exp {
    let FringeEntry { var, ty } = subject;
    match ty {
        sp!(ty_loc, N::Type_::Ref(false, inner)) => {
            let loc = var.loc;
            let copy_exp = make_copy_exp(sp(ty_loc, N::Type_::Ref(false, inner.clone())), loc, var);
            make_deref_exp(*inner, loc, copy_exp)
        }
        sp!(_, N::Type_::Ref(true, inner)) => {
            let loc = var.loc;

            // NB(cswords): we now freeze the mut ref at the non-mut ref type.
            let ref_ty = sp(loc, N::Type_::Ref(true, inner.clone()));
            let freeze_arg = make_copy_exp(ref_ty, loc, var);
            let freeze_ty = sp(loc, N::Type_::Ref(false, inner.clone()));
            let frozen_exp = make_freeze_exp(freeze_ty, loc, freeze_arg);
            make_deref_exp(*inner, loc, frozen_exp)
        }
        _ty => unreachable!(),
    }
}

fn make_copy_bindings(bindings: PatBindings, next: T::Exp) -> T::Exp {
    make_bindings(bindings, next, true)
}

fn make_bindings(bindings: PatBindings, next: T::Exp, as_copy: bool) -> T::Exp {
    let eloc = next.exp.loc;
    let mut seq = VecDeque::new();
    for (lhs, (mut_, rhs)) in bindings {
        let binding = if as_copy {
            make_copy_binding(lhs, mut_, rhs.ty.clone(), rhs)
        } else {
            make_move_binding(lhs, mut_, rhs.ty.clone(), rhs)
        };
        seq.push_back(binding);
    }
    let result_type = next.ty.clone();
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type, exp_value)
}

fn make_lvalue(lhs: Var, mut_: Mutability, ty: Type) -> T::LValue {
    let lhs_loc = lhs.loc;
    let lhs_var = T::LValue_::Var {
        var: lhs,
        ty: Box::new(ty.clone()),
        mut_: Some(mut_),
        unused_binding: false,
    };
    sp(lhs_loc, lhs_var)
}

fn make_move_binding(lhs: Var, mut_: Mutability, ty: Type, rhs: FringeEntry) -> T::SequenceItem {
    let lhs_loc = lhs.loc;
    let lhs_lvalue = make_lvalue(lhs, mut_, ty.clone());
    let binder = T::SequenceItem_::Bind(
        sp(lhs_loc, vec![lhs_lvalue]),
        vec![Some(ty)],
        Box::new(rhs.into_move_exp()),
    );
    sp(lhs_loc, binder)
}

fn make_copy_binding(lhs: Var, mut_: Mutability, ty: Type, rhs: FringeEntry) -> T::SequenceItem {
    let lhs_loc = lhs.loc;
    let lhs_lvalue = make_lvalue(lhs, mut_, ty.clone());
    let binder = T::SequenceItem_::Bind(
        sp(lhs_loc, vec![lhs_lvalue]),
        vec![Some(ty.clone())],
        Box::new(make_copy_exp(ty, rhs.var.loc, rhs.var)),
    );
    sp(lhs_loc, binder)
}

fn make_lit_test(lit_exp: T::Exp, value: Value) -> T::Exp {
    let loc = value.loc;
    let value_exp = T::exp(
        lit_exp.ty.clone(),
        sp(loc, T::UnannotatedExp_::Value(value)),
    );
    make_eq_test(loc, lit_exp, value_exp)
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

fn make_eq_test(loc: Loc, lhs: T::Exp, rhs: T::Exp) -> T::Exp {
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

fn make_if_else(test: T::Exp, conseq: T::Exp, alt: T::Exp, result_ty: Type) -> T::Exp {
    // FIXME: this span is woefully wrong
    let loc = test.exp.loc;
    T::exp(
        result_ty,
        sp(
            loc,
            T::UnannotatedExp_::IfElse(Box::new(test), Box::new(conseq), Box::new(alt)),
        ),
    )
}

fn make_copy_exp(ty: Type, loc: Loc, var: Var) -> T::Exp {
    let exp_ = T::UnannotatedExp_::Copy {
        var,
        from_user: false,
    };
    T::exp(ty, sp(loc, exp_))
}

fn make_freeze_exp(ty: Type, loc: Loc, arg: T::Exp) -> T::Exp {
    let freeze_fn = Box::new(sp(loc, T::BuiltinFunction_::Freeze(ty.clone())));
    let freeze_exp = T::UnannotatedExp_::Builtin(freeze_fn, Box::new(arg));
    T::exp(ty, sp(loc, freeze_exp))
}

fn make_deref_exp(ty: Type, loc: Loc, arg: T::Exp) -> T::Exp {
    let deref_exp = T::UnannotatedExp_::Dereference(Box::new(arg));
    T::exp(ty, sp(loc, deref_exp))
}

//------------------------------------------------
// Counterexample Generation
//------------------------------------------------

#[derive(Clone, Debug)]
enum CounterExample {
    Wildcard,
    Literal(String),
    Struct(
        DatatypeName,
        /* is_positional */ bool,
        Vec<(String, CounterExample)>,
    ),
    Variant(
        DatatypeName,
        VariantName,
        /* is_positional */ bool,
        Vec<(String, CounterExample)>,
    ),
    Note(String, Box<CounterExample>),
}

impl CounterExample {
    fn into_notes(self) -> VecDeque<String> {
        match self {
            CounterExample::Wildcard => VecDeque::new(),
            CounterExample::Literal(_) => VecDeque::new(),
            CounterExample::Note(s, next) => {
                let mut notes = next.into_notes();
                notes.push_front(s.clone());
                notes
            }
            CounterExample::Variant(_, _, _, inner) => inner
                .into_iter()
                .flat_map(|(_, ce)| ce.into_notes())
                .collect::<VecDeque<_>>(),
            CounterExample::Struct(_, _, inner) => inner
                .into_iter()
                .flat_map(|(_, ce)| ce.into_notes())
                .collect::<VecDeque<_>>(),
        }
    }
}

impl Display for CounterExample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CounterExample::Wildcard => write!(f, "_"),
            CounterExample::Literal(s) => write!(f, "{}", s),
            CounterExample::Note(_, inner) => inner.fmt(f),
            CounterExample::Struct(s, is_positional, args) => {
                write!(f, "{}", s)?;
                if *is_positional {
                    write!(f, "(")?;
                    write!(
                        f,
                        "{}",
                        args.iter()
                            .map(|(_name, arg)| { format!("{}", arg) })
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                    write!(f, ")")
                } else {
                    write!(f, " {{ ")?;
                    write!(
                        f,
                        "{}",
                        args.iter()
                            .map(|(name, arg)| { format!("{}: {}", name, arg) })
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                    write!(f, " }}")
                }
            }
            CounterExample::Variant(e, v, is_positional, args) => {
                write!(f, "{}::{}", e, v)?;
                if !args.is_empty() {
                    if *is_positional {
                        write!(f, "(")?;
                        write!(
                            f,
                            "{}",
                            args.iter()
                                .map(|(_name, arg)| { format!("{}", arg) })
                                .collect::<Vec<_>>()
                                .join(", ")
                        )?;
                        write!(f, ")")
                    } else {
                        write!(f, " {{ ")?;
                        write!(
                            f,
                            "{}",
                            args.iter()
                                .map(|(name, arg)| { format!("{}: {}", name, arg) })
                                .collect::<Vec<_>>()
                                .join(", ")
                        )?;
                        write!(f, " }}")
                    }
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Returns true if it found a counter-example. Assumes all arms with guards have been removed from
/// the provided matrix.
fn find_counterexample(
    context: &mut Context,
    loc: Loc,
    matrix: PatternMatrix,
    has_guards: bool,
) -> bool {
    // If the matrix is only errors (or empty), it was all error or something else (like typing)
    // went wrong; no counterexample is required.
    if !matrix.is_empty() && !matrix.patterns_empty() && matrix.all_errors() {
        debug_print!(context.debug.match_counterexample, (msg "errors"), ("matrix" => matrix; dbg));
        assert!(context.env.has_errors());
        return true;
    }
    find_counterexample_impl(context, loc, matrix, has_guards)
}

/// Returns true if it found a counter-example.
fn find_counterexample_impl(
    context: &mut Context,
    loc: Loc,
    matrix: PatternMatrix,
    has_guards: bool,
) -> bool {
    fn make_wildcards(n: usize) -> Vec<CounterExample> {
        std::iter::repeat(CounterExample::Wildcard)
            .take(n)
            .collect()
    }

    #[growing_stack]
    fn counterexample_bool(
        context: &mut Context,
        matrix: PatternMatrix,
        arity: u32,
        ndx: &mut u32,
    ) -> Option<Vec<CounterExample>> {
        let literals = matrix.first_lits();
        assert!(literals.len() <= 2, "ICE match exhaustiveness failure");
        if literals.len() == 2 {
            // Saturated
            for lit in literals {
                if let Some(counterexample) =
                    counterexample_rec(context, matrix.specialize_literal(&lit).1, arity - 1, ndx)
                {
                    let lit_str = format!("{}", lit);
                    let result = [CounterExample::Literal(lit_str)]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                    return Some(result);
                }
            }
            None
        } else {
            let (_, default) = matrix.specialize_default();
            if let Some(counterexample) = counterexample_rec(context, default, arity - 1, ndx) {
                if literals.is_empty() {
                    let result = [CounterExample::Wildcard]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                    Some(result)
                } else {
                    let mut unused = BTreeSet::from([Value_::Bool(true), Value_::Bool(false)]);
                    for lit in literals {
                        unused.remove(&lit.value);
                    }
                    let result = [CounterExample::Literal(format!(
                        "{}",
                        unused.first().unwrap()
                    ))]
                    .into_iter()
                    .chain(counterexample)
                    .collect();
                    Some(result)
                }
            } else {
                None
            }
        }
    }

    #[growing_stack]
    fn counterexample_builtin(
        context: &mut Context,
        matrix: PatternMatrix,
        arity: u32,
        ndx: &mut u32,
    ) -> Option<Vec<CounterExample>> {
        // For all other non-literals, we don't consider a case where the constructors are
        // saturated.
        let literals = matrix.first_lits();
        let (_, default) = matrix.specialize_default();
        if let Some(counterexample) = counterexample_rec(context, default, arity - 1, ndx) {
            if literals.is_empty() {
                let result = [CounterExample::Wildcard]
                    .into_iter()
                    .chain(counterexample)
                    .collect();
                Some(result)
            } else {
                let n_id = format!("_{}", ndx);
                *ndx += 1;
                let lit_str = {
                    let lit_len = literals.len() as u64;
                    let fmt_lits = if lit_len > 4 {
                        let mut result = literals
                            .into_iter()
                            .take(3)
                            .map(|lit| lit.to_string())
                            .collect::<Vec<_>>();
                        result.push(format!("{} other values", lit_len - 3));
                        result
                    } else {
                        literals
                            .into_iter()
                            .map(|lit| lit.to_string())
                            .collect::<Vec<_>>()
                    };
                    format_oxford_list!("or", "{}", fmt_lits)
                };
                let lit_msg = format!("When '{}' is not {}", n_id, lit_str);
                let lit_ce = CounterExample::Note(lit_msg, Box::new(CounterExample::Literal(n_id)));
                let result = [lit_ce].into_iter().chain(counterexample).collect();
                Some(result)
            }
        } else {
            None
        }
    }

    #[growing_stack]
    fn counterexample_datatype(
        context: &mut Context,
        matrix: PatternMatrix,
        arity: u32,
        ndx: &mut u32,
        mident: ModuleIdent,
        datatype_name: DatatypeName,
    ) -> Option<Vec<CounterExample>> {
        debug_print!(
            context.debug.match_counterexample,
            (lines "matrix types" => &matrix.tys; verbose)
        );
        if context.is_struct(&mident, &datatype_name) {
            // For a struct, we only care if we destructure it. If we do, we want to specialize and
            // recur. If we don't, we check it as a default specialization.
            if let Some((ploc, arg_types)) = matrix.first_struct_ctors() {
                let ctor_arity = arg_types.len() as u32;
                let fringe_binders = context.make_imm_ref_match_binders(ploc, arg_types);
                let is_positional = context.struct_is_positional(&mident, &datatype_name);
                let names = fringe_binders
                    .iter()
                    .map(|(name, _, _)| name.to_string())
                    .collect::<Vec<_>>();
                let bind_tys = fringe_binders
                    .iter()
                    .map(|(_, _, ty)| ty)
                    .collect::<Vec<_>>();
                let (_, inner_matrix) = matrix.specialize_struct(context, bind_tys);
                if let Some(mut counterexample) =
                    counterexample_rec(context, inner_matrix, ctor_arity + arity - 1, ndx)
                {
                    let ctor_args = counterexample
                        .drain(0..(ctor_arity as usize))
                        .collect::<Vec<_>>();
                    assert!(ctor_args.len() == names.len());
                    let output = [CounterExample::Struct(
                        datatype_name,
                        is_positional,
                        names.into_iter().zip(ctor_args).collect::<Vec<_>>(),
                    )]
                    .into_iter()
                    .chain(counterexample)
                    .collect();
                    Some(output)
                } else {
                    // If we didn't find a counterexample in the destructuring cases, we're done.
                    None
                }
            } else {
                let (_, default) = matrix.specialize_default();
                // `_` is a reasonable counterexample since we never unpacked this struct
                if let Some(counterexample) = counterexample_rec(context, default, arity - 1, ndx) {
                    // If we didn't match any head constructor, `_` is a reasonable
                    // counter-example entry.
                    let mut result = vec![CounterExample::Wildcard];
                    result.extend(&mut counterexample.into_iter());
                    Some(result)
                } else {
                    None
                }
            }
        } else {
            let mut unmatched_variants = context
                .enum_variants(&mident, &datatype_name)
                .into_iter()
                .collect::<BTreeSet<_>>();

            let ctors = matrix.first_variant_ctors();
            for ctor in ctors.keys() {
                unmatched_variants.remove(ctor);
            }
            if unmatched_variants.is_empty() {
                for (ctor, (ploc, arg_types)) in ctors {
                    let ctor_arity = arg_types.len() as u32;
                    let fringe_binders = context.make_imm_ref_match_binders(ploc, arg_types);
                    let is_positional =
                        context.enum_variant_is_positional(&mident, &datatype_name, &ctor);
                    let names = fringe_binders
                        .iter()
                        .map(|(name, _, _)| name.to_string())
                        .collect::<Vec<_>>();
                    let bind_tys = fringe_binders
                        .iter()
                        .map(|(_, _, ty)| ty)
                        .collect::<Vec<_>>();
                    let (_, inner_matrix) = matrix.specialize_variant(context, &ctor, bind_tys);
                    if let Some(mut counterexample) =
                        counterexample_rec(context, inner_matrix, ctor_arity + arity - 1, ndx)
                    {
                        let ctor_args = counterexample
                            .drain(0..(ctor_arity as usize))
                            .collect::<Vec<_>>();
                        assert!(ctor_args.len() == names.len());
                        let output = [CounterExample::Variant(
                            datatype_name,
                            ctor,
                            is_positional,
                            names
                                .into_iter()
                                .zip(ctor_args.into_iter())
                                .collect::<Vec<_>>(),
                        )]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                        return Some(output);
                    }
                }
                None
            } else {
                let (_, default) = matrix.specialize_default();
                if let Some(counterexample) = counterexample_rec(context, default, arity - 1, ndx) {
                    if ctors.is_empty() {
                        // If we didn't match any head constructor, `_` is a reasonable
                        // counter-example entry.
                        let mut result = vec![CounterExample::Wildcard];
                        result.extend(&mut counterexample.into_iter());
                        Some(result)
                    } else {
                        let variant_name = unmatched_variants.first().unwrap();
                        let is_positional = context.enum_variant_is_positional(
                            &mident,
                            &datatype_name,
                            variant_name,
                        );
                        let ctor_args = context
                            .enum_variant_fields(&mident, &datatype_name, variant_name)
                            .unwrap();
                        let names = ctor_args
                            .iter()
                            .map(|(_, field, _)| field.to_string())
                            .collect::<Vec<_>>();
                        let ctor_arity = names.len();
                        let result = [CounterExample::Variant(
                            datatype_name,
                            *variant_name,
                            is_positional,
                            names.into_iter().zip(make_wildcards(ctor_arity)).collect(),
                        )]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                        Some(result)
                    }
                } else {
                    // If we are missing a variant but everything else is fine, we're done.
                    None
                }
            }
        }
    }

    // \mathcal{I} from Maranget. Warning for pattern matching. 1992.
    #[growing_stack]
    fn counterexample_rec(
        context: &mut Context,
        matrix: PatternMatrix,
        arity: u32,
        ndx: &mut u32,
    ) -> Option<Vec<CounterExample>> {
        debug_print!(context.debug.match_counterexample, ("checking matrix" => matrix; verbose));
        let result = if matrix.patterns_empty() {
            None
        } else if let Some(ty) = matrix.tys.first() {
            if let Some(sp!(_, BuiltinTypeName_::Bool)) = ty.value.unfold_to_builtin_type_name() {
                counterexample_bool(context, matrix, arity, ndx)
            } else if let Some(_builtin) = ty.value.unfold_to_builtin_type_name() {
                counterexample_builtin(context, matrix, arity, ndx)
            } else if let Some((mident, datatype_name)) = ty
                .value
                .unfold_to_type_name()
                .and_then(|sp!(_, name)| name.datatype_name())
            {
                counterexample_datatype(context, matrix, arity, ndx, mident, datatype_name)
            } else {
                // This can only be a binding or wildcard, so we act accordingly.
                let (_, default) = matrix.specialize_default();
                if let Some(counterexample) = counterexample_rec(context, default, arity - 1, ndx) {
                    let result = [CounterExample::Wildcard]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                    Some(result)
                } else {
                    None
                }
            }
        } else {
            assert!(matrix.is_empty());
            Some(make_wildcards(arity as usize))
        };
        debug_print!(context.debug.match_counterexample, (opt "result" => &result; sdbg));
        result
    }

    let mut ndx = 0;

    if let Some(mut counterexample) = counterexample_rec(context, matrix, 1, &mut ndx) {
        debug_print!(
            context.debug.match_counterexample,
            ("counterexamples #" => counterexample.len(); fmt),
            (lines "counterexamples" => &counterexample; fmt)
        );
        assert!(counterexample.len() == 1);
        let counterexample = counterexample.remove(0);
        let msg = format!("Pattern '{}' not covered", counterexample);
        let mut diag = diag!(TypeSafety::IncompletePattern, (loc, msg));
        for note in counterexample.into_notes() {
            diag.add_note(note);
        }
        if has_guards {
            diag.add_note("Match arms with guards are not considered for coverage.");
        }
        context.env.add_diag(diag);
        true
    } else {
        false
    }
}

//------------------------------------------------
// IDE Arm Suggestion Generation
//------------------------------------------------

/// Produces IDE information if the top-level match is incomplete. Assumes all arms with guards
/// have been removed from the provided matrix.
fn ide_report_missing_arms(context: &mut Context, loc: Loc, matrix: &PatternMatrix) {
    use PatternSuggestion as PS;
    // This function looks at the very top-level of the match. For any arm missing, it suggests the
    // IDE add an arm to address that missing one.

    fn report_bool(context: &mut Context, loc: Loc, matrix: &PatternMatrix) {
        let literals = matrix.first_lits();
        assert!(literals.len() <= 2, "ICE match exhaustiveness failure");
        // Figure out which are missing
        let mut unused = BTreeSet::from([Value_::Bool(true), Value_::Bool(false)]);
        for lit in literals {
            unused.remove(&lit.value);
        }
        if !unused.is_empty() {
            let arms = unused.into_iter().map(PS::Value).collect::<Vec<_>>();
            let info = MissingMatchArmsInfo { arms };
            context
                .env
                .add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        }
    }

    fn report_builtin(context: &mut Context, loc: Loc, matrix: &PatternMatrix) {
        // For all other non-literals, we don't consider a case where the constructors are
        // saturated. If it doesn't have a wildcard, we suggest adding a wildcard.
        if !matrix.has_default_arm() {
            let info = MissingMatchArmsInfo {
                arms: vec![PS::Wildcard],
            };
            context
                .env
                .add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        }
    }

    fn report_datatype(
        context: &mut Context,
        loc: Loc,
        matrix: &PatternMatrix,
        mident: ModuleIdent,
        name: DatatypeName,
    ) {
        if context.is_struct(&mident, &name) {
            if !matrix.is_empty() {
                // If the matrix isn't empty, we _must_ have matched the struct with at least one
                // non-guard arm (either wildcards or the struct itself), so we're fine.
                return;
            }
            // If the matrix _is_ empty, we suggest adding an unpack.
            let is_positional = context.struct_is_positional(&mident, &name);
            let Some(fields) = context.struct_fields(&mident, &name) else {
                context.env.add_diag(ice!((
                    loc,
                    "Tried to look up fields for this struct and found none"
                )));
                return;
            };
            // NB: We might not have a concrete type for the type parameters to the datatype (due
            // to type errors or otherwise), so we use stand-in types. Since this is IDE
            // information that should be inserted and then re-compiled, this should work for our
            // purposes.

            let suggestion = if is_positional {
                PS::UnpackPositionalStruct {
                    module: mident,
                    name,
                    field_count: fields.len(),
                }
            } else {
                PS::UnpackNamedStruct {
                    module: mident,
                    name,
                    fields: fields.into_iter().map(|(field, _)| field.value()).collect(),
                }
            };
            let info = MissingMatchArmsInfo {
                arms: vec![suggestion],
            };
            context
                .env
                .add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        } else {
            // If there's a default arm, no suggestion is necessary.
            if matrix.has_default_arm() {
                return;
            }

            let mut unmatched_variants = context
                .enum_variants(&mident, &name)
                .into_iter()
                .collect::<BTreeSet<_>>();
            let ctors = matrix.first_variant_ctors();
            for ctor in ctors.keys() {
                unmatched_variants.remove(ctor);
            }
            // If all of the variants were matched, no suggestion is necessary.
            if unmatched_variants.is_empty() {
                return;
            }
            let mut arms = vec![];
            // re-iterate the original so we generate these in definition order
            for variant in context.enum_variants(&mident, &name).into_iter() {
                if !unmatched_variants.contains(&variant) {
                    continue;
                }
                let is_empty = context.enum_variant_is_empty(&mident, &name, &variant);
                let is_positional = context.enum_variant_is_positional(&mident, &name, &variant);
                let Some(fields) = context.enum_variant_fields(&mident, &name, &variant) else {
                    context.env.add_diag(ice!((
                        loc,
                        "Tried to look up fields for this enum and found none"
                    )));
                    continue;
                };
                let suggestion = if is_empty {
                    PS::UnpackEmptyVariant {
                        module: mident,
                        enum_name: name,
                        variant_name: variant,
                    }
                } else if is_positional {
                    PS::UnpackPositionalVariant {
                        module: mident,
                        enum_name: name,
                        variant_name: variant,
                        field_count: fields.len(),
                    }
                } else {
                    PS::UnpackNamedVariant {
                        module: mident,
                        enum_name: name,
                        variant_name: variant,
                        fields: fields.into_iter().map(|(field, _)| field.value()).collect(),
                    }
                };
                arms.push(suggestion);
            }
            let info = MissingMatchArmsInfo { arms };
            context
                .env
                .add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        }
    }

    let Some(ty) = matrix.tys.first() else {
        context.env.add_diag(ice!((
            loc,
            "Pattern matrix with no types handed to IDE function"
        )));
        return;
    };
    if let Some(sp!(_, BuiltinTypeName_::Bool)) = &ty.value.unfold_to_builtin_type_name() {
        report_bool(context, loc, matrix)
    } else if let Some(_builtin) = ty.value.unfold_to_builtin_type_name() {
        report_builtin(context, loc, matrix)
    } else if let Some((mident, datatype_name)) = ty
        .value
        .unfold_to_type_name()
        .and_then(|sp!(_, name)| name.datatype_name())
    {
        report_datatype(context, loc, matrix, mident, datatype_name)
    } else {
        if !context.env.has_errors() {
            // It's unclear how we got here, so report an ICE and suggest a wildcard.
            context.env.add_diag(ice!((
                loc,
                format!(
                    "Found non-matchable type {} as match subject",
                    error_format(ty, &Subst::empty())
                )
            )));
        }
        if !matrix.has_default_arm() {
            let info = MissingMatchArmsInfo {
                arms: vec![PS::Wildcard],
            };
            context
                .env
                .add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        }
    }
}

//**************************************************************************************************
// Debug Print
//**************************************************************************************************

impl AstDebug for PatternMatrix {
    fn ast_debug(&self, w: &mut AstWriter) {
        for arm in &self.patterns {
            let PatternArm {
                pats: pat,
                guard,
                arm,
            } = arm;
            w.write("    { ");
            w.comma(pat, |w, p| p.ast_debug(w));
            w.write(" } =>");
            if let Some(guard) = guard {
                w.write(" if ");
                guard.ast_debug(w);
            }
            w.write(" [");
            w.write(format!("] arm {}\n", arm.index));
        }
    }
}

impl AstDebug for FringeEntry {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{:?} : ", self.var));
        self.ty.ast_debug(w);
    }
}
