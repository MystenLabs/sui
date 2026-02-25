// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! LLM-guided seed generation for the Sui Move VM fuzzer.
//!
//! Analyzes the corpus to find uncovered verifier paths, then uses an LLM
//! (via OpenRouter or the Anthropic API) to generate `ModuleSpec` JSON that
//! exercises those paths. Successfully built modules are saved to the corpus.
//!
//! Usage (OpenRouter — default):
//!   OPENROUTER_API_KEY=sk-or-... cargo run --features llm-guided --bin llm_seed_gen
//!   cargo run --features llm-guided --bin llm_seed_gen -- --dry-run
//!   cargo run --features llm-guided --bin llm_seed_gen -- \
//!       --model anthropic/claude-opus-4 \
//!       --corpus-dir corpus/verifier_crash \
//!       --output-dir corpus/llm_generated \
//!       --max-targets 3 --max-attempts 5
//!
//! Usage (Anthropic direct):
//!   OPENROUTER_API_KEY=sk-ant-... cargo run --features llm-guided --bin llm_seed_gen -- \
//!       --api-url https://api.anthropic.com/v1/messages \
//!       --model claude-sonnet-4-5-20251219

use std::fs;
use std::path::{Path, PathBuf};

use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format::CompiledModule;
use sui_move_fuzzer::coverage_gaps::{analyze_corpus, identify_gaps};
use sui_move_fuzzer::llm_client::call_llm_api;
use sui_move_fuzzer::llm_prompts::{build_retry_prompt, build_seed_gen_prompt, extract_json};
use sui_move_fuzzer::module_spec::{ModuleSpec, ModuleSpecBuilder};
use sui_move_fuzzer::sui_harness;

// ─── CLI ─────────────────────────────────────────────────────────────────────

struct Config {
    corpus_dirs: Vec<PathBuf>,
    output_dir: PathBuf,
    max_targets: usize,
    max_attempts: usize,
    dry_run: bool,
    model: String,
    api_url: String,
    source_root: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            corpus_dirs: vec![
                PathBuf::from("corpus/verifier_crash"),
                PathBuf::from("corpus/ref_safety_diff"),
            ],
            output_dir: PathBuf::from("corpus/llm_generated"),
            max_targets: 5,
            max_attempts: 3,
            dry_run: false,
            // OpenRouter model name. Opus is best for complex bytecode reasoning.
            // Alternatives: "anthropic/claude-sonnet-4-5", "anthropic/claude-opus-4"
            model: "anthropic/claude-opus-4-6".to_string(),
            api_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            source_root: PathBuf::from("../../"),
        }
    }
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().collect();
    let mut cfg = Config::default();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--corpus-dir" if i + 1 < args.len() => {
                i += 1;
                if cfg.corpus_dirs == Config::default().corpus_dirs {
                    cfg.corpus_dirs.clear();
                }
                cfg.corpus_dirs.push(PathBuf::from(&args[i]));
            }
            "--output-dir" if i + 1 < args.len() => {
                i += 1;
                cfg.output_dir = PathBuf::from(&args[i]);
            }
            "--max-targets" if i + 1 < args.len() => {
                i += 1;
                cfg.max_targets = args[i].parse().expect("max-targets must be a number");
            }
            "--max-attempts" if i + 1 < args.len() => {
                i += 1;
                cfg.max_attempts = args[i].parse().expect("max-attempts must be a number");
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
            "--dry-run" => {
                cfg.dry_run = true;
            }
            other => eprintln!("unknown flag: {other}"),
        }
        i += 1;
    }

    cfg
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let cfg = parse_args();

    let api_key = if cfg.dry_run {
        String::new()
    } else {
        std::env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
            eprintln!("error: OPENROUTER_API_KEY environment variable not set");
            eprintln!("       Use --dry-run to print prompts without calling the API");
            std::process::exit(1);
        })
    };

    // ── Step 1: Analyze corpus for gaps ────────────────────────────────────
    println!("[*] Analyzing corpus for coverage gaps...");
    let mut merged = sui_move_fuzzer::coverage_gaps::CorpusCoverageReport::default();
    for dir in &cfg.corpus_dirs {
        if dir.exists() {
            let r = analyze_corpus(dir);
            merged.total_seeds += r.total_seeds;
            merged.bounds_passing += r.bounds_passing;
            merged.move_passing += r.move_passing;
            merged.sui_passing += r.sui_passing;
            for (k, v) in r.error_code_counts {
                *merged.error_code_counts.entry(k).or_insert(0) += v;
            }
            merged.sui_pass_hit.extend(r.sui_pass_hit);
        }
    }
    println!(
        "    {}/{} seeds pass bounds check, {}/{} pass Move verify",
        merged.bounds_passing, merged.total_seeds, merged.move_passing, merged.total_seeds
    );

    let gaps = identify_gaps(&merged);
    if gaps.is_empty() {
        println!("[*] No coverage gaps found — corpus already covers all known paths.");
        return;
    }
    println!("[*] Found {} coverage gap(s):", gaps.len());
    for g in &gaps {
        println!("    {}", g.summary());
    }

    // ── Step 2: Prepare output directory ───────────────────────────────────
    if !cfg.dry_run {
        fs::create_dir_all(&cfg.output_dir).expect("create output dir");
    }

    // ── Step 3: For each gap, ask the LLM to generate a seed ───────────────
    let targets = gaps.iter().take(cfg.max_targets);
    let mut total_generated = 0usize;
    let mut total_attempts = 0usize;

    for gap in targets {
        println!("\n[*] Targeting gap: {}", gap.path_id);

        // Load relevant source context (best-effort; skip on read failure).
        let source_context = load_source_context(&cfg.source_root, gap.source_context_file);

        let mut last_spec_json = String::new();
        let mut last_error = String::new();

        for attempt in 1..=cfg.max_attempts {
            total_attempts += 1;
            println!("    Attempt {}/{}", attempt, cfg.max_attempts);

            // Build prompt.
            let prompt = if attempt == 1 {
                build_seed_gen_prompt(gap, &source_context)
            } else {
                build_retry_prompt(gap, &last_spec_json, &last_error)
            };

            if cfg.dry_run {
                println!("    [DRY RUN] Prompt ({} chars):", prompt.len());
                println!("    {}", &prompt[..prompt.len().min(500)]);
                println!("    ...(truncated)");
                break;
            }

            // Call LLM API.
            let response = match call_llm_api(&api_key, &cfg.model, &cfg.api_url, &prompt) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("    API error: {e}");
                    last_error = format!("API call failed: {e}");
                    continue;
                }
            };

            // Extract JSON from response.
            let spec_json = match extract_json(&response) {
                Some(j) => j.to_string(),
                None => {
                    eprintln!("    No JSON found in response");
                    last_error = "Response contained no ```json block".to_string();
                    last_spec_json = response[..response.len().min(200)].to_string();
                    continue;
                }
            };
            last_spec_json = spec_json.clone();

            // Parse ModuleSpec.
            let spec: ModuleSpec = match serde_json::from_str(&spec_json) {
                Ok(s) => s,
                Err(e) => {
                    last_error = format!("JSON parse error: {e}");
                    eprintln!("    JSON parse error: {e}");
                    continue;
                }
            };

            // Build CompiledModule.
            let module = match ModuleSpecBuilder::build(&spec) {
                Ok(m) => m,
                Err(e) => {
                    last_error = format!("ModuleSpec builder error: {e}");
                    eprintln!("    Build error: {e}");
                    continue;
                }
            };

            // Validate: bounds check.
            let mut bytes = Vec::new();
            if let Err(e) = module.serialize(&mut bytes) {
                last_error = format!("Serialization error: {e:?}");
                eprintln!("    Serialization error: {e:?}");
                continue;
            }
            let config = BinaryConfig::standard();
            if let Err(e) = CompiledModule::deserialize_with_config(&bytes, &config) {
                last_error = format!("Bounds check failed: {e:?}");
                eprintln!("    Bounds check failed: {e:?}");
                continue;
            }

            // Only save if the module actually reaches the target verifier path.
            let verify_result = sui_harness::run_full_verification(&module);
            if !check_reaches_target(&verify_result, gap) {
                let why = match &verify_result {
                    Ok(()) => "module passed full verification (target expects a rejection)".to_string(),
                    Err(e) => format!("wrong error: {}", &e[..e.len().min(80)]),
                };
                last_error = format!("does not reach target gap — {why}");
                eprintln!("    ✗ {}", last_error);
                last_spec_json = spec_json.clone();
                continue;
            }

            println!("    ✓ Module reaches target gap!");
            let filename = format!("llm_{}_{:04}.bin", gap.path_id, total_generated);
            let out_path = cfg.output_dir.join(&filename);
            fs::write(&out_path, &bytes).expect("write seed");
            println!("    Saved: {}", out_path.display());
            total_generated += 1;
            break;
        }
    }

    // ── Summary ────────────────────────────────────────────────────────────
    println!("\n[*] Summary:");
    println!("    Gaps targeted    : {}", cfg.max_targets.min(gaps.len()));
    println!("    Total API calls  : {}", total_attempts);
    println!("    Seeds generated  : {}", total_generated);
    if total_generated > 0 {
        println!("    Output directory : {}", cfg.output_dir.display());
        println!();
        println!("    Next: run the fuzzer with the new seeds:");
        println!(
            "      ./run_fuzzer.sh verifier_crash -max_total_time=3600"
        );
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Load a source file for context injection. Returns a truncated snippet
/// on success or an empty string if the file can't be read.
fn load_source_context(source_root: &Path, relative_path: &str) -> String {
    let full_path = source_root.join(relative_path);
    match fs::read_to_string(&full_path) {
        Ok(content) => {
            // Truncate to ~8000 chars to stay within prompt budget.
            if content.len() > 8000 {
                format!("{}... (truncated)", &content[..8000])
            } else {
                content
            }
        }
        Err(_) => {
            eprintln!(
                "    note: could not load source context from {}",
                full_path.display()
            );
            String::new()
        }
    }
}

/// Check whether the verification result indicates the module reaches the
/// target gap (either by producing the expected error or passing through to
/// the relevant pass).
fn check_reaches_target(
    verify_result: &Result<(), String>,
    gap: &sui_move_fuzzer::coverage_gaps::CoverageGap,
) -> bool {
    match verify_result {
        Ok(()) => {
            // Module passed full verification — check if the gap is a "should accept" path.
            false
        }
        Err(e) => {
            // Check if the error matches the target error code.
            if let Some(code) = gap.error_code {
                e.contains(&format!("{:?}", code))
            } else {
                // For Sui-pass gaps without a specific code, check if the error
                // mentions the right pass.
                let pass_name = match gap.pass {
                    sui_move_fuzzer::coverage_gaps::VerifierPass::SuiStructWithKey => {
                        "struct_with_key"
                    }
                    sui_move_fuzzer::coverage_gaps::VerifierPass::SuiIdLeak => "id_leak",
                    sui_move_fuzzer::coverage_gaps::VerifierPass::SuiEntryPoints => "entry_points",
                    _ => "",
                };
                !pass_name.is_empty() && e.contains(pass_name)
            }
        }
    }
}
