// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Seed corpus generator for the Sui Move VM fuzzer.
//!
//! Generates a set of valid `CompiledModule` bytes that pass verification,
//! suitable as starting points for mutation-based fuzzing.

use arbitrary::Unstructured;

use crate::module_gen::{ModuleBuilder, ModuleGenConfig};

/// Generate seed corpus entries as serialized module bytes.
/// Each entry uses a different random seed to produce a diverse set of modules.
pub fn generate_seeds(count: usize) -> Vec<Vec<u8>> {
    let mut seeds = Vec::new();

    for seed_idx in 0..count {
        // Create deterministic but varied random data for each seed
        let data: Vec<u8> = (0..2048)
            .map(|i| {
                ((i as u32)
                    .wrapping_mul(seed_idx as u32 + 1)
                    .wrapping_add(seed_idx as u32 * 37)
                    % 256) as u8
            })
            .collect();

        let mut u = Unstructured::new(&data);
        let config: ModuleGenConfig = match u.arbitrary() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let builder = ModuleBuilder::new(config);
        let module = match builder.build(&mut u) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let mut bytes = Vec::new();
        if module.serialize(&mut bytes).is_ok() {
            seeds.push(bytes);
        }
    }

    seeds
}

/// Generate seeds wrapped in the `Input` struct format used by structured fuzz targets.
/// Returns raw bytes suitable for the `verifier_crash` and `e2e_publish_execute` targets.
pub fn generate_structured_seeds(count: usize) -> Vec<Vec<u8>> {
    let mut seeds = Vec::new();

    for seed_idx in 0..count {
        // For structured targets, we need bytes that Arbitrary can parse into
        // our Input structs. The simplest approach: create a ModuleGenConfig
        // manually and serialize it with enough entropy for building.
        let mut data = Vec::with_capacity(1024);

        // ModuleGenConfig fields (6 bytes for Arbitrary):
        let num_structs = ((seed_idx % 5) + 1) as u8;
        let num_functions = ((seed_idx % 4) + 1) as u8;
        let num_fields = (seed_idx % 4) as u8;
        let max_code_len = ((seed_idx % 30) + 8) as u8;
        let has_key = if seed_idx % 3 == 0 { 1u8 } else { 0 };
        let has_entry = if seed_idx % 2 == 0 { 1u8 } else { 0 };

        data.push(num_structs);
        data.push(num_functions);
        data.push(num_fields);
        data.push(max_code_len);
        data.push(has_key);
        data.push(has_entry);

        // Option<MutationKind>: 0 = None (no mutation for seed corpus)
        data.push(0);

        // Vec<u8> raw_entropy: fill with deterministic data for module building
        let entropy: Vec<u8> = (0..512)
            .map(|i| {
                ((i as u32)
                    .wrapping_mul(seed_idx as u32 + 7)
                    .wrapping_add(13)
                    % 256) as u8
            })
            .collect();
        // Arbitrary for Vec<u8> reads a length prefix (ULEB128) then that many bytes
        // For simplicity, encode a small length that fits in two ULEB128 bytes
        let entropy_len = entropy.len().min(400);
        // Encode length as 2-byte ULEB128: low 7 bits + continuation, then high bits
        data.push((entropy_len as u8 & 0x7F) | 0x80);
        data.push((entropy_len >> 7) as u8);
        data.extend_from_slice(&entropy[..entropy_len]);

        seeds.push(data);
    }

    seeds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_raw_seeds() {
        let seeds = generate_seeds(20);
        assert!(!seeds.is_empty(), "should generate at least one seed");

        for (i, seed) in seeds.iter().enumerate() {
            // Each seed should start with Move magic bytes
            assert!(
                seed.len() > 4,
                "seed {i} too short: {} bytes",
                seed.len()
            );
            assert_eq!(
                &seed[..4],
                &[0xA1, 0x1C, 0xEB, 0x0B],
                "seed {i} missing Move magic"
            );
        }
    }

    #[test]
    fn generates_structured_seeds() {
        let seeds = generate_structured_seeds(10);
        assert_eq!(seeds.len(), 10);
        for seed in &seeds {
            assert!(seed.len() > 10, "structured seed too short");
        }
    }
}
