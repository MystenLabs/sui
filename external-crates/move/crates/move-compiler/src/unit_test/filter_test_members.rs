// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::{Loc, sp};
use move_symbol_pool::Symbol;

use crate::{
    command_line::compiler::FullyCompiledProgram,
    diag,
    diagnostics::DiagnosticReporter,
    parser::{
        ast::{self as P, DocComment, NamePath, PathEntry},
        filter::{FilterContext, filter_program},
    },
    shared::{
        CompilationEnv,
        known_attributes::{self, AttributeKind_},
    },
};

use std::sync::Arc;

struct Context<'env> {
    env: &'env CompilationEnv,
    is_source_def: bool,
    current_package: Option<Symbol>,
}

impl<'env> Context<'env> {
    fn new(env: &'env CompilationEnv) -> Self {
        Self {
            env,
            is_source_def: false,
            current_package: None,
        }
    }
}

impl FilterContext for Context<'_> {
    fn set_current_package(&mut self, package: Option<Symbol>) {
        self.current_package = package;
    }

    fn set_is_source_def(&mut self, is_source_def: bool) {
        self.is_source_def = is_source_def;
    }

    fn filter_map_module(
        &mut self,
        mut module_def: P::ModuleDefinition,
    ) -> Option<P::ModuleDefinition> {
        if self.should_remove_by_attributes(&module_def.attributes) {
            return None;
        }

        // instrument the test poison
        if !self.env.test_mode() {
            return Some(module_def);
        }

        let poison_function = create_test_poison(module_def.loc);
        module_def.members.push(poison_function);
        Some(module_def)
    }

    // Mode filtering happens in the mode filter for `#[mode(test)]`. We further remove any
    // `#[test]` or `#[rand_test]` that is not in our source definition. This means we will filter
    // the following definitions:
    // * Definitions annotated as a test function (test, random_test, abort) and test mode is not set
    // * Definitions in a library annotated with the same
    fn should_remove_by_attributes(&mut self, attrs: &[P::Attributes]) -> bool {
        let flattened_attrs: Vec<_> = attrs.iter().flat_map(test_attribute_kinds).collect();
        let has_test_attr = flattened_attrs
            .iter()
            .any(|attr| matches!(attr.1, AttributeKind_::Test | AttributeKind_::RandTest));
        has_test_attr && (!self.is_source_def || !self.env.keep_testing_functions())
    }
}

//***************************************************************************
// Filtering of test-annotated module members
//***************************************************************************

const UNIT_TEST_MODULE_NAME: Symbol = symbol!("unit_test");
const STDLIB_ADDRESS_NAME: Symbol = symbol!("std");
pub const UNIT_TEST_POISON_FUN_NAME: Symbol = symbol!("unit_test_poison");

// This filters out all test, and test-only annotated module member from `prog` if the `test` flag
// in `compilation_env` is not set. If the test flag is set, no filtering is performed, and instead
// a test plan is created for use by the testing framework.
pub fn program(
    compilation_env: &CompilationEnv,
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: P::Program,
) -> P::Program {
    let reporter = compilation_env.diagnostic_reporter_at_top_level();
    if !check_has_unit_test_module(compilation_env, &reporter, pre_compiled_lib, &prog) {
        return prog;
    }

    // filter and instrument the parsed AST
    let mut context = Context::new(compilation_env);
    filter_program(&mut context, prog)
}

fn has_stdlib_unit_test_module(prog: &P::Program) -> bool {
    prog.lib_definitions
        .iter()
        .chain(prog.source_definitions.iter())
        .any(|pkg| match &pkg.def {
            P::Definition::Module(mdef) => {
                mdef.name.0.value == UNIT_TEST_MODULE_NAME
                    && mdef.address.is_some()
                    && match &mdef.address.as_ref().unwrap().value {
                        // TODO: remove once named addresses have landed in the stdlib
                        P::LeadingNameAccess_::Name(name) => name.value == STDLIB_ADDRESS_NAME,
                        P::LeadingNameAccess_::GlobalAddress(name) => {
                            name.value == STDLIB_ADDRESS_NAME
                        }
                        P::LeadingNameAccess_::AnonymousAddress(_) => false,
                    }
            }
            _ => false,
        })
}

fn check_has_unit_test_module(
    compilation_env: &CompilationEnv,
    reporter: &DiagnosticReporter,
    pre_compiled_lib: Option<Arc<FullyCompiledProgram>>,
    prog: &P::Program,
) -> bool {
    let has_unit_test_module = has_stdlib_unit_test_module(prog)
        || pre_compiled_lib.is_some_and(|p| has_stdlib_unit_test_module(&p.parser));
    if !has_unit_test_module && compilation_env.test_mode() {
        if let Some(P::PackageDefinition { def, .. }) = prog
            .source_definitions
            .iter()
            .chain(prog.lib_definitions.iter())
            .next()
        {
            let loc = match def {
                P::Definition::Module(P::ModuleDefinition { name, .. }) => name.0.loc,
                P::Definition::Address(P::AddressDefinition { loc, .. }) => *loc,
            };
            reporter.add_diag(diag!(
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
/// function `0x1::unit_test::poison` that only exists if the VM is being run
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
        P::LeadingNameAccess_::Name(sp(mloc, STDLIB_ADDRESS_NAME)),
    );

    let mod_name = sp(mloc, UNIT_TEST_MODULE_NAME);
    let fn_name = sp(mloc, symbol!("poison"));
    let name_path = NamePath {
        root: P::RootPathEntry {
            name: leading_name_access,
            tyargs: None,
            is_macro: None,
        },
        entries: vec![
            PathEntry {
                name: mod_name,
                tyargs: None,
                is_macro: None,
            },
            PathEntry {
                name: fn_name,
                tyargs: None,
                is_macro: None,
            },
        ],
        is_incomplete: false,
    };
    let nop_call = P::Exp_::Call(
        sp(mloc, P::NameAccessChain_::Path(name_path)),
        sp(mloc, vec![]),
    );

    // fun unit_test_poison() { 0x1::UnitTest::poison(0); () }
    P::ModuleMember::Function(P::Function {
        doc: DocComment::empty(),
        attributes: vec![],
        loc: mloc,
        visibility: P::Visibility::Internal,
        entry: Some(mloc), // it's a bit of a hack to avoid treating this function as unused
        macro_: None,
        signature,
        name: P::FunctionName(sp(mloc, UNIT_TEST_POISON_FUN_NAME)),
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

fn test_attribute_kinds(attrs: &P::Attributes) -> Vec<(Loc, known_attributes::AttributeKind_)> {
    attrs
        .value
        .0
        .iter()
        .filter_map(|attr| match attr.value {
            P::Attribute_::BytecodeInstruction
            | P::Attribute_::DefinesPrimitive(..)
            | P::Attribute_::Deprecation { .. }
            | P::Attribute_::Error { .. }
            | P::Attribute_::External { .. }
            | P::Attribute_::Mode { .. }
            | P::Attribute_::Syntax { .. }
            | P::Attribute_::Allow { .. }
            | P::Attribute_::LintAllow { .. } => None,
            // -- testing attributes
            P::Attribute_::Test => Some((attr.loc, known_attributes::AttributeKind_::Test)),
            P::Attribute_::RandomTest => {
                Some((attr.loc, known_attributes::AttributeKind_::RandTest))
            }
            P::Attribute_::ExpectedFailure { .. } => None,
        })
        .collect()
}
