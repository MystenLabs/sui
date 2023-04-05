// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Wrapper around the boogie program. Allows to call boogie and analyze the output.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    num::ParseIntError,
    option::Option::None,
};

use anyhow::anyhow;
use codespan::{ByteIndex, ColumnIndex, LineIndex, Location, Span};
use codespan_reporting::diagnostic::{Diagnostic, Label};
use itertools::Itertools;
use log::{debug, info, warn};
use num::BigInt;
use once_cell::sync::Lazy;
use pretty::RcDoc;
use regex::Regex;

use move_binary_format::file_format::FunctionDefinitionIndex;
use move_model::{
    ast::TempIndex,
    code_writer::CodeWriter,
    model::{FunId, GlobalEnv, Loc, ModuleId, NodeId, QualifiedId, StructEnv},
    pragmas::INTRINSIC_TYPE_MAP,
    ty::{PrimitiveType, Type},
};
use move_stackless_bytecode::function_target_pipeline::{FunctionTargetsHolder, FunctionVariant};

// DEBUG
// use backtrace::Backtrace;
use crate::{
    boogie_helpers::{boogie_inst_suffix, boogie_struct_name},
    options::{BoogieOptions, VectorTheory},
    prover_task_runner::{ProverTaskRunner, RunBoogieWithSeeds},
};

/// A type alias for the way how we use crate `pretty`'s document type. `pretty` is a
/// Wadler-style pretty printer. Our simple usage doesn't require any lifetime management.
type PrettyDoc = RcDoc<'static, ()>;

// -----------------------------------------------
// # Boogie Wrapper

/// Represents the boogie wrapper.
pub struct BoogieWrapper<'env> {
    pub env: &'env GlobalEnv,
    pub targets: &'env FunctionTargetsHolder,
    pub writer: &'env CodeWriter,
    pub options: &'env BoogieOptions,
}

/// Output of a boogie run.
pub struct BoogieOutput {
    /// All errors which could be parsed from the output.
    pub errors: Vec<BoogieError>,

    /// Full output as a string.
    pub all_output: String,
}

/// Kind of boogie error.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BoogieErrorKind {
    Assertion,
    Inconclusive,
    Inconsistency,
    Internal,
}

impl BoogieErrorKind {
    fn is_from_verification(self) -> bool {
        use BoogieErrorKind::*;
        matches!(self, Assertion)
    }
}

/// A boogie error.
pub struct BoogieError {
    pub kind: BoogieErrorKind,
    pub loc: Loc,
    pub message: String,
    pub execution_trace: Vec<TraceEntry>,
    pub model: Option<Model>,
}

/// A trace entry.
pub enum TraceEntry {
    AtLocation(Loc),
    Temporary(QualifiedId<FunId>, TempIndex, ModelValue),
    Result(QualifiedId<FunId>, usize, ModelValue),
    Abort(QualifiedId<FunId>, ModelValue),
    Exp(NodeId, ModelValue),
    SubExp(NodeId, ModelValue),
    GlobalMem(NodeId, ModelValue),
    InfoLine(String),
}

// Error message matching
static VERIFICATION_DIAG_STARTS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^assert_failed\((?P<args>[^)]*)\): (?P<msg>.*)$").unwrap());

static INCONCLUSIVE_DIAG_STARTS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^.*\((?P<line>\d+),(?P<col>\d+)\).*Verification(?P<str>.*)(inconclusive|out of resource|timed out).*$")
        .unwrap()
});

static INCONSISTENCY_DIAG_STARTS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^inconsistency_detected\((?P<args>[^)]*)\)").unwrap());

impl<'env> BoogieWrapper<'env> {
    /// Calls boogie on the given file. On success, returns a struct representing the analyzed
    /// output of boogie.
    pub fn call_boogie(&self, boogie_file: &str) -> anyhow::Result<BoogieOutput> {
        let args = self.options.get_boogie_command(boogie_file)?;
        info!("running solver");
        debug!("command line: {}", args.iter().join(" "));
        let task = RunBoogieWithSeeds {
            options: self.options.clone(),
            boogie_file: boogie_file.to_string(),
        };
        // When running on complicated formulas(especially those with quantifiers), SMT solvers
        // can suffer from the so-called butterfly effect, where minor changes such as using
        // different random seeds cause significant instabilities in verification times.
        // Thus by running multiple instances of Boogie with different random seeds, we can
        // potentially alleviate the instability.
        let (seed, output_res) = ProverTaskRunner::run_tasks(
            task,
            self.options.num_instances,
            self.options.sequential_task,
            self.options.hard_timeout_secs,
        );
        let output = match output_res {
            Err(err) => {
                if err.kind() == std::io::ErrorKind::TimedOut {
                    let err = BoogieError {
                        kind: BoogieErrorKind::Internal,
                        loc: self.env.unknown_loc(),
                        message: format!(
                            "Boogie execution exceeded hard timeout of {}s",
                            self.options.hard_timeout_secs
                        ),
                        execution_trace: vec![],
                        model: None,
                    };
                    return Ok(BoogieOutput {
                        errors: vec![err],
                        all_output: "".to_string(),
                    });
                } else {
                    panic!("cannot execute boogie `{:?}`: {}", args, err)
                }
            }
            Ok(out) => out,
        };
        if self.options.num_instances > 1 {
            debug!("Boogie instance with seed {} finished first", seed);
        }

        debug!("analyzing boogie output");
        let out = String::from_utf8_lossy(&output.stdout).to_string();
        let err = String::from_utf8_lossy(&output.stderr).to_string();
        // Boogie prints a few ad-hoc error messages (with exit code 0!), so we have
        // no chance to catch an error until we recognize one of those patterns.
        if out
            .trim()
            .starts_with("Fatal Error: ProverException: Cannot find specified prover")
        {
            return Err(anyhow!(
                "The configured prover `{}` could not be found{}",
                if self.options.use_cvc5 {
                    &self.options.cvc5_exe
                } else {
                    &self.options.z3_exe
                },
                if self.options.use_cvc5 {
                    " (--use-cvc5 is set)"
                } else {
                    ""
                }
            ));
        }
        if !output.status.success() {
            // Exit here with raw output.
            return Err(anyhow!(
                "Boogie error ({}): {}\n\nstderr:\n{}",
                output.status,
                out,
                err
            ));
        }
        if out.trim().starts_with("Unable to monomorphize") {
            return Err(anyhow!("Boogie error: {}\n\nstderr:\n{}", out, err));
        }
        // Boogie output contains the string "errors detected in" whenever parsing,
        // resolution, or type checking errors are discovered.
        if out.contains("errors detected in") {
            return Err(anyhow!(
                "[internal] boogie exited with compilation errors:\n{}",
                out
            ));
        }
        if out.contains("Prover error:") {
            return Err(anyhow!(
                "[internal] boogie exited with prover errors:\n{}",
                out
            ));
        }
        let mut errors = self.extract_verification_errors(&out);
        errors.extend(self.extract_inconclusive_errors(&out));
        errors.extend(self.extract_inconsistency_errors(&out));
        Ok(BoogieOutput {
            errors,
            all_output: out,
        })
    }

    /// Calls boogie and analyzes output.
    pub fn call_boogie_and_verify_output(&self, boogie_file: &str) -> anyhow::Result<()> {
        let BoogieOutput { errors, all_output } = self.call_boogie(boogie_file)?;
        let boogie_log_file = self.options.get_boogie_log_file(boogie_file);
        let log_file_existed = std::path::Path::new(&boogie_log_file).exists();
        debug!("writing boogie log to {}", boogie_log_file);
        fs::write(&boogie_log_file, &all_output)?;

        for error in &errors {
            self.add_error(error);
        }

        if !log_file_existed && !self.options.keep_artifacts {
            std::fs::remove_file(boogie_log_file).unwrap_or_default();
        }

        Ok(())
    }

    /// Helper to add a boogie error as a codespan Diagnostic.
    fn add_error(&self, error: &BoogieError) {
        // Create the error
        let label = Label::primary(error.loc.file_id(), error.loc.span());
        let mut diag = Diagnostic::error()
            .with_message(error.message.clone())
            .with_labels(vec![label]);

        // Now add trace diagnostics.
        if error.kind.is_from_verification() && !error.execution_trace.is_empty() {
            let mut display = vec![];
            let mut last_loc = self.env.unknown_loc();
            let mut abort_in_progress = None;
            let print_loc = |loc: &Loc, last_loc: &mut Loc, display: &mut Vec<String>| {
                let info = if let Some(fun) = self.env.get_enclosing_function(loc) {
                    let spec_suffix = if let Some(spec_loc) = &fun.get_spec().loc {
                        if spec_loc.is_enclosing(loc) {
                            " (spec)"
                        } else {
                            ""
                        }
                    } else {
                        ""
                    };
                    format!(
                        ": {}{}",
                        fun.get_name().display(self.env.symbol_pool()),
                        spec_suffix
                    )
                } else {
                    "".to_string()
                };
                // Brute-force filter out "at" entries which look alike. This is cheaper than
                // avoiding producing them, because of the step of converting locations to line
                // numbers.
                let display_str = format!("    {}{}", loc.display_line_only(self.env), info);
                if display.is_empty() || display[display.len() - 1] != display_str {
                    display.push(display_str);
                }
                *last_loc = loc.clone();
            };

            let mut subexp_map = BTreeMap::new();
            let mut global_mem_map = BTreeMap::new();
            for entry in &error.execution_trace {
                use TraceEntry::*;
                if abort_in_progress.is_some() && !matches!(entry, Exp(..)) {
                    // Once abort happened, only allow expression traces.
                    continue;
                }
                match entry {
                    AtLocation(loc) => {
                        if loc != &last_loc {
                            print_loc(loc, &mut last_loc, &mut display);
                        }
                    }
                    Temporary(fun, idx, value) if error.model.is_some() => {
                        let fun_env = self.env.get_function(*fun);
                        let fun_target = self
                            .targets
                            .get_target(&fun_env, &FunctionVariant::Baseline);
                        if *idx < fun_target.get_local_count() {
                            let var_name = fun_target
                                .get_local_name(*idx)
                                .display(self.env.symbol_pool())
                                .to_string();
                            let var_name =
                                if self.options.stable_test_output && var_name.contains('$') {
                                    // Redact the name.
                                    // TODO(wrwg): There is some non-determinism in how the Move compiler
                                    //   generates temporaries resulting from if-expressions. They also
                                    //   happen with the assert-builtin, which the compilers maps to an if.
                                    //   We should fix those in the Move compiler instead of working around her.
                                    "<redacted>".to_string()
                                } else {
                                    var_name
                                };
                            let ty = fun_target.get_local_type(*idx);
                            let pretty =
                                value.pretty_or_raw(self, error.model.as_ref().unwrap(), ty);
                            display.extend(self.make_trace_entry(var_name, pretty));
                        }
                    }
                    Result(fun, idx, value) if error.model.is_some() => {
                        let fun_env = self.env.get_function(*fun);
                        let fun_target = self
                            .targets
                            .get_target(&fun_env, &FunctionVariant::Baseline);
                        let n = fun_target.get_return_count();
                        if *idx < n {
                            let var_name = if n > 1 {
                                format!("result_{}", idx.saturating_add(1))
                            } else {
                                "result".to_string()
                            };
                            let ty = fun_target.get_return_type(*idx);
                            let pretty =
                                value.pretty_or_raw(self, error.model.as_ref().unwrap(), ty);
                            display.extend(self.make_trace_entry(var_name, pretty));
                        }
                    }
                    Abort(_, value) => {
                        display.push("        ABORTED".to_string());
                        abort_in_progress = Some((last_loc.clone(), value));
                    }
                    Exp(node_id, value) => {
                        let loc = self.env.get_node_loc(*node_id);
                        if loc != last_loc {
                            print_loc(&loc, &mut last_loc, &mut display);
                        }
                        let ty = self.env.get_node_type(*node_id);
                        let value = value.pretty_or_raw(self, error.model.as_ref().unwrap(), &ty);
                        let exp_str = self.get_abbreviated_source(*node_id);
                        display.extend(self.make_trace_entry(exp_str, value));
                    }
                    SubExp(node_id, value) => {
                        let exp_loc = self.env.get_node_loc(*node_id);
                        if error.loc.is_enclosing(&exp_loc) {
                            // This sub-expression trace is related to the error, remember it.
                            // Notice that if a sub-expression with the same denotation appears
                            // twice we take the latest one. This treatment depends on that the
                            // sub-exp instrumentation in model/spec_translator does not decent
                            // in old expressions; rather it displays the old as an atomic value.
                            // Hence we can assume denotational and semantic equivalence.
                            let loc = self.env.get_node_loc(*node_id);
                            let denotation = self.env.get_source(&loc).unwrap_or("??");
                            subexp_map.insert(denotation.to_string(), (*node_id, value.clone()));
                        }
                    }
                    GlobalMem(node_id, ModelValue::List(elems)) => {
                        // The bytecode track_global_memory takes the form
                        // "($Memory_107864 |T@[Int]Bool!val!2| |T@[Int]$1_DiemTimestamp_CurrentTimeMicroseconds!val!0|)"
                        // so the extracted list has three values where elems[2] is the reference to the model
                        // of the corresponding global memory array while elems[1] is the reference to the array to denote
                        // whether the memory exists
                        if elems.len() == 3 {
                            if let ModelValue::Literal(s) = &elems[2] {
                                // Extract the struct name from elems[2]
                                // e.g., $1_DiemTimestamp_CurrentTimeMicroseconds
                                let struct_name_str =
                                    &s[s.find('_').unwrap() + 1..s.find('!').unwrap()];
                                global_mem_map.insert(
                                    struct_name_str.to_string(),
                                    (*node_id, elems[1].clone(), elems[2].clone()),
                                );
                            }
                        }
                    }
                    InfoLine(info_line) => {
                        // information that should be displayed to the user
                        display.push(format!("    {}", info_line));
                    }
                    _ => {}
                }
            }
            if let Some((abort_loc, value)) = abort_in_progress {
                let code = if let Some(c) = value.extract_i128() {
                    if c == -1 {
                        " with execution failure".to_string()
                    } else {
                        format!(" with code 0x{:X}", c)
                    }
                } else {
                    "".to_string()
                };
                diag = diag.with_labels(vec![Label::secondary(
                    abort_loc.file_id(),
                    abort_loc.span(),
                )
                .with_message(&format!("abort happened here{}", code))]);
            }

            // Inject information about sub-expressions of this failure
            if !subexp_map.is_empty() {
                let mut trace_display = std::mem::take(&mut display);
                display.push("Related Bindings: ".to_string());
                for (denotation, (id, value)) in subexp_map {
                    let ty = self.env.get_node_type(id);
                    let pretty = value.pretty_or_raw(self, error.model.as_ref().unwrap(), &ty);
                    display.extend(self.make_trace_entry(denotation, pretty));
                }
                display.push("Execution Trace:".to_string());
                display.append(&mut trace_display)
            }

            // Inject information about the global memory of the function where this failure happens
            if !global_mem_map.is_empty() {
                let mut trace_display = std::mem::take(&mut display);
                display.push("Related Global Memory: ".to_string());
                // Extract the domain information from the model
                // Each memory slot has a corresponding domain flag
                // Print the slot when the flag is set to true
                let domain_info = extract_domain(error.model.as_ref().unwrap());
                for (struct_name_str, (id, domain_value, mem_value)) in global_mem_map {
                    let ty = self.env.get_node_type(id);
                    let pretty = mem_value
                        .pretty_mem(
                            self,
                            &domain_value,
                            &domain_info,
                            error.model.as_ref().unwrap(),
                            &ty,
                        )
                        .unwrap();
                    display.extend(self.make_mem_entry(struct_name_str, pretty));
                }
                display.append(&mut trace_display)
            }

            diag = diag.with_notes(display);
        }
        self.env.add_diag(diag);
    }

    fn get_abbreviated_source(&self, node_id: NodeId) -> String {
        let loc = self.env.get_node_loc(node_id);
        let res = if let Ok(src) = self.env.get_source(&loc) {
            let src = src.lines().map(|s| s.trim()).join(" ");
            let l = src.len();
            if l > 70 {
                format!("{} ..", &src[0..67])
            } else {
                src
            }
        } else {
            loc.display(self.env).to_string()
        };
        format!("`{}`", res)
    }

    fn make_trace_entry(&self, var_name: String, value: PrettyDoc) -> Vec<String> {
        self.render(
            PrettyDoc::text(var_name)
                .append(PrettyDoc::space())
                .append(PrettyDoc::text("="))
                .append(PrettyDoc::line().append(value).nest(2).group()),
        )
        .lines()
        .map(|s| "        ".to_string() + s)
        .collect()
    }

    fn make_mem_entry(&self, var_name: String, value: PrettyDoc) -> Vec<String> {
        self.render(
            PrettyDoc::text("Resource name: ".to_string() + &var_name)
                .append(PrettyDoc::space())
                .append(PrettyDoc::line().append(PrettyDoc::text("Values: ")))
                .append(PrettyDoc::line().append(value).nest(2).group()),
        )
        .lines()
        .map(|s| "        ".to_string() + s)
        .collect()
    }

    /// Renders the doc.
    fn render(&self, doc: PrettyDoc) -> String {
        let mut lines = vec![];
        doc.render(70, &mut lines).unwrap();
        String::from_utf8_lossy(&lines).to_string()
    }

    /// Extracts verification errors from Boogie output.
    fn extract_verification_errors(&self, out: &str) -> Vec<BoogieError> {
        let mut errors = vec![];
        let mut at = 0;
        while let Some(cap) = VERIFICATION_DIAG_STARTS.captures(&out[at..]) {
            let inbetween = out[at..at + cap.get(0).unwrap().start()].trim();
            at = usize::saturating_add(at, cap.get(0).unwrap().end());
            let msg = cap.name("msg").unwrap().as_str();

            // Filter diags that we know and will be processed later (e.g., inconclusive).
            // Other unknown diags will be reported as unexpected boogie output.
            if !inbetween.is_empty()
                && !INCONCLUSIVE_DIAG_STARTS.is_match(inbetween)
                && !INCONSISTENCY_DIAG_STARTS.is_match(inbetween)
            {
                // This is unexpected text and we report it as an internal error
                errors.push(BoogieError {
                    kind: BoogieErrorKind::Internal,
                    loc: self.env.unknown_loc(),
                    message: format!(
                        "unexpected boogie output: `{} ..`",
                        &inbetween[0..inbetween.len().min(70)]
                    ),
                    execution_trace: vec![],
                    model: None,
                })
            }

            let args = cap.name("args").unwrap().as_str();
            let loc = self.report_error(self.extract_loc(args), self.env.unknown_loc());
            let plain_trace = self.extract_execution_trace(out, &mut at);
            let mut execution_trace = self.extract_augmented_trace(out, &mut at);
            let mut model = Model::new(self);
            if execution_trace.is_empty() {
                execution_trace.push(TraceEntry::InfoLine(format!(
                    "Boogie does not return any augmented executed trace. \
                    See the plain trace below:\n{}",
                    plain_trace.join("\n")
                )))
            } else {
                self.extract_model(&mut model, out, &mut at);
            }

            if msg != "expected to fail" {
                // Only add this if it is not a negative test. We still needed to parse it.
                errors.push(BoogieError {
                    kind: BoogieErrorKind::Assertion,
                    loc,
                    message: msg.to_string(),
                    execution_trace,
                    model: if model.is_empty() { None } else { Some(model) },
                });
            }
        }
        errors
    }

    fn report_error<A>(&self, res: Result<A, ModelParseError>, default: A) -> A {
        match res {
            Ok(a) => a,
            Err(ModelParseError(s)) => {
                warn!("[boogie model] {}", s);
                default
            }
        }
    }

    /// Extracts the model.
    fn extract_model(&self, model: &mut Model, out: &str, at: &mut usize) {
        static MODEL_REGION: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?m)^\*\*\* MODEL$(?P<mod>(?s:.)*?^\*\*\* END_MODEL$)").unwrap()
        });

        if let Some(cap) = MODEL_REGION.captures(&out[*at..]) {
            *at = usize::saturating_add(*at, cap.get(0).unwrap().end());

            // Cuts out the state info block which is not used currently.
            let re = Regex::new(r"(?m)\*\*\* STATE(?s:.)*?\*\*\* END_STATE\n").unwrap();
            let remnant = re.replace(cap.name("mod").unwrap().as_str(), "");

            match model.parse(self, remnant.as_ref()) {
                Ok(_) => {}
                Err(parse_error) => {
                    let context_module = self
                        .env
                        .symbol_pool()
                        .string(self.env.get_modules().last().unwrap().get_name().name());
                    warn!(
                        "[boogie model] failed to parse boogie model (module context `{}`): {}",
                        context_module, parse_error.0
                    );
                }
            }
        }
    }

    /// Extracts the plain execution trace.
    fn extract_execution_trace(&self, out: &str, at: &mut usize) -> Vec<String> {
        static TRACE_START: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(?m)^Execution trace:\s*$").unwrap());
        static TRACE_ENTRY: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s+(?P<name>[^(]+)\((?P<args>[^)]*)\): (?P<value>.*)\n").unwrap()
        });
        let mut result = vec![];
        if let Some(m) = TRACE_START.find(&out[*at..]) {
            *at = usize::saturating_add(*at, m.end());
            while let Some(cap) = TRACE_ENTRY.captures(&out[*at..]) {
                *at = usize::saturating_add(*at, cap.get(0).unwrap().end());
                let name = cap.name("name").unwrap().as_str();
                let args = cap.name("args").unwrap().as_str();
                let value = cap.name("value").unwrap().as_str();
                result.push(format!("{}({}): {}", name, args, value))
            }
        }
        result
    }

    /// Extracts augmented execution trace.
    fn extract_augmented_trace(&self, out: &str, at: &mut usize) -> Vec<TraceEntry> {
        static TRACE_START: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(?m)^Augmented execution trace:\s*$").unwrap());
        static TRACE_ENTRY: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*\$(?P<name>[a-zA-Z_]+)\((?P<args>[^)]*)\)(:(?P<value>.*))?\n").unwrap()
        });
        let mut result = vec![];
        if let Some(m) = TRACE_START.find(&out[*at..]) {
            *at = usize::saturating_add(*at, m.end());
            while let Some(cap) = TRACE_ENTRY.captures(&out[*at..]) {
                *at = usize::saturating_add(*at, cap.get(0).unwrap().end());
                let name = cap.name("name").unwrap().as_str();
                let args = cap.name("args").unwrap().as_str();
                let value = cap.name("value").map(|m| m.as_str());
                match self.extract_augmented_entry(name, args, value) {
                    Ok(entry) => {
                        result.push(entry);
                    }
                    Err(parse_error) => {
                        let context_module = self
                            .env
                            .symbol_pool()
                            .string(self.env.get_modules().last().unwrap().get_name().name());
                        warn!(
                            "[boogie model] failed to parse augmented execution trace (module context `{}`): {}",
                            context_module, parse_error.0
                        );
                    }
                }
            }
        }
        result
    }

    fn extract_augmented_entry(
        &self,
        name: &str,
        args: &str,
        value: Option<&str>,
    ) -> Result<TraceEntry, ModelParseError> {
        match name {
            "at" => Ok(TraceEntry::AtLocation(self.extract_loc(args)?)),
            "track_local" => {
                let (fun, idx) = self.extract_fun_and_index(args)?;
                let value = self.extract_value(value)?;
                Ok(TraceEntry::Temporary(fun, idx, value))
            }
            "track_return" => {
                let (fun, idx) = self.extract_fun_and_index(args)?;
                let value = self.extract_value(value)?;
                Ok(TraceEntry::Result(fun, idx, value))
            }
            "track_abort" => {
                let fun = self.extract_fun(args)?;
                let value = self.extract_value(value)?;
                Ok(TraceEntry::Abort(fun, value))
            }
            "track_exp" => {
                let node_id = self.extract_node_id(args)?;
                let value = self.extract_value(value)?;
                Ok(TraceEntry::Exp(node_id, value))
            }
            "track_exp_sub" => {
                let node_id = self.extract_node_id(args)?;
                let value = self.extract_value(value)?;
                Ok(TraceEntry::SubExp(node_id, value))
            }
            "track_global_mem" => {
                let node_id = self.extract_node_id(args)?;
                let value = self.extract_value(value)?;
                Ok(TraceEntry::GlobalMem(node_id, value))
            }
            "info" => match value {
                Some(info_line) => Ok(TraceEntry::InfoLine(info_line.trim().to_string())),
                None => Ok(TraceEntry::InfoLine("".to_string())),
            },
            _ => Err(ModelParseError::new(&format!(
                "unrecognized augmented trace entry `{}`",
                name
            ))),
        }
    }

    fn extract_node_id(&self, args: &str) -> Result<NodeId, ModelParseError> {
        let index = args.parse::<usize>()?;
        self.env
            .index_to_node_id(index)
            .ok_or_else(|| ModelParseError::new("undefined node id"))
    }

    fn extract_loc(&self, args: &str) -> Result<Loc, ModelParseError> {
        let elems = args.split(',').collect_vec();
        if elems.len() == 3 {
            let file_idx = elems[0].parse::<u16>()?;
            let start = elems[1].parse::<u32>()?;
            let end = elems[2].parse::<u32>()?;
            let file_id = self.env.file_idx_to_id(file_idx);
            let loc = Loc::new(file_id, Span::new(start, end));
            Ok(loc)
        } else {
            Err(ModelParseError("invalid location triple".to_string()))
        }
    }

    fn extract_fun(&self, args: &str) -> Result<QualifiedId<FunId>, ModelParseError> {
        let elems = args.split(',').collect_vec();
        if elems.len() == 2 {
            let module_idx = elems[0].parse::<usize>()?;
            index_range_check(self.env.get_module_count())(module_idx)?;
            let module_env = self.env.get_module(ModuleId::new(module_idx));
            let fun_idx = elems[1].parse::<u16>()?;
            if let Some(fun_id) =
                module_env.try_get_function_id(FunctionDefinitionIndex::new(fun_idx))
            {
                return Ok(module_env.get_id().qualified(fun_id));
            }
        }
        Err(ModelParseError("invalid function id".to_string()))
    }

    fn extract_fun_and_index(
        &self,
        args: &str,
    ) -> Result<(QualifiedId<FunId>, usize), ModelParseError> {
        let elems = args.split(',').collect_vec();
        if elems.len() == 3 {
            let fun = self.extract_fun(&elems[0..2].join(","))?;
            let idx = elems[2].parse::<usize>()?;
            return Ok((fun, idx));
        }
        Err(ModelParseError("invalid function id and index".to_string()))
    }

    fn extract_value(&self, value: Option<&str>) -> Result<ModelValue, ModelParseError> {
        let input = value.unwrap_or("");
        if !input.is_empty() {
            let mut parser = ModelParser { input, at: 0 };
            parser.parse_value()
        } else {
            Ok(ModelValue::error())
        }
    }

    /// Extracts inconclusive (timeout) errors.
    fn extract_inconclusive_errors(&self, out: &str) -> Vec<BoogieError> {
        INCONCLUSIVE_DIAG_STARTS
            .captures_iter(out)
            .filter_map(|cap| {
                let str = cap.name("str").unwrap().as_str();
                if str.contains("$verify_inconsistency") {
                    // Masks the timeout from the negative test for the inconsistency checking, and
                    // skips the error report of this instance.
                    None
                } else {
                    let line = cap.name("line").unwrap().as_str();
                    let col = cap.name("col").unwrap().as_str();
                    let msg = cap.get(0).unwrap().as_str();
                    let loc = self
                        .get_loc_from_pos(make_position(line, col))
                        .unwrap_or_else(|| self.env.unknown_loc());
                    Some(BoogieError {
                        kind: BoogieErrorKind::Inconclusive,
                        loc,
                        message: if msg.contains("out of resource") || msg.contains("timed out") {
                            let timeout = self.options.adjust_timeout(self.options.vc_timeout);
                            format!(
                                "verification out of resources/timeout (global timeout set to {}s)",
                                timeout
                            )
                        } else {
                            "verification inconclusive".to_string()
                        },
                        execution_trace: vec![],
                        model: None,
                    })
                }
            })
            .collect_vec()
    }

    /// Extracts inconsistency errors.
    fn extract_inconsistency_errors(&self, out: &str) -> Vec<BoogieError> {
        INCONSISTENCY_DIAG_STARTS
            .captures_iter(out)
            .map(|cap| {
                let args = cap.name("args").unwrap().as_str();
                let loc = self.report_error(self.extract_loc(args), self.env.unknown_loc());
                BoogieError {
                    kind: BoogieErrorKind::Inconsistency,
                    loc,
                    message: "there is an inconsistent assumption in the function, which may allow any post-condition (including false) to be proven".to_string(),
                    execution_trace: vec![],
                    model: None,
                }
            })
            .collect_vec()
    }

    /// Gets the code byte index and source location (if available) from a target line/column
    /// position.
    fn get_loc_from_pos(&self, pos: Location) -> Option<Loc> {
        let index = self
            .writer
            .get_output_byte_index(pos.line, pos.column)
            .unwrap_or(ByteIndex(0));
        self.writer.get_source_location(index)
    }
}

/// Creates a position (line/column pair) from strings which are known to consist only of digits.
fn make_position(line_str: &str, col_str: &str) -> Location {
    // This will crash on overflow.
    let mut line = line_str.parse::<u32>().unwrap();
    let col = col_str.parse::<u32>().unwrap();
    if line > 0 {
        line = u32::saturating_sub(line, 1);
    }
    Location::new(LineIndex(line), ColumnIndex(col))
}

fn deduct_table_name(map_key: &str) -> Option<String> {
    // The generic representation of map keys is `|T@[Int]<X>!val!0` where `<X>` is the
    // vector element type.
    let i = map_key.find(']')?;
    let j = map_key.find('!')?;
    let suffix = &map_key[i + 1..j];
    Some(format!("Select__T@[Int]{}_", suffix))
}

fn deduct_update_table_name(map_key: &str) -> Option<String> {
    let i = map_key.find(']')?;
    let j = map_key.find('!')?;
    let suffix = &map_key[i + 1..j];
    Some(format!("Store__T@[Int]{}_", suffix))
}

/// Transpose the domain map in the model by aggregating values in it by the key into a standalone map
fn create_domain_map(
    model_map: &BTreeMap<ModelValue, ModelValue>,
    update_map_opt: Option<&BTreeMap<ModelValue, ModelValue>>,
) -> Option<(
    BTreeMap<ModelValue, BTreeMap<usize, bool>>,
    Option<ModelValue>,
    bool,
)> {
    let mut map: BTreeMap<ModelValue, BTreeMap<usize, bool>> = BTreeMap::new();

    let mut default_domain = None;

    let mut insert_map = |elems: &Vec<ModelValue>, val: &ModelValue| -> Option<()> {
        map.entry(elems[0].clone()).or_insert_with(BTreeMap::new);
        map.get_mut(&elems[0])
            .unwrap()
            .insert(elems[1].extract_number()?, extract_bool(val)?);
        Some(())
    };

    // Each entry in model_map takes the form:
    // key: |T@[Int]Bool!val!3| 153
    // value: true
    // elems[1] is the index of the array
    // value represents whether the memory indexed by elems[1] exists
    let mut default = false;
    for (key, value) in model_map {
        if let ModelValue::List(elems) = key {
            if elems.len() == 2 {
                insert_map(elems, value);
            }
        } else if let ModelValue::Literal(_) = key {
            default = extract_bool(value)?;
        }
    }

    // Each entry in update_map takes the form:
    // |T@[Int]Bool!val!2| 0 true -> |T@[Int]Bool!val!3|
    // else -> |T@[Int]Bool!val!3|
    if let Some(update_map) = update_map_opt {
        for (key, value) in update_map {
            if let ModelValue::List(elems) = key {
                if elems.len() == 3 {
                    insert_map(elems, &elems[2]);
                }
            } else {
                default_domain = Some(value.clone());
            }
        }
    }

    // map: mapping from memory slot to the domain flag
    // default_domain: used when the slot is not in the domain map, extracted from the update_map
    // default: default value in the domain map
    Some((map, default_domain, default))
}

/// Extract domain from the model
fn extract_domain(
    model: &Model,
) -> Option<(
    BTreeMap<ModelValue, BTreeMap<usize, bool>>,
    Option<ModelValue>,
    bool,
)> {
    let domain_table_name = "]Bool!".to_string();

    // Retrieve the domain map in the model
    let domain_map = model
        .vars
        .get(&ModelValue::literal(&deduct_table_name(
            &domain_table_name,
        )?))?
        .extract_map()?;

    // Retrieve update of the domain map in the model
    let domain_update_map_opt = model
        .vars
        .get(&ModelValue::literal(&deduct_update_table_name(
            &domain_table_name,
        )?))
        .and_then(|update_map| update_map.extract_map());

    create_domain_map(domain_map, domain_update_map_opt)
}

/// extract boolean value from ModelValue
fn extract_bool(value: &ModelValue) -> Option<bool> {
    let bool_value = value.extract_literal()?;
    if bool_value == "true" {
        Some(true)
    } else if bool_value == "false" {
        Some(false)
    } else {
        None
    }
}

// -----------------------------------------------
// # Boogie Model Analysis

/// Represents a boogie model.
#[derive(Debug)]
pub struct Model {
    vars: BTreeMap<ModelValue, ModelValue>,
    vector_theory: VectorTheory,
}

impl Model {
    /// Create a new model.
    fn new(wrapper: &BoogieWrapper<'_>) -> Self {
        Model {
            vars: Default::default(),
            vector_theory: wrapper.options.vector_theory,
        }
    }

    fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// Parses the given string into a model. The string is expected to end with MODULE_END_MARKER.
    fn parse(&mut self, _wrapper: &BoogieWrapper<'_>, input: &str) -> Result<(), ModelParseError> {
        let mut model_parser = ModelParser { input, at: 0 };
        model_parser
            .parse_map()
            .and_then(|m| {
                model_parser.expect(MODEL_END_MARKER)?;
                Ok(m)
            })
            .and_then(|m| match m {
                ModelValue::Map(vars) => {
                    self.vars.extend(vars);
                    Ok(())
                }
                _ => Err(ModelParseError("expected ModelValue::Map".to_string())),
            })
    }
}

/// Represents a model value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelValue {
    Literal(String),
    List(Vec<ModelValue>),
    Map(BTreeMap<ModelValue, ModelValue>),
}

/// Represents a spare representation of a model value vector.
#[derive(Debug)]
pub struct ModelValueVector {
    /// The size of the vector.
    size: usize,
    /// Those indices 0..size which have a value appear here.
    values: BTreeMap<usize, ModelValue>,
    /// Others in this range have assigned this value.
    default: ModelValue,
}

#[derive(Debug)]
pub struct ModelValueTable {
    /// The known keys. This might be more than we know a value for.
    keys: BTreeSet<usize>,
    /// The known value assignments
    values: BTreeMap<usize, ModelValue>,
    /// Whether the table is open, i.e. has more entries than the model determined.
    open: bool,
}

impl ModelValue {
    /// Makes a literal from a str.
    fn literal(s: &str) -> ModelValue {
        ModelValue::Literal(s.to_string())
    }

    // Makes an error value.
    fn error() -> ModelValue {
        ModelValue::List(vec![ModelValue::literal("Error")])
    }

    /// Extracts a vector from its representation.
    ///
    /// If the representation uses arrays it is defined by `(Vec* map_key size)`. The function
    /// follows indirections in the model. We find the array map at `Select_[$int]$Value`.
    /// This has e.g. the form
    /// ```model
    ///   Select_[$int]$Value -> {
    ///      |T@[Int]Value!val!1| 0 -> (Integer 2)
    ///      |T@[Int]Value!val!1| 22 -> (Integer 2)
    ///      else -> (Integer 0)
    ///    }
    /// ```
    /// If the value array is represented by a sequence instead, there are no indirections.
    /// It has the form
    /// ```(seq.++ (seq.unit (Integer 0)) (seq.unit (Integer 1)))```
    /// or
    /// ```(as seq.empty (Seq T@$Value))```
    /// depending on whether it is an empty or nonempty sequence, respectively.
    fn extract_vector(&self, model: &Model, _elem_ty: &Type) -> Option<ModelValueVector> {
        if matches!(model.vector_theory, VectorTheory::SmtSeq) {
            // Implementation of vectors using sequences
            let mut values = BTreeMap::new();
            if let Some(elems) = self.extract_list("as") {
                if elems.is_empty() {
                    return None;
                }
            } else if let Some(elem) = self.extract_seq_unit() {
                values.insert(0, elem);
            } else if let Some(elems) = self.extract_list("seq.++") {
                for (i, e) in elems.iter().enumerate() {
                    values.insert(i, e.extract_seq_unit()?);
                }
            } else {
                return None;
            };
            Some(ModelValueVector {
                size: values.len(),
                values,
                default: ModelValue::error(),
            })
        } else {
            // Implementation of vectors using arrays
            let args = self.extract_list_ctor_prefix("Vec_")?;
            if args.len() != 2 {
                return None;
            }
            let size = args[1].extract_number()?;
            let map_key = &args[0];
            let value_array_map = model
                .vars
                .get(&ModelValue::literal(&deduct_table_name(
                    map_key.extract_literal()?,
                )?))?
                .extract_map()?;
            let mut values = BTreeMap::new();
            let mut default = ModelValue::error();
            for (key, value) in value_array_map {
                if let ModelValue::List(elems) = key {
                    if elems.len() == 2 && &elems[0] == map_key {
                        if let Some(idx) = elems[1].extract_number() {
                            values.insert(idx, value.clone());
                        }
                    }
                } else if key == &ModelValue::literal("else") {
                    default = value.clone();
                }
            }
            Some(ModelValueVector {
                size,
                values,
                default,
            })
        }
    }

    /// Extracts a table from a model value.
    fn extract_table(
        &self,
        model: &Model,
        _key_ty: &Type,
        _val_ty: &Type,
    ) -> Option<ModelValueTable> {
        let args = self.extract_list_ctor_prefix("Table_")?;
        if args.len() != 3 {
            return None;
        }
        let (val_map, _val_default) = args[0].extract_select_int_map(model)?;
        let (dom_map, dom_default) = args[1].extract_select_int_map(model)?;
        let size = args[2].extract_number()?;
        let key_exists_by_default = extract_bool(&dom_default).unwrap_or(false);
        let mut keys = BTreeSet::new();
        let mut values = BTreeMap::new();
        for (key, exists) in dom_map {
            if extract_bool(&exists).unwrap_or(false) {
                keys.insert(key);
            }
        }
        for (key, value) in val_map {
            if key_exists_by_default {
                keys.insert(key);
            }
            if keys.contains(&key) {
                values.insert(key, value);
            }
        }
        let open = values.len() < size;
        Some(ModelValueTable { keys, values, open })
    }

    /// Extract memory content from the model
    fn extract_mem_vector(
        &self,
        model: &Model,
        domain: &ModelValue,
        domain_info_opt: &Option<(
            BTreeMap<ModelValue, BTreeMap<usize, bool>>,
            Option<ModelValue>,
            bool,
        )>,
    ) -> Option<ModelValueVector> {
        let mut size = 0;
        let domain_info = (*domain_info_opt).as_ref()?;
        let domain_idx_map = &domain_info.0;
        let default_domain_model_opt = &domain_info.1;
        let default_domain_flag = domain_info.2;

        let value_array_map = model
            .vars
            .get(&ModelValue::literal(&deduct_table_name(
                self.extract_literal()?,
            )?))?
            .extract_map()?;

        // In the model, the Store_ represents the update of that map
        let value_update_map_opt = model
            .vars
            .get(&ModelValue::literal(&deduct_update_table_name(
                self.extract_literal()?,
            )?))
            .and_then(|update_map| update_map.extract_map());

        let mut domain_exists_map_opt = domain_idx_map.get(domain);
        if domain_exists_map_opt.is_none() {
            if let Some(default_domain_model) = default_domain_model_opt {
                domain_exists_map_opt = domain_idx_map.get(default_domain_model);
            }
        }

        let mut values = BTreeMap::new();
        let mut default = ModelValue::error();

        let mut insert_values = |elem_opt: &ModelValue, new_value: &ModelValue| {
            if let Some(idx) = elem_opt.extract_number() {
                if let Some(domain_exists_map) = domain_exists_map_opt {
                    let mut flag = default_domain_flag;
                    if let Some(domain_exists_value) = domain_exists_map.get(&idx) {
                        flag = *domain_exists_value;
                    }
                    if flag {
                        // Only print the memory locations in the domain
                        values.insert(idx, new_value.clone());
                        size += 1;
                    }
                }
            }
        };

        // Each entry in value_array_map takes the form:
        // key: |T@[Int]$1_Bug7_BallotCounter!val!0| 0
        // value: ($1_Bug7_BallotCounter 18446744073709522257)
        // elems[1] is index to represent the instance of the memory
        for (key, value) in value_array_map {
            if let ModelValue::List(elems) = key {
                if elems.len() == 2 && &elems[0] == self {
                    insert_values(&elems[1], value);
                }
            } else if key == &ModelValue::literal("else") {
                // Currently, we assume existence of default memory locations print out the value.
                if default_domain_flag {
                    default = value.clone();
                }
            }
        }

        // Traverse the update map to obtain the updated value
        if let Some(value_update_map) = value_update_map_opt {
            for up_k in value_update_map.keys() {
                if let ModelValue::List(elems) = up_k {
                    // We only consider the case
                    // Where elems has three elements: Array symbol, position, updated value
                    if elems.len() == 3 && &elems[0] == self {
                        insert_values(&elems[1], &elems[2]);
                    }
                }
            }
        }

        Some(ModelValueVector {
            size,
            values,
            default,
        })
    }

    fn extract_seq_unit(&self) -> Option<ModelValue> {
        self.extract_list("seq.unit").and_then(|elems| {
            if elems.is_empty() {
                None
            } else {
                Some(elems[0].clone())
            }
        })
    }

    /// Extracts a $Select map with self the variable in the model.
    fn extract_select_int_map(
        &self,
        model: &Model,
    ) -> Option<(BTreeMap<usize, ModelValue>, ModelValue)> {
        let map = model
            .vars
            .get(&ModelValue::literal(&deduct_table_name(
                self.extract_literal()?,
            )?))?
            .extract_map()?;
        let mut values = BTreeMap::new();
        let mut default = Self::error();
        for (key, value) in map {
            if let ModelValue::List(elems) = key {
                if elems.len() == 2 && &elems[0] == self {
                    if let Some(idx) = elems[1].extract_number() {
                        values.insert(idx, value.clone());
                    }
                }
            } else if key == &ModelValue::literal("else") {
                default = value.clone();
            }
        }
        Some((values, default))
    }

    /// Extract a reverse mapping of values encoded by integers.
    fn extract_encoding_map(&self) -> Option<BTreeMap<usize, ModelValue>> {
        let mut res = BTreeMap::new();
        for (key, val) in self.extract_map()? {
            if key != &ModelValue::literal("else") {
                res.insert(val.extract_number()?, key.clone());
            }
        }
        Some(res)
    }

    fn extract_map(&self) -> Option<&BTreeMap<ModelValue, ModelValue>> {
        if let ModelValue::Map(map) = self {
            Some(map)
        } else {
            None
        }
    }

    /// Extract the arguments of a list of the form `(<ctor> element...)`.
    fn extract_list(&self, ctor: &str) -> Option<&[ModelValue]> {
        if let ModelValue::List(elems) = self {
            if !elems.is_empty() && elems[0] == ModelValue::literal(ctor) {
                return Some(&elems[1..]);
            }
        }
        None
    }

    /// Extract the arguments of a list of the form `(<ctor> element...)`.
    fn extract_list_ctor_prefix(&self, ctor_prefix: &str) -> Option<&[ModelValue]> {
        if let ModelValue::List(elems) = self {
            if !elems.is_empty() && elems[0].extract_literal()?.starts_with(ctor_prefix) {
                return Some(&elems[1..]);
            }
        }
        None
    }

    /// Extract a $Value box value.
    fn extract_box(&self) -> &ModelValue {
        if let ModelValue::List(elems) = self {
            if elems.len() == 2 {
                return &elems[1];
            }
        }
        self
    }

    /// Extract a number from a literal.
    fn extract_number(&self) -> Option<usize> {
        if let Ok(n) = self.extract_literal()?.parse::<usize>() {
            Some(n)
        } else {
            None
        }
    }

    /// Extract a i128 from a literal.
    fn extract_i128(&self) -> Option<i128> {
        if let Some(value) = self.extract_list("-").and_then(|values| {
            if values.len() == 1 {
                values[0].extract_i128().map(i128::saturating_neg)
            } else {
                None
            }
        }) {
            Some(value)
        } else if let Ok(n) = self.extract_literal()?.parse::<i128>() {
            Some(n)
        } else {
            None
        }
    }

    /// Extract a literal.
    fn extract_literal(&self) -> Option<&String> {
        if let ModelValue::Literal(s) = self {
            Some(s)
        } else {
            None
        }
    }

    /// Pretty prints the given model value which has given type. If printing fails, falls
    /// back to print the debug value.
    pub fn pretty_or_raw(&self, wrapper: &BoogieWrapper, model: &Model, ty: &Type) -> PrettyDoc {
        if wrapper.options.stable_test_output {
            return PrettyDoc::text("<redacted>");
        }
        self.pretty(wrapper, model, ty).unwrap_or_else(|| {
            // Print the raw debug value.
            PrettyDoc::text(format!("<? {:?}>", self))
        })
    }

    /// Pretty prints the given model value which has given type.
    pub fn pretty(&self, wrapper: &BoogieWrapper, model: &Model, ty: &Type) -> Option<PrettyDoc> {
        if self.extract_list("Error").is_some() {
            // This is an undefined value
            return Some(PrettyDoc::text("<undef>"));
        }
        match ty {
            Type::Primitive(PrimitiveType::U8) => Some(PrettyDoc::text(format!(
                "{}u8",
                self.extract_literal().and_then(|s| s.parse::<u8>().ok())?
            ))),
            Type::Primitive(PrimitiveType::U64) => Some(PrettyDoc::text(
                self.extract_literal()
                    .and_then(|s| s.parse::<u64>().ok())?
                    .to_string(),
            )),
            Type::Primitive(PrimitiveType::U128) => Some(PrettyDoc::text(format!(
                "{}u128",
                self.extract_literal()
                    .and_then(|s| s.parse::<u128>().ok())?
            ))),
            Type::Primitive(PrimitiveType::Num) => Some(PrettyDoc::text(format!(
                "{}num",
                self.extract_literal()
                    .and_then(|s| s.parse::<i128>().ok())?
            ))),
            Type::Primitive(PrimitiveType::Bool) => Some(PrettyDoc::text(
                self.extract_literal()
                    .and_then(|s| s.parse::<bool>().ok())?
                    .to_string(),
            )),
            Type::Primitive(PrimitiveType::Address) => {
                let addr = BigInt::parse_bytes(&self.extract_literal()?.clone().into_bytes(), 10)?;
                Some(PrettyDoc::text(format!("0x{}", &addr.to_str_radix(16))))
            }
            Type::Primitive(PrimitiveType::Signer) => {
                let l = self.extract_list("$signer")?;
                let addr = BigInt::parse_bytes(&l[0].extract_literal()?.clone().into_bytes(), 10)?;
                Some(PrettyDoc::text(format!(
                    "signer{{0x{}}}",
                    &addr.to_str_radix(16)
                )))
            }
            Type::Vector(param) => self.pretty_vector(wrapper, model, param),
            Type::Struct(module_id, struct_id, params) => {
                let struct_env = wrapper.env.get_struct_qid(module_id.qualified(*struct_id));
                if struct_env.is_intrinsic_of(INTRINSIC_TYPE_MAP) {
                    self.pretty_table(wrapper, model, &params[0], &params[1])
                } else {
                    self.pretty_struct(wrapper, model, &struct_env, params)
                }
            }
            Type::Reference(_, bt) => {
                Some(PrettyDoc::text("&").append(self.pretty(wrapper, model, bt)?))
            }
            Type::TypeParameter(_) => {
                // The value of a generic cannot be easily displayed because we do not know the
                // actual type unless we parse it out from the model (via the type value parameter)
                // and convert into a Type. However, since the value is parametric and cannot
                // effect the verification outcome, we may not have much need for seeing it.
                Some(PrettyDoc::text("<generic>"))
            }
            Type::Tuple(_)
            | Type::Primitive(_)
            | Type::Fun(_, _)
            | Type::TypeDomain(_)
            | Type::ResourceDomain(_, _, _)
            | Type::Error
            | Type::Var(_) => None,
        }
    }

    /// Pretty prints the body of a struct or vector, enclosed in braces.
    pub fn pretty_vec_or_struct_body(entries: Vec<PrettyDoc>) -> PrettyDoc {
        PrettyDoc::text("{")
            .append(
                PrettyDoc::line_()
                    .append(PrettyDoc::intersperse(
                        entries,
                        PrettyDoc::text(",").append(PrettyDoc::line()),
                    ))
                    .nest(2)
                    .group(),
            )
            .append(PrettyDoc::text("}"))
    }

    /// Pretty prints a vector.
    pub fn pretty_vector(
        &self,
        wrapper: &BoogieWrapper,
        model: &Model,
        param: &Type,
    ) -> Option<PrettyDoc> {
        let values = self.extract_vector(model, param)?;
        let mut entries = vec![];
        let mut next = 0;
        let mut sparse = false;
        for idx in values.values.keys().sorted() {
            if *idx >= values.size {
                // outside of domain, ignore.
                continue;
            }
            let mut p = values
                .values
                .get(idx)?
                .extract_box()
                .pretty_or_raw(wrapper, model, param);
            if *idx > next {
                p = PrettyDoc::text(format!("{}: ", idx)).append(p);
                sparse = true;
            }
            next = usize::saturating_add(*idx, 1);
            entries.push(p);
        }
        if next < values.size || sparse {
            let default = values
                .default
                .extract_box()
                .pretty(wrapper, model, param)
                .unwrap_or_else(|| PrettyDoc::text("undef"));
            entries.insert(0, PrettyDoc::text(format!("(size): {}", values.size)));
            entries.push(PrettyDoc::text("default: ").append(default));
        }
        Some(PrettyDoc::text("vector").append(Self::pretty_vec_or_struct_body(entries)))
    }

    /// Pretty prints a struct.
    pub fn pretty_struct(
        &self,
        wrapper: &BoogieWrapper,
        model: &Model,
        struct_env: &StructEnv,
        inst: &[Type],
    ) -> Option<PrettyDoc> {
        let entries = if struct_env.is_native_or_intrinsic() {
            let mut rep = self.extract_literal()?.to_string();
            if rep.starts_with("T@") {
                if let Some(i) = rep.rfind('!') {
                    rep = format!("#{}", &rep[i + 1..])
                }
            }
            vec![PrettyDoc::text(rep)]
        } else {
            let struct_name = &boogie_struct_name(struct_env, inst);
            let values = self
                .extract_list(struct_name)
                // It appears sometimes keys are represented witout, sometimes with enclosing
                // bars?
                .or_else(|| self.extract_list(&format!("|{}|", struct_name)))?;
            struct_env
                .get_fields()
                .enumerate()
                .map(|(i, f)| {
                    let ty = f.get_type().instantiate(inst);
                    let default = ModelValue::error();
                    let v = values.get(i).unwrap_or(&default);
                    let vp = v
                        .pretty(wrapper, model, &ty)
                        .unwrap_or_else(|| default.pretty_or_raw(wrapper, model, &ty));
                    PrettyDoc::text(format!(
                        "{}",
                        f.get_name().display(struct_env.symbol_pool())
                    ))
                    .append(PrettyDoc::text(" ="))
                    .append(PrettyDoc::line().append(vp).nest(2).group())
                })
                .collect_vec()
        };
        Some(
            PrettyDoc::text(format!(
                "{}.{}",
                struct_env
                    .module_env
                    .get_name()
                    .name()
                    .display(struct_env.symbol_pool()),
                struct_env.get_name().display(struct_env.symbol_pool())
            ))
            .append(Self::pretty_vec_or_struct_body(entries)),
        )
    }

    /// Pretty prints a table.
    pub fn pretty_table(
        &self,
        wrapper: &BoogieWrapper,
        model: &Model,
        key_ty: &Type,
        val_ty: &Type,
    ) -> Option<PrettyDoc> {
        // Compute the encoding map. Table keys are encoded as ints, and this finds the
        // function table of $EncodeKey and turns into a map from int to encoded ModelValue.
        let encoding_key = format!(
            "$EncodeKey{}",
            boogie_inst_suffix(wrapper.env, &[key_ty.clone()])
        );
        let encoding_map = model
            .vars
            .get(&ModelValue::literal(&encoding_key))?
            .extract_encoding_map()?;
        // Build the entries to display
        let ModelValueTable { keys, values, open } = self.extract_table(model, key_ty, val_ty)?;
        let mut entries = vec![];
        for key in keys {
            let key_rep = if let Some(k) = encoding_map.get(&key) {
                k.pretty_or_raw(wrapper, model, key_ty)
            } else {
                PrettyDoc::text("?")
            };
            let val_rep = if let Some(val) = values.get(&key) {
                val.pretty_or_raw(wrapper, model, val_ty)
            } else {
                PrettyDoc::text("?")
            };
            entries.push(key_rep.append(PrettyDoc::text(": ")).append(val_rep))
        }
        if open {
            // Append a marker to indicate this table is open ended.
            entries.push(PrettyDoc::text(".."));
        }
        Some(PrettyDoc::text("Table").append(Self::pretty_vec_or_struct_body(entries)))
    }

    /// Pretty prints the global memory
    pub fn pretty_mem(
        &self,
        wrapper: &BoogieWrapper,
        domain: &ModelValue,
        domain_info_opt: &Option<(
            BTreeMap<ModelValue, BTreeMap<usize, bool>>,
            Option<ModelValue>,
            bool,
        )>,
        model: &Model,
        param: &Type,
    ) -> Option<PrettyDoc> {
        let values = self.extract_mem_vector(model, domain, domain_info_opt)?;
        let mut entries = vec![];
        for idx in values.values.keys().sorted() {
            let mut p = values.values.get(idx)?.pretty_or_raw(wrapper, model, param);
            p = PrettyDoc::text(format!("Address({}): ", idx)).append(p);
            entries.push(p);
        }
        let default = if values.default == ModelValue::error() {
            PrettyDoc::text("empty")
        } else {
            values
                .default
                .pretty(wrapper, model, param)
                .unwrap_or_else(|| PrettyDoc::text("undef"))
        };
        entries.push(PrettyDoc::text("Default: ").append(default));
        Some(Self::pretty_vec_or_struct_body(entries))
    }
}

/// Represents an expression descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct ExpDescriptor {
    node_id: NodeId,
}

/// Represents parser for a boogie model.
struct ModelParser<'s> {
    input: &'s str,
    at: usize,
}

/// Represents error resulting from model parsing.
struct ModelParseError(String);

impl ModelParseError {
    fn new(s: &str) -> Self {
        ModelParseError(s.to_string())
    }
}

impl From<ParseIntError> for ModelParseError {
    fn from(_: ParseIntError) -> Self {
        Self::new("invalid integer")
    }
}

const MODEL_END_MARKER: &str = "*** END_MODEL";

impl<'s> ModelParser<'s> {
    fn skip_space(&mut self) {
        while self.input[self.at..].starts_with(|ch| [' ', '\r', '\n', '\t'].contains(&ch)) {
            self.at = usize::saturating_add(self.at, 1);
        }
    }

    fn looking_at(&mut self, s: &str) -> bool {
        self.skip_space();
        self.input[self.at..].starts_with(s)
    }

    fn looking_at_eol(&mut self) -> bool {
        while self.input[self.at..].starts_with(|ch| [' ', '\r', '\t'].contains(&ch)) {
            self.at = usize::saturating_add(self.at, 1);
        }
        self.input[self.at..].starts_with('\n')
    }

    fn looking_at_then_consume(&mut self, s: &str) -> bool {
        if self.looking_at(s) {
            self.at = usize::saturating_add(self.at, s.len());
            true
        } else {
            false
        }
    }

    fn expect(&mut self, s: &str) -> Result<(), ModelParseError> {
        self.skip_space();
        if self.input[self.at..].starts_with(s) {
            self.at = usize::saturating_add(self.at, s.len());
            Ok(())
        } else {
            let end = std::cmp::min(usize::saturating_add(self.at, 80), self.input.len());
            Err(ModelParseError(format!(
                "expected `{}` (at `{}...`)",
                s,
                &self.input[self.at..end]
            )))
        }
    }

    fn parse_map(&mut self) -> Result<ModelValue, ModelParseError> {
        let mut map = BTreeMap::new();
        while !self.looking_at("}") && !self.looking_at(MODEL_END_MARKER) {
            let key = self.parse_key()?;
            self.expect("->")?;
            if self.looking_at_eol() {
                // Entry without a value, skip
                continue;
            }
            let value = if self.looking_at_then_consume("{") {
                let value = self.parse_map()?;
                self.expect("}")?;
                value
            } else {
                self.parse_value()?
            };
            map.insert(key, value);
        }
        Ok(ModelValue::Map(map))
    }

    fn parse_key(&mut self) -> Result<ModelValue, ModelParseError> {
        let mut comps = vec![];
        while !self.looking_at("->") && self.at < self.input.len() {
            let value = self.parse_value()?;
            comps.push(value);
        }
        if comps.is_empty() {
            Err(ModelParseError(
                "expected at least one component of a key".to_string(),
            ))
        } else if comps.len() == 1 {
            Ok(comps.pop().unwrap())
        } else {
            Ok(ModelValue::List(comps))
        }
    }

    fn parse_value(&mut self) -> Result<ModelValue, ModelParseError> {
        if self.looking_at_then_consume("(") {
            let mut comps = vec![];
            while !self.looking_at_then_consume(")") {
                let value = self.parse_value()?;
                comps.push(value);
            }
            Ok(ModelValue::List(comps))
        } else {
            // We do not know the exact lexis, so take everything until next space or ).
            self.skip_space();
            let start = self.at;
            while self.at < self.input.len()
                && !self.input[self.at..]
                    .starts_with(|ch| [')', ' ', '\r', '\n', '\t'].contains(&ch))
            {
                self.at = usize::saturating_add(self.at, 1);
            }
            Ok(ModelValue::Literal(self.input[start..self.at].to_string()))
        }
    }
}

fn index_range_check(max: usize) -> impl FnOnce(usize) -> Result<usize, ModelParseError> {
    move |idx: usize| -> Result<usize, ModelParseError> {
        if idx < max {
            Ok(idx)
        } else {
            Err(ModelParseError::new(&format!(
                "invalid debug track info: index out of range (upper bound {}, got {})",
                max, idx
            )))
        }
    }
}
