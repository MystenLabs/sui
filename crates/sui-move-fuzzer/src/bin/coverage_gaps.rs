// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Standalone corpus coverage analysis tool.
//!
//! Analyzes the seed corpus for each fuzz target, identifies which known
//! verifier paths have zero coverage, and prints a gap report.
//!
//! Usage:
//!   cargo run --no-default-features --bin coverage_gaps
//!   cargo run --no-default-features --bin coverage_gaps -- --corpus-dir corpus/verifier_crash
//!   cargo run --no-default-features --bin coverage_gaps -- --json

use std::path::PathBuf;

use sui_move_fuzzer::coverage_gaps::{analyze_corpus, identify_gaps};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut corpus_dirs: Vec<PathBuf> = Vec::new();
    let mut json_output = false;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--corpus-dir" if i + 1 < args.len() => {
                i += 1;
                corpus_dirs.push(PathBuf::from(&args[i]));
            }
            "--json" => json_output = true,
            other if !other.starts_with("--") => {
                corpus_dirs.push(PathBuf::from(other));
            }
            other => eprintln!("unknown flag: {other}"),
        }
        i += 1;
    }

    // Default: analyze all known corpus directories.
    if corpus_dirs.is_empty() {
        let known = [
            "corpus/verifier_crash",
            "corpus/ref_safety_diff",
            "corpus/ref_safety_diff_gen",
            "corpus/deser_roundtrip",
            "corpus/bounds_escape",
            "corpus/verifier_soundness",
        ];
        for dir in &known {
            let p = PathBuf::from(dir);
            if p.exists() {
                corpus_dirs.push(p);
            }
        }
        if corpus_dirs.is_empty() {
            eprintln!("No corpus directories found. Run gen_corpus first or pass --corpus-dir.");
            std::process::exit(1);
        }
    }

    // Merge reports from all specified directories.
    let mut merged = sui_move_fuzzer::coverage_gaps::CorpusCoverageReport::default();

    for dir in &corpus_dirs {
        if !dir.exists() {
            eprintln!("warning: corpus dir {} does not exist, skipping", dir.display());
            continue;
        }
        println!("Analyzing {}...", dir.display());
        let report = analyze_corpus(dir);
        merged.total_seeds += report.total_seeds;
        merged.bounds_passing += report.bounds_passing;
        merged.move_passing += report.move_passing;
        merged.sui_passing += report.sui_passing;
        for (k, v) in report.error_code_counts {
            *merged.error_code_counts.entry(k).or_insert(0) += v;
        }
        merged.sui_pass_hit.extend(report.sui_pass_hit);
    }

    let gaps = identify_gaps(&merged);

    if json_output {
        print_json_report(&merged, &gaps);
    } else {
        print_text_report(&merged, &gaps);
    }
}

fn print_text_report(
    report: &sui_move_fuzzer::coverage_gaps::CorpusCoverageReport,
    gaps: &[sui_move_fuzzer::coverage_gaps::CoverageGap],
) {
    println!();
    println!("=== Corpus Coverage Report ===");
    println!("  Total seeds analyzed : {}", report.total_seeds);
    println!("  Bounds-passing       : {}", report.bounds_passing);
    println!("  Move-verify passing  : {}", report.move_passing);
    println!("  Sui-verify passing   : {}", report.sui_passing);

    if !report.error_code_counts.is_empty() {
        println!();
        println!("Move verifier error breakdown:");
        let mut codes: Vec<_> = report.error_code_counts.iter().collect();
        codes.sort_by(|a, b| b.1.cmp(a.1));
        for (code, count) in codes.iter().take(20) {
            println!("  {:5}  {}", count, code);
        }
        if codes.len() > 20 {
            println!("  ... and {} more", codes.len() - 20);
        }
    }

    println!();
    if gaps.is_empty() {
        println!("All known verifier paths have corpus coverage.");
    } else {
        println!("Coverage gaps ({} uncovered paths):", gaps.len());
        for gap in gaps {
            println!();
            println!("  [GAP] {}", gap.summary());
            println!("        hint: {}", &gap.llm_hint[..gap.llm_hint.len().min(120)]);
        }
        println!();
        println!(
            "Run 'cargo run --features llm-guided --bin llm_seed_gen' to generate seeds for these gaps."
        );
    }
}

fn print_json_report(
    report: &sui_move_fuzzer::coverage_gaps::CorpusCoverageReport,
    gaps: &[sui_move_fuzzer::coverage_gaps::CoverageGap],
) {
    let gaps_json: Vec<serde_json::Value> = gaps
        .iter()
        .map(|g| {
            serde_json::json!({
                "id": g.path_id,
                "description": g.description,
                "pass": format!("{:?}", g.pass),
                "error_code": g.error_code.map(|c| format!("{:?}", c)),
                "hint": g.llm_hint,
                "source_file": g.source_context_file,
            })
        })
        .collect();

    let output = serde_json::json!({
        "total_seeds": report.total_seeds,
        "bounds_passing": report.bounds_passing,
        "move_passing": report.move_passing,
        "sui_passing": report.sui_passing,
        "error_code_counts": report.error_code_counts,
        "coverage_gaps": gaps_json,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
