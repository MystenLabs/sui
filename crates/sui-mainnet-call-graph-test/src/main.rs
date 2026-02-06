// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Validates move-model-2 call graph extraction against reference `call_graph.json`
//! files generated from 1000 mainnet packages.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use move_binary_format::CompiledModule;
use move_model_2::{call_graph::CallGraph, compiled_model::Model, model::ModelConfig};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "sui-mainnet-call-graph-test")]
#[command(about = "Validate call graph construction against mainnet packages")]
struct Cli {
    /// Path to the dataset directory containing prefix subdirs (0x00..0xff).
    #[arg(long, default_value_t = default_dataset_path())]
    dataset_path: String,

    /// Write generated call graphs as `call_graph_generated.json` alongside reference files.
    #[arg(long)]
    generate: bool,

    /// Alternate output directory for generated files (used with --generate).
    #[arg(long)]
    output_dir: Option<PathBuf>,

    /// Process only packages whose ID starts with this prefix (e.g., "0x00").
    #[arg(long)]
    filter: Option<String>,
}

fn default_dataset_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    format!("{home}/sui-packages/packages/mainnet_most_used")
}

// ── Serde types matching reference JSON format ──────────────────────────────

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct PackageCallGraph {
    package_id: String,
    module_call_graphs: Vec<ModuleCallGraph>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct ModuleCallGraph {
    module_name: String,
    call_graph: BTreeMap<String, Vec<String>>,
}

// ── Package discovery ───────────────────────────────────────────────────────

struct PackageDir {
    /// Full 0x-prefixed, 64-hex-char package ID.
    package_id: String,
    /// Path to the package directory.
    path: PathBuf,
}

fn discover_packages(dataset_path: &Path, filter: Option<&str>) -> Result<Vec<PackageDir>> {
    let mut packages = Vec::new();

    let mut prefix_dirs: Vec<_> = fs::read_dir(dataset_path)
        .with_context(|| format!("cannot read dataset dir: {}", dataset_path.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    prefix_dirs.sort_by_key(|e| e.file_name());

    for prefix_entry in prefix_dirs {
        let prefix_name = prefix_entry.file_name();
        let prefix_str = prefix_name.to_string_lossy();

        let mut pkg_dirs: Vec<_> = fs::read_dir(prefix_entry.path())
            .with_context(|| {
                format!("cannot read prefix dir: {}", prefix_entry.path().display())
            })?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        pkg_dirs.sort_by_key(|e| e.file_name());

        for pkg_entry in pkg_dirs {
            let dir_name = pkg_entry.file_name();
            let dir_str = dir_name.to_string_lossy();

            // Reconstruct full package ID: "0x" + prefix (without "0x") + dir_name
            let prefix_hex = prefix_str.strip_prefix("0x").unwrap_or(&prefix_str);
            let package_id = format!("0x{prefix_hex}{dir_str}");

            if let Some(ref f) = filter {
                if !package_id.starts_with(f) {
                    continue;
                }
            }

            packages.push(PackageDir {
                package_id,
                path: pkg_entry.path(),
            });
        }
    }

    Ok(packages)
}

// ── Bytecode loading & model construction ───────────────────────────────────

fn load_modules(bytecode_dir: &Path) -> Result<Vec<CompiledModule>> {
    let mut entries: Vec<_> = fs::read_dir(bytecode_dir)
        .with_context(|| format!("cannot read bytecode dir: {}", bytecode_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "mv")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut modules = Vec::new();
    for entry in entries {
        let bytes = fs::read(entry.path())
            .with_context(|| format!("cannot read module: {}", entry.path().display()))?;
        let module = CompiledModule::deserialize_with_defaults(&bytes)
            .with_context(|| format!("cannot deserialize: {}", entry.path().display()))?;
        modules.push(module);
    }
    Ok(modules)
}

fn extract_call_graph(modules: Vec<CompiledModule>) -> Result<(PackageCallGraph, CallGraph)> {
    let address_map = BTreeMap::new();
    let config = ModelConfig {
        allow_missing_dependencies: true,
    };
    let model = Model::from_compiled_with_config(config, &address_map, modules);

    // Extract per-module call data using Function::calls() (includes cross-package).
    let mut module_graphs: Vec<ModuleCallGraph> = Vec::new();

    let mut package_id_str = None;

    for module in model.modules() {
        let module_id = module.id();
        let module_name = module.name().as_str().to_owned();

        // Capture the package address from the first module we see.
        if package_id_str.is_none() {
            package_id_str =
                Some(format!("0x{}", module_id.address.to_canonical_string(false)));
        }

        let mut call_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for function in module.functions() {
            let fn_name = function.name().as_str().to_owned();
            let mut callees: Vec<String> = function
                .calls()
                .iter()
                .map(|(callee_mod_id, callee_fn_name)| {
                    format!(
                        "0x{}::{}::{}",
                        callee_mod_id.address.to_canonical_string(false),
                        callee_mod_id.name.as_str(),
                        callee_fn_name.as_str(),
                    )
                })
                .collect();
            // Sort by string representation to match reference data ordering.
            callees.sort();
            call_map.insert(fn_name, callees);
        }

        module_graphs.push(ModuleCallGraph {
            module_name,
            call_graph: call_map,
        });
    }

    // Sort modules alphabetically.
    module_graphs.sort_by(|a, b| a.module_name.cmp(&b.module_name));

    let pkg_id = package_id_str.unwrap_or_default();

    // Secondary validation: construct CallGraph from model (should not panic).
    let call_graph = CallGraph::from_model(&model);

    Ok((
        PackageCallGraph {
            package_id: pkg_id,
            module_call_graphs: module_graphs,
        },
        call_graph,
    ))
}

// ── Comparison ──────────────────────────────────────────────────────────────

fn compare_call_graphs(
    expected: &PackageCallGraph,
    actual: &PackageCallGraph,
) -> Vec<String> {
    let mut diffs = Vec::new();

    if expected.package_id != actual.package_id {
        diffs.push(format!(
            "package_id mismatch: expected={}, actual={}",
            expected.package_id, actual.package_id
        ));
    }

    let expected_modules: BTreeMap<&str, &ModuleCallGraph> = expected
        .module_call_graphs
        .iter()
        .map(|m| (m.module_name.as_str(), m))
        .collect();
    let actual_modules: BTreeMap<&str, &ModuleCallGraph> = actual
        .module_call_graphs
        .iter()
        .map(|m| (m.module_name.as_str(), m))
        .collect();

    for name in expected_modules.keys() {
        if !actual_modules.contains_key(name) {
            diffs.push(format!("module '{name}' missing from actual"));
        }
    }
    for name in actual_modules.keys() {
        if !expected_modules.contains_key(name) {
            diffs.push(format!("module '{name}' unexpected in actual"));
        }
    }

    for (name, exp_mod) in &expected_modules {
        let Some(act_mod) = actual_modules.get(name) else {
            continue;
        };
        for (fn_name, exp_callees) in &exp_mod.call_graph {
            match act_mod.call_graph.get(fn_name) {
                None => {
                    diffs.push(format!("module '{name}': function '{fn_name}' missing from actual"));
                }
                Some(act_callees) => {
                    if exp_callees != act_callees {
                        diffs.push(format!(
                            "module '{name}': function '{fn_name}' callees differ:\n  expected: {exp_callees:?}\n  actual:   {act_callees:?}",
                        ));
                    }
                }
            }
        }
        for fn_name in act_mod.call_graph.keys() {
            if !exp_mod.call_graph.contains_key(fn_name) {
                diffs.push(format!(
                    "module '{name}': function '{fn_name}' unexpected in actual"
                ));
            }
        }
    }

    diffs
}

// ── Main ────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dataset_path = PathBuf::from(&cli.dataset_path);

    if !dataset_path.exists() {
        bail!(
            "dataset path does not exist: {}\nDownload the mainnet packages dataset first.",
            dataset_path.display()
        );
    }

    let packages = discover_packages(&dataset_path, cli.filter.as_deref())?;
    if packages.is_empty() {
        bail!("no packages found in {}", dataset_path.display());
    }

    eprintln!("Processing {} packages...", packages.len());

    let pass_count = AtomicUsize::new(0);
    let fail_count = AtomicUsize::new(0);
    let error_count = AtomicUsize::new(0);
    let generated_bytes = AtomicUsize::new(0);

    packages.par_iter().for_each(|pkg| {
        let result = process_package(pkg, cli.generate, cli.output_dir.as_deref());
        match result {
            Ok(ProcessResult::Pass) => {
                pass_count.fetch_add(1, Ordering::Relaxed);
            }
            Ok(ProcessResult::Fail(diffs)) => {
                fail_count.fetch_add(1, Ordering::Relaxed);
                eprintln!("FAIL {}: {} differences", pkg.package_id, diffs.len());
                for diff in &diffs {
                    eprintln!("  {diff}");
                }
            }
            Ok(ProcessResult::Generated(bytes)) => {
                pass_count.fetch_add(1, Ordering::Relaxed);
                generated_bytes.fetch_add(bytes, Ordering::Relaxed);
            }
            Ok(ProcessResult::NoReference) => {
                // No reference file — skip comparison.
                pass_count.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                error_count.fetch_add(1, Ordering::Relaxed);
                eprintln!("ERROR {}: {e:#}", pkg.package_id);
            }
        }
    });

    let pass = pass_count.load(Ordering::Relaxed);
    let fail = fail_count.load(Ordering::Relaxed);
    let errors = error_count.load(Ordering::Relaxed);
    let gen_bytes = generated_bytes.load(Ordering::Relaxed);

    eprintln!();
    eprintln!("Results: {pass} passed, {fail} failed, {errors} errors (total: {})", packages.len());
    if cli.generate {
        eprintln!("Generated bytes: {gen_bytes}");
    }

    if fail > 0 || errors > 0 {
        bail!("{fail} failures, {errors} errors");
    }

    Ok(())
}

enum ProcessResult {
    Pass,
    Fail(Vec<String>),
    Generated(usize),
    NoReference,
}

fn process_package(
    pkg: &PackageDir,
    generate: bool,
    output_dir: Option<&Path>,
) -> Result<ProcessResult> {
    let bytecode_dir = pkg.path.join("bytecode_modules");
    if !bytecode_dir.exists() {
        return Ok(ProcessResult::NoReference);
    }

    let modules = load_modules(&bytecode_dir)?;
    if modules.is_empty() {
        return Ok(ProcessResult::NoReference);
    }

    let (mut actual, _call_graph) = extract_call_graph(modules)?;

    // Override with the canonical package ID from directory structure
    // (the model may see a different address if deserialized modules use self-address 0x0).
    actual.package_id = pkg.package_id.clone();

    if generate {
        let json = serde_json::to_string_pretty(&actual)?;
        let bytes = json.len();
        let out_dir = output_dir.unwrap_or(&pkg.path);
        let out_path = out_dir.join("call_graph_generated.json");
        if output_dir.is_some() {
            fs::create_dir_all(out_dir)?;
        }
        fs::write(&out_path, &json)?;
        return Ok(ProcessResult::Generated(bytes));
    }

    let reference_path = pkg.path.join("call_graph.json");
    if !reference_path.exists() {
        return Ok(ProcessResult::NoReference);
    }

    let reference_json = fs::read_to_string(&reference_path)
        .with_context(|| format!("cannot read {}", reference_path.display()))?;
    let expected: PackageCallGraph = serde_json::from_str(&reference_json)
        .with_context(|| format!("cannot parse {}", reference_path.display()))?;

    let diffs = compare_call_graphs(&expected, &actual);
    if diffs.is_empty() {
        Ok(ProcessResult::Pass)
    } else {
        Ok(ProcessResult::Fail(diffs))
    }
}
