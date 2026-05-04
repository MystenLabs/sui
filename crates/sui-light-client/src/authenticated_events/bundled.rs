// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Trusted committees bundled with the crate so that a fresh client can skip
//! ratcheting through epochs that pre-date authenticated events being
//! available on chain. The bundled committee is the trust anchor — depending
//! on this crate is what binds the user to it.

use sui_types::committee::Committee;

/// First testnet epoch where the `enable_authenticated_event_streams` flag
/// went live (protocol version 114). Authenticated events cannot exist on
/// any earlier epoch, so a client only consuming them can ratchet from here.
pub const TESTNET_START_EPOCH: u64 = 1029;

const TESTNET_START_COMMITTEE_BCS: &[u8] = include_bytes!("bundled/testnet_start_committee.bcs");

/// Bundled committee for [`TESTNET_START_EPOCH`].
pub fn testnet_start_committee() -> Committee {
    let committee: Committee = bcs::from_bytes(TESTNET_START_COMMITTEE_BCS)
        .expect("bundled testnet start committee is corrupted");
    debug_assert_eq!(committee.epoch(), TESTNET_START_EPOCH);
    committee
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn testnet_bundle_has_expected_epoch() {
        assert_eq!(testnet_start_committee().epoch(), TESTNET_START_EPOCH);
    }
}
