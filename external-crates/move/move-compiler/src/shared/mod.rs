// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::visitor::{AbsIntVisitorObj, AbstractInterpreterVisitor},
    command_line as cli,
    diagnostics::{
        codes::{
            Category, CategoryID, Declarations, DiagnosticsID, Severity, UnusedItem, WarningFilter,
        },
        Diagnostic, Diagnostics, WarningFilters,
    },
    editions::{Edition, Flavor},
    expansion::ast as E,
    naming::ast::ModuleDefinition,
    sui_mode,
    typing::visitor::{TypingVisitor, TypingVisitorObj},
};
use clap::*;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use petgraph::{algo::astar as petgraph_astar, graphmap::DiGraphMap};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
    hash::Hash,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

pub mod ast_debug;
pub mod remembering_unique_map;
pub mod unique_map;
pub mod unique_set;

//**************************************************************************************************
// Numbers
//**************************************************************************************************

pub use move_command_line_common::parser::{
    parse_address_number as parse_address, parse_u128, parse_u16, parse_u256, parse_u32, parse_u64,
    parse_u8, NumberFormat,
};

//**************************************************************************************************
// Address
//**************************************************************************************************

pub use move_command_line_common::address::NumericalAddress;

pub fn parse_named_address(s: &str) -> anyhow::Result<(String, NumericalAddress)> {
    let before_after = s.split('=').collect::<Vec<_>>();

    if before_after.len() != 2 {
        anyhow::bail!(
            "Invalid named address assignment. Must be of the form <address_name>=<address>, but \
             found '{}'",
            s
        );
    }
    let name = before_after[0].parse()?;
    let addr = NumericalAddress::parse_str(before_after[1])
        .map_err(|err| anyhow::format_err!("{}", err))?;

    Ok((name, addr))
}

//**************************************************************************************************
// Name
//**************************************************************************************************

pub trait TName: Eq + Ord + Clone {
    type Key: Ord + Clone;
    type Loc: Copy;
    fn drop_loc(self) -> (Self::Loc, Self::Key);
    fn add_loc(loc: Self::Loc, key: Self::Key) -> Self;
    fn borrow(&self) -> (&Self::Loc, &Self::Key);
}

pub trait Identifier {
    fn value(&self) -> Symbol;
    fn loc(&self) -> Loc;
}

// TODO maybe we should intern these strings somehow
pub type Name = Spanned<Symbol>;

impl TName for Name {
    type Key = Symbol;
    type Loc = Loc;

    fn drop_loc(self) -> (Loc, Symbol) {
        (self.loc, self.value)
    }

    fn add_loc(loc: Loc, key: Symbol) -> Self {
        sp(loc, key)
    }

    fn borrow(&self) -> (&Loc, &Symbol) {
        (&self.loc, &self.value)
    }
}

//**************************************************************************************************
// Graphs
//**************************************************************************************************

pub fn shortest_cycle<'a, T: Ord + Hash>(
    dependency_graph: &DiGraphMap<&'a T, ()>,
    start: &'a T,
) -> Vec<&'a T> {
    let shortest_path = dependency_graph
        .neighbors(start)
        .fold(None, |shortest_path, neighbor| {
            let path_opt = petgraph_astar(
                dependency_graph,
                neighbor,
                |finish| finish == start,
                |_e| 1,
                |_| 0,
            );
            match (shortest_path, path_opt) {
                (p, None) | (None, p) => p,
                (Some((acc_len, acc_path)), Some((cur_len, cur_path))) => {
                    Some(if cur_len < acc_len {
                        (cur_len, cur_path)
                    } else {
                        (acc_len, acc_path)
                    })
                }
            }
        });
    let (_, mut path) = shortest_path.unwrap();
    path.insert(0, start);
    path
}

//**************************************************************************************************
// Compilation Env
//**************************************************************************************************

pub const FILTER_ALL: &str = "all";
pub const FILTER_UNUSED: &str = "unused";
pub const FILTER_MISSING_PHANTOM: &str = "missing_phantom";
pub const FILTER_UNUSED_USE: &str = "unused_use";
pub const FILTER_UNUSED_VARIABLE: &str = "unused_variable";
pub const FILTER_UNUSED_ASSIGNMENT: &str = "unused_assignment";
pub const FILTER_UNUSED_TRAILING_SEMI: &str = "unused_trailing_semi";
pub const FILTER_UNUSED_ATTRIBUTE: &str = "unused_attribute";
pub const FILTER_UNUSED_TYPE_PARAMETER: &str = "unused_type_parameter";
pub const FILTER_UNUSED_FUNCTION: &str = "unused_function";
pub const FILTER_DEAD_CODE: &str = "dead_code";

pub type NamedAddressMap = BTreeMap<Symbol, NumericalAddress>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamedAddressMapIndex(usize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedAddressMaps(Vec<NamedAddressMap>);

impl NamedAddressMaps {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn insert(&mut self, m: NamedAddressMap) -> NamedAddressMapIndex {
        let index = self.0.len();
        self.0.push(m);
        NamedAddressMapIndex(index)
    }

    pub fn get(&self, idx: NamedAddressMapIndex) -> &NamedAddressMap {
        &self.0[idx.0]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackagePaths<Path: Into<Symbol> = Symbol, NamedAddress: Into<Symbol> = Symbol> {
    pub name: Option<(Symbol, PackageConfig)>,
    pub paths: Vec<Path>,
    pub named_address_map: BTreeMap<NamedAddress, NumericalAddress>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedPackagePath {
    pub package: Option<Symbol>,
    pub path: Symbol,
    pub named_address_map: NamedAddressMapIndex,
}

pub type AttributeDeriver = dyn Fn(&mut CompilationEnv, &mut ModuleDefinition);

/// Filter info for example filter #[allow(unused_function)] would have `name` to be
/// `unused_function` and `attribute_name` to be `allow`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KnownFilterInfo {
    name: Symbol,
    attribute_name: E::AttributeName_,
}

impl KnownFilterInfo {
    pub fn new(n: &str, attribute_name: E::AttributeName_) -> Self {
        let name = Symbol::from(n);
        KnownFilterInfo {
            name,
            attribute_name,
        }
    }
}

pub struct CompilationEnv {
    flags: Flags,
    // filters warnings when added.
    warning_filter: Vec<WarningFilters>,
    diags: Diagnostics,
    visitors: Rc<Visitors>,
    package_configs: BTreeMap<Symbol, PackageConfig>,
    /// Config for any package not found in `package_configs`, or for inputs without a package.
    default_config: PackageConfig,
    /// Maps warning filter key (filter name and filter attribute name) to the filter itself.
    known_filters: BTreeMap<KnownFilterInfo, WarningFilter>,
    /// Maps a diagnostics ID to a known filter name.
    known_filter_names: BTreeMap<DiagnosticsID, KnownFilterInfo>,
    /// Attribute names (including externally provided ones) identifying known warning filters.
    known_filter_attributes: BTreeSet<E::AttributeName_>,
    // TODO(tzakian): Remove the global counter and use this counter instead
    // pub counter: u64,
}

macro_rules! known_code_filter {
    ($name:ident, $category:ident::$code:ident, $attr_name:ident) => {
        (
            KnownFilterInfo::new($name, $attr_name),
            WarningFilter::Code(
                DiagnosticsID::new(Category::$category as u8, $category::$code as u8, None),
                Some($name),
            ),
        )
    };
}

impl CompilationEnv {
    pub fn new(
        flags: Flags,
        mut visitors: Vec<cli::compiler::Visitor>,
        package_configs: BTreeMap<Symbol, PackageConfig>,
        default_config: Option<PackageConfig>,
    ) -> Self {
        visitors.extend([
            sui_mode::id_leak::IDLeakVerifier.visitor(),
            sui_mode::typing::SuiTypeChecks.visitor(),
        ]);
        let filter_attr_name =
            E::AttributeName_::Known(known_attributes::KnownAttribute::Diagnostic(
                known_attributes::DiagnosticAttribute::Allow,
            ));
        let filter_attributes = BTreeSet::from([filter_attr_name]);
        let known_filters = BTreeMap::from([
            (
                KnownFilterInfo::new(FILTER_ALL, filter_attr_name),
                WarningFilter::All(None),
            ),
            (
                KnownFilterInfo::new(FILTER_UNUSED, filter_attr_name),
                WarningFilter::Category(
                    CategoryID::new(Category::UnusedItem as u8, None),
                    Some(FILTER_UNUSED),
                ),
            ),
            known_code_filter!(
                FILTER_MISSING_PHANTOM,
                Declarations::InvalidNonPhantomUse,
                filter_attr_name
            ),
            known_code_filter!(FILTER_UNUSED_USE, UnusedItem::Alias, filter_attr_name),
            known_code_filter!(
                FILTER_UNUSED_VARIABLE,
                UnusedItem::Variable,
                filter_attr_name
            ),
            known_code_filter!(
                FILTER_UNUSED_ASSIGNMENT,
                UnusedItem::Assignment,
                filter_attr_name
            ),
            known_code_filter!(
                FILTER_UNUSED_TRAILING_SEMI,
                UnusedItem::TrailingSemi,
                filter_attr_name
            ),
            known_code_filter!(
                FILTER_UNUSED_ATTRIBUTE,
                UnusedItem::Attribute,
                filter_attr_name
            ),
            known_code_filter!(
                FILTER_UNUSED_TYPE_PARAMETER,
                UnusedItem::StructTypeParam,
                filter_attr_name
            ),
            known_code_filter!(
                FILTER_UNUSED_FUNCTION,
                UnusedItem::Function,
                filter_attr_name
            ),
            known_code_filter!(FILTER_DEAD_CODE, UnusedItem::DeadCode, filter_attr_name),
        ]);

        let known_filter_names: BTreeMap<DiagnosticsID, KnownFilterInfo> = known_filters
            .iter()
            .filter_map(|(known_filter_info, v)| {
                if let WarningFilter::Code(diag_id, _) = v {
                    Some((*diag_id, known_filter_info.clone()))
                } else {
                    None
                }
            })
            .collect();

        Self {
            flags,
            warning_filter: vec![],
            diags: Diagnostics::new(),
            visitors: Rc::new(Visitors::new(visitors)),
            package_configs,
            default_config: default_config.unwrap_or_default(),
            known_filters,
            known_filter_names,
            known_filter_attributes: filter_attributes,
        }
    }

    pub fn add_diag(&mut self, mut diag: Diagnostic) {
        let is_filtered = self
            .warning_filter
            .last()
            .map(|filter| filter.is_filtered(&diag))
            .unwrap_or(false);
        if !is_filtered {
            // add help to suppress warning, if applicable
            // TODO do we want a centralized place for tips like this?
            if diag.info().severity() == Severity::Warning {
                if let Some(filter_info) = self.known_filter_names.get(&diag.info().id()) {
                    //                    if let Some(filter_attr_name) =
                    let help = format!(
                        "This warning can be suppressed with '#[{}({})]' \
                         applied to the 'module' or module member ('const', 'fun', or 'struct')",
                        filter_info.attribute_name.name(),
                        filter_info.name.as_str()
                    );
                    diag.add_note(help)
                }
            }
            self.diags.add(diag)
        }
    }

    pub fn add_diags(&mut self, diags: Diagnostics) {
        for diag in diags.into_vec() {
            self.add_diag(diag)
        }
    }

    pub fn has_warnings_or_errors(&self) -> bool {
        !self.diags.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        // Non-blocking Error is the min level considered an error
        self.has_diags_at_or_above_severity(Severity::NonblockingError)
    }

    pub fn count_diags(&self) -> usize {
        self.diags.len()
    }

    pub fn has_diags_at_or_above_severity(&self, threshold: Severity) -> bool {
        match self.diags.max_severity() {
            Some(max) if max >= threshold => true,
            Some(_) | None => false,
        }
    }

    pub fn check_diags_at_or_above_severity(
        &mut self,
        threshold: Severity,
    ) -> Result<(), Diagnostics> {
        if self.has_diags_at_or_above_severity(threshold) {
            Err(std::mem::take(&mut self.diags))
        } else {
            Ok(())
        }
    }

    /// Should only be called after compilation is finished
    pub fn take_final_warning_diags(&mut self) -> Diagnostics {
        let final_diags = std::mem::take(&mut self.diags);
        debug_assert!(final_diags
            .max_severity()
            .map(|s| s == Severity::Warning)
            .unwrap_or(true));
        final_diags
    }

    /// Add a new filter for warnings
    pub fn add_warning_filter_scope(&mut self, mut filter: WarningFilters) {
        // This essentially "clones" the current filter into the next scope. This should be
        // efficient enough since the diag_filter vec should be only about 2 or 3 elements deep
        // and the size of the filter should only be relatively small (at most 10 or so elements)
        debug_assert!(
            self.warning_filter.len() <= 3,
            "TODO If triggered this TODO you might want to make this more efficient"
        );
        if let Some(cur_filter) = self.warning_filter.last() {
            filter.union(&cur_filter)
        }
        self.warning_filter.push(filter)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.warning_filter.pop().unwrap();
    }

    pub fn filter_from_str(
        &self,
        name: String,
        attribute_name: E::AttributeName_,
    ) -> Option<WarningFilter> {
        self.known_filters
            .get(&KnownFilterInfo::new(name.as_str(), attribute_name))
            .cloned()
    }

    pub fn filter_attributes(&self) -> &BTreeSet<E::AttributeName_> {
        &self.known_filter_attributes
    }

    pub fn add_custom_known_filters(
        &mut self,
        filters: Vec<WarningFilter>,
        filter_attr_name: E::AttributeName_,
    ) -> anyhow::Result<()> {
        self.known_filter_attributes.insert(filter_attr_name);
        for filter in filters {
            match filter {
                WarningFilter::All(_) => {
                    self.known_filters
                        .insert(KnownFilterInfo::new(FILTER_ALL, filter_attr_name), filter);
                }
                WarningFilter::Category(_, name) => {
                    let Some(n) = name else {
                        anyhow::bail!("A known Category warning filter must have a name specified");
                    };
                    self.known_filters
                        .insert(KnownFilterInfo::new(n, filter_attr_name), filter);
                }
                WarningFilter::Code(diag_id, name) => {
                    let Some(n) = name else {
                        anyhow::bail!("A known Code warning filter must have a name specified");
                    };
                    let known_filter_info = KnownFilterInfo::new(n, filter_attr_name);
                    self.known_filters.insert(known_filter_info.clone(), filter);
                    self.known_filter_names.insert(diag_id, known_filter_info);
                }
            }
        }
        Ok(())
    }

    pub fn flags(&self) -> &Flags {
        &self.flags
    }

    pub fn visitors(&self) -> Rc<Visitors> {
        self.visitors.clone()
    }

    pub fn package_config(&self, package: Option<Symbol>) -> &PackageConfig {
        package
            .and_then(|p| self.package_configs.get(&p))
            .unwrap_or(&self.default_config)
    }
}

//**************************************************************************************************
// Counter
//**************************************************************************************************

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Counter(usize);

impl Counter {
    pub fn next() -> u64 {
        static COUNTER_NEXT: AtomicUsize = AtomicUsize::new(0);

        COUNTER_NEXT.fetch_add(1, AtomicOrdering::AcqRel) as u64
    }
}

//**************************************************************************************************
// Display
//**************************************************************************************************

pub fn format_delim<T: fmt::Display, I: IntoIterator<Item = T>>(items: I, delim: &str) -> String {
    items
        .into_iter()
        .map(|item| format!("{}", item))
        .collect::<Vec<_>>()
        .join(delim)
}

pub fn format_comma<T: fmt::Display, I: IntoIterator<Item = T>>(items: I) -> String {
    format_delim(items, ", ")
}

//**************************************************************************************************
// Flags
//**************************************************************************************************

#[derive(Clone, Debug, Eq, PartialEq, Parser)]
pub struct Flags {
    /// Compile in test mode
    #[clap(
        short = cli::TEST_SHORT,
        long = cli::TEST,
    )]
    test: bool,

    /// Compile in verification mode
    #[clap(
    short = cli::VERIFY_SHORT,
    long = cli::VERIFY,
    )]
    verify: bool,

    /// Bytecode version.
    #[clap(
        long = cli::BYTECODE_VERSION,
    )]
    bytecode_version: Option<u32>,

    /// If set, source files will not shadow dependency files. If the same file is passed to both,
    /// an error will be raised
    #[clap(
        name = "SOURCES_SHADOW_DEPS",
        short = cli::SHADOW_SHORT,
        long = cli::SHADOW,
    )]
    shadow: bool,

    /// Internal flag used by the model builder to maintain functions which would be otherwise
    /// included only in tests, without creating the unit test code regular tests do.
    #[clap(skip)]
    keep_testing_functions: bool,
}

impl Flags {
    pub fn empty() -> Self {
        Self {
            test: false,
            verify: false,
            shadow: false,
            bytecode_version: None,
            keep_testing_functions: false,
        }
    }

    pub fn testing() -> Self {
        Self {
            test: true,
            verify: false,
            shadow: false,
            bytecode_version: None,
            keep_testing_functions: false,
        }
    }

    pub fn verification() -> Self {
        Self {
            test: false,
            verify: true,
            shadow: true, // allows overlapping between sources and deps
            bytecode_version: None,
            keep_testing_functions: false,
        }
    }

    pub fn set_keep_testing_functions(self, value: bool) -> Self {
        Self {
            keep_testing_functions: value,
            ..self
        }
    }

    pub fn set_sources_shadow_deps(self, sources_shadow_deps: bool) -> Self {
        Self {
            shadow: sources_shadow_deps,
            ..self
        }
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::empty()
    }

    pub fn is_testing(&self) -> bool {
        self.test
    }

    pub fn keep_testing_functions(&self) -> bool {
        self.test || self.keep_testing_functions
    }

    pub fn is_verification(&self) -> bool {
        self.verify
    }

    pub fn sources_shadow_deps(&self) -> bool {
        self.shadow
    }

    pub fn bytecode_version(&self) -> Option<u32> {
        self.bytecode_version
    }
}

//**************************************************************************************************
// Package Level Config
//**************************************************************************************************

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct PackageConfig {
    pub warning_filter: WarningFilters,
    pub flavor: Flavor,
    pub edition: Edition,
}

impl Default for PackageConfig {
    fn default() -> Self {
        Self {
            warning_filter: WarningFilters::Empty,
            flavor: Flavor::default(),
            edition: Edition::default(),
        }
    }
}

//**************************************************************************************************
// Visitors
//**************************************************************************************************

pub struct Visitors {
    pub typing: Vec<RefCell<TypingVisitorObj>>,
    pub abs_int: Vec<RefCell<AbsIntVisitorObj>>,
}

impl Visitors {
    pub fn new(passes: Vec<cli::compiler::Visitor>) -> Self {
        use cli::compiler::Visitor;
        let mut vs = Visitors {
            typing: vec![],
            abs_int: vec![],
        };
        for pass in passes {
            match pass {
                Visitor::AbsIntVisitor(f) => vs.abs_int.push(RefCell::new(f)),
                Visitor::TypingVisitor(f) => vs.typing.push(RefCell::new(f)),
            }
        }
        vs
    }
}

//**************************************************************************************************
// Attributes
//**************************************************************************************************

pub mod known_attributes {
    use once_cell::sync::Lazy;
    use std::{collections::BTreeSet, fmt};

    use crate::diagnostics::codes::WARNING_FILTER_ATTR;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum AttributePosition {
        AddressBlock,
        Module,
        Script,
        Use,
        Friend,
        Constant,
        Struct,
        Function,
        Spec,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum KnownAttribute {
        Testing(TestingAttribute),
        Verification(VerificationAttribute),
        Native(NativeAttribute),
        Diagnostic(DiagnosticAttribute),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum TestingAttribute {
        // Can be called by other testing code, and included in compilation in test mode
        TestOnly,
        // Is a test that will be run
        Test,
        // This test is expected to fail
        ExpectedFailure,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    pub enum VerificationAttribute {
        // The associated AST node will be included in the compilation in prove mode
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
    }

    impl fmt::Display for AttributePosition {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::AddressBlock => write!(f, "address block"),
                Self::Module => write!(f, "module"),
                Self::Script => write!(f, "script"),
                Self::Use => write!(f, "use"),
                Self::Friend => write!(f, "friend"),
                Self::Constant => write!(f, "constant"),
                Self::Struct => write!(f, "struct"),
                Self::Function => write!(f, "function"),
                Self::Spec => write!(f, "spec"),
            }
        }
    }

    impl KnownAttribute {
        pub fn resolve(attribute_str: impl AsRef<str>) -> Option<Self> {
            Some(match attribute_str.as_ref() {
                TestingAttribute::TEST => Self::Testing(TestingAttribute::Test),
                TestingAttribute::TEST_ONLY => Self::Testing(TestingAttribute::TestOnly),
                TestingAttribute::EXPECTED_FAILURE => {
                    Self::Testing(TestingAttribute::ExpectedFailure)
                }
                VerificationAttribute::VERIFY_ONLY => {
                    Self::Verification(VerificationAttribute::VerifyOnly)
                }
                NativeAttribute::BYTECODE_INSTRUCTION => {
                    Self::Native(NativeAttribute::BytecodeInstruction)
                }
                DiagnosticAttribute::ALLOW => Self::Diagnostic(DiagnosticAttribute::Allow),
                _ => return None,
            })
        }

        pub const fn name(&self) -> &str {
            match self {
                Self::Testing(a) => a.name(),
                Self::Verification(a) => a.name(),
                Self::Native(a) => a.name(),
                Self::Diagnostic(a) => a.name(),
            }
        }

        pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
            match self {
                Self::Testing(a) => a.expected_positions(),
                Self::Verification(a) => a.expected_positions(),
                Self::Native(a) => a.expected_positions(),
                Self::Diagnostic(a) => a.expected_positions(),
            }
        }
    }

    impl TestingAttribute {
        pub const TEST: &'static str = "test";
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
                    AttributePosition::Function,
                ])
            });
            static TEST_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
                Lazy::new(|| BTreeSet::from([AttributePosition::Function]));
            static EXPECTED_FAILURE_POSITIONS: Lazy<BTreeSet<AttributePosition>> =
                Lazy::new(|| BTreeSet::from([AttributePosition::Function]));
            match self {
                TestingAttribute::TestOnly => &TEST_ONLY_POSITIONS,
                TestingAttribute::Test => &TEST_POSITIONS,
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
        pub const ALLOW: &'static str = WARNING_FILTER_ATTR;

        pub const fn name(&self) -> &str {
            match self {
                DiagnosticAttribute::Allow => Self::ALLOW,
            }
        }

        pub fn expected_positions(&self) -> &'static BTreeSet<AttributePosition> {
            static ALLOW_WARNING_POSITIONS: Lazy<BTreeSet<AttributePosition>> = Lazy::new(|| {
                BTreeSet::from([
                    AttributePosition::Module,
                    AttributePosition::Script,
                    AttributePosition::Constant,
                    AttributePosition::Struct,
                    AttributePosition::Function,
                ])
            });
            match self {
                DiagnosticAttribute::Allow => &ALLOW_WARNING_POSITIONS,
            }
        }
    }
}
