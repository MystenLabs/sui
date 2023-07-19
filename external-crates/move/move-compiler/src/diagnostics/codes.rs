// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//**************************************************************************************************
// Main types
//**************************************************************************************************

#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
pub enum Severity {
    Warning = 0,
    NonblockingError = 1,
    BlockingError = 2,
    Bug = 3,
}

/// A an optional prefix to distinguish between different types of warnings (internal vs. possibly
/// multiple externally provided ones).
type ExternalPrefix = Option<&'static str>;

/// Identifies a warning category. Includes an external prefix to distinguish warnings from
/// different sources.
#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
pub struct CategoryID {
    category: u8,
    external_prefix: ExternalPrefix,
}

/// Identifies a warning diagnostic through a warning category ID and a warning code.
#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash, PartialOrd, Ord)]
pub struct DiagnosticsID {
    category_id: CategoryID,
    code: u8,
}

#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct DiagnosticInfo {
    severity: Severity,
    id: DiagnosticsID,
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
            id: DiagnosticsID::new(category, code, None),
            message,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
/// Represents a single annotation for a diagnostic filter
pub enum WarningFilter {
    /// Filters all warnings
    All(ExternalPrefix),
    /// Filters all warnings of a specific category. Only known filters have names.
    Category(CategoryID, /* name */ Option<&'static str>),
    /// Filters a single warning, as defined by codes below. Only known filters have names.
    Code(DiagnosticsID, /* name */ Option<&'static str>),
}

/// The text used in the attribute for warning suppression
pub const WARNING_FILTER_ATTR: &str = "allow";

//**************************************************************************************************
// Categories and Codes
//**************************************************************************************************

/// A custom DiagnosticInfo.
/// The diagnostic will get rendered as
/// `"[{external_prefix}{severity}{category}{code}] {message}"`.
/// Note, this will will panic if `category > 99`
pub const fn custom(
    external_prefix: &'static str,
    severity: Severity,
    category: u8,
    code: u8,
    message: &'static str,
) -> DiagnosticInfo {
    assert!(category <= 99);
    DiagnosticInfo {
        severity,
        id: DiagnosticsID {
            category_id: CategoryID {
                category,
                external_prefix: Some(external_prefix),
            },
            code,
        },
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
        UnboundField: { msg: "unbound field", severity: BlockingError },
        ReservedName: { msg: "invalid use of reserved name", severity: BlockingError },
        UnboundMacro: { msg: "unbound macro", severity: BlockingError },
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
        TooFewArguments: { msg: "too few arguments", severity: BlockingError },
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
    BytecodeGeneration: [
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
    ],
    Attributes: [
        Duplicate: { msg: "invalid duplicate attribute", severity: NonblockingError },
        InvalidName: { msg: "invalid attribute name", severity: NonblockingError },
        InvalidValue: { msg: "invalid attribute value", severity: NonblockingError },
        InvalidUsage: { msg: "invalid usage of known attribute", severity: NonblockingError },
        InvalidTest: { msg: "unable to generate test", severity: NonblockingError },
        InvalidBytecodeInst:
            { msg: "unknown bytecode instruction function", severity: NonblockingError },
        ValueWarning: { msg: "potential issue with attribute value", severity: Warning }
    ],
    Tests: [
        TestFailed: { msg: "test failure", severity: BlockingError },
    ],
    Bug: [
        BytecodeGeneration: { msg: "BYTECODE GENERATION FAILED", severity: Bug },
        BytecodeVerification: { msg: "BYTECODE VERIFICATION FAILED", severity: Bug },
    ],
    Editions: [
        FeatureTooNew: {
            msg: "feature is not supported in specified edition",
            severity: BlockingError,
        },
    ]
);

//**************************************************************************************************
// Warning Filter
//**************************************************************************************************

impl WarningFilter {
    pub fn to_str(self) -> Option<&'static str> {
        match self {
            Self::All(_) => Some("all"),
            Self::Category(_, n) => n,
            Self::Code(_, n) => n,
        }
    }
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl CategoryID {
    pub fn new(category: u8, external_prefix: ExternalPrefix) -> Self {
        CategoryID {
            category,
            external_prefix,
        }
    }

    pub fn category(&self) -> u8 {
        self.category
    }

    pub fn external_prefix(&self) -> Option<&'static str> {
        self.external_prefix
    }
}

impl DiagnosticsID {
    pub fn new(category: u8, code: u8, external_prefix: ExternalPrefix) -> Self {
        let category_id = CategoryID {
            category,
            external_prefix,
        };
        DiagnosticsID { category_id, code }
    }

    pub fn category(&self) -> u8 {
        self.category_id.category
    }

    pub fn code(&self) -> u8 {
        self.code
    }

    pub fn external_prefix(&self) -> ExternalPrefix {
        self.category_id.external_prefix
    }

    pub fn category_id(&self) -> CategoryID {
        self.category_id
    }
}

impl DiagnosticInfo {
    pub fn render(self) -> (/* code */ String, /* message */ &'static str) {
        let Self {
            severity,
            id:
                DiagnosticsID {
                    category_id:
                        CategoryID {
                            category,
                            external_prefix,
                        },
                    code,
                },
            message,
        } = self;
        let sev_prefix = match severity {
            Severity::BlockingError | Severity::NonblockingError => "E",
            Severity::Warning => "W",
            Severity::Bug => "ICE",
        };
        debug_assert!(category <= 99);
        let string_code = if let Some(ext) = external_prefix {
            format!("{ext}{sev_prefix}{category:02}{code:03}")
        } else {
            format!("{sev_prefix}{category:02}{code:03}")
        };
        (string_code, message)
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn category(&self) -> u8 {
        self.id.category_id.category
    }

    pub fn code(&self) -> u8 {
        self.id.code
    }

    pub fn message(&self) -> &'static str {
        self.message
    }

    pub fn is_external(&self) -> bool {
        self.id.category_id.external_prefix.is_some()
    }

    pub fn id(&self) -> DiagnosticsID {
        self.id
    }

    pub fn category_id(&self) -> CategoryID {
        self.id.category_id
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
        }
    }
}

impl Default for Severity {
    fn default() -> Self {
        Self::MIN
    }
}
