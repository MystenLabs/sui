// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Continuous autonomous fuzz loop for the Sui Move VM fuzzer.
//!
//! Combines coverage gap analysis, LLM-guided seed generation, and fuzzer
//! execution into a single long-running process that runs unattended for
//! 5-10 hours, finding real verifier crashes/bypasses.
//!
//! Performance design:
//!   - Fuzzer uses `-jobs=N -workers=N` (all available cores).
//!   - LLM calls for all gaps fire in parallel (one thread per gap).
//!   - Fuzzer subprocess and LLM generation run concurrently; seeds land in
//!     the corpus while the fuzzer is already running, ready for the next round.
//!
//! Usage:
//!   # Full autonomous run (10 hours):
//!   OPENROUTER_API_KEY=sk-or-... cargo run --features llm-guided --bin fuzz_loop
//!
//!   # Dry test: no LLM, 2-minute global timeout, 60s fuzz window:
//!   cargo run --features llm-guided --bin fuzz_loop -- --no-llm --timeout 120 --fuzz-time 60
//!
//!   # Custom parallelism:
//!   OPENROUTER_API_KEY=sk-or-... cargo run --features llm-guided --bin fuzz_loop -- \
//!       --jobs 8 --timeout 36000 --fuzz-time 600 --seeds-per-round 5 --keep-going

use std::collections::HashSet;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::Instant;

use arbitrary::{Arbitrary, Unstructured};
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format::CompiledModule;
use move_bytecode_verifier::verify_module_with_config_metered;
use move_bytecode_verifier_meter::dummy::DummyMeter;
use sui_move_fuzzer::coverage_gaps::{CoverageGap, VerifierPass, analyze_corpus, identify_gaps};
use sui_move_fuzzer::llm_client::call_llm_api;
use sui_move_fuzzer::llm_prompts::{build_retry_prompt, build_seed_gen_prompt, extract_json};
use sui_move_fuzzer::module_gen::{ModuleBuilder, ModuleGenConfig};
use sui_move_fuzzer::module_spec::{ModuleSpec, ModuleSpecBuilder};
use sui_move_fuzzer::mutators::{MutationKind, apply_mutation};
use sui_move_fuzzer::sui_harness;

// ─── CLI ─────────────────────────────────────────────────────────────────────

struct Config {
    target: String,
    timeout: u64,
    fuzz_time: u64,
    seeds_per_round: usize,
    max_attempts: usize,
    /// Parallel fuzzer workers (`-jobs=N -workers=N` passed to libfuzzer).
    jobs: usize,
    model: String,
    api_url: String,
    source_root: PathBuf,
    keep_going: bool,
    no_llm: bool,
}

impl Default for Config {
    fn default() -> Self {
        // Leave one core free for the fuzz_loop process itself.
        let parallelism = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .saturating_sub(1)
            .max(1);
        Self {
            target: "verifier_crash".to_string(),
            timeout: 36000,
            fuzz_time: 600,
            seeds_per_round: 3,
            max_attempts: 3,
            jobs: parallelism,
            model: "anthropic/claude-opus-4-6".to_string(),
            api_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            source_root: PathBuf::from("../../"),
            keep_going: false,
            no_llm: false,
        }
    }
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut cfg = Config::default();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--target" if i + 1 < args.len() => {
                i += 1;
                cfg.target = args[i].clone();
            }
            "--timeout" if i + 1 < args.len() => {
                i += 1;
                cfg.timeout = args[i].parse().expect("timeout must be a number");
            }
            "--fuzz-time" if i + 1 < args.len() => {
                i += 1;
                cfg.fuzz_time = args[i].parse().expect("fuzz-time must be a number");
            }
            "--seeds-per-round" if i + 1 < args.len() => {
                i += 1;
                cfg.seeds_per_round = args[i].parse().expect("seeds-per-round must be a number");
            }
            "--max-attempts" if i + 1 < args.len() => {
                i += 1;
                cfg.max_attempts = args[i].parse().expect("max-attempts must be a number");
            }
            "--jobs" if i + 1 < args.len() => {
                i += 1;
                cfg.jobs = args[i].parse().expect("jobs must be a number");
            }
            "--model" if i + 1 < args.len() => {
                i += 1;
                cfg.model = args[i].clone();
            }
            "--api-url" if i + 1 < args.len() => {
                i += 1;
                cfg.api_url = args[i].clone();
            }
            "--source-root" if i + 1 < args.len() => {
                i += 1;
                cfg.source_root = PathBuf::from(&args[i]);
            }
            "--keep-going" => cfg.keep_going = true,
            "--no-llm" => cfg.no_llm = true,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => eprintln!("unknown flag: {other}"),
        }
        i += 1;
    }

    cfg
}

fn print_usage() {
    println!(
        "Usage: fuzz_loop [OPTIONS]

  --target <NAME>         Fuzz target name (default: verifier_crash)
  --timeout <SECS>        Global timeout in seconds (default: 36000 = 10 hours)
  --fuzz-time <SECS>      Fuzzer time per round in seconds (default: 600 = 10 min)
  --seeds-per-round <N>   Max gaps to target per LLM round (default: 3)
  --max-attempts <N>      Max LLM retries per gap (default: 3)
  --jobs <N>              Parallel fuzzer workers (default: num_cpus - 1)
  --model <MODEL>         LLM model (default: anthropic/claude-opus-4-6)
  --api-url <URL>         API endpoint (default: OpenRouter)
  --source-root <PATH>    Repo root for verifier source context (default: ../../)
  --keep-going            Don't stop on first crash, keep fuzzing
  --no-llm                Skip LLM seed generation, just loop the fuzzer

Environment:
  OPENROUTER_API_KEY      API key for LLM calls (required unless --no-llm)
"
    );
}

// ─── Fuzz input mirror (matches verifier_crash fuzz target's Input struct) ────

/// Mirrors the `Input` struct in fuzz_targets/verifier_crash.rs so crash
/// artifacts can be reproduced for validation without spawning a subprocess.
#[derive(Arbitrary)]
struct FuzzInput {
    config: ModuleGenConfig,
    mutation: Option<MutationKind>,
    raw_entropy: Vec<u8>,
}

// ─── LLM seed generation ──────────────────────────────────────────────────────

struct SeedGenResult {
    api_calls: usize,
    saved: usize,
    /// Per-gap messages collected during the parallel run, printed after joining.
    log: Vec<String>,
}

/// Attempt to generate a seed for a single coverage gap (sequential retry loop).
/// Returns a `SeedGenResult` with api_calls, saved count, and log lines.
fn attempt_gap(
    api_key: &str,
    model: &str,
    api_url: &str,
    source_root: &Path,
    max_attempts: usize,
    gap: &CoverageGap,
    corpus_dir: &Path,
) -> SeedGenResult {
    let mut api_calls = 0usize;
    let mut saved = 0usize;
    let mut log = Vec::new();
    let source_context = load_source_context(source_root, gap.source_context_file);
    let mut last_spec_json = String::new();
    let mut last_error = String::new();

    for attempt in 1..=max_attempts {
        let prompt = if attempt == 1 {
            build_seed_gen_prompt(gap, &source_context)
        } else {
            build_retry_prompt(gap, &last_spec_json, &last_error)
        };

        api_calls += 1;
        let response = match call_llm_api(api_key, model, api_url, &prompt) {
            Ok(r) => r,
            Err(e) => {
                last_error = format!("API call failed: {e}");
                log.push(format!("  [{}] attempt {attempt}: API error: {e}", gap.path_id));
                continue;
            }
        };

        let spec_json = match extract_json(&response) {
            Some(j) => j.to_string(),
            None => {
                last_error = "Response contained no ```json block".to_string();
                last_spec_json = response[..response.len().min(200)].to_string();
                log.push(format!("  [{}] attempt {attempt}: no JSON in response", gap.path_id));
                continue;
            }
        };
        last_spec_json = spec_json.clone();

        let spec: ModuleSpec = match serde_json::from_str(&spec_json) {
            Ok(s) => s,
            Err(e) => {
                last_error = format!("JSON parse error: {e}");
                log.push(format!("  [{}] attempt {attempt}: JSON error: {e}", gap.path_id));
                continue;
            }
        };

        let module = match ModuleSpecBuilder::build(&spec) {
            Ok(m) => m,
            Err(e) => {
                last_error = format!("ModuleSpec builder error: {e}");
                log.push(format!("  [{}] attempt {attempt}: build error: {e}", gap.path_id));
                continue;
            }
        };

        let mut bytes = Vec::new();
        if let Err(e) = module.serialize(&mut bytes) {
            last_error = format!("Serialization error: {e:?}");
            log.push(format!("  [{}] attempt {attempt}: serialize error: {e:?}", gap.path_id));
            continue;
        }

        let config = BinaryConfig::standard();
        if let Err(e) = CompiledModule::deserialize_with_config(&bytes, &config) {
            last_error = format!("Bounds check failed: {e:?}");
            log.push(format!(
                "  [{}] attempt {attempt}: bounds check failed: {e:?}",
                gap.path_id
            ));
            continue;
        }

        let verify_result = sui_harness::run_full_verification(&module);
        if !check_reaches_target(&verify_result, gap) {
            let why = match &verify_result {
                Ok(()) => "module passed full verification".to_string(),
                Err(e) => format!("wrong error: {}", &e[..e.len().min(80)]),
            };
            last_error = format!("does not reach target gap — {why}");
            last_spec_json = spec_json;
            log.push(format!("  [{}] attempt {attempt}: ✗ {}", gap.path_id, last_error));
            continue;
        }

        let filename = format!("llm_{}_{:04}.bin", gap.path_id, saved);
        let out_path = corpus_dir.join(&filename);
        if let Err(e) = fs::write(&out_path, &bytes) {
            log.push(format!(
                "  [{}] attempt {attempt}: failed to write seed: {e}",
                gap.path_id
            ));
        } else {
            log.push(format!(
                "  [{}] ✓ saved {} ({attempt} attempt(s))",
                gap.path_id, filename
            ));
            saved += 1;
        }
        break;
    }

    if saved == 0 && log.last().map(|l| !l.contains('✓')).unwrap_or(true) {
        log.push(format!(
            "  [{}] gave up after {max_attempts} attempt(s): {}",
            gap.path_id, last_error
        ));
    }

    SeedGenResult { api_calls, saved, log }
}

/// Fire one thread per gap (up to `seeds_per_round`), collect and return
/// aggregated results. Returns immediately when all threads finish.
fn generate_seeds_parallel(
    api_key: &str,
    model: &str,
    api_url: &str,
    source_root: &Path,
    max_attempts: usize,
    seeds_per_round: usize,
    gaps: &[CoverageGap],
    corpus_dir: &Path,
) -> SeedGenResult {
    let handles: Vec<_> = gaps
        .iter()
        .take(seeds_per_round)
        .map(|gap| {
            // Clone all data needed by the thread.
            let api_key = api_key.to_string();
            let model = model.to_string();
            let api_url = api_url.to_string();
            let source_root = source_root.to_path_buf();
            let corpus_dir = corpus_dir.to_path_buf();
            let gap = gap.clone();

            thread::spawn(move || {
                attempt_gap(
                    &api_key,
                    &model,
                    &api_url,
                    &source_root,
                    max_attempts,
                    &gap,
                    &corpus_dir,
                )
            })
        })
        .collect();

    let mut total = SeedGenResult { api_calls: 0, saved: 0, log: Vec::new() };
    for h in handles {
        match h.join() {
            Ok(r) => {
                total.api_calls += r.api_calls;
                total.saved += r.saved;
                total.log.extend(r.log);
            }
            Err(_) => total.log.push("  [LLM] thread panicked".to_string()),
        }
    }
    total
}

// ─── Fuzzer subprocess ────────────────────────────────────────────────────────

/// Spawn `cargo +nightly fuzz run` and return the child process handle
/// without waiting. Uses `-jobs=N -workers=N` for parallel execution and
/// includes the dictionary for richer mutations.
fn spawn_fuzzer(crate_dir: &Path, target: &str, fuzz_time: u64, jobs: usize) -> Option<Child> {
    let corpus = format!("corpus/{target}");
    let artifact_prefix = format!("-artifact_prefix=artifacts/{target}/");
    let max_time = format!("-max_total_time={fuzz_time}");
    let jobs_flag = format!("-jobs={jobs}");
    let workers_flag = format!("-workers={jobs}");
    let dict = format!("-dict={}", crate_dir.join("dictionaries/move_bytecode.dict").display());

    match Command::new("cargo")
        .args([
            "+nightly",
            "fuzz",
            "run",
            "--fuzz-dir",
            crate_dir.to_str().unwrap_or("."),
            "--no-default-features",
            target,
            &corpus,
            "--",
            &artifact_prefix,
            "-print_final_stats=1",
            &max_time,
            &jobs_flag,
            &workers_flag,
            &dict,
        ])
        .current_dir(crate_dir)
        .spawn()
    {
        Ok(child) => Some(child),
        Err(e) => {
            eprintln!("  [Fuzzer] Failed to spawn subprocess: {e}");
            None
        }
    }
}

// ─── Crash scanning and validation ───────────────────────────────────────────

/// List all crash artifact files in `dir` (files whose names start with `crash-`).
fn list_crash_artifacts(dir: &Path) -> HashSet<PathBuf> {
    let mut files = HashSet::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && (name.starts_with("crash-") || name.starts_with("leak-"))
            {
                files.insert(path);
            }
        }
    }
    files
}

/// Try to reproduce a crash artifact by mirroring the fuzz target's logic.
///
/// Returns the artifact file name on confirmed panic, `None` if the crash
/// does not reproduce (e.g., a fuzzer metadata artifact or a fluke).
fn validate_artifact(artifact_path: &Path) -> Option<String> {
    let bytes = match fs::read(artifact_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("    Could not read artifact {}: {e}", artifact_path.display());
            return None;
        }
    };

    // Decode using the same Arbitrary layout as the fuzz target's Input struct.
    let mut u = Unstructured::new(&bytes);
    let input = match FuzzInput::arbitrary(&mut u) {
        Ok(i) => i,
        Err(_) => return None,
    };

    let mut u2 = Unstructured::new(&input.raw_entropy);
    let builder = ModuleBuilder::new(input.config);
    let mut module = match builder.build(&mut u2) {
        Ok(m) => m,
        Err(_) => return None,
    };

    if input.mutation.is_some() {
        let mut u3 = Unstructured::new(&input.raw_entropy);
        let _ = apply_mutation(&mut u3, &mut module);
    }

    // Run the same pipeline as the fuzz target under catch_unwind.
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        sui_harness::run_full_verification(&module).ok();

        let config = sui_harness::sui_verifier_config();
        let move_ok =
            verify_module_with_config_metered(&config, &module, &mut DummyMeter).is_ok();

        if move_ok {
            let results = sui_harness::run_sui_passes_individually(&module);
            for (pass_name, result) in results {
                if let Err(ref err_msg) = result
                    && err_msg.contains("CRASH")
                {
                    panic!("Sui verifier crash in {}: {}", pass_name, err_msg);
                }
            }
        }
    }))
    .is_err();

    if panicked {
        let name = artifact_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        Some(name)
    } else {
        None
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn count_files(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}

fn load_source_context(source_root: &Path, relative_path: &str) -> String {
    let full_path = source_root.join(relative_path);
    match fs::read_to_string(&full_path) {
        Ok(content) => {
            if content.len() > 8000 {
                format!("{}... (truncated)", &content[..8000])
            } else {
                content
            }
        }
        Err(_) => String::new(),
    }
}

fn check_reaches_target(verify_result: &Result<(), String>, gap: &CoverageGap) -> bool {
    match verify_result {
        Ok(()) => false,
        Err(e) => {
            if let Some(code) = gap.error_code {
                e.contains(&format!("{:?}", code))
            } else {
                let pass_name = match gap.pass {
                    VerifierPass::SuiStructWithKey => "struct_with_key",
                    VerifierPass::SuiIdLeak => "id_leak",
                    VerifierPass::SuiEntryPoints => "entry_points",
                    _ => "",
                };
                !pass_name.is_empty() && e.contains(pass_name)
            }
        }
    }
}

/// Ensure the corpus directory exists, running gen_corpus if needed.
fn ensure_corpus(crate_dir: &Path, target: &str) {
    let corpus_dir = crate_dir.join(format!("corpus/{target}"));
    if !corpus_dir.exists() {
        println!("[*] Corpus missing — running gen_corpus...");
        let status = Command::new("cargo")
            .args(["run", "--bin", "gen_corpus"])
            .current_dir(crate_dir)
            .status();
        match status {
            Ok(s) if s.success() => println!("[*] Corpus generated."),
            Ok(s) => eprintln!("[!] gen_corpus exited with: {s}"),
            Err(e) => eprintln!("[!] Failed to run gen_corpus: {e}"),
        }
    }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let cfg = parse_args();

    let api_key: Option<String> = if cfg.no_llm {
        None
    } else {
        std::env::var("OPENROUTER_API_KEY").ok()
    };

    if !cfg.no_llm && api_key.is_none() {
        eprintln!(
            "[!] OPENROUTER_API_KEY not set — LLM seed generation disabled.\n    \
             Set the variable or pass --no-llm to suppress this warning."
        );
    }

    let crate_dir = std::env::current_dir().expect("could not determine working directory");
    let corpus_dir = crate_dir.join(format!("corpus/{}", cfg.target));
    let artifact_dir = crate_dir.join(format!("artifacts/{}", cfg.target));

    ensure_corpus(&crate_dir, &cfg.target);
    fs::create_dir_all(&artifact_dir).expect("create artifact dir");

    let sep = "=".repeat(70);
    println!("\n{sep}");
    println!("  sui-move-fuzzer :: continuous fuzz loop");
    println!("  Target    : {}", cfg.target);
    println!(
        "  Timeout   : {}s ({:.1}h)",
        cfg.timeout,
        cfg.timeout as f64 / 3600.0
    );
    println!("  Fuzz/round: {}s  |  Workers: {}", cfg.fuzz_time, cfg.jobs);
    match &api_key {
        Some(key) => println!(
            "  LLM       : {} (key ...{})",
            cfg.model,
            &key[key.len().saturating_sub(4)..]
        ),
        None => println!("  LLM       : disabled"),
    }
    println!("{sep}\n");

    let start = Instant::now();
    let mut round = 0usize;
    let mut total_api_calls = 0usize;
    let mut total_seeds_generated = 0usize;
    let mut crashes_found: Vec<String> = Vec::new();
    let mut known_artifacts = list_crash_artifacts(&artifact_dir);

    loop {
        round += 1;
        let elapsed = start.elapsed().as_secs();
        if elapsed >= cfg.timeout {
            println!("\n[*] Global timeout reached after {} round(s).", round - 1);
            break;
        }
        let remaining = cfg.timeout - elapsed;

        println!(
            "{}\nRound {} | Elapsed: {}m {}s | Remaining: {}m",
            "-".repeat(60),
            round,
            elapsed / 60,
            elapsed % 60,
            remaining / 60,
        );

        // Phase 1: Gap analysis.
        let report = analyze_corpus(&corpus_dir);
        let gaps = identify_gaps(&report);
        println!(
            "[1] Corpus: {} seeds  |  bounds-ok: {}  |  gaps: {}",
            report.total_seeds, report.bounds_passing, gaps.len()
        );

        // Phase 2+3: Start fuzzer and LLM generation concurrently.
        //
        // The fuzzer runs for fuzz_time seconds. LLM calls finish much sooner
        // (~10-60s) and write seeds to corpus/ while the fuzzer is still running.
        // Those seeds are available to the NEXT fuzzer round.
        let fuzz_time = remaining.min(cfg.fuzz_time);
        let corpus_before = count_files(&corpus_dir);

        // Spawn the fuzzer subprocess (non-blocking).
        println!(
            "[2+3] Fuzzer ({fuzz_time}s, {} workers) + LLM generation starting in parallel...",
            cfg.jobs
        );
        let mut fuzzer_child = spawn_fuzzer(&crate_dir, &cfg.target, fuzz_time, cfg.jobs);

        // Fire LLM calls in a background thread while the fuzzer runs.
        let llm_handle: Option<thread::JoinHandle<SeedGenResult>> =
            if let Some(ref key) = api_key
                && !gaps.is_empty()
            {
                let key = key.clone();
                let model = cfg.model.clone();
                let api_url = cfg.api_url.clone();
                let source_root = cfg.source_root.clone();
                let max_attempts = cfg.max_attempts;
                let seeds_per_round = cfg.seeds_per_round;
                let corpus_dir = corpus_dir.clone();
                let gaps: Vec<CoverageGap> = gaps.clone();

                Some(thread::spawn(move || {
                    generate_seeds_parallel(
                        &key,
                        &model,
                        &api_url,
                        &source_root,
                        max_attempts,
                        seeds_per_round,
                        &gaps,
                        &corpus_dir,
                    )
                }))
            } else {
                None
            };

        // Wait for fuzzer to finish (it exits after fuzz_time seconds).
        if let Some(ref mut child) = fuzzer_child {
            match child.wait() {
                Ok(s) if !s.success() => eprintln!("  [Fuzzer] exited with status: {s}"),
                Err(e) => eprintln!("  [Fuzzer] wait error: {e}"),
                _ => {}
            }
        }

        let corpus_after = count_files(&corpus_dir);
        println!(
            "    Corpus: {} → {} (+{} new entries)",
            corpus_before,
            corpus_after,
            corpus_after.saturating_sub(corpus_before)
        );

        // Collect LLM results (thread has likely already finished by now).
        match llm_handle {
            Some(h) => match h.join() {
                Ok(result) => {
                    for line in &result.log {
                        println!("{line}");
                    }
                    println!(
                        "    LLM: {} API call(s)  |  {} seed(s) saved",
                        result.api_calls, result.saved
                    );
                    total_api_calls += result.api_calls;
                    total_seeds_generated += result.saved;
                }
                Err(_) => eprintln!("    LLM thread panicked"),
            },
            None => println!("    LLM: disabled or no gaps."),
        }

        // Phase 4: Crash scan.
        let current_artifacts = list_crash_artifacts(&artifact_dir);
        let new_artifacts: Vec<PathBuf> = current_artifacts
            .difference(&known_artifacts)
            .cloned()
            .collect();
        known_artifacts = current_artifacts;

        if new_artifacts.is_empty() {
            println!("[4] No new crash artifacts.");
        } else {
            println!("[4] Checking {} new artifact(s)...", new_artifacts.len());
            for artifact_path in &new_artifacts {
                print!("    {} ... ", artifact_path.display());
                if let Some(name) = validate_artifact(artifact_path) {
                    println!("*** CONFIRMED CRASH: {name} ***");
                    crashes_found.push(format!("{name} ({})", artifact_path.display()));
                    if !cfg.keep_going {
                        break;
                    }
                } else {
                    println!("not reproduced (fuzzer artifact, not a verifier panic)");
                }
            }
        }

        if !crashes_found.is_empty() && !cfg.keep_going {
            println!("\n[!] Stopping on confirmed crash (pass --keep-going to continue).");
            break;
        }

        if start.elapsed().as_secs() >= cfg.timeout {
            println!("\n[*] Global timeout reached.");
            break;
        }
    }

    // Final summary.
    let elapsed = start.elapsed().as_secs();
    let sep = "=".repeat(70);
    println!("\n{sep}");
    println!("  Final Summary");
    println!("  Rounds completed  : {}", round);
    println!("  Elapsed           : {}m {}s", elapsed / 60, elapsed % 60);
    println!("  Total API calls   : {}", total_api_calls);
    println!("  Seeds generated   : {}", total_seeds_generated);
    println!("  Crashes confirmed : {}", crashes_found.len());
    for crash in &crashes_found {
        println!("    * {crash}");
    }
    println!("{sep}");

    if !crashes_found.is_empty() {
        std::process::exit(1);
    }
}
