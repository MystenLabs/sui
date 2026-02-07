// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use crate::{
    diag,
    diagnostics::{Diagnostic, DiagnosticReporter, Diagnostics, warning_filters::WarningFilters},
    editions::Flavor,
    expansion::ast::{Fields, ModuleIdent, Visibility},
    naming::ast::{
        self as N, BuiltinTypeName_, FunctionSignature, StructFields, Type, Type_, TypeInner as TI,
        TypeName_, UNIT_TYPE,
    },
    parser::ast::{Ability_, DatatypeName, DocComment, FunctionName, TargetKind},
    shared::{CompilationEnv, Identifier, program_info::TypingProgramInfo},
    sui_mode::*,
    typing::{
        ast::{self as T, ModuleCall},
        core::{Subst, error_format, error_format_},
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

impl TypingVisitorContext for Context<'_> {
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
        signature: _,
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
    if !matches!(return_type.value.inner(), TI::Unit) {
        let msg = format!(
            "'init' functions must have a return type of {}",
            error_format_(&UNIT_TYPE.clone(), &Subst::empty())
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
        .unwrap_or(Some(TxContextKind::None));
    if matches!(
        tx_ctx_kind,
        Some(TxContextKind::None | TxContextKind::Owned)
    ) {
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
        && matches!(
            tx_ctx_kind,
            Some(TxContextKind::Mutable | TxContextKind::Immutable)
        )
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
        let is_otw = matches!(&first_ty.value.inner(), TI::UnresolvedError | TI::Var(_))
            || matches!(
                first_ty.value.type_name(),
                Some(sp!(_, TypeName_::ModuleType(m, n)))
                    if m.as_ref() == context.current_module() && n.value() == otw_name
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
// well known type helpers
//**************************************************************************************************

pub fn tx_context_kind(sp!(_, param_ty): &Type) -> Option<TxContextKind> {
    let (ref_kind, inner_name) = match param_ty.inner() {
        TI::Ref(is_mut, inner_ty) => match &inner_ty.value.inner() {
            TI::Apply(_, sp!(_, inner_name), _) => (Some(*is_mut), inner_name),
            // Unknown type resulting from a previous error
            TI::UnresolvedError | TI::Var(_) => return None,
            // not a user defined type
            _ => return Some(TxContextKind::None),
        },
        TI::Apply(_, sp!(_, inner_name), _) => (None, inner_name),
        // Unknown type resulting from a previous error
        TI::UnresolvedError | TI::Var(_) => return None,
        // not a reference or user defined type
        _ => return Some(TxContextKind::None),
    };
    let kind = if inner_name.is(
        &SUI_ADDR_VALUE,
        TX_CONTEXT_MODULE_NAME,
        TX_CONTEXT_TYPE_NAME,
    ) {
        match ref_kind {
            None => TxContextKind::Owned,
            Some(true) => TxContextKind::Mutable,
            Some(false) => TxContextKind::Immutable,
        }
    } else {
        // not the tx context
        TxContextKind::None
    };
    Some(kind)
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TxContextKind {
    // No TxContext
    None,
    // Invalid but TxContext
    Owned,
    // &mut TxContext
    Mutable,
    // &TxContext
    Immutable,
}

pub fn is_mut_clock(param_ty: &Type) -> bool {
    match &param_ty.value.inner() {
        TI::Ref(/* mut */ false, _) => false,
        TI::Ref(/* mut */ true, t) => is_mut_clock(t),
        TI::Apply(_, sp!(_, n_), _) => n_.is(&SUI_ADDR_VALUE, CLOCK_MODULE_NAME, CLOCK_TYPE_NAME),
        TI::Unit
        | TI::Param(_)
        | TI::Var(_)
        | TI::Anything
        | TI::Void
        | TI::UnresolvedError
        | TI::Fun(_, _) => false,
    }
}

pub fn is_mut_random(param_ty: &Type) -> bool {
    match &param_ty.value.inner() {
        TI::Ref(/* mut */ false, _) => false,
        TI::Ref(/* mut */ true, t) => is_mut_random(t),
        TI::Apply(_, sp!(_, n_), _) => n_.is(
            &SUI_ADDR_VALUE,
            RANDOMNESS_MODULE_NAME,
            RANDOMNESS_STATE_TYPE_NAME,
        ),
        TI::Unit
        | TI::Param(_)
        | TI::Var(_)
        | TI::Anything
        | TI::Void
        | TI::UnresolvedError
        | TI::Fun(_, _) => false,
    }
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
                && (name.value() == EVENT_FUNCTION_NAME
                    || name.value() == EVENT_AUTHENTICATED_FUNCTION_NAME)
            {
                check_event_emit(context, e.exp.loc, mcall)
            }

            if module.value.is(&SUI_ADDR_VALUE, COIN_REGISTRY_MODULE_NAME)
                && name.value() == DYNAMIC_COIN_CREATION_FUNCTION_NAME
            {
                check_dynamic_coin_creation(context, e.exp.loc, mcall)
            }

            let is_transfer_module = module.value.is(&SUI_ADDR_VALUE, TRANSFER_MODULE_NAME);
            if is_transfer_module && PRIVATE_TRANSFER_FUNCTIONS.contains(&name.value()) {
                check_private_transfer(context, e.exp.loc, mcall)
            }

            if module.value.is(&STD_ADDR_VALUE, INTERNAL_MODULE_NAME)
                && name.value() == INTERNAL_PERMIT_FUNCTION_NAME
            {
                check_internal_permit(context, e.exp.loc, mcall)
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
    let is_defined_in_current_module = matches!(first_ty.value.type_name(), Some(sp!(_, TypeName_::ModuleType(m, _))) if m.as_ref() == current_module);
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

fn check_dynamic_coin_creation(context: &mut Context, loc: Loc, mcall: &ModuleCall) {
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
    let is_defined_in_current_module = matches!(first_ty.value.type_name(), Some(sp!(_, TypeName_::ModuleType(m, _))) if m.as_ref() == current_module);
    if !is_defined_in_current_module {
        let msg = format!(
            "Invalid coin creation. The function '{}::{}' must be called with a type defined in the current module",
            module, name
        );
        let ty_msg = format!(
            "The type {} is not declared in the current module",
            error_format(first_ty, &Subst::empty()),
        );
        context.add_diag(diag!(
            DYNAMIC_COIN_CREATION_CALL_DIAG,
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
        Some(sp!(_, TypeName_::ModuleType(m, n))) => (m.as_ref() == current_module, Some((m, n))),
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

fn check_internal_permit(context: &mut Context, loc: Loc, mcall: &ModuleCall) {
    let ModuleCall {
        module,
        name,
        type_arguments,
        ..
    } = mcall;
    let current_module = context.current_module();
    let Some(first_ty) = type_arguments.first() else {
        // invalid arity
        debug_assert!(false, "ICE arity should have been expanded for errors");
        return;
    };
    let (in_current_module, first_ty_tn) = match first_ty.value.type_name() {
        Some(sp!(_, TypeName_::Multiple(_))) | Some(sp!(_, TypeName_::Builtin(_))) | None => {
            (false, None)
        }
        Some(sp!(_, TypeName_::ModuleType(m, n))) => (m.as_ref() == current_module, Some((m, n))),
    };
    if !in_current_module {
        let mut msg = format!(
            "Invalid call to an internal function. \
            The function '{}::{}' is restricted to being called in the module that defines the type",
            module, name,
        );
        if let Some((first_ty_module, _)) = &first_ty_tn {
            msg = format!("{}, '{}'", msg, first_ty_module);
        };
        let ty_msg = format!(
            "The type {} is not declared in the current module",
            error_format(first_ty, &Subst::empty()),
        );
        let diag = diag!(
            INTERNAL_PERMIT_CALL_DIAG,
            (loc, msg),
            (first_ty.loc, ty_msg)
        );
        context.add_diag(diag)
    }
}
