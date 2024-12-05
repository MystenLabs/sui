// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module verifies the usage of the "syntax method" functions. These functions are declared
//! as 'syntax' but have not been ensured to be type-compatible or otherwise adhere to our
//! trait-like constraints around their definitions. We process them here, using typing machinery
//! to ensure the are.

use crate::{
    diag,
    diagnostics::Diagnostic,
    expansion::ast::ModuleIdent,
    ice,
    naming::ast::{self as N, IndexSyntaxMethods, SyntaxMethod},
    typing::core::{self, Context},
};
use move_ir_types::location::*;
use std::collections::{BTreeMap, BTreeSet};

//-------------------------------------------------------------------------------------------------
// Validation
//-------------------------------------------------------------------------------------------------

pub fn validate_syntax_methods(
    context: &mut Context,
    _mident: &ModuleIdent,
    module: &mut N::ModuleDefinition,
) {
    let methods = &mut module.syntax_methods;
    for (_, entry) in methods.iter_mut() {
        if let Some(index) = &mut entry.index {
            let IndexSyntaxMethods { index, index_mut } = &mut **index;
            if let (Some(index_defn), Some(index_mut_defn)) = (index.as_ref(), index_mut.as_ref()) {
                if !validate_index_syntax_methods(context, index_defn, index_mut_defn) {
                    // If we didn't validate they wre comptaible, we remove the mut one to avoid more
                    // typing issues later.
                    assert!(context.env.has_errors());
                    *index_mut = None;
                }
            }
        }
    }
}

fn validate_index_syntax_methods(
    context: &mut Context,
    index: &SyntaxMethod,
    index_mut: &SyntaxMethod,
) -> bool {
    let index_ann_loc = index.kind.loc;
    let (index_module, index_fn) = &index.target_function;
    let (index_mut_module, index_mut_fn) = &index_mut.target_function;

    let index_finfo = context.function_info(index_module, index_fn).clone();
    let mut_finfo = context
        .function_info(index_mut_module, index_mut_fn)
        .clone();

    if index_finfo.signature.type_parameters.len() != mut_finfo.signature.type_parameters.len() {
        let index_msg = format!(
            "This index function expects {} type arguments",
            index_finfo.signature.type_parameters.len()
        );
        let index_mut_msg = format!(
            "This mutable index function expects {} type arguments",
            mut_finfo.signature.type_parameters.len()
        );
        let mut diag = diag!(
            TypeSafety::IncompatibleSyntaxMethods,
            (index.loc, index_msg),
            (index_mut.loc, index_mut_msg),
        );
        diag.add_note(
            "Index operations on the same type must take the name number of type arguments",
        );
        context.add_diag(diag);
        return false;
    }

    if index_finfo.signature.parameters.len() != mut_finfo.signature.parameters.len() {
        let index_msg = format!(
            "This index function expects {} parameters",
            index_finfo.signature.parameters.len()
        );
        let index_mut_msg = format!(
            "This mutable index function expects {} parameters ",
            mut_finfo.signature.parameters.len()
        );
        let mut diag = diag!(
            TypeSafety::IncompatibleSyntaxMethods,
            (index.loc, index_msg),
            (index_mut.loc, index_mut_msg),
        );
        diag.add_note("Index operations on the same type must take the name number of parameters");
        context.add_diag(diag);
        return false;
    }

    let mut valid = true;

    // Check that the type parameters have the same abilities.
    for (index_tparam, mut_tparam) in index_finfo
        .signature
        .type_parameters
        .iter()
        .zip(mut_finfo.signature.type_parameters.iter())
    {
        for ability in index_tparam.abilities.iter() {
            if !mut_tparam.abilities.has_ability(&ability) {
                let index_msg = format!(
                    "This index function type parameter has the '{}' ability",
                    ability
                );
                let index_mut_msg = "This mutable index function type parameter does not";
                let mut diag = diag!(
                    TypeSafety::IncompatibleSyntaxMethods,
                    (index_tparam.user_specified_name.loc, index_msg),
                    (ability.loc, "Ability defined here"),
                    (mut_tparam.user_specified_name.loc, index_mut_msg),
                );
                diag.add_note(
                    "Index operations on use the same abilities for their type parameters",
                );
                context.add_diag(diag);
                valid = false;
            }
        }

        for ability in mut_tparam.abilities.iter() {
            if !index_tparam.abilities.has_ability(&ability) {
                let index_mut_msg = format!(
                    "This mutable index function type parameter has the '{}' ability",
                    ability
                );
                let index_msg = "This index function type parameter does not";
                let mut diag = diag!(
                    TypeSafety::IncompatibleSyntaxMethods,
                    (mut_tparam.user_specified_name.loc, index_mut_msg),
                    (ability.loc, "Ability defined here"),
                    (index_tparam.user_specified_name.loc, index_msg),
                );
                diag.add_note(
                    "Index operations on use the same abilities for their type parameters",
                );
                context.add_diag(diag);
                valid = false;
            }
        }
    }

    // Now we simply want to make the types the same w/r/t type parameters. To do this, we
    // instantiate the type parameters of the index one and ground them to the type parameters of
    // the mutable one. Finally, we walk both types and make sure they agree.

    // make_function_type updates the subst on the context, and we also don't want to leave
    // lingering constraints, so we preserve the current versions here to reinstate at the end.
    let prev_constraints = std::mem::take(&mut context.constraints);
    let prev_subst = std::mem::replace(&mut context.subst, core::Subst::empty());

    let mut_tparam_types = mut_finfo
        .signature
        .type_parameters
        .iter()
        .map(|tp| sp(tp.user_specified_name.loc, N::Type_::Param(tp.clone())))
        .collect::<Vec<_>>();

    // NOTE: This calls the version of `make_function_type_` that does not check function
    // visibility, since that is not relevant here.
    let index_ty = core::make_function_type_no_visibility_check(
        context,
        index_ann_loc,
        index_module,
        index_fn,
        Some(mut_tparam_types),
    );
    context.current_module = None;

    let index_params = index_ty.params.iter().map(|(_, t1)| t1);
    let mut_params = mut_finfo.signature.parameters.iter().map(|(_, _, ty)| ty);
    let mut param_tys = index_params.zip(mut_params).enumerate();

    let mut subst = std::mem::replace(&mut context.subst, core::Subst::empty());

    // The first one is a subtype because we want to ensure the `&mut` param is a subtype of the
    // `&` param. We already ensured they were both references of the appropriate shape in naming,
    // so this is a bit redundant.
    if let Some((ndx, (subject_ref_type, subject_mut_ref_type))) = param_tys.next() {
        if let Ok((subst_, _)) = core::subtype(
            &mut context.tvar_counter,
            subst.clone(),
            subject_mut_ref_type,
            subject_ref_type,
        ) {
            subst = subst_;
        } else {
            let (_, _, index_type) = &index_finfo.signature.parameters[ndx];
            let (_, _, mut_type) = &mut_finfo.signature.parameters[ndx];
            // This case shouldn't really be reachable, but we might as well provide an error.
            let index_msg = format!(
                "This index function subject has type {}",
                ty_str(index_type)
            );
            let N::Type_::Ref(false, inner) =
                core::ready_tvars(&subst, subject_ref_type.clone()).value
            else {
                context.add_diag(ice!((
                    index_finfo.signature.return_type.loc,
                    "This index function got to type verification with an invalid type"
                )));
                return false;
            };
            let expected_type = sp(mut_type.loc, N::Type_::Ref(true, inner.clone()));
            let mut_msg = format!(
                "Expected this mutable index function subject to have type {}",
                ty_str(&expected_type)
            );
            let mut_msg_2 = format!("It has type {}", ty_str(mut_type));
            let mut diag = diag!(
                TypeSafety::IncompatibleSyntaxMethods,
                (index_type.loc, index_msg),
                (mut_type.loc, mut_msg),
                (mut_type.loc, mut_msg_2)
            );
            add_type_param_info(
                &mut diag,
                index_type,
                &index_finfo.signature.type_parameters,
                mut_type,
                &mut_finfo.signature.type_parameters,
            );
            diag.add_note(
                "These functions must take the same subject type, differing only by mutability",
            );
            context.add_diag(diag);
            valid = false;
        }
    } else {
        valid = false;
    }

    // We ensure the rest of the parameters match exactly.
    for (ndx, (ptype, mut_ptype)) in param_tys {
        if let Ok((subst_, _)) =
            core::invariant(&mut context.tvar_counter, subst.clone(), ptype, mut_ptype)
        {
            subst = subst_;
        } else {
            let (_, _, index_type) = &index_finfo.signature.parameters[ndx];
            let (_, _, mut_type) = &mut_finfo.signature.parameters[ndx];
            let index_msg = format!("This parameter has type {}", ty_str(index_type));
            let mut_msg = format!(
                "Expected this parameter to have type {}",
                ty_str(&core::ready_tvars(&subst, ptype.clone()))
            );
            let mut_msg_2 = format!("It has type {}", ty_str(mut_type));
            let mut diag = diag!(
                TypeSafety::IncompatibleSyntaxMethods,
                (index_type.loc, index_msg),
                (mut_type.loc, mut_msg),
                (mut_type.loc, mut_msg_2)
            );
            add_type_param_info(
                &mut diag,
                index_type,
                &index_finfo.signature.type_parameters,
                mut_type,
                &mut_finfo.signature.type_parameters,
            );
            diag.add_note("Index operation non-subject parameter types must match exactly");
            context.add_diag(diag);
            valid = false;
        }
    }

    // Similar to the subject type, we ensure the return types are the same. We already checked
    // that they are appropriately-shaped references, and now we ensure they refer to the same type
    // under the reference.
    if core::subtype(
        &mut context.tvar_counter,
        subst.clone(),
        &mut_finfo.signature.return_type,
        &index_ty.return_,
    )
    .is_err()
    {
        let index_type = &index_finfo.signature.return_type;
        let mut_type = &mut_finfo.signature.return_type;
        let index_msg = format!("This index function returns type {}", ty_str(index_type));
        let N::Type_::Ref(false, inner) = core::ready_tvars(&subst, index_ty.return_.clone()).value
        else {
            context.add_diag(ice!((
                index_finfo.signature.return_type.loc,
                "This index function got to type verification with an invalid type"
            )));
            return false;
        };
        let expected_type = sp(mut_type.loc, N::Type_::Ref(true, inner.clone()));
        let mut_msg = format!(
            "Expected this mutable index function to return type {}",
            ty_str(&expected_type)
        );
        let mut_msg_2 = format!("It returns type {}", ty_str(mut_type));
        let mut diag = diag!(
            TypeSafety::IncompatibleSyntaxMethods,
            (index_type.loc, index_msg),
            (mut_type.loc, mut_msg),
            (mut_type.loc, mut_msg_2)
        );
        add_type_param_info(
            &mut diag,
            index_type,
            &index_finfo.signature.type_parameters,
            mut_type,
            &mut_finfo.signature.type_parameters,
        );
        diag.add_note("These functions must return the same type, differing only by mutability");
        context.add_diag(diag);
        valid = false;
    }

    let _ = std::mem::replace(&mut context.subst, prev_subst);
    let _ = std::mem::replace(&mut context.constraints, prev_constraints);

    valid
}

// Error printing helpers

fn add_type_param_info(
    diag: &mut Diagnostic,
    index_ty: &N::Type,
    index_tparams: &[N::TParam],
    mut_ty: &N::Type,
    mut_tparams: &[N::TParam],
) {
    let index_posns = type_param_positions(index_ty, index_tparams);
    let mut_posns = type_param_positions(mut_ty, mut_tparams);
    let index_names = index_posns.keys().clone().collect::<BTreeSet<_>>();
    let mut_names = mut_posns.keys().clone().collect::<BTreeSet<_>>();
    let shared_names = index_names.intersection(&mut_names);
    let mut added_info = false;
    for name in shared_names {
        let (index_posn, index_loc) = index_posns.get(name).unwrap();
        let (mut_posn, mut_loc) = mut_posns.get(name).unwrap();
        if index_posn != mut_posn {
            added_info = true;
            diag.add_secondary_label((
                *index_loc,
                format!(
                    "Type parameter {} appears in position {} here",
                    name,
                    index_posn + 1
                ),
            ));
            diag.add_secondary_label((
                *mut_loc,
                format!(
                    "Type parameter {} appears in position {} here",
                    name,
                    mut_posn + 1
                ),
            ));
        }
    }
    if added_info {
        diag.add_note("Type parameters must be used the same by position, not name");
    }
}

fn type_param_positions(
    ty: &N::Type,
    tparams: &[N::TParam],
) -> BTreeMap<crate::shared::Name, (usize, Loc)> {
    let fn_tparams = core::all_tparams(ty.clone());
    fn_tparams
        .into_iter()
        .filter_map(|tparam| {
            if let Some(posn) = tparams.iter().position(|t| t == &tparam) {
                Some((
                    tparam.user_specified_name,
                    (posn, tparam.user_specified_name.loc),
                ))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>()
}

fn ty_str(ty: &N::Type) -> String {
    core::error_format(ty, &core::Subst::empty())
}
