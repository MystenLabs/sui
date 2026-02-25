// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared structure-aware mutation logic for libFuzzer's custom mutator hook.
//!
//! Wires the existing targeted mutations from `mutators.rs` into libFuzzer's
//! `fuzz_mutator!` macro, giving coverage-guided feedback on structurally
//! meaningful mutations instead of random byte flips.

use arbitrary::Unstructured;
use move_binary_format::binary_config::BinaryConfig;
use move_binary_format::file_format::CompiledModule;
use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};

use crate::mutators::{apply_mutation, ensure_code_unit};

/// Structure-aware mutation function for use with `fuzz_mutator!`.
///
/// Returns `Some(new_size)` on success, or `None` to signal the caller
/// should fall back to `libfuzzer_sys::fuzzer_mutate()`.
pub fn mutate_move_module(
    data: &mut [u8],
    size: usize,
    max_size: usize,
    seed: u32,
) -> Option<usize> {
    // 20% of the time, fall back to libFuzzer's default byte mutations
    // for exploration diversity.
    if seed.is_multiple_of(5) {
        return None;
    }

    let config = BinaryConfig::standard();
    let mut module = match CompiledModule::deserialize_with_config(&data[..size], &config) {
        Ok(m) => m,
        Err(_) => return None,
    };

    let mut rng = SmallRng::seed_from_u64(seed as u64);
    let mut entropy = [0u8; 64];
    rng.fill_bytes(&mut entropy);
    let mut u = Unstructured::new(&entropy);

    ensure_code_unit(&mut module);

    let num_mutations = (seed as usize % 3) + 1;
    for _ in 0..num_mutations {
        if apply_mutation(&mut u, &mut module).is_err() {
            break;
        }
    }

    let mut out = Vec::new();
    if module.serialize(&mut out).is_err() {
        return None;
    }

    if out.len() > max_size {
        return None;
    }

    data[..out.len()].copy_from_slice(&out);
    Some(out.len())
}
