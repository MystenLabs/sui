// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::format_module_id;
use colored::{control, Colorize};
use move_binary_format::errors::{ExecutionState, Location, VMError};
use move_command_line_common::error_bitset::ErrorBitset;
use move_compiler::{
    diagnostics::{self, Diagnostic, Diagnostics},
    unit_test::{ModuleTestPlan, MoveErrorType, TestPlan},
};
use move_core_types::{
    language_storage::ModuleId,
    vm_status::{StatusCode, StatusType},
};
use move_ir_types::location::Loc;
use move_trace_format::format::MoveTrace;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Result, Write},
    path::Path,
    sync::Mutex,
    time::Duration,
};

pub use move_compiler::unit_test::ExpectedMoveError as MoveError;

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub enum FailureReason {
    // Expected to error, but it didn't
    NoError(String),
    // Aborted with the wrong code
    WrongError(String, MoveError, MoveError),
    // Aborted with the wrong code, without location specified
    WrongAbortDEPRECATED(String, MoveErrorType, MoveError),
    // Error wasn't expected, but it did
    UnexpectedError(String, MoveError),
    // Test timed out
    Timeout(String),
    // Property checking failed
    Property(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFailure {
    pub test_run_info: TestRunInfo,
    pub vm_error: Option<VMError>,
    pub failure_reason: FailureReason,
    pub prng_seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRunInfo {
    pub elapsed_time: Duration,
    pub instructions_executed: u64,
    pub trace: Option<MoveTrace>,
}

type TestRuns<T> = BTreeMap<String, Vec<T>>;

#[derive(Debug, Clone)]
pub struct TestStatistics {
    passed: BTreeMap<ModuleId, TestRuns<TestRunInfo>>,
    failed: BTreeMap<ModuleId, TestRuns<TestFailure>>,
}

// #[derive(Debug, Clone)]
pub struct TestResults {
    final_statistics: TestStatistics,
    test_plan: TestPlan,
}

fn write_string_to_file(filepath: &str, content: &str) -> std::io::Result<()> {
    let path = Path::new(filepath);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

impl TestRunInfo {
    pub fn new(
        elapsed_time: Duration,
        instructions_executed: u64,
        trace: Option<MoveTrace>,
    ) -> Self {
        Self {
            elapsed_time,
            instructions_executed,
            trace,
        }
    }

    pub fn save_trace(&self, path: &str) -> Result<()> {
        if let Some(trace) = &self.trace {
            write_string_to_file(path, &format!("{}", trace.to_json()))
        } else {
            Ok(())
        }
    }
}

impl FailureReason {
    pub fn no_error() -> Self {
        FailureReason::NoError("Test did not error as expected".to_string())
    }

    pub fn wrong_error(expected: MoveError, actual: MoveError) -> Self {
        FailureReason::WrongError(
            "Test did not error as expected".to_string(),
            expected,
            actual,
        )
    }

    pub fn wrong_abort_deprecated(expected: MoveErrorType, actual: MoveError) -> Self {
        FailureReason::WrongAbortDEPRECATED(
            "Test did not abort with expected code".to_string(),
            expected,
            actual,
        )
    }

    pub fn unexpected_error(error: MoveError) -> Self {
        FailureReason::UnexpectedError("Test was not expected to error".to_string(), error)
    }

    pub fn timeout() -> Self {
        FailureReason::Timeout("Test timed out".to_string())
    }

    pub fn property(details: String) -> Self {
        FailureReason::Property(details)
    }
}

fn clever_error_line_number_to_loc(test_plan: &TestPlan, vm_error: &VMError) -> Option<Loc> {
    let abort_code = match (vm_error.major_status(), vm_error.sub_status()) {
        (StatusCode::ABORTED, Some(abort_code)) => abort_code,
        _ => return None,
    };
    let location = vm_error.location();
    let bitset = ErrorBitset::from_u64(abort_code)?;
    if bitset.identifier_index().is_some() || bitset.constant_index().is_some() {
        return None;
    }
    let line_number = bitset.line_number()? - 1;

    match location {
        Location::Undefined => None,
        Location::Module(module_id) => {
            let source_map = &test_plan.module_info.get(module_id)?.source_map;
            let file_hash = source_map.definition_location.file_hash();
            let loc = test_plan
                .mapped_files
                .line_to_loc_opt(&file_hash, line_number as usize)?;
            test_plan.mapped_files.trimmed_loc_opt(&loc)
        }
    }
}

impl TestFailure {
    pub fn new(
        failure_reason: FailureReason,
        test_run_info: TestRunInfo,
        vm_error: Option<VMError>,
        prng_seed: Option<u64>,
    ) -> Self {
        Self {
            test_run_info,
            vm_error,
            failure_reason,
            prng_seed,
        }
    }

    pub fn render_error(&self, test_plan: &TestPlan) -> String {
        match &self.failure_reason {
            FailureReason::NoError(message) => message.to_string(),
            FailureReason::Timeout(message) => message.to_string(),
            FailureReason::WrongError(message, expected, actual) => {
                let base_message = format!(
                    "{message}. Expected test {} but instead it {} rooted here",
                    expected
                        .with_context(&test_plan.module_info)
                        .present_tense(),
                    actual.with_context(&test_plan.module_info).past_tense(),
                );
                Self::report_error_with_location(test_plan, base_message, &self.vm_error)
            }
            FailureReason::WrongAbortDEPRECATED(message, expected_code, actual) => {
                let base_message = format!(
                    "{}. \
                    Expected test to abort with code {}, but instead it {} rooted here",
                    message,
                    expected_code,
                    actual.with_context(&test_plan.module_info).past_tense(),
                );
                Self::report_error_with_location(test_plan, base_message, &self.vm_error)
            }
            FailureReason::UnexpectedError(message, error) => {
                let prefix = match error.0.status_type() {
                    StatusType::Validation => "INTERNAL TEST ERROR: Unexpected Validation Error\n",
                    StatusType::Verification => {
                        "INTERNAL TEST ERROR: Unexpected Verification Error\n"
                    }
                    StatusType::InvariantViolation => {
                        "INTERNAL TEST ERROR: INTERNAL VM INVARIANT VIOLATION.\n"
                    }
                    StatusType::Deserialization => {
                        "INTERNAL TEST ERROR: Unexpected Deserialization Error\n"
                    }
                    StatusType::Unknown => "INTERNAL TEST ERROR: UNKNOWN ERROR.\n",
                    // execution errors are expected, so no message
                    StatusType::Execution => "",
                };
                let base_message = format!(
                    "{}{}, but it {} rooted here",
                    prefix,
                    message,
                    error.with_context(&test_plan.module_info).past_tense(),
                );
                Self::report_error_with_location(test_plan, base_message, &self.vm_error)
            }
            FailureReason::Property(message) => message.clone(),
        }
    }

    fn report_exec_state(test_plan: &TestPlan, exec_state: &ExecutionState) -> String {
        let stack_trace = exec_state.stack_trace();
        let mut buf = String::new();
        if !stack_trace.is_empty() {
            buf.push_str("stack trace\n");
            for frame in stack_trace {
                let module_id = &frame.0;
                let named_module = match test_plan.module_info.get(module_id) {
                    Some(v) => v,
                    None => return "\tmalformed stack trace (no module)".to_string(),
                };
                let function_source_map =
                    match named_module.source_map.get_function_source_map(frame.1) {
                        Ok(v) => v,
                        Err(_) => return "\tmalformed stack trace (no source map)".to_string(),
                    };
                // unwrap here is a mirror of the same unwrap in report_error_with_location
                let loc = function_source_map.get_code_location(frame.2).unwrap();
                let fn_handle_idx = named_module.module.function_def_at(frame.1).function;
                let fn_id_idx = named_module.module.function_handle_at(fn_handle_idx).name;
                let fn_name = named_module.module.identifier_at(fn_id_idx).as_str();
                let file_name = test_plan.mapped_files.filename(&loc.file_hash());
                let formatted_line = {
                    // Adjust lines by 1 to report 1-indexed
                    let position = test_plan.mapped_files.position(&loc);
                    let start_line = position.start.user_line();
                    let end_line = position.end.user_line();
                    if start_line == end_line {
                        format!("{}", start_line)
                    } else {
                        format!("{}-{}", start_line, end_line)
                    }
                };
                buf.push_str(
                    &format!(
                        "\t{}::{}({}:{})\n",
                        module_id.name(),
                        fn_name,
                        file_name,
                        formatted_line
                    )
                    .to_string(),
                );
            }
        }
        buf
    }

    fn report_error_with_location(
        test_plan: &TestPlan,
        base_message: String,
        vm_error: &Option<VMError>,
    ) -> String {
        let report_diagnostics = |mapped_files, diags| {
            diagnostics::report_diagnostics_to_buffer_with_mapped_files(
                mapped_files,
                diags,
                control::SHOULD_COLORIZE.should_colorize(),
            )
        };

        let vm_error = match vm_error {
            None => return base_message,
            Some(vm_error) => vm_error,
        };

        let diags = match vm_error.location() {
            Location::Module(module_id) => {
                let diag_opt = vm_error.offsets().first().and_then(|(fdef_idx, offset)| {
                    let function_source_map = test_plan
                        .module_info
                        .get(module_id)?
                        .source_map
                        .get_function_source_map(*fdef_idx)
                        .ok()?;
                    let loc = function_source_map.get_code_location(*offset).unwrap();

                    let alternate_location_opt =
                        clever_error_line_number_to_loc(test_plan, vm_error);
                    let loc =
                        if alternate_location_opt.is_some_and(|alt_loc| !loc.overlaps(&alt_loc)) {
                            alternate_location_opt.unwrap()
                        } else {
                            loc
                        };
                    let msg = format!(
                        "In this function in {}",
                        format_module_id(&test_plan.module_info, module_id)
                    );
                    // TODO(tzakian) maybe migrate off of move-langs diagnostics?
                    Some(Diagnostic::new(
                        diagnostics::codes::Tests::TestFailed,
                        (loc, base_message.clone()),
                        vec![(function_source_map.definition_location, msg)],
                        std::iter::empty::<String>(),
                    ))
                });
                match diag_opt {
                    None => base_message,
                    Some(diag) => String::from_utf8(report_diagnostics(
                        &test_plan.mapped_files,
                        Diagnostics::from(vec![diag]),
                    ))
                    .unwrap(),
                }
            }
            _ => base_message,
        };

        match vm_error.exec_state() {
            None => diags,
            Some(exec_state) => {
                let exec_state_str = Self::report_exec_state(test_plan, exec_state);
                if exec_state_str.is_empty() {
                    diags
                } else {
                    format!("{}\n{}", diags, exec_state_str)
                }
            }
        }
    }
}

impl Default for TestStatistics {
    fn default() -> Self {
        Self::new()
    }
}

impl TestStatistics {
    pub fn new() -> Self {
        Self {
            passed: BTreeMap::new(),
            failed: BTreeMap::new(),
        }
    }

    pub fn test_failure(
        &mut self,
        test_name: String,
        test_failure: TestFailure,
        test_plan: &ModuleTestPlan,
    ) -> bool {
        self.failed
            .entry(test_plan.module_id.clone())
            .or_default()
            .entry(test_name)
            .or_default()
            .push(test_failure);
        false
    }

    pub fn test_success(
        &mut self,
        test_name: String,
        test_info: TestRunInfo,
        test_plan: &ModuleTestPlan,
    ) -> bool {
        self.passed
            .entry(test_plan.module_id.clone())
            .or_default()
            .entry(test_name)
            .or_default()
            .push(test_info);
        true
    }

    pub fn combine(mut self, other: Self) -> Self {
        for (module_id, test_result) in other.passed {
            let entry = self.passed.entry(module_id).or_default();
            for (function_ident, test_run_info) in test_result {
                entry
                    .entry(function_ident)
                    .or_default()
                    .extend(test_run_info);
            }
        }
        for (module_id, test_result) in other.failed {
            let entry = self.failed.entry(module_id).or_default();
            entry.extend(test_result.into_iter());
        }
        self
    }
}

fn calculate_run_statistics<'a, I: IntoIterator<Item = &'a TestRunInfo>>(
    test_results: I,
) -> (Duration, u64) {
    test_results.into_iter().fold(
        (Duration::new(0, 0), 0),
        |(mut acc_time, mut acc_instrs), test_run_info| {
            acc_time += test_run_info.elapsed_time;
            acc_instrs += test_run_info.instructions_executed;
            (acc_time, acc_instrs)
        },
    )
}

impl TestResults {
    pub fn new(final_statistics: TestStatistics, test_plan: TestPlan) -> Self {
        Self {
            final_statistics,
            test_plan,
        }
    }

    pub fn report_statistics<W: Write>(
        &self,
        writer: &Mutex<W>,
        report_format: &Option<String>,
    ) -> Result<()> {
        if let Some(report_type) = report_format {
            if report_type == "csv" {
                writeln!(writer.lock().unwrap(), "name,nanos,gas")?;
                for (module_id, test_results) in self.final_statistics.passed.iter() {
                    for (function_name, test_results) in test_results {
                        let qualified_function_name = format!(
                            "{}::{}",
                            format_module_id(&self.test_plan.module_info, module_id),
                            function_name,
                        );
                        let (time, instrs_executed) = calculate_run_statistics(test_results);
                        writeln!(
                            writer.lock().unwrap(),
                            "{},{},{}",
                            qualified_function_name,
                            time.as_nanos(),
                            instrs_executed,
                        )?;
                    }
                }
                return Ok(());
            } else {
                writeln!(
                    std::io::stderr(),
                    "Unknown output format '{report_type}' provided. Defaulting to basic format."
                )?
            }
        }

        writeln!(writer.lock().unwrap(), "\nTest Statistics:\n")?;

        let mut max_function_name_size = 0;
        let mut stats = Vec::new();

        let mut passed_fns = BTreeSet::new();

        for (module_id, test_results) in self.final_statistics.passed.iter() {
            for (function_name, test_results) in test_results {
                let qualified_function_name = format!(
                    "{}::{}",
                    format_module_id(&self.test_plan.module_info, module_id),
                    function_name,
                );
                passed_fns.insert(qualified_function_name.clone());
                max_function_name_size =
                    std::cmp::max(max_function_name_size, qualified_function_name.len());
                let (time, instrs_executed) = calculate_run_statistics(test_results);
                stats.push((qualified_function_name, time.as_secs_f32(), instrs_executed))
            }
        }

        for (module_id, test_failures) in self.final_statistics.failed.iter() {
            for (function_name, test_failure) in test_failures {
                let qualified_function_name = format!(
                    "{}::{}",
                    format_module_id(&self.test_plan.module_info, module_id),
                    function_name
                );
                // If the test is a #[random_test] some of the tests may have passed, and others
                // failed. We want to mark the any results in the statistics where there is both
                // successful and failed runs as "failure run" to indicate that these stats are for
                // the case where the test failed.
                let also_passed_modifier = if passed_fns.contains(&qualified_function_name) {
                    " (failure)"
                } else {
                    ""
                };
                let qualified_function_name =
                    format!("{qualified_function_name}{also_passed_modifier}");
                max_function_name_size =
                    std::cmp::max(max_function_name_size, qualified_function_name.len());
                let (time, instrs_executed) =
                    calculate_run_statistics(test_failure.iter().map(|f| &f.test_run_info));
                stats.push((qualified_function_name, time.as_secs_f32(), instrs_executed));
            }
        }

        if !stats.is_empty() {
            writeln!(
                writer.lock().unwrap(),
                "┌─{:─^width$}─┬─{:─^10}─┬─{:─^25}─┐",
                "",
                "",
                "",
                width = max_function_name_size,
            )?;
            writeln!(
                writer.lock().unwrap(),
                "│ {name:^width$} │ {time:^10} │ {instructions:^25} │",
                width = max_function_name_size,
                name = "Test Name",
                time = "Time",
                instructions = "Gas Used"
            )?;

            for (qualified_function_name, time, instructions) in stats {
                writeln!(
                    writer.lock().unwrap(),
                    "├─{:─^width$}─┼─{:─^10}─┼─{:─^25}─┤",
                    "",
                    "",
                    "",
                    width = max_function_name_size,
                )?;
                writeln!(
                    writer.lock().unwrap(),
                    "│ {name:<width$} │ {time:^10.3} │ {instructions:^25} │",
                    name = qualified_function_name,
                    width = max_function_name_size,
                    time = time,
                    instructions = instructions,
                )?;
            }

            writeln!(
                writer.lock().unwrap(),
                "└─{:─^width$}─┴─{:─^10}─┴─{:─^25}─┘",
                "",
                "",
                "",
                width = max_function_name_size,
            )?;
        }

        writeln!(writer.lock().unwrap())
    }

    /// Returns `true` if all tests passed, `false` if there was a test failure/timeout
    pub fn summarize<W: Write>(self, writer: &Mutex<W>) -> Result<bool> {
        let num_failed_tests = self
            .final_statistics
            .failed
            .iter()
            .fold(0, |acc, (_, fns)| acc + fns.len()) as u64;
        let num_passed_tests = self
            .final_statistics
            .passed
            .iter()
            .fold(0, |acc, (_, fns)| acc + fns.len()) as u64;
        if !self.final_statistics.failed.is_empty() {
            writeln!(writer.lock().unwrap(), "\nTest failures:\n")?;
            for (module_id, test_failures) in &self.final_statistics.failed {
                writeln!(
                    writer.lock().unwrap(),
                    "Failures in {}:",
                    format_module_id(&self.test_plan.module_info, module_id)
                )?;
                for (test_name, test_failures) in test_failures {
                    for test_failure in test_failures {
                        writeln!(
                            writer.lock().unwrap(),
                            "\n┌── {} ──────{}",
                            test_name.bold(),
                            if let Some(seed) = test_failure.prng_seed {
                                format!(" (seed = {seed})").red().bold().to_string()
                            } else {
                                "".to_string()
                            }
                        )?;
                        writeln!(
                            writer.lock().unwrap(),
                            "│ {}",
                            test_failure
                                .render_error(&self.test_plan)
                                .replace('\n', "\n│ ")
                        )?;
                        if let Some(seed) = test_failure.prng_seed {
                            writeln!(writer.lock().unwrap(),
                            "│ {}",
                            format!(
                                "This test uses randomly generated inputs. Rerun with `{}` to recreate this test failure.\n",
                                format!("test {} --seed {}",
                                    test_name,
                                    seed
                                ).bright_red().bold()
                            ).replace('\n', "\n│ ")
                        )?;
                        }
                        writeln!(writer.lock().unwrap(), "└──────────────────\n")?;
                    }
                }
            }
        }

        writeln!(
            writer.lock().unwrap(),
            "Test result: {}. Total tests: {}; passed: {}; failed: {}",
            if num_failed_tests == 0 {
                "OK".bold().bright_green()
            } else {
                "FAILED".bold().bright_red()
            },
            num_passed_tests + num_failed_tests,
            num_passed_tests,
            num_failed_tests
        )?;
        Ok(num_failed_tests == 0)
    }
}
