// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::ast as G,
    diag,
    diagnostics::{warning_filters::WarningFilters, Diagnostic, DiagnosticReporter, Diagnostics},
    expansion::ast::{Address, Attributes, ModuleIdent, ModuleIdent_},
    hlir::ast as HA,
    ice, ice_assert,
    naming::ast as NA,
    parser::ast::ConstantName,
    shared::{
        known_attributes::{self as KA, AttributeKind_, KnownAttribute, TestingAttribute},
        unique_map::UniqueMap,
        CompilationEnv, Identifier, NumericalAddress,
    },
    unit_test::{ExpectedMoveError, ModuleTestPlan, MoveErrorType, TestArgument, TestCase},
};
use move_core_types::{
    language_storage::{ModuleId, TypeTag},
    runtime_value::MoveValue,
};
use move_ir_types::location::{Loc, Spanned};
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

use super::ExpectedFailure;

struct Context<'env> {
    #[allow(unused)]
    env: &'env CompilationEnv,
    reporter: DiagnosticReporter<'env>,
    constants: UniqueMap<ModuleIdent, UniqueMap<ConstantName, (Loc, Option<u64>, Attributes)>>,
}

impl<'env> Context<'env> {
    fn new(compilation_env: &'env CompilationEnv, prog: &G::Program) -> Self {
        let constants = prog.modules.ref_map(|_mident, module| {
            module.constants.ref_map(|_name, constant| {
                let v_opt = constant.value.as_ref().and_then(|v| match v {
                    MoveValue::U64(u) => Some(*u),
                    _ => None,
                });
                (constant.loc, v_opt, constant.attributes.clone())
            })
        });
        let reporter = compilation_env.diagnostic_reporter_at_top_level();
        Self {
            env: compilation_env,
            reporter,
            constants,
        }
    }

    pub fn add_diag(&self, diag: Diagnostic) {
        self.reporter.add_diag(diag);
    }

    #[allow(unused)]
    pub fn add_diags(&self, diags: Diagnostics) {
        self.reporter.add_diags(diags);
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.reporter.push_warning_filter_scope(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.reporter.pop_warning_filter_scope()
    }

    fn resolve_address(&self, addr: &Address) -> NumericalAddress {
        (*addr).into_addr_bytes()
    }

    fn constants(
        &self,
    ) -> &UniqueMap<ModuleIdent, UniqueMap<ConstantName, (Loc, Option<u64>, Attributes)>> {
        &self.constants
    }
}

//***************************************************************************
// Test Plan Building
//***************************************************************************

// Constructs a test plan for each module in `prog`. This also validates the structure of the
// attributes as the test plan is constructed.
pub fn construct_test_plan(
    compilation_env: &CompilationEnv,
    package_filter: Option<Symbol>,
    prog: &G::Program,
) -> Option<Vec<ModuleTestPlan>> {
    if !compilation_env.flags().is_testing() {
        return None;
    }

    let mut context = Context::new(compilation_env, prog);
    Some(
        prog.modules
            .key_cloned_iter()
            .flat_map(|(module_ident, module_def)| {
                context.push_warning_filter_scope(module_def.warning_filter);
                let plan = construct_module_test_plan(
                    &mut context,
                    package_filter,
                    module_ident,
                    module_def,
                );
                context.pop_warning_filter_scope();
                plan
            })
            .collect(),
    )
}

fn construct_module_test_plan(
    context: &mut Context,
    package_filter: Option<Symbol>,
    module_ident: ModuleIdent,
    module: &G::ModuleDefinition,
) -> Option<ModuleTestPlan> {
    if package_filter.is_some() && module.package_name != package_filter {
        return None;
    }
    let tests: BTreeMap<_, _> = module
        .functions
        .iter()
        .filter_map(|(loc, fn_name, func)| {
            context.push_warning_filter_scope(func.warning_filter);
            let info = build_test_info(context, loc, fn_name, func)
                .map(|test_case| (fn_name.to_string(), test_case));
            context.pop_warning_filter_scope();
            info
        })
        .collect();

    if tests.is_empty() {
        None
    } else {
        let sp!(_, ModuleIdent_ { address, module }) = &module_ident;
        let addr_bytes = context.resolve_address(address);
        Some(ModuleTestPlan::new(&addr_bytes, &module.0.value, tests))
    }
}

fn build_test_info<'func>(
    context: &mut Context,
    fn_loc: Loc,
    fn_name: &str,
    function: &'func G::Function,
) -> Option<TestCase> {
    let get_attrs = |attr: AttributeKind_| -> Option<&'func Spanned<KnownAttribute>> {
        function.attributes.get_(&attr)
    };

    const IN_THIS_TEST_MSG: &str = "Error found in this test";

    let test_attribute_opt = get_attrs(AttributeKind_::Test);
    let random_test_attribute_opt = get_attrs(AttributeKind_::RandTest);
    let expected_failure_attribute_opt = get_attrs(AttributeKind_::ExpectedFailure);
    let test_only_attribute_opt = get_attrs(AttributeKind_::TestOnly);

    let (test_attribute, is_random_test) = if let Some(test_attribute) = test_attribute_opt {
        ice_assert!(
            context.reporter,
            random_test_attribute_opt.is_none(),
            fn_loc,
            "Found test and rand_test attributes"
        );
        (test_attribute, false)
    } else if let Some(rand_test_attribute) = random_test_attribute_opt {
        (rand_test_attribute, true)
    } else {
        // expected failures cannot be annotated on non-#[test] functions
        if let Some(abort_attribute) = expected_failure_attribute_opt {
            let fn_msg = "Only functions defined as a test with #[test] can also have an \
                              #[expected_failure] attribute";
            let abort_msg = "Attributed as #[expected_failure] here";
            context.add_diag(diag!(
                Attributes::InvalidUsage,
                (fn_loc, fn_msg),
                (abort_attribute.loc, abort_msg),
            ))
        }
        return None;
    };

    // A #[test] function cannot also be annotated #[test_only]
    if test_only_attribute_opt.is_some() {
        ice_assert!(
            context.reporter,
            false,
            fn_loc,
            "Found test_only and test or rand_test attributes"
        );
        return None;
    }

    let mut arguments = Vec::new();
    if is_random_test {
        for (_mut, var, s_type) in &function.signature.parameters {
            let sp!(_, _) = var.0;
            let generated_type = match convert_builtin_type_to_typetag(&s_type.value) {
                Some(generated_type) => generated_type,
                None => {
                    let msg = "Unsupported type for generated input for test. Only built-in types \
                            are supported for generated test inputs";
                    let mut diag = diag!(
                        Attributes::InvalidTest,
                        (s_type.loc, msg),
                        (fn_loc, IN_THIS_TEST_MSG),
                    );
                    diag.add_note(
                        "Supported builti-in types are: bool, u8, u16, u32, u64, \
                            u128, u256, address, and vector<T> where T is a built-in type",
                    );
                    context.add_diag(diag);
                    return None;
                }
            };
            arguments.push(TestArgument::Generate { generated_type })
        }
        if arguments.is_empty() {
            let msg = "No parameters to generate for random test. A #[random_test] function must \
                       have at least one parameter to generate.";
            context.add_diag(diag!(
                Attributes::InvalidTest,
                (test_attribute.loc, msg),
                (fn_loc, IN_THIS_TEST_MSG),
            ));
            return None;
        }
    } else if !function.signature.parameters.is_empty() {
        let mut diag = diag!(
            Attributes::ValueWarning,
            (function.loc, "Invalid test function")
        );
        diag.add_note("Test functions with arguments have been deprecated");
        diag.add_note("If you would like to test functions with random inputs, consider using '#[rand_test]' instead");
        context.add_diag(diag);
        return None;
    }
    let expected_failure =
        expected_failure_attribute_opt.and_then(|ef| lower_expected_failure(context, ef));

    Some(TestCase {
        test_name: fn_name.to_string(),
        arguments,
        expected_failure,
    })
}

//***************************************************************************
// Attribute parsers
//***************************************************************************

const INVALID_VALUE: &str = "Invalid value in attribute assignment";

fn lower_expected_failure(
    context: &mut Context,
    sp!(loc, attribute): &Spanned<KnownAttribute>,
) -> Option<ExpectedFailure> {
    use KA::ExpectedFailure as EF;
    let KnownAttribute::Testing(TestingAttribute::ExpectedFailure(failure)) = attribute else {
        let attr = TestingAttribute::EXPECTED_FAILURE;
        context.add_diag(ice!((
            *loc,
            format!("Expected {attr} attribute based on kind")
        )));
        return None;
    };
    match &**failure {
        EF::Expected => Some(ExpectedFailure::Expected),
        EF::ExpectedWithCodeDEPRECATED(code) => Some(ExpectedFailure::ExpectedWithCodeDEPRECATED(
            MoveErrorType::Code(*code),
        )),
        EF::ExpectedWithError {
            status_code,
            minor_code,
            location,
        } => {
            let sub_status_code = minor_code
                .as_ref()
                .and_then(|value| convert_minor_code_to_sub_status_code(context, value));
            let location =
                move_binary_format::errors::Location::Module(convert_module_id(context, location)?);
            Some(ExpectedFailure::ExpectedWithError(ExpectedMoveError(
                *status_code,
                sub_status_code,
                location,
            )))
        }
    }
}

fn convert_module_id(context: &mut Context, module: &ModuleIdent) -> Option<ModuleId> {
    if !context.constants.contains_key(module) {
        context.add_diag(diag!(
            NameResolution::UnboundModule,
            (module.loc, format!("Unbound module '{module}'")),
        ));
        return None;
    }
    let sp!(mloc, ModuleIdent_ { address, module }) = module;
    let addr = match address {
        Address::Numerical {
            value: sp!(_, a), ..
        } => a.into_inner(),
        Address::NamedUnassigned(addr) => {
            context.add_diag(diag!(
                NameResolution::AddressWithoutValue,
                (*mloc, format!("Unbound address '{addr}'")),
            ));
            return None;
        }
    };
    let mname = move_core_types::identifier::Identifier::new(module.value().to_string()).unwrap();
    let mid = ModuleId::new(addr, mname);
    Some(mid)
}

fn convert_minor_code_to_sub_status_code(
    context: &mut Context,
    value: &KA::MinorCode,
) -> Option<MoveErrorType> {
    match value {
        sp!(_, KA::MinorCode_::Value(value)) => Some(MoveErrorType::Code(*value)),
        sp!(loc, KA::MinorCode_::Constant(module, member)) => {
            let Some(module_constants) = context.constants().get(module) else {
                // NB: Name resolution _should_ have already complained about this.
                debug_assert!(context.env.has_errors());
                return None;
            };
            let Some(constant) = module_constants.get_(&member.value) else {
                context.add_diag(diag!(
                    Attributes::InvalidValue,
                    (*loc, INVALID_VALUE),
                    (
                        module.loc,
                        format!("Unbound constant '{member}' in module '{module}'")
                    ),
                ));
                return None;
            };
            match constant {
                (_, None, attrs) if attrs.contains_key_(&KA::AttributeKind_::Error) => {
                    Some(MoveErrorType::ConstantName(member.value.to_string()))
                }
                (cloc, None, _) => {
                    let msg = format!(
                        "Constant '{module}::{member}' has a non-u64 value. \
                        Only 'u64' values are permitted"
                    );
                    context.add_diag(diag!(
                        Attributes::InvalidValue,
                        (*loc, INVALID_VALUE),
                        (*cloc, msg),
                    ));
                    None
                }
                (_, Some(u), _) => Some(MoveErrorType::Code(*u)),
            }
        }
    }
}

fn convert_builtin_type_to_typetag(s_type: &HA::SingleType_) -> Option<TypeTag> {
    fn get_builtin_type_inner(bt: &HA::BaseType) -> Option<TypeTag> {
        match &bt.value {
            HA::BaseType_::Apply(_, sp!(_, HA::TypeName_::Builtin(b)), bts) => {
                let mut tts = bts
                    .iter()
                    .map(get_builtin_type_inner)
                    .collect::<Option<Vec<_>>>()?;
                let tag = match b.value {
                    NA::BuiltinTypeName_::Bool => TypeTag::Bool,
                    NA::BuiltinTypeName_::Address => TypeTag::Address,
                    NA::BuiltinTypeName_::U8 => TypeTag::U8,
                    NA::BuiltinTypeName_::U64 => TypeTag::U64,
                    NA::BuiltinTypeName_::U128 => TypeTag::U128,
                    NA::BuiltinTypeName_::U256 => TypeTag::U256,
                    NA::BuiltinTypeName_::U16 => TypeTag::U16,
                    NA::BuiltinTypeName_::U32 => TypeTag::U32,
                    NA::BuiltinTypeName_::Vector => {
                        if tts.len() != 1 {
                            return None;
                        }
                        TypeTag::Vector(Box::new(tts.remove(0)))
                    }
                    NA::BuiltinTypeName_::Signer => TypeTag::Signer,
                };
                Some(tag)
            }
            HA::BaseType_::Apply(_, _, _) => None,
            HA::BaseType_::Param(_)
            | HA::BaseType_::Unreachable
            | HA::BaseType_::UnresolvedError => None,
        }
    }
    match s_type {
        HA::SingleType_::Base(bt) => get_builtin_type_inner(bt),
        _ => None,
    }
}
