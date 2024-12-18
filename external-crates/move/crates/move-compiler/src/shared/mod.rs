// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{
        ast as G,
        visitor::{AbsIntVisitorObj, AbstractInterpreterVisitor, CFGIRVisitorObj},
    },
    command_line as cli,
    diagnostics::{
        codes::{DiagnosticsID, Severity},
        warning_filters::{
            FilterName, FilterPrefix, WarningFilter, WarningFiltersBuilder, WarningFiltersScope,
            WarningFiltersTable, FILTER_ALL,
        },
        DiagnosticReporter, Diagnostics, DiagnosticsFormat,
    },
    editions::{check_feature_or_error, feature_edition_error_msg, Edition, FeatureGate, Flavor},
    expansion::ast as E,
    hlir::ast as H,
    naming::ast as N,
    parser::ast as P,
    shared::{
        files::{FileName, MappedFiles},
        ide::IDEInfo,
    },
    sui_mode,
    typing::{
        ast as T,
        visitor::{TypingVisitor, TypingVisitorObj},
    },
};
use clap::*;
use move_command_line_common::files::FileHash;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use petgraph::{algo::astar as petgraph_astar, graphmap::DiGraphMap};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    hash::Hash,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
        Arc, Mutex, OnceLock, RwLock,
    },
};
use vfs::{VfsError, VfsPath};

pub mod ast_debug;
pub mod files;
pub mod ide;
pub mod known_attributes;
pub mod matching;
pub mod program_info;
pub mod remembering_unique_map;
pub mod string_utils;
pub mod unique_map;
pub mod unique_set;

pub use ast_debug::AstDebug;

//**************************************************************************************************
// Numbers
//**************************************************************************************************

pub use move_core_types::parsing::parser::{
    parse_address_number as parse_address, parse_u128, parse_u16, parse_u256, parse_u32, parse_u64,
    parse_u8, NumberFormat,
};

//**************************************************************************************************
// Address
//**************************************************************************************************

pub use move_core_types::parsing::address::NumericalAddress;

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
    fn with_loc(self, loc: Self::Loc) -> Self {
        let (_old_loc, base) = self.drop_loc();
        Self::add_loc(loc, base)
    }
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

pub type NamedAddressMap = BTreeMap<Symbol, NumericalAddress>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamedAddressMapIndex(usize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedAddressMaps(Vec<NamedAddressMap>);

impl Default for NamedAddressMaps {
    fn default() -> Self {
        Self::new()
    }
}

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

    pub fn all(&self) -> &[NamedAddressMap] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackagePaths<Path: Into<Symbol> = Symbol, NamedAddress: Into<Symbol> = Symbol> {
    pub name: Option<(Symbol, PackageConfig)>,
    pub paths: Vec<Path>,
    pub named_address_map: BTreeMap<NamedAddress, NumericalAddress>,
}

pub struct CompilationEnv {
    flags: Flags,
    top_level_warning_filter_scope: Option<&'static WarningFiltersBuilder>,
    diags: RwLock<Diagnostics>,
    visitors: Visitors,
    package_configs: BTreeMap<Symbol, PackageConfig>,
    /// Config for any package not found in `package_configs`, or for inputs without a package.
    default_config: PackageConfig,
    /// Maps warning filter key (filter name and filter attribute name) to the filter itself.
    known_filters: BTreeMap<FilterPrefix, BTreeMap<FilterName, BTreeSet<WarningFilter>>>,
    /// Maps a diagnostics ID to a known filter name.
    known_filter_names: BTreeMap<DiagnosticsID, (FilterPrefix, FilterName)>,
    prim_definers: OnceLock<BTreeMap<N::BuiltinTypeName_, E::ModuleIdent>>,
    // TODO(tzakian): Remove the global counter and use this counter instead
    // pub counter: u64,
    mapped_files: MappedFiles,
    save_hooks: Vec<SaveHook>,
    ide_information: RwLock<IDEInfo>,
    // Files to fully compile (as opposed to omitting function bodies)
    files_to_compile: Option<BTreeSet<PathBuf>>,
}

impl CompilationEnv {
    pub fn new(
        flags: Flags,
        mut visitors: Vec<cli::compiler::Visitor>,
        save_hooks: Vec<SaveHook>,
        warning_filters: Option<WarningFiltersBuilder>,
        package_configs: BTreeMap<Symbol, PackageConfig>,
        default_config: Option<PackageConfig>,
        files_to_compile: Option<BTreeSet<PathBuf>>,
    ) -> Self {
        visitors.extend([
            sui_mode::id_leak::IDLeakVerifier.visitor(),
            sui_mode::typing::SuiTypeChecks.visitor(),
        ]);
        let mut known_filters_: BTreeMap<FilterName, BTreeSet<WarningFilter>> =
            WarningFilter::compiler_known_filters();
        if flags.ide_mode() {
            known_filters_.extend(WarningFilter::ide_known_filters());
        }
        let known_filters: BTreeMap<FilterPrefix, BTreeMap<FilterName, BTreeSet<WarningFilter>>> =
            BTreeMap::from([(None, known_filters_)]);

        let known_filter_names: BTreeMap<DiagnosticsID, (FilterPrefix, FilterName)> = known_filters
            .iter()
            .flat_map(|(attr, all_filters)| {
                all_filters.iter().flat_map(|(name, filters)| {
                    filters.iter().filter_map(|v| {
                        if let WarningFilter::Code {
                            prefix,
                            category,
                            code,
                            ..
                        } = v
                        {
                            Some(((*prefix, *category, *code), (*attr, *name)))
                        } else {
                            None
                        }
                    })
                })
            })
            .collect();

        let top_level_warning_filter_opt = if flags.silence_warnings() {
            let mut f = WarningFiltersBuilder::new_for_source();
            f.add(WarningFilter::All(None));
            Some(f)
        } else {
            warning_filters
        };
        let top_level_warning_filter_scope: Option<&'static WarningFiltersBuilder> =
            top_level_warning_filter_opt.map(|f| {
                let f: &'static WarningFiltersBuilder = Box::leak(Box::new(f));
                f
            });
        let mut diags = Diagnostics::new();
        if flags.json_errors() {
            diags.set_format(DiagnosticsFormat::JSON);
        }
        Self {
            flags,
            top_level_warning_filter_scope,
            diags: RwLock::new(diags),
            visitors: Visitors::new(visitors),
            package_configs,
            default_config: default_config.unwrap_or_default(),
            known_filters,
            known_filter_names,
            prim_definers: OnceLock::new(),
            mapped_files: MappedFiles::empty(),
            save_hooks,
            ide_information: RwLock::new(IDEInfo::new()),
            files_to_compile,
        }
    }

    pub fn add_source_file(
        &mut self,
        file_hash: FileHash,
        file_name: FileName,
        source_text: Arc<str>,
    ) {
        self.mapped_files.add(file_hash, file_name, source_text)
    }

    pub fn mapped_files(&self) -> &MappedFiles {
        &self.mapped_files
    }

    pub fn diagnostic_reporter_at_top_level(&self) -> DiagnosticReporter {
        DiagnosticReporter::new(
            &self.flags,
            &self.known_filter_names,
            &self.diags,
            &self.ide_information,
            WarningFiltersScope::root(self.top_level_warning_filter_scope),
        )
    }

    pub fn has_warnings_or_errors(&self) -> bool {
        !self.diags.read().unwrap().is_empty()
    }

    pub fn has_errors(&self) -> bool {
        // Non-blocking Error is the min level considered an error
        self.has_diags_at_or_above_severity(Severity::NonblockingError)
    }

    pub fn count_diags(&self) -> usize {
        self.diags.read().unwrap().len()
    }

    pub fn count_diags_at_or_above_severity(&self, threshold: Severity) -> usize {
        self.diags
            .read()
            .unwrap()
            .count_diags_at_or_above_severity(threshold)
    }

    pub fn has_diags_at_or_above_severity(&self, threshold: Severity) -> bool {
        self.diags
            .read()
            .unwrap()
            .max_severity_at_or_above_severity(threshold)
    }

    pub fn check_diags_at_or_above_severity(&self, threshold: Severity) -> Result<(), Diagnostics> {
        if self.has_diags_at_or_above_severity(threshold) {
            let diagnostics: &mut Diagnostics = &mut self.diags.write().unwrap();
            Err(std::mem::take(diagnostics))
        } else {
            Ok(())
        }
    }

    /// Should only be called after compilation is finished
    pub fn take_final_diags(&self) -> Diagnostics {
        let diagnostics: &mut Diagnostics = &mut self.diags.write().unwrap();
        std::mem::take(diagnostics)
    }

    /// Should only be called after compilation is finished
    pub fn take_final_warning_diags(&self) -> Diagnostics {
        let final_diags = self.take_final_diags();
        debug_assert!(final_diags.max_severity_at_or_under_severity(Severity::Warning));
        final_diags
    }

    pub fn known_filter_names(&self) -> impl IntoIterator<Item = FilterPrefix> + '_ {
        self.known_filters.keys().copied()
    }

    pub fn filter_from_str(
        &self,
        prefix: Option<impl Into<Symbol>>,
        name: impl Into<Symbol>,
    ) -> BTreeSet<WarningFilter> {
        self.known_filters
            .get(&prefix.map(|p| p.into()))
            .and_then(|filters| filters.get(&name.into()).cloned())
            .unwrap_or_default()
    }

    pub fn add_custom_known_filters(
        &mut self,
        attr_name: FilterPrefix,
        filters: Vec<WarningFilter>,
    ) -> anyhow::Result<()> {
        let filter_attr = self.known_filters.entry(attr_name).or_default();
        for filter in filters {
            let (prefix, n) = match filter {
                WarningFilter::All(prefix) => (prefix, Symbol::from(FILTER_ALL)),
                WarningFilter::Category { name, prefix, .. } => {
                    let Some(n) = name else {
                        anyhow::bail!("A known Category warning filter must have a name specified");
                    };
                    (prefix, Symbol::from(n))
                }
                WarningFilter::Code {
                    prefix,
                    category,
                    code,
                    name,
                } => {
                    let Some(n) = name else {
                        anyhow::bail!("A known Code warning filter must have a name specified");
                    };
                    let n = Symbol::from(n);
                    self.known_filter_names
                        .insert((prefix, category, code), (attr_name, n));
                    (prefix, n)
                }
            };
            anyhow::ensure!(
                attr_name.is_some() == prefix.is_some(),
                "If the attribute name is specified, e.g. Some(_), the external prefix must also \
                be specified. attribute name: {attr_name:?}, external prefix: {prefix:?}",
            );
            filter_attr.entry(n).or_default().insert(filter);
        }
        Ok(())
    }

    pub fn flags(&self) -> &Flags {
        &self.flags
    }

    pub fn visitors(&self) -> &Visitors {
        &self.visitors
    }

    pub fn files_to_compile(&self) -> Option<&BTreeSet<PathBuf>> {
        self.files_to_compile.as_ref()
    }

    // Logs an error if the feature isn't supported. Returns `false` if the feature is not
    // supported, and `true` otherwise.
    pub fn check_feature(
        &self,
        reporter: &DiagnosticReporter,
        package: Option<Symbol>,
        feature: FeatureGate,
        loc: Loc,
    ) -> bool {
        check_feature_or_error(reporter, self.package_config(package).edition, feature, loc)
    }

    // Returns an error string if if the feature isn't supported, or None otherwise.
    pub fn feature_edition_error_msg(
        &self,
        feature: FeatureGate,
        package: Option<Symbol>,
    ) -> Option<String> {
        feature_edition_error_msg(self.package_config(package).edition, feature)
    }

    pub fn supports_feature(&self, package: Option<Symbol>, feature: FeatureGate) -> bool {
        self.package_config(package).edition.supports(feature)
    }

    pub fn edition(&self, package: Option<Symbol>) -> Edition {
        self.package_config(package).edition
    }

    pub fn package_config(&self, package: Option<Symbol>) -> &PackageConfig {
        package
            .and_then(|p| self.package_configs.get(&p))
            .unwrap_or(&self.default_config)
    }

    pub fn package_configs(&self) -> impl Iterator<Item = (Option<Symbol>, &PackageConfig)> {
        std::iter::once((None, &self.default_config)).chain(
            self.package_configs
                .iter()
                .map(|(n, config)| (Some(*n), config)),
        )
    }

    pub fn set_primitive_type_definers(&self, m: BTreeMap<N::BuiltinTypeName_, E::ModuleIdent>) {
        self.prim_definers.set(m).unwrap()
    }

    pub fn primitive_definer(&self, t: N::BuiltinTypeName_) -> Option<&E::ModuleIdent> {
        self.prim_definers.get().and_then(|m| m.get(&t))
    }

    pub fn save_parser_ast(&self, ast: &P::Program) {
        for hook in &self.save_hooks {
            hook.save_parser_ast(ast)
        }
    }

    pub fn save_expansion_ast(&self, ast: &E::Program) {
        for hook in &self.save_hooks {
            hook.save_expansion_ast(ast)
        }
    }

    pub fn save_naming_ast(&self, ast: &N::Program) {
        for hook in &self.save_hooks {
            hook.save_naming_ast(ast)
        }
    }

    pub fn save_typing_ast(&self, ast: &T::Program) {
        for hook in &self.save_hooks {
            hook.save_typing_ast(ast)
        }
    }

    pub fn save_typing_info(&self, info: &Arc<program_info::TypingProgramInfo>) {
        for hook in &self.save_hooks {
            hook.save_typing_info(info)
        }
    }

    pub fn save_hlir_ast(&self, ast: &H::Program) {
        for hook in &self.save_hooks {
            hook.save_hlir_ast(ast)
        }
    }

    pub fn save_cfgir_ast(&self, ast: &G::Program) {
        for hook in &self.save_hooks {
            hook.save_cfgir_ast(ast)
        }
    }

    // -- IDE Information --

    pub fn ide_mode(&self) -> bool {
        self.flags.ide_mode()
    }

    pub fn ide_information(&self) -> std::sync::RwLockReadGuard<'_, IDEInfo> {
        self.ide_information.read().unwrap()
    }
}

pub fn format_allow_attr(attr_name: FilterPrefix, filter: FilterName) -> String {
    match attr_name {
        None => filter.to_string(),
        Some(attr_name) => format!("{attr_name}({filter})"),
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

    /// If set, warnings become errors.
    #[clap(
        long = cli::WARNINGS_ARE_ERRORS,
    )]
    warnings_are_errors: bool,

    /// If set, report errors as json.
    #[clap(
        long = cli::JSON_ERRORS,
    )]
    json_errors: bool,

    /// If set, all warnings are silenced
    #[clap(
        long = cli::SILENCE_WARNINGS,
        short = cli::SILENCE_WARNINGS_SHORT,
    )]
    silence_warnings: bool,

    /// If set, source files will not shadow dependency files. If the same file is passed to both,
    /// an error will be raised
    #[clap(
        name = "SOURCES_SHADOW_DEPS",
        short = cli::SHADOW_SHORT,
        long = cli::SHADOW,
    )]
    shadow: bool,

    /// Bytecode version.
    #[clap(
        long = cli::BYTECODE_VERSION,
    )]
    bytecode_version: Option<u32>,

    /// Internal flag used by the model builder to maintain functions which would be otherwise
    /// included only in tests, without creating the unit test code regular tests do.
    #[clap(skip)]
    keep_testing_functions: bool,

    /// If set, we are in IDE testing mode. This will report IDE annotations as diagnostics.
    #[clap(skip = false)]
    ide_test_mode: bool,

    /// If set, we are in IDE mode.
    #[clap(skip = false)]
    ide_mode: bool,
}

impl Flags {
    pub fn empty() -> Self {
        Self {
            test: false,
            shadow: false,
            bytecode_version: None,
            warnings_are_errors: false,
            silence_warnings: false,
            json_errors: false,
            keep_testing_functions: false,
            ide_mode: false,
            ide_test_mode: false,
        }
    }

    pub fn testing() -> Self {
        Self {
            test: true,
            shadow: false,
            bytecode_version: None,
            warnings_are_errors: false,
            json_errors: false,
            silence_warnings: false,
            keep_testing_functions: false,
            ide_mode: false,
            ide_test_mode: false,
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

    pub fn set_warnings_are_errors(self, value: bool) -> Self {
        Self {
            warnings_are_errors: value,
            ..self
        }
    }

    pub fn set_silence_warnings(self, value: bool) -> Self {
        Self {
            silence_warnings: value,
            ..self
        }
    }

    pub fn set_json_errors(self, value: bool) -> Self {
        Self {
            json_errors: value,
            ..self
        }
    }

    pub fn set_ide_test_mode(self, value: bool) -> Self {
        Self {
            ide_test_mode: value,
            ..self
        }
    }

    pub fn set_ide_mode(self, value: bool) -> Self {
        Self {
            ide_mode: value,
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

    pub fn sources_shadow_deps(&self) -> bool {
        self.shadow
    }

    pub fn bytecode_version(&self) -> Option<u32> {
        self.bytecode_version
    }

    pub fn warnings_are_errors(&self) -> bool {
        self.warnings_are_errors
    }

    pub fn json_errors(&self) -> bool {
        self.json_errors
    }

    pub fn silence_warnings(&self) -> bool {
        self.silence_warnings
    }

    pub fn ide_test_mode(&self) -> bool {
        self.ide_test_mode
    }

    pub fn ide_mode(&self) -> bool {
        self.ide_mode
    }
}

//**************************************************************************************************
// Package Level Config
//**************************************************************************************************

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct PackageConfig {
    pub is_dependency: bool,
    pub warning_filter: WarningFiltersBuilder,
    pub flavor: Flavor,
    pub edition: Edition,
}

impl Default for PackageConfig {
    fn default() -> Self {
        Self {
            is_dependency: false,
            warning_filter: WarningFiltersBuilder::new_for_source(),
            flavor: Flavor::default(),
            edition: Edition::default(),
        }
    }
}

//**************************************************************************************************
// Visitors
//**************************************************************************************************

pub struct Visitors {
    pub typing: Vec<TypingVisitorObj>,
    pub abs_int: Vec<AbsIntVisitorObj>,
    pub cfgir: Vec<CFGIRVisitorObj>,
}

impl Visitors {
    pub fn new(passes: Vec<cli::compiler::Visitor>) -> Self {
        use cli::compiler::Visitor;
        let mut vs = Visitors {
            typing: vec![],
            abs_int: vec![],
            cfgir: vec![],
        };
        for pass in passes {
            match pass {
                Visitor::AbsIntVisitor(f) => vs.abs_int.push(f),
                Visitor::TypingVisitor(f) => vs.typing.push(f),
                Visitor::CFGIRVisitor(f) => vs.cfgir.push(f),
            }
        }
        vs
    }
}

// TODO remove it once visitor invocation is parallel
#[allow(unused)]
fn check<T: Send + Sync>() {}
#[allow(unused)]
fn check_all() {
    check::<Visitors>();
    check::<&Visitors>();
    check::<&WarningFiltersTable>();
    check::<&WarningFiltersScope>();
    check::<&CompilationEnv>();
}

//**************************************************************************************************
// Save Hooks
//**************************************************************************************************

#[derive(Clone)]
pub struct SaveHook(Arc<Mutex<SavedInfo>>);

#[derive(Clone)]
pub(crate) struct SavedInfo {
    flags: BTreeSet<SaveFlag>,
    parser: Option<P::Program>,
    expansion: Option<E::Program>,
    naming: Option<N::Program>,
    typing: Option<T::Program>,
    typing_info: Option<Arc<program_info::TypingProgramInfo>>,
    hlir: Option<H::Program>,
    cfgir: Option<G::Program>,
}

#[derive(Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub enum SaveFlag {
    Parser,
    Expansion,
    Naming,
    Typing,
    TypingInfo,
    HLIR,
    CFGIR,
}

impl SaveHook {
    pub fn new(flags: impl IntoIterator<Item = SaveFlag>) -> Self {
        let flags = flags.into_iter().collect();
        Self(Arc::new(Mutex::new(SavedInfo {
            flags,
            parser: None,
            expansion: None,
            naming: None,
            typing: None,
            typing_info: None,
            hlir: None,
            cfgir: None,
        })))
    }

    pub(crate) fn save_parser_ast(&self, ast: &P::Program) {
        let mut r = self.0.lock().unwrap();
        if r.parser.is_none() && r.flags.contains(&SaveFlag::Parser) {
            r.parser = Some(ast.clone())
        }
    }

    pub(crate) fn save_expansion_ast(&self, ast: &E::Program) {
        let mut r = self.0.lock().unwrap();
        if r.expansion.is_none() && r.flags.contains(&SaveFlag::Expansion) {
            r.expansion = Some(ast.clone())
        }
    }

    pub(crate) fn save_naming_ast(&self, ast: &N::Program) {
        let mut r = self.0.lock().unwrap();
        if r.naming.is_none() && r.flags.contains(&SaveFlag::Naming) {
            r.naming = Some(ast.clone())
        }
    }

    pub(crate) fn save_typing_ast(&self, ast: &T::Program) {
        let mut r = self.0.lock().unwrap();
        if r.typing.is_none() && r.flags.contains(&SaveFlag::Typing) {
            r.typing = Some(ast.clone())
        }
    }

    pub(crate) fn save_typing_info(&self, info: &Arc<program_info::TypingProgramInfo>) {
        let mut r = self.0.lock().unwrap();
        if r.typing_info.is_none() && r.flags.contains(&SaveFlag::TypingInfo) {
            r.typing_info = Some(info.clone())
        }
    }

    pub(crate) fn save_hlir_ast(&self, ast: &H::Program) {
        let mut r = self.0.lock().unwrap();
        if r.hlir.is_none() && r.flags.contains(&SaveFlag::HLIR) {
            r.hlir = Some(ast.clone())
        }
    }

    pub(crate) fn save_cfgir_ast(&self, ast: &G::Program) {
        let mut r = self.0.lock().unwrap();
        if r.cfgir.is_none() && r.flags.contains(&SaveFlag::CFGIR) {
            r.cfgir = Some(ast.clone())
        }
    }

    pub fn take_parser_ast(&self) -> P::Program {
        let mut r = self.0.lock().unwrap();
        assert!(
            r.flags.contains(&SaveFlag::Parser),
            "Parser AST not saved. Please set the flag when creating the SaveHook"
        );
        r.parser.take().unwrap()
    }

    pub fn take_expansion_ast(&self) -> E::Program {
        let mut r = self.0.lock().unwrap();
        assert!(
            r.flags.contains(&SaveFlag::Expansion),
            "Expansion AST not saved. Please set the flag when creating the SaveHook"
        );
        r.expansion.take().unwrap()
    }

    pub fn take_naming_ast(&self) -> N::Program {
        let mut r = self.0.lock().unwrap();
        assert!(
            r.flags.contains(&SaveFlag::Naming),
            "Naming AST not saved. Please set the flag when creating the SaveHook"
        );
        r.naming.take().unwrap()
    }

    pub fn take_typing_ast(&self) -> T::Program {
        let mut r = self.0.lock().unwrap();
        assert!(
            r.flags.contains(&SaveFlag::Typing),
            "Typing AST not saved. Please set the flag when creating the SaveHook"
        );
        r.typing.take().unwrap()
    }

    pub fn take_typing_info(&self) -> Arc<program_info::TypingProgramInfo> {
        let mut r = self.0.lock().unwrap();
        assert!(
            r.flags.contains(&SaveFlag::TypingInfo),
            "Typing info not saved. Please set the flag when creating the SaveHook"
        );
        r.typing_info.take().unwrap()
    }

    pub fn take_hlir_ast(&self) -> H::Program {
        let mut r = self.0.lock().unwrap();
        assert!(
            r.flags.contains(&SaveFlag::HLIR),
            "HLIR AST not saved. Please set the flag when creating the SaveHook"
        );
        r.hlir.take().unwrap()
    }

    pub fn take_cfgir_ast(&self) -> G::Program {
        let mut r = self.0.lock().unwrap();
        assert!(
            r.flags.contains(&SaveFlag::CFGIR),
            "CFGIR AST not saved. Please set the flag when creating the SaveHook"
        );
        r.cfgir.take().unwrap()
    }
}

//**************************************************************************************************
// Binop Processing Macro
//**************************************************************************************************

/// A macro to handle binop processing without recursion in various passes. This macro proceeds by:
///
/// 1. unravelling nested binops into a work queue;
/// 2. processing that work queue to create a Polish notation expression stack consisting of `Op`
///    (operator) and `Val` (value) entries;
/// 3. processing the expression stack in reverse (RPN-style) alongside a value stack to reassemble
///    the binary operation expressions;
/// 4. and, finally, returning the final value left on the value stack.
///
/// The macro takes the following arguments:
///
///  Type arguments:
///
/// * `$optype` - The type contained in the Op entries on the expression stack.
/// * `$valtype` - The type contained in the Val entries on the expression stack.
///
/// Work Queue Arguments:
///
/// * `$e` - The initial expression to start processing.
/// * `$work_pat` - The pattern used to disassemble entries in the work queue. Note that the work
///    queue may contain any arbitrary type (such as a tuple of a block and expression), so the
///    work pattern is used to disassemble and bind component parts.
/// * `$work_exp` - The actual expression to match on, as defined in the `$work_pat`.
/// * `$binop_pat` - This is a pattern matched against the `$work_exp` that matches if and only if
///    the `$work_exp` is in fact a binary operation expression.
/// * `$bind_rhs` - This block is executed when `$work_exp` matches `$binop_pat`, with any pattern
///   binders from `$binop_pat` in scope. This block must return a 3-tuple consisting of the
///   left-hand side work queue entry, the `$optype` entry for the operand, and the right-hand side
///   work queue entry (as `(lhs, op, rhs)`). Note that `lhs` and `rhs` here should have the same
///   type as the initial `$e`.
/// * `$default` - This block processes a work queue entry when the pattern match fails, and is
///   expected to yield a `$valtype` entry. Note this should be the value you would like on your
///   value stack (i.e., the type of the final result).
///
/// Value Stack Arguments:
///
/// * `$value_stack` - An identifier that names the value stack.
/// * `$op_pat` - When the expression stack finds an `Op`, it will match its contents with this.
/// * `$op_rhs` - This block is executed when an Op is found on the expression stack. Any pattern
///   binders from `$op_pat` will be in scope. This block must return value for the `$value_stack`,
///   and can do so by popping the left-hand side and right-hand side results from the
///   `$value_stack` (in that order). These values should always be available as per the contract
///   of the macro and how it disassembles and pushes values across its computation.
///
/// Examples of usage can be found in `expansion/`, `naming/`, `typing/`, and `hlir/`, in their
/// respective `translation.rs` implementations.

macro_rules! process_binops {
    ($optype:ty,
     $valtype:ty,
     $e:expr,
     $work_pat:pat,
     $work_exp:expr,
     $binop_pat:pat => $binop_rhs:block,
     $default:block,
     $value_stack:ident,
     $op_pat:pat => $op_rhs:block
    ) => {{
        enum Pn {
            Op($optype),
            Val($valtype),
        }

        let mut pn_stack: Vec<Pn> = vec![];
        let mut work_queue = vec![$e];

        while let Some($work_pat) = work_queue.pop() {
            if let $binop_pat = $work_exp {
                let (lhs, op, rhs) = $binop_rhs;
                pn_stack.push(Pn::Op(op));
                work_queue.push(rhs);
                work_queue.push(lhs);
            } else {
                let result = $default;
                pn_stack.push(Pn::Val(result));
            }
        }

        let mut $value_stack = vec![];
        for entry in pn_stack.into_iter().rev() {
            match entry {
                Pn::Op($op_pat) => {
                    let op_result = $op_rhs;
                    $value_stack.push(op_result);
                }
                Pn::Val(v) => $value_stack.push(v),
            }
        }
        let result = $value_stack.pop().unwrap();
        assert!($value_stack.is_empty());
        result
    }};
}

pub(crate) use process_binops;

//**************************************************************************************************
// Virtual file system support
//**************************************************************************************************

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedPackagePath<P> {
    pub package: Option<Symbol>,
    pub path: P,
    pub named_address_map: NamedAddressMapIndex,
}

pub type IndexedPhysicalPackagePath = IndexedPackagePath<Symbol>;

pub type IndexedVfsPackagePath = IndexedPackagePath<VfsPath>;

pub fn vfs_path_from_str(path: String, vfs_path: &VfsPath) -> Result<VfsPath, VfsError> {
    // we need to canonicalized paths for virtual file systems as some of them (e.g., implementation
    // of the physical one) cannot handle relative paths
    fn canonicalize(p: String) -> String {
        // dunce's version of canonicalize does a better job on Windows
        match dunce::canonicalize(&p) {
            Ok(s) => s.to_string_lossy().to_string(),
            Err(_) => p,
        }
    }

    vfs_path.join(canonicalize(path))
}

impl IndexedPhysicalPackagePath {
    pub fn to_vfs_path(self, vfs_root: &VfsPath) -> Result<IndexedVfsPackagePath, VfsError> {
        let IndexedPhysicalPackagePath {
            package,
            path,
            named_address_map,
        } = self;

        Ok(IndexedVfsPackagePath {
            package,
            path: vfs_path_from_str(path.to_string(), vfs_root)?,
            named_address_map,
        })
    }
}
