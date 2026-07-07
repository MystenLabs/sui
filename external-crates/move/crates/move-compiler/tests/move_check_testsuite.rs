// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use move_binary_format::file_format::FunctionDefinitionIndex;
use move_bytecode_source_map::source_map::{MacroFrameInfoEntry, MacroFrameKind};
use move_command_line_common::{
    env::read_bool_env_var,
    files::{FileHash, MOVE_EXTENSION},
    insta_assert,
    testing::{InstaOptions, OUT_EXT},
};
use move_compiler::{
    Compiler, PASS_PARSER,
    command_line::compiler::move_check_for_errors,
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::filter::{empty_filter_scope, unused_for_test_filter_scope},
    diagnostics::*,
    editions::{Edition, Flavor},
    linters::{self, LintLevel},
    shared::{
        Flags, NumericalAddress, PackageConfig, PackagePaths, files::MappedFiles,
        macro_frames::MACRO_FRAMES_MODE,
    },
    sui_mode,
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Shared flag to keep any temporary results of the test
const KEEP_TMP: &str = "KEEP";

const TEST_EXT: &str = "unit_test";
const UNUSED_EXT: &str = "unused";
const MIGRATION_EXT: &str = "migration";
const IDE_EXT: &str = "ide";
const NO_STDLIB_EXT: &str = "no_std";
const MACRO_FRAMES_EXT: &str = "macro_frames";
const SOURCE_MAP_EXT: &str = "source_map";
const MODE_EXT: &str = "mode";

const LINTER_DIR: &str = "linter";
const SUI_MODE_DIR: &str = "sui_mode";
const MOVE_2024_DIR: &str = "move_2024";
const DEV_DIR: &str = "development";

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct TestInfo {
    flavor: Flavor,
    edition: Edition,
    lint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TestKind {
    // Normal test
    Normal,
    // Tests unit test functionality
    Test,
    // Does not silence warnings for unused items
    Unused,
    // Tests edition migration
    Migration,
    // Tests additional generation for the IDE
    IDE,
    // Tests with std library disabled
    NoStd,
    // Tests macro frame info for debugger support
    MacroFrames,
    // Tests generated source maps for debugger support
    SourceMap,
    // Tests with a mode enabled
    Mode(Vec<Symbol>),
}

impl TestKind {
    fn from_extension(path_extension: &std::ffi::OsStr) -> Self {
        match () {
            _ if path_extension == MOVE_EXTENSION => TestKind::Normal,
            _ if path_extension == TEST_EXT => TestKind::Test,
            _ if path_extension == UNUSED_EXT => TestKind::Unused,
            _ if path_extension == MIGRATION_EXT => TestKind::Migration,
            _ if path_extension == IDE_EXT => TestKind::IDE,
            _ if path_extension == NO_STDLIB_EXT => TestKind::NoStd,
            _ if path_extension == MACRO_FRAMES_EXT => TestKind::MacroFrames,
            _ if path_extension == SOURCE_MAP_EXT => TestKind::SourceMap,
            _ if path_extension.to_string_lossy().starts_with(MODE_EXT) => {
                let pe_str = path_extension.to_string_lossy();
                let mode_str = pe_str.strip_prefix(MODE_EXT).unwrap();
                let modes = mode_str
                    .split('-')
                    .map(|str| str.into())
                    .collect::<Vec<_>>();
                TestKind::Mode(modes)
            }
            _ => panic!("Unknown extension: {}", path_extension.to_string_lossy()),
        }
    }

    fn snap_suffix(&self) -> Option<String> {
        match self {
            TestKind::Normal => None,
            TestKind::Test => Some(TEST_EXT.to_string()),
            TestKind::Unused => Some(UNUSED_EXT.to_string()),
            TestKind::Migration => Some(MIGRATION_EXT.to_string()),
            TestKind::IDE => Some(IDE_EXT.to_string()),
            TestKind::NoStd => Some(NO_STDLIB_EXT.to_string()),
            TestKind::MacroFrames => Some(MACRO_FRAMES_EXT.to_string()),
            TestKind::SourceMap => Some(SOURCE_MAP_EXT.to_string()),
            TestKind::Mode(modes) => Some(format!(
                "{MODE_EXT}{}",
                modes
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join("-")
            )),
        }
    }
}

fn default_testing_addresses(flavor: Flavor) -> BTreeMap<String, NumericalAddress> {
    let mut mapping = vec![
        ("std", "0x1"),
        ("sui", "0x2"),
        ("M", "0x40"),
        ("A", "0x41"),
        ("B", "0x42"),
        ("K", "0x19"),
        ("a", "0x44"),
        ("b", "0x45"),
        ("k", "0x19"),
    ];
    if flavor == Flavor::Sui {
        mapping.extend([("sui", "0x2"), ("sui_system", "0x3")]);
    }
    mapping
        .into_iter()
        .map(|(name, addr)| (name.to_string(), NumericalAddress::parse_str(addr).unwrap()))
        .collect::<BTreeMap<_, _>>()
}

fn test_config(path: &Path) -> (TestKind, TestInfo, PackageConfig, Flags) {
    let test_kind = TestKind::from_extension(path.extension().unwrap());
    let path_contains = |s| path.components().any(|c| c.as_os_str() == s);
    let lint = path_contains(LINTER_DIR);
    let flavor = if path_contains(SUI_MODE_DIR) {
        Flavor::Sui
    } else {
        Flavor::default()
    };
    let move_2024_mode = path_contains(MOVE_2024_DIR);
    let dev_mode = path_contains(DEV_DIR);
    assert!(
        [move_2024_mode, dev_mode]
            .into_iter()
            .filter(|x| *x)
            .count()
            <= 1,
        "A test can have at most directory based edition"
    );
    let edition = if test_kind == TestKind::Migration {
        // migration mode overrides the edition
        Edition::E2024_MIGRATION
    } else if move_2024_mode {
        Edition::E2024_ALPHA
    } else if dev_mode {
        Edition::DEVELOPMENT
    } else {
        Edition::LEGACY
    };
    // config
    let warning_filter = if matches!(
        test_kind,
        TestKind::Unused | TestKind::IDE | TestKind::MacroFrames | TestKind::SourceMap
    ) {
        empty_filter_scope()
    } else {
        unused_for_test_filter_scope()
    };
    let config = PackageConfig {
        flavor,
        edition,
        is_dependency: false,
        warning_filter,
    };
    // test info
    let test_info = TestInfo {
        flavor,
        edition,
        lint,
    };
    // flags
    let flags = match &test_kind {
        // no flags for normal tests and no-stdlib tests
        TestKind::Normal | TestKind::NoStd => Flags::empty(),
        // we want to be able to see test/test_only elements in these modes
        TestKind::Test | TestKind::Unused | TestKind::Migration => Flags::testing(),
        // additional flags for IDE
        TestKind::IDE => Flags::testing().set_ide_test_mode(true).set_ide_mode(true),
        // MacroFrames tests use a special mode to emit only macro frame diagnostics
        TestKind::MacroFrames => Flags::empty().set_modes(vec![MACRO_FRAMES_MODE.into()]),
        TestKind::SourceMap => Flags::empty(),
        // Setting a mode flag
        TestKind::Mode(modes) => Flags::empty().set_modes(modes.clone()),
    };
    (test_kind, test_info, config, flags)
}

fn out_path(path: &Path, test_name: &str, test_kind: &Option<String>) -> PathBuf {
    let n;
    let file_name = match test_kind {
        Some(c) => {
            n = format!("{test_name}@{c}");
            &n
        }
        None => test_name,
    };
    path.with_file_name(file_name).with_extension(OUT_EXT)
}

fn render_source_map_snapshot(files: &MappedFiles, units: &[AnnotatedCompiledUnit]) -> String {
    let mut out = String::new();
    render_sources(&mut out, files, units);

    for (unit_idx, unit) in units.iter().enumerate() {
        if unit_idx > 0 {
            out.push('\n');
        }
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        let module = &unit.named_module.module;
        let source_map = &unit.named_module.source_map;
        let (addr, module_name) = &source_map.module_name;
        let _ = writeln!(out, "module {}::{module_name}", addr.short_str_lossless());

        for (fdef_idx, function_source_map) in source_map.function_map_iter() {
            let fdef_idx = FunctionDefinitionIndex(fdef_idx);
            let fdef = module.function_def_at(fdef_idx);
            let fhandle = module.function_handle_at(fdef.function);
            let function_name = module.identifier_at(fhandle.name);

            let _ = writeln!(out, "\nfunction {function_name}:");
            let _ = writeln!(
                out,
                "  location: {}",
                format_loc_with_snippet(files, function_source_map.location)
            );
            let _ = writeln!(
                out,
                "  definition: {}",
                format_loc_with_snippet(files, function_source_map.definition_location)
            );
            render_source_names(
                &mut out,
                "parameters",
                function_source_map
                    .parameters
                    .iter()
                    .map(|(n, loc)| (n, *loc)),
                files,
            );
            render_locs(
                &mut out,
                "returns",
                function_source_map.returns.iter().copied(),
                files,
            );
            render_source_names(
                &mut out,
                "locals",
                function_source_map.locals.iter().map(|(n, loc)| (n, *loc)),
                files,
            );
            render_code_map(&mut out, function_source_map.code_map.iter(), files);
            render_macro_frames(&mut out, &function_source_map.macro_frame_info, files);
            render_macro_color_map(&mut out, &function_source_map.macro_color_map);
        }
    }
    out
}

fn render_sources(out: &mut String, files: &MappedFiles, units: &[AnnotatedCompiledUnit]) {
    let mut source_hashes = BTreeSet::new();
    for unit in units {
        collect_unit_source_hashes(unit, &mut source_hashes);
    }

    for (source_idx, file_hash) in source_hashes.into_iter().enumerate() {
        let Some((_file_name, source)) = files.get(&file_hash) else {
            continue;
        };
        if source_idx > 0 {
            out.push('\n');
        }
        let _ = writeln!(out, "source {}:", format_file_hash(file_hash, 5));
        let line_width = source.lines().count().max(1).to_string().len();
        for (line_idx, line) in source.lines().enumerate() {
            let _ = writeln!(out, "  {:>line_width$} | {line}", line_idx + 1);
        }
    }
}

fn collect_unit_source_hashes(
    unit: &AnnotatedCompiledUnit,
    source_hashes: &mut BTreeSet<FileHash>,
) {
    collect_loc_source_hash(unit.loc, source_hashes);
    collect_loc_source_hash(unit.module_name_loc, source_hashes);
    let source_map = &unit.named_module.source_map;
    collect_loc_source_hash(source_map.definition_location, source_hashes);
    for (_, function_source_map) in source_map.function_map_iter() {
        collect_loc_source_hash(function_source_map.location, source_hashes);
        collect_loc_source_hash(function_source_map.definition_location, source_hashes);
        for (_, loc) in &function_source_map.type_parameters {
            collect_loc_source_hash(*loc, source_hashes);
        }
        for (_, loc) in &function_source_map.parameters {
            collect_loc_source_hash(*loc, source_hashes);
        }
        for loc in &function_source_map.returns {
            collect_loc_source_hash(*loc, source_hashes);
        }
        for (_, loc) in &function_source_map.locals {
            collect_loc_source_hash(*loc, source_hashes);
        }
        for loc in function_source_map.code_map.values() {
            collect_loc_source_hash(*loc, source_hashes);
        }
        for frame in &function_source_map.macro_frame_info {
            collect_loc_source_hash(frame.source_loc, source_hashes);
            collect_loc_source_hash(frame.call_loc, source_hashes);
        }
    }
}

fn collect_loc_source_hash(loc: Loc, source_hashes: &mut BTreeSet<FileHash>) {
    if loc.is_valid() {
        source_hashes.insert(loc.file_hash());
    }
}

fn render_source_names<'a>(
    out: &mut String,
    label: &str,
    source_names: impl Iterator<Item = (&'a String, Loc)>,
    files: &MappedFiles,
) {
    let source_names = source_names.collect::<Vec<_>>();
    if source_names.is_empty() {
        return;
    }
    let _ = writeln!(out, "  {label}:");
    for (idx, (name, loc)) in source_names.into_iter().enumerate() {
        let _ = writeln!(
            out,
            "    {idx} {name}: {}",
            format_loc_with_snippet(files, loc)
        );
    }
}

fn render_locs(
    out: &mut String,
    label: &str,
    locs: impl Iterator<Item = Loc>,
    files: &MappedFiles,
) {
    let locs = locs.collect::<Vec<_>>();
    if locs.is_empty() {
        return;
    }
    let _ = writeln!(out, "  {label}:");
    for (idx, loc) in locs.into_iter().enumerate() {
        let _ = writeln!(out, "    {idx}: {}", format_loc_with_snippet(files, loc));
    }
}

fn render_code_map<'a>(
    out: &mut String,
    code_map: impl Iterator<Item = (&'a u16, &'a Loc)>,
    files: &MappedFiles,
) {
    let code_map = code_map.collect::<Vec<_>>();
    if code_map.is_empty() {
        return;
    }
    let _ = writeln!(out, "  code_map:");
    for (offset, loc) in code_map {
        let _ = writeln!(
            out,
            "    {offset}: {}",
            format_loc_with_snippet(files, *loc)
        );
    }
}

fn render_macro_frames(out: &mut String, frames: &[MacroFrameInfoEntry], files: &MappedFiles) {
    if frames.is_empty() {
        return;
    }
    let _ = writeln!(out, "  macro_frame_info:");
    for (idx, frame) in frames.iter().enumerate() {
        let parent = frame
            .parent_index
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(out, "    [{idx}] {}", format_macro_frame_kind(frame));
        let _ = writeln!(out, "        parent: {parent}");
        let _ = writeln!(
            out,
            "        source: {}",
            format_loc_with_snippet(files, frame.source_loc)
        );
        let _ = writeln!(
            out,
            "        call: {}",
            format_loc_with_snippet(files, frame.call_loc)
        );
    }
}

fn render_macro_color_map(out: &mut String, color_map: &[(u16, Option<u32>)]) {
    if color_map.is_empty() {
        return;
    }
    let _ = writeln!(out, "  macro_color_map:");
    for (offset, frame_idx) in color_map {
        let frame_idx = frame_idx
            .map(|idx| idx.to_string())
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(out, "    {offset} -> {frame_idx}");
    }
}

fn format_macro_frame_kind(frame: &MacroFrameInfoEntry) -> String {
    match &frame.kind {
        MacroFrameKind::MacroBody {
            module_addr,
            module_name,
            function_name,
        } => format!(
            "MacroBody({}::{module_name}::{function_name})",
            module_addr.short_str_lossless()
        ),
        MacroFrameKind::Lambda => "Lambda".to_string(),
        MacroFrameKind::Argument => "Argument".to_string(),
    }
}

fn format_loc(files: &MappedFiles, loc: Loc) -> String {
    if !loc.is_valid() {
        return "-".to_string();
    }
    let Some(position) = files.position_opt(&loc) else {
        return format!("bytes {}-{}", loc.start(), loc.end());
    };
    format!(
        "{}:{}-{}:{}",
        position.start.user_line(),
        position.start.user_column(),
        position.end.user_line(),
        position.end.user_column(),
    )
}

fn format_loc_with_snippet(files: &MappedFiles, loc: Loc) -> String {
    let loc_str = format_loc(files, loc);
    let Some(snippet) = snippet(files, loc) else {
        return loc_str;
    };
    format!("{loc_str} `{snippet}`")
}

fn snippet(files: &MappedFiles, loc: Loc) -> Option<String> {
    let source = files.source_of_loc_opt(&loc)?.trim();
    if source.is_empty() {
        return None;
    }
    let mut lines = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let mut snippet = lines.next()?.to_string();
    if lines.next().is_some() {
        snippet.push_str(" ...");
    }
    if snippet.len() > 80 {
        snippet.truncate(77);
        snippet.push_str("...");
    }
    Some(snippet.replace('`', "\\`"))
}

fn render_raw_source_map_snapshot(
    units: &[AnnotatedCompiledUnit],
) -> datatest_stable::Result<String> {
    let mut value = if units.len() == 1 {
        serde_json::to_value(&units[0].named_module.source_map)?
    } else {
        Value::Array(
            units
                .iter()
                .map(|unit| serde_json::to_value(&unit.named_module.source_map))
                .collect::<Result<Vec<_>, _>>()?,
        )
    };
    abbreviate_file_hashes(&mut value);
    Ok(format_raw_source_map_json(&value))
}

fn format_raw_source_map_json(value: &Value) -> String {
    let mut out = String::new();
    write_json_value(&mut out, value, 0);
    out
}

fn write_json_value(out: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Object(map) if is_json_loc(map) => write_compact_json_loc(out, map),
        Value::Object(map) => write_json_object(out, map, indent),
        Value::Array(values) if is_json_name_loc_pair(values) => {
            write_compact_json_name_loc_pair(out, values)
        }
        Value::Array(values) => write_json_array(out, values, indent),
        _ => out.push_str(&serde_json::to_string(value).unwrap()),
    }
}

fn is_json_loc(map: &serde_json::Map<String, Value>) -> bool {
    map.len() == 3
        && matches!(map.get("file_hash"), Some(Value::String(_)))
        && matches!(map.get("start"), Some(Value::Number(_)))
        && matches!(map.get("end"), Some(Value::Number(_)))
}

fn write_compact_json_loc(out: &mut String, map: &serde_json::Map<String, Value>) {
    out.push('{');
    out.push_str("\"file_hash\": ");
    out.push_str(&serde_json::to_string(map.get("file_hash").unwrap()).unwrap());
    out.push_str(", \"start\": ");
    out.push_str(&serde_json::to_string(map.get("start").unwrap()).unwrap());
    out.push_str(", \"end\": ");
    out.push_str(&serde_json::to_string(map.get("end").unwrap()).unwrap());
    out.push('}');
}

fn is_json_name_loc_pair(values: &[Value]) -> bool {
    values.len() == 2
        && matches!(values.first(), Some(Value::String(_)))
        && matches!(values.get(1), Some(Value::Object(map)) if is_json_loc(map))
}

fn write_compact_json_name_loc_pair(out: &mut String, values: &[Value]) {
    out.push('[');
    out.push_str(&serde_json::to_string(&values[0]).unwrap());
    out.push_str(", ");
    write_json_value(out, &values[1], 0);
    out.push(']');
}

fn write_json_object(out: &mut String, map: &serde_json::Map<String, Value>, indent: usize) {
    if map.is_empty() {
        out.push_str("{}");
        return;
    }

    let entries = sorted_json_object_entries(map);
    out.push('{');
    for (idx, &(key, value)) in entries.iter().enumerate() {
        out.push('\n');
        write_indent(out, indent + 2);
        out.push_str(&serde_json::to_string(key).unwrap());
        out.push_str(": ");
        if key == "macro_color_map" {
            write_compact_macro_color_map(out, value, indent + 2);
        } else {
            write_json_value(out, value, indent + 2);
        }
        if idx + 1 != entries.len() {
            out.push(',');
        }
    }
    out.push('\n');
    write_indent(out, indent);
    out.push('}');
}

fn sorted_json_object_entries(map: &serde_json::Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut entries = map.iter().collect::<Vec<_>>();
    if entries.iter().all(|(key, _)| key.parse::<u64>().is_ok()) {
        entries.sort_by_key(|(key, _)| key.parse::<u64>().unwrap());
    }
    entries
}

fn write_json_array(out: &mut String, values: &[Value], indent: usize) {
    if values.is_empty() {
        out.push_str("[]");
        return;
    }

    out.push('[');
    for (idx, value) in values.iter().enumerate() {
        out.push('\n');
        write_indent(out, indent + 2);
        write_json_value(out, value, indent + 2);
        if idx + 1 != values.len() {
            out.push(',');
        }
    }
    out.push('\n');
    write_indent(out, indent);
    out.push(']');
}

fn write_compact_macro_color_map(out: &mut String, value: &Value, indent: usize) {
    let Value::Array(entries) = value else {
        write_json_value(out, value, indent);
        return;
    };
    if entries.is_empty() {
        out.push_str("[]");
        return;
    }

    out.push('[');
    for (idx, entry) in entries.iter().enumerate() {
        out.push('\n');
        write_indent(out, indent + 2);
        match entry {
            Value::Array(pair) if pair.len() == 2 => {
                out.push('[');
                out.push_str(&serde_json::to_string(&pair[0]).unwrap());
                out.push_str(", ");
                out.push_str(&serde_json::to_string(&pair[1]).unwrap());
                out.push(']');
            }
            _ => write_json_value(out, entry, indent + 2),
        }
        if idx + 1 != entries.len() {
            out.push(',');
        }
    }
    out.push('\n');
    write_indent(out, indent);
    out.push(']');
}

fn write_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push(' ');
    }
}

fn abbreviate_file_hashes(value: &mut Value) {
    let mut hashes = BTreeSet::new();
    collect_json_file_hashes(value, &mut hashes);
    let labels = abbreviated_hash_labels(&hashes.into_iter().collect::<Vec<_>>());
    replace_json_file_hashes(value, &labels);
}

fn collect_json_file_hashes(value: &Value, hashes: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(hash)) = map.get("file_hash") {
                hashes.insert(hash.clone());
            }
            for value in map.values() {
                collect_json_file_hashes(value, hashes);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_json_file_hashes(value, hashes);
            }
        }
        _ => (),
    }
}

fn abbreviated_hash_labels(hashes: &[String]) -> BTreeMap<String, String> {
    let mut suffix_len = 5;
    loop {
        let labels = hashes
            .iter()
            .map(|hash| (hash.clone(), format_hash_str(hash, suffix_len)))
            .collect::<BTreeMap<_, _>>();
        let unique_labels = labels.values().collect::<BTreeSet<_>>();
        if unique_labels.len() == labels.len() || hashes.iter().all(|hash| suffix_len >= hash.len())
        {
            return labels;
        }
        suffix_len += 1;
    }
}

fn replace_json_file_hashes(value: &mut Value, labels: &BTreeMap<String, String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(hash)) = map.get_mut("file_hash")
                && let Some(label) = labels.get(hash)
            {
                *hash = label.clone();
            }
            for value in map.values_mut() {
                replace_json_file_hashes(value, labels);
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_json_file_hashes(value, labels);
            }
        }
        _ => (),
    }
}

fn format_file_hash(file_hash: FileHash, suffix_len: usize) -> String {
    format_hash_str(&file_hash.to_string(), suffix_len)
}

fn format_hash_str(hash: &str, suffix_len: usize) -> String {
    if hash.len() <= suffix_len {
        format!("0x{hash}")
    } else {
        format!("0x..{}", &hash[hash.len() - suffix_len..])
    }
}

fn run_source_map_test(
    compiler: Compiler,
    move_path: PathBuf,
    test_name: &str,
    suffix: Option<String>,
    test_info: TestInfo,
) -> datatest_stable::Result<()> {
    let (files, units_res) = compiler.build()?;
    let (units, warnings) = match units_res {
        Ok(res) => res,
        Err(diags) => {
            let diag_buffer =
                report_diagnostics_to_buffer(&files, diags, /* ansi_color */ false);
            return Err(anyhow::anyhow!(
                "Source map test failed to compile:\n{}",
                std::str::from_utf8(&diag_buffer)?
            )
            .into());
        }
    };
    if !warnings.is_empty() {
        let diag_buffer =
            report_diagnostics_to_buffer(&files, warnings, /* ansi_color */ false);
        return Err(anyhow::anyhow!(
            "Source map test emitted warnings:\n{}",
            std::str::from_utf8(&diag_buffer)?
        )
        .into());
    }

    let rendered = render_source_map_snapshot(&files, &units);
    let mut options = InstaOptions::new();
    options.info(test_info);
    if let Some(suffix) = &suffix {
        options.suffix(suffix.clone());
    }
    options.name(test_name);
    let readable_snapshot_path = move_path.clone();
    insta_assert! {
        input_path: readable_snapshot_path,
        contents: rendered,
        options: options,
    };

    let raw_rendered = render_raw_source_map_snapshot(&units)?;
    let mut raw_options = InstaOptions::new();
    raw_options.info(test_info);
    raw_options.suffix(match suffix {
        Some(suffix) => format!("{suffix}_raw"),
        None => "source_map_raw".to_string(),
    });
    raw_options.name(test_name);
    insta_assert! {
        input_path: move_path,
        contents: raw_rendered,
        options: raw_options,
    };
    Ok(())
}

// Runs all tests under the test/testsuite directory.
pub fn run_test(path: &Path) -> datatest_stable::Result<()> {
    let (test_kind, test_info, package_config, flags) = test_config(path);
    let suffix = test_kind.snap_suffix();
    let migration_mode = package_config.edition == Edition::E2024_MIGRATION;
    let test_name = path.file_stem().unwrap().to_string_lossy();
    let test_name: &str = test_name.as_ref();
    let move_path = path.with_extension(MOVE_EXTENSION);
    let out_path = out_path(path, test_name, &suffix);
    let flavor = package_config.flavor;
    let targets: Vec<String> = vec![move_path.to_str().unwrap().to_owned()];
    let named_address_map = default_testing_addresses(flavor);
    let deps = if matches!(test_kind, TestKind::NoStd) {
        vec![]
    } else {
        vec![PackagePaths {
            name: Some(("stdlib".into(), PackageConfig::default())),
            paths: move_stdlib::source_files(),
            named_address_map: named_address_map.clone(),
        }]
    };
    let target_name = if migration_mode {
        Some(("test".into(), package_config.clone()))
    } else {
        None
    };
    let targets = vec![PackagePaths {
        name: target_name,
        paths: targets,
        named_address_map,
    }];

    let flags = flags.set_sources_shadow_deps(true);
    let mut compiler = Compiler::from_package_paths(None, targets, deps)
        .unwrap()
        .set_flags(flags)
        .set_default_config(package_config);

    if flavor == Flavor::Sui {
        let (prefix, filters) = sui_mode::linters::known_filters();
        compiler = compiler.add_custom_known_filters(prefix, filters);
        if test_info.lint {
            compiler = compiler.add_visitors(sui_mode::linters::linter_visitors(LintLevel::All))
        }
    }
    let (prefix, filters) = linters::known_filters();
    compiler = compiler.add_custom_known_filters(prefix, filters);
    if test_info.lint {
        compiler = compiler.add_visitors(linters::linter_visitors(LintLevel::All))
    }

    if matches!(test_kind, TestKind::SourceMap) {
        return run_source_map_test(compiler, move_path, test_name, suffix, test_info);
    }

    let (files, comments_and_compiler_res) = compiler.run::<PASS_PARSER>()?;
    let diags = move_check_for_errors(comments_and_compiler_res);

    let has_diags = !diags.is_empty();
    let diag_buffer = if has_diags {
        if migration_mode {
            report_migration_to_buffer(&files, diags)
        } else {
            report_diagnostics_to_buffer(&files, diags, /* ansi_color */ false)
        }
    } else {
        vec![]
    };

    let save_diags = read_bool_env_var(KEEP_TMP);

    let rendered_diags = std::str::from_utf8(&diag_buffer)?;
    if save_diags {
        fs::write(out_path, &diag_buffer)?;
    }

    // Macro frame diagnostics render color/location inconsistencies as `!!`
    // markers. Fail outright rather than relying on snapshot comparison, so
    // that a newly added test cannot commit a snapshot containing a
    // violation unnoticed.
    if matches!(test_kind, TestKind::MacroFrames) && rendered_diags.contains("!!") {
        return Err(anyhow::anyhow!(
            "Macro frames test output contains `!!` invariant violation markers:\n{rendered_diags}"
        )
        .into());
    }

    let mut options = InstaOptions::new();
    options.info(test_info);
    if let Some(suffix) = suffix {
        options.suffix(suffix);
    }
    options.name(test_name);
    insta_assert! {
        input_path: move_path,
        contents: rendered_diags,
        options: options,
    };
    Ok(())
}

datatest_stable::harness!(
    run_test,
    "tests/",
    r".*\.move$",
    run_test,
    "tests/",
    r".*\.unit_test$",
    run_test,
    "tests/",
    r".*\.unused$",
    run_test,
    "tests/",
    r".*\.migration$",
    run_test,
    "tests/",
    r".*\.ide$",
    run_test,
    "tests/",
    r".*\.no_std$",
    run_test,
    "tests/",
    r".*\.macro_frames$",
    run_test,
    "tests/",
    r".*\.source_map$",
    run_test,
    "tests/",
    r".*\.mode-.*$",
);
