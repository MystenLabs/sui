// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sui Move VM Fuzzer
//!
//! A structure-aware fuzzer targeting the full Sui Move VM pipeline:
//! deserialization → verification (Move + Sui) → publish → execute.
//!
//! Bug classes detected:
//! - Validator crashes (panics in verification or execution)
//! - Fund loss (SUI conservation violations)
//! - Verifier soundness (bytecode passing verification but violating safety at runtime)

#[cfg(feature = "e2e")]
pub mod authority_harness;
pub mod coverage_gaps;
#[cfg(feature = "fork")]
pub mod forked_executor;
#[cfg(feature = "fork")]
pub mod forked_store;
#[cfg(feature = "fork")]
pub mod oracle_override;
#[cfg(feature = "llm-guided")]
pub mod llm_client;
pub mod crash_validator;
pub mod custom_mutator;
pub mod llm_prompts;
pub mod module_gen;
pub mod module_spec;
pub mod mutators;
pub mod oracle;
pub mod seed_corpus;
pub mod sui_harness;
