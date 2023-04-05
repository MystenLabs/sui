// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compiled_unit::{AnnotatedCompiledUnit, NamedCompiledModule},
    diagnostics::FilesSourceText,
    shared::NumericalAddress,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
    value::MoveValue, vm_status::StatusCode,
};
use std::{collections::BTreeMap, fmt};

pub mod filter_test_members;
pub mod plan_builder;

pub type TestName = String;

#[derive(Debug, Clone)]
pub struct TestPlan {
    pub files: FilesSourceText,
    pub module_tests: BTreeMap<ModuleId, ModuleTestPlan>,
    pub module_info: BTreeMap<ModuleId, NamedCompiledModule>,
}

#[derive(Debug, Clone)]
pub struct ModuleTestPlan {
    pub module_id: ModuleId,
    pub tests: BTreeMap<TestName, TestCase>,
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub test_name: TestName,
    pub arguments: Vec<MoveValue>,
    pub expected_failure: Option<ExpectedFailure>,
}

#[derive(Debug, Clone)]
pub enum ExpectedFailure {
    // expected failure, but codes are not checked
    Expected,
    // expected failure, abort code checked but without the module specified
    ExpectedWithCodeDEPRECATED(u64),
    // expected failure, abort code with the module specified
    ExpectedWithError(ExpectedMoveError),
}

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct ExpectedMoveError(
    pub StatusCode,
    pub Option<u64>,
    pub move_binary_format::errors::Location,
);

pub struct ExpectedMoveErrorDisplay<'a> {
    error: &'a ExpectedMoveError,
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
        files: FilesSourceText,
        units: Vec<AnnotatedCompiledUnit>,
    ) -> Self {
        let module_tests: BTreeMap<_, _> = tests
            .into_iter()
            .map(|module_test| (module_test.module_id.clone(), module_test))
            .collect();

        let module_info = units
            .into_iter()
            .filter_map(|unit| {
                if let AnnotatedCompiledUnit::Module(annot_module) = unit {
                    Some((
                        annot_module.named_module.module.self_id(),
                        annot_module.named_module,
                    ))
                } else {
                    None
                }
            })
            .collect();

        Self {
            files,
            module_tests,
            module_info,
        }
    }
}

impl ExpectedMoveError {
    pub fn verbiage(&self, is_past_tense: bool) -> ExpectedMoveErrorDisplay {
        ExpectedMoveErrorDisplay {
            error: self,
            is_past_tense,
        }
    }
}

impl<'a> fmt::Display for ExpectedMoveErrorDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use move_binary_format::errors::Location;
        let Self {
            error: ExpectedMoveError(status, sub_status, location),
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
            write!(f, " with code {}", sub_status.unwrap())?
        } else if let Some(code) = sub_status {
            write!(f, " with sub-status {code}")?
        };
        if status != &StatusCode::OUT_OF_GAS {
            write!(f, " originating")?;
        }
        match location {
            Location::Undefined => write!(f, " in an unknown location"),
            Location::Script => write!(f, " in the script"),
            Location::Module(id) => write!(f, " in the module {id}"),
        }
    }
}
