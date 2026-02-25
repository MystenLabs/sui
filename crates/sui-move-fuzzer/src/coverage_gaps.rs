// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Corpus coverage analysis for the Sui Move VM fuzzer.
//!
//! Runs the existing seed corpus through each verifier pass and identifies
//! which known verifier paths have never been exercised. These "gaps" are
//! targets for LLM-guided seed generation.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use move_binary_format::binary_config::BinaryConfig;
use move_core_types::vm_status::StatusCode;
use move_binary_format::file_format::CompiledModule;
use move_bytecode_verifier::verify_module_with_config_metered;
use move_bytecode_verifier_meter::dummy::DummyMeter;

use crate::sui_harness;

// ─── Known verifier paths ─────────────────────────────────────────────────────

/// A specific verifier path we want corpus coverage for.
#[derive(Debug, Clone)]
pub struct KnownPath {
    /// Short identifier used as a key.
    pub id: &'static str,
    /// Human-readable description of the path.
    pub description: &'static str,
    /// The verifier pass where this path lives.
    pub pass: VerifierPass,
    /// The `StatusCode` the path emits on failure (if it's a rejection path).
    pub error_code: Option<StatusCode>,
    /// Guidance for the LLM: what bytecode pattern reaches this path.
    pub llm_hint: &'static str,
    /// Relevant source file for context injection.
    pub source_context: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VerifierPass {
    BoundsChecker,
    SignatureChecker,
    TypeSafety,
    ReferenceSafety,
    LocalsSafety,
    InstantiationLoops,
    CodeUnitVerifier,
    SuiStructWithKey,
    SuiIdLeak,
    SuiEntryPoints,
    SuiGlobalStorage,
    SuiOneTimeWitness,
}

/// The canonical list of verifier paths we want to reach.
pub fn known_paths() -> Vec<KnownPath> {
    vec![
        KnownPath {
            id: "ref_safety_loop_join",
            description: "Reference safety: loop convergence with divergent borrow states \
                          (join_() called at back-edge with mismatched ref sets)",
            pass: VerifierPass::ReferenceSafety,
            error_code: Some(StatusCode::UNSAFE_RET_LOCAL_OR_RESOURCE_STILL_BORROWED),
            llm_hint: "Create a loop (back-edge) where one branch borrows a local via \
                       MutBorrowLoc and the other does not. The join at the loop header \
                       sees divergent borrow states, triggering the expensive join_() path. \
                       Pattern: LdTrue → BrFalse(end) → MutBorrowLoc(0) → Pop → Branch(loop_top)",
            source_context: "external-crates/move/crates/move-bytecode-verifier/src/reference_safety/abstract_state.rs",
        },
        KnownPath {
            id: "type_instantiation_loop",
            description: "Instantiation loop detection: generic call graph SCC with \
                          TyConApp edges (LOOP_IN_INSTANTIATION_GRAPH)",
            pass: VerifierPass::InstantiationLoops,
            error_code: Some(StatusCode::LOOP_IN_INSTANTIATION_GRAPH),
            llm_hint: "Create a generic function f<T>() that calls itself via CallGeneric \
                       with a concrete instantiation, forming a cycle in the instantiation \
                       graph. Or have f<T>() call g<vector<T>>() which calls f<T>().",
            source_context: "external-crates/move/crates/move-bytecode-verifier/src/instantiation_loops.rs",
        },
        KnownPath {
            id: "id_leak_through_conditional",
            description: "ID leak: Fresh→Other join at a branch merge precedes Pack of \
                          a key struct (id_leak_verifier)",
            pass: VerifierPass::SuiIdLeak,
            error_code: None,
            llm_hint: "Create an entry function that conditionally calls object::new() \
                       on one branch but not the other. At the merge point the UID state \
                       is Fresh on one path and Other on the other. Packing a key struct \
                       at the merge exercises the Fresh→Other join logic in id_leak_verifier.",
            source_context: "sui-execution/latest/sui-verifier/src/id_leak_verifier.rs",
        },
        KnownPath {
            id: "control_flow_reducibility",
            description: "Control flow: almost-irreducible CFG with multiple back-edges \
                          to the same loop header from different branches",
            pass: VerifierPass::CodeUnitVerifier,
            error_code: Some(StatusCode::INVALID_LOOP_SPLIT),
            llm_hint: "Create two Branch instructions that both target the same earlier \
                       offset, producing two separate back-edges to one block. \
                       Pattern: block0 → block1 → BrTrue(block0) → Branch(block0)",
            source_context: "external-crates/move/crates/move-bytecode-verifier/src/control_flow.rs",
        },
        KnownPath {
            id: "key_struct_uid_check",
            description: "Sui struct-with-key verifier: key struct whose first field \
                          is not 0x2::object::UID",
            pass: VerifierPass::SuiStructWithKey,
            error_code: None,
            llm_hint: "Define a struct with the 'key' ability where the first field \
                       is NOT of type 0x2::object::UID. This exercises the \
                       struct_with_key_verifier check for the ID field constraint.",
            source_context: "sui-execution/latest/sui-verifier/src/struct_with_key_verifier.rs",
        },
        KnownPath {
            id: "entry_non_droppable_return",
            description: "Entry points verifier: entry function returns a non-droppable value",
            pass: VerifierPass::SuiEntryPoints,
            error_code: None,
            llm_hint: "Define an entry function that returns a struct without the 'drop' \
                       ability. The entry_points_verifier rejects this because entry function \
                       return values must be droppable or transferable.",
            source_context: "sui-execution/latest/sui-verifier/src/entry_points_verifier.rs",
        },
        KnownPath {
            id: "signature_phantom_constraint",
            description: "Signature checker: phantom type parameter used in non-phantom position",
            pass: VerifierPass::SignatureChecker,
            error_code: Some(StatusCode::INVALID_PHANTOM_TYPE_PARAM_POSITION),
            llm_hint: "Declare a struct with a phantom type parameter T, then use T as \
                       a field type (non-phantom position). This triggers the phantom \
                       constraint check in the signature verifier.",
            source_context: "external-crates/move/crates/move-bytecode-verifier/src/signature.rs",
        },
        KnownPath {
            id: "stack_balance_across_branch",
            description: "Code unit verifier: stack depth imbalance across a conditional branch \
                          (different depths in then/else arms)",
            pass: VerifierPass::CodeUnitVerifier,
            error_code: Some(StatusCode::NEGATIVE_STACK_SIZE_WITHIN_BLOCK),
            llm_hint: "Create a BrFalse branch where the then-arm pushes two values \
                       but the else-arm pushes one. The merge point sees inconsistent \
                       stack depths, triggering NEGATIVE_STACK_SIZE_WITHIN_BLOCK or \
                       INVALID_FALLTHROUGH.",
            source_context: "external-crates/move/crates/move-bytecode-verifier/src/code_unit_verifier.rs",
        },
    ]
}

// ─── Coverage analysis ────────────────────────────────────────────────────────

/// Which verifier outcome each seed produced.
#[derive(Debug, Clone)]
pub struct SeedResult {
    pub path: PathBuf,
    /// Whether the seed deserializes to a valid CompiledModule.
    pub bounds_ok: bool,
    /// Error code produced by the Move bytecode verifier, if any.
    pub move_error: Option<StatusCode>,
    /// Error descriptions from individual Sui passes.
    pub sui_pass_errors: Vec<(&'static str, String)>,
}

/// Aggregated coverage report for a corpus directory.
#[derive(Debug, Default)]
pub struct CorpusCoverageReport {
    pub total_seeds: usize,
    pub bounds_passing: usize,
    pub move_passing: usize,
    pub sui_passing: usize,
    /// How many seeds triggered each StatusCode.
    pub error_code_counts: HashMap<String, usize>,
    /// Which Sui pass errors appeared at least once.
    pub sui_pass_hit: HashSet<String>,
}

/// Walk `corpus_dir`, deserialize each seed, run through verifier passes,
/// and return the aggregated coverage report.
pub fn analyze_corpus(corpus_dir: &Path) -> CorpusCoverageReport {
    let mut report = CorpusCoverageReport::default();
    let config = BinaryConfig::standard();
    let verifier_cfg = sui_harness::sui_verifier_config();

    let entries = match fs::read_dir(corpus_dir) {
        Ok(e) => e,
        Err(_) => return report,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            continue; // skip README files
        }
        let data = match fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        report.total_seeds += 1;

        let module = match CompiledModule::deserialize_with_config(&data, &config) {
            Ok(m) => {
                report.bounds_passing += 1;
                m
            }
            Err(_) => continue,
        };

        let mut meter = DummyMeter;
        match verify_module_with_config_metered(&verifier_cfg, &module, &mut meter) {
            Ok(()) => {
                report.move_passing += 1;
                // Run Sui passes.
                if sui_harness::run_full_verification(&module).is_ok() {
                    report.sui_passing += 1;
                } else {
                    let sui_results = sui_harness::run_sui_passes_individually(&module);
                    for (pass, result) in sui_results {
                        if let Err(e) = result {
                            let key = format!("{}:{}", pass, &e[..e.len().min(40)]);
                            report.sui_pass_hit.insert(key);
                            *report
                                .error_code_counts
                                .entry(format!("sui::{pass}"))
                                .or_insert(0) += 1;
                        }
                    }
                }
            }
            Err(e) => {
                let code = e.major_status();
                *report
                    .error_code_counts
                    .entry(format!("{:?}", code))
                    .or_insert(0) += 1;
            }
        }
    }

    report
}

/// Identify which known verifier paths have zero corpus coverage.
pub fn identify_gaps(report: &CorpusCoverageReport) -> Vec<CoverageGap> {
    let mut gaps = Vec::new();

    for path in known_paths() {
        let covered = match &path.error_code {
            Some(code) => report
                .error_code_counts
                .contains_key(&format!("{:?}", code)),
            None => {
                // For Sui passes without a specific error code, check if the
                // pass name appears in sui_pass_hit.
                let pass_key = match path.pass {
                    VerifierPass::SuiStructWithKey => "struct_with_key_verifier",
                    VerifierPass::SuiIdLeak => "id_leak_verifier",
                    VerifierPass::SuiEntryPoints => "entry_points_verifier",
                    VerifierPass::SuiGlobalStorage => "global_storage_access_verifier",
                    VerifierPass::SuiOneTimeWitness => "one_time_witness_verifier",
                    _ => "",
                };
                !pass_key.is_empty()
                    && report
                        .sui_pass_hit
                        .iter()
                        .any(|k| k.starts_with(pass_key))
            }
        };

        if !covered {
            gaps.push(CoverageGap {
                path_id: path.id,
                description: path.description,
                pass: path.pass,
                error_code: path.error_code,
                llm_hint: path.llm_hint,
                source_context_file: path.source_context,
            });
        }
    }

    gaps
}

/// A verifier path that the current corpus does not exercise.
#[derive(Debug, Clone)]
pub struct CoverageGap {
    pub path_id: &'static str,
    pub description: &'static str,
    pub pass: VerifierPass,
    pub error_code: Option<StatusCode>,
    pub llm_hint: &'static str,
    pub source_context_file: &'static str,
}

impl CoverageGap {
    /// One-line summary for display.
    pub fn summary(&self) -> String {
        let code = self
            .error_code
            .map(|c| format!(" [{:?}]", c))
            .unwrap_or_default();
        format!("[{}]{} — {}", self.path_id, code, self.description)
    }
}
