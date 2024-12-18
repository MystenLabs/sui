// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compiled_unit::NamedCompiledModule, shared::files::MappedFiles, shared::NumericalAddress,
};
use move_binary_format::CompiledModule;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, TypeTag},
    runtime_value::MoveValue,
    vm_status::StatusCode,
};
use std::{collections::BTreeMap, fmt};

pub mod filter_test_members;
pub mod plan_builder;

pub type TestName = String;

pub struct TestPlan {
    pub mapped_files: MappedFiles,
    pub module_tests: BTreeMap<ModuleId, ModuleTestPlan>,
    pub module_info: BTreeMap<ModuleId, NamedCompiledModule>,
    pub bytecode_deps_modules: Vec<CompiledModule>,
}

#[derive(Debug, Clone)]
pub struct ModuleTestPlan {
    pub module_id: ModuleId,
    pub tests: BTreeMap<TestName, TestCase>,
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub test_name: TestName,
    pub arguments: Vec<TestArgument>,
    pub expected_failure: Option<ExpectedFailure>,
}

#[derive(Debug, Clone)]
pub enum TestArgument {
    Value(MoveValue),
    Generate { generated_type: TypeTag },
}

#[derive(Debug, Clone)]
pub enum ExpectedFailure {
    // expected failure, but codes are not checked
    Expected,
    // expected failure, abort code checked but without the module specified
    ExpectedWithCodeDEPRECATED(MoveErrorType),
    // expected failure, abort code with the module specified
    ExpectedWithError(ExpectedMoveError),
}

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub enum MoveErrorType {
    Code(u64),
    ConstantName(String),
}

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct ExpectedMoveError(
    pub StatusCode,
    pub Option<MoveErrorType>,
    pub move_binary_format::errors::Location,
);

pub struct ExpectedMoveErrorDisplay<'a> {
    error: &'a ExpectedMoveError,
    context: &'a BTreeMap<ModuleId, NamedCompiledModule>,
    is_past_tense: bool,
}

impl ModuleTestPlan {
    pub fn new(
        addr: &NumericalAddress,
        module_name: &str,
        tests: BTreeMap<TestName, TestCase>,
    ) -> Self {
        let addr = AccountAddress::new((*addr).into_bytes());
        let name = Identifier::new(module_name.to_owned()).unwrap();
        let module_id = ModuleId::new(addr, name);
        ModuleTestPlan { module_id, tests }
    }
}

impl TestPlan {
    pub fn new(
        tests: Vec<ModuleTestPlan>,
        mapped_files: MappedFiles,
        units: Vec<NamedCompiledModule>,
        bytecode_deps_modules: Vec<CompiledModule>,
    ) -> Self {
        let module_tests: BTreeMap<_, _> = tests
            .into_iter()
            .map(|module_test| (module_test.module_id.clone(), module_test))
            .collect();

        let module_info = units
            .into_iter()
            .map(|unit| (unit.module.self_id(), unit))
            .collect();

        Self {
            mapped_files,
            module_tests,
            module_info,
            bytecode_deps_modules,
        }
    }
}

impl<'a> ExpectedMoveError {
    pub fn with_context(
        &'a self,
        context: &'a BTreeMap<ModuleId, NamedCompiledModule>,
    ) -> ExpectedMoveErrorDisplay<'a> {
        ExpectedMoveErrorDisplay {
            error: self,
            context,
            is_past_tense: false,
        }
    }
}

impl<'a> ExpectedMoveErrorDisplay<'a> {
    pub fn past_tense(mut self) -> Self {
        self.is_past_tense = true;
        self
    }

    pub fn present_tense(mut self) -> Self {
        self.is_past_tense = false;
        self
    }
}

impl fmt::Display for MoveErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoveErrorType::Code(code) => write!(f, "{}", code),
            MoveErrorType::ConstantName(name) => write!(f, "'{}'", name),
        }
    }
}

impl<'a> fmt::Display for ExpectedMoveErrorDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use move_binary_format::errors::Location;
        let Self {
            error: ExpectedMoveError(status, sub_status, location),
            context,
            is_past_tense,
        } = self;
        let status_val: u64 = (*status).into();
        if *is_past_tense {
            match status {
                StatusCode::ABORTED => write!(f, "aborted")?,
                StatusCode::ARITHMETIC_ERROR => write!(f, "gave an arithmetic error")?,
                StatusCode::VECTOR_OPERATION_ERROR => write!(f, "gave a vector operation error")?,
                StatusCode::OUT_OF_GAS => write!(f, "ran out of gas")?,
                _ => write!(f, "gave a {status:?} (code {status_val}) error")?,
            };
        } else {
            match status {
                StatusCode::ABORTED => write!(f, "to abort")?,
                StatusCode::ARITHMETIC_ERROR => write!(f, "to give an arithmetic error")?,
                StatusCode::VECTOR_OPERATION_ERROR => {
                    write!(f, "to give a vector operation error")?
                }
                StatusCode::OUT_OF_GAS => write!(f, "to run out of gas")?,
                _ => write!(f, "to give a {status:?} (code {status_val}) error")?,
            };
        }
        if status == &StatusCode::ABORTED {
            match sub_status {
                Some(MoveErrorType::Code(code)) => write!(f, " with code {}", code)?,
                Some(MoveErrorType::ConstantName(name)) => {
                    write!(f, " with error constant '{}'", name)?
                }
                None => (),
            }
        } else if let Some(code) = sub_status {
            write!(f, " with sub-status {code}")?
        };
        if status != &StatusCode::OUT_OF_GAS {
            write!(f, " originating")?;
        }
        match location {
            Location::Undefined => write!(f, " in an unknown location"),
            Location::Module(id) => {
                let module_id =
                    if let Some(address_name) = context.get(id).and_then(|m| m.address_name()) {
                        format!("{}::{}", address_name, id.name())
                    } else {
                        id.short_str_lossless()
                    };
                write!(f, " in the module {}", module_id)
            }
        }
    }
}
