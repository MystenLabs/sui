// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::Lazy;
use std::{collections::BTreeSet, fmt};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnownAttribute {
    Testing(TestingAttribute),
    Verification(VerificationAttribute),
    Native(NativeAttribute),
    Diagnostic(DiagnosticAttribute),
    DefinesPrimitive(DefinesPrimitive),
    External(ExternalAttribute),
    Syntax(SyntaxAttribute),
    Error(ErrorAttribute),
    Deprecation(DeprecationAttribute),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TestingAttribute {
    // Can be called by other testing code, and included in compilation in test mode
    TestOnly,
    // Is a test that will be run
    Test,
    // This test is expected to fail
    ExpectedFailure,
    // This is a test that uses randomly-generated arguments
    RandTest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerificationAttribute {
    // deprecated spec only annotation
    VerifyOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NativeAttribute {
    // It is a fake native function that actually compiles to a bytecode instruction
    BytecodeInstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticAttribute {
    Allow,
    // Deprecated lint allow syntax
    LintAllow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SyntaxAttribute {
    Syntax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefinesPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExternalAttribute;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErrorAttribute;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeprecationAttribute;

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
    pub fn resolve(attribute_str: impl AsRef<str>) -> Option<Self> {
        Some(match attribute_str.as_ref() {
            TestingAttribute::TEST => TestingAttribute::Test.into(),
            TestingAttribute::TEST_ONLY => TestingAttribute::TestOnly.into(),
            TestingAttribute::EXPECTED_FAILURE => TestingAttribute::ExpectedFailure.into(),
            TestingAttribute::RAND_TEST => TestingAttribute::RandTest.into(),
            VerificationAttribute::VERIFY_ONLY => VerificationAttribute::VerifyOnly.into(),
            NativeAttribute::BYTECODE_INSTRUCTION => NativeAttribute::BytecodeInstruction.into(),
            DiagnosticAttribute::ALLOW => DiagnosticAttribute::Allow.into(),
            DiagnosticAttribute::LINT_ALLOW => DiagnosticAttribute::LintAllow.into(),
            DefinesPrimitive::DEFINES_PRIM => DefinesPrimitive.into(),
            ExternalAttribute::EXTERNAL => ExternalAttribute.into(),
            SyntaxAttribute::SYNTAX => SyntaxAttribute::Syntax.into(),
            ErrorAttribute::ERROR => ErrorAttribute.into(),
            DeprecationAttribute::DEPRECATED => DeprecationAttribute.into(),
            _ => return None,
        })
    }

    pub const fn name(&self) -> &str {
        match self {
            Self::Testing(a) => a.name(),
            Self::Verification(a) => a.name(),
            Self::Native(a) => a.name(),
            Self::Diagnostic(a) => a.name(),
            Self::DefinesPrimitive(a) => a.name(),
            Self::External(a) => a.name(),
            Self::Syntax(a) => a.name(),
            Self::Error(a) => a.name(),
            Self::Deprecation(a) => a.name(),
        }
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        match self {
            Self::Testing(a) => a.expected_positions(),
            Self::Verification(a) => a.expected_positions(),
            Self::Native(a) => a.expected_positions(),
            Self::Diagnostic(a) => a.expected_positions(),
            Self::DefinesPrimitive(a) => a.expected_positions(),
            Self::External(a) => a.expected_positions(),
            Self::Syntax(a) => a.expected_positions(),
            Self::Error(a) => a.expected_positions(),
            Self::Deprecation(a) => a.expected_positions(),
        }
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
            Self::ExpectedFailure => Self::EXPECTED_FAILURE,
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
            TestingAttribute::ExpectedFailure => &EXPECTED_FAILURE_POSITIONS,
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

impl NativeAttribute {
    pub const BYTECODE_INSTRUCTION: &'static str = "bytecode_instruction";

    pub const fn name(&self) -> &str {
        match self {
            NativeAttribute::BytecodeInstruction => Self::BYTECODE_INSTRUCTION,
        }
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static BYTECODE_INSTRUCTION_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| IntoIterator::into_iter([AttributePosition::Function]).collect());
        match self {
            NativeAttribute::BytecodeInstruction => &BYTECODE_INSTRUCTION_POSITIONS,
        }
    }
}

impl DiagnosticAttribute {
    pub const ALLOW: &'static str = "allow";
    pub const LINT_ALLOW: &'static str = "lint_allow";

    pub const fn name(&self) -> &str {
        match self {
            DiagnosticAttribute::Allow => Self::ALLOW,
            DiagnosticAttribute::LintAllow => Self::LINT_ALLOW,
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
        match self {
            DiagnosticAttribute::Allow | DiagnosticAttribute::LintAllow => &ALLOW_WARNING_POSITIONS,
        }
    }
}

impl DefinesPrimitive {
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

impl ErrorAttribute {
    pub const ERROR: &'static str = "error";

    pub const fn name(&self) -> &str {
        Self::ERROR
    }

    pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
        static ERROR_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
            Lazy::new(|| BTreeSet::from([AttributePosition::Constant]));
        &ERROR_POSITIONS
    }
}

impl DeprecationAttribute {
    pub const DEPRECATED: &'static str = "deprecated";

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
            Self::Testing(a) => a.fmt(f),
            Self::Verification(a) => a.fmt(f),
            Self::Native(a) => a.fmt(f),
            Self::Diagnostic(a) => a.fmt(f),
            Self::DefinesPrimitive(a) => a.fmt(f),
            Self::External(a) => a.fmt(f),
            Self::Syntax(a) => a.fmt(f),
            Self::Error(a) => a.fmt(f),
            Self::Deprecation(a) => a.fmt(f),
        }
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

impl fmt::Display for NativeAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for DiagnosticAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for DefinesPrimitive {
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

impl fmt::Display for ErrorAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Display for DeprecationAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

//**************************************************************************************************
// From
//**************************************************************************************************

impl From<TestingAttribute> for KnownAttribute {
    fn from(a: TestingAttribute) -> Self {
        Self::Testing(a)
    }
}
impl From<VerificationAttribute> for KnownAttribute {
    fn from(a: VerificationAttribute) -> Self {
        Self::Verification(a)
    }
}
impl From<NativeAttribute> for KnownAttribute {
    fn from(a: NativeAttribute) -> Self {
        Self::Native(a)
    }
}
impl From<DiagnosticAttribute> for KnownAttribute {
    fn from(a: DiagnosticAttribute) -> Self {
        Self::Diagnostic(a)
    }
}
impl From<DefinesPrimitive> for KnownAttribute {
    fn from(a: DefinesPrimitive) -> Self {
        Self::DefinesPrimitive(a)
    }
}
impl From<ExternalAttribute> for KnownAttribute {
    fn from(a: ExternalAttribute) -> Self {
        Self::External(a)
    }
}
impl From<SyntaxAttribute> for KnownAttribute {
    fn from(a: SyntaxAttribute) -> Self {
        Self::Syntax(a)
    }
}
impl From<ErrorAttribute> for KnownAttribute {
    fn from(a: ErrorAttribute) -> Self {
        Self::Error(a)
    }
}
impl From<DeprecationAttribute> for KnownAttribute {
    fn from(a: DeprecationAttribute) -> Self {
        Self::Deprecation(a)
    }
}
