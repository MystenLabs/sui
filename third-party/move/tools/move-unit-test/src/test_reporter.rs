// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::format_module_id;
use codespan_reporting::files::{Files, SimpleFiles};
use colored::{control, Colorize};
use move_binary_format::{
    access::ModuleAccess,
    errors::{ExecutionState, Location, VMError, VMResult},
};
use move_command_line_common::files::FileHash;
use move_compiler::{
    diagnostics::{self, Diagnostic, Diagnostics},
    unit_test::{ModuleTestPlan, TestName, TestPlan},
};
use move_core_types::{effects::ChangeSet, language_storage::ModuleId, vm_status::StatusType};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{Result, Write},
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
    WrongAbortDEPRECATED(String, u64, MoveError),
    // Error wasn't expected, but it did
    UnexpectedError(String, MoveError),
    // Test timed out
    Timeout(String),
    // The execution results of the Move VM and stackless VM does not match
    Mismatch {
        move_vm_return_values: Box<VMResult<Vec<Vec<u8>>>>,
        move_vm_change_set: Box<VMResult<ChangeSet>>,
        stackless_vm_return_values: Box<VMResult<Vec<Vec<u8>>>>,
        stackless_vm_change_set: Box<VMResult<ChangeSet>>,
    },
    // Property checking failed
    Property(String),

    // Failed to compile Move code into EVM bytecode.
    #[cfg(feature = "evm-backend")]
    MoveToEVMError(String),
}

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct TestFailure {
    pub test_run_info: TestRunInfo,
    pub vm_error: Option<VMError>,
    pub failure_reason: FailureReason,
    pub storage_state: Option<String>,
}

#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
pub struct TestRunInfo {
    pub function_ident: String,
    pub elapsed_time: Duration,
    pub instructions_executed: u64,
}

#[derive(Debug, Clone)]
pub struct TestStatistics {
    passed: BTreeMap<ModuleId, BTreeSet<TestRunInfo>>,
    failed: BTreeMap<ModuleId, BTreeSet<TestFailure>>,
    output: BTreeMap<ModuleId, BTreeMap<TestName, String>>,
}

#[derive(Debug, Clone)]
pub struct TestResults {
    final_statistics: TestStatistics,
    test_plan: TestPlan,
}

impl TestRunInfo {
    pub fn new(function_ident: String, elapsed_time: Duration, instructions_executed: u64) -> Self {
        Self {
            function_ident,
            elapsed_time,
            instructions_executed,
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

    pub fn wrong_abort_deprecated(expected: u64, actual: MoveError) -> Self {
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

    pub fn mismatch(
        move_vm_return_values: VMResult<Vec<Vec<u8>>>,
        move_vm_change_set: VMResult<ChangeSet>,
        stackless_vm_return_values: VMResult<Vec<Vec<u8>>>,
        stackless_vm_change_set: VMResult<ChangeSet>,
    ) -> Self {
        FailureReason::Mismatch {
            move_vm_return_values: Box::new(move_vm_return_values),
            move_vm_change_set: Box::new(move_vm_change_set),
            stackless_vm_return_values: Box::new(stackless_vm_return_values),
            stackless_vm_change_set: Box::new(stackless_vm_change_set),
        }
    }

    pub fn property(details: String) -> Self {
        FailureReason::Property(details)
    }

    #[cfg(feature = "evm-backend")]
    pub fn move_to_evm_error(diagnostics: String) -> Self {
        FailureReason::MoveToEVMError(diagnostics)
    }
}

impl TestFailure {
    pub fn new(
        failure_reason: FailureReason,
        test_run_info: TestRunInfo,
        vm_error: Option<VMError>,
        storage_state: Option<String>,
    ) -> Self {
        Self {
            test_run_info,
            vm_error,
            failure_reason,
            storage_state,
        }
    }

    pub fn render_error(&self, test_plan: &TestPlan) -> String {
        let error_string = match &self.failure_reason {
            FailureReason::NoError(message) => message.to_string(),
            FailureReason::Timeout(message) => message.to_string(),
            FailureReason::WrongError(message, expected, actual) => {
                let base_message = format!(
                    "{message}. Expected test {} but instead it {} rooted here",
                    expected.verbiage(/* is_past_tense */ false),
                    actual.verbiage(/* is_past_tense */ true),
                );
                Self::report_error_with_location(test_plan, base_message, &self.vm_error)
            }
            FailureReason::WrongAbortDEPRECATED(message, expected_code, actual) => {
                let base_message = format!(
                    "{}. \
                    Expected test to abort with code {}, but instead it {} rooted here",
                    message,
                    expected_code,
                    actual.verbiage(/* is_past_tense */ true),
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
                    error.verbiage(/* is_past_tense */ true)
                );
                Self::report_error_with_location(test_plan, base_message, &self.vm_error)
            }
            FailureReason::Mismatch {
                move_vm_return_values,
                move_vm_change_set,
                stackless_vm_return_values,
                stackless_vm_change_set,
            } => {
                format!(
                    "Executions via Move VM [M] and stackless VM [S] yield different results.\n\
                    [M] - return values: {:?}\n\
                    [S] - return values: {:?}\n\
                    [M] - change set: {:?}\n\
                    [S] - change set: {:?}\n\
                    ",
                    move_vm_return_values,
                    stackless_vm_return_values,
                    move_vm_change_set,
                    stackless_vm_change_set
                )
            }
            FailureReason::Property(message) => message.clone(),

            #[cfg(feature = "evm-backend")]
            FailureReason::MoveToEVMError(diagnostics) => {
                format!(
                    "Failed to compile Move code into EVM bytecode.\n\n{}",
                    diagnostics
                )
            }
        };

        match &self.storage_state {
            None => error_string,
            Some(storage_state) => {
                format!(
                    "{}\n────── Storage state at point of failure ──────\n{}",
                    error_string,
                    if storage_state.is_empty() {
                        "<empty>"
                    } else {
                        storage_state
                    }
                )
            }
        }
    }

    fn get_line_number(
        loc: &Loc,
        files: &SimpleFiles<Symbol, &str>,
        file_mapping: &HashMap<FileHash, usize>,
    ) -> String {
        Self::get_line_number_internal(loc, files, file_mapping)
            .unwrap_or_else(|_| "no_source_line".to_string())
    }

    fn get_line_number_internal(
        loc: &Loc,
        files: &SimpleFiles<Symbol, &str>,
        file_mapping: &HashMap<FileHash, usize>,
    ) -> std::result::Result<String, codespan_reporting::files::Error> {
        let id = file_mapping
            .get(&loc.file_hash())
            .ok_or(codespan_reporting::files::Error::FileMissing)?;
        let start_line_index = files.line_index(*id, loc.start() as usize)?;
        let start_line_number = files.line_number(*id, start_line_index)?;
        let end_line_index = files.line_index(*id, loc.end() as usize)?;
        let end_line_number = files.line_number(*id, end_line_index)?;
        if start_line_number == end_line_number {
            Ok(start_line_number.to_string())
        } else {
            Ok(format!("{}-{}", start_line_number, end_line_number))
        }
    }

    fn report_exec_state(test_plan: &TestPlan, exec_state: &ExecutionState) -> String {
        let stack_trace = exec_state.stack_trace();
        let mut buf = String::new();
        if !stack_trace.is_empty() {
            buf.push_str("stack trace\n");
            let mut files = SimpleFiles::new();
            let mut file_mapping = HashMap::new();
            for (fhash, (fname, source)) in &test_plan.files {
                let id = files.add(*fname, source.as_str());
                file_mapping.insert(*fhash, id);
            }

            for frame in stack_trace {
                let module_id = match &frame.0 {
                    Some(v) => v,
                    None => return "\tmalformed stack trace (no module ID)".to_string(),
                };
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
                let file_name = match test_plan.files.get(&loc.file_hash()) {
                    Some(v) => format!("{}", v.0),
                    None => "unknown_source".to_string(),
                };
                buf.push_str(
                    &format!(
                        "\t{}::{}({}:{})\n",
                        module_id.name(),
                        fn_name,
                        file_name,
                        Self::get_line_number(&loc, &files, &file_mapping)
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
        let report_diagnostics = if control::SHOULD_COLORIZE.should_colorize() {
            diagnostics::report_diagnostics_to_color_buffer
        } else {
            diagnostics::report_diagnostics_to_buffer
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
                    let msg = format!("In this function in {}", format_module_id(module_id));
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
                        &test_plan.files,
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

impl TestStatistics {
    pub fn new() -> Self {
        Self {
            passed: BTreeMap::new(),
            failed: BTreeMap::new(),
            output: BTreeMap::new(),
        }
    }

    pub fn test_failure(&mut self, test_failure: TestFailure, test_plan: &ModuleTestPlan) {
        self.failed
            .entry(test_plan.module_id.clone())
            .or_insert_with(BTreeSet::new)
            .insert(test_failure);
    }

    pub fn test_success(&mut self, test_info: TestRunInfo, test_plan: &ModuleTestPlan) {
        self.passed
            .entry(test_plan.module_id.clone())
            .or_insert_with(BTreeSet::new)
            .insert(test_info);
    }

    pub fn test_output(&mut self, test_name: TestName, test_plan: &ModuleTestPlan, output: String) {
        self.output
            .entry(test_plan.module_id.clone())
            .or_insert_with(BTreeMap::new)
            .insert(test_name, output);
    }

    pub fn combine(mut self, other: Self) -> Self {
        for (module_id, test_result) in other.passed {
            let entry = self.passed.entry(module_id).or_default();
            entry.extend(test_result.into_iter());
        }
        for (module_id, test_result) in other.failed {
            let entry = self.failed.entry(module_id).or_default();
            entry.extend(test_result.into_iter());
        }
        for (module_id, test_output) in other.output {
            let entry = self.output.entry(module_id).or_default();
            entry.extend(test_output.into_iter());
        }
        self
    }
}

impl TestResults {
    pub fn new(final_statistics: TestStatistics, test_plan: TestPlan) -> Self {
        Self {
            final_statistics,
            test_plan,
        }
    }

    pub fn report_goldens<W: Write>(&self, writer: &Mutex<W>) -> Result<()> {
        for (module_name, test_outputs) in self.final_statistics.output.iter() {
            for (test_name, write_set) in test_outputs.iter() {
                writeln!(
                    writer.lock().unwrap(),
                    "{}::{}",
                    format_module_id(module_name),
                    test_name
                )?;
                writeln!(writer.lock().unwrap(), "Output: {}", write_set)?;
            }
        }
        Ok(())
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
                    for test_result in test_results {
                        let qualified_function_name = format!(
                            "{}::{}",
                            format_module_id(module_id),
                            test_result.function_ident
                        );
                        writeln!(
                            writer.lock().unwrap(),
                            "{},{},{}",
                            qualified_function_name,
                            test_result.elapsed_time.as_nanos(),
                            test_result.instructions_executed
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

        for (module_id, test_results) in self.final_statistics.passed.iter() {
            for test_result in test_results {
                let qualified_function_name = format!(
                    "{}::{}",
                    format_module_id(module_id),
                    test_result.function_ident
                );
                max_function_name_size =
                    std::cmp::max(max_function_name_size, qualified_function_name.len());
                stats.push((
                    qualified_function_name,
                    test_result.elapsed_time.as_secs_f32(),
                    test_result.instructions_executed,
                ))
            }
        }

        for (module_id, test_failures) in self.final_statistics.failed.iter() {
            for test_failure in test_failures {
                let qualified_function_name = format!(
                    "{}::{}",
                    format_module_id(module_id),
                    test_failure.test_run_info.function_ident
                );
                max_function_name_size =
                    std::cmp::max(max_function_name_size, qualified_function_name.len());
                stats.push((
                    qualified_function_name,
                    test_failure.test_run_info.elapsed_time.as_secs_f32(),
                    test_failure.test_run_info.instructions_executed,
                ));
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
                    format_module_id(module_id)
                )?;
                for test_failure in test_failures {
                    writeln!(
                        writer.lock().unwrap(),
                        "\n┌── {} ──────",
                        test_failure.test_run_info.function_ident.bold()
                    )?;
                    writeln!(
                        writer.lock().unwrap(),
                        "│ {}",
                        test_failure
                            .render_error(&self.test_plan)
                            .replace('\n', "\n│ ")
                    )?;
                    writeln!(writer.lock().unwrap(), "└──────────────────\n")?;
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
