// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::{sp, Loc};

use crate::{
    diag,
    diagnostics::Diagnostics,
    parser::{
        ast as P,
        filter::{filter_program, FilterContext},
    },
    shared::{known_attributes, CompilationEnv},
};

struct Context<'env> {
    env: &'env mut CompilationEnv,
}

impl<'env> Context<'env> {
    fn new(compilation_env: &'env mut CompilationEnv) -> Self {
        Self {
            env: compilation_env,
        }
    }
}

impl FilterContext for Context<'_> {
    fn should_remove_by_attributes(
        &mut self,
        attrs: &[P::Attributes],
        is_source_def: bool,
    ) -> bool {
        should_remove_node(self.env, attrs, is_source_def)
    }

    fn filter_map_module(
        &mut self,
        mut module_def: P::ModuleDefinition,
        is_source_def: bool,
    ) -> Option<P::ModuleDefinition> {
        if self.should_remove_by_attributes(&module_def.attributes, is_source_def) {
            return None;
        }

        // instrument the test poison
        if !self.env.flags().is_testing() {
            return Some(module_def);
        }

        let poison_function = create_test_poison(module_def.loc);
        module_def.members.push(poison_function);
        Some(module_def)
    }

    fn filter_map_script(
        &mut self,
        script_def: P::Script,
        _is_source_def: bool,
    ) -> Option<P::Script> {
        // extra sanity check on scripts
        let P::Script {
            attributes,
            uses,
            constants,
            function,
            specs,
            loc: _,
        } = &script_def;

        let script_attributes = attributes
            .iter()
            .chain(uses.iter().flat_map(|use_decl| &use_decl.attributes))
            .chain(constants.iter().flat_map(|constant| &constant.attributes))
            .chain(function.attributes.iter())
            .chain(specs.iter().flat_map(|spec| &spec.value.attributes));

        let diags: Diagnostics = script_attributes
            .flat_map(|attr| {
                test_attributes(attr).into_iter().map(|(loc, _)| {
                    let msg = "Testing attributes are not allowed in scripts.";
                    diag!(Attributes::InvalidTest, (loc, msg))
                })
            })
            .collect();

        // filter the script based on whether there are error messages
        if diags.is_empty() {
            Some(script_def)
        } else {
            self.env.add_diags(diags);
            None
        }
    }
}

//***************************************************************************
// Filtering of test-annotated module members
//***************************************************************************

const UNIT_TEST_MODULE_NAME: &str = "unit_test";
const STDLIB_ADDRESS_NAME: &str = "std";

// This filters out all test, and test-only annotated module member from `prog` if the `test` flag
// in `compilation_env` is not set. If the test flag is set, no filtering is performed, and instead
// a test plan is created for use by the testing framework.
pub fn program(compilation_env: &mut CompilationEnv, prog: P::Program) -> P::Program {
    if !check_has_unit_test_module(compilation_env, &prog) {
        return prog;
    }

    // filter and instrument the parsed AST
    let mut context = Context::new(compilation_env);
    filter_program(&mut context, prog)
}

fn check_has_unit_test_module(compilation_env: &mut CompilationEnv, prog: &P::Program) -> bool {
    let has_unit_test_module = prog
        .lib_definitions
        .iter()
        .chain(prog.source_definitions.iter())
        .any(|pkg| match &pkg.def {
            P::Definition::Module(mdef) => {
                mdef.name.0.value.as_str() == UNIT_TEST_MODULE_NAME
                    && mdef.address.is_some()
                    && match &mdef.address.as_ref().unwrap().value {
                        // TODO: remove once named addresses have landed in the stdlib
                        P::LeadingNameAccess_::Name(name) => {
                            name.value.as_str() == STDLIB_ADDRESS_NAME
                        }
                        P::LeadingNameAccess_::AnonymousAddress(_) => false,
                    }
            }
            _ => false,
        });

    if !has_unit_test_module && compilation_env.flags().is_testing() {
        if let Some(P::PackageDefinition { def, .. }) = prog
            .source_definitions
            .iter()
            .chain(prog.lib_definitions.iter())
            .next()
        {
            let loc = match def {
                P::Definition::Module(P::ModuleDefinition { name, .. }) => name.0.loc,
                P::Definition::Address(P::AddressDefinition { loc, .. })
                | P::Definition::Script(P::Script { loc, .. }) => *loc,
            };
            compilation_env.add_diag(diag!(
                Attributes::InvalidTest,
                (
                    loc,
                    "Compilation in test mode requires passing the UnitTest module in the Move \
                     stdlib as a dependency",
                )
            ));
            return false;
        }
    }

    true
}

/// If a module is being compiled in test mode, create a dummy function that calls a native
/// function `0x1::UnitTest::create_signers_for_testing` that only exists if the VM is being run
/// with the "unit_test" feature flag set. This will then cause the module to fail to link if
/// an attempt is made to publish a module that has been compiled in test mode on a VM that is not
/// running in test mode.
fn create_test_poison(mloc: Loc) -> P::ModuleMember {
    let signature = P::FunctionSignature {
        type_parameters: vec![],
        parameters: vec![],
        return_type: sp(mloc, P::Type_::Unit),
    };

    let leading_name_access = sp(
        mloc,
        P::LeadingNameAccess_::Name(sp(mloc, STDLIB_ADDRESS_NAME.into())),
    );

    let mod_name = sp(mloc, UNIT_TEST_MODULE_NAME.into());
    let mod_addr_name = sp(mloc, (leading_name_access, mod_name));
    let fn_name = sp(mloc, "create_signers_for_testing".into());
    let args_ = vec![sp(
        mloc,
        P::Exp_::Value(sp(mloc, P::Value_::Num("0".into()))),
    )];
    let nop_call = P::Exp_::Call(
        sp(mloc, P::NameAccessChain_::Three(mod_addr_name, fn_name)),
        false,
        None,
        sp(mloc, args_),
    );

    // fun unit_test_poison() { 0x1::UnitTest::create_signers_for_testing(0); () }
    P::ModuleMember::Function(P::Function {
        attributes: vec![],
        loc: mloc,
        visibility: P::Visibility::Internal,
        entry: None,
        acquires: vec![],
        signature,
        name: P::FunctionName(sp(mloc, "unit_test_poison".into())),
        body: sp(
            mloc,
            P::FunctionBody_::Defined((
                vec![],
                vec![sp(
                    mloc,
                    P::SequenceItem_::Seq(Box::new(sp(mloc, nop_call))),
                )],
                None,
                Box::new(Some(sp(mloc, P::Exp_::Unit))),
            )),
        ),
    })
}

// A module member should be removed if:
// * It is annotated as a test function (test_only, test, abort) and test mode is not set; or
// * If it is a library and is annotated as #[test]
fn should_remove_node(env: &CompilationEnv, attrs: &[P::Attributes], is_source_def: bool) -> bool {
    use known_attributes::TestingAttribute;
    let flattened_attrs: Vec<_> = attrs.iter().flat_map(test_attributes).collect();
    let is_test_only = flattened_attrs
        .iter()
        .any(|attr| matches!(attr.1, TestingAttribute::Test | TestingAttribute::TestOnly));
    is_test_only && !env.flags().keep_testing_functions()
        || (!is_source_def
            && flattened_attrs
                .iter()
                .any(|attr| attr.1 == TestingAttribute::Test))
}

fn test_attributes(attrs: &P::Attributes) -> Vec<(Loc, known_attributes::TestingAttribute)> {
    use known_attributes::KnownAttribute;
    attrs
        .value
        .iter()
        .filter_map(
            |attr| match KnownAttribute::resolve(attr.value.attribute_name().value)? {
                KnownAttribute::Testing(test_attr) => Some((attr.loc, test_attr)),
                KnownAttribute::Verification(_) | KnownAttribute::Native(_) => None,
            },
        )
        .collect()
}
