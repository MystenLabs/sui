// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::ast::{ModuleAccess, Value},
    parser::ast::ParsedAttribute,
    shared::Name,
};

use move_ir_types::{ast::ModuleIdent, location::Spanned};
use once_cell::sync::Lazy;
use std::{collections::BTreeSet, fmt};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnownAttribute {
    BytecodeInstruction(BytecodeInstructionAttribute),
    DefinesPrimitive(DefinesPrimitiveAttribute),
    Deprecation(DeprecationAttribute),
    Diagnostic(DiagnosticAttribute),
    Error(ErrorAttribute),
    External(ExternalAttribute),
    Syntax(SyntaxAttribute),
    Testing(TestingAttribute),
    Verification(VerificationAttribute),
}

// -----------------------------------------------
// Individual Attributes
// -----------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// It is a fake native function that actually compiles to a bytecode instruction
pub struct BytecodeInstructionAttribute;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefinesPrimitiveAttribute {
    name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Deprecated spec only annotation
pub struct DeprecationAttribute {
    note: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiagnosticAttribute {
    allow_set: BTreeSet<(Option<Name>, Name)>,
    from_lint_allow: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErrorAttribute {
    code: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExternalAttribute {
    attrs: Spanned<Vec<ParsedAttribute>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SyntaxAttribute {
    kind: Name,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TestingAttribute {
    // Is a test that will be run
    Test,
    // Can be called by other testing code, and included in compilation in test mode
    TestOnly,
    // This test is expected to fail
    ExpectedFailure(ExpectedFailure),
    // This is a test that uses randomly-generated arguments
    RandTest,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExpectedFailure {
    KnownFailure {
        kind: Name,
        location: ModuleIdent,
    },
    AbortCodeFailure {
        abort_code: u64,
        location: ModuleIdent,
    },
    ConstantAbortCodeFailure {
        constant: ModuleAccess,
    },
    StatusCodeFailure {
        major_code: u64,
        minor_code: Option<u64>,
        location: ModuleIdent,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerificationAttribute {
    /// Deprecated spec verification annotation
    VerifyOnly,
}

// -----------------------------------------------
// Attribute Positions
// -----------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttributePosition {
    AddressBlock,
    Module,
    Use,
    Friend,
    Constant,
    Struct,
    Enum,
    Function,
    Spec,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl AttributePosition {
    const ALL: &'static [Self] = &[
        Self::AddressBlock,
        Self::Module,
        Self::Use,
        Self::Friend,
        Self::Constant,
        Self::Struct,
        Self::Function,
        Self::Spec,
    ];
}

impl KnownAttribute {
    pub const fn name(&self) -> &str {
        match self {
            Self::BytecodeInstruction(attr) => attr.name(),
            Self::DefinesPrimitive(attr) => attr.name(),
            Self::Deprecation(attr) => attr.name(),
            Self::Diagnostic(attr) => attr.name(),
            Self::Error(attr) => attr.name(),
            Self::External(attr) => attr.name(),
            Self::Syntax(attr) => attr.name(),
            Self::Verification(attr) => attr.name(),
            Self::Testing(attr) => attr.name(),
        }
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        match self {
            Self::BytecodeInstruction(attr) => attr.expected_positions(),
            Self::DefinesPrimitive(attr) => attr.expected_positions(),
            Self::Deprecation(attr) => attr.expected_positions(),
            Self::Diagnostic(attr) => attr.expected_positions(),
            Self::Error(attr) => attr.expected_positions(),
            Self::External(attr) => attr.expected_positions(),
            Self::Syntax(attr) => attr.expected_positions(),
            Self::Verification(attr) => attr.expected_positions(),
            Self::Testing(attr) => attr.expected_positions(),
        }
    }
}

impl BytecodeInstructionAttribute {
    pub const BYTECODE_INSTRUCTION: &'static str = "bytecode_instruction";

    pub const fn name(&self) -> &str {
        Self::BYTECODE_INSTRUCTION
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static BYTECODE_INSTRUCTION_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| IntoIterator::into_iter([AttributePosition::Function]).collect());
        &BYTECODE_INSTRUCTION_POSITIONS
    }
}

impl DefinesPrimitiveAttribute {
    pub const DEFINES_PRIM: &'static str = "defines_primitive";

    pub const fn name(&self) -> &str {
        Self::DEFINES_PRIM
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static DEFINES_PRIM_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| IntoIterator::into_iter([AttributePosition::Module]).collect());
        &DEFINES_PRIM_POSITIONS
    }
}

impl DeprecationAttribute {
    pub const DEPRECATED: &'static str = "deprecated";
    pub const NOTE: &'static str = "node";

    pub const fn name(&self) -> &str {
        Self::DEPRECATED
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static DEPRECATION_POSITIONS: Lazy<BTreeSet<AttributePosition>> = Lazy::new(|| {
            BTreeSet::from([
                AttributePosition::Constant,
                AttributePosition::Module,
                AttributePosition::Struct,
                AttributePosition::Enum,
                AttributePosition::Function,
            ])
        });
        &DEPRECATION_POSITIONS
    }
}

pub static DEPRECATED_EXPECTED_KEYS: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    keys.insert(DeprecationAttribute::NOTE.to_string());
    keys
});

impl DiagnosticAttribute {
    pub const ALLOW: &'static str = "allow";
    pub const LINT: &'static str = "LINT";
    pub const LINT_ALLOW: &'static str = "lint_allow";

    pub const fn name(&self) -> &str {
        if self.from_lint_allow {
            Self::LINT_ALLOW
        } else {
            Self::ALLOW
        }
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static ALLOW_WARNING_POSITIONS: Lazy<BTreeSet<AttributePosition>> = Lazy::new(|| {
            BTreeSet::from([
                AttributePosition::Module,
                AttributePosition::Constant,
                AttributePosition::Struct,
                AttributePosition::Enum,
                AttributePosition::Function,
            ])
        });
        &ALLOW_WARNING_POSITIONS
    }
}

impl ErrorAttribute {
    pub const ERROR: &'static str = "error";
    pub const CODE: &'static str = "code";

    pub const fn name(&self) -> &str {
        Self::ERROR
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static ERROR_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| BTreeSet::from([AttributePosition::Constant]));
        &ERROR_POSITIONS
    }
}

pub static ERROR_EXPECTED_KEYS: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    keys.insert(ErrorAttribute::CODE.to_string());
    keys
});

impl ExternalAttribute {
    pub const EXTERNAL: &'static str = "ext";

    pub const fn name(&self) -> &str {
        Self::EXTERNAL
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static DEFINES_PRIM_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| AttributePosition::ALL.iter().copied().collect());
        &DEFINES_PRIM_POSITIONS
    }
}

impl SyntaxAttribute {
    pub const SYNTAX: &'static str = "syntax";
    pub const INDEX: &'static str = "index";
    pub const FOR: &'static str = "for";
    pub const ASSIGN: &'static str = "assign";

    pub const fn name(&self) -> &str {
        Self::SYNTAX
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static ALLOW_WARNING_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| BTreeSet::from([AttributePosition::Function]));
        &ALLOW_WARNING_POSITIONS
    }

    pub fn expected_syntax_cases() -> &'static [&'static str] {
        &[Self::INDEX, Self::FOR, Self::ASSIGN]
    }
}

impl TestingAttribute {
    pub const TEST: &'static str = "test";
    pub const RAND_TEST: &'static str = "random_test";
    pub const EXPECTED_FAILURE: &'static str = "expected_failure";
    pub const TEST_ONLY: &'static str = "test_only";
    pub const ABORT_CODE_NAME: &'static str = "abort_code";
    pub const ARITHMETIC_ERROR_NAME: &'static str = "arithmetic_error";
    pub const VECTOR_ERROR_NAME: &'static str = "vector_error";
    pub const OUT_OF_GAS_NAME: &'static str = "out_of_gas";
    pub const MAJOR_STATUS_NAME: &'static str = "major_status";
    pub const MINOR_STATUS_NAME: &'static str = "minor_status";
    pub const ERROR_LOCATION: &'static str = "location";

    pub const fn name(&self) -> &str {
        match self {
            Self::Test => Self::TEST,
            Self::TestOnly => Self::TEST_ONLY,
            Self::ExpectedFailure { .. } => Self::EXPECTED_FAILURE,
            Self::RandTest => Self::RAND_TEST,
        }
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static TEST_ONLY_POSITIONS: Lazy<BTreeSet<AttributePosition>> = Lazy::new(|| {
            BTreeSet::from([
                AttributePosition::AddressBlock,
                AttributePosition::Module,
                AttributePosition::Use,
                AttributePosition::Friend,
                AttributePosition::Constant,
                AttributePosition::Struct,
                AttributePosition::Enum,
                AttributePosition::Function,
            ])
        });
        static TEST_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| BTreeSet::from([AttributePosition::Function]));
        static EXPECTED_FAILURE_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| BTreeSet::from([AttributePosition::Function]));
        match self {
            TestingAttribute::TestOnly => &TEST_ONLY_POSITIONS,
            TestingAttribute::Test | TestingAttribute::RandTest => &TEST_POSITIONS,
            TestingAttribute::ExpectedFailure { .. } => &EXPECTED_FAILURE_POSITIONS,
        }
    }

    pub fn expected_failure_cases() -> &'static [&'static str] {
        &[
            Self::ABORT_CODE_NAME,
            Self::ARITHMETIC_ERROR_NAME,
            Self::VECTOR_ERROR_NAME,
            Self::OUT_OF_GAS_NAME,
            Self::MAJOR_STATUS_NAME,
        ]
    }
}

pub static EXPECTED_FAILURE_EXPECTED_NAMES: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    keys.insert(TestingAttribute::ARITHMETIC_ERROR_NAME.to_string());
    keys.insert(TestingAttribute::VECTOR_ERROR_NAME.to_string());
    keys.insert(TestingAttribute::OUT_OF_GAS_NAME.to_string());
    keys
});

pub static EXPECTED_FAILURE_EXPECTED_KEYS: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    keys.insert(TestingAttribute::ABORT_CODE_NAME.to_string());
    keys.insert(TestingAttribute::MAJOR_STATUS_NAME.to_string());
    keys.insert(TestingAttribute::MINOR_STATUS_NAME.to_string());
    keys.insert(TestingAttribute::ERROR_LOCATION.to_string());
    keys
});

impl VerificationAttribute {
    pub const VERIFY_ONLY: &'static str = "verify_only";

    pub const fn name(&self) -> &str {
        match self {
            Self::VerifyOnly => Self::VERIFY_ONLY,
        }
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static VERIFY_ONLY_POSITIONS: Lazy<BTreeSet<AttributePosition>> = Lazy::new(|| {
            BTreeSet::from([
                AttributePosition::AddressBlock,
                AttributePosition::Module,
                AttributePosition::Use,
                AttributePosition::Friend,
                AttributePosition::Constant,
                AttributePosition::Struct,
                AttributePosition::Enum,
                AttributePosition::Function,
            ])
        });
        match self {
            Self::VerifyOnly => &VERIFY_ONLY_POSITIONS,
        }
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

impl fmt::Display for AttributePosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddressBlock => write!(f, "address block"),
            Self::Module => write!(f, "module"),
            Self::Use => write!(f, "use"),
            Self::Friend => write!(f, "friend"),
            Self::Constant => write!(f, "constant"),
            Self::Struct => write!(f, "struct"),
            Self::Enum => write!(f, "enum"),
            Self::Function => write!(f, "function"),
            Self::Spec => write!(f, "spec"),
        }
    }
}

impl fmt::Display for KnownAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BytecodeInstruction(a) => a.fmt(f),
            Self::DefinesPrimitive(a) => a.fmt(f),
            Self::Deprecation(a) => a.fmt(f),
            Self::Diagnostic(a) => a.fmt(f),
            Self::Error(a) => a.fmt(f),
            Self::External(a) => a.fmt(f),
            Self::Syntax(a) => a.fmt(f),
            Self::Testing(a) => a.fmt(f),
            Self::Verification(a) => a.fmt(f),
        }
    }
}

impl fmt::Display for BytecodeInstructionAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for DefinesPrimitiveAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for DeprecationAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for DiagnosticAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for ErrorAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for ExternalAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for SyntaxAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for TestingAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for VerificationAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

//**************************************************************************************************
// From
//**************************************************************************************************

macro_rules! impl_from_for_known_attribute {
    ($($source:ty => $variant:ident),* $(,)?) => {
        $(
            impl From<$source> for KnownAttribute {
                fn from(a: $source) -> Self {
                    Self::$variant(a)
                }
            }
        )*
    };
}

impl_from_for_known_attribute! {
    BytecodeInstructionAttribute => BytecodeInstruction,
    DefinesPrimitiveAttribute => DefinesPrimitive,
    DeprecationAttribute => Deprecation,
    DiagnosticAttribute => Diagnostic,
    ErrorAttribute => Error,
    ExternalAttribute => External,
    SyntaxAttribute => Syntax,
    TestingAttribute => Testing,
    VerificationAttribute => Verification,
}
