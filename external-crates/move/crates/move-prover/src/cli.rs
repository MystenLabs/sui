// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

//! Functionality related to the command line interface of the Move prover.

use std::{
    collections::BTreeMap,
    io::IsTerminal,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::anyhow;
use clap::{Arg, Command};
use log::LevelFilter;
use move_compiler::shared::NumericalAddress;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use simplelog::{
    CombinedLogger, Config, ConfigBuilder, LevelPadding, SimpleLogger, TermLogger, TerminalMode,
};

use codespan_reporting::diagnostic::Severity;
use move_docgen::DocgenOptions;
use move_model::options::ModelBuilderOptions;
use move_stackless_bytecode::options::{AutoTraceLevel, ProverOptions};

/// Atomic used to prevent re-initialization of logging.
static LOGGER_CONFIGURED: AtomicBool = AtomicBool::new(false);

/// Atomic used to detect whether we are running in test mode.
static TEST_MODE: AtomicBool = AtomicBool::new(false);

/// Represents options provided to the tool. Most of those options are configured via a toml
/// source; some over the command line flags.
///
/// NOTE: any fields carrying structured data must appear at the end for making
/// toml printing work. When changing this config, use `mvp --print-config` to
/// verify this works.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Options {
    /// The path to the boogie output which represents the verification problem.
    pub output_path: String,
    /// Verbosity level for logging.
    pub verbosity_level: LevelFilter,
    /// Whether to run the documentation generator instead of the prover.
    pub run_docgen: bool,
    /// Whether to run the internal reference escape analysis instead of the prover
    pub run_escape: bool,
    /// The paths to the Move sources.
    pub move_sources: Vec<String>,
    /// The paths to any dependencies for the Move sources. Those will not be verified but
    /// can be used by `move_sources`.
    pub move_deps: Vec<String>,
    /// The values assigned to named addresses in the Move code being verified.
    pub move_named_address_values: Vec<String>,
    /// Whether to run experimental pipeline
    pub experimental_pipeline: bool,

    /// BEGIN OF STRUCTURED OPTIONS. DO NOT ADD VALUE FIELDS AFTER THIS
    /// Options for the model builder.
    pub model_builder: ModelBuilderOptions,
    /// Options for the documentation generator.
    pub docgen: DocgenOptions,
    /// Options for the prover.
    pub prover: ProverOptions,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            output_path: "output.bpl".to_string(),
            run_docgen: false,
            run_escape: false,
            verbosity_level: LevelFilter::Info,
            move_sources: vec![],
            move_deps: vec![],
            move_named_address_values: vec![],
            model_builder: ModelBuilderOptions::default(),
            prover: ProverOptions::default(),
            docgen: DocgenOptions::default(),
            experimental_pipeline: false,
        }
    }
}

pub static DEFAULT_OPTIONS: Lazy<Options> = Lazy::new(Options::default);

impl Options {
    /// Creates options from toml configuration source.
    pub fn create_from_toml(toml_source: &str) -> anyhow::Result<Options> {
        Ok(toml::from_str(toml_source)?)
    }

    /// Creates options from toml configuration file.
    pub fn create_from_toml_file(toml_file: &str) -> anyhow::Result<Options> {
        Self::create_from_toml(&std::fs::read_to_string(toml_file)?)
    }

    // Creates options from command line arguments. This parses the arguments and terminates
    // the program on errors, printing usage information. The first argument is expected to be
    // the program name.
    pub fn create_from_args(args: &[String]) -> anyhow::Result<Options> {
        // Clap definition of the command line interface.
        let is_number = |s: &str| {
            s.parse::<usize>()
                .map(|_| ())
                .map_err(|_| "expected number".to_string())
        };
        let cli = Command::new("mvp")
            .version("0.1.0")
            .about("The Move Prover")
            .author("The Diem Core Contributors")
            .arg(
                Arg::new("config")
                    .short('c')
                    .long("config")
                    .num_args(1)
                    .value_name("TOML_FILE")
                    .help("path to a configuration file. \
                     Values in this file will be overridden by command line flags"),
            )
            .arg(
                Arg::new("config-str")
                    .conflicts_with("config")
                    .short('C')
                    .long("config-str")
                    .num_args(1)
                    .action(clap::ArgAction::Append)
                    .number_of_values(1)
                    .value_name("TOML_STRING")
                    .help("inlines configuration string in toml syntax. Can be repeated. \
                     Use as in `-C=prover.opt=value -C=backend.opt=value`"),
            )
            .arg(
                Arg::new("print-config")
                    .long("print-config")
                    .action(clap::ArgAction::SetTrue)
                    .help("prints the effective toml configuration, then exits")
            )
            .arg(
                Arg::new("output")
                    .short('o')
                    .long("output")
                    .num_args(1)
                    .value_name("BOOGIE_FILE")
                    .help("path to the boogie output which represents the verification problem"),
            )
            .arg(
                Arg::new("verbosity")
                    .short('v')
                    .long("verbose")
                    .num_args(1)
                    .value_parser(["error", "warn", "info", "debug"])
                    .help("verbosity level"),
            )
            .arg(
                Arg::new("vector-theory")
                    .long("vector-theory")
                    .num_args(1)
                    .value_parser(["BoogieArray", "BoogieArrayIntern",
                                              "SmtArray", "SmtArrayExt", "SmtSeq"])
                    .help("vector theory to use"),
            )
            .arg(
                Arg::new("generate-only")
                    .long("generate-only")
                    .short('g')
                    .action(clap::ArgAction::SetTrue)
                    .help("only generates boogie file but does not call boogie"),
            )
            .arg(
                Arg::new("severity")
                    .long("severity")
                    .short('s')
                    .num_args(1)
                    .value_parser(["bug", "error", "warn", "note"])
                    .help("The minimall level on which diagnostics are reported")
            )
            .arg(
                Arg::new("trace")
                    .long("trace")
                    .short('t')
                    .action(clap::ArgAction::SetTrue)
                    .help("enables automatic tracing of expressions in prover errors")
            )
            .arg(
                Arg::new("keep")
                    .long("keep")
                    .short('k')
                    .action(clap::ArgAction::SetTrue)
                    .help("keeps intermediate artifacts of the backend around")
            )
            .arg(
                Arg::new("boogie-poly")
                    .long("boogie-poly")
                    .action(clap::ArgAction::SetTrue)
                    .help("whether to use the old polymorphic Boogie backend")
            )
            .arg(
                Arg::new("inv-v1")
                    .long("inv-v1")
                    .action(clap::ArgAction::SetTrue)
                    .help("whether to use the old v1 invariant processing (without disabled invariants)")
            )
            .arg(
                Arg::new("negative")
                    .long("negative")
                    .action(clap::ArgAction::SetTrue)
                    .help("runs negative verification checks")
            ).arg(
                Arg::new("seed")
                    .long("seed")
                    .short('S')
                    .num_args(1)
                    .value_name("NUMBER")
                    .value_parser(is_number)
                    .help("sets a random seed for the prover (default 0)")
            )
            .arg(
                Arg::new("cores")
                    .long("cores")
                    .num_args(1)
                    .value_name("NUMBER")
                    .value_parser(is_number)
                    .help("sets the number of cores to use. \
                     NOTE: multiple cores may currently lead to scrambled model \
                     output from boogie (default 4)")
            )
            .arg(
                Arg::new("timeout")
                    .long("timeout")
                    .short('T')
                    .num_args(1)
                    .value_name("NUMBER")
                    .value_parser(is_number)
                    .help("sets a timeout (in seconds) for each \
                             individual verification condition (default 40)")
            )
            .arg(
                Arg::new("ignore-pragma-opaque-when-possible")
                    .long("ignore-pragma-opaque-when-possible")
                    .action(clap::ArgAction::SetTrue)
                    .help("Ignore the \"opaque\" pragma on specs of \
                    all functions when possible"),
            )
            .arg(
                Arg::new("ignore-pragma-opaque-internal-only")
                    .long("ignore-pragma-opaque-internal-only")
                    .action(clap::ArgAction::SetTrue)
                    .help("Ignore the \"opaque\" pragma on specs of \
                    internal functions when possible"),
            )
            .arg(
                Arg::new("simplification-pipeline")
                    .long("simplify")
                    .num_args(1)
                    .action(clap::ArgAction::Append)
                    .number_of_values(1)
                    .help("Specify one simplification pass to run on the specifications. \
                    This option May be specified multiple times to compose a pipeline")
            )
            .arg(
                Arg::new("docgen")
                    .long("docgen")
                    .action(clap::ArgAction::SetTrue)
                    .help("runs the documentation generator instead of the prover. \
                    Generated docs will be written into the directory `./doc` unless configured otherwise via toml"),
            )
            .arg(
                Arg::new("docgen-template")
                    .long("docgen-template")
                    .num_args(1)
                    .value_name("FILE")
                    .help("a template for documentation generation."),
            )
            .arg(
                Arg::new("abigen")
                    .long("abigen")
                    .action(clap::ArgAction::SetTrue)
                    .help("runs the ABI generator instead of the prover. \
                    Generated ABIs will be written into the directory `./abi` unless configured otherwise via toml"),
            )
            .arg(
                Arg::new("packedtypesgen")
                    .long("packedtypesgen")
                    .action(clap::ArgAction::SetTrue)
                    .help("runs the packed types generator instead of the prover.")
            )
            .arg(
                Arg::new("escape")
                    .long("escape")
                    .action(clap::ArgAction::SetTrue)
                    .help("runs the escape analysis instead of the prover.")
            )
            .arg(
                Arg::new("read-write-set")
                    .long("read-write-set")
                    .action(clap::ArgAction::SetTrue)
                    .help("runs the read/write set analysis instead of the prover.")
            )
            .arg(
                Arg::new("verify")
                    .long("verify")
                    .num_args(1)
                    .value_parser(["public", "all", "none"])
                    .value_name("SCOPE")
                    .help("default scope of verification \
                    (can be overridden by `pragma verify=true|false`)"),
            )
            .arg(
                Arg::new("bench-repeat")
                    .long("bench-repeat")
                    .num_args(1)
                    .value_name("COUNT")
                    .value_parser(is_number)
                    .help(
                        "for benchmarking: how many times to call the backend on the verification problem",
                    ),
            )
            .arg(
                Arg::new("mutation")
                    .long("mutation")
                    .action(clap::ArgAction::SetTrue)
                    .help(
                        "Specifies to use the mutation pass",
                    ),
            )
            .arg(
                Arg::new("mutation-add-sub")
                    .long("mutation-add-sub")
                    .num_args(1)
                    .value_name("COUNT")
                    .value_parser(is_number)
                    .help(
                        "indicates that this program should mutate the indicated plus operation to a minus\
                        specifically by modifyig the \"nth\" such operation",
                    ),
            )
            .arg(
                Arg::new("mutation-sub-add")
                    .long("mutation-sub-add")
                    .num_args(1)
                    .value_name("COUNT")
                    .value_parser(is_number)
                    .help(
                        "indicates that this program should mutate the indicated minus operation to a plus\
                        specifically by modifyig the \"nth\" such operation",
                    ),
            )
            .arg(
                Arg::new("mutation-mul-div")
                    .long("mutation-mul-div")
                    .num_args(1)
                    .value_name("COUNT")
                    .value_parser(is_number)
                    .help(
                        "indicates that this program should mutate the indicated multiplication operation to a divide\
                        specifically by modifyig the \"nth\" such operation",
                    ),
            )
            .arg(
                Arg::new("mutation-div-mul")
                    .long("mutation-div-mul")
                    .num_args(1)
                    .value_name("COUNT")
                    .value_parser(is_number)
                    .help(
                        "indicates that this program should mutate the indicated divide operation to a multiplication\
                        specifically by modifyig the \"nth\" such operation",
                    ),
            )
            .arg(
                Arg::new("dependencies")
                    .long("dependency")
                    .short('d')
                    .action(clap::ArgAction::Append)
                    .number_of_values(1)
                    .num_args(1)
                    .value_name("PATH_TO_DEPENDENCY")
                    .help("path to a Move file, or a directory which will be searched for \
                    Move files, containing dependencies which will not be verified")
            )
            .arg(
                Arg::new("named-addresses")
                .long("named-addresses")
                .short('a')
                .action(clap::ArgAction::Append)
                .num_args(1)
                .help("specifies the value(s) of named addresses used in Move files")
            )
            .arg(
                Arg::new("sources")
                    .action(clap::ArgAction::Append)
                    .value_name("PATH_TO_SOURCE_FILE")
                    .num_args(1..)
                    .help("the source files to verify"),
            )
            .arg(
                Arg::new("eager-threshold")
                    .long("eager-threshold")
                    .num_args(1)
                    .value_name("NUMBER")
                    .value_parser(is_number)
                    .help("sets the eager threshold for quantifier instantiation (default 100)")
            )
            .arg(
                Arg::new("lazy-threshold")
                    .long("lazy-threshold")
                    .num_args(1)
                    .value_name("NUMBER")
                    .value_parser(is_number)
                    .help("sets the lazy threshold for quantifier instantiation (default 100)")
            )
            .arg(
                Arg::new("dump-bytecode")
                    .long("dump-bytecode")
                    .action(clap::ArgAction::SetTrue)
                    .help("whether to dump the transformed bytecode to a file")
            )
            .arg(
                Arg::new("dump-cfg")
                    .long("dump-cfg")
                    .action(clap::ArgAction::SetTrue)
                    .requires("dump-bytecode")
                    .help("whether to dump the per-function control-flow graphs (in dot format) to files")
            )
            .arg(
                Arg::new("num-instances")
                    .long("num-instances")
                    .num_args(1)
                    .value_name("NUMBER")
                    .value_parser(is_number)
                    .help("sets the number of Boogie instances to run concurrently (default 1)")
            )
            .arg(
                Arg::new("sequential")
                    .long("sequential")
                    .action(clap::ArgAction::SetTrue)
                    .help("whether to run the Boogie instances sequentially")
            )
            .arg(
                Arg::new("stable-test-output")
                    .long("stable-test-output")
                    .action(clap::ArgAction::SetTrue)
                    .help("instruct the prover to produce output in diagnosis which is stable \
                     and suitable for baseline tests. This redacts values in diagnosis which might\
                     be non-deterministic, and may do other things to keep output stable.")
            )
            .arg(
                Arg::new("use-cvc5")
                    .long("use-cvc5")
                    .action(clap::ArgAction::SetTrue)
                    .help("uses cvc5 solver instead of z3")
            )
            .arg(
                Arg::new("use-exp-boogie")
                    .long("use-exp-boogie")
                    .action(clap::ArgAction::SetTrue)
                    .help("uses experimental boogie expected in EXP_BOOGIE_EXE")
            )
            .arg(
                Arg::new("generate-smt")
                    .long("generate-smt")
                    .action(clap::ArgAction::SetTrue)
                    .help("instructs boogie to log smtlib files for verified functions")
            )
            .arg(
                Arg::new("experimental-pipeline")
                    .long("experimental-pipeline")
                    .short('e')
                    .action(clap::ArgAction::SetTrue)
                    .help("whether to run experimental pipeline")
            )
            .arg(
                Arg::new("weak-edges")
                    .long("weak-edges")
                    .action(clap::ArgAction::SetTrue)
                    .help("whether to use exclusively weak edges in borrow analysis")
            )
            .arg(
                Arg::new("exp_mut_param")
                    .long("exp-mut-param")
                    .action(clap::ArgAction::SetTrue)
                    .help("exp_mut_param experiment")
            )
            .arg(
                Arg::new("check-inconsistency")
                    .long("check-inconsistency")
                    .action(clap::ArgAction::SetTrue)
                    .help("checks whether there is any inconsistency")
            )
            .arg(
                Arg::new("unconditional-abort-as-inconsistency")
                    .long("unconditional-abort-as-inconsistency")
                    .action(clap::ArgAction::SetTrue)
                    .help("treat functions that do not return (i.e., abort unconditionally) \
                    as inconsistency violations")
            )
            .arg(
                Arg::new("verify-only")
                    .long("verify-only")
                    .num_args(1)
                    .value_name("FUNCTION_NAME")
                    .help("only generate verification condition for one function. \
                    This overrides verification scope and can be overridden by the pragma verify=false")
            )
            .arg(
                Arg::new("z3-trace")
                    .long("z3-trace")
                    .num_args(1)
                    .value_name("FUNCTION_NAME")
                    .help("only generate verification condition for given function, \
                    and generate a z3 trace file for analysis. The file will be stored \
                    at FUNCTION_NAME.z3log.")
            )
            .arg(
                Arg::new("script-reach")
                    .long("script-reach")
                    .action(clap::ArgAction::SetTrue)
                    .help("For each script function which is verification target, \
                    print out the names of all called functions, directly or indirectly.")
            )
            .arg(
                Arg::new("ban-int-2-bv")
                    .long("ban-int-2-bv")
                    .action(clap::ArgAction::SetTrue)
                    .long("whether allow converting int to bit vector when generating the boogie file")
            )
            .after_help("More options available via `--config file` or `--config-str str`. \
            Use `--print-config` to see format and current values. \
            See `move-prover/src/cli.rs::Option` for documentation.");

        // Parse the arguments. This will abort the program on parsing errors and print help.
        // It will also accept options like --help.
        let matches = cli.get_matches_from(args);

        // Initialize options.
        let mut options = if matches.contains_id("config") {
            if matches.contains_id("config-str") {
                return Err(anyhow!(
                    "currently, if `--config` (including via $MOVE_PROVER_CONFIG) is given \
                       `--config-str` cannot be used. Consider editing your \
                       configuration file instead."
                ));
            }
            let value = matches.get_one::<String>("config").unwrap();
            if value.is_empty() {
                Self::default()
            } else {
                Self::create_from_toml_file(matches.get_one::<String>("config").unwrap())?
            }
        } else if matches.contains_id("config-str") {
            Self::create_from_toml(matches.get_one::<String>("config-str").unwrap())?
        } else {
            Options::default()
        };

        // Analyze arguments.
        if matches.contains_id("output") {
            options.output_path = matches.get_one::<String>("output").unwrap().to_string();
        }
        if matches.contains_id("verbosity") {
            options.verbosity_level = match matches.get_one::<String>("verbosity").unwrap().as_str()
            {
                "error" => LevelFilter::Error,
                "warn" => LevelFilter::Warn,
                "info" => LevelFilter::Info,
                "debug" => LevelFilter::Debug,
                _ => unreachable!("should not happen"),
            }
        }

        if matches.contains_id("severity") {
            options.prover.report_severity =
                match matches.get_one::<String>("severity").unwrap().as_str() {
                    "bug" => Severity::Bug,
                    "error" => Severity::Error,
                    "warn" => Severity::Warning,
                    "note" => Severity::Note,
                    _ => unreachable!("should not happen"),
                }
        }

        if matches.get_flag("generate-only") {
            options.prover.generate_only = true;
        }
        if let Some(m) = matches.get_many::<String>("sources") {
            options.move_sources = m.cloned().collect();
        }
        if let Some(m) = matches.get_many::<String>("dependencies") {
            options.move_deps = m.cloned().collect();
        }
        if let Some(m) = matches.get_many::<String>("named-addresses") {
            options.move_named_address_values = m.cloned().collect();
        }
        if matches.get_flag("mutation") {
            options.prover.mutation = true;
        }
        if matches.contains_id("mutation-add-sub") {
            options.prover.mutation_add_sub = matches
                .get_one::<String>("mutation-add-sub")
                .unwrap()
                .parse::<usize>()?;
        }
        if matches.contains_id("mutation-sub-add") {
            options.prover.mutation_sub_add = matches
                .get_one::<String>("mutation-sub-add")
                .unwrap()
                .parse::<usize>()?;
        }
        if matches.contains_id("mutation-mul-div") {
            options.prover.mutation_mul_div = matches
                .get_one::<String>("mutation-mul-div")
                .unwrap()
                .parse::<usize>()?;
        }
        if matches.contains_id("mutation-div-mul") {
            options.prover.mutation_div_mul = matches
                .get_one::<String>("mutation-div-mul")
                .unwrap()
                .parse::<usize>()?;
        }
        if matches.get_flag("docgen") {
            options.run_docgen = true;
        }
        if matches.contains_id("docgen-template") {
            options.run_docgen = true;
            options.docgen.root_doc_templates = vec![matches
                .get_one::<String>("docgen-template")
                .map(|s| s.to_string())
                .unwrap()]
        }
        if matches.get_flag("escape") {
            options.run_escape = true;
        }
        if matches.get_flag("trace") {
            options.prover.auto_trace_level = AutoTraceLevel::VerifiedFunction;
        }
        if matches.get_flag("dump-bytecode") {
            options.prover.dump_bytecode = true;
        }
        if matches.get_flag("dump-cfg") {
            options.prover.dump_cfg = true;
        }

        if matches.get_flag("check-inconsistency") {
            options.prover.check_inconsistency = true;
        }
        if matches.get_flag("unconditional-abort-as-inconsistency") {
            options.prover.unconditional_abort_as_inconsistency = true;
        }

        if matches.get_flag("ban-int-2-bv") {
            options.prover.ban_int_2_bv = true;
        }

        if matches.get_flag("print-config") {
            println!("{}", toml::to_string(&options).unwrap());
            Err(anyhow!("exiting"))
        } else {
            Ok(options)
        }
    }

    /// Sets up logging based on provided options. This should be called as early as possible
    /// and before any use of info!, warn! etc.
    pub fn setup_logging(&self) {
        if LOGGER_CONFIGURED
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        let config = ConfigBuilder::new()
            .set_time_level(LevelFilter::Debug)
            .set_level_padding(LevelPadding::Off)
            .build();
        let logger = if std::io::stderr().is_terminal() && std::io::stdout().is_terminal() {
            CombinedLogger::init(vec![TermLogger::new(
                self.verbosity_level,
                config,
                TerminalMode::Mixed,
            )])
        } else {
            CombinedLogger::init(vec![SimpleLogger::new(self.verbosity_level, config)])
        };
        logger.expect("Unexpected CombinedLogger init failure");
    }

    pub fn setup_logging_for_test(&self) {
        if LOGGER_CONFIGURED
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        TEST_MODE.store(true, Ordering::Relaxed);
        SimpleLogger::init(self.verbosity_level, Config::default())
            .expect("UnexpectedSimpleLogger failure");
    }

    /// Convenience function to enable debugging (like high verbosity) on this instance.
    pub fn enable_debug(&mut self) {
        self.verbosity_level = LevelFilter::Debug;
    }

    /// Convenience function to set verbosity level to only show errors and warnings.
    pub fn set_quiet(&mut self) {
        self.verbosity_level = LevelFilter::Warn
    }
}

pub fn named_addresses_for_options(
    named_address_values: &BTreeMap<String, NumericalAddress>,
) -> Vec<String> {
    named_address_values
        .iter()
        .map(|(name, addr)| format!("{}={}", name, addr))
        .collect()
}
