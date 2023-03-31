// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{sandbox::utils::module, DEFAULT_BUILD_DIR, DEFAULT_STORAGE_DIR};

use move_command_line_common::{
    env::read_bool_env_var,
    files::{find_filenames, path_to_string},
    testing::{add_update_baseline_fix, format_diff, read_env_update_baseline, EXP_EXT},
};
use move_compiler::command_line::COLOR_MODE_ENV_VAR;
use move_coverage::coverage_map::{CoverageMap, ExecCoverageMapWithModules};
use move_package::{
    compilation::{compiled_package::OnDiskCompiledPackage, package_layout::CompiledPackageLayout},
    resolution::resolution_graph::ResolvedGraph,
    source_package::{layout::SourcePackageLayout, manifest_parser::parse_move_manifest_from_file},
    BuildConfig,
};
use std::{
    collections::{BTreeMap, HashMap},
    env,
    fmt::Write as FmtWrite,
    fs::{self, File},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::tempdir;

/// Basic datatest testing framework for the CLI. The `run_one` entrypoint expects
/// an `args.txt` file with arguments that the `move` binary understands (one set
/// of arguments per line). The testing framework runs the commands, compares the
/// result to the expected output, and runs `move clean` to discard resources,
/// modules, and event data created by running the test.

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

fn collect_coverage(
    trace_file: &Path,
    build_dir: &Path,
) -> anyhow::Result<ExecCoverageMapWithModules> {
    let canonical_build = build_dir.canonicalize().unwrap();
    let package_name = parse_move_manifest_from_file(
        &SourcePackageLayout::try_find_root(&canonical_build).unwrap(),
    )?
    .package
    .name
    .to_string();
    let pkg = OnDiskCompiledPackage::from_path(
        &build_dir
            .join(package_name)
            .join(CompiledPackageLayout::BuildInfo.path()),
    )?
    .into_compiled_package()?;
    let src_modules = pkg
        .all_modules()
        .map(|unit| {
            let absolute_path = path_to_string(&unit.source_path.canonicalize()?)?;
            Ok((absolute_path, module(&unit.unit)?.clone()))
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

fn determine_package_nest_depth(
    resolution_graph: &ResolvedGraph,
    pkg_dir: &Path,
) -> anyhow::Result<usize> {
    let mut depth = 0;
    for (_, dep) in resolution_graph.package_table.iter() {
        depth = std::cmp::max(
            depth,
            dep.package_path.strip_prefix(pkg_dir)?.components().count() + 1,
        );
    }
    Ok(depth)
}

fn pad_tmp_path(tmp_dir: &Path, pad_amount: usize) -> anyhow::Result<PathBuf> {
    let mut tmp_dir = tmp_dir.to_path_buf();
    for i in 0..pad_amount {
        tmp_dir.push(format!("{}", i));
    }
    std::fs::create_dir_all(&tmp_dir)?;
    Ok(tmp_dir)
}

// We need to copy dependencies over (transitively) and at the same time keep the paths valid in
// the package. To do this we compute the resolution graph for all possible dependencies (so in dev
// mode) and then calculate the nesting under `tmp_dir` the we need to copy the root package so
// that it, and all its dependencies reside under `tmp_dir` with the same paths as in the original
// package manifest.
fn copy_deps(tmp_dir: &Path, pkg_dir: &Path) -> anyhow::Result<PathBuf> {
    // Sometimes we run a test that isn't a package for metatests so if there isn't a package we
    // don't need to nest at all. Resolution graph diagnostics are only needed for CLI commands so
    // ignore them by passing a vector as the writer.
    let package_resolution = match (BuildConfig {
        dev_mode: true,
        ..Default::default()
    })
    .resolution_graph_for_package(pkg_dir, &mut Vec::new())
    {
        Ok(pkg) => pkg,
        Err(_) => return Ok(tmp_dir.to_path_buf()),
    };
    let package_nest_depth = determine_package_nest_depth(&package_resolution, pkg_dir)?;
    let tmp_dir = pad_tmp_path(tmp_dir, package_nest_depth)?;
    for (_, dep) in package_resolution.package_table.iter() {
        let source_dep_path = &dep.package_path;
        let dest_dep_path = tmp_dir.join(dep.package_path.strip_prefix(pkg_dir).unwrap());
        if !dest_dep_path.exists() {
            fs::create_dir_all(&dest_dep_path)?;
        }
        simple_copy_dir(&dest_dep_path, source_dep_path)?;
    }
    Ok(tmp_dir)
}

fn simple_copy_dir(dst: &Path, src: &Path) -> io::Result<()> {
    for entry in fs::read_dir(src)? {
        let src_entry = entry?;
        let src_entry_path = src_entry.path();
        let dst_entry_path = dst.join(src_entry.file_name());
        if src_entry_path.is_dir() {
            fs::create_dir_all(&dst_entry_path)?;
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
        // symlink everything in the exe_dir into the temp_dir
        let dir = tempdir()?;
        let padded_dir = copy_deps(dir.path(), exe_dir)?;
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
    env::set_var(COLOR_MODE_ENV_VAR, "NONE");
    for args_line in args_file {
        let args_line = args_line?;

        if let Some(external_cmd) = args_line.strip_prefix('>') {
            let external_cmd = external_cmd.trim_start();
            let mut cmd_iter = external_cmd.split_ascii_whitespace();

            let external_program = cmd_iter.next().expect("empty external command");

            let mut command = Command::new(external_program);
            command.args(cmd_iter);
            if let Some(work_dir) = temp_dir.as_ref() {
                command.current_dir(&work_dir.1);
            } else {
                command.current_dir(exe_dir);
            }
            let cmd_output = command.output()?;

            writeln!(&mut output, "External Command `{}`:", external_cmd)?;
            output += std::str::from_utf8(&cmd_output.stdout)?;
            output += std::str::from_utf8(&cmd_output.stderr)?;

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
                env::remove_var(MOVE_VM_TRACING_ENV_VAR_NAME);
            }
            Some(path) => env::set_var(MOVE_VM_TRACING_ENV_VAR_NAME, path.as_os_str()),
        }

        let cmd_output = cli_command_template().args(args_iter).output()?;
        writeln!(&mut output, "Command `{}`:", args_line)?;
        output += std::str::from_utf8(&cmd_output.stdout)?;
        output += std::str::from_utf8(&cmd_output.stderr)?;
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
        if let Some(trace_path) = &trace_file {
            if trace_path.exists() {
                fs::remove_file(trace_path)?;
            }
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

    // find `args.txt` and iterate over them
    for entry in find_filenames(&[args_path], |fpath| {
        fpath.file_name().expect("unexpected file entry path") == TEST_ARGS_FILENAME
    })? {
        match run_one(Path::new(&entry), cli_binary, use_temp_dir, track_cov) {
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
