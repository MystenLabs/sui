// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::ast as G,
    diag,
    expansion::ast::{
        self as E, Address, Attribute, AttributeValue, ModuleAccess_, ModuleIdent, ModuleIdent_,
    },
    hlir::translate::display_var,
    parser::ast::ConstantName,
    shared::{
        known_attributes::{KnownAttribute, TestingAttribute},
        unique_map::UniqueMap,
        CompilationEnv, Identifier, NumericalAddress,
    },
    unit_test::{ExpectedFailure, ExpectedMoveError, ModuleTestPlan, TestCase},
};
use move_core_types::{
    account_address::AccountAddress as MoveAddress, language_storage::ModuleId, u256::U256,
    value::MoveValue, vm_status::StatusCode,
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

struct Context<'env> {
    env: &'env mut CompilationEnv,
    constants: UniqueMap<ModuleIdent, UniqueMap<ConstantName, (Loc, Option<u64>)>>,
}

impl<'env> Context<'env> {
    fn new(compilation_env: &'env mut CompilationEnv, prog: &G::Program) -> Self {
        let constants = prog.modules.ref_map(|_mident, module| {
            module.constants.ref_map(|_name, constant| {
                let v_opt = constant.value.as_ref().and_then(|v| match v {
                    MoveValue::U64(u) => Some(*u),
                    _ => None,
                });
                (constant.loc, v_opt)
            })
        });
        Self {
            env: compilation_env,
            constants,
        }
    }

    fn resolve_address(&self, addr: &Address) -> NumericalAddress {
        (*addr).into_addr_bytes()
    }

    fn constants(&self) -> &UniqueMap<ModuleIdent, UniqueMap<ConstantName, (Loc, Option<u64>)>> {
        &self.constants
    }
}

//***************************************************************************
// Test Plan Building
//***************************************************************************

// Constructs a test plan for each module in `prog`. This also validates the structure of the
// attributes as the test plan is constructed.
pub fn construct_test_plan(
    compilation_env: &mut CompilationEnv,
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
                construct_module_test_plan(&mut context, package_filter, module_ident, module_def)
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
            build_test_info(context, loc, fn_name, func)
                .map(|test_case| (fn_name.to_string(), test_case))
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
    let get_attrs = |attr: TestingAttribute| -> Option<&'func E::Attribute> {
        function
            .attributes
            .get_(&E::AttributeName_::Known(KnownAttribute::Testing(attr)))
    };

    const PREVIOUSLY_ANNOTATED_MSG: &str = "Previously annotated here";
    const IN_THIS_TEST_MSG: &str = "Error found in this test";

    let test_attribute_opt = get_attrs(TestingAttribute::Test);
    let abort_attribute_opt = get_attrs(TestingAttribute::ExpectedFailure);
    let test_only_attribute_opt = get_attrs(TestingAttribute::TestOnly);

    let test_attribute = match test_attribute_opt {
        None => {
            // expected failures cannot be annotated on non-#[test] functions
            if let Some(abort_attribute) = abort_attribute_opt {
                let fn_msg = "Only functions defined as a test with #[test] can also have an \
                              #[expected_failure] attribute";
                let abort_msg = "Attributed as #[expected_failure] here";
                context.env.add_diag(diag!(
                    Attributes::InvalidUsage,
                    (fn_loc, fn_msg),
                    (abort_attribute.loc, abort_msg),
                ))
            }
            return None;
        }
        Some(test_attribute) => test_attribute,
    };

    // A #[test] function cannot also be annotated #[test_only]
    if let Some(test_only_attribute) = test_only_attribute_opt {
        let msg = "Function annotated as both #[test(...)] and #[test_only]. You need to declare \
                   it as either one or the other";
        context.env.add_diag(diag!(
            Attributes::InvalidUsage,
            (test_only_attribute.loc, msg),
            (test_attribute.loc, PREVIOUSLY_ANNOTATED_MSG),
            (fn_loc, IN_THIS_TEST_MSG),
        ))
    }

    let test_annotation_params = parse_test_attribute(context, test_attribute, 0);
    let mut arguments = Vec::new();
    for (var, _) in &function.signature.parameters {
        let sp!(vloc, var_) = var.0;
        let var_ = match display_var(var_) {
            crate::hlir::translate::DisplayVar::Orig(s) => s.into(),
            crate::hlir::translate::DisplayVar::Tmp => panic!("ICE temp as parameter"),
        };
        match test_annotation_params.get(&var_) {
            Some(value) => arguments.push(value.clone()),
            None => {
                let missing_param_msg = "Missing test parameter assignment in test. Expected a \
                                         parameter to be assigned in this attribute";
                context.env.add_diag(diag!(
                    Attributes::InvalidTest,
                    (test_attribute.loc, missing_param_msg),
                    (vloc, "Corresponding to this parameter"),
                    (fn_loc, IN_THIS_TEST_MSG),
                ))
            }
        }
    }

    let expected_failure = match abort_attribute_opt {
        None => None,
        Some(abort_attribute) => parse_failure_attribute(context, abort_attribute),
    };

    Some(TestCase {
        test_name: fn_name.to_string(),
        arguments,
        expected_failure,
    })
}

//***************************************************************************
// Attribute parsers
//***************************************************************************

fn parse_test_attribute(
    context: &mut Context,
    sp!(aloc, test_attribute): &E::Attribute,
    depth: usize,
) -> BTreeMap<Symbol, MoveValue> {
    use E::Attribute_ as EA;

    match test_attribute {
        EA::Name(_) | EA::Parameterized(_, _) if depth > 0 => {
            context.env.add_diag(diag!(
                Attributes::InvalidTest,
                (*aloc, "Unexpected nested attribute in test declaration"),
            ));
            BTreeMap::new()
        }
        EA::Name(nm) => {
            assert!(
                nm.value.as_str() == TestingAttribute::Test.name() && depth == 0,
                "ICE: We should only be parsing a raw test attribute"
            );
            BTreeMap::new()
        }
        EA::Assigned(nm, attr_value) => {
            if depth != 1 {
                context.env.add_diag(diag!(
                    Attributes::InvalidTest,
                    (*aloc, "Unexpected nested attribute in test declaration"),
                ));
                return BTreeMap::new();
            }
            let sp!(assign_loc, attr_value) = &**attr_value;
            let value = match convert_attribute_value_to_move_value(context, attr_value) {
                Some(move_value) => move_value,
                None => {
                    context.env.add_diag(diag!(
                        Attributes::InvalidValue,
                        (*assign_loc, "Unsupported attribute value"),
                        (*aloc, "Assigned in this attribute"),
                    ));
                    return BTreeMap::new();
                }
            };

            let mut args = BTreeMap::new();
            args.insert(nm.value, value);
            args
        }
        EA::Parameterized(nm, attributes) => {
            assert!(
                nm.value.as_str() == TestingAttribute::Test.name() && depth == 0,
                "ICE: We should only be parsing a raw test attribute"
            );
            attributes
                .iter()
                .flat_map(|(_, _, attr)| parse_test_attribute(context, attr, depth + 1))
                .collect()
        }
    }
}

const BAD_ABORT_VALUE_WARNING: &str = "WARNING: passes for an abort from any module.";
const INVALID_VALUE: &str = "Invalid value in attribute assignment";

fn parse_failure_attribute(
    context: &mut Context,
    sp!(aloc, expected_attr): &E::Attribute,
) -> Option<ExpectedFailure> {
    use E::Attribute_ as EA;
    match expected_attr {
        EA::Name(nm) => {
            assert!(
                nm.value.as_str() == TestingAttribute::ExpectedFailure.name(),
                "ICE: We should only be parsing a raw expected failure attribute"
            );
            Some(ExpectedFailure::Expected)
        }
        EA::Assigned(_, value) => {
            let assign_loc = value.loc;
            let invalid_assignment_msg = "Invalid expected failure code assignment";
            let expected_msg =
                "Expect an #[expected_failure(...)] attribute for error specification";
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (assign_loc, invalid_assignment_msg),
                (*aloc, expected_msg),
            ));
            None
        }
        EA::Parameterized(sp!(_, nm), attrs) => {
            assert!(
                nm.as_str() == TestingAttribute::ExpectedFailure.name(),
                "ICE: expected failure attribute must have the right name"
            );
            let mut attrs: BTreeMap<String, (Loc, Attribute)> = attrs
                .key_cloned_iter()
                .map(|(sp!(kloc, k_), v)| (k_.to_string(), (kloc, v.clone())))
                .collect();
            let mut expected_failure_kind_vec = TestingAttribute::expected_failure_cases()
                .iter()
                .filter_map(|k| {
                    let k = k.to_string();
                    let attr_opt = attrs.remove(&k)?;
                    Some((k, attr_opt))
                })
                .collect::<Vec<_>>();
            if expected_failure_kind_vec.len() != 1 {
                let invalid_attr_msg = format!(
                    "Invalid #[expected_failure(...)] attribute, expected 1 failure kind but found {}. Expected one of: {}",
                    expected_failure_kind_vec.len(),
                    TestingAttribute::expected_failure_cases().to_vec().join(", ")
                );
                context
                    .env
                    .add_diag(diag!(Attributes::InvalidValue, (*aloc, invalid_attr_msg)));
                return None;
            }
            let (expected_failure_kind, (attr_loc, attr)) =
                expected_failure_kind_vec.pop().unwrap();
            let location_opt = attrs.remove(TestingAttribute::ERROR_LOCATION);
            let (status_code, sub_status_code, location) = match expected_failure_kind.as_str() {
                TestingAttribute::ABORT_CODE_NAME => {
                    let (value_name_loc, attr_value) = get_assigned_attribute(
                        context,
                        TestingAttribute::ABORT_CODE_NAME,
                        attr_loc,
                        attr,
                    )?;
                    let (value_loc, const_location_opt, u) =
                        convert_constant_value_u64_constant_or_value(
                            context,
                            value_name_loc,
                            &attr_value,
                        )?;
                    let location = if let Some((location_loc, location_attr)) = location_opt {
                        convert_location(context, location_loc, location_attr)?
                    } else if let Some(location) = const_location_opt {
                        location
                    } else {
                        let tip = format!(
                            "Replace value with constant from expected module or add `{}=...` \
                            attribute.",
                            TestingAttribute::ERROR_LOCATION
                        );
                        context.env.add_diag(diag!(
                            Attributes::ValueWarning,
                            (attr_loc, BAD_ABORT_VALUE_WARNING),
                            (value_loc, tip)
                        ));
                        return Some(ExpectedFailure::ExpectedWithCodeDEPRECATED(u));
                    };
                    (StatusCode::ABORTED, Some(u), location)
                }
                TestingAttribute::ARITHMETIC_ERROR_NAME => {
                    check_attribute_unassigned(
                        context,
                        TestingAttribute::ARITHMETIC_ERROR_NAME,
                        attr_loc,
                        attr,
                    )?;
                    let (location_loc, location_attr) = check_location(
                        context,
                        attr_loc,
                        TestingAttribute::ARITHMETIC_ERROR_NAME,
                        location_opt,
                    )?;
                    let location = convert_location(context, location_loc, location_attr)?;
                    (StatusCode::ARITHMETIC_ERROR, None, location)
                }
                TestingAttribute::OUT_OF_GAS_NAME => {
                    check_attribute_unassigned(
                        context,
                        TestingAttribute::OUT_OF_GAS_NAME,
                        attr_loc,
                        attr,
                    )?;
                    let (location_loc, location_attr) = check_location(
                        context,
                        attr_loc,
                        TestingAttribute::OUT_OF_GAS_NAME,
                        location_opt,
                    )?;
                    let location = convert_location(context, location_loc, location_attr)?;
                    (StatusCode::OUT_OF_GAS, None, location)
                }
                TestingAttribute::VECTOR_ERROR_NAME => {
                    check_attribute_unassigned(
                        context,
                        TestingAttribute::VECTOR_ERROR_NAME,
                        attr_loc,
                        attr,
                    )?;
                    let minor_attr_opt = attrs.remove(TestingAttribute::MINOR_STATUS_NAME);
                    let minor_status = if let Some((minor_loc, minor_attr)) = minor_attr_opt {
                        let (minor_value_loc, minor_value) = get_assigned_attribute(
                            context,
                            TestingAttribute::MINOR_STATUS_NAME,
                            minor_loc,
                            minor_attr,
                        )?;
                        let (_, _, minor_status) = convert_constant_value_u64_constant_or_value(
                            context,
                            minor_value_loc,
                            &minor_value,
                        )?;
                        Some(minor_status)
                    } else {
                        None
                    };
                    let (location_loc, location_attr) = check_location(
                        context,
                        attr_loc,
                        TestingAttribute::VECTOR_ERROR_NAME,
                        location_opt,
                    )?;
                    let location = convert_location(context, location_loc, location_attr)?;
                    (StatusCode::VECTOR_OPERATION_ERROR, minor_status, location)
                }
                TestingAttribute::MAJOR_STATUS_NAME => {
                    let (value_name_loc, attr_value) = get_assigned_attribute(
                        context,
                        TestingAttribute::MAJOR_STATUS_NAME,
                        attr_loc,
                        attr,
                    )?;
                    let (major_value_loc, _, major_status_u64) =
                        convert_constant_value_u64_constant_or_value(
                            context,
                            value_name_loc,
                            &attr_value,
                        )?;
                    let major_status = if let Ok(c) = StatusCode::try_from(major_status_u64) {
                        c
                    } else {
                        let bad_value = format!(
                            "Invalid value for '{}'",
                            TestingAttribute::MAJOR_STATUS_NAME,
                        );
                        let no_code =
                            format!("No status code associated with value '{major_status_u64}'");
                        context.env.add_diag(diag!(
                            Attributes::InvalidValue,
                            (value_name_loc, bad_value),
                            (major_value_loc, no_code)
                        ));
                        return None;
                    };
                    let minor_attr_opt = attrs.remove(TestingAttribute::MINOR_STATUS_NAME);
                    let minor_status = if let Some((minor_loc, minor_attr)) = minor_attr_opt {
                        let (minor_value_loc, minor_value) = get_assigned_attribute(
                            context,
                            TestingAttribute::MINOR_STATUS_NAME,
                            minor_loc,
                            minor_attr,
                        )?;
                        let (_, _, minor_status) = convert_constant_value_u64_constant_or_value(
                            context,
                            minor_value_loc,
                            &minor_value,
                        )?;
                        Some(minor_status)
                    } else {
                        None
                    };
                    let (location_loc, location_attr) = check_location(
                        context,
                        attr_loc,
                        TestingAttribute::MAJOR_STATUS_NAME,
                        location_opt,
                    )?;
                    let location = convert_location(context, location_loc, location_attr)?;
                    (major_status, minor_status, location)
                }
                _ => unreachable!(),
            };
            // warn for any remaining attrs
            for (_, (loc, _)) in attrs {
                let msg = format!(
                    "Unused attribute for {}",
                    TestingAttribute::ExpectedFailure.name()
                );
                context
                    .env
                    .add_diag(diag!(UnusedItem::Attribute, (loc, msg)));
            }
            Some(ExpectedFailure::ExpectedWithError(ExpectedMoveError(
                status_code,
                sub_status_code,
                move_binary_format::errors::Location::Module(location),
            )))
        }
    }
}

fn check_attribute_unassigned(
    context: &mut Context,
    kind: &str,
    attr_loc: Loc,
    attr: Attribute,
) -> Option<()> {
    use E::Attribute_ as EA;
    match attr {
        sp!(_, EA::Name(sp!(_, nm))) => {
            assert!(nm.as_str() == kind);
            Some(())
        }
        sp!(loc, _) => {
            let msg = format!(
                "Expected no assigned value, e.g. '{}', for expected failure attribute",
                kind
            );
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (attr_loc, "Unsupported attribute in this location"),
                (loc, msg)
            ));
            None
        }
    }
}

fn get_assigned_attribute(
    context: &mut Context,
    kind: &str,
    attr_loc: Loc,
    attr: Attribute,
) -> Option<(Loc, AttributeValue)> {
    use E::Attribute_ as EA;
    match attr {
        sp!(assign_loc, EA::Assigned(sp!(_, nm), value)) => {
            assert!(nm.as_str() == kind);
            Some((assign_loc, *value))
        }
        sp!(loc, _) => {
            let msg = format!(
                "Expected assigned value, e.g. '{}=...', for expected failure attribute",
                kind
            );
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (attr_loc, "Unsupported attribute in this location"),
                (loc, msg)
            ));
            None
        }
    }
}

fn convert_location(context: &mut Context, attr_loc: Loc, attr: Attribute) -> Option<ModuleId> {
    use E::AttributeValue_ as EAV;
    let (loc, value) =
        get_assigned_attribute(context, TestingAttribute::ERROR_LOCATION, attr_loc, attr)?;
    match value {
        sp!(vloc, EAV::Module(module)) => convert_module_id(context, vloc, &module),
        sp!(vloc, _) => {
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (loc, INVALID_VALUE),
                (vloc, "Expected a module identifier, e.g. 'std::vector'")
            ));
            None
        }
    }
}

fn convert_constant_value_u64_constant_or_value(
    context: &mut Context,
    loc: Loc,
    value: &AttributeValue,
) -> Option<(Loc, Option<ModuleId>, u64)> {
    use E::AttributeValue_ as EAV;
    let (vloc, module, member) = match value {
        sp!(
            vloc,
            EAV::ModuleAccess(sp!(_, ModuleAccess_::ModuleAccess(m, n)))
        ) => (*vloc, m, n),
        _ => {
            let (vloc, u) = convert_attribute_value_u64(context, loc, value)?;
            return Some((vloc, None, u));
        }
    };
    let module_id = convert_module_id(context, vloc, module)?;
    let modules_constants = context.constants().get(module).unwrap();
    let constant = match modules_constants.get_(&member.value) {
        None => {
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (vloc, INVALID_VALUE),
                (
                    module.loc,
                    format!("Unbound constant '{member}' in module '{module}'")
                ),
            ));
            return None;
        }
        Some(c) => c,
    };
    match constant {
        (cloc, None) => {
            let msg = format!(
                "Constant '{module}::{member}' has a non-u64 value. \
                Only 'u64' values are permitted"
            );
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (vloc, INVALID_VALUE),
                (*cloc, msg),
            ));
            None
        }
        (_, Some(u)) => Some((vloc, Some(module_id), *u)),
    }
}

fn convert_module_id(context: &mut Context, vloc: Loc, module: &ModuleIdent) -> Option<ModuleId> {
    if !context.constants.contains_key(module) {
        context.env.add_diag(diag!(
            Attributes::InvalidValue,
            (vloc, INVALID_VALUE),
            (module.loc, format!("Unbound module '{module}'")),
        ));
        return None;
    }
    let sp!(mloc, ModuleIdent_ { address, module }) = module;
    let addr = match address {
        Address::Numerical(_, sp!(_, a)) => a.into_inner(),
        Address::NamedUnassigned(addr) => {
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (vloc, INVALID_VALUE),
                (*mloc, format!("Unbound address '{addr}'")),
            ));
            return None;
        }
    };
    let mname = move_core_types::identifier::Identifier::new(module.value().to_string()).unwrap();
    let mid = ModuleId::new(addr, mname);
    Some(mid)
}

fn convert_attribute_value_u64(
    context: &mut Context,
    loc: Loc,
    value: &AttributeValue,
) -> Option<(Loc, u64)> {
    use E::{AttributeValue_ as EAV, Value_ as EV};
    match value {
        sp!(vloc, EAV::Value(sp!(_, EV::InferredNum(u)))) if *u <= U256::from(std::u64::MAX) => {
            Some((*vloc, u.down_cast_lossy()))
        }
        sp!(vloc, EAV::Value(sp!(_, EV::U64(u)))) => Some((*vloc, *u)),
        sp!(vloc, EAV::Value(sp!(_, EV::U8(_))))
        | sp!(vloc, EAV::Value(sp!(_, EV::U16(_))))
        | sp!(vloc, EAV::Value(sp!(_, EV::U32(_))))
        | sp!(vloc, EAV::Value(sp!(_, EV::U128(_))))
        | sp!(vloc, EAV::Value(sp!(_, EV::U256(_)))) => {
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (loc, INVALID_VALUE),
                (*vloc, "Annotated non-u64 literals are not permitted"),
            ));
            None
        }
        sp!(vloc, _) => {
            context.env.add_diag(diag!(
                Attributes::InvalidValue,
                (loc, INVALID_VALUE),
                (*vloc, "Unsupported value in this assignment"),
            ));
            None
        }
    }
}

fn convert_attribute_value_to_move_value(
    context: &mut Context,
    value: &E::AttributeValue_,
) -> Option<MoveValue> {
    use E::{AttributeValue_ as EAV, Value_ as EV};
    match value {
        // Only addresses are allowed
        EAV::Value(sp!(_, EV::Address(a))) => Some(MoveValue::Address(MoveAddress::new(
            context.resolve_address(a).into_bytes(),
        ))),
        _ => None,
    }
}

fn check_location<T>(
    context: &mut Context,
    loc: Loc,
    attr: &str,
    location: Option<T>,
) -> Option<T> {
    if location.is_none() {
        let msg = format!(
            "Expected '{}' following '{attr}'",
            TestingAttribute::ERROR_LOCATION
        );
        context
            .env
            .add_diag(diag!(Attributes::InvalidUsage, (loc, msg)));
    }
    location
}
