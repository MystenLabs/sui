// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Oracle price manipulation API for forked-execution mode.
//!
//! This module provides utilities for fetching real oracle price objects
//! (Pyth, Switchboard, etc.) from the network and injecting modified versions
//! into a `ForkedStore` so that adversarial modules see manipulated prices.
//!
//! # BCS byte patching
//!
//! Oracle price data is embedded as BCS-serialized Move struct fields inside
//! the object's `contents` bytes.  Protocol-specific helpers decode and re-encode
//! just the price fields while preserving the rest of the object layout.
//!
//! Initial implementation: raw byte-level patching via `BytePatch` (offset +
//! replacement bytes).  Protocol-specific helpers (Pyth struct layout, dynamic
//! field children) can be layered on top incrementally.

use anyhow::{bail, Result};
use sui_types::base_types::ObjectID;
use sui_types::object::{Data, Object};

use crate::forked_store::ForkedStore;

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// Specification for a price override to apply to an oracle object.
pub struct PriceOverride {
    /// On-chain object ID of the oracle price feed object.
    pub oracle_object_id: ObjectID,
    /// New price value (signed integer, same units as the oracle's exponent).
    pub price: i64,
    /// Optional 95% confidence interval around `price`.
    pub confidence: Option<u64>,
    /// Optional exponent — number of decimal places (e.g. -8 means × 10⁻⁸).
    pub exponent: Option<i32>,
}

/// A raw byte patch: replace `len` bytes at `offset` in the object's BCS
/// contents with `replacement`.
pub struct BytePatch {
    pub offset: usize,
    pub replacement: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch the oracle object identified by `spec.oracle_object_id` from the RPC,
/// apply `patches` to its BCS bytes, and inject the modified object into
/// `store`'s override layer.
///
/// After this call, any Move code that reads the oracle object will see the
/// patched bytes.
///
/// # Errors
/// Returns an error if:
/// - The object cannot be fetched (not found, RPC failure).
/// - The object is a package (not a Move object).
/// - A patch extends beyond the object's content length.
pub fn apply_price_override(
    store: &mut ForkedStore,
    spec: &PriceOverride,
    patches: &[BytePatch],
) -> Result<()> {
    let obj = store
        .fetch_object(&spec.oracle_object_id)?
        .ok_or_else(|| anyhow::anyhow!("oracle object {} not found", spec.oracle_object_id))?;

    let patched = patch_object_contents(obj, patches)?;
    store.inject_object(patched);
    Ok(())
}

/// Like `apply_price_override` but also applies the same patches to the
/// dynamic-field child object identified by `child_id`.
///
/// Pyth price feeds store the actual price data in a child `PriceInfoObject`
/// that is a dynamic field of the parent feed.  This variant patches both.
pub fn apply_price_override_with_child(
    store: &mut ForkedStore,
    spec: &PriceOverride,
    parent_patches: &[BytePatch],
    child_id: ObjectID,
    child_patches: &[BytePatch],
) -> Result<()> {
    // Patch parent
    let parent_obj = store
        .fetch_object(&spec.oracle_object_id)?
        .ok_or_else(|| anyhow::anyhow!("parent oracle object {} not found", spec.oracle_object_id))?;
    let patched_parent = patch_object_contents(parent_obj, parent_patches)?;
    store.inject_object(patched_parent);

    // Patch child
    let child_obj = store
        .fetch_object(&child_id)?
        .ok_or_else(|| anyhow::anyhow!("child oracle object {} not found", child_id))?;
    let patched_child = patch_object_contents(child_obj, child_patches)?;
    store.inject_object(patched_child);

    Ok(())
}

/// Parse a `"<object_id>:<price_i64>"` string into a `(ObjectID, i64)` pair.
/// Used for command-line `--oracle-override` flag parsing.
///
/// Example: `"0x1234abcd...:500000000"` → `(ObjectID, 500_000_000i64)`
pub fn parse_override_spec(s: &str) -> Result<(ObjectID, i64)> {
    let (id_part, price_part) = s
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("expected '<object_id>:<price>', got: {s}"))?;

    let id: ObjectID = id_part
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid ObjectID '{id_part}': {e}"))?;
    let price: i64 = price_part
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid price '{price_part}': {e}"))?;

    Ok((id, price))
}

/// Build `BytePatch` entries from a `PriceOverride` spec, targeting the most
/// common BCS field layout for oracle price objects.
///
/// Patches are generated at i64/u64/i32-aligned offsets:
/// - offset 0: `price` (i64, 8 bytes)
/// - offset 8: `confidence` (u64, 8 bytes), if `spec.confidence` is set
/// - offset 16: `exponent` (i32, 4 bytes), if `spec.exponent` is set
///
/// The patch list is validated against the actual object size in
/// `apply_price_override` via `patch_object_contents`.
pub fn build_price_patches(spec: &PriceOverride) -> Vec<BytePatch> {
    let mut patches = Vec::new();
    patches.push(BytePatch {
        offset: 0,
        replacement: spec.price.to_le_bytes().to_vec(),
    });
    if let Some(conf) = spec.confidence {
        patches.push(BytePatch {
            offset: 8,
            replacement: conf.to_le_bytes().to_vec(),
        });
    }
    if let Some(exp) = spec.exponent {
        patches.push(BytePatch {
            offset: 16,
            replacement: exp.to_le_bytes().to_vec(),
        });
    }
    patches
}

/// Scan `contents` and return patches that overwrite every 8-byte aligned
/// position with `price` in little-endian format.
///
/// Useful when the oracle struct layout is unknown — this maximises the chance
/// that at least one field actually holds the price.
pub fn build_simple_price_patch(price: i64, contents: &[u8]) -> Vec<BytePatch> {
    let price_bytes = price.to_le_bytes().to_vec();
    let max_offset = contents.len().saturating_sub(8);
    (0..=max_offset)
        .step_by(8)
        .map(|offset| BytePatch {
            offset,
            replacement: price_bytes.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Clone `obj`, apply `patches` to its Move object BCS contents, and return
/// the modified object.
fn patch_object_contents(mut obj: Object, patches: &[BytePatch]) -> Result<Object> {
    let move_obj = match &mut obj.data {
        Data::Move(m) => m,
        Data::Package(_) => bail!("expected a Move object, got a package"),
    };

    // Clone the current contents so we can validate patches before applying.
    let mut contents = move_obj.contents().to_vec();

    for patch in patches {
        let end = patch
            .offset
            .checked_add(patch.replacement.len())
            .ok_or_else(|| anyhow::anyhow!("patch offset overflow"))?;
        if end > contents.len() {
            bail!(
                "patch at offset {} (len {}) extends beyond object contents (len {})",
                patch.offset,
                patch.replacement.len(),
                contents.len()
            );
        }
        contents[patch.offset..end].copy_from_slice(&patch.replacement);
    }

    // Intentionally injecting adversarial bytes to test how downstream
    // protocols react to manipulated oracle data.
    move_obj.set_contents_unsafe(contents);

    Ok(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::SuiAddress;

    /// Verify that `BytePatch` is applied correctly and that out-of-bounds
    /// patches are rejected.
    #[test]
    fn byte_patch_applied() {
        let obj = Object::new_gas_with_balance_and_owner_for_testing(0, SuiAddress::ZERO);
        let original_len = match &obj.data {
            Data::Move(m) => m.contents().len(),
            _ => panic!("expected Move object"),
        };

        // Single byte patch at offset 0
        let patches = vec![BytePatch {
            offset: 0,
            replacement: vec![0xFF],
        }];
        let patched = patch_object_contents(obj.clone(), &patches).unwrap();
        match &patched.data {
            Data::Move(m) => assert_eq!(m.contents()[0], 0xFF),
            _ => panic!("expected Move object"),
        }

        // Out-of-bounds patch should fail
        let bad_patches = vec![BytePatch {
            offset: original_len,
            replacement: vec![0x00],
        }];
        assert!(patch_object_contents(obj, &bad_patches).is_err());
    }

    #[test]
    fn parse_override_spec_roundtrip() {
        // A real-looking object ID + price
        let s = "0x0000000000000000000000000000000000000000000000000000000000000006:1234567";
        let (id, price) = parse_override_spec(s).unwrap();
        assert_eq!(price, 1_234_567i64);
        // id should equal the SUI clock object ID
        assert_eq!(id, "0x6".parse::<ObjectID>().unwrap());
    }

    #[test]
    fn parse_override_spec_rejects_bad_input() {
        assert!(parse_override_spec("no_colon").is_err());
        assert!(parse_override_spec("0x1:not_a_number").is_err());
        assert!(parse_override_spec("not_an_id:42").is_err());
    }
}
