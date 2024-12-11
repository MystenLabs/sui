// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::warning_filters::WarningFilters,
    expansion::ast::{ModuleIdent, Value_},
    ice,
    naming::ast::BuiltinTypeName_,
    parser::ast::{DatatypeName, VariantName},
    shared::{
        ide::{IDEAnnotation, MissingMatchArmsInfo, PatternSuggestion},
        matching::{MatchContext, PatternMatrix},
        string_utils::{debug_print, format_oxford_list},
        Identifier,
    },
    typing::{
        ast as T,
        core::{error_format, Context, Subst},
        visitor::TypingMutVisitorContext,
    },
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use std::{
    collections::{BTreeSet, VecDeque},
    fmt::Display,
};

//**************************************************************************************************
// Description
//**************************************************************************************************
// This visitor performs two match analysis steps:
// 1. If IDE mode is enabled, report all missing top-level arms as IDE information.
// 2. Ensure the match is exhaustive, or replace it with an error if it is not.

//**************************************************************************************************
// Entry and Visitor
//**************************************************************************************************

struct MatchCompiler<'ctx, 'env> {
    context: &'ctx mut Context<'env>,
}

impl TypingMutVisitorContext for MatchCompiler<'_, '_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        use T::UnannotatedExp_ as E;
        let eloc = exp.exp.loc;
        if let E::Match(subject, arms) = &exp.exp.value {
            debug_print!(self.context.debug.match_counterexample,
                ("subject" => subject),
                (lines "arms" => &arms.value)
            );
            if invalid_match(self.context, eloc, subject, arms) {
                debug_print!(
                    self.context.debug.match_counterexample,
                    (msg "counterexample found")
                );
                let err_exp = T::exp(
                    exp.ty.clone(),
                    sp(subject.exp.loc, T::UnannotatedExp_::UnresolvedError),
                );
                let _ = std::mem::replace(exp, err_exp);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn push_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.context.push_warning_filter_scope(filter);
    }

    fn pop_warning_filter_scope(&mut self) {
        self.context.pop_warning_filter_scope();
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

/// Check a match, generating a counterexample if one exists. Also reports IDE arm suggestions as
/// IDE information. If this returns `true`, the match is invalid and should be replaced with an
/// error.
fn invalid_match(
    context: &mut Context,
    loc: Loc,
    subject: &T::Exp,
    arms: &Spanned<Vec<T::MatchArm>>,
) -> bool {
    let arms_loc = arms.loc;
    let (pattern_matrix, _arms) =
        PatternMatrix::from(context, loc, subject.ty.clone(), arms.value.clone());

    let mut counterexample_matrix = pattern_matrix.clone();
    let has_guards = counterexample_matrix.has_guards();
    counterexample_matrix.remove_guarded_arms();
    if context.env.ide_mode() {
        // Do this first, as it's a borrow and a shallow walk.
        ide_report_missing_arms(context, arms_loc, &counterexample_matrix);
    }
    find_counterexample(context, subject.exp.loc, counterexample_matrix, has_guards)
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
        if context.modules.is_struct(&mident, &datatype_name) {
            // For a struct, we only care if we destructure it. If we do, we want to specialize and
            // recur. If we don't, we check it as a default specialization.
            if let Some((ploc, arg_types)) = matrix.first_struct_ctors() {
                let ctor_arity = arg_types.len() as u32;
                let decl_fields = context
                    .modules
                    .struct_fields(&mident, &datatype_name)
                    .unwrap();
                let fringe_binders =
                    context.make_imm_ref_match_binders(decl_fields, ploc, arg_types);
                let is_positional = context
                    .modules
                    .struct_is_positional(&mident, &datatype_name);
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
                .modules
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
                    let decl_fields = context
                        .modules
                        .enum_variant_fields(&mident, &datatype_name, &ctor)
                        .unwrap();
                    let fringe_binders =
                        context.make_imm_ref_match_binders(decl_fields, ploc, arg_types);
                    let is_positional =
                        context
                            .modules
                            .enum_variant_is_positional(&mident, &datatype_name, &ctor);
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
                        let is_positional = context.modules.enum_variant_is_positional(
                            &mident,
                            &datatype_name,
                            variant_name,
                        );
                        let ctor_args = context
                            .modules
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
        } else if matrix.is_empty() {
            Some(make_wildcards(arity as usize))
        } else {
            // An error case: no entry on the fringe but no
            if !context.env.has_errors() {
                context.add_diag(ice!((
                    matrix.loc,
                    "Non-empty matrix with non errors but no type"
                )));
            }
            None
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
        context.add_diag(diag);
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
            context.add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        }
    }

    fn report_builtin(context: &mut Context, loc: Loc, matrix: &PatternMatrix) {
        // For all other non-literals, we don't consider a case where the constructors are
        // saturated. If it doesn't have a wildcard, we suggest adding a wildcard.
        if !matrix.has_default_arm() {
            let info = MissingMatchArmsInfo {
                arms: vec![PS::Wildcard],
            };
            context.add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        }
    }

    fn report_datatype(
        context: &mut Context,
        loc: Loc,
        matrix: &PatternMatrix,
        mident: ModuleIdent,
        name: DatatypeName,
    ) {
        if context.modules.is_struct(&mident, &name) {
            if !matrix.is_empty() {
                // If the matrix isn't empty, we _must_ have matched the struct with at least one
                // non-guard arm (either wildcards or the struct itself), so we're fine.
                return;
            }
            // If the matrix _is_ empty, we suggest adding an unpack.
            let is_positional = context.modules.struct_is_positional(&mident, &name);
            let Some(fields) = context.modules.struct_fields(&mident, &name) else {
                context.add_diag(ice!((
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
            context.add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        } else {
            // If there's a default arm, no suggestion is necessary.
            if matrix.has_default_arm() {
                return;
            }

            let mut unmatched_variants = context
                .modules
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
            for variant in context.modules.enum_variants(&mident, &name).into_iter() {
                if !unmatched_variants.contains(&variant) {
                    continue;
                }
                let is_empty = context
                    .modules
                    .enum_variant_is_empty(&mident, &name, &variant);
                let is_positional = context
                    .modules
                    .enum_variant_is_positional(&mident, &name, &variant);
                let Some(fields) = context
                    .modules
                    .enum_variant_fields(&mident, &name, &variant)
                else {
                    context.add_diag(ice!((
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
            context.add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        }
    }

    let Some(ty) = matrix.tys.first() else {
        context.add_diag(ice!((
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
            context.add_diag(ice!((
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
            context.add_ide_annotation(loc, IDEAnnotation::MissingMatchArms(Box::new(info)));
        }
    }
}
