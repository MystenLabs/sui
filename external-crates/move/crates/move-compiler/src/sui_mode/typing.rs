// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use crate::{
    diag,
    diagnostics::{warning_filters::WarningFilters, Diagnostic, DiagnosticReporter, Diagnostics},
    editions::Flavor,
    expansion::ast::{AbilitySet, Fields, ModuleIdent, Mutability, Visibility},
    naming::ast::{
        self as N, BuiltinTypeName_, FunctionSignature, StructFields, Type, TypeName_, Type_, Var,
    },
    parser::ast::{Ability_, DatatypeName, DocComment, FunctionName, TargetKind},
    shared::{program_info::TypingProgramInfo, CompilationEnv, Identifier},
    sui_mode::*,
    typing::{
        ast::{self as T, ModuleCall},
        core::{ability_not_satisfied_tips, error_format, error_format_, Subst},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};

//**************************************************************************************************
// Visitor
//**************************************************************************************************

pub struct SuiTypeChecks;

impl TypingVisitorConstructor for SuiTypeChecks {
    type Context<'a> = Context<'a>;
    fn context<'a>(env: &'a CompilationEnv, program: &T::Program) -> Self::Context<'a> {
        Context::new(env, program.info.clone())
    }
}

//**************************************************************************************************
// Context
//**************************************************************************************************

#[allow(unused)]
pub struct Context<'a> {
    env: &'a CompilationEnv,
    reporter: DiagnosticReporter<'a>,
    info: Arc<TypingProgramInfo>,
    sui_transfer_ident: Option<ModuleIdent>,
    current_module: Option<ModuleIdent>,
    otw_name: Option<Symbol>,
    one_time_witness: Option<Result<DatatypeName, ()>>,
    in_test: bool,
}

impl<'a> Context<'a> {
    fn new(env: &'a CompilationEnv, info: Arc<TypingProgramInfo>) -> Self {
        let sui_module_ident = info
            .modules
            .key_cloned_iter()
            .find(|(m, _)| m.value.is(&SUI_ADDR_VALUE, TRANSFER_MODULE_NAME))
            .map(|(m, _)| m);
        let reporter = env.diagnostic_reporter_at_top_level();
        Context {
            env,
            reporter,
            info,
            sui_transfer_ident: sui_module_ident,
            current_module: None,
            otw_name: None,
            one_time_witness: None,
            in_test: false,
        }
    }

    fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    #[allow(unused)]
    fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    fn set_module(&mut self, current_module: ModuleIdent) {
        self.current_module = Some(current_module);
        self.otw_name = Some(Symbol::from(
            current_module.value.module.0.value.as_str().to_uppercase(),
        ));
        self.one_time_witness = None;
    }

    fn current_module(&self) -> &ModuleIdent {
        self.current_module.as_ref().unwrap()
    }

    fn otw_name(&self) -> Symbol {
        self.otw_name.unwrap()
    }
}

const OTW_NOTE: &str = "One-time witness types are structs with the following requirements: \
                        their name is the upper-case version of the module's name, \
                        they have no fields (or a single boolean field), \
                        they have no type parameters, \
                        and they have only the 'drop' ability.";

//**************************************************************************************************
// Entry
//**************************************************************************************************

impl<'a> TypingVisitorContext for Context<'a> {
    fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }

    fn visit_module_custom(&mut self, ident: ModuleIdent, mdef: &T::ModuleDefinition) -> bool {
        let config = self.env.package_config(mdef.package_name);
        if config.flavor != Flavor::Sui {
            // Skip if not sui
            return true;
        }
        if !matches!(
            mdef.target_kind,
            TargetKind::Source {
                is_root_package: true
            }
        ) {
            // Skip non-source, dependency modules
            return true;
        }

        self.set_module(ident);
        self.in_test = mdef.attributes.is_test_or_test_only();
        if let Some(sdef) = mdef.structs.get_(&self.otw_name()) {
            let valid_fields = if let N::StructFields::Defined(_, fields) = &sdef.fields {
                invalid_otw_field_loc(fields).is_none()
            } else {
                true
            };
            if valid_fields {
                let name = mdef.structs.get_full_key_(&self.otw_name()).unwrap();
                check_otw_type(self, name, sdef, None)
            }
        }

        if let Some(fdef) = mdef.functions.get_(&INIT_FUNCTION_NAME) {
            let name = mdef.functions.get_full_key_(&INIT_FUNCTION_NAME).unwrap();
            init_signature(self, name, &fdef.signature)
        }

        for (name, sdef) in mdef.structs.key_cloned_iter() {
            struct_def(self, name, sdef)
        }

        for (name, edef) in mdef.enums.key_cloned_iter() {
            enum_def(self, name, edef)
        }

        // do not skip module
        false
    }

    fn visit_function_custom(
        &mut self,
        module: ModuleIdent,
        name: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        debug_assert!(self.current_module.as_ref() == Some(&module));
        function(self, name, fdef);
        // skip since we have already visited the body
        true
    }

    fn visit_exp_custom(&mut self, e: &T::Exp) -> bool {
        exp(self, e);
        // do not skip recursion
        false
    }
}

//**************************************************************************************************
// Structs
//**************************************************************************************************

fn struct_def(context: &mut Context, name: DatatypeName, sdef: &N::StructDefinition) {
    let N::StructDefinition {
        doc: _,
        warning_filter: _,
        index: _,
        loc: _,
        attributes: _,
        abilities,
        type_parameters: _,
        fields,
    } = sdef;
    let Some(key_loc) = abilities.ability_loc_(Ability_::Key) else {
        // not an object, no extra rules
        return;
    };

    let StructFields::Defined(_, fields) = fields else {
        return;
    };
    let invalid_first_field = if fields.is_empty() {
        // no fields
        Some(name.loc())
    } else {
        fields
            .iter()
            .find(|(_, name, (idx, _))| *idx == 0 && **name != ID_FIELD_NAME)
            .map(|(loc, _, _)| loc)
    };
    if let Some(loc) = invalid_first_field {
        // no fields or an invalid 'id' field
        context.add_diag(invalid_object_id_field_diag(key_loc, loc, name));
        return;
    };

    let (_, (_, id_field_type)) = fields.get_(&ID_FIELD_NAME).unwrap();
    let id_field_loc = fields.get_loc_(&ID_FIELD_NAME).unwrap();
    if !id_field_type
        .value
        .is(&SUI_ADDR_VALUE, OBJECT_MODULE_NAME, UID_TYPE_NAME)
    {
        let actual = format!(
            "But found type: {}",
            error_format(id_field_type, &Subst::empty())
        );
        let mut diag = invalid_object_id_field_diag(key_loc, *id_field_loc, name);
        diag.add_secondary_label((id_field_type.loc, actual));
        context.add_diag(diag);
    }
}

fn invalid_object_id_field_diag(key_loc: Loc, loc: Loc, name: DatatypeName) -> Diagnostic {
    const KEY_MSG: &str = "The 'key' ability is used to declare objects in Sui";

    let msg = format!(
        "Invalid object '{}'. \
        Structs with the '{}' ability must have '{}: {}::{}::{}' as their first field",
        name,
        Ability_::Key,
        ID_FIELD_NAME,
        SUI_ADDR_NAME,
        OBJECT_MODULE_NAME,
        UID_TYPE_NAME
    );
    diag!(OBJECT_DECL_DIAG, (loc, msg), (key_loc, KEY_MSG))
}

//**************************************************************************************************
// Enums
//**************************************************************************************************

fn enum_def(context: &mut Context, name: DatatypeName, edef: &N::EnumDefinition) {
    let N::EnumDefinition {
        doc: _,
        warning_filter: _,
        index: _,
        loc: _loc,
        attributes: _,
        abilities,
        type_parameters: _,
        variants: _,
    } = edef;
    if let Some(key_loc) = abilities.ability_loc_(Ability_::Key) {
        let msg = format!("Invalid object '{name}'");
        let key_msg = format!("Enums cannot have the '{}' ability.", Ability_::Key);
        let diag = diag!(OBJECT_DECL_DIAG, (name.loc(), msg), (key_loc, key_msg));
        context.add_diag(diag);
    };
}

//**************************************************************************************************
// Functions
//**********************************************************************************************

fn function(context: &mut Context, name: FunctionName, fdef: &T::Function) {
    let T::Function {
        doc: _,
        loc: _,
        compiled_visibility: _,
        visibility,
        signature,
        body,
        warning_filter: _,
        index: _,
        macro_: _,
        attributes,
        entry,
    } = fdef;
    let prev_in_test = context.in_test;
    if attributes.is_test_or_test_only() {
        context.in_test = true;
    }
    if name.0.value == INIT_FUNCTION_NAME {
        init_visibility(context, name, *visibility, *entry);
    }
    if let Some(entry_loc) = entry {
        entry_signature(context, *entry_loc, name, signature);
    }
    if let sp!(_, T::FunctionBody_::Defined(seq)) = body {
        context.visit_seq(body.loc, seq)
    }
    context.in_test = prev_in_test;
}

//**************************************************************************************************
// init
//**************************************************************************************************

fn init_visibility(
    context: &mut Context,
    name: FunctionName,
    visibility: Visibility,
    entry: Option<Loc>,
) {
    match visibility {
        Visibility::Public(loc) | Visibility::Friend(loc) | Visibility::Package(loc) => context
            .add_diag(diag!(
                INIT_FUN_DIAG,
                (name.loc(), "Invalid 'init' function declaration"),
                (loc, "'init' functions must be internal to their module"),
            )),
        Visibility::Internal => (),
    }
    if let Some(entry) = entry {
        context.add_diag(diag!(
            INIT_FUN_DIAG,
            (name.loc(), "Invalid 'init' function declaration"),
            (entry, "'init' functions cannot be 'entry' functions"),
        ));
    }
}

fn init_signature(context: &mut Context, name: FunctionName, signature: &FunctionSignature) {
    let FunctionSignature {
        type_parameters,
        parameters,
        return_type,
    } = signature;
    if !type_parameters.is_empty() {
        let tp_loc = type_parameters[0].user_specified_name.loc;
        context.add_diag(diag!(
            INIT_FUN_DIAG,
            (name.loc(), "Invalid 'init' function declaration"),
            (tp_loc, "'init' functions cannot have type parameters"),
        ));
    }
    if !matches!(return_type, sp!(_, Type_::Unit)) {
        let msg = format!(
            "'init' functions must have a return type of {}",
            error_format_(&Type_::Unit, &Subst::empty())
        );
        context.add_diag(diag!(
            INIT_FUN_DIAG,
            (name.loc(), "Invalid 'init' function declaration"),
            (return_type.loc, msg),
        ))
    }
    let last_loc = parameters
        .last()
        .map(|(_, _, sp!(loc, _))| *loc)
        .unwrap_or(name.loc());
    let tx_ctx_kind = parameters
        .last()
        .map(|(_, _, last_param_ty)| tx_context_kind(last_param_ty))
        .unwrap_or(TxContextKind::None);
    if tx_ctx_kind == TxContextKind::None {
        let msg = format!(
            "'init' functions must have their last parameter as \
            '&{a}::{m}::{t}' or '&mut {a}::{m}::{t}'",
            a = SUI_ADDR_NAME,
            m = TX_CONTEXT_MODULE_NAME,
            t = TX_CONTEXT_TYPE_NAME,
        );
        context.add_diag(diag!(
            INIT_FUN_DIAG,
            (name.loc(), "Invalid 'init' function declaration"),
            (last_loc, msg),
        ))
    }

    let info = context.info.clone();
    let otw_name: Symbol = context.otw_name();
    if parameters.len() == 1
        && context.one_time_witness.is_some()
        && tx_ctx_kind != TxContextKind::None
    {
        // if there is 1 parameter, and a OTW, this is an error since the OTW must be used
        let msg = format!(
            "Invalid first parameter to 'init'. \
            Expected this module's one-time witness type '{}::{otw_name}'",
            context.current_module(),
        );
        let otw_loc = context
            .info
            .struct_declared_loc_(context.current_module(), &otw_name);
        let otw_msg = "One-time witness declared here";
        let mut diag = diag!(
            INIT_FUN_DIAG,
            (parameters[0].2.loc, msg),
            (otw_loc, otw_msg),
        );
        diag.add_note(OTW_NOTE);
        context.add_diag(diag)
    } else if parameters.len() > 1 {
        // if there is more than one parameter, the first must be the OTW
        let (_, first_var, first_ty) = parameters.first().unwrap();
        let is_otw = matches!(&first_ty.value, Type_::UnresolvedError | Type_::Var(_))
            || matches!(
                first_ty.value.type_name(),
                Some(sp!(_, TypeName_::ModuleType(m, n)))
                    if m == context.current_module() && n.value() == otw_name
            );
        if !is_otw {
            let msg = format!(
                "Invalid parameter '{}' of type {}. \
                Expected a one-time witness type, '{}::{otw_name}",
                first_var.value.name,
                error_format(first_ty, &Subst::empty()),
                context.current_module(),
            );
            let mut diag = diag!(
                INIT_FUN_DIAG,
                (name.loc(), "Invalid 'init' function declaration"),
                (first_ty.loc, msg)
            );
            diag.add_note(OTW_NOTE);
            context.add_diag(diag)
        } else if let Some(sdef) = info
            .module(context.current_module())
            .structs
            .get_(&otw_name)
        {
            let name = context
                .info
                .module(context.current_module())
                .structs
                .get_full_key_(&otw_name)
                .unwrap();
            check_otw_type(context, name, sdef, Some(first_ty.loc))
        }
    }
    if parameters.len() > 2 {
        // no init function can take more than 2 parameters (the OTW and the TxContext)
        let (_, third_var, _) = &parameters[2];
        context.add_diag(diag!(
            INIT_FUN_DIAG,
            (name.loc(), "Invalid 'init' function declaration"),
            (
                third_var.loc,
                "'init' functions can have at most two parameters"
            ),
        ));
    }
}

// While theoretically we could call this just once for the upper cased module struct, we break it
// out into a separate function to help programmers understand the rules for one-time witness types,
// when trying to write an 'init' function.
fn check_otw_type(
    context: &mut Context,
    name: DatatypeName,
    sdef: &N::StructDefinition,
    usage_loc: Option<Loc>,
) {
    const OTW_USAGE: &str = "Attempted usage as a one-time witness here";
    if context.one_time_witness.is_some() {
        return;
    }

    let otw_diag = |mut diag: Diagnostic| {
        if let Some(usage) = usage_loc {
            diag.add_secondary_label((usage, OTW_USAGE))
        }
        diag.add_note(OTW_NOTE);
        diag
    };
    let mut valid = true;
    if let Some(tp) = sdef.type_parameters.first() {
        let msg = "One-time witness types cannot have type parameters";
        context.add_diag(otw_diag(diag!(
            OTW_DECL_DIAG,
            (name.loc(), "Invalid one-time witness declaration"),
            (tp.param.user_specified_name.loc, msg),
        )));
        valid = false;
    }

    if let N::StructFields::Defined(_, fields) = &sdef.fields {
        let invalid_otw_opt = invalid_otw_field_loc(fields);
        if let Some(invalid_otw_opt) = invalid_otw_opt {
            let msg_base = format!(
                "One-time witness types must have no fields, \
                or exactly one field of type {}",
                error_format(&Type_::bool(name.loc()), &Subst::empty())
            );
            let (invalid_loc, invalid_msg) = match invalid_otw_opt {
                InvalidOTW::FirstFieldNotBool(loc) => (loc, msg_base),
                InvalidOTW::MoreThanOneField(loc) => {
                    (loc, format!("Found more than one field. {msg_base}"))
                }
            };
            context.add_diag(otw_diag(diag!(
                OTW_DECL_DIAG,
                (name.loc(), "Invalid one-time witness declaration"),
                (invalid_loc, invalid_msg),
            )));
            valid = false
        };
    }

    let invalid_ability_loc =
        if !sdef.abilities.has_ability_(Ability_::Drop) || sdef.abilities.len() > 1 {
            let loc = sdef
                .abilities
                .iter()
                .find_map(|a| {
                    if a.value != Ability_::Drop {
                        Some(a.loc)
                    } else {
                        None
                    }
                })
                .unwrap_or(name.loc());
            Some(loc)
        } else {
            None
        };
    if let Some(loc) = invalid_ability_loc {
        let msg = format!(
            "One-time witness types can only have the have the '{}' ability",
            Ability_::Drop
        );
        context.add_diag(otw_diag(diag!(
            OTW_DECL_DIAG,
            (name.loc(), "Invalid one-time witness declaration"),
            (loc, msg),
        )));
        valid = false
    }

    context.one_time_witness = Some(if valid { Ok(name) } else { Err(()) })
}

enum InvalidOTW {
    FirstFieldNotBool(Loc),
    MoreThanOneField(Loc),
}

// Find the first invalid field in a one-time witness type, if any.
// First looks for a non-boolean field, otherwise looks for any field after the first.
fn invalid_otw_field_loc(fields: &Fields<(DocComment, Type)>) -> Option<InvalidOTW> {
    let invalid_first_field = fields.iter().find_map(|(loc, _, (idx, (_, ty)))| {
        if *idx != 0 {
            return None;
        }
        match ty.value.builtin_name() {
            Some(sp!(_, BuiltinTypeName_::Bool)) => None,
            _ => Some(loc),
        }
    });
    if let Some(loc) = invalid_first_field {
        return Some(InvalidOTW::FirstFieldNotBool(loc));
    }

    let more_than_one_field = fields
        .iter()
        .find(|(_, _, (idx, _))| *idx > 0)
        .map(|(loc, _, _)| loc);
    if let Some(loc) = more_than_one_field {
        return Some(InvalidOTW::MoreThanOneField(loc));
    }

    None
}

//**************************************************************************************************
// entry types
//**************************************************************************************************

fn entry_signature(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    signature: &FunctionSignature,
) {
    let FunctionSignature {
        type_parameters: _,
        parameters,
        return_type,
    } = signature;
    let all_non_ctx_parameters = match parameters.last() {
        Some((_, _, last_param_ty)) if tx_context_kind(last_param_ty) != TxContextKind::None => {
            &parameters[0..parameters.len() - 1]
        }
        _ => parameters,
    };
    entry_param(context, entry_loc, name, all_non_ctx_parameters);
    entry_return(context, entry_loc, name, return_type);
}

fn tx_context_kind(sp!(_, last_param_ty_): &Type) -> TxContextKind {
    // Already an error, so assume a valid, mutable TxContext
    if matches!(last_param_ty_, Type_::UnresolvedError | Type_::Var(_)) {
        return TxContextKind::Mutable;
    }

    let Type_::Ref(is_mut, inner_ty) = last_param_ty_ else {
        // not a reference
        return TxContextKind::None;
    };
    let Type_::Apply(_, sp!(_, inner_name), _) = &inner_ty.value else {
        // not a user defined type
        return TxContextKind::None;
    };
    if inner_name.is(
        &SUI_ADDR_VALUE,
        TX_CONTEXT_MODULE_NAME,
        TX_CONTEXT_TYPE_NAME,
    ) {
        if *is_mut {
            TxContextKind::Mutable
        } else {
            TxContextKind::Immutable
        }
    } else {
        // not the tx context
        TxContextKind::None
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TxContextKind {
    // No TxContext
    None,
    // &mut TxContext
    Mutable,
    // &TxContext
    Immutable,
}

fn entry_param(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    parameters: &[(Mutability, Var, Type)],
) {
    for (_, var, ty) in parameters {
        entry_param_ty(context, entry_loc, name, var, ty);
    }
}

/// A valid entry param type is
/// - A primitive (including strings, ID, and object)
/// - A vector of primitives (including nested vectors)
///
/// - An object
/// - A reference to an object
/// - A vector of objects
fn entry_param_ty(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    param: &Var,
    param_ty: &Type,
) {
    let is_mut_clock = is_mut_clock(param_ty);
    let is_mut_random = is_mut_random(param_ty);

    // TODO better error message for cases such as `MyObject<InnerTypeWithoutStore>`
    // which should give a contextual error about `MyObject` having `key`, but the instantiation
    // `MyObject<InnerTypeWithoutStore>` not having `key` due to `InnerTypeWithoutStore` not having
    // `store`
    let is_valid = is_entry_primitive_ty(param_ty)
        || is_entry_object_ty(param_ty)
        || is_entry_receiving_ty(param_ty);
    if is_mut_clock || is_mut_random || !is_valid {
        let pmsg = format!(
            "Invalid 'entry' parameter type for parameter '{}'",
            param.value.name
        );
        let tmsg = if is_mut_clock {
            format!(
                "{a}::{m}::{n} must be passed by immutable reference, e.g. '&{a}::{m}::{n}'",
                a = SUI_ADDR_NAME,
                m = CLOCK_MODULE_NAME,
                n = CLOCK_TYPE_NAME,
            )
        } else if is_mut_random {
            format!(
                "{a}::{m}::{n} must be passed by immutable reference, e.g. '&{a}::{m}::{n}'",
                a = SUI_ADDR_NAME,
                m = RANDOMNESS_MODULE_NAME,
                n = RANDOMNESS_STATE_TYPE_NAME,
            )
        } else {
            "'entry' parameters must be primitives (by-value), vectors of primitives, objects \
            (by-reference or by-value), vectors of objects, or 'Receiving' arguments (by-reference or by-value)"
                .to_owned()
        };
        let emsg = format!("'{name}' was declared 'entry' here");
        context.add_diag(diag!(
            ENTRY_FUN_SIGNATURE_DIAG,
            (param.loc, pmsg),
            (param_ty.loc, tmsg),
            (entry_loc, emsg)
        ));
    }
}

fn is_mut_clock(param_ty: &Type) -> bool {
    match &param_ty.value {
        Type_::Ref(/* mut */ false, _) => false,
        Type_::Ref(/* mut */ true, t) => is_mut_clock(t),
        Type_::Apply(_, sp!(_, n_), _) => {
            n_.is(&SUI_ADDR_VALUE, CLOCK_MODULE_NAME, CLOCK_TYPE_NAME)
        }
        Type_::Unit
        | Type_::Param(_)
        | Type_::Var(_)
        | Type_::Anything
        | Type_::UnresolvedError
        | Type_::Fun(_, _) => false,
    }
}

fn is_mut_random(param_ty: &Type) -> bool {
    match &param_ty.value {
        Type_::Ref(/* mut */ false, _) => false,
        Type_::Ref(/* mut */ true, t) => is_mut_random(t),
        Type_::Apply(_, sp!(_, n_), _) => n_.is(
            &SUI_ADDR_VALUE,
            RANDOMNESS_MODULE_NAME,
            RANDOMNESS_STATE_TYPE_NAME,
        ),
        Type_::Unit
        | Type_::Param(_)
        | Type_::Var(_)
        | Type_::Anything
        | Type_::UnresolvedError
        | Type_::Fun(_, _) => false,
    }
}

fn is_entry_receiving_ty(param_ty: &Type) -> bool {
    match &param_ty.value {
        Type_::Ref(_, t) => is_entry_receiving_ty(t),
        Type_::Apply(_, sp!(_, n), targs)
            if n.is(&SUI_ADDR_VALUE, TRANSFER_MODULE_NAME, RECEIVING_TYPE_NAME) =>
        {
            debug_assert!(targs.len() == 1);
            // Don't care about the type parameter, just that it's a receiving type -- since it has
            // a `key` requirement on the type parameter it must be an object or type checking will
            // fail.
            true
        }
        _ => false,
    }
}

fn is_entry_primitive_ty(param_ty: &Type) -> bool {
    use BuiltinTypeName_ as B;
    use TypeName_ as N;

    match &param_ty.value {
        // A bit of a hack since no primitive has key
        Type_::Param(tp) => !tp.abilities.has_ability_(Ability_::Key),
        // nonsensical, but no error needed
        Type_::Apply(_, sp!(_, N::Multiple(_)), ts) => ts.iter().all(is_entry_primitive_ty),
        // Simple recursive cases
        Type_::Ref(_, t) => is_entry_primitive_ty(t),
        Type_::Apply(_, sp!(_, N::Builtin(sp!(_, B::Vector))), targs) => {
            debug_assert!(targs.len() == 1);
            is_entry_primitive_ty(&targs[0])
        }

        // custom "primitives"
        Type_::Apply(_, sp!(_, n), targs)
            if n.is(&STD_ADDR_VALUE, ASCII_MODULE_NAME, ASCII_TYPE_NAME)
                || n.is(&STD_ADDR_VALUE, UTF_MODULE_NAME, UTF_TYPE_NAME)
                || n.is(&SUI_ADDR_VALUE, OBJECT_MODULE_NAME, ID_TYPE_NAME) =>
        {
            debug_assert!(targs.is_empty());
            true
        }
        Type_::Apply(_, sp!(_, n), targs)
            if n.is(&STD_ADDR_VALUE, OPTION_MODULE_NAME, OPTION_TYPE_NAME) =>
        {
            debug_assert!(targs.len() == 1);
            is_entry_primitive_ty(&targs[0])
        }

        // primitives
        Type_::Apply(_, sp!(_, N::Builtin(_)), targs) => {
            debug_assert!(targs.is_empty());
            true
        }

        // Non primitive
        Type_::Apply(_, sp!(_, N::ModuleType(_, _)), _) => false,
        Type_::Unit => false,

        // Error case nothing to do
        Type_::UnresolvedError | Type_::Anything | Type_::Var(_) | Type_::Fun(_, _) => true,
    }
}

fn is_entry_object_ty(param_ty: &Type) -> bool {
    use BuiltinTypeName_ as B;
    use TypeName_ as N;
    match &param_ty.value {
        Type_::Ref(_, t) => is_entry_object_ty_inner(t),
        Type_::Apply(_, sp!(_, N::Builtin(sp!(_, B::Vector))), targs) => {
            debug_assert!(targs.len() == 1);
            is_entry_object_ty_inner(&targs[0])
        }
        _ => is_entry_object_ty_inner(param_ty),
    }
}

fn is_entry_object_ty_inner(param_ty: &Type) -> bool {
    use TypeName_ as N;
    match &param_ty.value {
        Type_::Param(tp) => tp.abilities.has_ability_(Ability_::Key),
        // nonsensical, but no error needed
        Type_::Apply(_, sp!(_, N::Multiple(_)), ts) => ts.iter().all(is_entry_object_ty_inner),
        // Simple recursive cases, shouldn't be hit but no need to error
        Type_::Ref(_, t) => is_entry_object_ty_inner(t),

        // Objects
        Type_::Apply(Some(abilities), _, _) => abilities.has_ability_(Ability_::Key),

        // Error case nothing to do
        Type_::UnresolvedError
        | Type_::Anything
        | Type_::Var(_)
        | Type_::Unit
        | Type_::Fun(_, _) => true,
        // Unreachable cases
        Type_::Apply(None, _, _) => unreachable!("ICE abilities should have been expanded"),
    }
}

fn entry_return(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    return_type @ sp!(tloc, return_type_): &Type,
) {
    match return_type_ {
        // unit is fine, nothing to do
        Type_::Unit => (),
        Type_::Ref(_, _) => {
            let fmsg = format!("Invalid return type for entry function '{}'", name);
            let tmsg = "Expected a non-reference type";
            context.add_diag(diag!(
                ENTRY_FUN_SIGNATURE_DIAG,
                (entry_loc, fmsg),
                (*tloc, tmsg)
            ))
        }
        Type_::Param(tp) => {
            if !tp.abilities.has_ability_(Ability_::Drop) {
                let declared_loc_opt = Some(tp.user_specified_name.loc);
                let declared_abilities = tp.abilities.clone();
                invalid_entry_return_ty(
                    context,
                    entry_loc,
                    name,
                    return_type,
                    declared_loc_opt,
                    &declared_abilities,
                    std::iter::empty(),
                )
            }
        }
        Type_::Apply(Some(abilities), sp!(_, tn_), ty_args) => {
            if !abilities.has_ability_(Ability_::Drop) {
                let (declared_loc_opt, declared_abilities) = match tn_ {
                    TypeName_::Multiple(_) => (None, AbilitySet::collection(*tloc)),
                    TypeName_::ModuleType(m, n) => (
                        Some(context.info.datatype_declared_loc(m, n)),
                        context.info.datatype_declared_abilities(m, n).clone(),
                    ),
                    TypeName_::Builtin(b) => (None, b.value.declared_abilities(b.loc)),
                };
                invalid_entry_return_ty(
                    context,
                    entry_loc,
                    name,
                    return_type,
                    declared_loc_opt,
                    &declared_abilities,
                    ty_args.iter().map(|ty_arg| (ty_arg, get_abilities(ty_arg))),
                )
            }
        }
        // Error case nothing to do
        Type_::UnresolvedError | Type_::Anything | Type_::Var(_) | Type_::Fun(_, _) => (),
        // Unreachable cases
        Type_::Apply(None, _, _) => unreachable!("ICE abilities should have been expanded"),
    }
}

fn get_abilities(sp!(loc, ty_): &Type) -> AbilitySet {
    ty_.abilities(*loc)
        .expect("ICE abilities should have been expanded")
}

fn invalid_entry_return_ty<'a>(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    ty: &Type,
    declared_loc_opt: Option<Loc>,
    declared_abilities: &AbilitySet,
    ty_args: impl IntoIterator<Item = (&'a Type, AbilitySet)>,
) {
    let fmsg = format!("Invalid return type for entry function '{}'", name);
    let mut diag = diag!(ENTRY_FUN_SIGNATURE_DIAG, (entry_loc, fmsg));
    ability_not_satisfied_tips(
        &Subst::empty(),
        &mut diag,
        Ability_::Drop,
        ty,
        declared_loc_opt,
        declared_abilities,
        ty_args,
    );
    context.add_diag(diag)
}

//**************************************************************************************************
// Expr
//**************************************************************************************************

fn exp(context: &mut Context, e: &T::Exp) {
    match &e.exp.value {
        T::UnannotatedExp_::ModuleCall(mcall) => {
            let T::ModuleCall { module, name, .. } = &**mcall;
            if !context.in_test && name.value() == symbol!("init") {
                let msg = format!(
                    "Invalid call to '{}::{}'. \
                    Module initializers cannot be called directly",
                    module, name
                );
                let mut diag = diag!(INIT_CALL_DIAG, (e.exp.loc, msg));
                diag.add_note(
                    "Module initializers are called implicitly upon publishing. \
                    If you need to reuse this function (or want to call it from a test), \
                    consider extracting the logic into a new function and \
                    calling that instead.",
                );
                context.add_diag(diag)
            }
            if module.value.is(&SUI_ADDR_VALUE, EVENT_MODULE_NAME)
                && name.value() == EVENT_FUNCTION_NAME
            {
                check_event_emit(context, e.exp.loc, mcall)
            }
            let is_transfer_module = module.value.is(&SUI_ADDR_VALUE, TRANSFER_MODULE_NAME);
            if is_transfer_module && PRIVATE_TRANSFER_FUNCTIONS.contains(&name.value()) {
                check_private_transfer(context, e.exp.loc, mcall)
            }
        }
        T::UnannotatedExp_::Pack(m, s, _, _) => {
            if !context.in_test
                && !otw_special_cases(context)
                && context.one_time_witness.as_ref().is_some_and(|otw| {
                    otw.as_ref()
                        .is_ok_and(|o| m == context.current_module() && o == s)
                })
            {
                let msg = "Invalid one-time witness construction. One-time witness types \
                    cannot be created manually, but are passed as an argument 'init'";
                let mut diag = diag!(OTW_USAGE_DIAG, (e.exp.loc, msg));
                diag.add_note(OTW_NOTE);
                context.add_diag(diag)
            }
        }
        _ => (),
    }
}

fn otw_special_cases(context: &Context) -> bool {
    BRIDGE_SUPPORTED_ASSET
        .iter()
        .any(|token| context.current_module().value.is(&BRIDGE_ADDR_VALUE, token))
        || context
            .current_module()
            .value
            .is(&SUI_ADDR_VALUE, SUI_MODULE_NAME)
}

fn check_event_emit(context: &mut Context, loc: Loc, mcall: &ModuleCall) {
    let current_module = context.current_module();
    let ModuleCall {
        module,
        name,
        type_arguments,
        ..
    } = mcall;
    let Some(first_ty) = type_arguments.first() else {
        // invalid arity
        debug_assert!(false, "ICE arity should have been expanded for errors");
        return;
    };
    let is_defined_in_current_module = matches!(first_ty.value.type_name(), Some(sp!(_, TypeName_::ModuleType(m, _))) if m == current_module);
    if !is_defined_in_current_module {
        let msg = format!(
            "Invalid event. The function '{}::{}' must be called with a type defined in the current module",
            module, name
        );
        let ty_msg = format!(
            "The type {} is not declared in the current module",
            error_format(first_ty, &Subst::empty()),
        );
        context.add_diag(diag!(
            EVENT_EMIT_CALL_DIAG,
            (loc, msg),
            (first_ty.loc, ty_msg)
        ));
    }
}

fn check_private_transfer(context: &mut Context, loc: Loc, mcall: &ModuleCall) {
    let ModuleCall {
        module,
        name,
        type_arguments,
        ..
    } = mcall;
    let current_module = context.current_module();
    if current_module
        .value
        .is(&SUI_ADDR_VALUE, TRANSFER_FUNCTION_NAME)
    {
        // inside the transfer module, so no private transfer rules
        return;
    }
    let Some(first_ty) = type_arguments.first() else {
        // invalid arity
        debug_assert!(false, "ICE arity should have been expanded for errors");
        return;
    };
    let (in_current_module, first_ty_tn) = match first_ty.value.type_name() {
        Some(sp!(_, TypeName_::Multiple(_))) | Some(sp!(_, TypeName_::Builtin(_))) | None => {
            (false, None)
        }
        Some(sp!(_, TypeName_::ModuleType(m, n))) => (m == current_module, Some((m, n))),
    };
    if !in_current_module {
        let mut msg = format!(
            "Invalid private transfer. \
            The function '{}::{}' is restricted to being called in the object's module",
            module, name,
        );
        if let Some((first_ty_module, _)) = &first_ty_tn {
            msg = format!("{}, '{}'", msg, first_ty_module);
        };
        let ty_msg = format!(
            "The type {} is not declared in the current module",
            error_format(first_ty, &Subst::empty()),
        );
        let mut diag = diag!(
            PRIVATE_TRANSFER_CALL_DIAG,
            (loc, msg),
            (first_ty.loc, ty_msg)
        );
        if first_ty
            .value
            .has_ability_(Ability_::Store)
            .is_some_and(|b| b)
        {
            let store_loc = if let Some((first_ty_module, first_ty_name)) = &first_ty_tn {
                let abilities = context
                    .info
                    .datatype_declared_abilities(first_ty_module, first_ty_name);
                abilities.ability_loc_(Ability_::Store).unwrap()
            } else {
                first_ty
                    .value
                    .abilities(first_ty.loc)
                    .expect("ICE abilities should have been expanded")
                    .ability_loc_(Ability_::Store)
                    .unwrap()
            };
            let store_msg = format!(
                "The object has '{}' so '{}::public_{}' can be called instead",
                Ability_::Store,
                module,
                name
            );
            diag.add_secondary_label((store_loc, store_msg))
        }
        context.add_diag(diag)
    }
}
