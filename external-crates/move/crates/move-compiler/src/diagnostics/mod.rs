// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod codes;
pub mod warning_filters;

use crate::{
    command_line::COLOR_MODE_ENV_VAR,
    diagnostics::{
        codes::{Category, DiagnosticCode, DiagnosticInfo, DiagnosticsID, Severity},
        warning_filters::{FilterName, FilterPrefix, WarningFilters, WarningFiltersScope},
    },
    shared::{
        files::{ByteSpan, FileByteSpan, FileId, MappedFiles},
        format_allow_attr,
        ide::{IDEAnnotation, IDEInfo},
        known_attributes,
    },
    Flags,
};
use codespan_reporting::{
    self as csr,
    term::{
        emit,
        termcolor::{Buffer, ColorChoice, StandardStream, WriteColor},
        Config,
    },
};
use csr::files::Files;
use move_command_line_common::{env::read_env_var, files::FileHash};
use move_ir_types::location::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Write,
    iter::FromIterator,
    ops::Range,
    path::PathBuf,
    sync::RwLock,
};

//**************************************************************************************************
// Types
//**************************************************************************************************

#[derive(Clone, Debug)]
pub struct DiagnosticReporter<'env> {
    flags: &'env Flags,
    known_filter_names: &'env BTreeMap<DiagnosticsID, (FilterPrefix, FilterName)>,
    diags: &'env RwLock<Diagnostics>,
    ide_information: &'env RwLock<IDEInfo>,
    warning_filters_scope: WarningFiltersScope,
}

#[derive(PartialEq, Eq, Hash, Clone, Debug, Default)]
pub struct Diagnostics {
    diags: Option<Diagnostics_>,
    format: DiagnosticsFormat,
}

#[derive(PartialEq, Eq, Hash, Clone, Debug, Default)]
struct Diagnostics_ {
    diagnostics: Vec<Diagnostic>,
    // diagnostics filtered in source code
    filtered_source_diagnostics: Vec<Diagnostic>,
    severity_count: BTreeMap<Severity, usize>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug, Hash)]
#[must_use]
pub struct Diagnostic {
    info: DiagnosticInfo,
    primary_label: (Loc, String),
    secondary_labels: Vec<(Loc, String)>,
    notes: Vec<String>,
}

#[derive(PartialEq, Eq, Hash, Clone, Debug, Default)]
pub enum DiagnosticsFormat {
    #[default]
    Text,
    JSON,
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonDiagnostic {
    file: String,
    line: usize,
    column: usize,
    level: String,
    category: u8,
    code: u8,
    msg: String,
}

#[derive(PartialEq, Eq, Clone, Debug, PartialOrd, Ord)]
enum MigrationChange {
    AddMut,
    AddPublic,
    Backquote(String),
    AddGlobalQual,
    RemoveFriend,
    MakePubPackage,
    AddressRemove,
    AddressAdd(String),
}

// All of the migration changes
pub struct Migration {
    mapped_files: MappedFiles,
    changes: BTreeMap<FileHash, Vec<(ByteSpan, MigrationChange)>>,
}

//**************************************************************************************************
// Diagnostic Reporting
//**************************************************************************************************

pub fn report_diagnostics(files: &MappedFiles, diags: Diagnostics) -> ! {
    let should_exit = true;
    report_diagnostics_impl(files, diags, should_exit);
    std::process::exit(1)
}

pub fn report_warnings(files: &MappedFiles, warnings: Diagnostics) {
    if warnings.is_empty() {
        return;
    }
    debug_assert!(warnings.max_severity_at_or_under_severity(Severity::Warning));
    report_diagnostics_impl(files, warnings, false)
}

fn report_diagnostics_impl(files: &MappedFiles, diags: Diagnostics, should_exit: bool) {
    let color_choice = diags.env_color();
    let mut writer = StandardStream::stderr(color_choice);
    render_diagnostics(&mut writer, files, diags);
    if should_exit {
        std::process::exit(1);
    }
}

pub fn unwrap_or_report_pass_diagnostics<T, Pass>(
    files: &MappedFiles,
    res: Result<T, (Pass, Diagnostics)>,
) -> T {
    match res {
        Ok(t) => t,
        Err((_pass, diags)) => {
            assert!(!diags.is_empty());
            report_diagnostics(files, diags)
        }
    }
}

pub fn unwrap_or_report_diagnostics<T>(files: &MappedFiles, res: Result<T, Diagnostics>) -> T {
    match res {
        Ok(t) => t,
        Err(diags) => {
            assert!(!diags.is_empty());
            report_diagnostics(files, diags)
        }
    }
}

pub fn report_diagnostics_to_buffer_with_env_color(
    files: &MappedFiles,
    diags: Diagnostics,
) -> Vec<u8> {
    let ansi_color = match env_color() {
        ColorChoice::Always | ColorChoice::AlwaysAnsi | ColorChoice::Auto => true,
        ColorChoice::Never => false,
    };
    report_diagnostics_to_buffer(files, diags, ansi_color)
}

pub fn report_diagnostics_to_buffer(
    files: &MappedFiles,
    diags: Diagnostics,
    ansi_color: bool,
) -> Vec<u8> {
    let mut writer = if ansi_color {
        Buffer::ansi()
    } else {
        Buffer::no_color()
    };
    render_diagnostics(&mut writer, files, diags);
    writer.into_inner()
}

pub fn report_diagnostics_to_buffer_with_mapped_files(
    mapped_files: &MappedFiles,
    diags: Diagnostics,
    ansi_color: bool,
) -> Vec<u8> {
    let mut writer = if ansi_color {
        Buffer::ansi()
    } else {
        Buffer::no_color()
    };
    render_diagnostics(&mut writer, mapped_files, diags);
    writer.into_inner()
}

pub fn env_color() -> ColorChoice {
    match read_env_var(COLOR_MODE_ENV_VAR).as_str() {
        "NONE" => ColorChoice::Never,
        "ANSI" => ColorChoice::AlwaysAnsi,
        "ALWAYS" => ColorChoice::Always,
        _ => ColorChoice::Auto,
    }
}

fn render_diagnostics(writer: &mut dyn WriteColor, mapping: &MappedFiles, diags: Diagnostics) {
    let Diagnostics {
        diags: Some(mut diags),
        format,
    } = diags
    else {
        return;
    };

    // Do not render / report migration diagnostics.
    diags.diagnostics.retain(|diag| !diag.is_migration());

    diags.diagnostics.sort_by(|e1, e2| {
        let loc1: &Loc = &e1.primary_label.0;
        let loc2: &Loc = &e2.primary_label.0;
        loc1.cmp(loc2).then_with(|| e1.cmp(e2))
    });
    match format {
        DiagnosticsFormat::Text => emit_diagnostics_text(writer, mapping, diags),
        DiagnosticsFormat::JSON => emit_diagnostics_json(writer, mapping, diags),
    }
}

fn convert_loc(mapped_files: &MappedFiles, loc: Loc) -> Option<(FileId, Range<usize>)> {
    let fname = loc.file_hash();
    let id = mapped_files.file_hash_to_file_id(&fname)?;
    let range = loc.usize_range();
    Some((id, range))
}

fn emit_diagnostics_text(
    writer: &mut dyn WriteColor,
    mapped_files: &MappedFiles,
    diags: Diagnostics_,
) {
    let mut seen: HashSet<Diagnostic> = HashSet::new();
    for diag in diags.diagnostics {
        if seen.contains(&diag) {
            continue;
        }
        seen.insert(diag.clone());
        let rendered = render_diagnostic_text(mapped_files, diag);
        emit(writer, &Config::default(), mapped_files.files(), &rendered).unwrap()
    }
}

fn render_diagnostic_text(
    mapped_files: &MappedFiles,
    diag: Diagnostic,
) -> csr::diagnostic::Diagnostic<FileId> {
    use csr::diagnostic::{Label, LabelStyle};
    let mk_lbl = |style: LabelStyle,
                  (loc, msg): (Loc, String),
                  notes: &mut Vec<String>|
     -> Option<Label<FileId>> {
        let Some((id, range)) = convert_loc(mapped_files, loc) else {
            notes.push(format!(
                "Compiler Error -- no location information for error:\n  {msg}"
            ));
            return None;
        };
        Some(csr::diagnostic::Label::new(style, id, range).with_message(msg))
    };
    let Diagnostic {
        info,
        primary_label,
        secondary_labels,
        mut notes,
    } = diag;
    let mut diag = csr::diagnostic::Diagnostic::new(info.severity().into_codespan_severity());
    let (code, message) = info.render();
    diag = diag.with_code(code);
    diag = diag.with_message(message.to_string());
    let labels = vec![mk_lbl(LabelStyle::Primary, primary_label, &mut notes)]
        .into_iter()
        .chain(
            secondary_labels
                .into_iter()
                .map(|msg| mk_lbl(LabelStyle::Secondary, msg, &mut notes)),
        )
        .flatten()
        .collect::<Vec<_>>();
    diag = diag.with_labels(labels);
    diag = diag.with_notes(notes);
    diag
}

fn emit_diagnostics_json(
    writer: &mut dyn WriteColor,
    mapped_files: &MappedFiles,
    diags: Diagnostics_,
) {
    let mut seen: HashSet<Diagnostic> = HashSet::new();
    let mut output_diagnostics = vec![];
    for diag in diags.diagnostics {
        if seen.contains(&diag) {
            continue;
        }
        seen.insert(diag.clone());
        let json_diag = diag.to_json(mapped_files);
        output_diagnostics.push(json_diag);
    }
    writeln!(
        writer,
        "{}",
        serde_json::to_string_pretty(&output_diagnostics).unwrap()
    )
    .expect("ICE reporting error");
}

//**************************************************************************************************
// Migration Diff Reporting
//**************************************************************************************************

pub fn generate_migration_diff(
    files: &MappedFiles,
    diags: &Diagnostics,
) -> Option<(Migration, /* Migration errors */ Diagnostics)> {
    match diags {
        Diagnostics {
            diags: Some(inner),
            format,
        } => {
            assert!(
                matches!(format, DiagnosticsFormat::Text),
                "Cannot migrate with json mode set"
            );
            let migration_diags = inner
                .diagnostics
                .iter()
                .filter(|diag| diag.is_migration())
                .cloned()
                .collect::<Vec<_>>();
            if migration_diags.is_empty() {
                return None;
            }
            Some(Migration::new(files.clone(), migration_diags))
        }
        _ => None,
    }
}

// Used in test harness for unit testing
pub fn report_migration_to_buffer(files: &MappedFiles, diags: Diagnostics) -> Vec<u8> {
    let mut writer = Buffer::no_color();
    if let Some((mut diff, errors)) = generate_migration_diff(files, &diags) {
        let rendered_errors = report_diagnostics_to_buffer(files, errors, /* color */ false);
        let _ = writer.write_all(&rendered_errors);
        let _ = writer.write_all(diff.render_output().as_bytes());
    } else {
        let _ = writer.write_all("No migration report".as_bytes());
    }
    writer.into_inner()
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl<'env> DiagnosticReporter<'env> {
    pub const fn new(
        flags: &'env Flags,
        known_filter_names: &'env BTreeMap<DiagnosticsID, (FilterPrefix, FilterName)>,
        diags: &'env RwLock<Diagnostics>,
        ide_information: &'env RwLock<IDEInfo>,
        warning_filters_scope: WarningFiltersScope,
    ) -> Self {
        Self {
            flags,
            known_filter_names,
            diags,
            ide_information,
            warning_filters_scope,
        }
    }

    pub fn push_warning_filter_scope(&mut self, filters: WarningFilters) {
        self.warning_filters_scope.push(filters)
    }

    pub fn pop_warning_filter_scope(&mut self) {
        self.warning_filters_scope.pop()
    }

    pub fn add_diag(&self, mut diag: Diagnostic) {
        if diag.info().severity() <= Severity::NonblockingError
            && self
                .diags
                .read()
                .unwrap()
                .any_syntax_error_with_primary_loc(diag.primary_loc())
        {
            // do not report multiple diags for the same location (unless they are blocking) to
            // avoid noise that is likely to confuse the developer trying to localize the problem
            //
            // TODO: this check is O(n^2) for n diags - shouldn't be a huge problem but fix if it
            // becomes one
            return;
        }

        if !self.warning_filters_scope.is_filtered(&diag) {
            // add help to suppress warning, if applicable
            // TODO do we want a centralized place for tips like this?
            if diag.info().severity() == Severity::Warning {
                if let Some((prefix, name)) = self.known_filter_names.get(&diag.info().id()) {
                    let help = format!(
                        "This warning can be suppressed with '#[{}({})]' \
                         applied to the 'module' or module member ('const', 'fun', or 'struct')",
                        known_attributes::DiagnosticAttribute::ALLOW,
                        format_allow_attr(*prefix, *name),
                    );
                    diag.add_note(help)
                }
                if self.flags.warnings_are_errors() {
                    diag = diag.set_severity(Severity::NonblockingError)
                }
            }
            self.diags.write().unwrap().add(diag)
        } else if !self.warning_filters_scope.is_filtered_for_dependency() {
            // unwrap above is safe as the filter has been used (thus it must exist)
            self.diags.write().unwrap().add_source_filtered(diag)
        }
    }

    pub fn add_diags(&self, diags: Diagnostics) {
        for diag in diags.into_vec() {
            self.add_diag(diag)
        }
    }

    pub fn extend_ide_info(&self, info: IDEInfo) {
        if self.flags.ide_test_mode() {
            for entry in info.annotations.iter() {
                let diag = entry.clone().into();
                self.add_diag(diag);
            }
        }
        self.ide_information.write().unwrap().extend(info);
    }

    pub fn add_ide_annotation(&self, loc: Loc, info: IDEAnnotation) {
        if self.flags.ide_test_mode() {
            let diag = (loc, info.clone()).into();
            self.add_diag(diag);
        }
        self.ide_information
            .write()
            .unwrap()
            .add_ide_annotation(loc, info);
    }
}

impl Diagnostics {
    pub fn new() -> Self {
        Self {
            diags: None,
            format: DiagnosticsFormat::default(),
        }
    }

    pub fn set_format(&mut self, format: DiagnosticsFormat) {
        self.format = format;
    }

    /// Always false when no diagnostics are present.
    pub fn max_severity_at_or_above_severity(&self, threshold: Severity) -> bool {
        match self.max_severity() {
            Some(max) if max >= threshold => true,
            Some(_) | None => false,
        }
    }

    /// Always true when no diagnostics are present.
    pub fn max_severity_at_or_under_severity(&self, threshold: Severity) -> bool {
        match self.max_severity() {
            Some(max) if max <= threshold => true,
            None => true,
            Some(_) => false,
        }
    }

    pub fn max_severity(&self) -> Option<Severity> {
        let Self {
            diags: Some(inner),
            format: _,
        } = self
        else {
            return None;
        };
        // map would be empty at the severity, so it should never be zero
        debug_assert!(inner.severity_count.values().all(|count| *count > 0));
        inner
            .severity_count
            .iter()
            .max_by_key(|(sev, _count)| **sev)
            .map(|(sev, _count)| *sev)
    }

    pub fn count_diags_at_or_above_severity(&self, threshold: Severity) -> usize {
        let Self {
            diags: Some(inner),
            format: _,
        } = self
        else {
            return 0;
        };
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
        let Self {
            diags: Some(inner),
            format: _,
        } = self
        else {
            return true;
        };
        inner.diagnostics.is_empty()
    }

    pub fn len(&self) -> usize {
        let Self {
            diags: Some(inner),
            format: _,
        } = self
        else {
            return 0;
        };
        inner.diagnostics.len()
    }

    pub fn add(&mut self, diag: Diagnostic) {
        if self.diags.is_none() {
            self.diags = Some(Diagnostics_::default())
        }
        let inner = self.diags.as_mut().unwrap();
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
        if self.diags.is_none() {
            self.diags = Some(Diagnostics_::default())
        }
        let inner = self.diags.as_mut().unwrap();
        inner.filtered_source_diagnostics.push(diag)
    }

    pub fn extend(&mut self, other: Self) {
        let Self {
            diags:
                Some(Diagnostics_ {
                    diagnostics,
                    filtered_source_diagnostics: _,
                    severity_count,
                }),
            format: _format,
        } = other
        else {
            return;
        };
        if self.diags.is_none() {
            self.diags = Some(Diagnostics_::default())
        }
        let inner = self.diags.as_mut().unwrap();
        for (sev, count) in severity_count {
            *inner.severity_count.entry(sev).or_insert(0) += count;
        }
        inner.diagnostics.extend(diagnostics);
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diags
            .map(|inner| inner.diagnostics)
            .unwrap_or_default()
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

    pub fn retain(&mut self, f: impl FnMut(&Diagnostic) -> bool) {
        if self.diags.is_none() {
            return;
        }
        let inner = self.diags.as_mut().unwrap();
        inner.diagnostics.retain(f);
    }

    pub fn any_with_prefix(&self, prefix: &str) -> bool {
        let Self {
            diags: Some(inner),
            format: _,
        } = self
        else {
            return false;
        };
        inner
            .diagnostics
            .iter()
            .any(|d| d.info.external_prefix() == Some(prefix))
    }

    /// Returns true if any diagnostic in the Syntax category have already been recorded.
    pub fn any_syntax_error_with_primary_loc(&self, loc: Loc) -> bool {
        let Self {
            diags: Some(inner),
            format: _,
        } = self
        else {
            return false;
        };
        inner
            .diagnostics
            .iter()
            .any(|d| d.info().category() == Category::Syntax as u8 && d.primary_label.0 == loc)
    }

    /// Returns the number of diags filtered in source (user) code (not in the dependencies) that
    /// have a given prefix and how many different unique lints were filtered.
    pub fn filtered_source_diags_with_prefix(&self, prefix: &str) -> (usize, usize) {
        let Self {
            diags: Some(inner),
            format: _,
        } = self
        else {
            return (0, 0);
        };
        let mut filtered_diags_num = 0;
        let mut unique = HashSet::new();
        inner.filtered_source_diagnostics.iter().for_each(|d| {
            if d.info.external_prefix() == Some(prefix) {
                filtered_diags_num += 1;
                unique.insert((d.info.category(), d.info.code()));
            }
        });
        (filtered_diags_num, unique.len())
    }

    fn env_color(&self) -> ColorChoice {
        match self.format {
            DiagnosticsFormat::Text => (),
            DiagnosticsFormat::JSON => {
                return ColorChoice::Never;
            }
        };
        match read_env_var(COLOR_MODE_ENV_VAR).as_str() {
            "NONE" => ColorChoice::Never,
            "ANSI" => ColorChoice::AlwaysAnsi,
            "ALWAYS" => ColorChoice::Always,
            _ => ColorChoice::Auto,
        }
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

    pub fn primary_msg(&self) -> &str {
        &self.primary_label.1
    }

    pub fn primary_loc(&self) -> Loc {
        self.primary_label.0
    }

    pub fn is_migration(&self) -> bool {
        const MIGRATION_CATEGORY: u8 = codes::Category::Migration as u8;
        self.info.category() == MIGRATION_CATEGORY
    }

    fn to_json(&self, mapped_files: &MappedFiles) -> JsonDiagnostic {
        let Diagnostic {
            info,
            primary_label: (ploc, _pmsg),
            secondary_labels: _,
            notes: _,
        } = self;

        let bloc = mapped_files.position(ploc);
        JsonDiagnostic {
            file: mapped_files
                .file_path(&bloc.file_hash)
                .to_string_lossy()
                .to_string(),
            // TODO: This line and column choice is a bit weird. Consider changing it.
            line: bloc.start.user_line(),
            column: bloc.start.column_offset(),
            level: format!("{:?}", info.severity()),
            category: info.category(),
            code: info.code(),
            msg: info.message().to_string(),
        }
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
    Please report this issue to the Mysten Labs Move language team,\n \
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

#[macro_export]
macro_rules! ice_assert {
    ($reporter: expr, $cond: expr, $loc: expr, $($arg:tt)*) => {{
        if !$cond {
            $reporter.add_diag($crate::ice!((
                $loc,
                format!($($arg)*),
            )));
        }
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

impl Migration {
    pub fn new(
        mapped_files: MappedFiles,
        diags: Vec<Diagnostic>,
    ) -> (Migration, /* Migration errors */ Diagnostics) {
        let mut mig = Migration {
            changes: BTreeMap::new(),
            mapped_files,
        };

        let migration_errors = Diagnostics::new();
        for diag in diags {
            mig.add_diagnostic(diag);
        }

        (mig, migration_errors)
    }

    fn add_diagnostic(&mut self, diag: Diagnostic) {
        const CAT: u8 = Category::Migration as u8;
        const NEEDS_MUT: u8 = codes::Migration::NeedsLetMut as u8;
        const NEEDS_PUBLIC: u8 = codes::Migration::NeedsPublic as u8;
        const NEEDS_BACKTICKS: u8 = codes::Migration::NeedsRestrictedIdentifier as u8;
        const NEEDS_GLOBAL_QUAL: u8 = codes::Migration::NeedsGlobalQualification as u8;
        const REMOVE_FRIEND: u8 = codes::Migration::RemoveFriend as u8;
        const MAKE_PUB_PACKAGE: u8 = codes::Migration::MakePubPackage as u8;
        const ADDRESS_REMOVE: u8 = codes::Migration::AddressRemove as u8;
        const ADDRESS_ADD: u8 = codes::Migration::AddressAdd as u8;

        let FileByteSpan {
            file_hash: file_id,
            byte_span,
        } = self.find_file_location(&diag);
        let file_change_entry = self.changes.entry(file_id).or_default();
        let change = match (diag.info().category(), diag.info().code()) {
            (CAT, NEEDS_MUT) => MigrationChange::AddMut,
            (CAT, NEEDS_PUBLIC) => MigrationChange::AddPublic,
            (CAT, NEEDS_BACKTICKS) => {
                let old_name = diag.primary_msg().to_string();
                MigrationChange::Backquote(old_name)
            }
            (CAT, NEEDS_GLOBAL_QUAL) => MigrationChange::AddGlobalQual,
            (CAT, REMOVE_FRIEND) => MigrationChange::RemoveFriend,
            (CAT, MAKE_PUB_PACKAGE) => MigrationChange::MakePubPackage,
            (CAT, ADDRESS_REMOVE) => MigrationChange::AddressRemove,
            (CAT, ADDRESS_ADD) => {
                let insertion = diag.primary_msg().to_string();
                MigrationChange::AddressAdd(insertion)
            }
            _ => unreachable!(),
        };
        file_change_entry.push((byte_span, change));
    }

    fn find_file_location(&mut self, diag: &Diagnostic) -> FileByteSpan {
        let (loc, _msg) = &diag.primary_label;
        self.mapped_files.byte_span(loc)
    }

    fn get_file_contents(&self, file_id: FileId) -> String {
        self.mapped_files
            .files()
            .source(file_id)
            .unwrap()
            .to_string()
    }

    fn render_changes(source: String, changes: &mut [(ByteSpan, MigrationChange)]) -> String {
        changes.sort_by(|(loc0, _), (loc1, _)| loc0.start.partial_cmp(&loc1.start).unwrap());
        let mut output = "".to_string();

        let mut source_prefix = &source[..];
        let mut last_seen = source_prefix.len();
        for (loc, change) in changes.iter().rev() {
            assert!(loc.end <= last_seen, "Found overlapping migrations.");
            match change {
                MigrationChange::AddMut => {
                    let rest = &source_prefix[loc.start..];
                    output = format!("mut {}{}", rest, output);
                }
                MigrationChange::AddPublic => {
                    let rest = &source_prefix[loc.start..];
                    output = format!("public {}{}", rest, output);
                }
                MigrationChange::Backquote(old_name) => {
                    let rest = &source_prefix[loc.end..];
                    output = format!("`{}`{}{}", old_name, rest, output);
                }
                MigrationChange::AddGlobalQual => {
                    let rest = &source_prefix[loc.start..];
                    output = format!("::{}{}", rest, output);
                }
                MigrationChange::RemoveFriend => {
                    let rest = &source_prefix[loc.end..];
                    output = format!(
                        "/* {} */{}{}",
                        &source_prefix[loc.start..loc.end],
                        rest,
                        output
                    );
                }
                MigrationChange::MakePubPackage => {
                    let rest = &source_prefix[loc.end..];
                    output = format!("public(package){}{}", rest, output);
                }
                MigrationChange::AddressRemove => {
                    let rest = &source_prefix[loc.end..];
                    output = format!(
                        "/* {} */{}{}",
                        &source_prefix[loc.start..loc.end],
                        rest,
                        output
                    );
                }
                MigrationChange::AddressAdd(insertion) => {
                    let rest = &source_prefix[loc.start..];
                    output = format!("{}{}{}", insertion, rest, output);
                }
            }
            source_prefix = &source_prefix[..loc.start];
            last_seen = loc.start;
        }
        output = format!("{}{}", source_prefix, output);

        output
    }

    pub fn render_output(&mut self) -> String {
        let mut output = vec![];
        let mut names = self
            .changes
            .keys()
            .cloned()
            .map(|hash| (hash, self.mapped_files.file_hash_to_file_id(&hash).unwrap()))
            .map(|(hash, id)| (hash, id, *self.mapped_files.files().get(id).unwrap().name()))
            .collect::<Vec<_>>();
        names.sort_by_key(|(_, _, name)| *name);
        for (file_hash, file_id, name) in names {
            let original = self.get_file_contents(file_id);
            let file_changes = self.changes.get_mut(&file_hash).unwrap();
            Self::ensure_unique_changes(file_changes);
            let migrated = Self::render_changes(original.clone(), file_changes);
            let diff = similar::TextDiff::from_lines(&original, &migrated);
            output.push(
                diff.unified_diff()
                    .context_radius(0)
                    .header(&name, &name)
                    .to_string(),
            );
        }

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
            .cloned()
            .map(|hash| (hash, self.mapped_files.file_hash_to_file_id(&hash).unwrap()))
            .map(|(hash, id)| (hash, id, *self.mapped_files.files().get(id).unwrap().name()))
            .collect::<Vec<_>>();
        names.sort_by_key(|(_, _, name)| *name);
        for (file_hash, file_id, name) in names {
            let original = self.get_file_contents(file_id);
            let file_changes = self.changes.get_mut(&file_hash).unwrap();
            Self::ensure_unique_changes(file_changes);
            let migrated = Self::render_changes(original.clone(), file_changes);
            let path = PathBuf::from(name.to_string());
            writeln!(w, "Updating {:#?} . . .", path)?;
            Self::ensure_unique_changes(file_changes);
            std::fs::write(path, migrated)?;
        }
        Ok(())
    }

    fn ensure_unique_changes(changes: &mut Vec<(ByteSpan, MigrationChange)>) {
        let file_changes = std::mem::take(changes);
        let mut set_changes = BTreeSet::new();
        for change in file_changes {
            set_changes.insert(change);
        }
        let out_changes = set_changes.into_iter().collect::<Vec<_>>();
        let _ = std::mem::replace(changes, out_changes);
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
            return Self {
                diags: None,
                format: DiagnosticsFormat::default(),
            };
        }

        let mut severity_count = BTreeMap::new();
        for diag in &diagnostics {
            *severity_count.entry(diag.info.severity()).or_insert(0) += 1;
        }
        Self {
            diags: Some(Diagnostics_ {
                diagnostics,
                filtered_source_diagnostics: vec![],
                severity_count,
            }),
            format: DiagnosticsFormat::default(),
        }
    }
}

impl From<Option<Diagnostic>> for Diagnostics {
    fn from(diagnostic_opt: Option<Diagnostic>) -> Self {
        Diagnostics::from(diagnostic_opt.map_or_else(Vec::new, |diag| vec![diag]))
    }
}

impl<C: DiagnosticCode> From<C> for DiagnosticInfo {
    fn from(value: C) -> Self {
        value.into_info()
    }
}
