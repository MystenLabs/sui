// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::ast::{Address, ModuleAccess, ModuleIdent, Value},
    shared::Name,
    shared::{ast_debug::AstWriter, unique_map::UniqueMap, AstDebug, TName},
};

use move_core_types::vm_status::StatusCode;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;
use std::{collections::BTreeSet, fmt};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
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

/// A full summary of all attribute kinds, used for looking up an individual attribute and
/// organizing them into sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttributeKind_ {
    Allow,
    BytecodeInstruction,
    DefinesPrimitive,
    Deprecation,
    Error,
    ExpectedFailure,
    External,
    LintAllow,
    RandTest,
    Syntax,
    Test,
    TestOnly,
    VerifyOnly,
}

pub type AttributeKind = Spanned<AttributeKind_>;

// -----------------------------------------------
// Individual Attributes
// -----------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
/// It is a fake native function that actually compiles to a bytecode instruction
pub struct BytecodeInstructionAttribute;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinesPrimitiveAttribute {
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Deprecated spec only annotation
pub struct DeprecationAttribute {
    pub note: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticAttribute {
    Allow {
        allow_set: BTreeSet<(Option<Name>, Name)>,
    },
    LintAllow {
        allow_set: BTreeSet<Name>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorAttribute {
    pub code: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAttribute {
    pub attrs: ExternalAttributeEntries,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxAttribute {
    pub kind: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestingAttribute {
    // Is a test that will be run
    Test,
    // Can be called by other testing code, and included in compilation in test mode
    TestOnly,
    // This test is expected to fail
    ExpectedFailure(Box<ExpectedFailure>),
    // This is a test that uses randomly-generated arguments
    RandTest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum ExpectedFailure {
    Expected,
    ExpectedWithCodeDEPRECATED(u64),
    ExpectedWithError {
        status_code: StatusCode,
        minor_code: Option<MinorCode>,
        location: ModuleIdent,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum MinorCode_ {
    Value(u64),
    Constant(ModuleIdent, Name),
}

pub type MinorCode = Spanned<MinorCode_>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationAttribute {
    /// Deprecated spec verification annotation
    VerifyOnly,
}

// -----------------------------------------------
// External Attributes
// -----------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalAttributeValue_ {
    Value(Value),
    Address(Address),
    Module(ModuleIdent),
    ModuleAccess(ModuleAccess),
}
pub type ExternalAttributeValue = Spanned<ExternalAttributeValue_>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalAttributeEntry_ {
    Name(Name),
    Assigned(Name, Box<ExternalAttributeValue>),
    Parameterized(Name, ExternalAttributeEntries),
}

pub type ExternalAttributeEntry = Spanned<ExternalAttributeEntry_>;

pub type ExternalAttributeEntries = UniqueMap<Name, ExternalAttributeEntry>;

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

impl AttributeKind_ {
    pub const fn name(&self) -> &'static str {
        match self {
            AttributeKind_::BytecodeInstruction => {
                BytecodeInstructionAttribute::BYTECODE_INSTRUCTION
            }
            AttributeKind_::Allow => DiagnosticAttribute::ALLOW,
            AttributeKind_::DefinesPrimitive => DefinesPrimitiveAttribute::DEFINES_PRIM,
            AttributeKind_::Deprecation => DeprecationAttribute::DEPRECATED,
            AttributeKind_::Error => ErrorAttribute::ERROR,
            AttributeKind_::ExpectedFailure => TestingAttribute::EXPECTED_FAILURE,
            AttributeKind_::External => ExternalAttribute::EXTERNAL,
            AttributeKind_::LintAllow => DiagnosticAttribute::LINT_ALLOW,
            AttributeKind_::RandTest => TestingAttribute::RAND_TEST,
            AttributeKind_::Syntax => SyntaxAttribute::SYNTAX,
            AttributeKind_::Test => TestingAttribute::TEST,
            AttributeKind_::TestOnly => TestingAttribute::TEST_ONLY,
            AttributeKind_::VerifyOnly => VerificationAttribute::VERIFY_ONLY,
        }
    }
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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        match self {
            Self::BytecodeInstruction(attr) => attr.attribute_kind(),
            Self::DefinesPrimitive(attr) => attr.attribute_kind(),
            Self::Deprecation(attr) => attr.attribute_kind(),
            Self::Diagnostic(attr) => attr.attribute_kind(),
            Self::Error(attr) => attr.attribute_kind(),
            Self::External(attr) => attr.attribute_kind(),
            Self::Syntax(attr) => attr.attribute_kind(),
            Self::Verification(attr) => attr.attribute_kind(),
            Self::Testing(attr) => attr.attribute_kind(),
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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        AttributeKind_::BytecodeInstruction
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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        AttributeKind_::DefinesPrimitive
    }
}

impl DeprecationAttribute {
    pub const DEPRECATED: &'static str = "deprecated";
    pub const NOTE: &'static str = "note";

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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        AttributeKind_::Deprecation
    }
}

pub static DEPRECATED_EXPECTED_KEYS: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    keys.insert(DeprecationAttribute::NOTE.to_string());
    keys
});

impl DiagnosticAttribute {
    pub const ALLOW: &'static str = "allow";
    pub const LINT_ALLOW: &'static str = "lint_allow";
    pub const LINT: &'static str = "lint";
    pub const LINT_SYMBOL: Symbol = symbol!("lint");

    pub const fn name(&self) -> &str {
        match self {
            DiagnosticAttribute::Allow { .. } => Self::ALLOW,
            DiagnosticAttribute::LintAllow { .. } => Self::LINT_ALLOW,
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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        match self {
            DiagnosticAttribute::Allow { .. } => AttributeKind_::Allow,
            DiagnosticAttribute::LintAllow { .. } => AttributeKind_::LintAllow,
        }
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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        AttributeKind_::Error
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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        AttributeKind_::External
    }
}

impl ExternalAttributeEntry_ {
    pub fn name(&self) -> Name {
        match self {
            ExternalAttributeEntry_::Name(name)
            | ExternalAttributeEntry_::Assigned(name, _)
            | ExternalAttributeEntry_::Parameterized(name, _) => *name,
        }
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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        AttributeKind_::Syntax
    }
}

impl TestingAttribute {
    // Testing annotation names
    pub const TEST: &'static str = "test";
    pub const RAND_TEST: &'static str = "random_test";
    pub const TEST_ONLY: &'static str = "test_only";
    pub const EXPECTED_FAILURE: &'static str = "expected_failure";

    // Failure kinds
    pub const ABORT_CODE_NAME: &'static str = "abort_code";
    pub const ARITHMETIC_ERROR_NAME: &'static str = "arithmetic_error";
    pub const VECTOR_ERROR_NAME: &'static str = "vector_error";
    pub const OUT_OF_GAS_NAME: &'static str = "out_of_gas";
    pub const MAJOR_STATUS_NAME: &'static str = "major_status";

    // Other failure arguments
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

    pub fn expected_failure_kinds() -> &'static BTreeSet<String> {
        &EXPECTED_FAILURE_KINDS
    }

    pub fn expected_failure_names() -> &'static BTreeSet<String> {
        &EXPECTED_FAILURE_NAME_KEYS
    }

    pub fn expected_failure_assigned_keys() -> &'static BTreeSet<String> {
        &EXPECTED_FAILURE_ASSIGNED_KEYS
    }

    pub fn expected_failure_valid_keys() -> &'static BTreeSet<String> {
        &EXPECTED_FAILURE_ALL_KEYS
    }

    pub fn attribute_kind(&self) -> AttributeKind_ {
        match self {
            TestingAttribute::Test => AttributeKind_::Test,
            TestingAttribute::TestOnly => AttributeKind_::TestOnly,
            TestingAttribute::ExpectedFailure(..) => AttributeKind_::ExpectedFailure,
            TestingAttribute::RandTest => AttributeKind_::RandTest,
        }
    }
}

static EXPECTED_FAILURE_KINDS: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    keys.insert(TestingAttribute::ARITHMETIC_ERROR_NAME.to_string());
    keys.insert(TestingAttribute::VECTOR_ERROR_NAME.to_string());
    keys.insert(TestingAttribute::OUT_OF_GAS_NAME.to_string());
    keys.insert(TestingAttribute::MAJOR_STATUS_NAME.to_string());
    keys.insert(TestingAttribute::ABORT_CODE_NAME.to_string());
    keys
});

static EXPECTED_FAILURE_NAME_KEYS: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    keys.insert(TestingAttribute::ARITHMETIC_ERROR_NAME.to_string());
    keys.insert(TestingAttribute::VECTOR_ERROR_NAME.to_string());
    keys.insert(TestingAttribute::OUT_OF_GAS_NAME.to_string());
    keys
});

static EXPECTED_FAILURE_ASSIGNED_KEYS: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    keys.insert(TestingAttribute::ABORT_CODE_NAME.to_string());
    keys.insert(TestingAttribute::MAJOR_STATUS_NAME.to_string());
    keys.insert(TestingAttribute::MINOR_STATUS_NAME.to_string());
    keys.insert(TestingAttribute::ERROR_LOCATION.to_string());
    keys
});

static EXPECTED_FAILURE_ALL_KEYS: Lazy<BTreeSet<String>> = Lazy::new(|| {
    let mut keys = BTreeSet::new();
    for key in EXPECTED_FAILURE_NAME_KEYS.iter() {
        keys.insert(key.to_string());
    }
    for key in EXPECTED_FAILURE_ASSIGNED_KEYS.iter() {
        keys.insert(key.to_string());
    }
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

    pub fn attribute_kind(&self) -> AttributeKind_ {
        match self {
            VerificationAttribute::VerifyOnly => AttributeKind_::VerifyOnly,
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

// -------------------------------------------------------------------------------------------------
// TName
// -------------------------------------------------------------------------------------------------

impl TName for Spanned<AttributeKind_> {
    type Key = AttributeKind_;

    type Loc = move_ir_types::location::Loc;

    fn drop_loc(self) -> (Self::Loc, Self::Key) {
        let sp!(loc, value) = self;
        (loc, value)
    }

    fn add_loc(loc: Self::Loc, key: Self::Key) -> Self {
        sp(loc, key)
    }

    fn borrow(&self) -> (&Self::Loc, &Self::Key) {
        let sp!(loc, value) = self;
        (loc, value)
    }
}

// -------------------------------------------------------------------------------------------------
// From
// -------------------------------------------------------------------------------------------------

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

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

impl std::fmt::Display for AttributeKind_ {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

// -------------------------------------------------------------------------------------------------
// AstDebug
// -------------------------------------------------------------------------------------------------

impl AstDebug for BytecodeInstructionAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("bytecode_instruction");
    }
}

impl AstDebug for DefinesPrimitiveAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("defines_primitive(");
        w.write(self.name.to_string());
        w.write(")");
    }
}

impl AstDebug for DeprecationAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("deprecated");
        if let Some(ref note) = self.note {
            w.write("(note= ");
            w.write(std::str::from_utf8(note).unwrap());
            w.write(")");
        }
    }
}

impl AstDebug for DiagnosticAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(self.name());
        w.write("(");
        let mut first = true;
        match self {
            DiagnosticAttribute::Allow { allow_set } => {
                for (prefix, name) in allow_set {
                    if !first {
                        w.write(", ");
                    }
                    first = false;
                    match prefix {
                        Some(pref) => {
                            w.write(pref.to_string());
                            w.write("(");
                            w.write(name.to_string());
                            w.write(")");
                        }
                        None => {
                            w.write(name.to_string());
                        }
                    }
                }
            }
            DiagnosticAttribute::LintAllow { allow_set } => {
                for name in allow_set {
                    if !first {
                        w.write(", ");
                    }
                    first = false;
                    w.write(name.to_string());
                }
            }
        };
        // Each entry is a pair: (Option<Name>, Name)
        w.write(")");
    }
}

impl AstDebug for ErrorAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("error");
        if let Some(code) = self.code {
            w.write("(code= ");
            w.write(code.to_string());
            w.write(")");
        }
    }
}

impl AstDebug for ExternalAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("external");
        // You might choose to print additional details from `self.attrs` if desired.
    }
}

impl AstDebug for SyntaxAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write("syntax(");
        w.write(self.kind.to_string());
        w.write(")");
    }
}

impl AstDebug for VerificationAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            VerificationAttribute::VerifyOnly => w.write("verify_only"),
        }
    }
}

impl AstDebug for TestingAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            TestingAttribute::Test => w.write("test"),
            TestingAttribute::TestOnly => w.write("test_only"),
            TestingAttribute::ExpectedFailure(exp) => {
                w.write("expected_failure(");
                exp.ast_debug(w);
                w.write(")")
            }
            TestingAttribute::RandTest => w.write("rand_test"),
        }
    }
}

impl AstDebug for ExpectedFailure {
    fn ast_debug(&self, _w: &mut AstWriter) {
        todo!()
    }
}

impl AstDebug for KnownAttribute {
    fn ast_debug(&self, w: &mut AstWriter) {
        match self {
            KnownAttribute::BytecodeInstruction(attr) => attr.ast_debug(w),
            KnownAttribute::DefinesPrimitive(attr) => attr.ast_debug(w),
            KnownAttribute::Deprecation(attr) => attr.ast_debug(w),
            KnownAttribute::Diagnostic(attr) => attr.ast_debug(w),
            KnownAttribute::Error(attr) => attr.ast_debug(w),
            KnownAttribute::External(attr) => attr.ast_debug(w),
            KnownAttribute::Syntax(attr) => attr.ast_debug(w),
            KnownAttribute::Testing(attr) => attr.ast_debug(w),
            KnownAttribute::Verification(attr) => attr.ast_debug(w),
        }
    }
}
