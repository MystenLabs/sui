// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod codes;

use crate::{
    command_line::COLOR_MODE_ENV_VAR,
    diagnostics::codes::{
        Category, DiagnosticCode, DiagnosticInfo, ExternalPrefix, Severity, WarningFilter,
        WellKnownFilterName,
    },
    shared::{
        ast_debug::AstDebug, known_attributes, FILTER_UNUSED_CONST, FILTER_UNUSED_FUNCTION,
        FILTER_UNUSED_MUT_PARAM, FILTER_UNUSED_MUT_REF, FILTER_UNUSED_STRUCT_FIELD,
        FILTER_UNUSED_TYPE_PARAMETER,
    },
};
use codespan_reporting::{
    self as csr,
    files::SimpleFiles,
    term::{
        emit,
        termcolor::{Buffer, ColorChoice, StandardStream, WriteColor},
        Config,
    },
};
use csr::files::Files;
use move_command_line_common::{env::read_env_var, files::FileHash};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::Write,
    iter::FromIterator,
    ops::Range,
    path::PathBuf,
};

use self::codes::UnusedItem;

//**************************************************************************************************
// Types
//**************************************************************************************************

pub type FileId = usize;
pub type FileName = Symbol;

pub type FilesSourceText = HashMap<FileHash, (FileName, String)>;
type FileMapping = HashMap<FileHash, FileId>;

#[derive(PartialEq, Eq, Clone, Debug, Hash)]
#[must_use]
pub struct Diagnostic {
    info: DiagnosticInfo,
    primary_label: (Loc, String),
    secondary_labels: Vec<(Loc, String)>,
    notes: Vec<String>,
}

#[derive(PartialEq, Eq, Hash, Clone, Debug, Default)]
pub struct Diagnostics(Option<Diagnostics_>);

#[derive(PartialEq, Eq, Hash, Clone, Debug, Default)]
struct Diagnostics_ {
    diagnostics: Vec<Diagnostic>,
    // diagnostics filtered in source code
    filtered_source_diagnostics: Vec<Diagnostic>,
    severity_count: BTreeMap<Severity, usize>,
}

#[derive(PartialEq, Eq, Clone, Debug)]
/// Used to filter out diagnostics, specifically used for warning suppression
pub struct WarningFilters {
    filters: BTreeMap<ExternalPrefix, UnprefixedWarningFilters>,
    for_dependency: bool, // if false, the filters are used for source code
}

#[derive(PartialEq, Eq, Clone, Debug)]
/// Filters split by category and code
enum UnprefixedWarningFilters {
    /// Remove all warnings
    All,
    Specified {
        /// Remove all diags of this category with optional known name
        categories: BTreeMap<u8, Option<WellKnownFilterName>>,
        /// Remove specific diags with optional known filter name
        codes: BTreeMap<(u8, u8), Option<WellKnownFilterName>>,
    },
    /// No filter
    Empty,
}

#[derive(PartialEq, Eq, Clone, Debug, PartialOrd, Ord, Copy)]
enum MigrationChange {
    AddMut,
    AddPublic,
}

// All of the migration changes
pub struct Migration {
    files: SimpleFiles<Symbol, String>,
    file_mapping: FileMapping,
    changes: BTreeMap<FileId, BTreeMap<usize, Vec<(usize, MigrationChange)>>>,
}

//**************************************************************************************************
// Diagnostic Reporting
//**************************************************************************************************

pub fn report_diagnostics(files: &FilesSourceText, diags: Diagnostics) -> ! {
    let should_exit = true;
    report_diagnostics_impl(files, diags, should_exit);
    std::process::exit(1)
}

pub fn report_warnings(files: &FilesSourceText, warnings: Diagnostics) {
    if warnings.is_empty() {
        return;
    }
    debug_assert!(warnings.max_severity().unwrap() == Severity::Warning);
    report_diagnostics_impl(files, warnings, false)
}

fn report_diagnostics_impl(files: &FilesSourceText, diags: Diagnostics, should_exit: bool) {
    let color_choice = match read_env_var(COLOR_MODE_ENV_VAR).as_str() {
        "NONE" => ColorChoice::Never,
        "ANSI" => ColorChoice::AlwaysAnsi,
        "ALWAYS" => ColorChoice::Always,
        _ => ColorChoice::Auto,
    };
    let mut writer = StandardStream::stderr(color_choice);
    output_diagnostics(&mut writer, files, diags);
    if should_exit {
        std::process::exit(1);
    }
}

pub fn unwrap_or_report_diagnostics<T>(files: &FilesSourceText, res: Result<T, Diagnostics>) -> T {
    match res {
        Ok(t) => t,
        Err(diags) => {
            assert!(!diags.is_empty());
            report_diagnostics(files, diags)
        }
    }
}

pub fn report_diagnostics_to_buffer(files: &FilesSourceText, diags: Diagnostics) -> Vec<u8> {
    let mut writer = Buffer::no_color();
    output_diagnostics(&mut writer, files, diags);
    writer.into_inner()
}

pub fn report_diagnostics_to_color_buffer(files: &FilesSourceText, diags: Diagnostics) -> Vec<u8> {
    let mut writer = Buffer::ansi();
    output_diagnostics(&mut writer, files, diags);
    writer.into_inner()
}

fn output_diagnostics<W: WriteColor>(
    writer: &mut W,
    sources: &FilesSourceText,
    diags: Diagnostics,
) {
    let mut files = SimpleFiles::new();
    let mut file_mapping = HashMap::new();
    for (fhash, (fname, source)) in sources {
        let id = files.add(*fname, source.as_str());
        file_mapping.insert(*fhash, id);
    }
    render_diagnostics(writer, &files, &file_mapping, diags);
}

fn render_diagnostics(
    writer: &mut dyn WriteColor,
    files: &SimpleFiles<Symbol, &str>,
    file_mapping: &FileMapping,
    diags: Diagnostics,
) {
    let Diagnostics(Some(mut diags)) = diags else {
        return;
    };

    // Do not render / report migration diagnostics.
    diags.diagnostics.retain(|diag| !diag.is_migration());

    diags.diagnostics.sort_by(|e1, e2| {
        let loc1: &Loc = &e1.primary_label.0;
        let loc2: &Loc = &e2.primary_label.0;
        loc1.cmp(loc2)
    });
    let mut seen: HashSet<Diagnostic> = HashSet::new();
    for diag in diags.diagnostics {
        if seen.contains(&diag) {
            continue;
        }
        seen.insert(diag.clone());
        let rendered = render_diagnostic(file_mapping, diag);
        emit(writer, &Config::default(), files, &rendered).unwrap()
    }
}

fn convert_loc(file_mapping: &FileMapping, loc: Loc) -> (FileId, Range<usize>) {
    let fname = loc.file_hash();
    let id = *file_mapping.get(&fname).unwrap();
    let range = loc.usize_range();
    (id, range)
}

fn render_diagnostic(
    file_mapping: &FileMapping,
    diag: Diagnostic,
) -> csr::diagnostic::Diagnostic<FileId> {
    use csr::diagnostic::{Label, LabelStyle};
    let mk_lbl = |style: LabelStyle, msg: (Loc, String)| -> Label<FileId> {
        let (id, range) = convert_loc(file_mapping, msg.0);
        csr::diagnostic::Label::new(style, id, range).with_message(msg.1)
    };
    let Diagnostic {
        info,
        primary_label,
        secondary_labels,
        notes,
    } = diag;
    let mut diag = csr::diagnostic::Diagnostic::new(info.severity().into_codespan_severity());
    let (code, message) = info.render();
    diag = diag.with_code(code);
    diag = diag.with_message(message.to_string());
    diag = diag.with_labels(vec![mk_lbl(LabelStyle::Primary, primary_label)]);
    diag = diag.with_labels(
        secondary_labels
            .into_iter()
            .map(|msg| mk_lbl(LabelStyle::Secondary, msg))
            .collect(),
    );
    diag = diag.with_notes(notes);
    diag
}

//**************************************************************************************************
// Migration Diff Reporting
//**************************************************************************************************

pub fn generate_migration_diff(files: &FilesSourceText, diags: &Diagnostics) -> Option<Migration> {
    match diags {
        Diagnostics(Some(inner)) => {
            let migration_diags = inner
                .diagnostics
                .iter()
                .filter(|diag| diag.is_migration())
                .cloned()
                .collect::<Vec<_>>();
            if migration_diags.is_empty() {
                return None;
            }
            let migration = Migration::new(files.clone(), migration_diags);
            Some(migration)
        }
        _ => None,
    }
}

// Used in test harness for unit testing
pub fn report_migration_to_buffer(files: &FilesSourceText, diags: Diagnostics) -> Vec<u8> {
    let mut writer = Buffer::no_color();
    if let Some(mut diff) = generate_migration_diff(files, &diags) {
        let _ = writer.write_all(diff.render_output().as_bytes());
    } else {
        let _ = writer.write_all("No migration report".as_bytes());
    }
    writer.into_inner()
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl Diagnostics {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn max_severity(&self) -> Option<Severity> {
        let Self(Some(inner)) = self else { return None };
        // map would be empty at the severity, so it should never be zero
        debug_assert!(inner.severity_count.values().all(|count| *count > 0));
        inner
            .severity_count
            .iter()
            .max_by_key(|(sev, _count)| **sev)
            .map(|(sev, _count)| *sev)
    }

    pub fn count_diags_at_or_above_severity(&self, threshold: Severity) -> usize {
        let Self(Some(inner)) = self else { return 0 };
        // map would be empty at the severity, so it should never be zero
        debug_assert!(inner.severity_count.values().all(|count| *count > 0));
        inner
            .severity_count
            .iter()
            .filter(|(sev, _count)| **sev >= threshold)
            .map(|(_sev, count)| *count)
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        let Self(Some(inner)) = self else { return true };
        inner.diagnostics.is_empty()
    }

    pub fn len(&self) -> usize {
        let Self(Some(inner)) = self else { return 0 };
        inner.diagnostics.len()
    }

    pub fn add(&mut self, diag: Diagnostic) {
        if self.0.is_none() {
            self.0 = Some(Diagnostics_::default())
        }
        let inner = self.0.as_mut().unwrap();
        *inner
            .severity_count
            .entry(diag.info.severity())
            .or_insert(0) += 1;
        inner.diagnostics.push(diag)
    }

    pub fn add_opt(&mut self, diag_opt: Option<Diagnostic>) {
        if let Some(diag) = diag_opt {
            self.add(diag)
        }
    }

    pub fn add_source_filtered(&mut self, diag: Diagnostic) {
        if self.0.is_none() {
            self.0 = Some(Diagnostics_::default())
        }
        let inner = self.0.as_mut().unwrap();
        inner.filtered_source_diagnostics.push(diag)
    }

    pub fn extend(&mut self, other: Self) {
        let Self(Some(Diagnostics_ {
            diagnostics,
            filtered_source_diagnostics: _,
            severity_count,
        })) = other
        else {
            return;
        };
        if self.0.is_none() {
            self.0 = Some(Diagnostics_::default())
        }
        let inner = self.0.as_mut().unwrap();
        for (sev, count) in severity_count {
            *inner.severity_count.entry(sev).or_insert(0) += count;
        }
        inner.diagnostics.extend(diagnostics);
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.0.map(|inner| inner.diagnostics).unwrap_or_default()
    }

    pub fn into_codespan_format(
        self,
    ) -> Vec<(
        codespan_reporting::diagnostic::Severity,
        &'static str,
        (Loc, String),
        Vec<(Loc, String)>,
        Vec<String>,
    )> {
        let mut v = vec![];
        for diag in self.into_vec() {
            let Diagnostic {
                info,
                primary_label,
                secondary_labels,
                notes,
            } = diag;
            let csr_diag = (
                info.severity().into_codespan_severity(),
                info.message(),
                primary_label,
                secondary_labels,
                notes,
            );
            v.push(csr_diag)
        }
        v
    }

    pub fn any_with_prefix(&self, prefix: &str) -> bool {
        let Self(Some(inner)) = self else {
            return false;
        };
        inner
            .diagnostics
            .iter()
            .any(|d| d.info.external_prefix() == Some(prefix))
    }

    /// Returns the number of diags filtered in source (user) code (an not in the dependencies) that
    /// have a given prefix (first value returned) and how many different categories of diags were
    /// filtered.
    pub fn filtered_source_diags_with_prefix(&self, prefix: &str) -> (usize, usize) {
        let Self(Some(inner)) = self else {
            return (0, 0);
        };
        let mut filtered_diags_num = 0;
        let mut filtered_categories = HashSet::new();
        inner.filtered_source_diagnostics.iter().for_each(|d| {
            if d.info.external_prefix() == Some(prefix) {
                filtered_diags_num += 1;
                filtered_categories.insert(d.info.category());
            }
        });
        (filtered_diags_num, filtered_categories.len())
    }
}

impl Diagnostic {
    pub fn new(
        code: impl Into<DiagnosticInfo>,
        (loc, label): (Loc, impl ToString),
        secondary_labels: impl IntoIterator<Item = (Loc, impl ToString)>,
        notes: impl IntoIterator<Item = impl ToString>,
    ) -> Self {
        Diagnostic {
            info: code.into(),
            primary_label: (loc, label.to_string()),
            secondary_labels: secondary_labels
                .into_iter()
                .map(|(loc, msg)| (loc, msg.to_string()))
                .collect(),
            notes: notes.into_iter().map(|msg| msg.to_string()).collect(),
        }
    }

    pub fn set_code(mut self, code: impl Into<DiagnosticInfo>) -> Self {
        self.info = code.into();
        self
    }

    pub(crate) fn set_severity(mut self, severity: Severity) -> Self {
        self.info = self.info.set_severity(severity);
        self
    }

    #[allow(unused)]
    pub fn add_secondary_labels(
        &mut self,
        additional_labels: impl IntoIterator<Item = (Loc, impl ToString)>,
    ) {
        self.secondary_labels.extend(
            additional_labels
                .into_iter()
                .map(|(loc, msg)| (loc, msg.to_string())),
        )
    }

    pub fn add_secondary_label(&mut self, (loc, msg): (Loc, impl ToString)) {
        self.secondary_labels.push((loc, msg.to_string()))
    }

    pub fn extra_labels_len(&self) -> usize {
        self.secondary_labels.len() + self.notes.len()
    }

    #[allow(unused)]
    pub fn add_notes(&mut self, additional_notes: impl IntoIterator<Item = impl ToString>) {
        self.notes
            .extend(additional_notes.into_iter().map(|msg| msg.to_string()))
    }

    pub fn add_note(&mut self, msg: impl ToString) {
        self.notes.push(msg.to_string())
    }

    pub fn info(&self) -> &DiagnosticInfo {
        &self.info
    }

    pub fn is_migration(&self) -> bool {
        const MIGRATION_CATEGORY: u8 = codes::Category::Migration as u8;
        self.info.category() == MIGRATION_CATEGORY
    }
}

#[macro_export]
macro_rules! diag {
    ($code: expr, $primary: expr $(,)?) => {{
        #[allow(unused)]
        use $crate::diagnostics::codes::*;
        $crate::diagnostics::Diagnostic::new(
            $code,
            $primary,
            std::iter::empty::<(move_ir_types::location::Loc, String)>(),
            std::iter::empty::<String>(),
        )
    }};
    ($code: expr, $primary: expr, $($secondary: expr),+ $(,)?) => {{
        #[allow(unused)]
        use $crate::diagnostics::codes::*;
        $crate::diagnostics::Diagnostic::new(
            $code,
            $primary,
            vec![$($secondary, )*],
            std::iter::empty::<String>(),
        )
    }};
}

pub const ICE_BUG_REPORT_MESSAGE: &str =
    "The Move compiler has encountered an internal compiler error.\n \
    Please report this this issue to the Mysten Labs Move language team,\n \
    including this error and any relevant code, to the Mysten Labs issue tracker\n \
    at : https://github.com/MystenLabs/sui/issues";

#[macro_export]
macro_rules! ice {
    ($primary: expr $(,)?) => {{
        $crate::diagnostics::print_stack_trace();
        let mut diag = $crate::diag!($crate::diagnostics::codes::Bug::ICE, $primary);
        diag.add_note($crate::diagnostics::ICE_BUG_REPORT_MESSAGE.to_string());
        diag
    }};
    ($primary: expr, $($secondary: expr),+ $(,)?) => {{
        $crate::diagnostics::print_stack_trace();
        let mut diag =
            $crate::diag!($crate::diagnostics::codes::Bug::ICE, $primary, $($secondary, )*);
        diag.add_note($crate::diagnostics::ICE_BUG_REPORT_MESSAGE.to_string());
        diag
    }}
}

#[allow(clippy::wildcard_in_or_patterns)]
pub fn print_stack_trace() {
    use std::backtrace::{Backtrace, BacktraceStatus};
    let stacktrace = Backtrace::capture();
    match stacktrace.status() {
        BacktraceStatus::Captured => {
            eprintln!("stacktrace:");
            eprintln!("{}", stacktrace);
        }
        BacktraceStatus::Unsupported | BacktraceStatus::Disabled | _ => (),
    }
}

impl WarningFilters {
    pub fn new_for_source() -> Self {
        Self {
            filters: BTreeMap::new(),
            for_dependency: false,
        }
    }

    pub fn new_for_dependency() -> Self {
        Self {
            filters: BTreeMap::new(),
            for_dependency: true,
        }
    }

    pub fn is_filtered(&self, diag: &Diagnostic) -> bool {
        self.is_filtered_by_info(&diag.info)
    }

    fn is_filtered_by_info(&self, info: &DiagnosticInfo) -> bool {
        let prefix = info.external_prefix();
        self.filters
            .get(&prefix)
            .is_some_and(|filters| filters.is_filtered_by_info(info))
    }

    pub fn union(&mut self, other: &Self) {
        for (prefix, filters) in &other.filters {
            self.filters
                .entry(*prefix)
                .or_insert_with(UnprefixedWarningFilters::new)
                .union(filters);
        }
        // if there is a dependency code filter on the stack, it means we are filtering dependent
        // code and this information must be preserved when stacking up additional filters (which
        // involves union of the current filter with the new one)
        self.for_dependency = self.for_dependency || other.for_dependency;
    }

    pub fn add(&mut self, filter: WarningFilter) {
        let (prefix, category, code, name) = match filter {
            WarningFilter::All(prefix) => {
                self.filters.insert(prefix, UnprefixedWarningFilters::All);
                return;
            }
            WarningFilter::Category {
                prefix,
                category,
                name,
            } => (prefix, category, None, name),
            WarningFilter::Code {
                prefix,
                category,
                code,
                name,
            } => (prefix, category, Some(code), name),
        };
        self.filters
            .entry(prefix)
            .or_insert(UnprefixedWarningFilters::Empty)
            .add(category, code, name)
    }

    pub fn unused_warnings_filter_for_test() -> Self {
        Self {
            filters: BTreeMap::from([(
                None,
                UnprefixedWarningFilters::unused_warnings_filter_for_test(),
            )]),
            for_dependency: false,
        }
    }

    pub fn for_dependency(&self) -> bool {
        self.for_dependency
    }
}

impl UnprefixedWarningFilters {
    pub fn new() -> Self {
        Self::Empty
    }

    fn is_filtered_by_info(&self, info: &DiagnosticInfo) -> bool {
        match self {
            Self::All => info.severity() == Severity::Warning,
            Self::Specified { categories, codes } => {
                info.severity() == Severity::Warning
                    && (categories.contains_key(&info.category())
                        || codes.contains_key(&(info.category(), info.code())))
            }
            Self::Empty => false,
        }
    }

    pub fn union(&mut self, other: &Self) {
        match (self, other) {
            // if self is empty, just take the other filter
            (s @ Self::Empty, _) => *s = other.clone(),
            // if other is empty, or self is ALL, no change to the filter
            (_, Self::Empty) => (),
            (Self::All, _) => (),
            // if other is all, self is now all
            (s, Self::All) => *s = Self::All,
            // category and code level union
            (
                Self::Specified { categories, codes },
                Self::Specified {
                    categories: other_categories,
                    codes: other_codes,
                },
            ) => {
                categories.extend(other_categories);
                // remove any codes covered by the category level filter
                codes.extend(
                    other_codes
                        .iter()
                        .filter(|((category, _), _)| !categories.contains_key(category)),
                );
            }
        }
    }

    /// Add a specific filter to the filter map.
    /// If filter_code is None, then the filter applies to all codes in the filter_category.
    fn add(
        &mut self,
        filter_category: u8,
        filter_code: Option<u8>,
        filter_name: Option<WellKnownFilterName>,
    ) {
        match self {
            Self::All => (),
            Self::Empty => {
                *self = Self::Specified {
                    categories: BTreeMap::new(),
                    codes: BTreeMap::new(),
                };
                self.add(filter_category, filter_code, filter_name)
            }
            Self::Specified { categories, .. } if categories.contains_key(&filter_category) => (),
            Self::Specified { categories, codes } => {
                if let Some(filter_code) = filter_code {
                    codes.insert((filter_category, filter_code), filter_name);
                } else {
                    categories.insert(filter_category, filter_name);
                    codes.retain(|(category, _), _| *category != filter_category);
                }
            }
        }
    }

    pub fn unused_warnings_filter_for_test() -> Self {
        let filtered_codes = [
            (UnusedItem::Function, FILTER_UNUSED_FUNCTION),
            (UnusedItem::StructField, FILTER_UNUSED_STRUCT_FIELD),
            (UnusedItem::FunTypeParam, FILTER_UNUSED_TYPE_PARAMETER),
            (UnusedItem::Constant, FILTER_UNUSED_CONST),
            (UnusedItem::MutReference, FILTER_UNUSED_MUT_REF),
            (UnusedItem::MutParam, FILTER_UNUSED_MUT_PARAM),
        ]
        .into_iter()
        .map(|(item, filter)| {
            let info = item.into_info();
            ((info.category(), info.code()), Some(filter))
        })
        .collect();
        Self::Specified {
            categories: BTreeMap::new(),
            codes: filtered_codes,
        }
    }
}

impl Migration {
    pub fn new(sources: FilesSourceText, diags: Vec<Diagnostic>) -> Migration {
        let mut files = SimpleFiles::new();
        let mut file_mapping = HashMap::new();
        for (fhash, (fname, source)) in sources {
            let id = files.add(fname, source);
            file_mapping.insert(fhash, id);
        }
        let mut mig = Migration {
            files,
            file_mapping,
            changes: BTreeMap::new(),
        };

        for diag in diags {
            mig.add_diagnostic(diag);
        }

        mig
    }

    fn add_diagnostic(&mut self, diag: Diagnostic) {
        const CAT: u8 = Category::Migration as u8;
        const NEEDS_MUT: u8 = codes::Migration::NeedsLetMut as u8;
        const NEEDS_PUBLIC: u8 = codes::Migration::NeedsPublic as u8;

        let (file_id, line, col) = self.find_file_location(&diag);
        let file_change_entry = self.changes.entry(file_id).or_default();
        let line_change_entry = file_change_entry.entry(line).or_default();
        match (diag.info().category(), diag.info().code()) {
            (CAT, NEEDS_MUT) => line_change_entry.push((col, MigrationChange::AddMut)),
            (CAT, NEEDS_PUBLIC) => line_change_entry.push((col, MigrationChange::AddPublic)),
            _ => unreachable!(),
        }
    }

    fn find_file_location(&mut self, diag: &Diagnostic) -> (usize, usize, usize) {
        let (loc, _msg) = &diag.primary_label;
        let start_loc = loc.start() as usize;
        let file_id = *self.file_mapping.get(&loc.file_hash()).unwrap();
        let file_loc = self.files.location(file_id, start_loc).unwrap();
        (file_id, file_loc.line_number, file_loc.column_number - 1)
    }

    fn get_line(&self, file_id: FileId, line_index: usize) -> String {
        let line_range = self.files.line_range(file_id, line_index).unwrap();
        self.files.source(file_id).unwrap()[line_range].to_string()
    }

    fn render_line(
        line_text: String,
        migration_set: BTreeMap<usize, BTreeSet<MigrationChange>>,
    ) -> String {
        let mut line_prefix: &str = &line_text[..];
        let mut output = "".to_string();
        for (col, changes) in migration_set.iter().rev() {
            let rest = &line_prefix[*col..];
            for change in changes {
                match change {
                    MigrationChange::AddMut => {
                        output = format!("mut {}{}", rest, output);
                        line_prefix = &line_prefix[..*col];
                    }
                    MigrationChange::AddPublic => {
                        output = format!("public {}{}", rest, output);
                        line_prefix = &line_prefix[..*col];
                    }
                }
            }
        }
        output = format!("{}{}", line_prefix, output);
        output
    }

    pub fn render_output(&mut self) -> String {
        let mut changes = std::mem::take(&mut self.changes);

        let mut output = vec![];
        let mut names = changes
            .keys()
            .map(|id| (*id, *self.files.get(*id).unwrap().name()))
            .collect::<Vec<_>>();
        names.sort_by_key(|(_, name)| *name);
        for (file_id, name) in names {
            let file_changes = changes.get_mut(&file_id).unwrap();
            output.push(format!("--- {}\n+++ {}\n", name, name));
            for (line_number, line_changes) in file_changes.iter() {
                let migration_set = Self::unique_changes(line_changes);
                let line = self.get_line(file_id, *line_number - 1).to_string();
                output.push(format!("@@ -{line_number},1 +{line_number},1 @@\n"));
                output.push(format!("-{}", line));
                let new_line = Self::render_line(line.to_string(), migration_set);
                output.push(format!("+{}", new_line));
            }
        }

        let _ = std::mem::replace(&mut self.changes, changes);

        output.join("")
    }

    pub fn record_diff(&mut self, path: PathBuf) -> anyhow::Result<String> {
        let output_path = path.join("migration.patch");
        let string_result = output_path.to_str().unwrap_or("invalid path").to_string();
        std::fs::write(output_path, self.render_output())?;
        Ok(string_result)
    }

    pub fn apply_changes<W: Write>(&mut self, w: &mut W) -> anyhow::Result<()> {
        writeln!(w)?;
        let mut names = self
            .changes
            .keys()
            .map(|id| (*id, self.files.get(*id).unwrap()))
            .collect::<Vec<_>>();
        names.sort_by_key(|(_, file)| file.name());
        for (file_id, file) in names {
            let file_changes = self.changes.get_mut(&file_id).unwrap();
            let name = file.name();
            let path = PathBuf::from(name.to_string());
            let mut output = vec![];
            for (ndx, line) in file.source().lines().enumerate() {
                if let Some(line_changes) = file_changes.get(&(ndx + 1)) {
                    let migration_set = Self::unique_changes(line_changes);
                    output.push(Self::render_line(line.to_string(), migration_set))
                } else {
                    output.push(line.to_string());
                }
            }
            writeln!(w, "Updating {:#?} . . .", path)?;
            // let out_writer = std::fs::write(path, contents)
            let mut buf: Vec<u8> = Vec::new();
            for line in output {
                writeln!(&mut buf, "{}", line)?;
            }
            std::fs::write(path, buf)?;
        }
        Ok(())
    }

    // Processes a vector of changes for a single line, returning a BTreeMap of unique ones
    // per-column. The map iterates in sorted order, so this also sorts them.
    fn unique_changes(
        change_list: &[(usize, MigrationChange)],
    ) -> BTreeMap<usize, BTreeSet<MigrationChange>> {
        let mut migration_set: BTreeMap<usize, BTreeSet<MigrationChange>> = BTreeMap::new();
        for (col, change) in change_list {
            migration_set.entry(*col).or_default().insert(*change);
        }
        migration_set
    }
}

//**************************************************************************************************
// traits
//**************************************************************************************************

impl FromIterator<Diagnostic> for Diagnostics {
    fn from_iter<I: IntoIterator<Item = Diagnostic>>(iter: I) -> Self {
        let diagnostics = iter.into_iter().collect::<Vec<_>>();
        Self::from(diagnostics)
    }
}

impl From<Vec<Diagnostic>> for Diagnostics {
    fn from(diagnostics: Vec<Diagnostic>) -> Self {
        if diagnostics.is_empty() {
            return Self(None);
        }

        let mut severity_count = BTreeMap::new();
        for diag in &diagnostics {
            *severity_count.entry(diag.info.severity()).or_insert(0) += 1;
        }
        Self(Some(Diagnostics_ {
            diagnostics,
            filtered_source_diagnostics: vec![],
            severity_count,
        }))
    }
}

impl From<Option<Diagnostic>> for Diagnostics {
    fn from(diagnostic_opt: Option<Diagnostic>) -> Self {
        Diagnostics::from(diagnostic_opt.map_or_else(Vec::new, |diag| vec![diag]))
    }
}

impl AstDebug for WarningFilters {
    fn ast_debug(&self, w: &mut crate::shared::ast_debug::AstWriter) {
        for (prefix, filters) in &self.filters {
            let prefix_str = prefix.unwrap_or(known_attributes::DiagnosticAttribute::ALLOW);
            match filters {
                UnprefixedWarningFilters::All => w.write(&format!(
                    "#[{}({})]",
                    prefix_str,
                    WarningFilter::All(*prefix).to_str().unwrap(),
                )),
                UnprefixedWarningFilters::Specified { categories, codes } => {
                    w.write(&format!("#[{}(", prefix_str));
                    let items = categories
                        .iter()
                        .map(|(cat, n)| WarningFilter::Category {
                            prefix: *prefix,
                            category: *cat,
                            name: *n,
                        })
                        .chain(codes.iter().map(|((cat, code), n)| WarningFilter::Code {
                            prefix: *prefix,
                            category: *cat,
                            code: *code,
                            name: *n,
                        }));
                    w.list(items, ",", |w, filter| {
                        w.write(filter.to_str().unwrap());
                        false
                    });
                    w.write(")]")
                }
                UnprefixedWarningFilters::Empty => (),
            }
        }
    }
}

impl<C: DiagnosticCode> From<C> for DiagnosticInfo {
    fn from(value: C) -> Self {
        value.into_info()
    }
}
