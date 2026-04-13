// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

//**************************************************************************************************
// Main types
//**************************************************************************************************

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
pub enum Severity {
    Note = 0,
    Warning = 1,
    NonblockingError = 2,
    BlockingError = 3,
    Bug = 4,
}

/// A compact identifier for a diagnostic family. Compiler-internal diagnostics use
/// `COMPILER_FAMILY`; external tools (linters, sui-specific checks, etc.) register their
/// family at startup and receive a unique `Family`.
#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
pub struct Family(u8);

pub const COMPILER_FAMILY: Family = Family(0);
pub const LINT_WARNING_FAMILY: Family = Family(1);
pub const SUI_LINT_FAMILY: Family = Family(2);

impl Family {
    pub fn as_u8(self) -> u8 {
        self.0
    }
}

/// Registry mapping `Family` values to their display prefixes. Built once during compiler
/// setup and shared (read-only) thereafter. The display prefix is used when rendering diagnostic
/// codes (e.g., `"Lint W04001"`).
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct FamilyRegistry {
    /// Index = Family.0. The compiler family (index 0) has no display prefix.
    prefixes: Vec<Option<&'static str>>,
    /// Reverse map from display prefix string to Family for lookup during filter resolution.
    by_prefix: BTreeMap<&'static str, Family>,
}

impl FamilyRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            prefixes: vec![None], // 0 = COMPILER, no display prefix
            by_prefix: BTreeMap::new(),
        };
        assert!(registry.register("Lint ") == LINT_WARNING_FAMILY);
        assert!(registry.register("Sui ") == SUI_LINT_FAMILY);
        registry
    }

    /// Register an external family with the given display prefix (e.g., `"Lint "`, `"Sui "`).
    /// Returns the assigned `Family`. Panics if more than 255 families are registered.
    pub fn register(&mut self, display_prefix: &'static str) -> Family {
        if let Some(&existing) = self.by_prefix.get(display_prefix) {
            return existing;
        }
        let id = self.prefixes.len();
        assert!(id <= u8::MAX as usize, "too many diagnostic families");
        let ns = Family(id as u8);
        self.prefixes.push(Some(display_prefix));
        self.by_prefix.insert(display_prefix, ns);
        ns
    }

    /// Look up the display prefix for a family. Returns `None` for the compiler family.
    pub fn display_prefix(&self, ns: Family) -> Option<&'static str> {
        self.prefixes.get(ns.0 as usize).copied().unwrap_or(None)
    }

    /// Look up a `Family` by its display prefix string.
    pub fn family_for_prefix(&self, prefix: &str) -> Option<Family> {
        self.by_prefix.get(prefix).copied()
    }
}

impl Default for FamilyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// The ID for a diagnostic, consisting of a family, category, and code.
pub type DiagnosticsID = (Family, u8, u8);

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Hash)]
pub struct DiagnosticInfo {
    severity: Severity,
    category: u8,
    code: u8,
    family: Family,
    message: &'static str,
}

pub(crate) trait DiagnosticCode: Copy {
    const CATEGORY: Category;

    fn severity(&self) -> Severity;

    fn code_and_message(&self) -> (u8, &'static str);

    fn into_info(self) -> DiagnosticInfo {
        let severity = self.severity();
        let category = Self::CATEGORY as u8;
        let (code, message) = self.code_and_message();
        DiagnosticInfo {
            severity,
            category,
            code,
            family: COMPILER_FAMILY,
            message,
        }
    }
}

//**************************************************************************************************
// Categories and Codes
//**************************************************************************************************

/// A custom DiagnosticInfo for external diagnostics (lints, sui-specific checks, etc.).
/// The diagnostic will get rendered as
/// `"[{display_prefix}{severity}{category}{code}] {message}"`,
/// where the display prefix is looked up from the `FamilyRegistry` at render time.
/// Note, this will panic if `category > 99`
pub const fn custom(
    family: Family,
    severity: Severity,
    category: u8,
    code: u8,
    message: &'static str,
) -> DiagnosticInfo {
    assert!(category <= 99);
    DiagnosticInfo {
        severity,
        category,
        code,
        family,
        message,
    }
}

macro_rules! codes {
    ($($cat:ident: [
        $($code:ident: { msg: $code_msg:literal, severity:$sev:ident $(,)? }),* $(,)?
    ]),* $(,)?) => {
        #[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Category {
            $($cat,)*
        }

        impl TryFrom<u8> for Category {
            type Error = ();
            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match () {
                    $(_ if value == (Category::$cat as u8) => Ok(Category::$cat),)*
                    _ => Err(()),
                }
            }
        }

        $(
            #[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
            #[repr(u8)]
            pub enum $cat {
                DontStartAtZeroPlaceholder,
                $($code,)*
            }

            impl DiagnosticCode for $cat {
                const CATEGORY: Category = {
                    // hacky check that $cat_num <= 99
                    let cat_is_leq_99 = (Category::$cat as u8) <= 99;
                    ["Diagnostic Category must be a u8 <= 99"][!cat_is_leq_99 as usize];
                    Category::$cat
                };

                fn severity(&self) -> Severity {
                    match self {
                        Self::DontStartAtZeroPlaceholder =>
                            panic!("ICE do not use placeholder error code"),
                        $(Self::$code => Severity::$sev,)*
                    }
                }

                fn code_and_message(&self) -> (u8, &'static str) {
                    let code = *self as u8;
                    debug_assert!(code > 0);
                    match self {
                        Self::DontStartAtZeroPlaceholder =>
                            panic!("ICE do not use placeholder error code"),
                        $(Self::$code => (code, $code_msg),)*
                    }
                }
            }
        )*

    };
}

codes!(
    // bucket for random one off errors. unlikely to be used
    Uncategorized: [
        DeprecatedWillBeRemoved: { msg: "DEPRECATED. will be removed", severity: Warning },
        DeprecatedSpecItem: { msg: "DEPRECATED. unexpected spec item", severity: NonblockingError },
        UnableToMigrate: { msg: "unable to migrate", severity: NonblockingError },
    ],
    // syntax errors
    Syntax: [
        InvalidCharacter: { msg: "invalid character", severity: NonblockingError },
        UnexpectedToken: { msg: "unexpected token", severity: NonblockingError },
        InvalidModifier: { msg: "invalid modifier", severity: NonblockingError },
        InvalidDocComment: { msg: "invalid documentation comment", severity: Warning },
        InvalidAddress: { msg: "invalid address", severity: NonblockingError },
        InvalidNumber: { msg: "invalid number literal", severity: NonblockingError },
        InvalidByteString: { msg: "invalid byte string", severity: NonblockingError },
        InvalidHexString: { msg: "invalid hex string", severity: NonblockingError },
        InvalidLValue: { msg: "invalid assignment", severity: NonblockingError },
        SpecContextRestricted:
            { msg: "syntax item restricted to spec contexts", severity: BlockingError },
        InvalidSpecBlockMember: { msg: "invalid spec block member", severity: NonblockingError },
        InvalidRestrictedIdentifier:
            { msg: "invalid identifier escape", severity: NonblockingError },
        InvalidMoveOrCopy: { msg: "invalid 'move' or 'copy'", severity: NonblockingError },
        InvalidLabel: { msg: "invalid expression label", severity: NonblockingError },
        AmbiguousCast: { msg: "ambiguous 'as'", severity: NonblockingError },
        InvalidName: { msg: "invalid name", severity: BlockingError },
        InvalidMacro: { msg: "invalid macro invocation", severity: BlockingError },
        InvalidMatch: { msg: "invalid 'match'", severity: BlockingError },
        InvalidString: { msg: "invalid string literal", severity: NonblockingError },
    ],
    // errors for any rules around declaration items
    Declarations: [
        DuplicateItem:
            { msg: "duplicate declaration, item, or annotation", severity: NonblockingError },
        UnnecessaryItem: { msg: "unnecessary or extraneous item", severity: NonblockingError },
        InvalidAddress: { msg: "invalid 'address' declaration", severity: NonblockingError },
        InvalidModule: { msg: "invalid 'module' declaration", severity: NonblockingError },
        InvalidScript: { msg: "invalid 'script' declaration", severity: NonblockingError },
        InvalidConstant: { msg: "invalid 'const' declaration", severity: NonblockingError },
        InvalidFunction: { msg: "invalid 'fun' declaration", severity: NonblockingError },
        InvalidStruct: { msg: "invalid 'struct' declaration", severity: NonblockingError },
        InvalidSpec: { msg: "invalid 'spec' declaration", severity: NonblockingError },
        InvalidName: { msg: "invalid name", severity: BlockingError },
        InvalidFriendDeclaration:
            { msg: "invalid 'friend' declaration", severity: NonblockingError },
        InvalidAcquiresItem: { msg: "invalid 'acquires' item", severity: NonblockingError },
        InvalidPhantomUse:
            { msg: "invalid phantom type parameter usage", severity: NonblockingError },
        InvalidNonPhantomUse:
            { msg: "invalid non-phantom type parameter usage", severity: Warning },
        InvalidAttribute: { msg: "invalid attribute", severity: NonblockingError },
        InvalidVisibilityModifier:
            { msg: "invalid visibility modifier", severity: NonblockingError },
        InvalidUseFun: { msg: "invalid 'use fun' declaration", severity: NonblockingError },
        UnknownAttribute: { msg: "unknown attribute", severity: Warning },
        InvalidSyntaxMethod: { msg: "invalid 'syntax' method type", severity: NonblockingError },
        MissingSyntaxMethod: { msg: "no valid 'syntax' declaration found", severity: BlockingError },
        DuplicateAlias: { msg: "duplicate alias", severity: Warning },
        InvalidEnum: { msg: "invalid 'enum' declaration", severity: NonblockingError },
        AmbiguousAddress: { msg: "ambiguous address", severity: Warning },
    ],
    // errors name resolution, mostly expansion/translate and naming/translate
    NameResolution: [
        AddressWithoutValue: { msg: "address with no value", severity: NonblockingError },
        UnboundModule: { msg: "unbound module", severity: BlockingError },
        UnboundModuleMember: { msg: "unbound module member", severity: BlockingError },
        UnboundType: { msg: "unbound type", severity: BlockingError },
        UnboundUnscopedName: { msg: "unbound unscoped name", severity: BlockingError },
        NamePositionMismatch: { msg: "unexpected name in this position", severity: BlockingError },
        TooManyTypeArguments: { msg: "too many type arguments", severity: NonblockingError },
        TooFewTypeArguments: { msg: "too few type arguments", severity: BlockingError },
        UnboundVariable: { msg: "unbound variable", severity: BlockingError },
        UnboundField: { msg: "unbound field", severity: NonblockingError },
        ReservedName: { msg: "invalid use of reserved name", severity: BlockingError },
        UnboundMacro: { msg: "unbound macro", severity: BlockingError },
        PositionalCallMismatch: { msg: "positional call mismatch", severity: NonblockingError },
        InvalidLabel: { msg: "invalid use of label", severity: BlockingError },
        UnboundLabel: { msg: "unbound label", severity: BlockingError },
        InvalidMut: { msg: "invalid 'mut' declaration", severity: NonblockingError },
        InvalidMacroParameter: { msg: "invalid macro parameter", severity: NonblockingError },
        InvalidTypeParameter: { msg: "invalid type parameter", severity: NonblockingError },
        InvalidPattern: { msg: "invalid pattern", severity: BlockingError },
        UnboundVariant: { msg: "unbound variant", severity: BlockingError },
        InvalidTypeAnnotation: { msg: "invalid type annotation", severity: NonblockingError },
        InvalidPosition: { msg: "invalid usage position", severity: NonblockingError },
    ],
    // errors for typing rules. mostly typing/translate
    TypeSafety: [
        Visibility: { msg: "restricted visibility", severity: NonblockingError },
        ScriptContext: { msg: "requires script context", severity: NonblockingError },
        BuiltinOperation: { msg: "built-in operation not supported", severity: BlockingError },
        ExpectedBaseType: { msg: "expected a single non-reference type", severity: BlockingError },
        ExpectedSingleType: { msg: "expected a single type", severity: BlockingError },
        SubtypeError: { msg: "invalid subtype", severity: BlockingError },
        JoinError: { msg: "incompatible types", severity: BlockingError },
        RecursiveType: { msg: "invalid type. recursive type found", severity: BlockingError },
        ExpectedSpecificType: { msg: "expected specific type", severity: BlockingError },
        UninferredType: { msg: "cannot infer type", severity: BlockingError },
        ScriptSignature: { msg: "invalid script signature", severity: NonblockingError },
        TypeForConstant: { msg: "invalid type for constant", severity: BlockingError },
        UnsupportedConstant:
            { msg: "invalid statement or expression in constant", severity: BlockingError },
        InvalidLoopControl: { msg: "invalid loop control", severity: BlockingError },
        InvalidNativeUsage: { msg: "invalid use of native item", severity: BlockingError },
        TooFewArguments: { msg: "too few arguments", severity: NonblockingError },
        TooManyArguments: { msg: "too many arguments", severity: NonblockingError },
        CyclicData: { msg: "cyclic data", severity: NonblockingError },
        CyclicInstantiation:
            { msg: "cyclic type instantiation", severity: NonblockingError },
        MissingAcquires: { msg: "missing acquires annotation", severity: NonblockingError },
        InvalidNum: { msg: "invalid number after type inference", severity: NonblockingError },
        NonInvocablePublicScript: {
            msg: "script function cannot be invoked with this signature \
                (NOTE: this may become an error in the future)",
            severity: Warning
        },
        InvalidMethodCall: { msg: "invalid method call", severity: BlockingError },
        InvalidImmVariableUsage:
            { msg: "invalid usage of immutable variable", severity: NonblockingError },
        InvalidControlFlow: { msg: "invalid control flow", severity: BlockingError },
        InvalidCopyOp: { msg: "invalid 'copy' usage", severity: NonblockingError },
        InvalidMoveOp: { msg: "invalid 'move' usage", severity: NonblockingError },
        ImplicitConstantCopy: { msg: "implicit copy of a constant", severity: Warning },
        InvalidCallTarget: { msg: "invalid function call", severity: BlockingError },
        UnexpectedFunctionType: { msg: "invalid usage of lambda type", severity: BlockingError },
        UnexpectedLambda: { msg: "invalid usage of lambda", severity: BlockingError },
        CannotExpandMacro: { msg: "unable to expand macro function", severity: BlockingError },
        InvariantError: { msg: "types are not equal", severity: BlockingError },
        IncompatibleSyntaxMethods: { msg: "'syntax' method types differ", severity: BlockingError },
        InvalidErrorUsage: { msg: "invalid constant usage in error context", severity: BlockingError },
        IncompletePattern: { msg: "non-exhaustive pattern", severity: BlockingError },
        DeprecatedUsage: { msg: "deprecated usage", severity: Warning },
        InvalidString: { msg: "invalid string after type inference", severity: NonblockingError },
        MissingLiteralType:
            { msg: "unable to determine principal type for literal", severity: Warning },
    ],
    // errors for ability rules. mostly typing/translate
    AbilitySafety: [
        Constraint: { msg: "ability constraint not satisfied", severity: NonblockingError },
        ImplicitlyCopyable: { msg: "type not implicitly copyable", severity: NonblockingError },
    ],
    // errors for move rules. mostly cfgir/locals
    MoveSafety: [
        UnusedUndroppable: { msg: "unused value without 'drop'", severity: NonblockingError },
        UnassignedVariable: { msg: "use of unassigned variable", severity: NonblockingError },
    ],
    // errors for move rules. mostly cfgir/borrows
    ReferenceSafety: [
        RefTrans: { msg: "referential transparency violated", severity: BlockingError },
        MutOwns: { msg: "mutable ownership violated", severity: NonblockingError },
        Dangling: {
            msg: "invalid operation, could create dangling a reference",
            severity: NonblockingError,
        },
        InvalidReturn:
            { msg: "invalid return of locally borrowed state", severity: NonblockingError },
        InvalidTransfer: { msg: "invalid transfer of references", severity: NonblockingError },
        AmbiguousVariableUsage: { msg: "ambiguous usage of variable", severity: NonblockingError },
    ],
    CodeGeneration: [
        UnfoldableConstant: { msg: "cannot compute constant value", severity: NonblockingError },
    ],
    // errors for any unused code or items
    UnusedItem: [
        Alias: { msg: "unused alias", severity: Warning },
        Variable: { msg: "unused variable", severity: Warning },
        Assignment: { msg: "unused assignment", severity: Warning },
        TrailingSemi: { msg: "unnecessary trailing semicolon", severity: Warning },
        DeadCode: { msg: "dead or unreachable code", severity: Warning },
        StructTypeParam: { msg: "unused struct type parameter", severity: Warning },
        Attribute: { msg: "unused attribute", severity: Warning },
        Function: { msg: "unused function", severity: Warning },
        StructField: { msg: "unused struct field", severity: Warning },
        FunTypeParam: { msg: "unused function type parameter", severity: Warning },
        Constant: { msg: "unused constant", severity: Warning },
        MutModifier: { msg: "unused 'mut' modifiers", severity: Warning },
        MutReference: { msg: "unused mutable reference '&mut'", severity: Warning },
        MutParam: { msg: "unused mutable reference '&mut' parameter", severity: Warning },
    ],
    Attributes: [
        Duplicate: { msg: "invalid duplicate attribute", severity: NonblockingError },
        InvalidName: { msg: "invalid attribute name", severity: NonblockingError },
        InvalidValue: { msg: "invalid attribute value", severity: NonblockingError },
        InvalidUsage: { msg: "invalid usage of known attribute", severity: NonblockingError },
        InvalidTest: { msg: "unable to generate test", severity: NonblockingError },
        InvalidBytecodeInst:
            { msg: "unknown bytecode instruction function", severity: NonblockingError },
        ValueWarning: { msg: "issue with attribute value", severity: Warning },
        AmbiguousAttributeValue: { msg: "ambiguous attribute value", severity: NonblockingError },
    ],
    Tests: [
        TestFailed: { msg: "test failure", severity: BlockingError },
    ],
    Bug: [
        BytecodeGeneration: { msg: "BYTECODE GENERATION FAILED", severity: Bug },
        BytecodeVerification: { msg: "BYTECODE VERIFICATION FAILED", severity: Bug },
        ICE: { msg: "INTERNAL COMPILER ERROR", severity: Bug },
    ],
    Editions: [
        FeatureTooNew: {
            msg: "feature is not supported in specified edition",
            severity: NonblockingError,
        },
        DeprecatedFeature: {
            msg: "feature is deprecated in specified edition",
            severity: NonblockingError,
        },
        FeatureInDevelopment: {
            msg: "feature is under active development",
            severity: BlockingError,
        }
    ],
    Migration: [
        NeedsPublic: { msg: "move 2024 migration: public struct", severity: NonblockingError },
        NeedsLetMut: { msg: "move 2024 migration: let mut", severity: NonblockingError },
        NeedsRestrictedIdentifier: { msg: "move 2024 migration: restricted identifier", severity: NonblockingError },
        NeedsGlobalQualification: { msg: "move 2024 migration: global qualification", severity: NonblockingError },
        RemoveFriend: { msg: "move 2024 migration: remove 'friend'", severity: NonblockingError },
        MakePubPackage: { msg: "move 2024 migration: make 'public(package)'", severity: NonblockingError },
        AddressRemove: { msg: "move 2024 migration: address remove", severity: NonblockingError },
        AddressAdd: { msg: "move 2024 migration: address add", severity: NonblockingError },
    ],
    IDE: [
        DotAutocomplete: { msg: "IDE dot autocomplete", severity: Note },
        MacroCallInfo: { msg: "IDE macro call info", severity: Note },
        ExpandedLambda: { msg: "IDE expanded lambda", severity: Note },
        MissingMatchArms: { msg: "IDE missing match arms", severity: Note },
        EllipsisExpansion: { msg: "IDE ellipsis expansion", severity: Note },
        PathAutocomplete: { msg: "IDE path autocomplete", severity: Note },
        StringValue: { msg: "IDE string value", severity: Note },
    ],
);

//**************************************************************************************************
// impls
//**************************************************************************************************

impl DiagnosticInfo {
    pub fn render(
        self,
        registry: &FamilyRegistry,
    ) -> (/* code */ String, /* message */ &'static str) {
        let Self {
            severity,
            category,
            code,
            family,
            message,
        } = self;
        let sev_prefix = match severity {
            Severity::BlockingError | Severity::NonblockingError => "E",
            Severity::Warning => "W",
            Severity::Note => "I",
            Severity::Bug => "ICE",
        };
        debug_assert!(category <= 99);
        let string_code = if let Some(ext) = registry.display_prefix(family) {
            format!("{ext}{sev_prefix}{category:02}{code:03}")
        } else {
            format!("{sev_prefix}{category:02}{code:03}")
        };
        (string_code, message)
    }

    pub(crate) fn set_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn category(&self) -> u8 {
        self.category
    }

    pub fn code(&self) -> u8 {
        self.code
    }

    pub fn id(&self) -> DiagnosticsID {
        (self.family, self.category, self.code)
    }

    pub fn message(&self) -> &'static str {
        self.message
    }

    pub fn is_external(&self) -> bool {
        self.family != COMPILER_FAMILY
    }

    pub fn family(&self) -> Family {
        self.family
    }
}

impl Severity {
    pub const MIN: Self = Self::Warning;
    pub const MAX: Self = Self::Bug;

    pub fn into_codespan_severity(self) -> codespan_reporting::diagnostic::Severity {
        use codespan_reporting::diagnostic::Severity as CSRSeverity;
        match self {
            Severity::Bug => CSRSeverity::Bug,
            Severity::BlockingError | Severity::NonblockingError => CSRSeverity::Error,
            Severity::Warning => CSRSeverity::Warning,
            Severity::Note => CSRSeverity::Note,
        }
    }
}

impl Default for Severity {
    fn default() -> Self {
        Self::MIN
    }
}
