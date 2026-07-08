// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Utilities for working with chain identifier strings.
//!
//! A chain identifier can appear in two formats:
//! - the canonical form: the full genesis checkpoint digest, Base58-encoded (the format returned
//!   by the gRPC and GraphQL APIs), e.g. `4btiuiMPvEENsttpZC7CZ53DruC3MAgfznDbASZ7DR6S`;
//! - the legacy short form: the first 4 bytes of the digest, hex-encoded (the format returned by
//!   JSON-RPC and written by older tooling into `Move.toml` and `client.yaml`), e.g. `35834a8a`.
//!
//! New identifiers are written in the canonical form, but manifests and caches produced by older
//! tooling still contain short forms, so comparisons must treat the two encodings of the same
//! digest as equal.

use std::str::FromStr;

use sui_sdk::types::digests::{ChainIdentifier, CheckpointDigest};

/// The full Base58-encoded genesis checkpoint digest for `chain_id` — the canonical chain
/// identifier format. (`ChainIdentifier`'s `Display` renders the legacy 4-byte hex short form.)
pub fn chain_id_base58(chain_id: &ChainIdentifier) -> String {
    CheckpointDigest::new(*chain_id.as_bytes()).base58_encode()
}

/// Compare two chain identifier strings, each of which may independently be in the canonical
/// Base58 form or the legacy hex short form. A short form matches a full form if it encodes the
/// first 4 bytes of the digest. Strings in neither format (e.g. ad-hoc identifiers used by tests
/// or other flavors) only match by exact string equality.
pub fn chain_ids_match(a: &str, b: &str) -> bool {
    match (decode_chain_id(a), decode_chain_id(b)) {
        (Some(a), Some(b)) => {
            let len = a.len().min(b.len());
            a[..len] == b[..len]
        }
        _ => a == b,
    }
}

/// Decode a chain identifier string into digest bytes: all 32 bytes for the canonical Base58
/// form, the first 4 for the legacy hex short form; `None` if the string is in neither format.
fn decode_chain_id(s: &str) -> Option<Vec<u8>> {
    if let Ok(digest) = CheckpointDigest::from_str(s) {
        return Some(digest.inner().to_vec());
    }

    let hex = s.strip_prefix("0x").unwrap_or(s);
    if hex.len() == 8 {
        let bytes: Option<Vec<u8>> = (0..8)
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
            .collect();
        return bytes;
    }

    None
}

#[cfg(test)]
mod tests {
    use sui_sdk::types::digests::{
        MAINNET_CHAIN_IDENTIFIER_BASE58, TESTNET_CHAIN_IDENTIFIER_BASE58,
        get_mainnet_chain_identifier, get_testnet_chain_identifier,
    };

    use super::*;

    const MAINNET_SHORT: &str = "35834a8a";
    const TESTNET_SHORT: &str = "4c78adac";

    #[test]
    fn base58_form_of_known_chains() {
        assert_eq!(
            chain_id_base58(&get_mainnet_chain_identifier()),
            MAINNET_CHAIN_IDENTIFIER_BASE58
        );
        assert_eq!(
            chain_id_base58(&get_testnet_chain_identifier()),
            TESTNET_CHAIN_IDENTIFIER_BASE58
        );
    }

    #[test]
    fn full_matches_full() {
        assert!(chain_ids_match(
            MAINNET_CHAIN_IDENTIFIER_BASE58,
            MAINNET_CHAIN_IDENTIFIER_BASE58
        ));
        assert!(!chain_ids_match(
            MAINNET_CHAIN_IDENTIFIER_BASE58,
            TESTNET_CHAIN_IDENTIFIER_BASE58
        ));
    }

    #[test]
    fn short_matches_full_in_either_order() {
        assert!(chain_ids_match(
            MAINNET_SHORT,
            MAINNET_CHAIN_IDENTIFIER_BASE58
        ));
        assert!(chain_ids_match(
            MAINNET_CHAIN_IDENTIFIER_BASE58,
            MAINNET_SHORT
        ));
        assert!(!chain_ids_match(
            TESTNET_SHORT,
            MAINNET_CHAIN_IDENTIFIER_BASE58
        ));
    }

    #[test]
    fn short_matches_short() {
        assert!(chain_ids_match(MAINNET_SHORT, MAINNET_SHORT));
        assert!(chain_ids_match(MAINNET_SHORT, "35834A8A"));
        assert!(chain_ids_match("0x35834a8a", MAINNET_SHORT));
        assert!(!chain_ids_match(MAINNET_SHORT, TESTNET_SHORT));
    }

    #[test]
    fn opaque_ids_match_exactly() {
        assert!(chain_ids_match("localnet", "localnet"));
        assert!(!chain_ids_match("localnet", "Localnet"));
        assert!(!chain_ids_match(
            "localnet",
            MAINNET_CHAIN_IDENTIFIER_BASE58
        ));
    }
}
