// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod codes;

use crate::{
    command_line::COLOR_MODE_ENV_VAR,
    diagnostics::codes::{
        DiagnosticCode, DiagnosticInfo, ExternalPrefix, Severity, WarningFilter,
        WellKnownFilterName,
    },
    shared::{
        ast_debug::AstDebug, FILTER_UNUSED_CONST, FILTER_UNUSED_FUNCTION, FILTER_UNUSED_MUT_PARAM,
        FILTER_UNUSED_MUT_REF, FILTER_UNUSED_STRUCT_FIELD, FILTER_UNUSED_TYPE_PARAMETER,
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
use move_command_line_common::{env::read_env_var, files::FileHash};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    iter::FromIterator,
    ops::Range,
};

use self::codes::{UnusedItem, WARNING_FILTER_ATTR};

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

//**************************************************************************************************
// Reporting
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
    if let Some(id) = file_mapping.get(&fname) {
        let range = loc.usize_range();
        (*id, range)
    } else {
        let msg = format!("ICE Couldn't find filename hash {:?} in mapping", fname);
        panic!("{}", msg);
    }
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
// impls
//**************************************************************************************************

impl Diagnostics {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn max_severity(&self) -> Option<Severity> {
        let Self(Some(inner)) = self else { return None };
        debug_assert!(inner.severity_count.values().all(|count| *count > 0));
        inner
            .severity_count
            .iter()
            .max_by_key(|(sev, _count)| **sev)
            .map(|(sev, _count)| *sev)
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
        inner.diagnostics.extend(diagnostics)
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
            let prefix_str = prefix.unwrap_or(WARNING_FILTER_ATTR);
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

//**************************************************************************************************
// String Construction Helpers
//**************************************************************************************************

pub fn and_list_string(input: Vec<String>) -> String {
    comma_list_string(input, "and".to_string())
}

pub fn or_list_string(input: Vec<String>) -> String {
    comma_list_string(input, "or".to_string())
}

pub fn comma_list_string(mut input: Vec<String>, separator_word: String) -> String {
    assert!(!input.is_empty());
    if input.len() == 1 {
        input.pop().unwrap()
    } else if input.len() == 2 {
        let last = input.pop().unwrap();
        let first = input.pop().unwrap();
        format!("{} {} {}", first, separator_word, last)
    } else {
        let last = format!("{} {}", separator_word, input.pop().unwrap());
        input.push(last);
        input.join(", ")
    }
}
