// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{DEFAULT_BUILD_DIR, DEFAULT_STORAGE_DIR};

use move_command_line_common::{
    env::read_bool_env_var,
    files::{find_filenames, path_to_string},
};
use move_compiler::command_line::COLOR_MODE_ENV_VAR;
use move_coverage::coverage_map::{CoverageMap, ExecCoverageMapWithModules};

use move_package_alt::{
    flavor::{Vanilla, vanilla},
    package::{RootPackage, layout::SourcePackageLayout},
};
use move_package_alt_compilation::{
    layout::CompiledPackageLayout, on_disk_package::OnDiskCompiledPackage,
};
use path_clean::clean;
use std::{
    cmp::max,
    collections::{BTreeMap, HashMap},
    env,
    fmt::Write as FmtWrite,
    fs::{self, File},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;
use tracing::debug;

// Basic datatest testing framework for the CLI. The `run_one` entrypoint expects
// an `args.txt` file with arguments that the `move` binary understands (one set
// of arguments per line). The testing framework runs the commands, compares the
// result to the expected output, and runs `move clean` to discard resources,
// modules, and event data created by running the test.

/// If this env var is set, `move clean` will not be run after each test.
/// this is useful if you want to look at the `storage` or `move_events`
/// produced by a test. However, you'll have to manually run `move clean`
/// before re-running the test.
const NO_MOVE_CLEAN: &str = "NO_MOVE_CLEAN";

/// The filename that contains the arguments to the Move binary.
pub const TEST_ARGS_FILENAME: &str = "args.txt";

/// Name of the environment variable we need to set in order to get tracing
/// enabled in the move VM.
const MOVE_VM_TRACING_ENV_VAR_NAME: &str = "MOVE_VM_TRACE";

/// The default file name (inside the build output dir) for the runtime to
/// dump the execution trace to. The trace will be used by the coverage tool
/// if --track-cov is set. If --track-cov is not set, then no trace file will
/// be produced.
const DEFAULT_TRACE_FILE: &str = "trace";

/// The prefix for the stack trace that we want to remove from the stderr output if present.
const STACK_TRACE_PREFIX: &str = "\nStack backtrace:";

fn collect_coverage(
    trace_file: &Path,
    build_dir: &Path,
) -> anyhow::Result<ExecCoverageMapWithModules> {
    let canonical_build = build_dir.canonicalize().unwrap();

    let pkg_root = &SourcePackageLayout::try_find_root(&canonical_build).unwrap();
    let package_name = move_package_alt::read_name_from_manifest(pkg_root)?;

    let pkg_path = &build_dir
        .join(package_name)
        .join(CompiledPackageLayout::BuildInfo.path());
    let pkg = OnDiskCompiledPackage::from_path(pkg_path)?.into_compiled_package()?;

    let src_modules = pkg
        .all_compiled_units_with_source()
        .map(|unit| {
            let absolute_path = path_to_string(&unit.source_path.canonicalize()?)?;
            Ok((absolute_path, unit.unit.module.clone()))
        })
        .collect::<anyhow::Result<HashMap<_, _>>>()?;

    // build the filter
    let mut filter = BTreeMap::new();
    for (entry, module) in src_modules.into_iter() {
        let module_id = module.self_id();
        filter
            .entry(*module_id.address())
            .or_insert_with(BTreeMap::new)
            .insert(module_id.name().to_owned(), (entry, module));
    }

    // collect filtered trace
    let coverage_map = CoverageMap::from_trace_file(trace_file)
        .to_unified_exec_map()
        .into_coverage_map_with_modules(filter);

    Ok(coverage_map)
}

/// Given a list of paths `paths = [p_1, p_2, p_3, ...], produce another path `result = dir/dir/dir/...`
/// so that for each `i`, `{result}/{p_i}` cleans to a path with no `..`
///
/// For example, if `paths` is  [`foo`, `../bar`, `../../../../baz`], then `make_dir_prefix(paths)`
/// would be `dir/dir/dir/dir` so that `dir/dir/dir/dir/../../../../baz` would clean to `baz`.
fn make_dir_prefix(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> PathBuf {
    let mut max_depth = 0;
    for path in paths {
        let mut depth = 0;
        for component in clean(path).components() {
            if component.as_os_str() != ".." {
                break;
            }
            depth += 1;
        }
        max_depth = max(max_depth, depth);
    }
    let mut result = PathBuf::new();
    for _ in 0..max_depth - 1 {
        result.push("dir");
    }
    result
}

/// Copy `pkg_dir` and all of its dependencies into `tmp_dir`, keeping all of the relative
/// paths the same. This may require copying into a subdirectory of `tmp_dir` if the local paths
/// start with `..`; the actual subdirectory containing the copied files is returned.
fn copy_pkg_and_deps(tmp_dir: &Path, pkg_dir: &Path) -> anyhow::Result<PathBuf> {
    let paths = match package_paths(pkg_dir) {
        Ok(paths) => paths,
        Err(e) => {
            debug!("couldn't find packages: {e}");
            [pkg_dir.to_path_buf()].into()
        }
    };

    let prefix = make_dir_prefix(&paths);

    debug!("copying {paths:?}");

    for path in paths {
        debug!("cp {:?} {:?}", &path, tmp_dir.join(&prefix).join(&path));
        simple_copy_dir(&tmp_dir.join(&prefix).join(&path), &path)?;
    }

    Ok(tmp_dir.join(prefix).join(pkg_dir))
}

/// Return the paths to all the packages needed by the package at `pkg_dir` (including itself); if
/// the package cannot be loaded we just return the package itself. (sometimes we run a test that
/// isn't a package for metatests so if there isn't a package we don't need to nest at all).
///
/// We copy as if `--mode test` were passed, so that `dev-dependencies` will be included; if tests
/// use moded dependencies with any other modes, those dependencies won't be copied and this code
/// will need to be fixed.
fn package_paths(pkg_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let rt = tokio::runtime::Runtime::new()?;

    let root_pkg = rt.block_on(RootPackage::<Vanilla>::load(
        pkg_dir,
        vanilla::default_environment(),
        vec!["test".into()],
    ))?;

    let packages = root_pkg.packages();

    Ok(packages
        .iter()
        .map(|pkg| pkg.path().path().to_path_buf())
        .collect())
}

/// Recursively copy all files in `src` into `dir`
fn simple_copy_dir(dst: &Path, src: &Path) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let src_entry = entry?;
        let src_entry_path = src_entry.path();
        let dst_entry_path = dst.join(src_entry.file_name());
        if src_entry_path.is_dir() {
            simple_copy_dir(&dst_entry_path, &src_entry_path)?;
        } else {
            fs::copy(&src_entry_path, &dst_entry_path)?;
        }
    }
    Ok(())
}

/// Run the `args_path` batch file with`cli_binary`
pub fn run_one(
    args_path: &Path,
    cli_binary: &Path,
    use_temp_dir: bool,
    track_cov: bool,
) -> anyhow::Result<Option<ExecCoverageMapWithModules>> {
    let args_file = io::BufReader::new(File::open(args_path)?).lines();
    let cli_binary_path = cli_binary.canonicalize()?;

    // path where we will run the binary
    let exe_dir = args_path.parent().unwrap();
    let temp_dir = if use_temp_dir {
        // copy everything in the exe_dir into the temp_dir
        let dir = tempdir()?;
        let padded_dir = copy_pkg_and_deps(dir.path(), exe_dir)?;
        simple_copy_dir(&padded_dir, exe_dir)?;
        Some((dir, padded_dir))
    } else {
        None
    };
    let wks_dir = temp_dir.as_ref().map_or(exe_dir, |t| &t.1);

    let storage_dir = wks_dir.join(DEFAULT_STORAGE_DIR);
    let build_output = wks_dir
        .join(DEFAULT_BUILD_DIR)
        .join(CompiledPackageLayout::Root.path());

    // template for preparing a cli command
    let cli_command_template = || {
        let mut command = Command::new(cli_binary_path.clone());
        if let Some(work_dir) = temp_dir.as_ref() {
            command.current_dir(&work_dir.1);
        } else {
            command.current_dir(exe_dir);
        }
        command
    };

    if storage_dir.exists() || build_output.exists() {
        // need to clean before testing
        cli_command_template()
            .arg("sandbox")
            .arg("clean")
            .output()?;
    }
    let mut output = "".to_string();

    // always use the absolute path for the trace file as we may change dirs in the process
    let trace_file = if track_cov {
        Some(wks_dir.canonicalize()?.join(DEFAULT_TRACE_FILE))
    } else {
        None
    };

    // Disable colors in error reporting from the Move compiler
    unsafe { env::set_var(COLOR_MODE_ENV_VAR, "NONE") };
    for args_line in args_file {
        let args_line = args_line?;

        if let Some(external_cmd) = args_line.strip_prefix('>') {
            let external_cmd = external_cmd.trim_start();
            let mut command = Command::new("sh");
            command.arg("-c").arg(external_cmd);
            if let Some(work_dir) = temp_dir.as_ref() {
                command.current_dir(&work_dir.1);
            } else {
                command.current_dir(exe_dir);
            }
            let cmd_output = command.output()?;

            writeln!(&mut output, "External Command `{}`:", external_cmd)?;
            output += std::str::from_utf8(cmd_output.stdout.trim_ascii_start())?;
            output += std::str::from_utf8(cmd_output.stderr.trim_ascii_start())?;

            continue;
        }

        if args_line.starts_with('#') {
            // allow comments in args.txt
            continue;
        }
        let args_iter: Vec<&str> = args_line.split_whitespace().collect();
        if args_iter.is_empty() {
            // allow blank lines in args.txt
            continue;
        }

        // enable tracing in the VM by setting the env var.
        match &trace_file {
            None => {
                // this check prevents cascading the coverage tracking flag.
                // in particular, if
                //   1. we run with move-cli test <path-to-args-A.txt> --track-cov, and
                //   2. in this <args-A.txt>, there is another command: test <args-B.txt>
                // then, when running <args-B.txt>, coverage will not be tracked nor printed
                unsafe { env::remove_var(MOVE_VM_TRACING_ENV_VAR_NAME) };
            }
            Some(path) => unsafe { env::set_var(MOVE_VM_TRACING_ENV_VAR_NAME, path.as_os_str()) },
        }

        let cmd_output = cli_command_template().args(args_iter).output()?;
        writeln!(&mut output, "Command `{}`:", args_line)?;
        output += std::str::from_utf8(&cmd_output.stdout)?;
        let stderr_output = std::str::from_utf8(&cmd_output.stderr)?;
        // Remove stack traces from the stderr output if they exist
        let clean_stderr = stderr_output.split(STACK_TRACE_PREFIX).next().unwrap();
        output += clean_stderr;
    }

    // collect coverage information
    let cov_info = match &trace_file {
        None => None,
        Some(trace_path) => {
            if trace_path.exists() {
                Some(collect_coverage(trace_path, &build_output)?)
            } else {
                eprintln!(
                    "Trace file {:?} not found: coverage is only available with at least one `run` \
                    command in the args.txt (after a `clean`, if there is one)",
                    trace_path
                );
                None
            }
        }
    };

    // post-test cleanup and cleanup checks
    // check that the test command didn't create a src dir
    let run_move_clean = !read_bool_env_var(NO_MOVE_CLEAN);
    if run_move_clean {
        // run the clean command to ensure that temporary state is cleaned up
        cli_command_template()
            .arg("sandbox")
            .arg("clean")
            .output()?;

        // check that build and storage was deleted
        assert!(
            !storage_dir.exists(),
            "`move clean` failed to eliminate {} directory",
            DEFAULT_STORAGE_DIR
        );
        assert!(
            !build_output.exists(),
            "`move clean` failed to eliminate {} directory",
            DEFAULT_BUILD_DIR
        );

        // clean the trace file as well if it exists
        if let Some(trace_path) = &trace_file
            && trace_path.exists()
        {
            fs::remove_file(trace_path)?;
        }
    }

    // release the temporary workspace explicitly
    if let Some((t, _)) = temp_dir {
        t.close()?;
    }

    // compare output and exp_file
    let update_baseline = read_env_update_baseline();
    let exp_path = args_path.with_extension(EXP_EXT);
    if update_baseline {
        fs::write(exp_path, &output)?;
        return Ok(cov_info);
    }

    let expected_output = fs::read_to_string(exp_path).unwrap_or_else(|_| "".to_string());
    if expected_output != output {
        let msg = format!(
            "Expected output differs from actual output:\n{}",
            format_diff(expected_output, output)
        );
        anyhow::bail!(add_update_baseline_fix(msg))
    } else {
        Ok(cov_info)
    }
}

pub fn run_all(
    args_path: &Path,
    cli_binary: &Path,
    use_temp_dir: bool,
    track_cov: bool,
) -> anyhow::Result<()> {
    let mut test_total: u64 = 0;
    let mut test_passed: u64 = 0;
    let mut cov_info = ExecCoverageMapWithModules::empty();

    debug!("Current directory: {:?}", std::env::current_dir()?);

    // find `args.txt` and iterate over them
    for entry in find_filenames(&[args_path], |fpath| {
        tracing::debug!("fpath: {}", fpath.display());
        fpath.file_name().expect("unexpected file entry path") == TEST_ARGS_FILENAME
    })? {
        tracing::debug!("Entry {entry}, {args_path:?}");
        tracing::debug!(
            "Current directory when processing entry: {:?}",
            std::env::current_dir()?
        );
        // The entry path is already correct relative to the current directory
        // since find_filenames returns paths that include the base directory
        let entry_path = Path::new(&entry);
        tracing::debug!(
            "About to call run_one with path: {:?}, exists: {}",
            entry_path,
            entry_path.exists()
        );

        match run_one(entry_path, cli_binary, use_temp_dir, track_cov) {
            Ok(cov_opt) => {
                test_passed = test_passed.checked_add(1).unwrap();
                if let Some(cov) = cov_opt {
                    cov_info.merge(cov);
                }
            }
            Err(ex) => eprintln!("Test {} failed with error: {}", entry, ex),
        }
        test_total = test_total.checked_add(1).unwrap();
    }
    println!("{} / {} test(s) passed.", test_passed, test_total);

    // if any test fails, bail
    let test_failed = test_total.checked_sub(test_passed).unwrap();
    if test_failed != 0 {
        anyhow::bail!("{} / {} test(s) failed.", test_failed, test_total)
    }

    // show coverage information if requested
    if track_cov {
        let mut summary_writer: Box<dyn Write> = Box::new(io::stdout());
        for (_, module_summary) in cov_info.into_module_summaries() {
            module_summary.summarize_human(&mut summary_writer, true)?;
        }
    }

    Ok(())
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// The following code is migrated from `move-command-line-common` crate, which switched to `insta`
// for expected output testing. That is not really desierable for the Move CLI, so it has kept
// this hand rolled approach.

/// Extension for expected output files
const EXP_EXT: &str = "exp";

/// If any of these env vars is set, the test harness should overwrite
/// the existing .exp files with the output instead of checking
/// them against the output.
const UPDATE_BASELINE: &str = "UPDATE_BASELINE";
const UPBL: &str = "UPBL";
const UB: &str = "UB";

fn read_env_update_baseline() -> bool {
    read_bool_env_var(UPDATE_BASELINE) || read_bool_env_var(UPBL) || read_bool_env_var(UB)
}

fn add_update_baseline_fix(s: impl AsRef<str>) -> String {
    format!(
        "{}\n\
        Run with `env {}=1` (or `env {}=1`) to save the current output as \
        the new expected output",
        s.as_ref(),
        UB,
        UPDATE_BASELINE
    )
}

fn format_diff(expected: impl AsRef<str>, actual: impl AsRef<str>) -> String {
    use colored::Colorize;
    use similar::ChangeTag;
    let diff = similar::TextDiff::from_lines(expected.as_ref(), actual.as_ref());

    diff.iter_all_changes()
        .map(|change| match change.tag() {
            ChangeTag::Delete => format!("{}{}", "-".bold(), change.value()).red(),
            ChangeTag::Insert => format!("{}{}", "+".bold(), change.value()).green(),
            ChangeTag::Equal => change.value().dimmed(),
        })
        .map(|s| s.to_string())
        .collect()
}
